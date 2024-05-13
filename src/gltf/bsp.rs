use std::{
    collections::HashMap, fmt::Write, ops::Range, path::{Path, PathBuf}
};

use glam::{Vec3, Vec4};
use gsparser::{bsp::{BspEdge, BspFace, BspReader, BspSurfaceEdge, BspTextureInfo, BspVertex}, wad3::{MipmapedTextureData, WadArchive}};

use super::{
    add_and_get_index,
    animation::Animations,
    buffer::{BufferViewAndAccessorPair, BufferViewTarget, BufferWriter},
    coordinates::convert_coordinates,
    export::write_gltf,
    material::{Image, MagFilter, Material, MaterialData, MinFilter, Texture, Wrap},
    node::{MeshIndex, Node, Nodes},
    skin::Skins,
    Mesh, Model, Vertex, VertexAttributesSource,
};

struct DebugVertex {
    pos: [f32; 3],
    uv: [f32; 2],
}

impl Vertex for DebugVertex {
    fn write_slices(
        writer: &mut super::buffer::BufferWriter,
        vertices: &[Self],
    ) -> Box<dyn super::VertexAttributesSource> {
        // Split out the vertex data
        let mut positions = Vec::with_capacity(vertices.len());
        let mut uvs = Vec::with_capacity(vertices.len());
        for vertex in vertices {
            positions.push(vertex.pos);
            uvs.push(vertex.uv);
        }

        let vertex_positions_pair = writer
            .create_view_and_accessor_with_min_max(&positions, Some(BufferViewTarget::ArrayBuffer));
        let vertex_uvs_pair = writer
        .create_view_and_accessor_with_min_max(&uvs, Some(BufferViewTarget::ArrayBuffer));

        Box::new(DebugVertexAttributes {
            positions: vertex_positions_pair,
            uvs: vertex_uvs_pair,
        })
    }
}

struct DebugVertexAttributes {
    positions: BufferViewAndAccessorPair,
    uvs: BufferViewAndAccessorPair,
}

impl VertexAttributesSource for DebugVertexAttributes {
    fn attribute_pairs(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("POSITION", self.positions.accessor.0),
            ("TEXCOORD_0", self.uvs.accessor.0),
            ]
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
struct SharedVertex {
    vertex: usize,
    texture: usize,
}

pub fn export<P: AsRef<Path>>(
    resource_wad: &WadArchive, 
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

        writeln!(log, "Leaves:").unwrap();
        for (i, leaf) in reader.read_leaves().iter().enumerate() {
            writeln!(log, "  Leaf {}", i).unwrap();
            writeln!(log, "    contents: {:?}", leaf.contents).unwrap();
            writeln!(log, "    vis_offset: {}", leaf.vis_offset).unwrap();
            writeln!(
                log,
                "    mins: [ {}, {}, {} ]",
                leaf.mins[0], leaf.mins[1], leaf.mins[2]
            )
            .unwrap();
            writeln!(
                log,
                "    maxs: [ {}, {}, {} ]",
                leaf.maxs[0], leaf.maxs[1], leaf.maxs[2]
            )
            .unwrap();
            writeln!(log, "    first_mark_surface: {}", leaf.first_mark_surface).unwrap();
            writeln!(log, "    mark_surfaces: {}", leaf.mark_surfaces).unwrap();
            writeln!(
                log,
                "    ambient_levels: [ {}, {}, {}, {} ]",
                leaf.ambient_levels[0],
                leaf.ambient_levels[1],
                leaf.ambient_levels[2],
                leaf.ambient_levels[3]
            )
            .unwrap();
        }

        writeln!(log, "Mark Surfaces:").unwrap();
        for (i, surface) in reader.read_mark_surfaces().iter().enumerate() {
            writeln!(log, "  Mark Surface {}", i).unwrap();
            writeln!(log, "    index: {}", surface.0).unwrap();
        }

        writeln!(log, "Surface Edges:").unwrap();
        for (i, edge) in reader.read_surface_edges().iter().enumerate() {
            writeln!(log, "  Surface Edge {}", i).unwrap();
            writeln!(log, "    index: {}", edge.0).unwrap();
        }

        writeln!(log, "Faces:").unwrap();
        for (i, face) in reader.read_faces().iter().enumerate() {
            writeln!(log, "  Face {}", i).unwrap();
            writeln!(log, "    plane: {}", face.plane).unwrap();
            writeln!(log, "    plane_side: {}", face.plane_side).unwrap();
            writeln!(log, "    first_edge: {}", face.first_edge).unwrap();
            writeln!(log, "    edges: {}", face.edges).unwrap();
            writeln!(log, "    texture_info: {}", face.texture_info).unwrap();
            writeln!(
                log,
                "    styles: [ {}, {}, {}, {} ]",
                face.styles[0], face.styles[1], face.styles[2], face.styles[3]
            )
            .unwrap();
            writeln!(log, "    lightmap_offset: {}", face.lightmap_offset).unwrap();
        }

        writeln!(log, "Edges:").unwrap();
        for (i, edge) in reader.read_edges().iter().enumerate() {
            writeln!(log, "  Edge {}", i).unwrap();
            writeln!(
                log,
                "    vertices: [ {}, {} ]",
                edge.vertices[0], edge.vertices[1]
            )
            .unwrap();
        }

        writeln!(log, "Vertices:").unwrap();
        for (i, vertex) in reader.read_vertices().iter().enumerate() {
            writeln!(log, "  Vertex {}", i).unwrap();
            writeln!(
                log,
                "    vertices: [ {}, {}, {} ]",
                vertex.x, vertex.y, vertex.z
            )
            .unwrap();
        }

        writeln!(log, "Textures:").unwrap();
        let texture_reader = reader.read_textures();
        for i in 0..texture_reader.len() {
            let reader = texture_reader.get(i).unwrap();
            let name = reader.get_image_name();
            writeln!(log, "  {} - {} - {}", i, name, reader.has_local_image_data()).unwrap();
        }
    }

