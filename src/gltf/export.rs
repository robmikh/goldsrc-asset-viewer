use std::{
    collections::HashMap, ops::Range, path::{Path, PathBuf}
};

use glam::{EulerRot, Mat4, Quat, Vec3, Vec4};
use id_tree::{
    InsertBehavior::{AsRoot, UnderNode},
    Node, TreeBuilder,
};
use mdlparser::{MdlFile, MdlMeshSequenceType, MdlMeshVertex, MdlModel};

use crate::numerics::ToVec3;

use super::{BufferSlice, BufferType, BufferTypeEx, BufferViewAndAccessorSource, ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER};

struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

struct Mesh {
    texture_index: usize,
    indices_range: Range<usize>,
}

struct Model {
    indices: Vec<u32>,
    vertices: Vec<Vertex>,
    meshes: Vec<Mesh>,
}

pub fn export<P: AsRef<Path>>(file: &MdlFile, output_path: P) -> std::io::Result<()> {
    let body_part = file.body_parts.first().unwrap();
    let model = body_part.models.first().unwrap();

    // Compute bone transforms
    let mut local_bone_transforms = vec![Mat4::IDENTITY; file.bones.len()];
    let mut bone_tree = TreeBuilder::new()
        .with_node_capacity(file.bones.len())
        .build();
    let mut bone_map = HashMap::new();
    for (i, bone) in file.bones.iter().enumerate() {
        //println!("Bone {} : Parnet {}", i, bone.parent);
        let behavior = if bone.parent < 0 {
            AsRoot
        } else {
            let parent_node = bone_map.get(&(bone.parent as usize)).unwrap();
            UnderNode(parent_node)
        };
        let bone_id = bone_tree.insert(Node::new(i), behavior).unwrap();
        bone_map.insert(i, bone_id);
        let bone_pos = Vec3::from_array(convert_coordinates([bone.value[0], bone.value[1], bone.value[2]]));
        let bone_angles = Vec3::from_array(convert_coordinates([bone.value[3], bone.value[4], bone.value[5]]));
        
        // NOTE: These values have already been converted to GLTF's coordinate system
        //       Y is yaw, X is pitch, Z is roll
        let bone_transform = Mat4::from_rotation_translation(
            Quat::from_euler(EulerRot::YXZ, bone_angles.y, bone_angles.x, bone_angles.z).normalize(),
            bone_pos,
        );

        local_bone_transforms[i] = bone_transform;
    }
    let mut world_bone_transforms = vec![Mat4::IDENTITY; file.bones.len()];
    for node_id in bone_tree
        .traverse_pre_order_ids(bone_tree.root_node_id().unwrap())
        .unwrap()
    {
        let parent_index = {
            let mut ancestors = bone_tree.ancestors(&node_id).unwrap();
            if let Some(node) = ancestors.next() {
                Some(*node.data())
            } else {
                None
            }
        };

        let parent_transform = if let Some(parent_index) = parent_index {
            world_bone_transforms[parent_index]
        } else {
            Mat4::IDENTITY
        };
        let node_index = *bone_tree.get(&node_id).unwrap().data();
        let node_transform = *local_bone_transforms.get(node_index).unwrap();
        let node_world_transform = parent_transform * node_transform;

        world_bone_transforms[node_index] = node_world_transform;
    }
    let final_bone_transforms = world_bone_transforms;

    let converted_model = {
        // Gather mesh data
        let (meshes, indices, vertices) = {
            let mut meshes = Vec::with_capacity(model.meshes.len());
            let mut indices = Vec::new();
            let mut vertices = Vec::new();
            for mdl_mesh in &model.meshes {
                let texture = &file.textures[mdl_mesh.skin_ref as usize];
                let texture_width = texture.width as f32;
                let texture_height = texture.height as f32;

                let index_start = indices.len();
                let mut vertex_map = HashMap::new();
                for sequence in &mdl_mesh.sequences {
                    match sequence.ty {
                        MdlMeshSequenceType::TriangleStrip => {
                            let mut triverts = Vec::new();
                            for i in 0..sequence.triverts.len() - 2 {
                                if i % 2 == 0 {
                                    triverts.push(sequence.triverts[i + 1]);
                                    triverts.push(sequence.triverts[i]);
                                    triverts.push(sequence.triverts[i + 2]);
                                } else {
                                    triverts.push(sequence.triverts[i]);
                                    triverts.push(sequence.triverts[i + 1]);
                                    triverts.push(sequence.triverts[i + 2]);
                                }
                            }
                            process_indexed_triangles(
                                model,
                                texture_width,
                                texture_height,
                                &triverts,
                                &final_bone_transforms,
                                &mut indices,
                                &mut vertices,
                                &mut vertex_map,
                            );
                        }
                        MdlMeshSequenceType::TriangleFan => {
                            let mut triverts = Vec::new();
                            for i in 0..sequence.triverts.len() - 2 {
                                triverts.push(sequence.triverts[i + 2]);
                                triverts.push(sequence.triverts[i + 1]);
                                triverts.push(sequence.triverts[0]);
                            }
                            process_indexed_triangles(
                                model,
                                texture_width,
                                texture_height,
                                &triverts,
                                &final_bone_transforms,
                                &mut indices,
                                &mut vertices,
                                &mut vertex_map,
                            );
                        }
                    }
                }
                let index_end = indices.len();

                meshes.push(Mesh {
                    texture_index: mdl_mesh.skin_ref as usize,
                    indices_range: index_start..index_end,
                })
            }
            (meshes, indices, vertices)
        };
        Model {
            indices,
            vertices,
            meshes,
        }
    };
    
    //println!("Num meshes: {}", meshes.len());

    // Write our vertex and index data
    let mut data = Vec::new();
    let (slices, vertex_positions_min_max, vertex_normals_min_max, uvs_min_max) = {
        let mut slices = Vec::<Box<dyn BufferViewAndAccessorSource>>::new();

        // Split out the vertex data
        let mut positions = Vec::with_capacity(converted_model.vertices.len());
        let mut normals = Vec::with_capacity(converted_model.vertices.len());
        let mut uvs = Vec::with_capacity(converted_model.vertices.len());
        for vertex in &converted_model.vertices {
            positions.push(vertex.pos);
            normals.push(vertex.normal);
            uvs.push(vertex.uv);
        }

        let indices_slice = BufferSlice::record(&mut data, &converted_model.indices, ELEMENT_ARRAY_BUFFER);
        let vertex_positions_slice = BufferSlice::record(&mut data, &positions, ARRAY_BUFFER);
        let vertex_normals_slice = BufferSlice::record(&mut data, &normals, ARRAY_BUFFER);
        let uvs_slice = BufferSlice::record(&mut data, &uvs, ARRAY_BUFFER);    
    
        let vertex_positions_min_max = vertex_positions_slice.get_min_max_values();
        let vertex_normals_min_max = vertex_normals_slice.get_min_max_values();
        let uvs_min_max = uvs_slice.get_min_max_values();

        slices.push(Box::new(indices_slice));
        slices.push(Box::new(vertex_positions_slice));
        slices.push(Box::new(vertex_normals_slice));
        slices.push(Box::new(uvs_slice));

        (slices, vertex_positions_min_max, vertex_normals_min_max, uvs_min_max)
    };
    let indices_slice_index = 0;
    let vertex_positions_slice_index = 1;
    let vertex_normals_slice_index = 2;
    let uvs_slice_index = 3;

    // Record accessors for vertex and index data
    let mut accessors = Vec::new();

    // Vertex data
    let vertex_positions_accessor = add_accessor(&mut accessors, vertex_positions_slice_index, 0, converted_model.vertices.len(), vertex_positions_min_max.0, vertex_positions_min_max.1);
    let vertex_normals_accessor = add_accessor(&mut accessors, vertex_normals_slice_index, 0, converted_model.vertices.len(), vertex_normals_min_max.0, vertex_normals_min_max.1);
    let uvs_accessor = add_accessor(&mut accessors, uvs_slice_index, 0, converted_model.vertices.len(), uvs_min_max.0, uvs_min_max.1);

    let mut mesh_accessors = Vec::new();
    for mesh in &converted_model.meshes {
        let (min, max) = u32::find_min_max(&converted_model.indices[mesh.indices_range.start..mesh.indices_range.end]);
        let indices_accessor = add_accessor(&mut accessors, indices_slice_index, mesh.indices_range.start * std::mem::size_of::<u32>(), mesh.indices_range.end - mesh.indices_range.start, min, max);
        mesh_accessors.push((indices_accessor, vertex_positions_accessor, vertex_normals_accessor, uvs_accessor, mesh.texture_index));
    }

    // Create primitives
    let mut primitives = Vec::with_capacity(converted_model.meshes.len());
    for (indices, positions, normals, uvs, material) in mesh_accessors {
        primitives.push(format!(
            r#"         {{
            "attributes" : {{
            "POSITION" : {},
            "NORMAL" : {},
            "TEXCOORD_0" : {}
            }},
            "indices" : {},
            "material" : {}
        }}"#,
            positions, normals, uvs, indices, material
        ));
    }
    let primitives = primitives.join(",\n");

    // Create materials, textures, and images
    let mut materials = Vec::with_capacity(file.textures.len());
    let mut textures = Vec::with_capacity(file.textures.len());
    let mut images = Vec::with_capacity(file.textures.len());
    for (i, texture) in file.textures.iter().enumerate() {
        materials.push(format!(
            r#"          {{
            "pbrMetallicRoughness" : {{
              "baseColorTexture" : {{
                "index" : {}
              }},
              "metallicFactor" : 0.0,
              "roughnessFactor" : 1.0
            }}
          }}"#,
            i
        ));

        textures.push(format!(
            r#"           {{
            "sampler" : 0,
            "source" : {}
          }}"#,
            i
        ));

        images.push(format!(
            r#"         {{
            "uri" : "{}.png"
          }}"#,
            texture.name
        ));
    }
    let materials = materials.join(",\n");
    let textures = textures.join(",\n");
    let images = images.join(",\n");

    // Create buffer views and accessors
    let mut buffer_views = Vec::with_capacity(slices.len());
    let mut gltf_accessors = Vec::with_capacity(slices.len());
    for slice in &slices {
        buffer_views.push(slice.write_buffer_view());
    }
    for (buffer_view_index, byte_offset, count, min, max) in accessors {
        let slice = &slices[buffer_view_index];
        gltf_accessors.push(slice.write_accessor(buffer_view_index, byte_offset, count, &min, &max));
    }
    let buffer_views = buffer_views.join(",\n");
    let accessors = gltf_accessors.join(",\n");

    let gltf_text = format!(
        r#"{{
        "scene" : 0,
        "scenes" : [
            {{
                "nodes" : [ 0 ]
            }}
        ],
        "nodes" : [
            {{
                "mesh" : 0,
                "translation" : [ 1.0, 0.0, 0.0 ]
            }}
        ],
        
        "meshes" : [
            {{
            "primitives" : [ 
{}
             ]
            }}
        ],

        "materials" : [ 
            {}
         ],
        
          "textures" : [ 
            {}
           ],
          "images" : [ 
            {}
           ],
          "samplers" : [ {{
            "magFilter" : 9729,
            "minFilter" : 9987,
            "wrapS" : 33648,
            "wrapT" : 33648
          }} ],

          "buffers" : [
            {{
                "uri" : "data.bin",
                "byteLength" : {}
            }}
          ],

            "bufferViews" : [
                {}
            ],

            "accessors" : [
                {}
            ],

            "asset" : {{
                "version" : "2.0"
            }}
        }}
    "#,
        primitives,
        materials,
        textures,
        images,
        data.len(),
        buffer_views,
        accessors
    );

    let path = output_path.as_ref();
    let data_path = if let Some(parent_path) = path.parent() {
        let mut data_path = parent_path.to_owned();
        data_path.push("data.bin");
        data_path
    } else {
        PathBuf::from("data.bin")
    };

    std::fs::write(path, gltf_text)?;
    std::fs::write(data_path, data)?;

    // Write textures
    let mut texture_path = if let Some(parent_path) = path.parent() {
        let mut data_path = parent_path.to_owned();
        data_path.push("something");
        data_path
    } else {
        PathBuf::from("something")
    };
    for texture in &file.textures {
        texture_path.set_file_name(format!("{}.png", texture.name));
        texture
            .image_data
            .save_with_format(&texture_path, image::ImageFormat::Png)
            .unwrap();
    }

    Ok(())
}

