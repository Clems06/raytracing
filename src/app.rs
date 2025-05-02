use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Event, KeyEvent, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};

use std::time::{Instant, SystemTime, UNIX_EPOCH};
use wgpu::{BindGroup, ComputePipeline, Device, Surface};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event;

use crate::raytracer::Raytracer;

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::{Rc, Weak };
use crate::shapes::{Sphere, Triangle};

use pollster::block_on;

fn normalize(arr1: [f32; 4])-> [f32; 4]{
    let [a1, a2, a3, a4] = arr1;
    let b = (a1*a1 + a2*a2 + a3*a3).powf(0.5);
    [a1/b, a2/b, a3/b, 1.0]
}

enum Direction {
    Left,
    Right,
    Front,
    Back
}

fn move_cam(cam_pos: [f32; 4], cam_dir: [f32; 4], move_dir: Direction, speed: f32) -> [f32; 4]{
    let [x, y, z, a] = cam_pos;
    let [dx, dy, ..] = cam_dir;
    match move_dir {
        Direction::Left => {
            [x - speed * dy, y + speed * dx,  z, a]
        },
        Direction::Right => {
            [x + speed * dy, y - speed * dx,  z, a]
        },
        Direction::Front => {
            [x + speed * dx, y + speed * dy,  z, a]
        },
        Direction::Back => {
            [x - speed * dx, y - speed * dy,  z, a]
        },
    }
}


#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    screen_width: f32,
    screen_height: f32,
    do_merging: f32,
    time: f32,
    camera_position: [f32; 4],
    camera_direction: [f32; 4],
    padded_bytes_per_row: u32,
    bounce_num: u32,
    pad: [f32; 2],
}
//use crate::Uniforms;

struct EguiData {
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    egui_rpass: egui_wgpu_backend::RenderPass,
    egui_context: egui::Context,
}

impl EguiData{
    fn new(window: &Window, device: &Device, surface_format: wgpu::TextureFormat) -> Self {
        let egui_context = egui::Context::default();
        let mut egui_state = egui_winit::State::new(
            egui_context.clone(),
            egui::ViewportId::default(),
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        // Create egui renderer
        let mut egui_renderer = egui_wgpu::Renderer::new(
            device,
            surface_format,
            None,
            1,
            false,
        );

        let mut egui_rpass = egui_wgpu_backend::RenderPass::new(device, surface_format, 1);

        Self {egui_state, egui_renderer, egui_rpass, egui_context}
    }
}

struct WgpuData {
    device: Device,
    queue: wgpu::Queue,
    adapter: wgpu::Adapter,
    config: wgpu::SurfaceConfiguration,
    surface: Surface<'static>,

}

impl WgpuData {
    fn new(window: &Arc<Window>) -> Self {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = block_on(instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                //compatible_surface: Some(&surface),
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
            .unwrap();

        let (device, queue) = block_on(adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            ))
            .unwrap();

        // Surface configuration
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats[0];

        println!("Width: {}", window.inner_size().width);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: window.inner_size().width,
            height: window.inner_size().height,
            present_mode: surface_caps.present_modes[0],
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        Self {
            device,
            queue,
            adapter,
            config,
            surface,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }
}

struct RenderData {
    texture_size: wgpu::Extent3d,
    texture: wgpu::Texture,
    render_bind_group: BindGroup,
    render_pipeline: wgpu::RenderPipeline,

}

impl RenderData {
    fn new(device: &Device, texture_size: wgpu::Extent3d, surface_format: wgpu::TextureFormat) -> Self {
        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Render Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("render_shader.wgsl").into()),
        });

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Output Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());



        // Create the render pipeline layout
        let render_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Render Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Output Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Render Bind Group"),
            layout: &render_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&render_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            texture_size,
            texture,
            render_bind_group,
            render_pipeline
        }

    }

    fn resize(&mut self, device: &Device, texture_size: wgpu::Extent3d){

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Output Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());



        // Create the render pipeline layout
        let render_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Render Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Output Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Render Bind Group"),
            layout: &render_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        self.render_bind_group = render_bind_group;
        self.texture_size = texture_size;
        self.texture = texture;

    }
}

