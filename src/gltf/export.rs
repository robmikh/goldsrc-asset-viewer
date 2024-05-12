use crate::gltf::animation::Animations;

use super::{
    buffer::{BufferTypeEx, BufferViewTarget, BufferWriter, MinMax},
    material::MaterialData,
    node::{NodeIndex, Nodes},
    skin::Skins,
    Model, Vertex,
};

pub fn write_gltf<T: Vertex>(
    buffer_name: &str,
    buffer_writer: &mut BufferWriter,
    model: &Model<T>,
    material_data: &MaterialData,
    scene_root: NodeIndex,
    nodes: &Nodes,
    skins: &Skins,
    animations: &Animations,
) -> String {
    // Write our vertex and index data
    let indices_view =
        buffer_writer.create_view(&model.indices, Some(BufferViewTarget::ElementArrayBuffer));
    let vertex_attributes = T::write_slices(buffer_writer, &model.vertices);

    let mut mesh_primitives = Vec::new();
    for mesh in &model.meshes {
        let indices = &model.indices[mesh.indices_range.start..mesh.indices_range.end];
        let (min, max) = u32::find_min_max(indices);
        let indices_accessor = buffer_writer.create_accessor_with_min_max(
            indices_view,
            mesh.indices_range.start * std::mem::size_of::<u32>(),
            mesh.indices_range.end - mesh.indices_range.start,
            MinMax { min, max },
        );
        mesh_primitives.push((indices_accessor, mesh.texture_index));
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
    let mut primitives = Vec::with_capacity(model.meshes.len());
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

    // Write materials, textures, images, and samplers
    let materials = material_data.write_materials();
    let textures = material_data.write_textures();
    let images = material_data.write_images();
    let samplers = material_data.write_samplers();

    // Create buffer views and accessors
    let buffer_views = buffer_writer.write_buffer_views();
    let gltf_accessors = buffer_writer.write_accessors();
    let buffer_views = buffer_views.join(",\n");
    let accessors = gltf_accessors.join(",\n");

    // Write GLTF
    let mut gltf_parts = Vec::new();
    if !skins.is_empty() {
        let skins = format!(
            r#"    "skins" : [
        {}
    ]"#,
            skins.write_skins().join(",\n")
        );
        gltf_parts.push(skins);
    }
    if !animations.is_empty() {
        let animations = format!(
            r#"    "animations" : [
        {}
    ]"#,
            animations.write_animations().join(",\n")
        );
        gltf_parts.push(animations);
    }
    if !materials.is_empty() {
        let materials = format!(
            r#"    "materials" : [
{}
    ]"#,
            materials.join(",\n")
        );
        gltf_parts.push(materials);
    }
    if !textures.is_empty() {
        let textures = format!(
            r#"    "textures" : [
{}
    ]"#,
            textures.join(",\n")
        );
        gltf_parts.push(textures);
    }
    if !images.is_empty() {
        let images = format!(
            r#"    "images" : [
{}
    ]"#,
            images.join(",\n")
        );
        gltf_parts.push(images);
    }
    if !samplers.is_empty() {
        let samplers = format!(
            r#"    "samplers" : [
{}
    ]"#,
            samplers.join(",\n")
        );
        gltf_parts.push(samplers);
    }

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
        
        "meshes" : [
            {{
            "primitives" : [ 
{}
             ]
            }}
        ],

          "buffers" : [
            {{
                "uri" : "{}",
                "byteLength" : {}
            }}
          ],

            "bufferViews" : [
                {}
            ],

            "accessors" : [
                {}
            ],

{},

            "asset" : {{
                "version" : "2.0"
            }}
        }}
    "#,
        scene_root.0,
        nodes.write_nodes().join(",\n"),
        primitives,
        buffer_name,
        buffer_writer.buffer_len(),
        buffer_views,
        accessors,
        gltf_parts.join(",\n"),
    );

    gltf_text
}
