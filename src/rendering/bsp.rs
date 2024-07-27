use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};
use gltf::{Mesh, Model};
use gsparser::bsp::{BspEntity, BspReader};
use wgpu::util::DeviceExt;
use winit::keyboard::KeyCode;

use crate::{
    export::{
        bsp::{decode_atlas, ModelVertex, TextureInfo},
        coordinates::convert_coordinates,
    },
    hittest::hittest_clip_node,
    rendering::movement::MovingEntity,
    FileInfo,
};

use super::{
    camera::Camera,
    debug::{create_debug_point, create_debug_pyramid},
    Renderer,
};

struct GpuModel {
    index_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    _model_buffer: wgpu::Buffer,
    model_bind_group: wgpu::BindGroup,
    meshes: Vec<Mesh>,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GpuVertex {
    pos: [f32; 4],
    normal: [f32; 4],
    uv: [f32; 2],
    lightmap_uv: [f32; 2],
}

impl GpuVertex {
    fn from(vertex: &ModelVertex) -> Self {
        Self {
            pos: Vec3::from_array(vertex.pos).extend(1.0).to_array(),
            normal: Vec3::from_array(vertex.normal).extend(0.0).to_array(),
            uv: vertex.uv,
            lightmap_uv: vertex.lightmap_uv,
        }
    }
}

#[repr(i32)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DrawMode {
    Texture = 0,
    Lightmap = 1,
}

