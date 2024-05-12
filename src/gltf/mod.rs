use std::ops::Range;

use glam::Vec3;

use self::buffer::BufferWriter;

mod animation;
pub mod bsp;
mod buffer;
mod export;
pub mod mdl;
mod node;
mod skin;
mod transform;

trait VertexAttributesSource {
    fn attribute_pairs(&self) -> Vec<(&'static str, usize)>;
}

trait Vertex: Sized {
    fn write_slices(
        writer: &mut BufferWriter,
        vertices: &[Self],
    ) -> Box<dyn VertexAttributesSource>;
}

struct Mesh {
    texture_index: usize,
    indices_range: Range<usize>,
}

struct Model<V: Vertex> {
    indices: Vec<u32>,
    vertices: Vec<V>,
    meshes: Vec<Mesh>,
}

#[derive(Debug)]
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
    timestamps: Vec<f32>,
}

fn add_and_get_index<T>(vec: &mut Vec<T>, value: T) -> usize {
    let index = vec.len();
    vec.push(value);
    index
}

trait AsStr {
    fn as_str(&self) -> &'static str;
}

#[macro_export]
macro_rules! enum_with_str {
    ($name:ident { $($var_name:ident : $str_value:literal),* $(,)* }) => {
        #[derive(Copy, Clone, Debug)]
        pub enum $name {
            $(
                $var_name,
            )*
        }

        impl crate::gltf::AsStr for $name {
            fn as_str(&self) -> &'static str {
                match self {
                    $(
                        $name::$var_name => $str_value,
                    )*
                }
            }
        }
    };
}
