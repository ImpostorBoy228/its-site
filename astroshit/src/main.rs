use wgpu::util::DeviceExt;
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use winit::dpi::PhysicalSize;
use pollster::FutureExt;
use bytemuck::{Pod, Zeroable};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const SHADER: &str = r#"
struct Uniforms {
    mvp: mat4x4<f32>,
    color: vec4<f32>,
}
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return uniforms.mvp * vec4<f32>(position, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return uniforms.color;
}
"#;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct SphereUniform {
    mvp: [[f32; 4]; 4],
    color: [f32; 4],
}

type Mat4 = [[f32; 4]; 4];

fn identity() -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn mat4_mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut r = [[0.0; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            for k in 0..4 {
                r[col][row] += a[k][row] * b[col][k];
            }
        }
    }
    r
}

fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y / 2.0).tan();
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, far * near / (near - far), 0.0],
    ]
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / len, v[1] / len, v[2] / len]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> Mat4 {
    let f = normalize3(sub3(eye, target));
    let s = normalize3(cross3(up, f));
    let u = cross3(f, s);
    [
        [s[0], u[0], f[0], 0.0],
        [s[1], u[1], f[1], 0.0],
        [s[2], u[2], f[2], 0.0],
        [-dot3(s, eye), -dot3(u, eye), -dot3(f, eye), 1.0],
    ]
}

fn translate(v: [f32; 3]) -> Mat4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [v[0], v[1], v[2], 1.0],
    ]
}

