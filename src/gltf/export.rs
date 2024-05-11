use std::{
    collections::HashMap,
    fmt::Write,
    ops::Range,
    path::{Path, PathBuf},
};

use glam::{Mat4, Vec3, Vec4};
use gsparser::mdl::{
    null_terminated_bytes_to_str, BoneChannelAnimation, ComponentTransformTarget, MdlFile,
    MdlMeshSequenceType, MdlMeshVertex, MdlModel, VectorChannel,
};
use id_tree::{
    InsertBehavior::{AsRoot, UnderNode},
    Node, TreeBuilder,
};

use crate::{
    gltf::transform::quat_from_euler,
    numerics::{ToVec3, ToVec4},
};

use super::{
    add_and_get_index, buffer::{BufferSlice, BufferType, BufferTypeEx, BufferTypeMinMax, BufferViewAndAccessorSource, MinMax, ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER}, transform::ComponentTransform, BufferViewAndAccessorPair, BufferViewTarget, BufferWriter, GltfAnimation, GltfChannelAnimation, GltfTargetPath, Mesh, Model, Vertex, VertexAttributesSource
};

struct SkinnedVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    joints: [u8; 4],
    weights: [f32; 4],
}

impl Vertex for SkinnedVertex {
    fn write_slices(writer: &mut BufferWriter, vertices: &[Self]) -> Box<dyn VertexAttributesSource> {
        // Split out the vertex data
        let mut positions = Vec::with_capacity(vertices.len());
        let mut normals = Vec::with_capacity(vertices.len());
        let mut uvs = Vec::with_capacity(vertices.len());
        let mut joints = Vec::with_capacity(vertices.len());
        let mut weights = Vec::with_capacity(vertices.len());
        for vertex in vertices {
            positions.push(vertex.pos);
            normals.push(vertex.normal);
            uvs.push(vertex.uv);
            joints.push(vertex.joints);
            weights.push(vertex.weights);
        }

        let vertex_positions_pair = writer.create_view_and_accessor_with_min_max(&positions, Some(BufferViewTarget::ArrayBuffer));
        let vertex_normals_pair = writer.create_view_and_accessor_with_min_max(&normals, Some(BufferViewTarget::ArrayBuffer));
        let vertex_uvs_pair = writer.create_view_and_accessor_with_min_max(&uvs, Some(BufferViewTarget::ArrayBuffer));
        let vertex_joints_pair = writer.create_view_and_accessor_with_min_max(&joints, Some(BufferViewTarget::ArrayBuffer));
        let vertex_weights_pair = writer.create_view_and_accessor_with_min_max(&weights, Some(BufferViewTarget::ArrayBuffer));

        Box::new(SkinnedVertexAttributes {
            positions: vertex_positions_pair,
            normals: vertex_normals_pair,
            uvs: vertex_uvs_pair,
            joints: vertex_joints_pair,
            weights: vertex_weights_pair,
        })
    }
}

struct SkinnedVertexAttributes {
    positions: BufferViewAndAccessorPair,
    normals: BufferViewAndAccessorPair,
    uvs: BufferViewAndAccessorPair,
    joints: BufferViewAndAccessorPair,
    weights: BufferViewAndAccessorPair,
}

impl VertexAttributesSource for SkinnedVertexAttributes {
    fn attribute_pairs(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("POSITION", self.positions.accessor.0),
            ("NORMAL", self.normals.accessor.0),
            ("TEXCOORD_0", self.uvs.accessor.0),
            ("JOINTS_0", self.joints.accessor.0),
            ("WEIGHTS_0", self.weights.accessor.0),
        ]
    }
}

