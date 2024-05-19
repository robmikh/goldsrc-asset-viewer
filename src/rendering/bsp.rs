use std::{borrow::Cow, collections::HashSet, ops::Range};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use gsparser::{
    bsp::{BspEntity, BspReader},
    wad3::MipmapedTextureData,
};
use wgpu::util::DeviceExt;
use winit::event::VirtualKeyCode;

use crate::{
    gltf::{
        bsp::{ModelVertex, TextureInfo},
        coordinates::convert_coordinates,
        Mesh, Model,
    },
    numerics::{ToVec3, ToVec4},
};

use super::{debug::create_debug_point, Renderer};

struct GpuModel {
    index_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    meshes: Vec<Mesh>,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GpuVertex {
    pos: [f32; 4],
    normal: [f32; 4],
    uv: [f32; 2],
}

impl GpuVertex {
    fn from(vertex: &ModelVertex) -> Self {
        Self {
            pos: Vec3::from_array(vertex.pos).to_vec4().to_array(),
            normal: Vec3::from_array(vertex.normal).to_vec4().to_array(),
            uv: vertex.uv,
        }
    }
}

pub struct BspRenderer {
    map_model: GpuModel,
    map_models: Vec<GpuModel>,
    textures: Vec<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    sampler: wgpu::Sampler,

    _shader: wgpu::ShaderModule,
    config: wgpu::SurfaceConfiguration,

    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    depth_sampler: wgpu::Sampler,

    bind_group: wgpu::BindGroup,
    model_bind_group: wgpu::BindGroup,
    _bind_group_layout: wgpu::BindGroupLayout,
    _model_bind_group_layout: wgpu::BindGroupLayout,
    _texture_bind_group_layout: wgpu::BindGroupLayout,
    _pipeline_layout: wgpu::PipelineLayout,
    uniform_buffer: wgpu::Buffer,
    _model_buffer: wgpu::Buffer,
    render_pipeline: wgpu::RenderPipeline,

    camera_position: Vec3,
    facing: Vec3,

    new_debug_point: Option<Vec3>,
    debug_point: Option<GpuModel>,
}

impl BspRenderer {
    pub fn new(
        reader: &BspReader,
        loaded_model: &Model<ModelVertex>,
        loaded_map_models: &[Model<ModelVertex>],
        loaded_textures: &[TextureInfo],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
    ) -> Self {
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
        let model_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(64),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
                &model_bind_group_layout,
                &texture_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::MirrorRepeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Load textures
        let mut textures = Vec::with_capacity(loaded_textures.len());
        for texture in loaded_textures {
            let (texture, view) = create_texture_and_view(device, queue, &texture.image_data);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &texture_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
                label: None,
            });
            textures.push((texture, view, bind_group));
        }

        // Find the "info_player_start" entity
        let entities = BspEntity::parse_entities(reader.read_entities());
        let mut player_start_entity = None;
        for entity in &entities {
            if let Some(value) = entity.0.get("classname") {
                if *value == "info_player_start" {
                    player_start_entity = Some(entity);
                }
            }
        }
        let camera_start = {
            let entity = player_start_entity.unwrap();
            let value = entity.0.get("origin").unwrap();
            let mut split = value.split(" ");
            let x: f32 = split.next().unwrap().parse().unwrap();
            let y: f32 = split.next().unwrap().parse().unwrap();
            let z: f32 = split.next().unwrap().parse().unwrap();
            let coord = [x, y, z];
            let coord = convert_coordinates(coord);
            Vec3::from_array(coord)
        };
        println!("Start position: {:?}", camera_start);
        let facing = Vec3::new(1.0, 0.0, 0.0);

