use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
};

use glam::{Mat4, Vec3, Vec4};
use id_tree::{
    InsertBehavior::{AsRoot, UnderNode},
    Node, TreeBuilder,
};
use mdlparser::{AnimationValue, MdlFile, MdlMeshSequenceType, MdlMeshVertex, MdlModel};

use crate::numerics::{ToVec3, ToVec4};

use super::{
    transform::ComponentTransform, BufferSlice, BufferType, BufferTypeEx, BufferTypeMinMax,
    BufferViewAndAccessorSource, ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER,
};

struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    joints: [u8; 4],
    weights: [f32; 4],
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

struct Animation {
    name: String,
    bone_animations: Vec<BoneAnimation>,
}

struct BoneAnimation {
    target: usize,
    channels: Vec<BoneChannelAnimation>,
}

struct BoneChannelAnimation {
    target: ComponentTransformTarget,
    keyframes: Vec<f32>,
}

#[derive(Copy, Clone, Debug)]
enum ComponentTransformTarget {
    Translation(VectorChannel),
    Rotation(VectorChannel),
}

#[derive(Copy, Clone, Debug)]
enum VectorChannel {
    X,
    Y,
    Z
}

impl ComponentTransformTarget {
    fn from_index(index: usize) -> Self {
        if index < 3 {
            ComponentTransformTarget::Translation(VectorChannel::from_index(index))
        } else if index <= 5 {
            ComponentTransformTarget::Rotation(VectorChannel::from_index(index))
        } else {
            panic!()
        }
    }
}

impl VectorChannel {
    fn from_index(index: usize) -> Self {
        match index {
            0 | 3 => VectorChannel::X,
            1 | 4 => VectorChannel::Y,
            2 | 5 => VectorChannel::Z,
            _ => panic!(),
        }
    }
}

enum GltfTargetPath {
    Translation,
    Rotation,
}

impl GltfTargetPath {
    fn get_gltf_str(&self) -> &str {
        match self {
            GltfTargetPath::Translation => "translation",
            GltfTargetPath::Rotation => "rotation",
        }
    }
}

struct GltfAnimation {
    channels: Vec<GltfChannelAnimation>,
    name: String,
}

struct GltfChannelAnimation {
    node_index: usize,
    target: GltfTargetPath,
    values: Vec<Vec3>,
}

fn null_terminated_bytes_to_str<'a>(bytes: &'a [u8]) -> &'a str {
    let name = std::str::from_utf8(bytes).unwrap();
    let end = name.find('\0').unwrap_or(name.len());
    let name = &name[..end];
    name
}