fn scale(s: f32) -> Mat4 {
    [
        [s, 0.0, 0.0, 0.0],
        [0.0, s, 0.0, 0.0],
        [0.0, 0.0, s, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn get_sun(mjd: f64) -> [f32; 3] {
    let n = mjd - 51544.5;
    let l_deg = (280.460 + 0.9856474 * n) % 360.0;
    let g_deg = (357.528 + 0.9856003 * n) % 360.0;
    let g = g_deg.to_radians();
    let lambda = l_deg.to_radians()
        + 1.915_f64.to_radians() * g.sin()
        + 0.020_f64.to_radians() * (2.0 * g).sin();
    let eps = 23.4393_f64.to_radians();
    [
        lambda.cos() as f32,
        (eps.cos() * lambda.sin()) as f32,
        (eps.sin() * lambda.sin()) as f32,
    ]
}

fn get_nsk(mjd: f64) -> [f32; 3] {
    let n = mjd - 51544.5;
    let phi = 55.03_f64.to_radians();
    let lon = 82.93_f64.to_radians();
    let theta = (280.46061837 + 360.98564736629 * n).to_radians();
    [
        (phi.cos() * (lon + theta).cos()) as f32,
        (phi.cos() * (lon + theta).sin()) as f32,
        phi.sin() as f32,
    ]
}

fn mjd_now() -> f64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    now / 86400.0 + 40587.0
}

fn create_sphere(stacks: u32, slices: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for i in 0..=stacks {
        let phi = std::f32::consts::PI * i as f32 / stacks as f32;
        for j in 0..=slices {
            let theta = 2.0 * std::f32::consts::PI * j as f32 / slices as f32;
            vertices.push(Vertex {
                position: [
                    phi.sin() * theta.cos(),
                    phi.cos(),
                    phi.sin() * theta.sin(),
                ],
            });
        }
    }
    for i in 0..stacks {
        for j in 0..slices {
            let first = i * (slices + 1) + j;
            indices.push(first);
            indices.push(first + 1);
            indices.push(first + slices + 1);
            indices.push(first + 1);
            indices.push(first + slices + 2);
            indices.push(first + slices + 1);
        }
    }
    (vertices, indices)
}

struct App {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,
    bind_group_layout: wgpu::BindGroupLayout,
    earth_uniform: wgpu::Buffer,
    earth_bind_group: wgpu::BindGroup,
    sun_uniform: wgpu::Buffer,
    sun_bind_group: wgpu::BindGroup,
    mjd: f64,
    sun_pos: [f32; 3],
}

impl App {
    async fn new(window: &winit::window::Window) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
        });

        let surface = unsafe { instance.create_surface(window).unwrap() };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: None,
                },
                None,
            )
            .await
            .unwrap();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_preferred_format(&adapter).unwrap(),
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let mjd = mjd_now();
        let sun_pos = get_sun(mjd);

        let (sphere_verts, sphere_indices) = create_sphere(24, 36);
        let num_indices = sphere_indices.len() as u32;

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere vertex buffer"),
            contents: bytemuck::cast_slice(&sphere_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere index buffer"),
            contents: bytemuck::cast_slice(&sphere_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let vertex_attributes = wgpu::vertex_attr_array![0 => Float32x3];

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vertex_attributes,
        };

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_buffer_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        });

        let depth_texture_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let (earth_uniform, earth_bind_group, sun_uniform, sun_bind_group) = {
            let aspect = size.width as f32 / size.height as f32;
            let view = look_at([2.0, 1.5, 3.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
            let proj = perspective(std::f32::consts::PI / 3.0, aspect, 0.1, 100.0);

            let sun_m = mat4_mul(translate(sun_pos.map(|x| x * 5.0)), scale(0.3));
            let earth_m = identity();

            let earth_mvp = mat4_mul(mat4_mul(proj, view), earth_m);
            let sun_mvp = mat4_mul(mat4_mul(proj, view), sun_m);

            let earth_data = SphereUniform {
                mvp: earth_mvp,
                color: [0.1, 0.3, 0.9, 1.0],
            };
            let sun_data = SphereUniform {
                mvp: sun_mvp,
                color: [1.0, 0.8, 0.1, 1.0],
            };

            let earth_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("earth uniform"),
                contents: bytemuck::bytes_of(&earth_data),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let sun_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sun uniform"),
                contents: bytemuck::bytes_of(&sun_data),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            let earth_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("earth bind group"),
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: earth_buf.as_entire_binding(),
                }],
            });
            let sun_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sun bind group"),
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: sun_buf.as_entire_binding(),
                }],
            });

            (earth_buf, earth_bg, sun_buf, sun_bg)
        };

        Self {
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            depth_texture,
            depth_texture_view,
            bind_group_layout,
            earth_uniform,
            earth_bind_group,
            sun_uniform,
            sun_bind_group,
            mjd,
            sun_pos,
        }
    }

    fn update_uniforms(&mut self) {
        self.mjd = mjd_now();
        self.sun_pos = get_sun(self.mjd);

        let aspect = self.size.width as f32 / self.size.height as f32;
        let view = look_at([2.0, 1.5, 3.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let proj = perspective(std::f32::consts::PI / 3.0, aspect, 0.1, 100.0);

        let sun_m = mat4_mul(translate(self.sun_pos.map(|x| x * 5.0)), scale(0.3));
        let earth_m = identity();

        let earth_mvp = mat4_mul(mat4_mul(proj, view), earth_m);
        let sun_mvp = mat4_mul(mat4_mul(proj, view), sun_m);

        self.queue.write_buffer(
            &self.earth_uniform,
            0,
            bytemuck::bytes_of(&SphereUniform {
                mvp: earth_mvp,
                color: [0.1, 0.3, 0.9, 1.0],
            }),
        );
        self.queue.write_buffer(
            &self.sun_uniform,
            0,
            bytemuck::bytes_of(&SphereUniform {
                mvp: sun_mvp,
                color: [1.0, 0.8, 0.1, 1.0],
            }),
        );
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);

        self.depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size: wgpu::Extent3d {
                width: new_size.width,
                height: new_size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        });
        self.depth_texture_view = self.depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
    }

    fn handle_input(&mut self, _event: &WindowEvent) -> bool {
        false
    }

    fn update(&mut self) {
        self.update_uniforms();
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            pass.set_pipeline(&self.render_pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

            // Earth
            pass.set_bind_group(0, &self.earth_bind_group, &[]);
            pass.draw_indexed(0..self.num_indices, 0, 0..1);

            // Sun
            pass.set_bind_group(0, &self.sun_bind_group, &[]);
            pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();

        Ok(())
    }
}

async fn run() {
    #[cfg(not(target_arch = "wasm32"))]
    env_logger::init();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("astronomic visualization")
        .with_inner_size(PhysicalSize::new(1024, 768))
        .build(&event_loop)
        .unwrap();

    let mut app = App::new(&window).await;

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent { ref event, window_id } if window_id == window.id() => {
                if !app.handle_input(event) {
                    match event {
                        WindowEvent::CloseRequested => {
                            *control_flow = ControlFlow::Exit;
                        }
                        WindowEvent::Resized(physical_size) => {
                            app.resize(*physical_size);
                        }
                        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                            app.resize(**new_inner_size);
                        }
                        _ => {}
                    }
                }
            }
            Event::RedrawRequested(window_id) if window_id == window.id() => {
                app.update();
                match app.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => app.resize(app.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    pollster::block_on(run());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init().expect("could not initialize logger");
    wasm_bindgen_futures::spawn_local(run());
}