struct App {
    frame_times: Vec<f32>,
    window: Arc<Window>,
    raytracer: Raytracer,
    egui_data: EguiData,
    wgpu_data: WgpuData,
    render_data: RenderData,
    last_frame_time: Instant,
    uniform_data: Uniforms,
    redraw: i32,
    pressed_keys: HashSet<winit::keyboard::KeyCode>,
    speed: f32,
    mouse_free: bool,
    show_gui: bool,
    gui_opacity: f32,
}

impl App {
    fn new(event_loop: &ActiveEventLoop) -> Self {
        let spheres = [
            Sphere::new( [4., 0., 0.], 9., [0.9, 0.9, 0.9, 1.], 0.5, [0; 12]),
            Sphere::new([13., -3., 0.], 16., [0.9, 0.9, 0.9, 1.], 1., [0; 12] ),
            Sphere::new([10., 6., 0.], 4., [0.9, 0., 0.9, 1.], 0., [0; 12] ),

            Sphere::new([-10., -10., 0.], 9.,  [0.05, 0., 0.9, 1.], 1., [0; 12] ),
            Sphere::new([10., 10., 0.], 9., [0.1, 0.1, 0.1, 1.], 1., [0; 12] ),

            Sphere::new([-10., 0., 0.], 9.,  [0.9, 0., 0.9, 1.], 1., [0; 12] ),
            Sphere::new([-10., -6., 0.], 4., [0.9, 0., 0.1, 1.], 1., [0; 12] ),

            Sphere::new([0., 0., -100.],9604., [0.9, 0.9, 0.9, 1.], 0., [0; 12] ),
        ];
        let triangles= [
            Triangle::new( [0.0, 0.0, 30.0], 0.5, [0.0, 1.0, 30.0], 0.0, [0.0, 0.0, 32.0], 0.0, [0.9, 0.9, 0.9, 1.]),
        ];



        let window = Arc::new(event_loop.create_window(
            winit::window::Window::default_attributes()
                .with_title("GPU Raytracer")
                .with_inner_size(PhysicalSize::new(1400, 700))
        ).unwrap());

        window.set_outer_position(PhysicalPosition::new(0, 0));



        let wgpu = WgpuData::new(&window);
        let surface_caps = wgpu.surface.get_capabilities(&wgpu.adapter);
        let surface_format = surface_caps.formats[0];


        let texture_size = wgpu::Extent3d {
            width: wgpu.config.width,
            height: wgpu.config.height,
            depth_or_array_layers: 1,
        };

        let bytes_per_pixel = 16;
        let unpadded_bytes_per_row = texture_size.width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256
        let padded_bytes_per_row = ((unpadded_bytes_per_row + align - 1) / align) * align;

        let mut uniform_data = Uniforms {
            screen_width: texture_size.width as f32,
            //screen_width: 10.,
            screen_height: texture_size.height as f32,
            do_merging: 0.,
            time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f32(),
            //screen_height: 10.,
            camera_position: [-20., 0., 0., 0.],
            //camera_position: [-14.492351, -83.80763, 0.0, 0.0],
            camera_direction: [1., 0., 0., 0.],
            //camera_direction: [0.031791087, 0.99949247, 0.0020453674, 1.0],

            padded_bytes_per_row,
            bounce_num: 8,
            pad: [0.; 2],
        };

        let buffer_size = (padded_bytes_per_row * texture_size.height) as wgpu::BufferAddress;
        let raytracer = Raytracer::new(&wgpu.device, &spheres, &triangles, uniform_data, buffer_size);

        let render_data = RenderData::new(&wgpu.device, texture_size, surface_format);


        Self {
            frame_times: vec![],
            window: window.clone(),
            raytracer,
            egui_data: EguiData::new(&window, &wgpu.device, surface_format),
            wgpu_data: wgpu,
            render_data,
            last_frame_time: Instant::now(),
            uniform_data,
            redraw: 0,
            pressed_keys: HashSet::new(),
            speed: 0.3,
            mouse_free: false,
            show_gui: true,
            gui_opacity: 0.0,
        }
    }
}

pub(crate) enum AppState {
    Uninitialized,
    Initialized(App),
}

impl AppState {
    pub fn new() -> Self{
        AppState::Uninitialized
    }
    fn init(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        match self {
            AppState::Initialized(_) => panic!(),
            AppState::Uninitialized => {
                *self = Self::Initialized(App::new(event_loop))
            }
        }
    }
}