pub fn export<P: AsRef<Path>>(
    file: &MdlFile,
    output_path: P,
    mut log: Option<&mut String>,
) -> std::io::Result<()> {
    let body_part = file.body_parts.first().unwrap();
    let model = body_part.models.first().unwrap();

    if let Some(log) = &mut log {
        writeln!(log, "Animation Sequence Groups:").unwrap();
        for group in &file.animation_sequence_groups {
            let name = null_terminated_bytes_to_str(group.name());
            let label = null_terminated_bytes_to_str(&group.label);

            writeln!(log, "  {} - {}", label, name).unwrap();
        }
    }

    // Compute bone transforms
    let mut bone_names = Vec::with_capacity(file.bones.len());
    let mut local_bone_transforms = Vec::with_capacity(file.bones.len());
    let mut local_bone_component_transforms = Vec::with_capacity(file.bones.len());
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
        let bone_pos = Vec3::from_array(convert_coordinates([
            bone.value[0],
            bone.value[1],
            bone.value[2],
        ]));
        let bone_angles = Vec3::from_array(convert_coordinates([
            bone.value[3],
            bone.value[4],
            bone.value[5],
        ]));

        // NOTE: These values have already been converted to GLTF's coordinate system
        //       Y is yaw, X is pitch, Z is roll
        let bone_component_transform = ComponentTransform::new(bone_pos, bone_angles);
        let bone_transform = bone_component_transform.to_mat4();

        bone_names.push(null_terminated_bytes_to_str(&bone.name).to_owned());
        local_bone_transforms.push(bone_transform);
        local_bone_component_transforms.push(bone_component_transform);
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

    // Compute the inverse bind matrices
    let inverse_bind_transforms: Vec<_> =
        final_bone_transforms.iter().map(|x| x.inverse()).collect();

    // Build nodes
    let mut nodes = Vec::with_capacity(file.bones.len() + 1);
    let mut bone_to_node: HashMap<usize, usize> = HashMap::new();
    nodes.push(
        r#"          {
                "mesh" : 0,
                "skin" : 0
            }"#
        .to_owned(),
    );
    for node_id in bone_tree
        .traverse_post_order_ids(bone_tree.root_node_id().unwrap())
        .unwrap()
    {
        let bone_index = *bone_tree.get(&node_id).unwrap().data();
        let component_transform = local_bone_component_transforms.get(bone_index).unwrap();
        let rotation = component_transform.get_rotation_quat();

        let mut children = Vec::new();
        for child in bone_tree.children(&node_id).unwrap() {
            let child = child.data();
            let bone_index = bone_to_node.get(child).unwrap();
            children.push(bone_index.to_string());
        }
        let children = if children.is_empty() {
            "".to_owned()
        } else {
            let children = children.join(", ");
            format!("\"children\" : [ {} ],\n           ", children)
        };

        let gltf_node_index = nodes.len();
        nodes.push(format!(
            r#"          {{
            {}"name" : "{}",
            "translation" : [ {}, {}, {} ],
            "rotation" : [ {}, {}, {}, {} ]
        }}"#,
            children,
            &bone_names[bone_index],
            component_transform.translation.x,
            component_transform.translation.y,
            component_transform.translation.z,
            rotation.x,
            rotation.y,
            rotation.z,
            rotation.w
        ));
        bone_to_node.insert(bone_index, gltf_node_index);
    }
    let skin_root = *bone_to_node
        .get(
            bone_tree
                .get(bone_tree.root_node_id().unwrap())
                .unwrap()
                .data(),
        )
        .unwrap();
    let scene_root = add_and_get_index(
        &mut nodes,
        format!(
            r#"     {{
            "children" : [ 0, {} ]
        }}"#,
            skin_root
        ),
    );
    let nodes = nodes.join(",\n");

    // Build animations
    let mut gltf_animations = Vec::with_capacity(file.animations.len());
    for animation in &file.animations {
        let mut animation_data = Vec::new();
        for bone_animation in &animation.bone_animations {
            let target_bone = bone_animation.target;

            // We need to collapse animations that target the same component
            // E.g. Translate(X) and Translate(Y) becomes Translate(XY)
            let mut translate_animations = Vec::new();
            let mut rotation_animations = Vec::new();
            for (channel_index, channel) in bone_animation.channels.iter().enumerate() {
                match channel.target {
                    ComponentTransformTarget::Translation(vec_channel) => {
                        translate_animations.push((vec_channel, channel_index));
                    }
                    ComponentTransformTarget::Rotation(vec_channel) => {
                        rotation_animations.push((vec_channel, channel_index));
                    }
                }
            }

            // Use the default pose as a baseline
            let component_transform = &local_bone_component_transforms[bone_animation.target];
            let mut translation = component_transform.translation;
            let mut rotation = component_transform.rotation;

            let target_node = *bone_to_node.get(&target_bone).unwrap();
            if let Some(animation) = process_animation(
                &mut translation,
                GltfTargetPath::Translation,
                &translate_animations,
                &bone_animation.channels,
                target_node,
                animation.fps,
            ) {
                animation_data.push(animation);
            }
            if let Some(animation) = process_animation(
                &mut rotation,
                GltfTargetPath::Rotation,
                &rotation_animations,
                &bone_animation.channels,
                target_node,
                animation.fps,
            ) {
                animation_data.push(animation);
            }
        }

        gltf_animations.push(GltfAnimation {
            channels: animation_data,
            name: animation.name.clone(),
        });
    }

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

    if let Some(log) = &mut log {
        writeln!(log, "Num meshes: {}", model.meshes.len()).unwrap();
    }

    // Write our vertex and index data
    let mut buffer_writer = BufferWriter::new();
    let indices_view = buffer_writer.create_view(&converted_model.indices, Some(BufferViewTarget::ElementArrayBuffer));
    let vertex_attributes = SkinnedVertex::write_slices(&mut buffer_writer, &converted_model.vertices);
    let inverse_bind_matrices_pair = buffer_writer.create_view_and_accessor(&inverse_bind_transforms, None);

    let mut mesh_primitives = Vec::new();
    for mesh in &converted_model.meshes {
        let indices = &converted_model.indices[mesh.indices_range.start..mesh.indices_range.end];
        let (min, max) = u32::find_min_max(
            indices
        );
        let indices_accessor = buffer_writer.create_accessor_with_min_max(
            indices_view,
            mesh.indices_range.start * std::mem::size_of::<u32>(),
            mesh.indices_range.end - mesh.indices_range.start,
            MinMax { min, max }
        );
        mesh_primitives.push((
            indices_accessor,
            mesh.texture_index,
        ));
    }
    let vertex_attribute_str = {
        let attribute_pairs = vertex_attributes.attribute_pairs();
        let mut attributes = Vec::with_capacity(attribute_pairs.len());
        for (name, accessor) in attribute_pairs {
            attributes.push(format!(r#"            "{}" : {}"#, name, accessor));
        }
        let attributes = attributes.join(",\n");
        attributes
    };

    // Create primitives
    let mut primitives = Vec::with_capacity(converted_model.meshes.len());
    for (indices, material) in mesh_primitives {
        primitives.push(format!(
            r#"         {{
            "attributes" : {{
{}
            }},
            "indices" : {},
            "material" : {}
        }}"#,
            vertex_attribute_str, indices.0, material
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

    // DEBUG
    if let Some(log) = &mut log {
        writeln!(log, "Bones to Nodes").unwrap();
        for bone in 0..file.bones.len() {
            writeln!(log, "  {} -> {}", bone, *bone_to_node.get(&bone).unwrap()).unwrap();
        }

        writeln!(log, "Bone Transforms").unwrap();
        for bone in 0..file.bones.len() {
            writeln!(log, "  {}:", bone).unwrap();
            let transform = &local_bone_component_transforms[bone];
            writeln!(log, "    Translation:   {}", transform.translation).unwrap();
            writeln!(log, "    Rotation:      {}", transform.rotation).unwrap();
        }

        writeln!(log, "Animations").unwrap();
        for animation in &gltf_animations {
            writeln!(log, "  {}", animation.name).unwrap();
            for channel in &animation.channels {
                let mut name = None;
                for (bone, node) in &bone_to_node {
                    if *node == channel.node_index {
                        name = Some(&bone_names[*bone]);
                    }
                }
                let name = name.unwrap();
                //writeln!(log, "    Node {}:", channel.node_index);
                writeln!(log, "    Node {}  ({}):", name, channel.node_index).unwrap();
                writeln!(log, "      ({:?})", channel.target).unwrap();
                write!(log, "      ").unwrap();
                for data in &channel.values {
                    match channel.target {
                        GltfTargetPath::Translation => {
                            write!(log, "{}, ", data).unwrap();
                        }
                        GltfTargetPath::Rotation => {
                            let data = quat_from_euler(*data);
                            write!(log, "{}, ", data).unwrap();
                        }
                    }
                }
                writeln!(log).unwrap();
            }
        }
    }

    // Create animation data slices
    let mut gltf_animation_strs = Vec::with_capacity(gltf_animations.len());
    for animation in gltf_animations {
        let mut channels = Vec::with_capacity(animation.channels.len());
        let mut samplers = Vec::with_capacity(animation.channels.len());
        for (i, animation_data) in animation.channels.iter().enumerate() {
            channels.push(format!(
                r#"           {{
                "sampler" : {},
                "target" : {{
                    "node" : {},
                    "path" : "{}"
                }}
            }}"#,
                i,
                animation_data.node_index,
                animation_data.target.get_gltf_str()
            ));
            // TODO: Consolodate timestamps
            let input_accessor_index = {
                let pair = buffer_writer.create_view_and_accessor_with_min_max(&animation_data.timestamps, None);
                pair.accessor.0
            };

            let output_accessor_index = match animation_data.target {
                GltfTargetPath::Translation => {
                    let pair = buffer_writer.create_view_and_accessor(&animation_data.values, None);
                    pair.accessor.0
                }
                GltfTargetPath::Rotation => {
                    let quats: Vec<_> = animation_data
                        .values
                        .iter()
                        .map(|x| {
                            ComponentTransform::new(Vec3::ZERO, *x)
                                .get_rotation_quat()
                                .to_vec4()
                        })
                        .collect();
                    let pair = buffer_writer.create_view_and_accessor(&quats, None);
                    pair.accessor.0
                }
            };

            samplers.push(format!(
                r#"           {{
                "input" : {},
                "interpolation" : "LINEAR",
                "output" : {}
            }}"#,
                input_accessor_index, output_accessor_index
            ));
        }
        let channels = channels.join(",\n");
        let samplers = samplers.join(",\n");

        gltf_animation_strs.push(format!(
            r#"        {{
            "channels" : [
{}
            ],
            "name" : "{}",
            "samplers" : [
{}
            ]
        }}"#,
            channels, &animation.name, samplers
        ));
    }
    let gltf_animations = gltf_animation_strs.join(",\n");

    // Create buffer views and accessors
    let buffer_views = buffer_writer.write_buffer_views();
    let gltf_accessors = buffer_writer.write_accessors();
    let buffer_views = buffer_views.join(",\n");
    let accessors = gltf_accessors.join(",\n");

    // Build skin
    let mut joints = Vec::with_capacity(file.bones.len());
    for i in 0..file.bones.len() {
        let node = *bone_to_node.get(&i).unwrap();
        joints.push(format!("               {}", node));
    }
    let joints = joints.join(",\n");
    let skin = format!(
        r#"          {{
                "inverseBindMatrices" : {},
                "joints" : [
{}
                ]
            }}"#,
            inverse_bind_matrices_pair.accessor.0, joints
    );

    let gltf_text = format!(
        r#"{{
        "scene" : 0,
        "scenes" : [
            {{
                "nodes" : [ {} ]
            }}
        ],
        "nodes" : [
{}
        ],

        "skins" : [
{}
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

            "animations" : [
{}
            ],

            "asset" : {{
                "version" : "2.0"
            }}
        }}
    "#,
        scene_root,
        nodes,
        skin,
        primitives,
        materials,
        textures,
        images,
        buffer_writer.buffer_len(),
        buffer_views,
        accessors,
        gltf_animations
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
    std::fs::write(data_path, buffer_writer.to_inner())?;

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