    let texture_reader = reader.read_textures();
    let mut textures = Vec::with_capacity(texture_reader.len());
    for i in 0..texture_reader.len() {
        let reader = texture_reader.get(i).unwrap();
        if reader.has_local_image_data() {
            unimplemented!("bsp local image data not implemented");
        } else {
            let name = reader.get_image_name();
            let search_name = name.to_uppercase();
            let texture_data = if let Some(file) = resource_wad.files.iter().find(|x| x.name.as_str() == search_name.as_str()) {
                println!("Found \"{}\"!", name);
                let texture_data = resource_wad.decode_mipmaped_image(file);
                Some(texture_data)
            } else {
                println!("Couldn't find \"{}\"", name);
                None
            };
            textures.push((name.to_owned(), texture_data.unwrap()));
        }
    }

    let mut indices = Vec::new();
    let mut vertices = Vec::new();
    let mut primitives = Vec::new();

    let bsp_vertices = reader.read_vertices();
    let texture_infos = reader.read_texture_infos();
    //assert_eq!(bsp_vertices.len(), texture_infos.len());
    //for (vertex, texture_info) in bsp_vertices.iter().zip(texture_infos) {
    //    let pos = convert_coordinates(vertex.to_array());
    //    let pos_vec = Vec3::from_array(pos);
    //    let s = Vec3::from_array(convert_coordinates(texture_info.s));
    //    let t = Vec3::from_array(convert_coordinates(texture_info.t));
    //
    //    let uv = [ pos_vec.dot(s) + texture_info.s_shift, pos_vec.dot(t) + texture_info.t_shift ];
    //    
    //    vertices.push(DebugVertex {
    //        pos,
    //        uv,
    //    });
    //}

    let mut vertex_map = HashMap::<SharedVertex, usize>::new();
    let mark_surfaces = reader.read_mark_surfaces();
    let faces = reader.read_faces();
    let edges = reader.read_edges();
    let surface_edges = reader.read_surface_edges();
    let read_vertex_index = |edge_index: &BspSurfaceEdge, edges: &[BspEdge]| -> u32 {
        let edge_vertex_index: usize = if edge_index.0 > 0 { 0 } else { 1 };
        let edge_index = edge_index.0.abs() as usize;
        let edge = &edges[edge_index];
        edge.vertices[edge_vertex_index] as u32
    };
    for leaf in reader.read_leaves().iter() {
        let mark_surfaces_range = leaf.first_mark_surface..leaf.first_mark_surface+leaf.mark_surfaces;   
        for mark_surface_index in mark_surfaces_range {
            let mark_surface = &mark_surfaces[mark_surface_index as usize];
            let face = &faces[mark_surface.0 as usize];

            if face.texture_info == 0 {
                continue;
            }

            let surface_edges_range = face.first_edge as usize..face.first_edge as usize + face.edges as usize;
            let surface_edges = &surface_edges[surface_edges_range];

            let first_vertex = read_vertex_index(&surface_edges[0], edges);
            
            let mut triangle_list = Vec::new();
            let to_shared_vertex = |index: u32, face: &BspFace| -> SharedVertex {
                SharedVertex {
                    vertex: index as usize,
                    texture: face.texture_info as usize,
                }
            };
            for i in 0..surface_edges.len() - 2 {
                triangle_list.push(to_shared_vertex(read_vertex_index(&surface_edges[i + 2], edges), face));
                triangle_list.push(to_shared_vertex(read_vertex_index(&surface_edges[i + 1], edges), face));
                triangle_list.push(to_shared_vertex(first_vertex, face));
            }
            let start = indices.len();
            process_indexed_triangles(
                &triangle_list,
                face,
                bsp_vertices,
                &textures,
                texture_infos,
                &mut indices,
                &mut vertices,
                &mut vertex_map,
            );
            let end = indices.len();

            primitives.push((start..end, texture_infos[face.texture_info as usize].texture_index as usize));
        }
    }

