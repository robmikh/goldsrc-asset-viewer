use std::ops::Range;

use glam::Vec3;

use self::buffer::{BufferType, BufferTypeMinMax, MinMax};

pub mod export;
mod transform;
mod buffer;
mod mdl;

trait VertexAttributesSource {
    fn attribute_pairs(&self) -> Vec<(&'static str, usize)>;
}

trait Vertex: Sized {
    fn write_slices(
        writer: &mut BufferWriter,
        vertices: &[Self]
    ) -> Box<dyn VertexAttributesSource>;
}

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
struct BufferViewIndex(usize);
#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
struct AccessorIndex(usize);

#[derive(Copy, Clone, Debug)]
struct BufferViewAndAccessorPair {
    pub view: BufferViewIndex,
    pub accessor: AccessorIndex,
}

impl BufferViewAndAccessorPair {
    pub fn new(view: BufferViewIndex, accessor: AccessorIndex) -> Self {
        Self { view, accessor }
    }
}

struct BufferWriter {
    buffer: Vec<u8>,
    views: Vec<BufferView>,
    accessors: Vec<Accessor>,
}

impl BufferWriter {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            views: Vec::new(),
            accessors: Vec::new(),
        }
    }

    pub fn create_view<T: BufferType + Copy>(&mut self, data: &[T], target: Option<BufferViewTarget>) -> BufferViewIndex {
        let offset = self.buffer.len();
        for item in data {
            let mut bytes = item.to_bytes();
            self.buffer.append(&mut bytes);
        }
        let byte_len = self.buffer.len() - offset;
        let stride = T::stride();
        let view_index = add_and_get_index(&mut self.views, BufferView {
            buffer: 0,
            byte_offset: offset,
            byte_len,
            stride,
            target,
        });
        BufferViewIndex(view_index)
    }

    pub fn create_accessor<T: BufferType + Copy>(&mut self, view_index: BufferViewIndex, byte_offset: usize, len: usize) -> AccessorIndex {
        AccessorIndex(add_and_get_index(&mut self.accessors, Accessor {
            buffer_view: view_index.0,
            byte_offset,
            count: len,
            component_ty: T::COMPONENT_TY,
            ty: T::TY,
            min_max: None,
        }))
    }

    pub fn create_accessor_with_min_max<T: BufferTypeMinMax + Copy>(&mut self, view_index: BufferViewIndex, byte_offset: usize, len: usize, min_max: MinMax<T>) -> AccessorIndex {
        AccessorIndex(add_and_get_index(&mut self.accessors, Accessor {
            buffer_view: view_index.0,
            byte_offset,
            count: len,
            component_ty: T::COMPONENT_TY,
            ty: T::TY,
            min_max: Some(MinMax { min: min_max.min.write_value(), max: min_max.max.write_value() }),
        }))
    }

    pub fn create_view_and_accessor<T: BufferType + Copy>(&mut self, data: &[T], target: Option<BufferViewTarget>) -> BufferViewAndAccessorPair {
        let view = self.create_view(data, target);
        let accessor = self.create_accessor::<T>(view, 0, data.len());
        BufferViewAndAccessorPair::new(view, accessor)
    }

    pub fn create_view_and_accessor_with_min_max<T: BufferTypeMinMax + Copy>(&mut self, data: &[T], target: Option<BufferViewTarget>) -> BufferViewAndAccessorPair {
        let view = self.create_view(data, target);
        let mut max = T::MIN;
        let mut min = T::MAX;
        for item in data {
            max = item.data_max(&max);
            min = item.data_min(&min);
        }
        let min_max = MinMax {
            min, max
        };
        let accessor = self.create_accessor_with_min_max(view, 0, data.len(), min_max);
        BufferViewAndAccessorPair::new(view, accessor)
    }

    pub fn write_buffer_views(&self) -> Vec<String> {
        let mut views = Vec::with_capacity(self.views.len());
        for view in &self.views {
            let extras = {
                let mut extras = Vec::with_capacity(2);
                if let Some(stride) = view.stride {
                    extras.push(format!(r#""byteStride" : {}"#, stride));
                }
                if let Some(target) = view.target {
                    extras.push(format!(r#""target" : {}"#, target as usize));
                }
                if extras.is_empty() {
                    "".to_owned()
                } else {
                    let extras = extras.join(",\n");
                    format!(",\n{}", extras)
                }
            };
            views.push(format!(
                r#"        {{
            "buffer" : {},
            "byteOffset" : {},
            "byteLength" : {}{}
        }}"#,
                0, view.byte_offset, view.byte_len, extras
            ));
        }
        views
    }

    pub fn write_accessors(&self) -> Vec<String> {
        let mut accessors = Vec::with_capacity(self.accessors.len());
        for accessor in &self.accessors {
            let extras = {
                let mut extras = Vec::with_capacity(1);
                if let Some(min_max) = accessor.min_max.as_ref() {
                    extras.push(format!(r#"                    "max" : {},
                    "min" : {}"#, min_max.max, min_max.min));
                }
                if extras.is_empty() {
                    "".to_owned()
                } else {
                    let extras = extras.join(",\n");
                    format!(",\n{}", extras)
                }
            };

            accessors.push(
                format!(
                    r#"                {{
                    "bufferView" : {},
                    "byteOffset" : {},
                    "componentType" : {},
                    "count" : {},
                    "type" : "{}"{}
                }}"#,
                    accessor.buffer_view,
                    accessor.byte_offset,
                    accessor.component_ty as usize,
                    accessor.count,
                    accessor.ty.as_str(),
                    extras
                )
            )
        }
        accessors
    }

    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    pub fn to_inner(self) -> Vec<u8> {
        self.buffer
    }
}

struct BufferView {
    buffer: usize,
    byte_offset: usize,
    byte_len: usize,
    stride: Option<usize>,
    target: Option<BufferViewTarget>,
}

#[derive(Copy, Clone, Debug)]
enum BufferViewTarget {
    ArrayBuffer = 34962,
    ElementArrayBuffer = 34963,
}

// https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#accessor-data-types
#[derive(Copy, Clone, Debug)]
enum AccessorComponentType {
    SignedByte = 5120,
    UnsignedByte = 5121,
    SignedShort = 5122,
    UnsignedShort = 5123,
    UnsignedInt = 5125,
    Float = 5126,
}

trait AsStr {
    fn as_str(&self) -> &'static str;
}

macro_rules! enum_with_str {
    ($name:ident { $($var_name:ident : $str_value:literal),* $(,)* }) => {
        #[derive(Copy, Clone, Debug)]
        enum $name {
            $(
                $var_name,
            )*
        }

        impl AsStr for $name {
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

enum_with_str!(AccessorDataType {
    Scalar: "SCALAR",
    Vec2: "VEC2",
    Vec3: "VEC3",
    Vec4: "VEC4",
    Mat2: "MAT2",
    Mat3: "MAT3",
    Mat4: "MAT4",
});

struct Accessor {
    buffer_view: usize,
    byte_offset: usize,
    count: usize,
    component_ty: AccessorComponentType,
    ty: AccessorDataType,
    min_max: Option<MinMax<String>>,
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