impl DrawMode {
    pub fn from_number(number: i32) -> Option<Self> {
        match number {
            0 => Some(Self::Texture),
            1 => Some(Self::Lightmap),
            _ => None,
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            Self::Texture => Self::Lightmap,
            Self::Lightmap => Self::Texture,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct DrawParams {
    draw_mode: i32,
}

pub struct BspRenderer {
    models_to_render: Vec<usize>,
    map_models: Vec<GpuModel>,
    textures: Vec<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    sampler: wgpu::Sampler,

    _shader: wgpu::ShaderModule,
    config: wgpu::SurfaceConfiguration,

    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    depth_sampler: wgpu::Sampler,

    _bind_group_layout: wgpu::BindGroupLayout,
    _draw_params_bind_group_layout: wgpu::BindGroupLayout,
    model_bind_group_layout: wgpu::BindGroupLayout,
    _texture_bind_group_layout: wgpu::BindGroupLayout,
    _pipeline_layout: wgpu::PipelineLayout,
    render_pipeline: wgpu::RenderPipeline,

    lightmap_texture: wgpu::Texture,
    lightmap_view: wgpu::TextureView,

    draw_mode: DrawMode,
    draw_mode_update: Option<DrawMode>,
    draw_params_buffer: wgpu::Buffer,
    draw_params_bind_group: wgpu::BindGroup,

    camera: Camera,
    player: MovingEntity,
    gravity: bool,

    new_debug_point: Option<Vec3>,
    debug_point: Option<GpuModel>,
    new_debug_pyramid_location: Option<(Vec3, Vec3)>,
    debug_pyramid: Option<GpuModel>,
}

impl BspRenderer {
    pub fn new(
        reader: &BspReader,
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
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            anisotropy_clamp: 1,
            ..Default::default()
        });

        // Load textures
        let mut textures = Vec::with_capacity(loaded_textures.len());
        for texture in loaded_textures {
            let (texture, view) = create_texture_and_view(device, queue, &texture.image_data.image);
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
        // Debug texture
        {
            let mut image = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::new(10, 10);
            for pixel in image.pixels_mut() {
                *pixel = image::Rgba::<u8>([255, 0, 0, 128]);
            }

            let (texture, view) = create_texture_and_view(device, queue, &image);
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

        // Create lightmap atlas
        let lightmap_atlas = decode_atlas(reader);
        let (lightmap_texture, lightmap_view) = {
            let mut image = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::new(
                lightmap_atlas.width(),
                lightmap_atlas.height(),
            );
            for (pixel, source_pixel) in image
                .pixels_mut()
                .zip(lightmap_atlas.data().chunks_exact(3))
            {
                *pixel =
                    image::Rgba::<u8>([source_pixel[0], source_pixel[1], source_pixel[2], 255]);
            }

            //image.save("testoutput/temp.png").unwrap();

            let (texture, view) = create_texture_and_view(device, queue, &image);
            (texture, view)
        };

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
        let player = MovingEntity::new(camera_start);

        // Create camera
        let camera = Camera::new(
            camera_start,
            Vec2::new(config.width as f32, config.height as f32),
            &bind_group_layout,
            &device,
        );

        // Initialize draw params
        let draw_mode = DrawMode::Texture;
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

        // Create a map of models to entities
        // TODO: Can we assume 1:1 (minus models not tied to any entities)?
        let mut model_to_entity = HashMap::new();
        for (entity_index, entity) in entities.iter().enumerate() {
            if let Some(model_value) = entity.0.get("model") {
                if model_value.starts_with('*') {
                    let model_index: usize = model_value.trim_start_matches('*').parse().unwrap();
                    let old = model_to_entity.insert(model_index, entity_index);
                    assert!(old.is_none());
                }
            }
        }

        let map_models: Vec<GpuModel> = loaded_map_models
            .iter()
            .enumerate()
            .map(|(i, model)| -> GpuModel {
                let origin = {
                    let mut origin = Vec3::ZERO;

                    if let Some(entity_index) = model_to_entity.get(&i) {
                        let entity = &entities[*entity_index];
                        if let Some(origin_str) = entity.0.get("origin") {
                            let mut parts = origin_str.split_whitespace();
                            let hl_x: isize = parts.next().unwrap().parse().unwrap();
                            let hl_y: isize = parts.next().unwrap().parse().unwrap();
                            let hl_z: isize = parts.next().unwrap().parse().unwrap();

                            let coord = convert_coordinates([hl_x, hl_y, hl_z]);
                            origin = Vec3::new(coord[0] as f32, coord[1] as f32, coord[2] as f32);
                        }
                    }

                    origin
                };

                create_gpu_model_for_model(
                    model,
                    origin,
                    device,
                    &model_bind_group_layout,
                    &sampler,
                    &lightmap_view,
                )
            })
            .collect();

        // Record which models to hide
        // TODO: More robust logic
        let mut models_to_render: Vec<usize> = (0..map_models.len()).collect();
        for entity in &entities {
            if let Some(class_name) = entity.0.get("classname") {
                if class_name.starts_with("trigger") || class_name.starts_with("func_ladder") {
                    if let Some(model_value) = entity.0.get("model") {
                        if model_value.starts_with('*') {
                            let model_index: usize =
                                model_value.trim_start_matches('*').parse().unwrap();
                            if let Some(position) =
                                models_to_render.iter().position(|x| *x == model_index)
                            {
                                models_to_render.remove(position);
                            }
                        }
                    }
                }
            }
        }

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
            models_to_render,
            map_models,
            textures,
            sampler,

            _shader: shader,
            config,

            depth_texture,
            depth_view,
            depth_sampler,

            _bind_group_layout: bind_group_layout,
            _draw_params_bind_group_layout: draw_params_bind_group_layout,
            model_bind_group_layout,
            _texture_bind_group_layout: texture_bind_group_layout,
            _pipeline_layout: pipeline_layout,
            render_pipeline,

            lightmap_texture,
            lightmap_view,

            draw_mode,
            draw_mode_update: None,
            draw_params_buffer,
            draw_params_bind_group,

            camera,
            player,
            gravity: true,

            new_debug_point: None,
            debug_point: None,
            new_debug_pyramid_location: None,
            debug_pyramid: None,
        }
    }

    fn render_model<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, model: &'a GpuModel) {
        render_pass.set_index_buffer(model.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.set_vertex_buffer(0, model.vertex_buffer.slice(..));

        for mesh in &model.meshes {
            let texture = mesh.texture_index;
            let (_, _, bind_group) = &self.textures[texture];
            render_pass.set_bind_group(3, bind_group, &[]);
            render_pass.draw_indexed(
                mesh.indices_range.start as u32..mesh.indices_range.end as u32,
                0,
                0..1,
            );
        }
    }

    fn process_movement(
        &self,
        reader: &BspReader,
        start_position: Vec3,
        end_position: Vec3,
        mut velocity: Vec3,
        project_collision: bool,
    ) -> (Vec3, Vec3, bool) {
        let mut position = end_position;
        let clip_node_index = reader.read_models()[0].head_nodes[1] as usize;

        let mut distance = start_position.distance(end_position);
        let full_distance = distance;
        let mut start_position = start_position;
        let mut end_position = end_position;
        let mut collisions = 0;
        while distance > 0.0 {
            if end_position.is_nan() {
                position = start_position;
                velocity = Vec3::ZERO;
                break;
            }

            if let Some(intersection) =
                hittest_clip_node(reader, clip_node_index, start_position, end_position)
            {
                collisions += 1;
                if collisions > 4 {
                    position = intersection.position;
                    break;
                }

                let direction = velocity.normalize();
                let dot = direction.dot(intersection.normal);
                if !project_collision || dot == -1.0 || intersection.normal.length() == 0.0 {
                    velocity = Vec3::ZERO;
                    position = start_position;
                    break;
                } else {
                    // Calc our new position
                    let v1 = direction.cross(intersection.normal).normalize();
                    let surface_dir = -v1.cross(intersection.normal).normalize();

                    let dist = start_position.distance(intersection.position);
                    distance -= dist;

                    if distance <= 0.0 || (dist <= 0.0 && distance / full_distance != 1.0) {
                        position = intersection.position;
                        break;
                    }

                    let new_vector = surface_dir * distance;
                    let new_velocity = velocity.length() * surface_dir;
                    velocity = new_velocity;

                    start_position = intersection.position;
                    end_position = intersection.position + new_vector;
                    position = end_position;
                }
            } else {
                break;
            }
        }
        (position, velocity, collisions > 0)
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

            render_pass.insert_debug_marker("Draw!");
            for model_index in &self.models_to_render {
                let model = &self.map_models[*model_index];
                render_pass.set_bind_group(2, &model.model_bind_group, &[]);
                self.render_model(&mut render_pass, model);
            }

            if let Some(model) = self.debug_point.as_ref() {
                self.render_model(&mut render_pass, model);
            }

            if let Some(model) = self.debug_pyramid.as_ref() {
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

    fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        delta: std::time::Duration,
        down_keys: &HashSet<KeyCode>,
        mouse_delta: Option<Vec2>,
        file_info: &Option<FileInfo>,
        noclip: bool,
    ) {
        let mut rotation = self.camera.yaw_pitch_roll();
        let old_rotation = rotation;

        if down_keys.contains(&KeyCode::KeyQ) {
            rotation.x += 5.0_f32.to_radians();
        } else if down_keys.contains(&KeyCode::KeyE) {
            rotation.x -= 5.0_f32.to_radians();
        }

        if down_keys.contains(&KeyCode::KeyF) {
            rotation.y += 5.0_f32.to_radians();
        } else if down_keys.contains(&KeyCode::KeyR) {
            rotation.y -= 5.0_f32.to_radians();
        }

        if let Some(mouse_delta) = mouse_delta {
            let sensitivity = 0.3;
            rotation.x -= mouse_delta.x.to_radians() * sensitivity;
            rotation.y += mouse_delta.y.to_radians() * sensitivity;
        }

        if rotation != old_rotation {
            self.camera.set_yaw_pitch_roll(rotation);
        }

        let mut direction = Vec3::ZERO;
        let facing = self.camera.facing();
        let up = self.camera.up();
        if down_keys.contains(&KeyCode::KeyW) {
            let delta_position = facing;
            direction += delta_position;
        } else if down_keys.contains(&KeyCode::KeyS) {
            let delta_position = -facing;
            direction += delta_position;
        }

        if down_keys.contains(&KeyCode::KeyA) {
            let delta_position = -facing.cross(up);
            direction += delta_position;
        } else if down_keys.contains(&KeyCode::KeyD) {
            let delta_position = facing.cross(up);
            direction += delta_position;
        }

        let wish_dir = if direction != Vec3::ZERO {
            direction = direction.normalize();

            let mut wish_dir = direction;
            wish_dir.y = 0.0;
            wish_dir.normalize()
        } else {
            Vec3::ZERO
        };

        // TODO: On release
        if down_keys.contains(&KeyCode::Space) && self.player.is_on_ground() {
            // https://www.jwchong.com/hl/duckjump.html#jumping
            let jump_impulse = (2.0f32 * 800.0 * 45.0).sqrt();
            self.player
                .set_velocity_from_gravity(Vec3::new(0.0, jump_impulse, 0.0));
            self.player.set_is_on_ground(false);
        }

        {
            // On c1a0, we start at -166 and fall to -179.96875 without adjustments
            const CROUCH_HEIGHT: Vec3 = Vec3::new(0.0, 13.97, 0.0);
            const AUTO_STEP_HEIGHT: f32 = 4.0;

            self.player.update_velocity_ground(wish_dir, delta);
            let mut velocity = self.player.velocity();
            let start_position = self.player.position() - CROUCH_HEIGHT;

            let mut position = start_position;

            if !noclip {
                let reader = match file_info.as_ref().unwrap() {
                    FileInfo::BspFile(file) => &file.reader,
                    _ => panic!(),
                };
                if velocity.length() > 0.0 {
                    let direction = velocity.normalize();
                    let clip_node_index = reader.read_models()[0].head_nodes[1] as usize;

                    let is_touching_ground = hittest_clip_node(
                        reader,
                        clip_node_index,
                        start_position,
                        start_position - Vec3::new(0.0, 1.0, 0.0),
                    )
                    .is_some();

                    // Project our movement along the surface we're standing on
                    let surface_normal = if let Some(intersection) = hittest_clip_node(
                        reader,
                        clip_node_index,
                        start_position,
                        start_position - CROUCH_HEIGHT * 2.0,
                    ) {
                        if intersection.normal != Vec3::new(0.0, 1.0, 0.0) {
                            let new_direction = (direction
                                - intersection.normal * direction.dot(intersection.normal))
                            .normalize();
                            velocity = new_direction * velocity.length();
                        }
                        intersection.normal
                    } else {
                        Vec3::new(0.0, 1.0, 0.0)
                    };

                    let previous_velocity = velocity;
                    let end_position = start_position + (velocity * delta.as_secs_f32());
                    let (new_position, new_velocity, colided) =
                        self.process_movement(reader, start_position, end_position, velocity, true);
                    position = new_position;
                    velocity = Vec3::new(new_velocity.x, velocity.y, new_velocity.z);

                    // If we've collided with something, check to see if we can move without a collision
                    // if we were a bit higher. This is an attempt to allow movement over small bumps.
                    if colided {
                        let nudged_start_position =
                            start_position + (surface_normal * AUTO_STEP_HEIGHT);
                        let nudged_end_position =
                            nudged_start_position + (previous_velocity * delta.as_secs_f32());
                        if hittest_clip_node(
                            reader,
                            clip_node_index,
                            nudged_start_position,
                            nudged_end_position,
                        )
                        .is_none()
                        {
                            position = nudged_end_position;
                            velocity = previous_velocity;

                            if is_touching_ground {
                                if let Some(intersection) = hittest_clip_node(
                                    reader,
                                    clip_node_index,
                                    nudged_end_position,
                                    nudged_end_position
                                        - Vec3::new(0.0, AUTO_STEP_HEIGHT * 2.0, 0.0),
                                ) {
                                    position = intersection.position;
                                }
                            }
                        }
                    }
                }

                if self.gravity {
                    // Apply gravity
                    let gravity_velocity = self.player.velocity_from_gravity()
                        + (Vec3::new(0.0, -800.0, 0.0) * delta.as_secs_f32());
                    let start_position = position;
                    let end_position = start_position + (gravity_velocity * delta.as_secs_f32());

                    let (new_position, new_velocity, collision) = self.process_movement(
                        reader,
                        start_position,
                        end_position,
                        gravity_velocity,
                        false,
                    );
                    position = new_position;
                    self.player.set_velocity_from_gravity(new_velocity);
                    if collision {
                        self.player.set_is_on_ground(true);
                    }
                }
            }

            position = position + CROUCH_HEIGHT;

            self.camera.set_position(position);
            self.player.set_velocity(velocity);
            self.player.set_position(position);
        }

        self.camera.update(queue);

        if let Some(draw_mode) = self.draw_mode_update.take() {
            self.draw_mode = draw_mode;
            let draw_params = DrawParams {
                draw_mode: draw_mode as i32,
            };
            queue.write_buffer(
                &self.draw_params_buffer,
                0,
                bytemuck::cast_slice(&[draw_params]),
            );
        }

        if let Some(new_debug_point) = self.new_debug_point.take() {
            let model = create_debug_point_model(new_debug_point, self.textures.len() - 1);
            let gpu_model = create_gpu_model_for_model(
                &model,
                Vec3::ZERO,
                device,
                &self.model_bind_group_layout,
                &self.sampler,
                &self.lightmap_view,
            );
            self.debug_point = Some(gpu_model);
        }

        if let Some((pos, dir)) = self.new_debug_pyramid_location.take() {
            let model = create_debug_pyramid_model(pos, dir, self.textures.len() - 1);
            let gpu_model = create_gpu_model_for_model(
                &model,
                Vec3::ZERO,
                device,
                &self.model_bind_group_layout,
                &self.sampler,
                &self.lightmap_view,
            );
            self.debug_pyramid = Some(gpu_model);
        }
    }

    fn world_pos_and_ray_from_screen_pos(&self, pos: Vec2) -> (Vec3, Vec3) {
        self.camera.world_pos_and_ray_from_screen_pos(pos)
    }

    fn get_position_and_direction(&self) -> (Vec3, Vec3) {
        (self.camera.position(), self.camera.facing())
    }

    fn set_debug_point(&mut self, point: Vec3) {
        self.new_debug_point = Some(point);
    }

    fn set_debug_pyramid(&mut self, point: Vec3, dir: Vec3) {
        self.new_debug_pyramid_location = Some((point, dir));
    }

    fn set_gravity(&mut self, gravity: bool) {
        self.gravity = gravity;
    }

    fn set_draw_mode(&mut self, draw_mode: DrawMode) {
        if self.draw_mode != draw_mode {
            self.draw_mode_update = Some(draw_mode);
        }
    }

    fn get_draw_mode(&self) -> DrawMode {
        self.draw_mode
    }
}

fn create_texture_and_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image_data: &image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture_extent = wgpu::Extent3d {
        width: image_data.width(),
        height: image_data.height(),
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
        &image_data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(image_data.width() * 4),
            rows_per_image: None,
        },
        texture_extent,
    );
    (texture, texture_view)
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

fn create_gpu_model_for_model(
    model: &Model<ModelVertex>,
    origin: Vec3,
    device: &wgpu::Device,
    model_bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    lightmap_view: &wgpu::TextureView,
) -> GpuModel {
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

    let transform = Mat4::from_translation(origin);
    let mx_ref: &[f32; 16] = transform.as_ref();
    let model_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Model Uniform Buffer"),
        contents: bytemuck::cast_slice(mx_ref),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(lightmap_view),
            },
        ],
        label: None,
    });

    GpuModel {
        index_buffer,
        vertex_buffer,
        _model_buffer: model_buffer,
        model_bind_group,
        meshes,
    }
}

fn create_debug_point_model(point: Vec3, texture_index: usize) -> Model<ModelVertex> {
    let mut indices = Vec::new();
    let mut vertices = Vec::new();
    let indices_range = create_debug_point(point, &mut indices, &mut vertices);
    Model {
        indices,
        vertices,
        meshes: vec![Mesh {
            texture_index,
            indices_range,
        }],
    }
}

fn create_debug_pyramid_model(point: Vec3, dir: Vec3, texture_index: usize) -> Model<ModelVertex> {
    let mut indices = Vec::new();
    let mut vertices = Vec::new();
    let indices_range = create_debug_pyramid(point, dir, &mut indices, &mut vertices);
    Model {
        indices,
        vertices,
        meshes: vec![Mesh {
            texture_index,
            indices_range,
        }],
    }
}
