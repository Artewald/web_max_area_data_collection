use chrono::Local;
use log::info;
#[cfg(target_arch = "wasm32")]
use wgpu::SurfaceTarget;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use web_sys::{window, HtmlElement};

use crate::{gather_data::InformationGathered, metrics::get_triangulation_statistics, vertex::{TriangulationType, Vertex}};

const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const FRAME_WIDTH: u32 = 1920;
const FRAME_HEIGHT: u32 = 1920;
const WARMUP_MS: i64 = 5_000;
const NUM_FRAMES_TO_CAPTURE: usize = 500;

pub async fn collect_data() -> anyhow::Result<()> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        #[cfg(not(target_arch = "wasm32"))]
        backends: wgpu::Backends::VULKAN,
        #[cfg(target_arch = "wasm32")]
        backends: wgpu::Backends::BROWSER_WEBGPU,
        flags: Default::default(),
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
    });
    info!("Instance found!");

    let size = PhysicalSize::new(FRAME_WIDTH, FRAME_HEIGHT);

    instance.enumerate_adapters(wgpu::Backends::all()).await.iter().for_each(|a|
        {
            let i = a.get_info();
            info!("Available adapter: {} | backend: {}", i.name, i.backend);
        }
    );

    let adapter_res = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            #[cfg(not(target_arch = "wasm32"))]
            compatible_surface: None,
            #[cfg(target_arch = "wasm32")]
            compatible_surface: None, /*Some(&{
                use wasm_bindgen::JsCast;

                const CANVAS_ID: &str = "canvas";

                let window = wgpu::web_sys::window().unwrap_throw();
                let document = window.document().unwrap_throw();
                let canvas = document.get_element_by_id(CANVAS_ID).unwrap_throw();
                let html_canvas_elem = canvas.unchecked_into::<web_sys::HtmlCanvasElement>();
                let target_surface = SurfaceTarget::Canvas(html_canvas_elem);
                instance.create_surface(target_surface).unwrap()
            }),*/
            force_fallback_adapter: false,
        })
        .await;
    #[cfg(target_arch = "wasm32")]
    let adapter = adapter_res.unwrap_throw();
    #[cfg(not(target_arch = "wasm32"))]
    let adapter = adapter_res?;


    let adapter_info = adapter.get_info();
    info!("Using device {:?}", adapter_info.name);
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Requesting device"),
            #[cfg(target_arch = "wasm32")]
            required_features: wgpu::Features::empty(), //::from_name("POLYGON_MODE_LINE").unwrap(),
            #[cfg(not(target_arch = "wasm32"))]
            required_features: wgpu::Features::from_name("POLYGON_MODE_LINE").unwrap(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            required_limits: if cfg!(target_arch = "wasm32") {
                wgpu::Limits::downlevel_webgl2_defaults()
            } else {
                wgpu::Limits::default()
            },
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
        })
        .await?;

    let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Render shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });
    info!("Shader created!");

    let render_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[],
            ..Default::default()
        });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &render_shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            }],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None, //Some(wgpu::Face::Back),
            // Requires Features::DEPTH_CLIP_CONTROL
            unclipped_depth: false,
            #[cfg(target_arch = "wasm32")]
            polygon_mode: wgpu::PolygonMode::Fill,
            #[cfg(not(target_arch = "wasm32"))]
            polygon_mode: wgpu::PolygonMode::Fill,
            // Requires Features::CONSERVATIVE_RASTERIZATION
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &render_shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: TEXTURE_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        cache: None,
        multiview_mask: None,
    });
    info!("Render pipeline created!");

    let extent = wgpu::Extent3d {
        width: size.width,
        height: size.height,
        depth_or_array_layers: 1,
    };

    let render_texture = device.create_texture(&wgpu::wgt::TextureDescriptor {
        label: Some("Texture to render to!"),
        size: extent.clone(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let render_texture_view = render_texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(TEXTURE_FORMAT),
        ..Default::default()
    });
    info!("Render texture created!");

    #[cfg(target_arch = "wasm32")]
    let bin_data = include_bytes!("fan_stripe_max_area.bin");
    #[cfg(not(target_arch = "wasm32"))]
    let bin_data = &std::fs::read("./src/fan_stripe_max_area.bin").unwrap();
    let mut data: Vec<(TriangulationType, Vec<Vertex>, Vec<u32>)> =
        postcard::from_bytes(bin_data).unwrap();
    #[cfg(target_arch = "wasm32")]
    let bin_data = include_bytes!("random_triangulations_262_144.bin");
    #[cfg(not(target_arch = "wasm32"))]
    let bin_data = &std::fs::read("./src/random_triangulations_262_144.bin").unwrap();
    let mut data_2: Vec<(TriangulationType, Vec<Vertex>, Vec<u32>)> =
        postcard::from_bytes(bin_data).unwrap();
    data.append(&mut data_2);
    let triangulations = data;
    info!("Triangulations loaded!");

    let mut triangulation_stats = Vec::new();
    #[cfg(target_arch = "wasm32")]
    let window = window().unwrap_throw();
    #[cfg(target_arch = "wasm32")]
    let document = window.document().unwrap_throw();
    #[cfg(target_arch = "wasm32")]
    let element = document.get_element_by_id("info").unwrap_throw().dyn_into::<HtmlElement>().unwrap_throw();

    let num_triangulations = triangulations.len();
    for (iter, (typ, vertices, mut indices)) in triangulations.into_iter().enumerate() {
        info!("Current triangulation: {iter} of {num_triangulations} | {typ:?} | Num vertices {}", vertices.len());
        #[cfg(target_arch = "wasm32")]
        element.set_inner_text(&format!("Current triangulation: {iter} of {num_triangulations} | {typ:?} | Num vertices {}", vertices.len()));

        indices.chunks_exact_mut(3).for_each(|c| c.reverse());

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        info!("Done creating vertex and index buffer!");

        // === WARMUP ===
        let warmup_start = Local::now();
        while (Local::now()-warmup_start).num_milliseconds() < WARMUP_MS {
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Forward pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &render_texture_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                renderpass.set_pipeline(&render_pipeline);
                renderpass.set_vertex_buffer(0, vertex_buffer.slice(..));
                renderpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                renderpass.draw_indexed(0..(u32::try_from(indices.len()).unwrap()), 0, 0..1);
            }

            let (send, recv) = futures::channel::oneshot::channel();
            encoder.on_submitted_work_done(move || {
                let _ = send.send(true);
            });

            queue.submit([encoder.finish()]);

            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            // #[cfg(not(target_arch = "wasm32"))]
            // {
            //     device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            // }

            let _ = recv.await;
        }
        info!("Done warming up!");

        // === COLLECT DATA ===
        let mut render_data: Vec<f64> = Vec::new();
        while render_data.len() < NUM_FRAMES_TO_CAPTURE {
            info!("Currently collected {} frames | Rendering new frame!", render_data.len());
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Forward pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &render_texture_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                renderpass.set_pipeline(&render_pipeline);
                renderpass.set_vertex_buffer(0, vertex_buffer.slice(..));
                renderpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                renderpass.draw_indexed(0..(u32::try_from(indices.len()).unwrap()), 0, 0..1);
            }

            let time_before_encoder = Local::now();
            let (send, recv) = futures::channel::oneshot::channel();
            encoder.on_submitted_work_done(move || {
                let time_passed = Local::now() - time_before_encoder;
                let millis = time_passed.num_microseconds().unwrap_or(i64::MAX).abs() as f64 / 1000.0;
                // info!("Sending data!");
                let _ = send.send(millis);
                // info!("Finished sending data!");
            });

            // info!("Submitting queue!");
            queue.submit([encoder.finish()]);

            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            // #[cfg(not(target_arch = "wasm32"))]
            // {
            //     device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            // }

            if let Ok(data) = recv.await {
                render_data.push(data);
            }
        }
        info!("Done gathering data!");

        let current_triangulation_stats = get_triangulation_statistics(&vertices, &indices);
        let data = render_data;
        let data = &data[0..NUM_FRAMES_TO_CAPTURE];
        let elapsed_ms = data.iter().fold(0.0, |a, b| a+b).max(1e-7) as u64;
        let frame_counter = data.len();
        let info_gathered = InformationGathered {
            triangulation_type: typ,
            num_vertices: vertices.len(),
            num_frames: frame_counter,
            total_time_ms: elapsed_ms,
            frame_width: FRAME_WIDTH,
            frame_height: FRAME_HEIGHT,
            metrics: current_triangulation_stats,
        };
        info!("Num frames collected: {} | Time taken to render frames {} ms.", info_gathered.num_frames, info_gathered.total_time_ms);
        triangulation_stats.push(info_gathered);
    }

    let mut wtr = csv::Writer::from_writer(vec![]);
    triangulation_stats.iter().for_each(|d| {
        wtr.serialize(d).unwrap();
    });
    let csv_bytes = &wtr.into_inner().unwrap();
    let csv_data = String::from_utf8_lossy(csv_bytes);
    let size = (FRAME_WIDTH, FRAME_HEIGHT);
    let gpu_name = adapter_info.name;

    let mut data = String::new();
    data += &format!("Name: {},\nResolution: {}x{},\n\n", gpu_name, size.0, size.1);
    data += &csv_data;
    let data = data.into_bytes();
    #[cfg(not(target_arch = "wasm32"))]
    {
        rfd::FileDialog::new().save_file().and_then(|p| std::fs::write(p, data).ok()).unwrap();
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm_bindgen_futures::spawn_local(
            async move {
                use rfd::AsyncFileDialog;

                let file_handle = AsyncFileDialog::new().set_file_name("rendering_metrics.csv").save_file().await.unwrap_throw();
                file_handle.write(&data).await.unwrap_throw();
            }
        )
    }

    Ok(())
}
