use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::EventLoopExtWebSys;
use winit::{application::ApplicationHandler, event::{KeyEvent, WindowEvent}, event_loop::EventLoop, keyboard::{KeyCode, PhysicalKey}, window::Window};

use crate::{gather_data::{DataGatherType, GatherData}, metrics::{TriangulationStatistics, get_triangulation_statistics}, vertex::{TriangulationType, Vertex, generate_circle}};

pub mod gather_data;
pub mod metrics;
pub mod vertex;

// Initial setup was done through the learn-wgpu tutorial on: https://sotrh.github.io/learn-wgpu/

const DEFAULT_TRIANGULATION: (NumVerticesCalculator, u32, TriangulationType) = (NumVerticesCalculator::Power(3), 5, TriangulationType::Fan);

#[derive(Clone, Copy, Debug)]
pub enum NumVerticesCalculator {
    Power(usize),
    Multiplicative(usize),
    Static(usize),
}

impl NumVerticesCalculator {
    pub fn get_num_vertices(&self, value: u32) -> usize {
        match self {
            Self::Power(x) => x.pow(value),
            Self::Multiplicative(x) => x * value as usize,
            Self::Static(x) => *x,
        }
    }
}

pub struct State {
    window: Arc<Window>,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
    surface: wgpu::Surface<'static>,
    surface_format: wgpu::TextureFormat,
    render_pipeline_layout: wgpu::PipelineLayout,
    render_pipeline: wgpu::RenderPipeline,
    shader: wgpu::ShaderModule,
    vertex_buffer: wgpu::Buffer,
    index_buffer: (wgpu::Buffer, u32),
    num_vertex_calculator: NumVerticesCalculator,
    num_vertex_calculator_value: u32,
    current_triangulation: TriangulationType,
    current_triangulation_stats: TriangulationStatistics,
}