fn write_and_convert_channel(base: &mut Vec3, channel: VectorChannel, value: f32) {
    match channel {
        // HL X => GLTF Z
        VectorChannel::X => base.z = value,
        // HL Y => GLTF X
        VectorChannel::Y => base.x = value,
        // HL Z => GLTF Y
        VectorChannel::Z => base.y = value,
    }
}

fn process_indexed_triangles(
    model: &MdlModel,
    texture_width: f32,
    texture_height: f32,
    triverts: &[MdlMeshVertex],
    world_bone_transforms: &[Mat4],
    indices: &mut Vec<u32>,
    vertices: &mut Vec<SkinnedVertex>,
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
            let joints = [
                // We don't use bone_to_node because we need the joint index.
                // Because of how we encode the joints, they match the bone index.
                bone_index, 0, 0, 0,
            ];
            let weights = [1.0, 0.0, 0.0, 0.0];

            let index = vertices.len();
            vertices.push(SkinnedVertex {
                pos,
                normal,
                uv,
                joints,
                weights,
            });
            vertex_map.insert(*trivert, index);
            index
        };
        indices.push(index as u32);
    };

    for trivert in triverts {
        process_trivert(trivert);
    }
}

fn add_accessor<T: BufferType>(
    accessors: &mut Vec<(usize, usize, usize, Option<(String, String)>)>,
    buffer_view_index: usize,
    byte_offset: usize,
    len: usize,
) -> usize {
    let index = accessors.len();
    accessors.push((buffer_view_index, byte_offset, len, None));
    index
}

