use std::sync::Arc;

use chrono::{DateTime, Local};
use log::info;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::EventLoopExtWebSys;
use winit::{application::ApplicationHandler, dpi::PhysicalSize, event::{ElementState, KeyEvent, MouseButton, Touch, WindowEvent}, event_loop::EventLoop, keyboard::{KeyCode, PhysicalKey}, window::Window};

use crate::{gather_data::GatherData, metrics::{TriangulationStatistics, get_triangulation_statistics}, vertex::{TriangulationType, Vertex}};

pub mod gather_data;
pub mod metrics;
pub mod vertex;

// Initial setup was done through the learn-wgpu tutorial on: https://sotrh.github.io/learn-wgpu/

pub struct State {
    window: Arc<Window>,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    gpu_name: String,
    queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    is_surface_configured: bool,
    render_pipeline_layout: wgpu::PipelineLayout,
    render_pipeline: wgpu::RenderPipeline,
    shader: wgpu::ShaderModule,
    vertex_buffer: wgpu::Buffer,
    index_buffer: (wgpu::Buffer, u32),
    current_triangulation_stats: TriangulationStatistics,
    triangulations: Vec<(TriangulationType, Vec<Vertex>, Vec<u32>)>,
    current_triangulation_index: usize,
}

impl State {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::VULKAN,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::GL,
            flags: Default::default(),
            memory_budget_thresholds: Default::default(),
            backend_options: Default::default(),
        });


        let size = window.inner_size();

        if size.width <= 0 && size.height <= 0 {
            panic!("Window size is 0");
        }

        #[cfg(target_arch="wasm32")]
        let size = PhysicalSize { width: 1920, height: 1080 };

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;
        let adapter_info = adapter.get_info();
        info!("Using device {:?}", adapter_info.name);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Requesting device"),
                #[cfg(target_arch = "wasm32")]
                required_features: wgpu::Features::empty(),//::from_name("POLYGON_MODE_LINE").unwrap(),
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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[],
            ..Default::default()
        });

        let (surface_config, render_pipeline) = Self::get_pipeline(&size, &surface, &adapter, &device, &render_pipeline_layout, &shader);

        #[cfg(target_arch = "wasm32")]
        let bin_data = include_bytes!("fan_stripe_max_area.bin");
        #[cfg(not(target_arch = "wasm32"))]
        let bin_data = &std::fs::read("./src/fan_stripe_max_area.bin").unwrap();
        let mut data: Vec<(TriangulationType, Vec<Vertex>, Vec<u32>)> = postcard::from_bytes(bin_data).unwrap();
        #[cfg(target_arch = "wasm32")]
        let bin_data = include_bytes!("random_triangulations_262_144.bin");
        #[cfg(not(target_arch = "wasm32"))]
        let bin_data = &std::fs::read("./src/random_triangulations_262_144.bin").unwrap();
        let mut data_2: Vec<(TriangulationType, Vec<Vertex>, Vec<u32>)> = postcard::from_bytes(bin_data).unwrap();
        data.append(&mut data_2);
        let (_, vertices, mut indices) = data[0].clone();
        indices.chunks_exact_mut(3).for_each(|c| c.reverse());

        let current_triangulation_stats = get_triangulation_statistics(&vertices, &indices);

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

        let state = State {
            window,
            device,
            gpu_name: adapter_info.name,
            queue,
            size,
            surface,
            render_pipeline,
            vertex_buffer,
            index_buffer: (index_buffer, indices.len() as u32),
            instance,
            adapter,
            shader,
            render_pipeline_layout,
            current_triangulation_stats,
            triangulations: data,
            current_triangulation_index: 0,
            surface_config,
            is_surface_configured: false,
        };

        Ok(state)
    }

    fn get_pipeline<'a>(
        size: &PhysicalSize<u32>,
        surface: &wgpu::Surface<'a>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        render_pipeline_layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
    ) -> (wgpu::SurfaceConfiguration, wgpu::RenderPipeline) {
        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps.formats.iter().find(|f| f.is_srgb()).copied().unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoNoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(device, &config);

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 0,
                            }
                        ],
                    },
                ],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,//Some(wgpu::Face::Back),
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
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
            }),
            cache: None,
            multiview_mask: None,
        });

        (config, render_pipeline)
    }

    fn recreate_pipeline(&mut self) {
        let surface = self.instance.create_surface(self.window.clone()).unwrap();
        let (surface_config, render_pipeline) = Self::get_pipeline(&self.window.inner_size(), &surface, &self.adapter, &self.device, &self.render_pipeline_layout, &self.shader);
        self.surface = surface;
        self.surface_config = surface_config;
        self.render_pipeline = render_pipeline;
        #[cfg(target_arch = "wasm32")]
        {
            self.size = PhysicalSize { width: 1920, height: 1080 };//self.window.inner_size();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.size = self.window.inner_size();
        }
        self.is_surface_configured = false;
    }

    pub fn get_window(&self) -> &Window {
        &self.window
    }


    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;

        if self.size.width > 0 && self.size.height > 0 {
            #[cfg(target_arch = "wasm32")]
            {
                self.size = PhysicalSize { width: 1920, height: 1080 };
            }
            self.surface_config.width = self.size.width;
            self.surface_config.height = self.size.height;
            self.surface.configure(&self.device, &self.surface_config);
            self.is_surface_configured = true;
        }

    }

    pub fn render(&mut self) {
        self.window.request_redraw();

        if !self.is_surface_configured {
            return;
        }

        let surface_texture = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.surface_config);
                self.surface.get_current_texture().expect("Failed to get the next swap chain texture!")
            },
            Err(wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.surface_config);
                self.surface.get_current_texture().expect("Failed to get the next swap chain texture!")
            }
            Err(e) => panic!("Failed to get next swap chain texture: {e}"),
        };
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                // Without add_srgb_suffix() the image we will be working with
                // might not be "gamma correct".
                format: Some(self.surface_config.format.add_srgb_suffix()),
                ..Default::default()
            });

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Forward pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
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

            renderpass.set_pipeline(&self.render_pipeline);
            renderpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            renderpass.set_index_buffer(self.index_buffer.0.slice(..), wgpu::IndexFormat::Uint32);
            renderpass.draw_indexed(0..self.index_buffer.1, 0, 0..1);
        }

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();
    }

    pub fn get_triangulations_len(&self) -> usize {
        self.triangulations.len()
    }

    fn set_vertices_and_indices(&mut self) {
        let (triangulation_type, vertices, mut indices) = self.triangulations[self.current_triangulation_index].clone();

        info!("{triangulation_type:?}: num_vertices {}", vertices.len());
        indices.chunks_exact_mut(3).for_each(|c| c.reverse());

        self.current_triangulation_stats = get_triangulation_statistics(&vertices, &indices);

        self.vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        self.index_buffer.0 = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.index_buffer.1 = indices.len() as u32;
    }

    pub fn set_triangulation(&mut self, index: usize) {
        self.current_triangulation_index = index;
        self.set_vertices_and_indices();
    }

    /// Returns true if we are on the last triangulation
    pub fn next_triangulation(&mut self) -> bool {
        let mut reset = false;
        self.current_triangulation_index += 1;
        if self.current_triangulation_index >= self.triangulations.len() {//|| self.current_triangulation_index >= 48 {
            reset = true;
            self.current_triangulation_index = 0;
        }
        info!("{} of {}", self.current_triangulation_index, self.triangulations.len());
        self.set_vertices_and_indices();
        reset
    }

    pub fn prev_triangulation(&mut self) {
        self.current_triangulation_index = self.current_triangulation_index.checked_sub(1).unwrap_or(self.triangulations.len()-1);
        info!("{} of {}", self.current_triangulation_index, self.triangulations.len());
        self.set_vertices_and_indices();
    }

    pub fn get_num_vertices(&self) -> usize {
        self.triangulations[self.current_triangulation_index].1.len()
    }

    pub fn get_triangulation_type(&self) -> TriangulationType {
        self.triangulations[self.current_triangulation_index].0.clone()
    }

    pub fn toggle_full_screen(&mut self) {
        self.window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(self.window.current_monitor())));
        self.recreate_pipeline();
    }

    pub fn get_current_triangulation_statistics(&self) -> TriangulationStatistics {
        self.current_triangulation_stats.clone()
    }

    pub fn reset_to_default_triangulation(&mut self) {
        self.set_triangulation(0);
    }

    pub fn get_gpu_name(&self) -> String {
        self.gpu_name.clone()
    }

    pub fn get_window_size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }

    pub fn get_render_size(&self) -> (u32, u32) {
        (self.size.width, self.size.height)
    }
}