impl State {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::GL,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await
            .unwrap();
        let adapter_info = adapter.get_info();
        println!("Using device {:?}", adapter_info.name);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::from_name("POLYGON_MODE_LINE").unwrap(),
                ..Default::default()
            })
            .await
            .unwrap();

        let size = window.inner_size();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[],
            ..Default::default()
        });

        let (surface, surface_format, render_pipeline) = Self::get_pipeline(&instance, window.clone(), &adapter, &device, &render_pipeline_layout, &shader);

        let (num_vertex_calculator, num_vertex_calculator_value, current_triangulation) = Self::get_default_triangulation();

        let (vertices, mut indices) = generate_circle(&current_triangulation, Self::calculate_num_vertices(num_vertex_calculator, num_vertex_calculator_value), Self::get_radius());
        indices.reverse();

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
            queue,
            size,
            surface,
            surface_format,
            render_pipeline,
            vertex_buffer,
            index_buffer: (index_buffer, indices.len() as u32),
            num_vertex_calculator_value,
            current_triangulation: TriangulationType::Fan,
            instance,
            adapter,
            shader,
            render_pipeline_layout,
            current_triangulation_stats,
            num_vertex_calculator,
        };

        state.configure_surface();

        Ok(state)
    }

    fn get_pipeline<'a>(
        instance: &wgpu::Instance,
        window: Arc<Window>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        render_pipeline_layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
    ) -> (wgpu::Surface<'a>, wgpu::TextureFormat, wgpu::RenderPipeline) {
        let surface = instance.create_surface(window.clone()).unwrap();
        let cap = surface.get_capabilities(&adapter);
        let surface_format = cap.formats[0];

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
                polygon_mode: wgpu::PolygonMode::Line,
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

        (surface, surface_format, render_pipeline)
    }

    fn recreate_pipeline(&mut self) {
        let (surface, surface_format, render_pipeline) = Self::get_pipeline(&self.instance, self.window.clone(), &self.adapter, &self.device, &self.render_pipeline_layout, &self.shader);
        self.surface = surface;
        self.surface_format = surface_format;
        self.render_pipeline = render_pipeline;
        self.size = self.window.inner_size();
        self.configure_surface();
    }

    pub fn get_window(&self) -> &Window {
        &self.window
    }

    pub fn configure_surface(&self) {
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            view_formats: vec![self.surface_format.add_srgb_suffix()],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.size.width,
            height: self.size.height,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::Immediate,
        };
        self.surface.configure(&self.device, &surface_config);
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;

        self.configure_surface();
    }

    pub fn render(&mut self) {
        let surface_texture = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost) => {
                self.configure_surface();
                self.surface.get_current_texture().expect("Failed to get the next swap chain texture!")
            },
            Err(wgpu::SurfaceError::Outdated) => {
                self.configure_surface();
                self.surface.get_current_texture().expect("Failed to get the next swap chain texture!")
            }
            Err(e) => panic!("Failed to get next swap chain texture: {e}"),
        };
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                // Without add_srgb_suffix() the image we will be working with
                // might not be "gamma correct".
                format: Some(self.surface_format.add_srgb_suffix()),
                ..Default::default()
            });

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
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

    fn set_vertices_and_indices(&mut self) {
        let (vertices, mut indices) = generate_circle(&self.current_triangulation, self.get_num_vertices(), Self::get_radius());
        indices.reverse();

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

    pub fn change_to_next_triangulation(&mut self) {
        self.current_triangulation.next();
        println!("Now showing triangulation {:?}.", self.current_triangulation);

        self.set_vertices_and_indices();
    }

    pub fn set_triangulation(&mut self, triangulation_type: TriangulationType) {
        self.current_triangulation = triangulation_type;
        println!("Now showing triangulation {:?}.", self.current_triangulation);

        self.set_vertices_and_indices();
    }

    pub fn step_up_num_vertices(&mut self) {
        self.num_vertex_calculator_value += 1;
        self.set_vertices_and_indices();
        println!("Now drawing {} vertices.", self.get_num_vertices());
    }

    pub fn get_next_num_vertices(&self) -> usize {
        self.num_vertex_calculator.get_num_vertices(self.num_vertex_calculator_value)
    }

    pub fn step_down_num_vertices(&mut self) {
        if self.num_vertex_calculator_value <= 1 {
            println!("Cannot have less than 3 vertices!");
            return;
        }
        self.num_vertex_calculator_value -= 1;
        self.set_vertices_and_indices();
        println!("Now drawing {} vertices.", self.get_num_vertices());
    }

    pub fn toggle_full_screen(&mut self) {
        self.window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(self.window.current_monitor())));
        self.recreate_pipeline();
    }

    pub fn get_num_vertices(&self) -> usize {
        Self::calculate_num_vertices(self.num_vertex_calculator, self.num_vertex_calculator_value)
    }

    fn calculate_num_vertices(vertex_num_type: NumVerticesCalculator, vertices_power: u32) -> usize {
        vertex_num_type.get_num_vertices(vertices_power)
    }

    fn get_radius() -> f32 {
        0.75
    }

    pub fn get_triangulation_type(&self) -> TriangulationType {
        self.current_triangulation.clone()
    }

    pub fn get_vertex_calculator_value(&self) -> u32 {
        self.num_vertex_calculator_value
    }

    pub fn get_current_triangulation_statistics(&self) -> TriangulationStatistics {
        self.current_triangulation_stats.clone()
    }

    fn get_default_triangulation() -> (NumVerticesCalculator, u32, TriangulationType) {
        DEFAULT_TRIANGULATION
    }

    pub fn reset_to_default_triangulation(&mut self) {
        let (c, n, t) = Self::get_default_triangulation();

        self.num_vertex_calculator_value = n;
        self.current_triangulation = t;
        self.num_vertex_calculator = c;

        self.set_vertices_and_indices();
    }

    pub fn get_window_size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }
}

pub struct App {
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<State>>,
    state: Option<State>,
    data_gathering: Option<GatherData>,
}

impl App {
    pub fn new(#[cfg(target_arch = "wasm32")] event_loop: &EventLoop<State>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());
        Self {
            state: None,
            data_gathering: None,
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

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.state = Some(pollster::block_on(State::new(window)).unwrap());
        }

        #[cfg(target_arch = "wasm32")]
        {
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

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, mut event: State) {
        #[cfg(target_arch = "wasm32")]
        {
            event.window.request_redraw();
            event.resize(
                event.window.inner_size().width,
                event.window.inner_size().height,
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
            },
            WindowEvent::KeyboardInput { event: KeyEvent {
                physical_key: PhysicalKey::Code(code),
                state: key_state,
                ..
            }, .. } => match (code, key_state.is_pressed()) {
                (KeyCode::Escape, true) => event_loop.exit(),
                (KeyCode::ArrowRight, true) => state.change_to_next_triangulation(),
                (KeyCode::ArrowDown, true) => state.step_down_num_vertices(),
                (KeyCode::ArrowUp, true) => state.step_up_num_vertices(),
                (KeyCode::KeyF, true) => state.toggle_full_screen(),
                // (KeyCode::KeyO, true) => {
                //     println!("WARNING: It will read the whole file of the stored triangulations for every time it switches between triangulations!");
                //     self.data_gathering = Some(GatherData::new(state, DataGatherType::TriangulationsFromFile(pollster::block_on(rfd::AsyncFileDialog::new().pick_file()).expect("Expected a file to be picked!"))))
                // }
                _ => {}
            },
            _ => {}
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
