use std::{fs::create_dir, num::NonZeroU64};

use nalgebra::Vector2;
use wgpu::{
    util::DeviceExt, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BufferUsages,
    PipelineLayoutDescriptor, ShaderModuleDescriptor, ShaderStages, Surface, Texture,
    TextureUsages, TextureView, VertexBufferLayout,
};
use winit::{
    event::WindowEvent,
    window::{self, Window},
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::world::scene::Scene;

pub struct SceneData {
    pub cameraPos: [f32; 3],
    pub cameraForward: [f32; 3],
    pub cameraRight: [f32; 3],
    pub cameraUp: [f32; 3],
    pub sphereCount: f32,
}

pub struct RayTraceRender {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    window: Window,
    compute_bind_group: wgpu::BindGroup,
    screen_bind_group: wgpu::BindGroup,
    compute_pipeline: wgpu::ComputePipeline,
    screen_pipeline: wgpu::RenderPipeline,
    scene_data: wgpu::Buffer,
}

impl RayTraceRender {
    fn create_surface(instance: &wgpu::Instance, window: &Window) -> wgpu::Surface {
        #[cfg(target_arch = "wasm32")]
        {
            let document = web_sys::window().unwrap().document().unwrap();
            let canvas = document.get_element_by_id("screen").unwrap();
            let canvas: web_sys::HtmlCanvasElement = canvas
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .map_err(|_| ())
                .unwrap();
            return instance.create_surface_from_canvas(&canvas).unwrap();
        }
        return unsafe { instance.create_surface(&window) }.unwrap();
    }

    pub async fn new(window: Window) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
        });

        let surface = RayTraceRender::create_surface(&instance, &window);

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
                    limits: if cfg!(target_arch = "wasm32") {
                        // wgpu::Limits::downlevel_webgl2_defaults()
                        wgpu::Limits::default()
                    } else {
                        wgpu::Limits::default()
                    },
                    label: None,
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);

        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .filter(|f| !f.describe().srgb)
            .next()
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let kernel = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Raytrace kernel"),
            source: wgpu::ShaderSource::Wgsl(include_str!("raytrace_kernel.wgsl").into()),
        });

        let screen_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Screen shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("screen_shader.wgsl").into()),
        });

        let color_buffer = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Color Buffer"),
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: TextureUsages::COPY_DST
                | TextureUsages::STORAGE_BINDING
                | TextureUsages::TEXTURE_BINDING,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });

        let scene_data = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Scene Data"),
            size: 52,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let color_buffer_view = color_buffer.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Color Buffer View"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: None,
            border_color: None,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_buffer_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &scene_data,
                        offset: 0,
                        size: Some(NonZeroU64::new(52).unwrap()),
                    }),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Pipeline"),
            layout: Some(&pipeline_layout),
            module: &kernel,
            entry_point: "main",
        });

        let screen_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Screen bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let screen_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Screen bind group"),
            layout: &screen_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&color_buffer_view),
                },
            ],
        });

        let screen_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Screen pipeline layout"),
            bind_group_layouts: &[&screen_bind_group_layout],
            push_constant_ranges: &[],
        });

        let screen_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Screen pipeline"),
            layout: Some(&screen_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &screen_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Front),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &screen_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            compute_bind_group,
            screen_bind_group,
            compute_pipeline,
            screen_pipeline,
            scene_data,
        }
    }

    pub fn prepare_scene(&mut self, scene: &Scene) {
        let data = SceneData { cameraPos: scene., cameraForward: todo!(), cameraRight: todo!(), cameraUp: todo!(), sphereCount: todo!() }
        // self.queue.write_buffer(&self.scene_data, 0, 
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        false
    }

    pub fn update(&mut self) {}

    pub fn render(&mut self, scene: &Scene) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        
        self.prepare_scene(scene);
        
        let mut ray_trace_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Ray trace pass"),
        });
        ray_trace_pass.set_pipeline(&self.compute_pipeline);
        ray_trace_pass.set_bind_group(0, &self.compute_bind_group, &[]);
        ray_trace_pass.dispatch_workgroups(self.size.width, self.size.height, 1);

        drop(ray_trace_pass);

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });
        render_pass.set_pipeline(&self.screen_pipeline);
        render_pass.set_bind_group(0, &self.screen_bind_group, &[]);
        render_pass.draw(0..6, 0..1);
        drop(render_pass);

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
