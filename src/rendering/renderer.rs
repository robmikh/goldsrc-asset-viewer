use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};
use wgpu::util::DeviceExt;

use super::{bsp::DrawMode, camera::Camera};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuVertex {
    pub pos: [f32; 4],
    pub normal: [f32; 4],
    pub uv: [f32; 2],
    pub lightmap_uv: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct DrawParams {
    pub draw_mode: i32,
}

#[repr(C, align(16))]
#[derive(Copy, Clone)]
pub struct ModelBuffer {
    pub transform: [f32; 16],
    pub alpha: f32,
}

pub struct Renderer {
    sampler: wgpu::Sampler,

    _shader: wgpu::ShaderModule,
    config: wgpu::SurfaceConfiguration,

    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    depth_sampler: wgpu::Sampler,

    bind_group_layout: wgpu::BindGroupLayout,
    _draw_params_bind_group_layout: wgpu::BindGroupLayout,
    model_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    _pipeline_layout: wgpu::PipelineLayout,
    render_pipeline: wgpu::RenderPipeline,

    draw_params_buffer: wgpu::Buffer,
    draw_params_bind_group: wgpu::BindGroup,

    camera: Camera,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, config: wgpu::SurfaceConfiguration) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "../../data/shaders/shader.wgsl"
            ))),
        });

        // Depth texture
        let (depth_texture, depth_view, depth_sampler) = create_depth_texture(&device, &config);

        // Create pipeline layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            }],
        });
        let draw_params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<DrawParams>() as u64
                        ),
                    },
                    count: None,
                }],
            });
        let model_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(
                                std::mem::size_of::<ModelBuffer>() as u64,
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                }],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                &bind_group_layout,
                &draw_params_bind_group_layout,
                &model_bind_group_layout,
                &texture_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Create camera
        let camera = Camera::new(
            Vec3::ZERO,
            Vec2::new(config.width as f32, config.height as f32),
            &bind_group_layout,
            &device,
        );

        // Initialize draw params
        let draw_mode = DrawMode::LitTexture;
        let draw_params = DrawParams {
            draw_mode: draw_mode as i32,
        };
        let draw_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Draw Params Uniform Buffer"),
            contents: bytemuck::cast_slice(&[draw_params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let draw_params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &draw_params_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: draw_params_buffer.as_entire_binding(),
            }],
            label: None,
        });

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 4 * 4,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 8 * 4,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 10 * 4,
                    shader_location: 3,
                },
            ],
        }];

        let mut target: wgpu::ColorTargetState = config.format.into();
        target.blend = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &vertex_buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(target)],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            sampler,

            _shader: shader,
            config,

            depth_texture,
            depth_view,
            depth_sampler,

            bind_group_layout: bind_group_layout,
            _draw_params_bind_group_layout: draw_params_bind_group_layout,
            model_bind_group_layout,
            texture_bind_group_layout: texture_bind_group_layout,
            _pipeline_layout: pipeline_layout,
            render_pipeline,

            draw_params_buffer,
            draw_params_bind_group,

            camera,
        }
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn model_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.model_bind_group_layout
    }

    pub fn texture_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bind_group_layout
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    pub fn render<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
        clear_color: wgpu::Color,
        view: &'a wgpu::TextureView,
    ) -> RenderPass<'a> {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        render_pass.push_debug_group("Prepare frame render pass.");
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, self.camera.bind_group(), &[]);
        render_pass.set_bind_group(1, &self.draw_params_bind_group, &[]);

        RenderPass::new(render_pass)
    }

    pub fn resize(
        &mut self,
        config: &wgpu::SurfaceConfiguration,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) {
        let (depth_texture, depth_view, depth_sampler) = create_depth_texture(device, config);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
        self.depth_sampler = depth_sampler;
        self.config = config.clone();
        self.camera
            .on_resize(Vec2::new(config.width as f32, config.height as f32));
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    pub fn update_draw_params(&self, queue: &wgpu::Queue, draw_params: DrawParams) {
        queue.write_buffer(
            &self.draw_params_buffer,
            0,
            bytemuck::cast_slice(&[draw_params]),
        );
    }
}

pub struct RenderPass<'a> {
    pub render_pass: wgpu::RenderPass<'a>,
}

impl<'a> RenderPass<'a> {
    fn new(render_pass: wgpu::RenderPass<'a>) -> Self {
        Self { render_pass }
    }
}

fn create_depth_texture(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler) {
    let size = wgpu::Extent3d {
        width: config.width,
        height: config.height,
        depth_or_array_layers: 1,
    };
    let desc = wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[wgpu::TextureFormat::Depth32Float],
    };
    let texture = device.create_texture(&desc);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (texture, view, sampler)
}
