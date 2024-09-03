use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use glam::{Mat4, Vec2, Vec3, Vec4Swizzles};
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
    hittest::{hittest_clip_node, hittest_node_for_leaf, IntersectionInfo, REALLY_FAR},
    logic::entity::{
        Entity, EntityEx, EntityState, FuncDoorState, ModelReference, ParseEntity, ParseEntityValue,
    },
    rendering::movement::MovingEntity,
    FileInfo,
};

use super::{
    debug::{create_debug_point, create_debug_pyramid},
    renderer::{DrawParams, GpuVertex, ModelBuffer},
    Renderer,
};

// This was eyeballed, but the camera fov doesn't quite match so this is imperfect.
const CROUCH_HEIGHT: Vec3 = Vec3::new(0.0, 30.0, 0.0);
// This gets me up the stairs in c1a0d
const AUTO_STEP_HEIGHT: f32 = 18.0;

struct GpuModel {
    index_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    model_buffer: wgpu::Buffer,
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

impl ParseEntityValue for RenderMode {
    fn parse(name: &str, values: &HashMap<&str, &str>) -> Self {
        let str_value = values.get(name).unwrap();
        let value: RenderMode = str_value.parse().unwrap();
        value
    }
}

struct MapData {
    models_to_render: Vec<usize>,
    transparent_models: HashSet<usize>,
    map_models: Vec<GpuModel>,
    textures: Vec<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,

    _lightmap_texture: wgpu::Texture,
    lightmap_view: wgpu::TextureView,

    entities: Vec<Entity>,
    model_to_entity: HashMap<usize, usize>,

    spawns: Vec<(Vec3, f32)>,
    entity_states: Vec<EntityState>,
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

        let entities: Vec<Entity> = BspEntity::parse_entities(reader.read_entities_str())
            .iter()
            .map(|x| Entity::parse(&x.0))
            .collect();