        // Create other resources
        let mx_total = generate_matrix(
            config.width as f32 / config.height as f32,
            camera_start,
            facing,
        );
        let mx_ref: &[f32; 16] = mx_total.as_ref();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Globals Uniform Buffer"),
            contents: bytemuck::cast_slice(mx_ref),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let mx_ref: &[f32; 16] = Mat4::IDENTITY.as_ref();
        let model_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Model Uniform Buffer"),
            contents: bytemuck::cast_slice(mx_ref),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: None,
        });
        let model_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &model_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: model_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
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
            ],
        }];

        let map_model = create_gpu_model_for_model(loaded_model, device);
        let map_models = loaded_map_models
            .iter()
            .map(|x| create_gpu_model_for_model(x, device))
            .collect();

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &vertex_buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(config.format.into())],
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
            map_model,
            map_models,
            textures,
            sampler,

            _shader: shader,
            config,

            depth_texture,
            depth_view,
            depth_sampler,

            bind_group,
            model_bind_group,
            _bind_group_layout: bind_group_layout,
            _model_bind_group_layout: model_bind_group_layout,
            _texture_bind_group_layout: texture_bind_group_layout,
            _pipeline_layout: pipeline_layout,
            uniform_buffer,
            _model_buffer: model_buffer,
            render_pipeline,

            camera_position: camera_start,
            facing,

            new_debug_point: None,
            debug_point: None,
        }
    }

    fn render_model<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, model: &'a GpuModel) {
        render_pass.set_index_buffer(model.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.set_vertex_buffer(0, model.vertex_buffer.slice(..));

        for mesh in &model.meshes {
            let texture = mesh.texture_index;
            let (_, _, bind_group) = &self.textures[texture];
            render_pass.set_bind_group(2, bind_group, &[]);
            render_pass.draw_indexed(
                mesh.indices_range.start as u32..mesh.indices_range.end as u32,
                0,
                0..1,
            );
        }
    }
}

impl Renderer for BspRenderer {
    fn render(
        &self,
        clear_color: wgpu::Color,
        view: &wgpu::TextureView,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: true,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });
            render_pass.push_debug_group("Prepare frame render pass.");
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);

            render_pass.insert_debug_marker("Draw!");
            render_pass.set_bind_group(1, &self.model_bind_group, &[]);
            self.render_model(&mut render_pass, &self.map_model);

            for model in &self.map_models {
                self.render_model(&mut render_pass, model);
            }

            if let Some(model) = self.debug_point.as_ref() {
                self.render_model(&mut render_pass, model);
            }

            render_pass.pop_debug_group();
        }

        queue.submit(Some(encoder.finish()));
    }

    fn resize(
        &mut self,
        config: &wgpu::SurfaceConfiguration,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let (depth_texture, depth_view, depth_sampler) = create_depth_texture(device, config);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
        self.depth_sampler = depth_sampler;
        self.config = config.clone();
    }

    fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        delta: std::time::Duration,
        down_keys: &HashSet<VirtualKeyCode>,
    ) {
        let mut dirty = false;

        if down_keys.contains(&VirtualKeyCode::Q) {
            let transform = Mat4::from_rotation_y(5.0_f32.to_radians());
            let new_facing = transform * self.facing.to_vec4();
            self.facing = new_facing.to_vec3().normalize();
            dirty = true;
        } else if down_keys.contains(&VirtualKeyCode::E) {
            let transform = Mat4::from_rotation_y(-5.0_f32.to_radians());
            let new_facing = transform * self.facing.to_vec4();
            self.facing = new_facing.to_vec3().normalize();
            dirty = true;
        }

        if down_keys.contains(&VirtualKeyCode::W) {
            let delta_position = 5.0 * self.facing;
            self.camera_position += delta_position;
            dirty = true;
        } else if down_keys.contains(&VirtualKeyCode::S) {
            let delta_position = -5.0 * self.facing;
            self.camera_position += delta_position;
            dirty = true;
        }

        if down_keys.contains(&VirtualKeyCode::A) {
            let delta_position = -5.0 * self.facing.cross(Vec3::new(0.0, 1.0, 0.0));
            self.camera_position += delta_position;
            dirty = true;
        } else if down_keys.contains(&VirtualKeyCode::D) {
            let delta_position = 5.0 * self.facing.cross(Vec3::new(0.0, 1.0, 0.0));
            self.camera_position += delta_position;
            dirty = true;
        }

        if dirty {
            let mx_total = generate_matrix(
                self.config.width as f32 / self.config.height as f32,
                self.camera_position,
                self.facing,
            );
            let mx_ref: &[f32; 16] = mx_total.as_ref();
            queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(mx_ref));
        }

        if let Some(new_debug_point) = self.new_debug_point.take() {
            let mut indices = Vec::new();
            let mut vertices = Vec::new();
            let range = create_debug_point(new_debug_point, &mut indices, &mut vertices);

            let vertices: Vec<GpuVertex> = vertices.iter().map(|x| GpuVertex::from(x)).collect();
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
            let meshes = vec![Mesh {
                indices_range: range,
                texture_index: 0,
            }];
            let model = GpuModel {
                index_buffer,
                vertex_buffer,
                meshes,
            };

            self.debug_point = Some(model);
        }
    }

    fn world_pos_and_ray_from_screen_pos(&self, mut pos: Vec2) -> (Vec3, Vec3) {
        let (projection, view) = compute_projection_and_view_transforms(
            self.config.width as f32 / self.config.height as f32,
            self.camera_position,
            self.facing,
        );

        let target_size = Vec2::new(self.config.width as f32, self.config.height as f32);
        pos.y = target_size.y - pos.y;
        let ndc = pos * 2.0 / target_size - Vec2::ONE;

        let ndc_to_world = (projection * view).inverse();
        let world_near_plane = ndc_to_world.project_point3(ndc.extend(-1.0));
        let world_far_plane = ndc_to_world.project_point3(ndc.extend(f32::EPSILON));

        let direction = world_far_plane - world_near_plane;
        let length = direction.length();
        let direction = (length.is_finite() && length > 0.0).then_some(direction / length);
        let direction = direction.unwrap();

        (world_near_plane, direction.normalize())
    }

    fn get_position_and_direction(&self) -> (Vec3, Vec3) {
        (self.camera_position, self.facing)
    }

    fn set_debug_point(&mut self, point: Vec3) {
        self.new_debug_point = Some(point);
    }
}

