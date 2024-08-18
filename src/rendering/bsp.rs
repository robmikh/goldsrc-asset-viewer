use std::collections::{HashMap, HashSet};

use glam::{Mat4, Vec2, Vec3};
use gltf::{Mesh, Model};
use gsparser::bsp::{BspEntity, BspReader};
use wgpu::util::DeviceExt;
use winit::keyboard::KeyCode;

use crate::{
    basic_enum,
    bsp_viewer::BspViewer,
    export::{
        bsp::{decode_atlas, ModelVertex, TextureInfo},
        coordinates::convert_coordinates,
    },
    hittest::hittest_clip_node,
    rendering::movement::MovingEntity,
    FileInfo,
};

use super::{
    debug::{create_debug_point, create_debug_pyramid},
    renderer::{DrawParams, GpuVertex, ModelBuffer},
    Renderer,
};

struct GpuModel {
    index_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    _model_buffer: wgpu::Buffer,
    model_bind_group: wgpu::BindGroup,
    meshes: Vec<Mesh>,
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
    LitTexture = 2,
}

impl DrawMode {
    pub fn from_number(number: i32) -> Option<Self> {
        match number {
            0 => Some(Self::Texture),
            1 => Some(Self::Lightmap),
            2 => Some(Self::LitTexture),
            _ => None,
        }
    }

    pub fn cycle(&self) -> Self {
        match self {
            Self::Texture => Self::Lightmap,
            Self::Lightmap => Self::LitTexture,
            Self::LitTexture => Self::Texture,
        }
    }
}

basic_enum! {
    RenderMode: i32 {
        Normal = 0,
        Color = 1,
        Texture = 2,
        Glow = 3,
        TransparentAlpha = 4,
        TransparentAdd = 5,
    }
}

struct MapData {
    models_to_render: Vec<usize>,
    transparent_models: HashSet<usize>,
    map_models: Vec<GpuModel>,
    textures: Vec<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,

    _lightmap_texture: wgpu::Texture,
    lightmap_view: wgpu::TextureView,
}