    let mut buffer_writer = BufferWriter::new();
    let model = {
        let meshes: Vec<_> = primitives
            .iter()
            .map(|(range, texture_index)| Mesh {
                texture_index: *texture_index,
                indices_range: range.clone(),
            })
            .collect();
        Model {
            indices,
            vertices,
            meshes,
        }
    };

    let mut material_data = MaterialData::new();
    //material_data.add_material(Material {
    //    base_color_factor: Some(Vec4::new(1.0, 0.0, 0.0, 1.0)),
    //    metallic_factor: 0.0,
    //    roughness_factor: 1.0,
    //    ..Default::default()
    //});
    let sampler = material_data.add_sampler(super::material::Sampler {
        mag_filter: MagFilter::Linear,
        min_filter: MinFilter::LinearMipMapLinear,
        wrap_s: Wrap::MirroredRepeat,
        wrap_t: Wrap::MirroredRepeat,
    });
    for (name, _) in &textures {
        let image = material_data.add_images(Image {
            uri: format!("{}.png", name),
        });
        let texture = material_data.add_texture(Texture {
            sampler,
            source: image,
        });
        material_data.add_material(Material {
            base_color_texture: Some(texture),
            metallic_factor: 0.0,
            roughness_factor: 1.0,
            ..Default::default()
        });
    }

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

    // Write textures
    let mut texture_path = if let Some(parent_path) = path.parent() {
        let mut data_path = parent_path.to_owned();
        data_path.push("something");
        data_path
    } else {
        PathBuf::from("something")
    };
    for (name, texture) in textures {
        //if let Some(texture) = texture {
            texture_path.set_file_name(format!("{}.png", name));
            texture
                .image
                .save_with_format(&texture_path, image::ImageFormat::Png)
                .unwrap();
        //}
    }

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
            uv: [0.0, 0.0],
        },
    ) as u32;
    let back_top_right = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [maxs[0] as f32, maxs[1] as f32, mins[2] as f32],
            uv: [0.0, 0.0],
        },
    ) as u32;
    let back_bottom_left = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [mins[0] as f32, mins[1] as f32, mins[2] as f32],
            uv: [0.0, 0.0],
        },
    ) as u32;
    let back_bottom_right = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [maxs[0] as f32, mins[1] as f32, mins[2] as f32],
            uv: [0.0, 0.0],
        },
    ) as u32;
    let front_top_left = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [mins[0] as f32, maxs[1] as f32, maxs[2] as f32],
            uv: [0.0, 0.0],
        },
    ) as u32;
    let front_top_right = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [maxs[0] as f32, maxs[1] as f32, maxs[2] as f32],
            uv: [0.0, 0.0],
        },
    ) as u32;
    let front_bottom_left = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [mins[0] as f32, mins[1] as f32, maxs[2] as f32],
            uv: [0.0, 0.0],
        },
    ) as u32;
    let front_bottom_right = add_and_get_index(
        vertices,
        DebugVertex {
            pos: [maxs[0] as f32, mins[1] as f32, maxs[2] as f32],
            uv: [0.0, 0.0],
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


fn process_indexed_triangles(
    triangle_list: &[SharedVertex],
    face: &BspFace,
    bsp_vertices: &[BspVertex],
    textures: &[(String, MipmapedTextureData)],
    texture_infos: &[BspTextureInfo],
    indices: &mut Vec<u32>,
    vertices: &mut Vec<DebugVertex>,
    vertex_map: &mut HashMap<SharedVertex, usize>,
) {
    assert!(
        triangle_list.len() % 3 == 0,
        "Vertices are not a multiple of 3: {}",
        triangle_list.len()
    );
    let texture_info = &texture_infos[face.texture_info as usize];
    let (_, texture) = &textures[texture_info.texture_index as usize];

    let mut process_trivert = |trivert| {
        let index = if let Some(index) = vertex_map.get(trivert) {
            *index
        } else {
            let pos = convert_coordinates(bsp_vertices[trivert.vertex as usize].to_array());
            let pos_vec = Vec3::from_array(pos);
            let s = Vec3::from_array(convert_coordinates(texture_info.s));
            let t = Vec3::from_array(convert_coordinates(texture_info.t));
            let uv = [ pos_vec.dot(s) + texture_info.s_shift, pos_vec.dot(t) + texture_info.t_shift ];

            let uv = [
                uv[0] / texture.image_width as f32,
                uv[1] / texture.image_height as f32,
            ];

            println!("{:?}", uv);

            let index = vertices.len();
            vertices.push(DebugVertex {
                pos,
                uv,
            });
            vertex_map.insert(*trivert, index);
            index
        };
        indices.push(index as u32);
    };

    for trivert in triangle_list {
        process_trivert(trivert);
    }
}