        // Create a map of models to entities
        // TODO: Can we assume 1:1 (minus models not tied to any entities)?
        let mut model_to_entity = HashMap::new();
        for (entity_index, entity) in entities.iter().enumerate() {
            if let Some(model_index) = entity.model.as_ref() {
                if let ModelReference::Index(model_index) = model_index {
                    let old = model_to_entity.insert(*model_index, entity_index);
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
                        if let Some(hl_origin) = entity.origin.as_ref() {
                            let coord = convert_coordinates(*hl_origin);
                            origin = Vec3::new(coord[0] as f32, coord[1] as f32, coord[2] as f32);
                        }
                        if let Some(render_mode) = entity.render_mode.as_ref() {
                            if *render_mode != RenderMode::Normal {
                                if let Some(render_amt) = entity.render_amount.as_ref() {
                                    if *render_amt != 0 && *render_amt != 255 {
                                        alpha = *render_amt as f32 / 255.0;
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
            if let Some(model_index) = entity.model.as_ref() {
                if let ModelReference::Index(model_index) = model_index {
                    let model_index: usize = *model_index;

                    if let Some(render_mode) = entity.render_mode.as_ref() {
                        if *render_mode != RenderMode::Normal {
                            if let Some(render_amt) = entity.render_amount.as_ref() {
                                let render_amt = *render_amt;
                                if render_amt != 0 && render_amt != 255 {
                                    transparent_models.insert(model_index);
                                    if let Some(position) =
                                        models_to_render.iter().position(|x| *x == model_index)
                                    {
                                        models_to_render.remove(position);
                                        continue;
                                    }
                                } else if render_amt == 0 {
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

                    if entity.class_name.starts_with("trigger")
                        || entity.class_name.starts_with("func_ladder")
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

        let spawns = Self::collect_start_positions_and_orientations(&entities);

        let entity_states: Vec<_> = entities
            .iter()
            .map(|x| match &x.ex {
                EntityEx::FuncDoor(door) => {
                    // Get the direction of movement
                    let direction = if door.angle >= 0 {
                        let angle_in_degrees = door.angle as f32;
                        let mut direction = Vec3::new(0.0, 0.0, 1.0);
                        direction = (Mat4::from_rotation_y(angle_in_degrees.to_radians())
                            * direction.extend(0.0))
                        .xyz();
                        direction = direction.normalize();
                        direction
                    } else {
                        if door.angle == -1 {
                            Vec3::new(0.0, 1.0, 0.0)
                        } else if door.angle == -2 {
                            Vec3::new(0.0, -1.0, 0.0)
                        } else {
                            println!("Negative door angle \"{}\" not suppported!", door.angle);
                            return EntityState::None;
                        }
                    };

                    // TODO: Incorporate origin
                    assert_eq!(x.origin, None);
                    let closed_offset = Vec3::ZERO;

                    // In order to project each vertex onto our direction vector,
                    // we'll create two points on that line.
                    let line_start = closed_offset;
                    let line_end = (direction * 10.0) + line_start;

                    // Get the model for this entity
                    let model_ref = x
                        .model
                        .as_ref()
                        .expect("Expected model reference for door!");
                    // TODO: Support external models
                    let source_model = match model_ref {
                        ModelReference::Index(index) => &loaded_map_models[*index],
                        _ => {
                            println!("External models not supported in door preprocessing!");
                            return EntityState::None;
                        }
                    };

                    // Project each vertex
                    let mut min_distance = f32::MAX;
                    let mut max_distance = f32::MIN;
                    for vertex in &source_model.vertices {
                        let pos = Vec3::from_array(vertex.pos);
                        let ap = pos - line_start;
                        let ab = line_end - line_start;
                        let projected = line_start + ap.dot(ab) / ab.dot(ab) * ab;
                        let distance = line_start.distance(projected);
                        if max_distance < distance {
                            max_distance = distance;
                        }
                        if min_distance > distance {
                            min_distance = distance;
                        }
                    }
                    let computed_distance = max_distance - min_distance;

                    // Move the model
                    let movement = direction * (computed_distance - door.lip as f32);
                    let open_offset = closed_offset + movement;

                    EntityState::FuncDoor(FuncDoorState {
                        closed_offset,
                        open_offset,
                        is_open: false,
                    })
                }
                _ => EntityState::None,
            })
            .collect();

        Self {
            models_to_render,
            transparent_models,
            map_models,
            textures,

            _lightmap_texture: lightmap_texture,
            lightmap_view,

            entities,
            model_to_entity,

            spawns,
            entity_states,
        }
    }

    fn get_default_start_position_and_orientation(&self) -> (Vec3, f32) {
        // Find the "info_player_start" entity
        for entity in &self.entities {
            match entity.ex {
                EntityEx::InfoPlayerStart(_) => {
                    let origin = if let Some(hl_origin) = entity.origin.as_ref() {
                        let coord = convert_coordinates(*hl_origin);
                        Vec3::new(coord[0] as f32, coord[1] as f32, coord[2] as f32)
                    } else {
                        println!("WARNING: No origin found on info_player_start entity!");
                        Vec3::ZERO
                    };
                    let angle = if let Some(angle) = entity.angle.as_ref() {
                        let angle_in_radians = (*angle as f32).to_radians();
                        angle_in_radians
                    } else {
                        println!("WARNING: No angle found on info_player_start entity!");
                        0.0
                    };
                    return (origin, angle);
                }
                _ => {}
            }
        }
        panic!("No info_player_start entities found!");
    }

    fn collect_start_positions_and_orientations(entities: &[Entity]) -> Vec<(Vec3, f32)> {
        let mut starts = Vec::new();
        // Find the "info_player_start" entity
        for entity in entities {
            match entity.ex {
                EntityEx::InfoPlayerStart(_) => {
                    let origin = if let Some(hl_origin) = entity.origin.as_ref() {
                        let coord = convert_coordinates(*hl_origin);
                        Vec3::new(coord[0] as f32, coord[1] as f32, coord[2] as f32)
                    } else {
                        println!("WARNING: No origin found on info_player_start entity!");
                        Vec3::ZERO
                    };
                    let angle = if let Some(angle) = entity.angle.as_ref() {
                        let angle_in_radians = (*angle as f32).to_radians();
                        angle_in_radians
                    } else {
                        println!("WARNING: No angle found on info_player_start entity!");
                        0.0
                    };
                    starts.push((origin, angle));
                }
                _ => {}
            }
        }
        starts
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

    debug_point_position: Option<Vec3>,
    new_debug_point: Option<Vec3>,
    debug_point: Option<GpuModel>,
    new_debug_pyramid_location: Option<(Vec3, Vec3)>,
    debug_pyramid: Option<GpuModel>,

    new_spawn_point: Option<(Vec3, f32)>,

    draw_mode: DrawMode,
    draw_mode_update: Option<DrawMode>,
    render_all: bool,
    disable_level_change: bool,

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

        let new_spawn_point = Some(map_data.get_default_start_position_and_orientation());
        let player = MovingEntity::new(Vec3::ZERO);

        let draw_mode = DrawMode::LitTexture;

        Self {
            map_data,

            renderer,
            player,
            noclip: false,
            gravity: true,

            debug_point_position: None,
            new_debug_point: None,
            debug_point: None,
            new_debug_pyramid_location: None,
            debug_pyramid: None,

            new_spawn_point,

            draw_mode,
            draw_mode_update: None,
            render_all: false,
            disable_level_change: true,

            ui: Some(BspViewer::new()),
        }
    }

    fn find_closest_clipnode_model_intersection(
        &self,
        reader: &BspReader,
        start_position: Vec3,
        end_position: Vec3,
    ) -> Option<(usize, IntersectionInfo)> {
        let mut closest_intersection: Option<(usize, IntersectionInfo)> = None;
        for (i, model) in reader.read_models().iter().enumerate() {
            if i > 0 {
                let can_collide = {
                    let mut can_collide = false;
                    if let Some(entity_index) = self.map_data.model_to_entity.get(&i) {
                        let entity = &self.map_data.entities[*entity_index];
                        match entity.ex {
                            EntityEx::FuncWall(_) => can_collide = true,
                            EntityEx::FuncDoor(_) => can_collide = true,
                            _ => (),
                        }
                    }
                    can_collide
                };

                if !can_collide {
                    continue;
                }
            }

            // TODO: Account for model transforms (e.g. origin entity property)
            if let Some(entity_index) = self.map_data.model_to_entity.get(&i) {
                let entity = &self.map_data.entities[*entity_index];
                if entity.origin.is_some() {
                    println!("WARNING! Wall with origin!");
                }
            }
            let clip_node_index = model.head_nodes[1] as usize;
            if let Some(intersection) =
                hittest_clip_node(reader, clip_node_index, start_position, end_position)
            {
                let distance = start_position.distance(intersection.position);
                if let Some((old_i, old_intersection)) = closest_intersection.take() {
                    let old_distance = start_position.distance(old_intersection.position);
                    if distance < old_distance {
                        closest_intersection = Some((i, intersection));
                    } else {
                        closest_intersection = Some((old_i, old_intersection));
                    }
                } else {
                    closest_intersection = Some((i, intersection));
                }
            }
        }
        closest_intersection
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
        let mut collisions = 0;

        if let Some((_model_index, _intersection)) =
            self.find_closest_clipnode_model_intersection(reader, start_position, end_position)
        {
            let mut distance = start_position.distance(end_position);
            let full_distance = distance;
            let mut start_position = start_position;
            let mut end_position = end_position;
            while distance > 0.0 {
                if end_position.is_nan() {
                    position = start_position;
                    velocity = Vec3::ZERO;
                    break;
                }

                if let Some((_model_index, intersection)) = self
                    .find_closest_clipnode_model_intersection(reader, start_position, end_position)
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

                        if distance <= 0.0 || (dist <= 0.0 && distance / full_distance < 1.0) {
                            position = intersection.position;
                            break;
                        }

                        let new_vector = surface_dir * distance;
                        let new_velocity = velocity.length() * surface_dir;
                        velocity = new_velocity;

                        start_position = intersection.position;
                        end_position = intersection.position + new_vector;
                        position = start_position;
                    }
                } else {
                    position = end_position;
                    break;
                }
            }
        }

        (position, velocity, collisions > 0)
    }

    fn find_closest_model_intersection(
        &self,
        pos: Vec3,
        ray: Vec3,
        distance: f32,
        reader: &BspReader,
        head_node_index: usize,
    ) -> Option<(usize, Vec3)> {
        let models = reader.read_models();
        let mut closest_intersection = None;
        for (i, model) in models.iter().enumerate() {
            let node_index = model.head_nodes[head_node_index] as usize;
            if let Some((intersection_point, _leaf_index)) =
                hittest_node_for_leaf(reader, node_index, pos, ray, distance)
            {
                let distance = pos.distance(intersection_point);
                if let Some((old_i, old_intersection)) = closest_intersection.take() {
                    let old_distance = pos.distance(old_intersection);
                    if distance < old_distance {
                        closest_intersection = Some((i, intersection_point));
                    } else {
                        closest_intersection = Some((old_i, old_intersection));
                    }
                } else {
                    closest_intersection = Some((i, intersection_point));
                }
            }
        }
        closest_intersection
    }

    fn find_closest_model_intersection_from_models(
        &self,
        pos: Vec3,
        ray: Vec3,
        distance: f32,
        reader: &BspReader,
        head_node_index: usize,
        model_filter: &[usize],
    ) -> Option<(usize, Vec3)> {
        let models = reader.read_models();
        let mut closest_intersection = None;
        for i in model_filter {
            let i = *i;
            let model = models[i];
            let node_index = model.head_nodes[head_node_index] as usize;
            if let Some((intersection_point, _leaf_index)) =
                hittest_node_for_leaf(reader, node_index, pos, ray, distance)
            {
                let distance = pos.distance(intersection_point);
                if let Some((old_i, old_intersection)) = closest_intersection.take() {
                    let old_distance = pos.distance(old_intersection);
                    if distance < old_distance {
                        closest_intersection = Some((i, intersection_point));
                    } else {
                        closest_intersection = Some((old_i, old_intersection));
                    }
                } else {
                    closest_intersection = Some((i, intersection_point));
                }
            }
        }
        closest_intersection
    }

    fn find_model_with_entity(
        &self,
        pos: Vec3,
        ray: Vec3,
        file_info: &FileInfo,
    ) -> Option<(usize, usize, Vec3)> {
        match file_info {
            FileInfo::BspFile(file_info) => {
                let closest_intersection = self.find_closest_model_intersection(
                    pos,
                    ray,
                    REALLY_FAR,
                    &file_info.reader,
                    0,
                );
                let (model_index, intersection_point) = closest_intersection?;
                let entity_index = *self.map_data.model_to_entity.get(&model_index)?;
                Some((model_index, entity_index, intersection_point))
            }
            _ => panic!(),
        }
    }

    fn intersecting_with_change_level_trigger(
        &self,
        pos: Vec3,
        ray: Vec3,
        file_info: &FileInfo,
    ) -> Option<(String, String, Vec3)> {
        let mut new_map = None;
        if let Some((_model_index, entity_index, _)) =
            self.find_model_with_entity(pos, ray, file_info)
        {
            let entity = &self.map_data.entities[entity_index];
            if let EntityEx::TriggerChangeLevel(trigger_entity) = &entity.ex {
                let map_name = &trigger_entity.map;
                let landmark = &trigger_entity.landmark;

                // Calculate the relative position
                let landmark_entity = self
                    .map_data
                    .entities
                    .iter()
                    .find(|x| {
                        if let Some(target_name) = x.name.as_ref() {
                            target_name == &landmark.0
                        } else {
                            false
                        }
                    })
                    .expect("Expected entity with matching targetname to trigger landmark");
                let origin = if let Some(hl_origin) = landmark_entity.origin.as_ref() {
                    let coord = convert_coordinates(*hl_origin);
                    Vec3::new(coord[0] as f32, coord[1] as f32, coord[2] as f32)
                } else {
                    Vec3::ZERO
                };

                new_map = Some((map_name.clone(), landmark.0.clone(), origin));
            }
        }
        new_map
    }

    fn reorient_player(&mut self, reader: &BspReader, position: Vec3, angle: f32) {
        let mut camera_start = position - CROUCH_HEIGHT;
        // Check to see if we have something underneath us...
        let has_ground_underneath = self
            .find_closest_clipnode_model_intersection(
                reader,
                camera_start,
                camera_start - Vec3::new(0.0, 1.0, 0.0),
            )
            .is_some();
        if !has_ground_underneath {
            println!("Adjusting start position...");
            let adjust = 2.0 * CROUCH_HEIGHT;
            // Try again but place the player further up
            if let Some((_, intersection)) = self.find_closest_clipnode_model_intersection(
                reader,
                camera_start + adjust,
                camera_start - Vec3::new(0.0, 1.0, 0.0),
            ) {
                camera_start = intersection.position;
            } else {
                println!("Falling through the floor...");
            }
        }

        let yaw_pitch_roll = Vec3::new(angle, 0.0, 0.0);
        self.renderer
            .camera_mut()
            .set_yaw_pitch_roll(yaw_pitch_roll);

        let start_position = camera_start + CROUCH_HEIGHT;
        println!("Start position: {:?}", camera_start);
        self.player.set_position(start_position);
        self.renderer.camera_mut().set_position(start_position);
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
    ) -> Option<(String, String, Vec3)> {
        // First move the player if requested
        if let Some((position, angle)) = self.new_spawn_point.take() {
            let reader = match file_info.as_ref().unwrap() {
                FileInfo::BspFile(file) => &file.reader,
                _ => panic!(),
            };
            self.reorient_player(reader, position, angle)
        }

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

        let old_position = self.player.position();
        {
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
                    //let clip_node_index = reader.read_models()[0].head_nodes[1] as usize;

                    let is_touching_ground = self
                        .find_closest_clipnode_model_intersection(
                            reader,
                            start_position,
                            start_position - Vec3::new(0.0, 1.0, 0.0),
                        )
                        .is_some();

                    // Project our movement along the surface we're standing on
                    let surface_normal = if let Some((_, intersection)) = self
                        .find_closest_clipnode_model_intersection(
                            reader,
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
                    // TODO: This needs to be limited to only walls
                    if colided {
                        let nudged_start_position =
                            start_position + (surface_normal * AUTO_STEP_HEIGHT);
                        let nudged_end_position =
                            nudged_start_position + (previous_velocity * delta.as_secs_f32());

                        if self
                            .find_closest_clipnode_model_intersection(
                                reader,
                                nudged_start_position,
                                nudged_end_position,
                            )
                            .is_none()
                        {
                            position = nudged_end_position;
                            velocity = previous_velocity;

                            if is_touching_ground {
                                if let Some((_, intersection)) = self
                                    .find_closest_clipnode_model_intersection(
                                        reader,
                                        nudged_end_position,
                                        nudged_end_position
                                            - Vec3::new(0.0, AUTO_STEP_HEIGHT * 2.0, 0.0),
                                    )
                                {
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
            self.debug_point_position = Some(new_debug_point);
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

        let (position, direction) = self.get_position_and_direction();
        if let Some(viewer) = self.ui.as_mut() {
            viewer.set_position(position, direction)
        }

        // TODO: Check this during movement, not after
        // Check to see if we're intersecting an entity
        let file_info = file_info.as_ref().unwrap();
        let ray = direction * 0.001;
        let new_map = if !self.disable_level_change
            && self
                .intersecting_with_change_level_trigger(old_position, ray, file_info)
                .is_none()
        {
            self.intersecting_with_change_level_trigger(position, ray, file_info)
        } else {
            None
        };

        new_map
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

            if old_state.render_all != new_state.render_all {
                self.render_all = new_state.render_all;
            }

            if old_state.disable_level_change != new_state.disable_level_change {
                self.disable_level_change = new_state.disable_level_change;
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
            if let Some(spawn_index) = viewer.build_spawn_window(ui, &self.map_data.spawns) {
                self.new_spawn_point = Some(self.map_data.spawns[spawn_index]);
            }
        }

        if let Some(debug_point) = self.debug_point_position.clone() {
            ui.window("Debug Point")
                .position([840.0, 785.0], imgui::Condition::FirstUseEver)
                .size([600.0, 75.0], imgui::Condition::FirstUseEver)
                .build(|| {
                    let mut point = debug_point.to_array();
                    if ui.input_float3("Position", &mut point).build() {
                        let point = Vec3::from_array(point);
                        self.set_debug_point(point);
                    }
                });
        }
    }

    fn process_shift_left_click(&mut self, screen_space: Vec2, file_info: &Option<FileInfo>) {
        let (pos, ray) = self.world_pos_and_ray_from_screen_pos(screen_space);
        println!("pos: {:?}    ray: {:?}", pos, ray);

        if let Some(file_info) = file_info.as_ref() {
            let reader = match file_info {
                FileInfo::BspFile(file) => &file.reader,
                _ => panic!(),
            };
            let mut intersection_info = None;

            // Try not to select the model/entity we're already inside of.
            // At a maximum, we could be inside every model.
            let mut model_indices: Vec<usize> = (0..self.map_data.map_models.len()).collect();
            // Keep checking as long as we hit something that matches our position.
            while !model_indices.is_empty() {
                if let Some((model_index, intersection_point)) = self
                    .find_closest_model_intersection_from_models(
                        pos,
                        ray,
                        REALLY_FAR,
                        reader,
                        0,
                        &model_indices,
                    )
                {
                    let entity_index = if let Some(entity_index) =
                        self.map_data.model_to_entity.get(&model_index)
                    {
                        *entity_index
                    } else {
                        let position = model_indices.iter().position(|x| *x == model_index);
                        if let Some(position) = position {
                            model_indices.remove(position);
                        }
                        continue;
                    };

                    if intersection_point == pos {
                        let position = model_indices
                            .iter()
                            .position(|x| *x == model_index)
                            .unwrap();
                        model_indices.remove(position);
                    } else {
                        intersection_info = Some((model_index, entity_index, intersection_point));
                        break;
                    }
                } else {
                    break;
                }
            }

            if let Some((model_index, entity_index, intersection_point)) = intersection_info {
                self.set_debug_point(intersection_point);
                println!("Intersection: {:?}", intersection_point);
                println!("Hit something... {}", model_index);

                println!("Found entity: {}", entity_index);
                if let Some(viewer) = self.ui.as_mut() {
                    viewer.select_entity(entity_index as i32);
                }
            } else {
                if let Some(viewer) = self.ui.as_mut() {
                    viewer.select_entity(-1);
                }
            }
        }
    }

    fn process_shift_right_click(
        &mut self,
        screen_space: Vec2,
        file_info: &Option<FileInfo>,
        queue: &wgpu::Queue,
    ) {
        let (pos, ray) = self.world_pos_and_ray_from_screen_pos(screen_space);
        println!("pos: {:?}    ray: {:?}", pos, ray);

        let reader = match file_info.as_ref().unwrap() {
            FileInfo::BspFile(file) => &file.reader,
            _ => panic!(),
        };
        if let Some((model_index, intersection)) =
            self.find_closest_clipnode_model_intersection(reader, pos, pos + (ray * REALLY_FAR))
        {
            println!("model: {}    intersection: {:?}", model_index, intersection);
            self.set_debug_pyramid(intersection.position, intersection.normal);

            // DEBUG: Start experimenting with doors
            if let Some(entity_index) = self.map_data.model_to_entity.get(&model_index) {
                let entity = &self.map_data.entities[*entity_index];
                match &entity.ex {
                    EntityEx::FuncDoor(_) => {
                        let entity_state = &mut self.map_data.entity_states[*entity_index];
                        let entity_state = if let EntityState::FuncDoor(state) = entity_state {
                            state
                        } else {
                            println!("Door not implemented!");
                            return;
                        };
                        entity_state.is_open = !entity_state.is_open;
                        let offset = if entity_state.is_open {
                            entity_state.open_offset
                        } else {
                            entity_state.closed_offset
                        };

                        // Move the model
                        let model = &self.map_data.map_models[model_index];
                        let transform = Mat4::from_translation(offset);
                        let transform_ref: &[f32; 16] = transform.as_ref();
                        // Our transform is at the beginning of the ModelBuffer struct
                        queue.write_buffer(
                            &model.model_buffer,
                            0,
                            bytemuck::cast_slice(transform_ref),
                        );
                    }
                    _ => (),
                }
            }
        }
    }

    fn load_file(
        &mut self,
        file_info: &Option<FileInfo>,
        landmark: &str,
        old_origin: Vec3,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let file = match file_info.as_ref().unwrap() {
            FileInfo::BspFile(file) => file,
            _ => panic!(),
        };

        let path = PathBuf::from(&file.path).canonicalize().unwrap();
        let game_root_path = crate::get_game_root_path(&path).unwrap();

        let mut wad_resources = crate::export::bsp::WadCollection::new();
        crate::export::bsp::read_wad_resources(&file.reader, &game_root_path, &mut wad_resources);

        let textures = crate::export::bsp::read_textures(&file.reader, &wad_resources);
        let lightmap_atlas = decode_atlas(&file.reader);
        let map_models =
            crate::export::bsp::convert_models(&file.reader, &textures, &lightmap_atlas);

        self.map_data = MapData::new(
            &self.renderer,
            &file.reader,
            &map_models,
            &textures,
            device,
            queue,
        );

        self.debug_point = None;
        self.debug_point_position = None;
        self.debug_pyramid = None;

        // Move both player and camera
        let landmark_entity = self
            .map_data
            .entities
            .iter()
            .find(|x| {
                if let Some(target_name) = x.name.as_ref() {
                    target_name == landmark
                } else {
                    false
                }
            })
            .expect("Expected entity with matching targetname to previous map landmark");
        let origin = if let Some(hl_origin) = landmark_entity.origin.as_ref() {
            let coord = convert_coordinates(*hl_origin);
            Vec3::new(coord[0] as f32, coord[1] as f32, coord[2] as f32)
        } else {
            Vec3::ZERO
        };
        let position_diff = origin - old_origin;
        let player_position = self.player.position();
        let new_player_position = player_position + position_diff;
        self.player.set_position(new_player_position);
        self.renderer.camera_mut().set_position(new_player_position);
        self.renderer.camera_mut().update(queue);
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
        model_buffer,
        model_bind_group,
        meshes,
    }
}

fn create_debug_point_model(point: Vec3, texture_index: usize) -> Model<ModelVertex> {
    let mut indices = Vec::new();
    let mut vertices = Vec::new();
    let indices_range = create_debug_point(point, 1, &mut indices, &mut vertices);
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

#[cfg(test)]
mod experiments {
    use std::{
        collections::{HashMap, VecDeque},
        path::{Path, PathBuf},
    };

    use glam::Vec3;
    use gsparser::{
        bsp::{BspEntity, BspReader},
        mdl::null_terminated_bytes_to_str,
    };

    use crate::hittest::hittest_clip_node;

    const HALF_LIFE_BASE_PATH: &str = "testdata/Half-Life/valve/maps";

    fn process_all_entities<F: FnMut(&PathBuf, &BspEntity), P: AsRef<Path>>(
        base_path: P,
        mut process_entity: F,
    ) {
        for item in std::fs::read_dir(base_path).unwrap() {
            let item = item.unwrap();
            let item_path = item.path();
            if let Some(extension) = item_path.extension() {
                let extension = extension.to_str().unwrap();
                if extension == "bsp" {
                    println!("Validating {}...", item_path.display());

                    let bsp_bytes = std::fs::read(&item_path).unwrap();
                    let reader = BspReader::read(bsp_bytes);
                    let entities_bytes = reader.read_entities();
                    let entities_str = match null_terminated_bytes_to_str(entities_bytes) {
                        Ok(entities) => entities.to_owned(),
                        Err(error) => {
                            println!("  WARNING: {:?}", error);
                            let start = error.str_error.valid_up_to();
                            let end = start + error.str_error.error_len().unwrap_or(1);
                            println!("           error bytes: {:?}", &entities_bytes[start..end]);
                            String::from_utf8_lossy(&entities_bytes[..error.end]).to_string()
                        }
                    };
                    let entities = BspEntity::parse_entities(&entities_str);
                    for entity in entities {
                        process_entity(&item_path, &entity);
                    }
                }
            }
        }
    }

    // Turns out https://developer.valvesoftware.com/wiki/BSP_(GoldSrc)#Entities says that all
    // entities must have a classname. But we did learn that c1a3d contains invalid utf8 in the
    // entities string.
    #[test]
    fn all_entities_have_class_names() {
        process_all_entities(HALF_LIFE_BASE_PATH, |_item_path, entity| {
            let _ = entity.0.get("classname").unwrap();
        });
    }

    #[test]
    fn audit_class_names() {
        let mut class_names = HashMap::<String, usize>::new();
        process_all_entities(HALF_LIFE_BASE_PATH, |_item_path, entity| {
            let class_name = entity.0.get("classname").unwrap();
            if let Some(count) = class_names.get_mut(*class_name) {
                *count += 1;
            } else {
                class_names.insert(class_name.to_string(), 1);
            }
        });
        println!();

        let sorted_class_names = {
            let mut class_names = class_names.iter().collect::<Vec<_>>();
            class_names.sort_by(|a, b| {
                let count_ordering = a.1.cmp(b.1);
                match count_ordering {
                    std::cmp::Ordering::Equal => a.0.cmp(b.0),
                    _ => count_ordering,
                }
            });
            class_names
        };
        println!("Class names:");
        for (class_name, count) in sorted_class_names {
            println!("  {:<25}     {:>5}", class_name, count);
        }
    }

    #[test]
    fn clip_node_reconstruction() {
        let model_index = 8;
        let map_name = "c1a0e.bsp";
        let map_path = {
            let mut path = PathBuf::from(HALF_LIFE_BASE_PATH);
            path.push(map_name);
            path
        };

        let bsp_bytes = std::fs::read(&map_path).unwrap();
        let reader = BspReader::read(bsp_bytes);

        let model = &reader.read_models()[model_index];
        let head_clip_node = model.head_nodes[1] as i16;
        let nodes = reader.read_clip_nodes();
        let planes = reader.read_planes();

        let mut stack = VecDeque::new();
        stack.push_back((0, head_clip_node));
        while let Some((indent, node)) = stack.pop_back() {
            let indent_str = "  ".repeat(indent);
            if node >= 0 {
                let clip_node = &nodes[node as usize];
                stack.push_back((indent + 1, clip_node.children[0]));
                stack.push_back((indent + 1, clip_node.children[1]));

                let plane = &planes[clip_node.plane_index as usize];

                println!("{}{} - {:?}", indent_str, node, plane);
            } else {
                println!("{}{}", indent_str, node);
            }
        }
    }

    #[test]
    fn func_wall_move_test() {
        let model_index = 8;
        let map_name = "c1a0e.bsp";
        let map_path = {
            let mut path = PathBuf::from(HALF_LIFE_BASE_PATH);
            path.push(map_name);
            path
        };

        let bsp_bytes = std::fs::read(&map_path).unwrap();
        let reader = BspReader::read(bsp_bytes);

        let model = &reader.read_models()[model_index];
        let head_clip_node = model.head_nodes[1] as usize;

        // Based on this output:
        //
        // Hit a func_wall! Normal: Vec3(0.9243739, 0.0, 0.38148767)
        // start_position: Vec3(450.5545, -267.95654, 2110.265)
        // end_position:   Vec3(449.47562, -267.95654, 2108.6653)
        // Trace: QuakeTrace { plane: QuakePlane { normal: Vec3(0.38148767, 0.9243739, 0.0), dist: 1221.2007 }, intersection: Vec3(2109.9773, 450.3606, -267.95654), all_solid: false, start_solid: false, in_open: true, in_water: false }
        // Desired end: Vec3(449.47562, -267.95654, 2108.6653)
        // New end: Vec3(450.3606, -267.95654, 2109.9773)
        //     (8) distance: 0.34686163
        //   Vec3(449.47562, -267.95654, 2108.6653) -> Vec3(450.3606, -267.95654, 2109.9773)
        //   no intersection! Vec3(450.96432, -267.95654, 2108.5144)

        let start = Vec3::new(450.5545, -267.95654, 2110.265);
        let end = Vec3::new(449.47562, -267.95654, 2108.6653);
        let intersection =
            hittest_clip_node(&reader, head_clip_node, start, end).expect("Expected intersection!");
        println!("{:?}", intersection);

        let start_2 = intersection.position;
        assert_eq!(start_2, Vec3::new(450.3606, -267.95654, 2109.9773));
        // Our last known position from colliding with the railing in process_movement
        let lkg_position = Vec3::new(450.96432, -267.95654, 2108.5144);
        assert!(
            hittest_clip_node(&reader, head_clip_node, start_2, lkg_position).is_none(),
            "Did not expect intersection!"
        );

        let start_3 = lkg_position;
        let final_end = end;
        let intersection = hittest_clip_node(&reader, head_clip_node, start_3, final_end)
            .expect("Expected intersection!");
        println!("{:?}", intersection);
    }
}
