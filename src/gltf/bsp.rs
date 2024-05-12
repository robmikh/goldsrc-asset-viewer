use std::{
    fmt::Write,
    ops::Range,
    path::{Path, PathBuf},
};

use glam::Vec4;
use gsparser::bsp::BspReader;

use super::{
    add_and_get_index,
    animation::Animations,
    buffer::{BufferViewAndAccessorPair, BufferViewTarget, BufferWriter},
    coordinates::convert_coordinates,
    export::write_gltf,
    material::{Material, MaterialData},
    node::{MeshIndex, Node, Nodes},
    skin::Skins,
    Mesh, Model, Vertex, VertexAttributesSource,
};

struct DebugVertex {
    pos: [f32; 3],
}

impl Vertex for DebugVertex {
    fn write_slices(
        writer: &mut super::buffer::BufferWriter,
        vertices: &[Self],
    ) -> Box<dyn super::VertexAttributesSource> {
        // Split out the vertex data
        let mut positions = Vec::with_capacity(vertices.len());
        for vertex in vertices {
            positions.push(vertex.pos);
        }

        let vertex_positions_pair = writer
            .create_view_and_accessor_with_min_max(&positions, Some(BufferViewTarget::ArrayBuffer));

        Box::new(DebugVertexAttributes {
            positions: vertex_positions_pair,
        })
    }
}

struct DebugVertexAttributes {
    positions: BufferViewAndAccessorPair,
}

impl VertexAttributesSource for DebugVertexAttributes {
    fn attribute_pairs(&self) -> Vec<(&'static str, usize)> {
        vec![("POSITION", self.positions.accessor.0)]
    }
}

pub fn export<P: AsRef<Path>>(
    reader: &BspReader,
    export_file_path: P,
    mut log: Option<&mut String>,
) -> std::io::Result<()> {
    if let Some(log) = &mut log {
        writeln!(log, "Nodes:").unwrap();
        for (i, node) in reader.read_nodes().iter().enumerate() {
            writeln!(log, "  Node {}", i).unwrap();
            writeln!(log, "    plane: {}", node.plane).unwrap();
            writeln!(
                log,
                "    children: [ {}, {} ]",
                node.children[0], node.children[1]
            )
            .unwrap();
            writeln!(
                log,
                "    mins: [ {}, {}, {} ]",
                node.mins[0], node.mins[1], node.mins[2]
            )
            .unwrap();
            writeln!(
                log,
                "    maxs: [ {}, {}, {} ]",
                node.maxs[0], node.maxs[1], node.maxs[2]
            )
            .unwrap();
            writeln!(log, "    first_face: {}", node.first_face).unwrap();
            writeln!(log, "    faces: {}", node.faces).unwrap();
        }
    }

    let mut indices = Vec::new();
    let mut vertices = Vec::new();
    let mut primitives = Vec::new();
    for node in reader.read_nodes().iter() {
        let primitive_range = create_primitive(
            &convert_coordinates(node.mins),
            &convert_coordinates(node.maxs),
            &mut indices,
            &mut vertices,
        );
        primitives.push(primitive_range);
    }

    let mut buffer_writer = BufferWriter::new();
    let model = {
        let meshes: Vec<_> = primitives
            .iter()
            .map(|x| Mesh {
                texture_index: 0,
                indices_range: x.clone(),
            })
            .collect();
        Model {
            indices,
            vertices,
            meshes,
        }
    };

    let mut material_data = MaterialData::new();
    material_data.add_material(Material {
        base_color_factor: Some(Vec4::new(1.0, 0.0, 0.0, 1.0)),
        metallic_factor: 0.0,
        roughness_factor: 1.0,
        ..Default::default()
    });

    let skins = Skins::new();
    let animations = Animations::new(0);
    let mut nodes = Nodes::new(1);
    let scene_root = nodes.add_node(Node {
        mesh: Some(MeshIndex(0)),
        ..Default::default()
    });

    let buffer_name = "data.bin";
    let gltf_text = write_gltf(
        buffer_name,
        &mut buffer_writer,
        &model,
        &material_data,
        scene_root,
        &nodes,
        &skins,
        &animations,
    );

    let path = export_file_path.as_ref();
    let data_path = if let Some(parent_path) = path.parent() {
        let mut data_path = parent_path.to_owned();
        data_path.push(buffer_name);
        data_path
    } else {
        PathBuf::from(buffer_name)
    };

    std::fs::write(path, gltf_text)?;
    std::fs::write(data_path, buffer_writer.to_inner())?;

    Ok(())
}

fn create_primitive(
    mins: &[i16; 3],
    maxs: &[i16; 3],
    indices: &mut Vec<u32>,
    vertices: &mut Vec<DebugVertex>,
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
    vertices: &mut Vec<DebugVertex>,
) {
    let back_top_left = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [mins[0] as f32, maxs[1] as f32, mins[2] as f32],
        },
    ) as u32;
    let back_top_right = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [maxs[0] as f32, maxs[1] as f32, mins[2] as f32],
        },
    ) as u32;
    let back_bottom_left = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [mins[0] as f32, mins[1] as f32, mins[2] as f32],
        },
    ) as u32;
    let back_bottom_right = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [maxs[0] as f32, mins[1] as f32, mins[2] as f32],
        },
    ) as u32;
    let front_top_left = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [mins[0] as f32, maxs[1] as f32, maxs[2] as f32],
        },
    ) as u32;
    let front_top_right = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [maxs[0] as f32, maxs[1] as f32, maxs[2] as f32],
        },
    ) as u32;
    let front_bottom_left = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [mins[0] as f32, mins[1] as f32, maxs[2] as f32],
        },
    ) as u32;
    let front_bottom_right = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [maxs[0] as f32, mins[1] as f32, maxs[2] as f32],
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