impl MapData {
    pub fn new(
        renderer: &super::renderer::Renderer,
        reader: &BspReader,
        loaded_map_models: &[Model<ModelVertex>],
        loaded_textures: &[TextureInfo],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Self {
        let model_bind_group_layout = renderer.model_bind_group_layout();
        let texture_bind_group_layout = renderer.texture_bind_group_layout();
        let sampler = renderer.sampler();

        // Load textures
        let mut textures = Vec::with_capacity(loaded_textures.len());
        for texture in loaded_textures {
            let (texture, view) = create_texture_and_view(device, queue, &texture.image_data.image);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: texture_bind_group_layout,
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
                layout: texture_bind_group_layout,
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

            //let image = image::imageops::resize(&image, lightmap_atlas.width() * 2, lightmap_atlas.height() * 2, image::imageops::FilterType::Nearest);
            //image.save("testoutput/tempScaled.png").unwrap();

            let (texture, view) = create_texture_and_view(device, queue, &image);
            (texture, view)
        };

        let entities = BspEntity::parse_entities(reader.read_entities());

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
                let (origin, alpha) = {
                    let mut origin = Vec3::ZERO;
                    let mut alpha = 1.0;

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
                        if let Some(render_mode) = entity.0.get("rendermode") {
                            if let Ok(render_mode) = render_mode.parse::<RenderMode>() {
                                if render_mode != RenderMode::Normal {
                                    if let Some(render_amt) = entity.0.get("renderamt") {
                                        if let Ok(render_amt) = render_amt.parse::<i32>() {
                                            if render_amt != 0 && render_amt != 255 {
                                                alpha = render_amt as f32 / 255.0;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    (origin, alpha)
                };

                create_gpu_model_for_model(
                    model,
                    origin,
                    alpha,
                    device,
                    model_bind_group_layout,
                    sampler,
                    &lightmap_view,
                )
            })
            .collect();

        // Record which models to hide
        // TODO: More robust logic
        let mut models_to_render: Vec<usize> = (0..map_models.len()).collect();
        let mut transparent_models = HashSet::new();
        for entity in &entities {
            if let Some(model_value) = entity.0.get("model") {
                if model_value.starts_with('*') {
                    let model_index: usize = model_value.trim_start_matches('*').parse().unwrap();

                    if let Some(render_mode) = entity.0.get("rendermode") {
                        if let Ok(render_mode) = render_mode.parse::<RenderMode>() {
                            if render_mode != RenderMode::Normal {
                                if let Some(render_amt) = entity.0.get("renderamt") {
                                    if let Ok(render_amt) = render_amt.parse::<i32>() {
                                        if render_amt != 0 && render_amt != 255 {
                                            transparent_models.insert(model_index);
                                            if let Some(position) = models_to_render
                                                .iter()
                                                .position(|x| *x == model_index)
                                            {
                                                models_to_render.remove(position);
                                                continue;
                                            }
                                        } else if render_amt == 0 {
                                            if let Some(position) = models_to_render
                                                .iter()
                                                .position(|x| *x == model_index)
                                            {
                                                models_to_render.remove(position);
                                                continue;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(class_name) = entity.0.get("classname") {
                        if class_name.starts_with("trigger")
                            || class_name.starts_with("func_ladder")
                        {
                            if let Some(position) =
                                models_to_render.iter().position(|x| *x == model_index)
                            {
                                models_to_render.remove(position);
                                continue;
                            }
                        }
                    }
                }
            }
        }

        Self {
            models_to_render,
            transparent_models,
            map_models,
            textures,

            _lightmap_texture: lightmap_texture,
            lightmap_view,
        }
    }

    fn get_start_position(&self, reader: &BspReader) -> Vec3 {
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
        camera_start
    }

    fn render_model<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>, model: &'a GpuModel) {
        render_pass.set_bind_group(2, &model.model_bind_group, &[]);
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

    fn render<'a>(&'a self, render_pass: &mut super::renderer::RenderPass<'a>, render_all: bool) {
        if !render_all {
            for model_index in &self.models_to_render {
                let model = &self.map_models[*model_index];
                self.render_model(&mut render_pass.render_pass, model);
            }
        } else {
            for model in &self.map_models {
                self.render_model(&mut render_pass.render_pass, model);
            }
        }

        for model_index in &self.transparent_models {
            let model = &self.map_models[*model_index];
            self.render_model(&mut render_pass.render_pass, model);
        }
    }
}

pub struct BspRenderer {
    map_data: MapData,

    renderer: super::renderer::Renderer,
    player: MovingEntity,
    noclip: bool,
    gravity: bool,

    new_debug_point: Option<Vec3>,
    debug_point: Option<GpuModel>,
    new_debug_pyramid_location: Option<(Vec3, Vec3)>,
    debug_pyramid: Option<GpuModel>,

    draw_mode: DrawMode,
    draw_mode_update: Option<DrawMode>,
    render_all: bool,

    ui: Option<BspViewer>,
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
        let renderer = super::renderer::Renderer::new(device, config);

        let map_data = MapData::new(
            &renderer,
            reader,
            loaded_map_models,
            loaded_textures,
            device,
            queue,
        );

        // Find the "info_player_start" entity
        let camera_start = map_data.get_start_position(reader);
        println!("Start position: {:?}", camera_start);
        let player = MovingEntity::new(camera_start);

        let draw_mode = DrawMode::LitTexture;

        Self {
            map_data,

            renderer,
            player,
            noclip: false,
            gravity: true,

            new_debug_point: None,
            debug_point: None,
            new_debug_pyramid_location: None,
            debug_pyramid: None,

            draw_mode,
            draw_mode_update: None,
            render_all: false,

            ui: Some(BspViewer::new()),
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
            let mut render_pass = self.renderer.render(&mut encoder, clear_color, view);

            self.map_data.render(&mut render_pass, self.render_all);

            if let Some(model) = self.debug_point.as_ref() {
                self.map_data
                    .render_model(&mut render_pass.render_pass, model);
            }

            if let Some(model) = self.debug_pyramid.as_ref() {
                self.map_data
                    .render_model(&mut render_pass.render_pass, model);
            }
        }

        queue.submit(Some(encoder.finish()));
    }

    fn resize(
        &mut self,
        config: &wgpu::SurfaceConfiguration,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        self.renderer.resize(config, device, queue);
    }

    fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        delta: std::time::Duration,
        down_keys: &HashSet<KeyCode>,
        mouse_delta: Option<Vec2>,
        file_info: &Option<FileInfo>,
    ) {
        let mut rotation = self.renderer.camera().yaw_pitch_roll();
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
            self.renderer.camera_mut().set_yaw_pitch_roll(rotation);
        }

        let mut direction = Vec3::ZERO;
        let facing = self.renderer.camera().facing();
        let up = self.renderer.camera().up();
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

            if !self.noclip {
                let mut wish_dir = direction;
                wish_dir.y = 0.0;
                wish_dir.normalize()
            } else {
                direction
            }
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

            if !self.noclip {
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
            } else {
                position = start_position + (velocity * delta.as_secs_f32());
            }

            position = position + CROUCH_HEIGHT;

            self.renderer.camera_mut().set_position(position);
            self.player.set_velocity(velocity);
            self.player.set_position(position);
        }

        self.renderer.camera_mut().update(queue);

        if let Some(draw_mode) = self.draw_mode_update.take() {
            self.draw_mode = draw_mode;
            let draw_params = DrawParams {
                draw_mode: draw_mode as i32,
            };
            self.renderer.update_draw_params(queue, draw_params);
        }

        if let Some(new_debug_point) = self.new_debug_point.take() {
            let model = create_debug_point_model(new_debug_point, self.map_data.textures.len() - 1);
            let gpu_model = create_gpu_model_for_model(
                &model,
                Vec3::ZERO,
                1.0,
                device,
                self.renderer.model_bind_group_layout(),
                self.renderer.sampler(),
                &self.map_data.lightmap_view,
            );
            self.debug_point = Some(gpu_model);
        }

        if let Some((pos, dir)) = self.new_debug_pyramid_location.take() {
            let model = create_debug_pyramid_model(pos, dir, self.map_data.textures.len() - 1);
            let gpu_model = create_gpu_model_for_model(
                &model,
                Vec3::ZERO,
                1.0,
                device,
                self.renderer.model_bind_group_layout(),
                self.renderer.sampler(),
                &self.map_data.lightmap_view,
            );
            self.debug_pyramid = Some(gpu_model);
        }
    }

    fn world_pos_and_ray_from_screen_pos(&self, pos: Vec2) -> (Vec3, Vec3) {
        self.renderer
            .camera()
            .world_pos_and_ray_from_screen_pos(pos)
    }

    fn get_position_and_direction(&self) -> (Vec3, Vec3) {
        (
            self.renderer.camera().position(),
            self.renderer.camera().facing(),
        )
    }

    fn set_debug_point(&mut self, point: Vec3) {
        self.new_debug_point = Some(point);
    }

    fn set_debug_pyramid(&mut self, point: Vec3, dir: Vec3) {
        self.new_debug_pyramid_location = Some((point, dir));
    }

    fn set_draw_mode(&mut self, draw_mode: DrawMode) {
        if self.draw_mode != draw_mode {
            self.draw_mode_update = Some(draw_mode);
        }
    }

    fn get_draw_mode(&self) -> DrawMode {
        self.draw_mode
    }

    fn build_ui_menu(&mut self, ui: &imgui::Ui) {
        if let Some(viewer) = self.ui.as_mut() {
            let old_state = *viewer.state();
            viewer.build_menu(ui);
            let new_state = *viewer.state();

            if old_state.noclip != new_state.noclip {
                self.noclip = new_state.noclip;
            }

            if old_state.gravity != new_state.gravity {
                self.gravity = new_state.gravity;
            }
        }
    }

    fn build_ui(&mut self, ui: &imgui::Ui, file_info: &FileInfo) {
        if let Some(viewer) = self.ui.as_mut() {
            let bsp_file = match file_info {
                FileInfo::BspFile(file) => file,
                _ => panic!(),
            };
            viewer.build_ui(ui, bsp_file);
        }
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

fn create_gpu_model_for_model(
    model: &Model<ModelVertex>,
    origin: Vec3,
    alpha: f32,
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

    let transform = *Mat4::from_translation(origin).as_ref();
    let model_buffer_data = ModelBuffer { transform, alpha };
    let model_buffer_data_bytes = unsafe {
        let len = std::mem::size_of_val(&model_buffer_data);
        let ptr = &model_buffer_data as *const _ as *const u8;
        std::slice::from_raw_parts(ptr, len)
    };
    let model_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Model Uniform Buffer"),
        contents: model_buffer_data_bytes,
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
