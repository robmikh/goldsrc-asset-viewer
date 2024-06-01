use std::ops::Range;

use glam::{Mat3, Mat4, Quat, Vec3};
use gltf::add_and_get_index;
use crate::export::bsp::ModelVertex;

pub fn create_debug_point(
    pos: Vec3,
    indices: &mut Vec<u32>,
    vertices: &mut Vec<ModelVertex>,
) -> Range<usize> {
    let size = 5;
    let mins = [
        pos.x as i16 - size,
        pos.y as i16 - size,
        pos.z as i16 - size,
    ];
    let maxs = [
        pos.x as i16 + size,
        pos.y as i16 + size,
        pos.z as i16 + size,
    ];

    create_primitive(&mins, &maxs, indices, vertices)
}

pub fn create_debug_pyramid(
    pos: Vec3,
    dir: Vec3,
    indices: &mut Vec<u32>,
    vertices: &mut Vec<ModelVertex>,
) -> Range<usize> {
    let start = indices.len();
    add_pointing_triangle(pos, dir, indices, vertices);
    let end = indices.len();
    start..end
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

fn add_pointing_triangle(
    point: Vec3,
    dir: Vec3,
    indices: &mut Vec<u32>,
    vertices: &mut Vec<ModelVertex>,
) {
    let size = 5.0;
    let mut positions = [
        Vec3::new(point.x - size, point.y + size, point.z),
        Vec3::new(point.x + size, point.y + size, point.z),
        Vec3::new(point.x - size, point.y - size, point.z),
        Vec3::new(point.x + size, point.y - size, point.z),
        Vec3::new(point.x, point.y, point.z + size),
    ];
    let default_dir = Vec3::new(0.0, 0.0, 1.0);

    if dir != default_dir {
        let axis = {
            let mut axis = dir.cross(default_dir).abs().normalize();
            if axis.is_nan() {
                // Just pick one
                axis = Vec3::new(0.0, 1.0, 0.0);
            }
            axis
        };
        let angle = dir.angle_between(default_dir);
        let quat = Quat::from_rotation_arc(default_dir, dir);
        println!("axis: {:?}", axis);
        println!("angle: {}", angle);
        let transform = Mat4::from_translation(point) *
            //Mat4::from_axis_angle(axis, angle) *
            Mat4::from_quat(quat) *
            Mat4::from_translation(-point);
        for position in &mut positions {
            *position = transform.transform_point3(*position);
        }
    }

    let top_left = add_and_get_index(
        vertices,
        ModelVertex {
            pos: positions[0].to_array(),
            ..Default::default()
        },
    ) as u32;
    let top_right = add_and_get_index(
        vertices,
        ModelVertex {
            pos: positions[1].to_array(),
            ..Default::default()
        },
    ) as u32;
    let bottom_left = add_and_get_index(
        vertices,
        ModelVertex {
            pos: positions[2].to_array(),
            ..Default::default()
        },
    ) as u32;
    let bottom_right = add_and_get_index(
        vertices,
        ModelVertex {
            pos: positions[3].to_array(),
            ..Default::default()
        },
    ) as u32;
    let front = add_and_get_index(
        vertices,
        ModelVertex {
            pos: positions[4].to_array(),
            ..Default::default()
        },
    ) as u32;

    append_quad(top_left, top_right, bottom_left, bottom_right, indices);
    append_quad(top_right, top_left, bottom_right, bottom_left, indices);
    append_triangle(top_right, top_left, front, indices);
    append_triangle(bottom_right, top_right, front, indices);
    append_triangle(bottom_left, bottom_right, front, indices);
    append_triangle(top_left, bottom_left, front, indices);
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