// Half-Life's coordinate system uses:
//    X is forward
//    Y is left
//    Z is up
//    (https://github.com/malortie/assimp/wiki/MDL:-Half-Life-1-file-format#notes)
// GLTF's coordinate system uses:
//    X is left (-X is right)
//    Y is up
//    Z is forward
//    (https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#coordinate-system-and-units)
fn convert_coordinates(half_life_xyz: [f32; 3]) -> [f32; 3] {
    [half_life_xyz[1], half_life_xyz[2], half_life_xyz[0]]
}

fn process_indexed_triangles(
    model: &MdlModel,
    texture_width: f32,
    texture_height: f32,
    triverts: &[MdlMeshVertex],
    world_bone_transforms: &[Mat4],
    indices: &mut Vec<u32>,
    vertices: &mut Vec<Vertex>,
    vertex_map: &mut HashMap<MdlMeshVertex, usize>,
) {
    assert!(
        triverts.len() % 3 == 0,
        "Vertices are not a multiple of 3: {}",
        triverts.len()
    );

    let mut process_trivert = |trivert| {
        let index = if let Some(index) = vertex_map.get(trivert) {
            *index
        } else {
            let pos = convert_coordinates(model.vertices[trivert.vertex_index as usize]);
            let normal = convert_coordinates(model.normals[trivert.normal_index as usize]);

            let bone_index = model.vertex_bone_indices[trivert.vertex_index as usize];
            let pos = {
                let bone = world_bone_transforms[bone_index as usize];
                let pos = bone * Vec4::new(pos[0], pos[1], pos[2], 1.0);
                let pos = pos.to_vec3().to_array();
                pos
            };
            let normal = {
                let bone = world_bone_transforms[bone_index as usize];
                let normal = bone * Vec4::new(normal[0], normal[1], normal[2], 0.0);
                let normal = normal.to_vec3().normalize().to_array();
                normal
            };

            let uv = [
                trivert.s as f32 / texture_width,
                trivert.t as f32 / texture_height,
            ];
            let index = vertices.len();
            vertices.push(Vertex { pos, normal, uv });
            vertex_map.insert(*trivert, index);
            index
        };
        indices.push(index as u32);
    };

    for trivert in triverts {
        process_trivert(trivert);
    }
}

fn add_accessor<T: BufferType>(accessors: &mut Vec<(usize, usize, usize, String, String)>, buffer_view_index: usize, byte_offset: usize, len: usize, min: T, max: T) -> usize {
    let index = accessors.len();
    accessors.push((buffer_view_index, byte_offset, len, min.write_value(), max.write_value()));
    index
}