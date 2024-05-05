use std::{collections::HashMap, path::{Path, PathBuf}};

use glam::{Mat4, Vec3, Vec4};
use id_tree::{Node, TreeBuilder, InsertBehavior::{AsRoot, UnderNode}};
use image::DynamicImage;
use mdlparser::{MdlFile, MdlMeshSequenceType, MdlMeshVertex, MdlModel};

use crate::numerics::ToVec3;

use super::{BufferSlice, BufferViewAndAccessorSource, ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER};

struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

struct Mesh {
    texture_index: usize,
    indices: Vec<u32>,
    vertices: Vec<Vertex>,
}

pub fn export<P: AsRef<Path>>(file: &MdlFile, output_path: P) -> std::io::Result<()> {
    let body_part = file.body_parts.first().unwrap();
    let model = body_part.models.first().unwrap();

    // Compute bone transforms
    let mut local_bone_transforms = Vec::with_capacity(file.bones.len());
    let mut bone_tree = TreeBuilder::new().with_node_capacity(file.bones.len()).build();
    let mut bone_map = HashMap::new();
    //let mut bone_iter = file.bones.iter().enumerate();
    //let (root_bone_index, first_bone) = bone_iter.next().unwrap();  
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
        //println!("{:?}", bone.value);
        let bone_pos = Vec3::new(bone.value[0], bone.value[1], bone.value[2]);
        let bone_transform = 
            Mat4::from_translation(bone_pos) * 
            Mat4::from_rotation_x(bone.value[3]) *
            Mat4::from_rotation_y(bone.value[4]) *
            Mat4::from_rotation_z(bone.value[5]);
            
        //let bone_transform =  
        //    Mat4::from_rotation_z(bone.value[5]) * 
        //    Mat4::from_rotation_y(bone.value[4]) *
        //    Mat4::from_rotation_x(bone.value[3]) *
        //    Mat4::from_translation(bone_pos);

        //let bone_transform = 
        //    Mat4::from_rotation_x(bone.value[3]) *
        //    Mat4::from_rotation_y(bone.value[4]) *
        //    Mat4::from_rotation_z(bone.value[5]) * 
        //    Mat4::from_translation(bone_pos);
        local_bone_transforms.push(bone_transform);
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
    //let world_bone_transforms = vec![Mat4::IDENTITY; file.bones.len()];

    // Gather mesh data
    let meshes = {
        let mut meshes = Vec::with_capacity(model.meshes.len());
        for mdl_mesh in &model.meshes {
            let texture = &file.textures[mdl_mesh.skin_ref as usize];
            let texture_width = texture.width as f32;
            let texture_height = texture.height as f32;

            let mut indices = Vec::new();
            let mut vertices = Vec::new();
            let mut vertex_map = HashMap::new();
            for sequence in &mdl_mesh.sequences {
                match sequence.ty {
                    MdlMeshSequenceType::TriangleStrip => {
                        let mut triverts = Vec::new();
                        for i in 0..sequence.triverts.len()-2 {
                            if i % 2 == 0 {
                                triverts.push(sequence.triverts[i + 2]);
                                triverts.push(sequence.triverts[i]);
                                triverts.push(sequence.triverts[i + 1]);
                            } else {
                                triverts.push(sequence.triverts[i + 2]);
                                triverts.push(sequence.triverts[i + 1]);
                                triverts.push(sequence.triverts[i]);
                            }
                        }
                        process_indexed_triangles(model, texture_width, texture_height, &triverts, &world_bone_transforms, &mut indices, &mut vertices, &mut vertex_map);
                    }
                    MdlMeshSequenceType::TriangleFan => {
                        let mut triverts = Vec::new();
                        let mut iter = sequence.triverts.iter();
                        let center = *iter.next().unwrap();
                        let mut last = *iter.next().unwrap();
                        for next in iter {
                            triverts.push(center);
                            triverts.push(last);
                            triverts.push(*next);
                            last = *next;
                        }
                        process_indexed_triangles(model, texture_width, texture_height, &triverts, &world_bone_transforms, &mut indices, &mut vertices, &mut vertex_map);
                    },
                }
            }

            meshes.push(Mesh {
                indices,
                vertices,
                texture_index: mdl_mesh.skin_ref as usize,
            })
        }
        meshes
    };
    //println!("Num meshes: {}", meshes.len());

    // Write our vertex and index data
    // TODO: Don't use seperate buffers for each mesh
    let mut data = Vec::new();
    let mut slices = Vec::<Box<dyn BufferViewAndAccessorSource>>::new();
    let mut mesh_slices = Vec::new();
    for mesh in &meshes {

        //println!("Indices {}       Vertices: {}       {}", mesh.indices.len(), mesh.vertices.len(), mesh.indices.len() % 3);

        // Split out the vertex data
        let mut positions = Vec::with_capacity(mesh.vertices.len());
        let mut normals = Vec::with_capacity(mesh.vertices.len());
        let mut uvs = Vec::with_capacity(mesh.vertices.len());
        for vertex in &mesh.vertices {
            positions.push(vertex.pos);
            normals.push(vertex.normal);
            uvs.push(vertex.uv);
        }

        // Write data
        let indices_slice = BufferSlice::record(&mut data, &mesh.indices, ELEMENT_ARRAY_BUFFER);
        let vertex_positions_slice =
            BufferSlice::record(&mut data, &positions, ARRAY_BUFFER);
        let vertex_normals_slice = BufferSlice::record(&mut data, &normals, ARRAY_BUFFER);
        let uvs_slice = BufferSlice::record(
            &mut data,
            &uvs,
            ARRAY_BUFFER,
        );

        // Record indices
        let base_index = slices.len();
        mesh_slices.push((base_index, base_index + 1, base_index + 2, base_index + 3, mesh.texture_index));
        slices.push(Box::new(indices_slice));
        slices.push(Box::new(vertex_positions_slice));
        slices.push(Box::new(vertex_normals_slice));
        slices.push(Box::new(uvs_slice));

        // DEBUG: Remove later
        //break;
    }

    // Create primitives
    let mut primitives = Vec::with_capacity(meshes.len());
    for (indices, positions, normals, uvs, material) in mesh_slices {
        primitives.push(format!(r#"         {{
            "attributes" : {{
            "POSITION" : {},
            "NORMAL" : {},
            "TEXCOORD_0" : {}
            }},
            "indices" : {},
            "material" : {}
        }}"#, positions, normals, uvs, indices, material));
    }
    let primitives = primitives.join(",\n");

    // Create materials, textures, and images
    let mut materials = Vec::with_capacity(file.textures.len());
    let mut textures = Vec::with_capacity(file.textures.len());
    let mut images = Vec::with_capacity(file.textures.len());
    for (i, texture) in file.textures.iter().enumerate() {
        materials.push(format!(r#"          {{
            "pbrMetallicRoughness" : {{
              "baseColorTexture" : {{
                "index" : {}
              }},
              "metallicFactor" : 0.0,
              "roughnessFactor" : 1.0
            }}
          }}"#, i));

        textures.push(format!(r#"           {{
            "sampler" : 0,
            "source" : {}
          }}"#, i));

        images.push(format!(r#"         {{
            "uri" : "{}.png"
          }}"#, texture.name));
    }
    let materials = materials.join(",\n");
    let textures = textures.join(",\n");
    let images = images.join(",\n");

    // Create buffer views and accessors
    let mut buffer_views = Vec::with_capacity(slices.len());
    let mut accessors = Vec::with_capacity(slices.len());
    for (i, slice) in slices.iter().enumerate() {
        buffer_views.push(slice.write_buffer_view());
        accessors.push(slice.write_accessor(i));
    }
    let buffer_views = buffer_views.join(",\n");
    let accessors = accessors.join(",\n");

    let gltf_text = format!(r#"{{
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
    "#, primitives, materials, textures, images, data.len(), buffer_views, accessors);

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
        let image = DynamicImage::ImageBgra8(texture.image_data.clone());
        let rgba_image = image.to_rgba8();
        rgba_image.save_with_format(&texture_path, image::ImageFormat::Png).unwrap();
    }

    Ok(())
}

