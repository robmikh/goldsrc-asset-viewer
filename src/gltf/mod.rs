use std::ops::Range;

use glam::Vec3;

use self::buffer::BufferWriter;

mod buffer;
pub mod export;
mod mdl;
mod transform;
mod node;
mod skin;

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