fn add_accessor_with_min_max<T: BufferTypeMinMax>(
    accessors: &mut Vec<(usize, usize, usize, Option<(String, String)>)>,
    buffer_view_index: usize,
    byte_offset: usize,
    len: usize,
    min: T,
    max: T,
) -> usize {
    let index = accessors.len();
    accessors.push((
        buffer_view_index,
        byte_offset,
        len,
        Some((min.write_value(), max.write_value())),
    ));
    index
}

fn process_animation(
    base: &mut Vec3,
    target: GltfTargetPath,
    animations: &[(VectorChannel, usize)],
    channels: &[BoneChannelAnimation],
    target_node: usize,
    fps: f32,
) -> Option<GltfChannelAnimation> {
    if !animations.is_empty() {
        let animation_length = channels[animations.first().unwrap().1].keyframes.len();
        assert!(animations
            .iter()
            .all(|(_, index)| channels[*index].keyframes.len() == animation_length));

        let mut new_keyframes = Vec::with_capacity(animation_length);
        for i in 0..animation_length {
            for (vec_channel, channel_index) in animations {
                let channel = &channels[*channel_index];
                let value = channel.keyframes[i];
                // NOTE: We are converting from Half-Life coordinates to GLTF
                //       See convert_coordinates for more details.
                write_and_convert_channel(base, *vec_channel, value);
            }
            new_keyframes.push(*base);
        }

        let mut timestamps = Vec::with_capacity(animation_length);
        let seconds_per_frame = 1.0 / fps;
        //let seconds_per_frame = seconds_per_frame * 4.0;
        for i in 0..animation_length {
            timestamps.push(i as f32 * seconds_per_frame);
        }

        Some(GltfChannelAnimation {
            node_index: target_node,
            target,
            values: new_keyframes,
            timestamps,
        })
    } else {
        None
    }
}
