use self::buffer::BufferWriter;
use std::ops::Range;

mod animation;
pub mod bsp;
mod buffer;
mod export;
pub mod mdl;
mod node;
mod skin;
mod transform;
mod material;

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
