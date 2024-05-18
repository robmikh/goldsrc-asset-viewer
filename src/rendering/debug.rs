use std::ops::Range;

use glam::Vec3;

use crate::gltf::{add_and_get_index, bsp::ModelVertex};

pub fn create_debug_point(
    pos: Vec3,
    indices: &mut Vec<u32>,
    vertices: &mut Vec<ModelVertex>,
) -> Range<usize> {
    let size = 5;
    let mins = [pos.x as i16 - size, pos.y as i16 - size, pos.z as i16 - size];
    let maxs = [pos.x as i16 + size, pos.y as i16 + size, pos.z as i16 + size];

    create_primitive(&mins, &maxs, indices, vertices)
}

pub fn create_primitive(
    mins: &[i16; 3],
    maxs: &[i16; 3],
    indices: &mut Vec<u32>,
    vertices: &mut Vec<ModelVertex>,
) -> Range<usize> {
    let start = indices.len();
    add_rect_prism(mins, maxs, indices, vertices);
    let end = indices.len();
    start..end
}

fn add_rect_prism(
    mins: &[i16; 3],
    maxs: &[i16; 3],
    indices: &mut Vec<u32>,
    vertices: &mut Vec<ModelVertex>,
) {
    let back_top_left = add_and_get_index(
        vertices,
        ModelVertex {
            pos: [mins[0] as f32, maxs[1] as f32, mins[2] as f32],
            uv: [0.0, 0.0],
            ..Default::default()
        },
    ) as u32;
    let back_top_right = add_and_get_index(
        vertices,
        ModelVertex {
            pos: [maxs[0] as f32, maxs[1] as f32, mins[2] as f32],
            uv: [1.0, 0.0],
            ..Default::default()
        },
    ) as u32;
    let back_bottom_left = add_and_get_index(
        vertices,
        ModelVertex {
            pos: [mins[0] as f32, mins[1] as f32, mins[2] as f32],
            uv: [1.0, 1.0],
            ..Default::default()
        },
    ) as u32;
    let back_bottom_right = add_and_get_index(
        vertices,
        ModelVertex {
            pos: [maxs[0] as f32, mins[1] as f32, mins[2] as f32],
            uv: [0.0, 1.0],
            ..Default::default()
        },
    ) as u32;
    let front_top_left = add_and_get_index(
        vertices,
        ModelVertex {
            pos: [mins[0] as f32, maxs[1] as f32, maxs[2] as f32],
            uv: [0.0, 1.0],
            ..Default::default()
        },
    ) as u32;
    let front_top_right = add_and_get_index(
        vertices,
        ModelVertex {
            pos: [maxs[0] as f32, maxs[1] as f32, maxs[2] as f32],
            uv: [1.0, 1.0],
            ..Default::default()
        },
    ) as u32;
    let front_bottom_left = add_and_get_index(
        vertices,
        ModelVertex {
            pos: [mins[0] as f32, mins[1] as f32, maxs[2] as f32],
            uv: [0.0, 1.0],
            ..Default::default()
        },
    ) as u32;
    let front_bottom_right = add_and_get_index(
        vertices,
        ModelVertex {
            pos: [maxs[0] as f32, mins[1] as f32, maxs[2] as f32],
            uv: [1.0, 1.0],
            ..Default::default()
        },
    ) as u32;

    // Back
    append_quad(
        back_top_left,
        back_top_right,
        back_bottom_left,
        back_bottom_right,
        indices,
    );
    // Front
    append_quad(
        front_top_left,
        front_bottom_left,
        front_top_right,
        front_bottom_right,
        indices,
    );
    // Top
    append_quad(
        back_top_left,
        front_top_left,
        back_top_right,
        front_top_right,
        indices,
    );
    // Bottom
    append_quad(
        back_bottom_left,
        back_bottom_right,
        front_bottom_left,
        front_bottom_right,
        indices,
    );
    // Left
    append_quad(
        front_top_left,
        back_top_left,
        front_bottom_left,
        back_bottom_left,
        indices,
    );
    // Right
    append_quad(
        front_top_right,
        front_bottom_right,
        back_top_right,
        back_bottom_right,
        indices,
    );
}

fn append_quad(vertex_0: u32, vertex_1: u32, vertex_2: u32, vertex_3: u32, indices: &mut Vec<u32>) {
    append_triangle(vertex_0, vertex_1, vertex_2, indices);
    append_triangle(vertex_3, vertex_2, vertex_1, indices);
}

fn append_triangle(vertex_0: u32, vertex_1: u32, vertex_2: u32, indices: &mut Vec<u32>) {
    let mut new_indices = vec![vertex_0, vertex_1, vertex_2];
    indices.append(&mut new_indices);
}