pub struct App {
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<State>>,
    state: Option<State>,
    data_gathering: Option<GatherData>,
    window: Option<Arc<Window>>,
    timer: DateTime<Local>,
    frame_counter: usize,
}

impl App {
    pub fn new(#[cfg(target_arch = "wasm32")] event_loop: &EventLoop<State>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());
        Self {
            state: None,
            data_gathering: None,
            window: None,
            timer: Local::now(),
            frame_counter: 0,
            #[cfg(target_arch = "wasm32")]
            proxy,
        }
    }
}

impl ApplicationHandler<State> for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        #[allow(unused_mut)]
        let mut window_attributes = Window::default_attributes();

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::{platform::web::WindowAttributesExtWebSys, window};

            const CANVAS_ID: &str = "canvas";

            let window = wgpu::web_sys::window().unwrap_throw();
            let document = window.document().unwrap_throw();
            let canvas = document.get_element_by_id(CANVAS_ID).unwrap_throw();
            let html_canvas_elem = canvas.unchecked_into();
            window_attributes = window_attributes.with_canvas(Some(html_canvas_elem));
        }

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        self.window = Some(window.clone());

        #[cfg(not(target_arch = "wasm32"))]
        {
            println!("Creating new state!");
            self.state = Some(pollster::block_on(State::new(window)).unwrap());
        }
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, mut event: State) {
        #[cfg(target_arch = "wasm32")]
        {
            event.window.request_redraw();
            event.resize(
                event.window.inner_size(),
            );
        }
        self.state = Some(event);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    )
    {
        #[cfg(target_arch = "wasm32")]
        {
            if let WindowEvent::Resized(size) = event && self.state.is_none() && size.width > 0 && size.height > 0 && let Some(window) = self.window.clone() {
                if let Some(proxy) = self.proxy.take() {
                    wasm_bindgen_futures::spawn_local(async move {
                        assert!(
                            proxy
                                .send_event(
                                    State::new(window)
                                        .await
                                        .expect("Unable to create canvas!")
                                )
                                .is_ok()
                        )
                    });
                }
            }
        }

        let state = match &mut self.state {
            Some(canvas) => canvas,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::RedrawRequested => {
                state.render();
                if let Some(gather_data) = &mut self.data_gathering {
                    gather_data.update(state);
                };
                state.get_window().request_redraw();
                self.frame_counter += 1;
            },
            WindowEvent::KeyboardInput { event: KeyEvent {
                physical_key: PhysicalKey::Code(code),
                state: key_state,
                ..
            }, .. } => match (code, key_state.is_pressed()) {
                (KeyCode::Escape, true) => event_loop.exit(),
                (KeyCode::ArrowRight, true) => {let _ = state.next_triangulation();},
                (KeyCode::ArrowLeft, true) => state.prev_triangulation(),
                // (KeyCode::KeyS, true) => self.data_gathering = Some(GatherData::new(state)),
                (KeyCode::KeyF, true) => state.toggle_full_screen(),
                _ => {}
            },
            WindowEvent::MouseInput { state: button_state, button, .. } => {
                match (button, button_state) {
                    (MouseButton::Left, ElementState::Pressed) => {
                        self.data_gathering = Some(GatherData::new(state));
                    },
                    _ => {}
                }
            },
            WindowEvent::Touch(_) => {
                self.data_gathering = Some(GatherData::new(state));
            },
            _ => {}
        }
        let elapsed_ms = (Local::now()-self.timer).num_milliseconds();
        if  elapsed_ms >= 1_000 {
            let fps = self.frame_counter as f64 / (elapsed_ms as f64 / 1_000.0);
            let frame_time = elapsed_ms as f64 / self.frame_counter as f64;
            let triangulation_type = state.get_triangulation_type();
            let num_vertices = state.get_num_vertices();
            state.window.set_title(&format!("{triangulation_type:?} - {num_vertices} | FPS: {fps:.2} | Frametime: {frame_time:.2}ms"));
            self.timer = Local::now();
            self.frame_counter = 0;
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
    }
    #[cfg(target_arch = "wasm32")]
    {
        console_log::init_with_level(log::Level::Info).unwrap_throw();
    }

    let event_loop = EventLoop::with_user_event().build()?;

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut app = App::new();
        println!("App created!");
        event_loop.run_app(&mut app)?;
    }

    #[cfg(target_arch = "wasm32")]
    {
        let app = App::new(&event_loop);
        event_loop.spawn_app(app);
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run_web() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    run().unwrap_throw();

    Ok(())
}