fn create_texture_and_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image_data: &MipmapedTextureData,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture_extent = wgpu::Extent3d {
        width: image_data.image_width,
        height: image_data.image_height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: texture_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    queue.write_texture(
        texture.as_image_copy(),
        &image_data.image,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(image_data.image_width * 4),
            rows_per_image: None,
        },
        texture_extent,
    );
    (texture, texture_view)
}

fn compute_projection_and_view_transforms(
    aspect_ratio: f32,
    camera_start: Vec3,
    facing: Vec3,
) -> (Mat4, Mat4) {
    let mx_projection = Mat4::perspective_rh(45.0_f32.to_radians(), aspect_ratio, 1.0, 10000.0);
    let mx_view = Mat4::look_to_rh(camera_start, facing, Vec3::new(0.0, 1.0, 0.0));
    (mx_projection, mx_view)
}

fn generate_matrix(aspect_ratio: f32, camera_start: Vec3, facing: Vec3) -> Mat4 {
    let mx_projection = Mat4::perspective_rh(45.0_f32.to_radians(), aspect_ratio, 1.0, 10000.0);
    let mx_view = Mat4::look_to_rh(camera_start, facing, Vec3::new(0.0, 1.0, 0.0));
    //let mx_view = Mat4::look_at_rh(
    //    Vec3::new(1305.5, -333.5, 779.5),
    //    camera_start,
    //    Vec3::new(0.0, 1.0, 0.0)
    //);
    mx_projection * mx_view
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
        address_mode_u: wgpu::AddressMode::MirrorRepeat,
        address_mode_v: wgpu::AddressMode::MirrorRepeat,
        address_mode_w: wgpu::AddressMode::MirrorRepeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (texture, view, sampler)
}

fn create_gpu_model_for_model(model: &Model<ModelVertex>, device: &wgpu::Device) -> GpuModel {
    let vertices: Vec<GpuVertex> = model.vertices.iter().map(|x| GpuVertex::from(x)).collect();
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertex Buffer"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(&model.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    let meshes = model.meshes.clone();
    GpuModel {
        index_buffer,
        vertex_buffer,
        meshes,
    }
}