pub fn export<P: AsRef<Path>>(file: &MdlFile, output_path: P) -> std::io::Result<()> {
    let body_part = file.body_parts.first().unwrap();
    let model = body_part.models.first().unwrap();

    // DEBUG: Move to mdlparser
    println!("Animation Sequence Groups:");
    for group in &file.animation_sequence_groups {
        let name = null_terminated_bytes_to_str(group.name());
        let label = null_terminated_bytes_to_str(&group.label);

        println!("  {} - {}", label, name);
    }

    // DEBUG: Move to mdlparser
    let mut animations = Vec::new();
    for animated_sequence in &file.animation_sequences {
        let name = null_terminated_bytes_to_str(&animated_sequence.name);

        // TODO: Load other files
        if animated_sequence.sequence_group == 0 {
            //println!("  {}", name);

            let sequence_group = &file.animation_sequence_groups[animated_sequence.sequence_group as usize];
            let animation_offset = sequence_group.unused_2 as usize + animated_sequence.animation_offset as usize;
            let animation_data = &file.raw_data()[animation_offset..];

            let mut bone_animations = Vec::new();
            for i in 0..file.bones.len() {
                //println!("    Bone {}:", i);
                let offset = i * 12;
                let mut offsets = [0u16; 6];
                for j in 0..offsets.len() {
                    let frame_offset = j * 2;
                    let data = [animation_data[offset + frame_offset], animation_data[offset + frame_offset + 1]];
                    offsets[j] = u16::from_le_bytes(data);
                }
                
                let mut channels = Vec::new();
                for (j, offset) in offsets.iter().enumerate() {
                    if *offset != 0 {
                        let anim_value = &animation_data[*offset as usize..];
                        let anim_value_ptr = anim_value.as_ptr() as *const AnimationValue;
                        let scale = file.bones[i].scale[j];
                    
                        //print!("      ");
                        let mut keyframes = Vec::new();
                        let target = ComponentTransformTarget::from_index(j);
                        for frame in 0..animated_sequence.num_frames as i32 {
                            let value = unsafe { decode_animation_frame(anim_value_ptr, frame, scale) };
                            //print!("{}:{}, ", frame, value);
                            keyframes.push(value);
                        }
                        //println!();

                        channels.push(BoneChannelAnimation {
                            target,
                            keyframes,
                        })
                    }
                }

                if !channels.is_empty() {
                    bone_animations.push(BoneAnimation {
                        target: i,
                        channels,
                    })
                }
            }

            animations.push(Animation {
                name: name.to_owned(),
                bone_animations,
            })
        }
    }

    // DEBUG
    println!("Animation Sequences:");
    for animation in &animations {
        println!("  {}", &animation.name);
        //for animation in &animation.bone_animations {
        //    for channel in &animation.channels {
        //        print!("      ");
        //        for (i, keyframe) in channel.keyframes.iter().enumerate() {
        //            print!("{}:{}, ", i, keyframe);
        //        }
        //        println!();
        //    }
        //}
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
    let nodes = nodes.join(",\n");

    // Build animations
    let mut gltf_animations = Vec::with_capacity(animations.len());
    for animation in &animations {
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
                    },
                    ComponentTransformTarget::Rotation(vec_channel) => {
                        rotation_animations.push((vec_channel, channel_index));
                    },
                }
            }

            // Use the default pose as a baseline
            let component_transform = &local_bone_component_transforms[bone_animation.target];
            let mut translation = component_transform.translation;
            let mut rotation = component_transform.rotation;
            // NOTE: We are converting from Half-Life coordinates to GLTF
            //       See convert_coordinates for more details.
            let write_channel = |base: &mut Vec3, vec_channel: VectorChannel, value: f32| match vec_channel {
                // HL X => GLTF Z
                VectorChannel::X => base.z = value,
                // HL Y => GLTF X
                VectorChannel::Y => base.x = value,
                // HL Z => GLTF Y
                VectorChannel::Z => base.y = value,
            };

            let process_animation = |base: &mut Vec3, target: GltfTargetPath, animations: &[(VectorChannel, usize)], channels: &[BoneChannelAnimation], bone_to_node: &HashMap<usize, usize>| -> Option<GltfChannelAnimation> {
                if !animations.is_empty() {
                    let animation_length = channels[animations.first().unwrap().1].keyframes.len();
                    assert!(animations.iter().all(|(_, index)| channels[*index].keyframes.len() == animation_length));
                
                    let mut new_keyframes = Vec::with_capacity(animation_length);
                    for i in 0..animation_length {
                        for (vec_channel, channel_index) in animations {
                            let channel = &channels[*channel_index];
                            let value = channel.keyframes[i];
                            write_channel(base, *vec_channel, value);
                        }
                        new_keyframes.push(*base);
                    }
    
                    Some(GltfChannelAnimation {
                        node_index: *bone_to_node.get(&target_bone).unwrap(),
                        target,
                        values: new_keyframes,
                    })
                } else {
                    None
                }
            };

            if let Some(animation) = process_animation(&mut translation, GltfTargetPath::Translation, &translate_animations, &bone_animation.channels, &bone_to_node) {
                animation_data.push(animation);
            }
            if let Some(animation) = process_animation(&mut rotation, GltfTargetPath::Rotation, &rotation_animations, &bone_animation.channels, &bone_to_node) {
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
                                &bone_to_node,
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
                                &bone_to_node,
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
    let (
        mut slices,
        vertex_positions_min_max,
        vertex_normals_min_max,
        uvs_min_max,
        joints_min_max,
        weights_min_max,
    ) = {
        let mut slices = Vec::<Box<dyn BufferViewAndAccessorSource>>::new();

        // Split out the vertex data
        let mut positions = Vec::with_capacity(converted_model.vertices.len());
        let mut normals = Vec::with_capacity(converted_model.vertices.len());
        let mut uvs = Vec::with_capacity(converted_model.vertices.len());
        let mut joints = Vec::with_capacity(converted_model.vertices.len());
        let mut weights = Vec::with_capacity(converted_model.vertices.len());
        for vertex in &converted_model.vertices {
            positions.push(vertex.pos);
            normals.push(vertex.normal);
            uvs.push(vertex.uv);
            joints.push(vertex.joints);
            weights.push(vertex.weights);
        }

        let indices_slice =
            BufferSlice::record(&mut data, &converted_model.indices, ELEMENT_ARRAY_BUFFER);
        let vertex_positions_slice =
            BufferSlice::record_with_min_max(&mut data, &positions, ARRAY_BUFFER);
        let vertex_normals_slice =
            BufferSlice::record_with_min_max(&mut data, &normals, ARRAY_BUFFER);
        let uvs_slice = BufferSlice::record_with_min_max(&mut data, &uvs, ARRAY_BUFFER);
        let joints_slice = BufferSlice::record_with_min_max(&mut data, &joints, ARRAY_BUFFER);
        let weights_slice = BufferSlice::record_with_min_max(&mut data, &weights, ARRAY_BUFFER);
        let inverse_bind_matrices_slice =
            BufferSlice::record_without_target(&mut data, &inverse_bind_transforms);

        let vertex_positions_min_max = vertex_positions_slice.get_min_max_values().unwrap();
        let vertex_normals_min_max = vertex_normals_slice.get_min_max_values().unwrap();
        let uvs_min_max = uvs_slice.get_min_max_values().unwrap();
        let joints_min_max = joints_slice.get_min_max_values().unwrap();
        let weights_min_max = weights_slice.get_min_max_values().unwrap();

        slices.push(Box::new(indices_slice));
        slices.push(Box::new(vertex_positions_slice));
        slices.push(Box::new(vertex_normals_slice));
        slices.push(Box::new(uvs_slice));
        slices.push(Box::new(joints_slice));
        slices.push(Box::new(weights_slice));
        slices.push(Box::new(inverse_bind_matrices_slice));

        (
            slices,
            vertex_positions_min_max,
            vertex_normals_min_max,
            uvs_min_max,
            joints_min_max,
            weights_min_max,
        )
    };
    let indices_slice_index = 0;
    let vertex_positions_slice_index = 1;
    let vertex_normals_slice_index = 2;
    let uvs_slice_index = 3;
    let joints_slice_index = 4;
    let weights_slice_index = 5;
    let inverse_bind_matrices_slice_index = 6;

    // Record accessors for vertex and index data
    let mut accessors = Vec::new();

    // Vertex data
    let vertex_positions_accessor = add_accessor_with_min_max(
        &mut accessors,
        vertex_positions_slice_index,
        0,
        converted_model.vertices.len(),
        vertex_positions_min_max.0,
        vertex_positions_min_max.1,
    );
    let vertex_normals_accessor = add_accessor_with_min_max(
        &mut accessors,
        vertex_normals_slice_index,
        0,
        converted_model.vertices.len(),
        vertex_normals_min_max.0,
        vertex_normals_min_max.1,
    );
    let uvs_accessor = add_accessor_with_min_max(
        &mut accessors,
        uvs_slice_index,
        0,
        converted_model.vertices.len(),
        uvs_min_max.0,
        uvs_min_max.1,
    );
    let joints_accessor = add_accessor_with_min_max(
        &mut accessors,
        joints_slice_index,
        0,
        converted_model.vertices.len(),
        joints_min_max.0,
        joints_min_max.1,
    );
    let weights_accessor = add_accessor_with_min_max(
        &mut accessors,
        weights_slice_index,
        0,
        converted_model.vertices.len(),
        weights_min_max.0,
        weights_min_max.1,
    );
    let inverse_bind_matrices_accessor = add_accessor::<Mat4>(
        &mut accessors,
        inverse_bind_matrices_slice_index,
        0,
        inverse_bind_transforms.len(),
    );

    let mut mesh_accessors = Vec::new();
    for mesh in &converted_model.meshes {
        let (min, max) = u32::find_min_max(
            &converted_model.indices[mesh.indices_range.start..mesh.indices_range.end],
        );
        let indices_accessor = add_accessor_with_min_max(
            &mut accessors,
            indices_slice_index,
            mesh.indices_range.start * std::mem::size_of::<u32>(),
            mesh.indices_range.end - mesh.indices_range.start,
            min,
            max,
        );
        mesh_accessors.push((
            indices_accessor,
            vertex_positions_accessor,
            vertex_normals_accessor,
            uvs_accessor,
            joints_accessor,
            weights_accessor,
            mesh.texture_index,
        ));
    }

    // Create primitives
    let mut primitives = Vec::with_capacity(converted_model.meshes.len());
    for (indices, positions, normals, uvs, joints, weights, material) in mesh_accessors {
        primitives.push(format!(
            r#"         {{
            "attributes" : {{
            "POSITION" : {},
            "NORMAL" : {},
            "TEXCOORD_0" : {},
            "JOINTS_0" : {},
            "WEIGHTS_0" : {}
            }},
            "indices" : {},
            "material" : {}
        }}"#,
            positions, normals, uvs, joints, weights, indices, material
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

    // Create animation data slices
    let mut gltf_animation_strs = Vec::with_capacity(gltf_animations.len());
    for animation in gltf_animations {
        let mut channels = Vec::with_capacity(animation.channels.len());
        let mut samplers = Vec::with_capacity(animation.channels.len());
        for (i, animation_data) in animation.channels.iter().enumerate() {
            channels.push(format!(r#"           {{
                "sampler" : {},
                "target" : {{
                    "node" : {},
                    "path" : "{}"
                }}
            }}"#, i, animation_data.node_index, animation_data.target.get_gltf_str()));
            // TODO: Determine timestamps for keyframes
            let input_accessor_index = 9001;

            let output_accessor_index = match animation_data.target {
                GltfTargetPath::Translation => {
                    let buffer_view = add_and_get_index(&mut slices, Box::new(BufferSlice::record_without_target(&mut data, &animation_data.values)));
                    add_accessor::<Vec3>(&mut accessors, buffer_view, 0, animation_data.values.len())
                },
                GltfTargetPath::Rotation => {
                    let quats: Vec<_> = animation_data.values.iter().map(|x| ComponentTransform::new(Vec3::ZERO, *x).get_rotation_quat().to_vec4()).collect();
                    let buffer_view = add_and_get_index(&mut slices, Box::new(BufferSlice::record_without_target(&mut data, &quats)));
                    add_accessor::<Vec4>(&mut accessors, buffer_view, 0, quats.len())
                },
            };

            samplers.push(format!(r#"           {{
                "input" : {},
                "interpolation" : "LINEAR",
                "output" : {}
            }}"#, input_accessor_index, output_accessor_index));
        }
        let channels = channels.join(",\n");
        let samplers = samplers.join(",\n");

        gltf_animation_strs.push(format!(r#"        {{
            "channels" : [
{}
            ],
            "name" : "{}",
            "samplers" : [
{}
            ]
        }}"#, channels, &animation.name, samplers));
        
    }
    let gltf_animations = gltf_animation_strs.join(",\n");

    // Create buffer views and accessors
    let mut buffer_views = Vec::with_capacity(slices.len());
    let mut gltf_accessors = Vec::with_capacity(slices.len());
    for slice in &slices {
        buffer_views.push(slice.write_buffer_view());
    }
    for (buffer_view_index, byte_offset, count, min_max) in accessors {
        let slice = &slices[buffer_view_index];
        if let Some((min, max)) = min_max {
            gltf_accessors.push(slice.write_accessor_with_min_max(
                buffer_view_index,
                byte_offset,
                count,
                &min,
                &max,
            ));
        } else {
            gltf_accessors.push(slice.write_accessor(buffer_view_index, byte_offset, count));
        }
    }
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
        inverse_bind_matrices_accessor, joints
    );

    let gltf_text = format!(
        r#"{{
        "scene" : 0,
        "scenes" : [
            {{
                "nodes" : [ 0, {} ]
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
        skin_root,
        nodes,
        skin,
        primitives,
        materials,
        textures,
        images,
        data.len(),
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
    bone_to_node: &HashMap<usize, usize>,
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
                *bone_to_node.get(&(bone_index as usize)).unwrap() as u8,
                0,
                0,
                0,
            ];
            let weights = [1.0, 0.0, 0.0, 0.0];

            let index = vertices.len();
            vertices.push(Vertex {
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

// TODO: This code is bananas, write a safer version 
unsafe fn decode_animation_frame(mut anim_value_ptr: *const AnimationValue, frame: i32, scale: f32) -> f32 {
    let mut k = frame;

    while (*anim_value_ptr).encoded_value.total as i32 <= k {
        k -= (*anim_value_ptr).encoded_value.total as i32;
        anim_value_ptr = anim_value_ptr.add((*anim_value_ptr).encoded_value.valid as usize + 1);
    }

    if (*anim_value_ptr).encoded_value.valid as i32 > k {
        (*anim_value_ptr.add(k as usize + 1)).value as f32 * scale
    } else {
        (*anim_value_ptr.add((*anim_value_ptr).encoded_value.valid as usize)).value as f32 * scale
    }
}

fn add_and_get_index<T>(vec: &mut Vec<T>, value: T) -> usize {
    let index = vec.len();
    vec.push(value);
    index
}