impl ApplicationHandler for AppState {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if let winit::event::StartCause::Init = cause {
            match self {
                AppState::Initialized(app) => (),
                AppState::Uninitialized => self.init(event_loop),
            };
        } else {
            if let AppState::Initialized(app) = self {
                let now = Instant::now();
                let frame_time = now.duration_since(app.last_frame_time).as_secs_f32();
                app.last_frame_time = now;

                if app.frame_times.len() >= 100 {
                    app.frame_times.remove(0);
                }
                app.frame_times.push(frame_time);
            }
        }

    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if cfg!(target_os = "android") {
            match self {
                AppState::Initialized(app) => (),
                AppState::Uninitialized { .. } => self.init(event_loop),
            };
        }

    }

    fn window_event(
        &mut self,
        elwt: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {



        match self {
            AppState::Uninitialized => (),
            AppState::Initialized(app) => {
                let egui_consumed = app.egui_data.egui_state.on_window_event(&app.window, &event).consumed;
                if egui_consumed {
                    elwt.set_control_flow(ControlFlow::Poll);
                    &app.window.request_redraw();
                    return;
                }

                match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::KeyboardInput { event: key_event, .. } => {
                        match key_event.physical_key {
                            PhysicalKey::Code(key) => {
                                if key_event.state == event::ElementState::Pressed {
                                    app.pressed_keys.insert(key);
                                } else {
                                    app.pressed_keys.remove(&key);
                                    if key == winit::keyboard::KeyCode::KeyG {
                                        app.show_gui = !app.show_gui;
                                    } else if key == winit::keyboard::KeyCode::KeyF {
                                        app.mouse_free = !app.mouse_free;
                                        app.window.set_cursor_visible(app.mouse_free);
                                    }
                                }
                            }
                            _ => {}
                        }
                    },
                    WindowEvent::Resized(new_size) => {
                        app.wgpu_data.resize(new_size);
                        let texture_size = wgpu::Extent3d {
                            width: app.wgpu_data.config.width,
                            height: app.wgpu_data.config.height,
                            depth_or_array_layers: 1,
                        };

                        let bytes_per_pixel = 16;
                        let unpadded_bytes_per_row = texture_size.width * bytes_per_pixel;
                        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256
                        let padded_bytes_per_row = ((unpadded_bytes_per_row + align - 1) / align) * align;

                        app.uniform_data.screen_width = texture_size.width as f32;
                        app.uniform_data.screen_height = texture_size.height as f32;
                        app.uniform_data.padded_bytes_per_row = padded_bytes_per_row;
                        let buffer_size = (padded_bytes_per_row * texture_size.height) as wgpu::BufferAddress;
                        app.raytracer.resize(&app.wgpu_data.device, buffer_size);
                        app.render_data.resize(&app.wgpu_data.device, texture_size);
                        app.raytracer.change_uniforms(&app.wgpu_data.queue, app.uniform_data);
                    },
                    WindowEvent::RedrawRequested => {
                        for key in &app.pressed_keys {
                            match key {
                                winit::keyboard::KeyCode::Escape => {
                                    elwt.exit();
                                }
                                winit::keyboard::KeyCode::Numpad1 => {
                                    app.redraw = 1;
                                }
                                winit::keyboard::KeyCode::KeyD => {
                                    app.uniform_data.camera_position = move_cam(app.uniform_data.camera_position, app.uniform_data.camera_direction, Direction::Right, app.speed);
                                    app.uniform_data.do_merging = 0.;
                                    app.redraw = 700;
                                }
                                winit::keyboard::KeyCode::KeyA => {
                                    app.uniform_data.camera_position = move_cam(app.uniform_data.camera_position, app.uniform_data.camera_direction, Direction::Left, app.speed);
                                    app.uniform_data.do_merging = 0.;
                                    app.redraw = 700;
                                }
                                winit::keyboard::KeyCode::KeyW => {
                                    app.uniform_data.camera_position = move_cam(app.uniform_data.camera_position, app.uniform_data.camera_direction, Direction::Front, app.speed);
                                    app.uniform_data.do_merging = 0.;
                                    app.redraw = 700;
                                }
                                winit::keyboard::KeyCode::KeyS => {
                                    app.uniform_data.camera_position = move_cam(app.uniform_data.camera_position, app.uniform_data.camera_direction, Direction::Back, app.speed);
                                    app.uniform_data.do_merging = 0.;
                                    app.redraw = 700;
                                }
                                winit::keyboard::KeyCode::Space => {
                                    app.uniform_data.camera_position[2] += app.speed;
                                    app.uniform_data.do_merging = 0.;
                                    app.redraw = 700;
                                }
                                winit::keyboard::KeyCode::ShiftLeft => {
                                    app.uniform_data.camera_position[2] -= app.speed;
                                    app.uniform_data.do_merging = 0.;
                                    app.redraw = 700;
                                }
                                _ => {}
                            }
                        }


                        let now = Instant::now();
                        let frame_time = now.duration_since(app.last_frame_time).as_secs_f32();
                        app.last_frame_time = now;

                        if app.frame_times.len() >= 100 {
                            app.frame_times.remove(0);
                        }
                        app.frame_times.push(frame_time);


                        // Run the compute shader
                        let mut compute_encoder = app.wgpu_data.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Compute compute_Encoder"),
                        });

                        app.raytracer.calculate(&mut compute_encoder, app.render_data.texture_size);

                        // Copy the output buffer to the texture

                        compute_encoder.copy_buffer_to_texture(
                            wgpu::ImageCopyBuffer {
                                buffer: app.raytracer.get_output(),
                                layout: wgpu::ImageDataLayout {
                                    offset: 0,
                                    bytes_per_row: Some(app.uniform_data.padded_bytes_per_row),
                                    rows_per_image: Some(app.render_data.texture_size.height),
                                },
                            },
                            wgpu::ImageCopyTexture {
                                texture: &app.render_data.texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            app.render_data.texture_size,
                        );

                        // Render the texture to the screen
                        let surface_texture = app.wgpu_data.surface.get_current_texture().unwrap();
                        let surface_view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());

                        {
                            let mut render_pass = compute_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Render Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &surface_view,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            });

                            render_pass.set_pipeline(&app.render_data.render_pipeline);
                            render_pass.set_bind_group(0, &app.render_data.render_bind_group, &[]);
                            render_pass.draw(0..6, 0..1);
                        }

                        app.wgpu_data.queue.submit(std::iter::once(compute_encoder.finish()));

                        if app.show_gui {
                            // Begin egui frame
                            app.egui_data.egui_state.handle_platform_output(
                                &app.window,
                                egui::PlatformOutput::default(),
                            );

                            let raw_input = app.egui_data.egui_state.take_egui_input(&app.window);
                            let egui_output = app.egui_data.egui_context.run(raw_input, |ctx| {
                                // Define your UI here
                                egui::Window::new("Raytracer Controls")
                                    .resizable(true)
                                    .default_pos([10.0, 10.0])
                                    .show(ctx, |ui| {
                                        ui.set_min_width(250.0);

                                        // Camera position display
                                        ui.heading("Camera");
                                        ui.label(format!("Position: [{:.2}, {:.2}, {:.2}]",
                                                         app.uniform_data.camera_position[0],
                                                         app.uniform_data.camera_position[1],
                                                         app.uniform_data.camera_position[2]));
                                        ui.label(format!("Direction: [{:.2}, {:.2}, {:.2}]",
                                                         app.uniform_data.camera_direction[0],
                                                         app.uniform_data.camera_direction[1],
                                                         app.uniform_data.camera_direction[2]));

                                        // Performance metrics
                                        ui.separator();
                                        ui.heading("Performance");
                                        let avg_frame_time = if !app.frame_times.is_empty() {
                                            app.frame_times.iter().sum::<f32>() / app.frame_times.len() as f32
                                        } else {
                                            0.0
                                        };
                                        ui.label(format!("Frame time: {:.2} ms", frame_time * 1000.0));
                                        ui.label(format!("Average: {:.2} ms", avg_frame_time * 1000.0));
                                        ui.label(format!("FPS: {:.1}", 1.0 / avg_frame_time.max(0.001)));

                                        // GUI settings
                                        ui.separator();
                                        ui.heading("Settings");
                                        ui.add(egui::Slider::new(&mut app.uniform_data.bounce_num, 1..=20).text("Number of bounces"));

                                        // Controls help
                                        ui.separator();
                                        ui.heading("Controls");
                                        ui.label("WASD - Move camera");
                                        ui.label("Space/Shift - Move up/down");
                                        ui.label("Mouse - Look around");
                                        ui.label("G - Toggle GUI");
                                        ui.label("F - Toggle mouse tracking");
                                        ui.label("Esc - Exit");
                                    });
                            });

                            // Update window based on gui output
                            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                                size_in_pixels: [app.wgpu_data.config.width, app.wgpu_data.config.height],
                                pixels_per_point: app.window.scale_factor() as f32,
                            };

                            // Render egui
                            let mut encoder = app.wgpu_data.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("GUI Encoder"),
                            });
                            let paint_jobs = app.egui_data.egui_context.tessellate(egui_output.shapes, app.window.scale_factor() as f32);
                            app.egui_data.egui_renderer.update_buffers(
                                &app.wgpu_data.device,
                                &app.wgpu_data.queue,
                                &mut encoder,
                                &paint_jobs,
                                &screen_descriptor,
                            );


                            let mut encoder = app.wgpu_data.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("encoder"),
                            });

                            // Upload all resources for the GPU.
                            let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
                                physical_width: app.wgpu_data.config.width,
                                physical_height: app.wgpu_data.config.height,
                                scale_factor: app.window.scale_factor() as f32,
                            };
                            let tdelta: egui::TexturesDelta = egui_output.textures_delta;
                            app.egui_data.egui_rpass
                                .add_textures(&app.wgpu_data.device, &app.wgpu_data.queue, &tdelta)
                                .expect("add texture ok");
                            app.egui_data.egui_rpass.update_buffers(&app.wgpu_data.device, &app.wgpu_data.queue, &paint_jobs, &screen_descriptor);

                            // Record all render passes.
                            app.egui_data.egui_rpass
                                .execute(
                                    &mut encoder,
                                    &surface_view,
                                    &paint_jobs,
                                    &screen_descriptor,
                                    None,
                                )
                                .unwrap();
                            // Submit the commands.
                            app.wgpu_data.queue.submit(std::iter::once(encoder.finish()));
                        }


                        surface_texture.present();
                    }
                    _ => {}
                }
            },
        }


    }
    fn device_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop,
                    device_id: winit::event::DeviceId,
                    event: winit::event::DeviceEvent, ) {
        match self {
            AppState::Uninitialized => (),
            AppState::Initialized(app) => (
                match event {
                    winit::event::DeviceEvent::MouseMotion { delta: (dx, dz) } => {
                        let center_x = app.window.inner_size().width as f64 / 2.0;
                        let center_y = app.window.inner_size().height as f64 / 2.0;

                        if !app.mouse_free {
                            let [x, y, z, a] = app.uniform_data.camera_direction;
                            //let (delta_x, delta_y) = (-(pos.x - center_x) as f32 * 0.001, (pos.y - center_y) as f32 * 0.001);
                            let (delta_x, delta_y) = (-dx as f32 * 0.001, -dz as f32 * 0.001);
                            let new_x = x * delta_x.cos() - y * delta_x.sin();
                            let new_y = x * delta_x.sin() + y * delta_x.cos();

                            app.uniform_data.do_merging = 0.;

                            //uniform_data.camera_direction = sum_list([x, y, z+delta_y, a], scalar_list(perpendicular([x, y, 0., 0.]), delta_x));
                            //uniform_data.camera_direction = [new_x, new_y, z+delta_y, a];
                            app.uniform_data.camera_direction = normalize([new_x, new_y, z + delta_y, a]);

                            app.redraw = 700;


                            app.window.set_cursor_position(PhysicalPosition::new(center_x, center_y)).unwrap();
                        } else {
                            app.redraw = 10;
                        }
                    },
                    _ => {},

                }
            )
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        match self {
            AppState::Uninitialized => (),
            AppState::Initialized(app) => (
                if app.redraw != 0 {
                    app.redraw-=1;
                    app.uniform_data.time = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .subsec_millis() as f32;
                    app.raytracer.change_uniforms(&app.wgpu_data.queue, app.uniform_data);
                    app.window.request_redraw();
                    app.uniform_data.do_merging = 1.;
                }
            )
        }
    }
}