fn process_indexed_triangles(model: &MdlModel, texture_width: f32, texture_height: f32, triverts: &[MdlMeshVertex], world_bone_transforms: &[Mat4], indices: &mut Vec<u32>, vertices: &mut Vec<Vertex>, vertex_map: &mut HashMap<MdlMeshVertex, usize>) {
    assert!(triverts.len() % 3 == 0, "Vertices are not a multiple of 3: {}", triverts.len());
    
    let mut process_trivert = |trivert| {
        let index = if let Some(index) = vertex_map.get(trivert) {
            *index
        } else {
            let pos = model.vertices[trivert.vertex_index as usize];
            let bone_index = model.vertex_bone_indices[trivert.vertex_index as usize];
            //println!("{}", bone_index);
            let pos = if bone_index < 0 {
                pos
            } else {
                let bone = world_bone_transforms[bone_index as usize];
                let pos = bone * Vec4::new(pos[0], pos[1], pos[2], 0.0);
                let pos = pos.to_vec3().to_array();
                pos
            };
            
            let normal = model.normals[trivert.normal_index as usize];
            let uv = [
                trivert.s as f32 / texture_width,
                trivert.t as f32 / texture_height
            ];
            let index = vertices.len();
            vertices.push(Vertex { pos, normal, uv });
            vertex_map.insert(*trivert, index);
            index
        };
        indices.push(index as u32);
    };
    
    
    // TODO: Winding order?
    //for trivert in triverts {
    //    process_trivert(trivert);
    //}
    for triverts in triverts.chunks_exact(3) {
        for trivert in triverts.iter().rev() {
            process_trivert(trivert);
        }
    }


}