use chrono::Local;
use log::info;
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
const NUM_FRAMES_TO_CAPTURE: usize = 100;

pub async fn collect_data() -> anyhow::Result<()> {
    let adapter_info_gl_name = {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::GL,
            flags: Default::default(),
            memory_budget_thresholds: Default::default(),
            backend_options: Default::default(),
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                #[cfg(not(target_arch = "wasm32"))]
                compatible_surface: None,
                #[cfg(target_arch = "wasm32")]
                compatible_surface: Some(&{
                    use wasm_bindgen::JsCast;
                    use wgpu::SurfaceTarget;

                    const CANVAS_ID: &str = "canvas";

                    let window = wgpu::web_sys::window().unwrap_throw();
                    let document = window.document().unwrap_throw();
                    let canvas = document.get_element_by_id(CANVAS_ID).unwrap_throw();
                    let html_canvas_elem = canvas.unchecked_into::<web_sys::HtmlCanvasElement>();
                    let target_surface = SurfaceTarget::Canvas(html_canvas_elem);
                    instance.create_surface(target_surface).unwrap()
                }),
                force_fallback_adapter: false,
            })
            .await?;

        adapter.get_info().name
    };
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        #[cfg(not(target_arch = "wasm32"))]
        backends: wgpu::Backends::PRIMARY,
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
            info!("{:?}", i);
            // info!("Available adapter: {} | backend: {}", i.name, i.backend);
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
    let gpu_info = if adapter_info.name.is_empty() {
        format!("Using fallback info (from GL backend): {}. This might not be the same GPU as WebGPU has selected!", adapter_info_gl_name)
        // format!("{:?}", adapter_info)
        // format!("Vendor ID: {:#06x} | Device ID: {:#06x}.", adapter_info.vendor, adapter_info.device)
        // String::from("Unable to get GPU information. Please write down the GPU name (row GL_RENDERER in the table) when you search for 'chrome://gpu' on any chrome based browser.")
    } else {
        adapter_info.name.clone()
    };
    drop(adapter_info);
    info!("Device info - {:?}", gpu_info);
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Requesting device"),
            #[cfg(target_arch = "wasm32")]
            required_features: wgpu::Features::TIMESTAMP_QUERY,//wgpu::Features::empty(), //::from_name("POLYGON_MODE_LINE").unwrap(),
            #[cfg(not(target_arch = "wasm32"))]
            required_features: wgpu::Features::from_name("POLYGON_MODE_LINE").unwrap() | wgpu::Features::TIMESTAMP_QUERY,
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

    let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("Timestamp Query Set"),
        ty: wgpu::QueryType::Timestamp,
        count: 2,
    });

    let query_gpu_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Query GPU Buffer"),
        size: 16,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let query_cpu_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Query CPU Buffer"),
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    #[cfg(target_arch = "wasm32")]
    let bin_data = include_bytes!("fan_stripe_max_area.bin");
    #[cfg(not(target_arch = "wasm32"))]
    let bin_data = &std::fs::read("./src/fan_stripe_max_area.bin").unwrap();
    let mut data: Vec<(TriangulationType, Vec<Vertex>, Vec<u32>)> =
        postcard::from_bytes(bin_data).unwrap();
    // #[cfg(target_arch = "wasm32")]
    // let bin_data = include_bytes!("random_triangulations_65_536.bin");
    // #[cfg(not(target_arch = "wasm32"))]
    // let bin_data = &std::fs::read("./src/random_triangulations_65_536.bin").unwrap();
    // let mut data: Vec<(TriangulationType, Vec<Vertex>, Vec<u32>)> =
    //     postcard::from_bytes(bin_data).unwrap();
    // data.append(&mut data_2);
    let triangulations = data;
    info!("Triangulations loaded!");

    let mut triangulation_stats = Vec::new();
    #[cfg(target_arch = "wasm32")]
    let window = window().unwrap_throw();
    #[cfg(target_arch = "wasm32")]
    let document = window.document().unwrap_throw();
    #[cfg(target_arch = "wasm32")]
    let info1 = document.get_element_by_id("info1").unwrap_throw().dyn_into::<HtmlElement>().unwrap_throw();
    #[cfg(target_arch = "wasm32")]
    let info2 = document.get_element_by_id("info2").unwrap_throw().dyn_into::<HtmlElement>().unwrap_throw();
    #[cfg(target_arch = "wasm32")]
    let info3 = document.get_element_by_id("info3").unwrap_throw().dyn_into::<HtmlElement>().unwrap_throw();
    #[cfg(target_arch = "wasm32")]
    let info4 = document.get_element_by_id("info4").unwrap_throw().dyn_into::<HtmlElement>().unwrap_throw();

    #[cfg(target_arch = "wasm32")]
    info1.set_inner_text(&format!("Using GPU: {}", gpu_info));

    let num_triangulations = triangulations.len();
    for (iter, (typ, vertices, mut indices)) in triangulations.into_iter().enumerate() {
        info!("Current triangulation: {} of {num_triangulations} | {typ:?} | Num vertices {}", iter+1, vertices.len());
        #[cfg(target_arch = "wasm32")]
        info2.set_inner_text(&format!("Current triangulation: {iter} of {num_triangulations} | {typ:?} | Num vertices {}", vertices.len()));

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
        #[cfg(target_arch = "wasm32")]
        info3.set_inner_text("Warming up!");
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
        let mut query_render_data: Vec<f64> = Vec::new();
        while query_render_data.len() < NUM_FRAMES_TO_CAPTURE {
            #[cfg(target_arch = "wasm32")]
            info3.set_inner_text(&format!("Currently collected {} frames | Rendering new frame!", query_render_data.len()));
            #[cfg(not(target_arch = "wasm32"))]
            info!("Currently collected {} frames | Rendering new frame!", query_render_data.len());
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
                    timestamp_writes: Some(
                        wgpu::RenderPassTimestampWrites {
                            query_set: &query_set,
                            beginning_of_pass_write_index: Some(0),
                            end_of_pass_write_index: Some(1),
                        }
                    ),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                renderpass.set_pipeline(&render_pipeline);
                renderpass.set_vertex_buffer(0, vertex_buffer.slice(..));
                renderpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                renderpass.draw_indexed(0..(u32::try_from(indices.len()).unwrap()), 0, 0..1);
            }
            encoder.resolve_query_set(&query_set, 0..2, &query_gpu_buf, 0);
            encoder.copy_buffer_to_buffer(&query_gpu_buf, 0, &query_cpu_buf, 0, 16);

            queue.submit([encoder.finish()]);

            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

            let (map_send, map_recv) = futures::channel::oneshot::channel();
            let slice = query_cpu_buf.slice(..);
            slice.map_async(wgpu::MapMode::Read, move |v| {
                let _ = map_send.send(v);
            });
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            if let Ok(Ok(_)) = map_recv.await {
                let buf_view = slice.get_mapped_range();
                let data: &[u8; 16] = buf_view.as_array().unwrap();
                let timestamps: &[u64] = bytemuck::cast_slice(data);
                let diff = (timestamps[1] - timestamps[0]) as f64 * 1e-6;
                query_render_data.push(diff);
            }
            query_cpu_buf.unmap();
        }
        info!("Done gathering data!");

        let current_triangulation_stats = get_triangulation_statistics(&vertices, &indices);
        let data = query_render_data;
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
        #[cfg(target_arch = "wasm32")]
        info4.set_inner_text(&format!("Previous render - Num frames collected: {} | Time taken to render frames {} ms.", info_gathered.num_frames, info_gathered.total_time_ms));
        #[cfg(not(target_arch = "wasm32"))]
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
    let gpu_name = gpu_info;

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
