use glam::{Mat4, Vec3, Vec4};

use super::add_and_get_index;

pub trait BufferType: Sized {
    const COMPONENT_TY: AccessorComponentType;
    const TY: AccessorDataType;
    fn to_bytes(&self) -> Vec<u8>;
    fn stride() -> Option<usize>;
}

pub trait BufferTypeMinMax: BufferType {
    const MIN: Self;
    const MAX: Self;
    fn data_max(&self, other: &Self) -> Self;
    fn data_min(&self, other: &Self) -> Self;
    fn write_value(&self) -> String;
}

pub trait BufferTypeEx: Sized {
    fn find_min_max(data: &[Self]) -> (Self, Self);
}

pub struct MinMax<T> {
    pub min: T,
    pub max: T,
}

impl<T: BufferTypeMinMax> BufferTypeEx for T {
    fn find_min_max(data: &[Self]) -> (Self, Self) {
        let mut max = T::MIN;
        let mut min = T::MAX;
        for face in data {
            max = face.data_max(&max);
            min = face.data_min(&min);
        }
        (min, max)
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct BufferViewIndex(pub usize);
#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct AccessorIndex(pub usize);

#[derive(Copy, Clone, Debug)]
pub struct BufferViewAndAccessorPair {
    pub view: BufferViewIndex,
    pub accessor: AccessorIndex,
}

impl BufferViewAndAccessorPair {
    pub fn new(view: BufferViewIndex, accessor: AccessorIndex) -> Self {
        Self { view, accessor }
    }
}

pub struct BufferWriter {
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

    pub fn create_view<T: BufferType + Copy>(
        &mut self,
        data: &[T],
        target: Option<BufferViewTarget>,
    ) -> BufferViewIndex {
        let offset = self.buffer.len();
        for item in data {
            let mut bytes = item.to_bytes();
            self.buffer.append(&mut bytes);
        }
        let byte_len = self.buffer.len() - offset;
        let stride = T::stride();
        let view_index = add_and_get_index(
            &mut self.views,
            BufferView {
                buffer: 0,
                byte_offset: offset,
                byte_len,
                stride,
                target,
            },
        );
        BufferViewIndex(view_index)
    }

    pub fn create_accessor<T: BufferType + Copy>(
        &mut self,
        view_index: BufferViewIndex,
        byte_offset: usize,
        len: usize,
    ) -> AccessorIndex {
        AccessorIndex(add_and_get_index(
            &mut self.accessors,
            Accessor {
                buffer_view: view_index.0,
                byte_offset,
                count: len,
                component_ty: T::COMPONENT_TY,
                ty: T::TY,
                min_max: None,
            },
        ))
    }

    pub fn create_accessor_with_min_max<T: BufferTypeMinMax + Copy>(
        &mut self,
        view_index: BufferViewIndex,
        byte_offset: usize,
        len: usize,
        min_max: MinMax<T>,
    ) -> AccessorIndex {
        AccessorIndex(add_and_get_index(
            &mut self.accessors,
            Accessor {
                buffer_view: view_index.0,
                byte_offset,
                count: len,
                component_ty: T::COMPONENT_TY,
                ty: T::TY,
                min_max: Some(MinMax {
                    min: min_max.min.write_value(),
                    max: min_max.max.write_value(),
                }),
            },
        ))
    }

    pub fn create_view_and_accessor<T: BufferType + Copy>(
        &mut self,
        data: &[T],
        target: Option<BufferViewTarget>,
    ) -> BufferViewAndAccessorPair {
        let view = self.create_view(data, target);
        let accessor = self.create_accessor::<T>(view, 0, data.len());
        BufferViewAndAccessorPair::new(view, accessor)
    }

    pub fn create_view_and_accessor_with_min_max<T: BufferTypeMinMax + Copy>(
        &mut self,
        data: &[T],
        target: Option<BufferViewTarget>,
    ) -> BufferViewAndAccessorPair {
        let view = self.create_view(data, target);
        let mut max = T::MIN;
        let mut min = T::MAX;
        for item in data {
            max = item.data_max(&max);
            min = item.data_min(&min);
        }
        let min_max = MinMax { min, max };
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
                view.buffer, view.byte_offset, view.byte_len, extras
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
                    extras.push(format!(
                        r#"                    "max" : {},
                    "min" : {}"#,
                        min_max.max, min_max.min
                    ));
                }
                if extras.is_empty() {
                    "".to_owned()
                } else {
                    let extras = extras.join(",\n");
                    format!(",\n{}", extras)
                }
            };

            accessors.push(format!(
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
            ))
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
pub enum BufferViewTarget {
    ArrayBuffer = 34962,
    ElementArrayBuffer = 34963,
}

// https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#accessor-data-types
#[derive(Copy, Clone, Debug)]
pub enum AccessorComponentType {
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
        pub enum $name {
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

impl BufferType for u16 {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::UnsignedShort;
    const TY: AccessorDataType = AccessorDataType::Scalar;

    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    fn stride() -> Option<usize> {
        None
    }
}

impl BufferTypeMinMax for u16 {
    const MIN: Self = u16::MIN;
    const MAX: Self = u16::MAX;

    fn data_max(&self, other: &Self) -> Self {
        (*self).max(*other)
    }

    fn data_min(&self, other: &Self) -> Self {
        (*self).min(*other)
    }

    fn write_value(&self) -> String {
        format!(" [ {} ]", self)
    }
}

impl BufferType for u32 {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::UnsignedInt;
    const TY: AccessorDataType = AccessorDataType::Scalar;

    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    fn stride() -> Option<usize> {
        None
    }
}

impl BufferTypeMinMax for u32 {
    const MIN: Self = u32::MIN;
    const MAX: Self = u32::MAX;

    fn data_max(&self, other: &Self) -> Self {
        (*self).max(*other)
    }

    fn data_min(&self, other: &Self) -> Self {
        (*self).min(*other)
    }

    fn write_value(&self) -> String {
        format!(" [ {} ]", self)
    }
}

impl BufferType for f32 {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::Float;
    const TY: AccessorDataType = AccessorDataType::Scalar;

    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    fn stride() -> Option<usize> {
        None
    }
}

impl BufferTypeMinMax for f32 {
    const MIN: Self = f32::MIN;
    const MAX: Self = f32::MAX;

    fn data_max(&self, other: &Self) -> Self {
        (*self).max(*other)
    }

    fn data_min(&self, other: &Self) -> Self {
        (*self).min(*other)
    }

    fn write_value(&self) -> String {
        format!(" [ {} ]", self)
    }
}

impl BufferType for u8 {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::UnsignedByte;
    const TY: AccessorDataType = AccessorDataType::Scalar;

    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    fn stride() -> Option<usize> {
        None
    }
}

impl BufferTypeMinMax for u8 {
    const MIN: Self = u8::MIN;
    const MAX: Self = u8::MAX;

    fn data_max(&self, other: &Self) -> Self {
        (*self).max(*other)
    }

    fn data_min(&self, other: &Self) -> Self {
        (*self).min(*other)
    }

    fn write_value(&self) -> String {
        format!(" [ {} ]", self)
    }
}

impl BufferType for [f32; 2] {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::Float;
    const TY: AccessorDataType = AccessorDataType::Vec2;

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(self));
        for value in self {
            let mut data = value.to_le_bytes().to_vec();
            bytes.append(&mut data);
        }
        bytes
    }

    fn stride() -> Option<usize> {
        Some(std::mem::size_of::<Self>())
    }
}

impl BufferTypeMinMax for [f32; 2] {
    const MIN: Self = [f32::MIN, f32::MIN];
    const MAX: Self = [f32::MAX, f32::MAX];

    fn data_max(&self, other: &Self) -> Self {
        [self[0].data_max(&other[0]), self[1].data_max(&other[1])]
    }

    fn data_min(&self, other: &Self) -> Self {
        [self[0].data_min(&other[0]), self[1].data_min(&other[1])]
    }

    fn write_value(&self) -> String {
        format!(" [ {}, {} ]", self[0], self[1])
    }
}

impl BufferType for [f32; 3] {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::Float;
    const TY: AccessorDataType = AccessorDataType::Vec3;

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(self));
        for value in self {
            let mut data = value.to_le_bytes().to_vec();
            bytes.append(&mut data);
        }
        bytes
    }

    fn stride() -> Option<usize> {
        Some(std::mem::size_of::<Self>())
    }
}

impl BufferTypeMinMax for [f32; 3] {
    const MIN: Self = [f32::MIN, f32::MIN, f32::MIN];
    const MAX: Self = [f32::MAX, f32::MAX, f32::MAX];

    fn data_max(&self, other: &Self) -> Self {
        [
            self[0].data_max(&other[0]),
            self[1].data_max(&other[1]),
            self[2].data_max(&other[2]),
        ]
    }

    fn data_min(&self, other: &Self) -> Self {
        [
            self[0].data_min(&other[0]),
            self[1].data_min(&other[1]),
            self[2].data_min(&other[2]),
        ]
    }

    fn write_value(&self) -> String {
        format!(" [ {}, {}, {} ]", self[0], self[1], self[2])
    }
}

impl BufferType for [f32; 4] {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::Float;
    const TY: AccessorDataType = AccessorDataType::Vec4;

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(self));
        for value in self {
            let mut data = value.to_le_bytes().to_vec();
            bytes.append(&mut data);
        }
        bytes
    }

    fn stride() -> Option<usize> {
        Some(std::mem::size_of::<Self>())
    }
}

impl BufferTypeMinMax for [f32; 4] {
    const MIN: Self = [f32::MIN, f32::MIN, f32::MIN, f32::MIN];
    const MAX: Self = [f32::MAX, f32::MAX, f32::MAX, f32::MAX];

    fn data_max(&self, other: &Self) -> Self {
        [
            self[0].data_max(&other[0]),
            self[1].data_max(&other[1]),
            self[2].data_max(&other[2]),
            self[3].data_max(&other[3]),
        ]
    }

    fn data_min(&self, other: &Self) -> Self {
        [
            self[0].data_min(&other[0]),
            self[1].data_min(&other[1]),
            self[2].data_min(&other[2]),
            self[3].data_min(&other[3]),
        ]
    }

    fn write_value(&self) -> String {
        format!(" [ {}, {}, {}, {} ]", self[0], self[1], self[2], self[3])
    }
}

impl BufferType for [u8; 4] {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::UnsignedByte;
    const TY: AccessorDataType = AccessorDataType::Vec4;

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(self));
        for value in self {
            let mut data = value.to_le_bytes().to_vec();
            bytes.append(&mut data);
        }
        bytes
    }

    fn stride() -> Option<usize> {
        Some(std::mem::size_of::<Self>())
    }
}

impl BufferTypeMinMax for [u8; 4] {
    const MIN: Self = [u8::MIN, u8::MIN, u8::MIN, u8::MIN];
    const MAX: Self = [u8::MAX, u8::MAX, u8::MAX, u8::MAX];

    fn data_max(&self, other: &Self) -> Self {
        [
            self[0].data_max(&other[0]),
            self[1].data_max(&other[1]),
            self[2].data_max(&other[2]),
            self[3].data_max(&other[3]),
        ]
    }

    fn data_min(&self, other: &Self) -> Self {
        [
            self[0].data_min(&other[0]),
            self[1].data_min(&other[1]),
            self[2].data_min(&other[2]),
            self[3].data_min(&other[3]),
        ]
    }

    fn write_value(&self) -> String {
        format!(" [ {}, {}, {}, {} ]", self[0], self[1], self[2], self[3])
    }
}

impl BufferType for Mat4 {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::Float;
    const TY: AccessorDataType = AccessorDataType::Mat4;

    fn to_bytes(&self) -> Vec<u8> {
        let array = self.to_cols_array();
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(&array));
        for value in array {
            let mut data = value.to_le_bytes().to_vec();
            bytes.append(&mut data);
        }
        bytes
    }

    fn stride() -> Option<usize> {
        None
    }
}

impl BufferType for Vec3 {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::Float;
    const TY: AccessorDataType = AccessorDataType::Vec3;

    fn to_bytes(&self) -> Vec<u8> {
        let array = self.to_array();
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(&array));
        for value in array {
            let mut data = value.to_le_bytes().to_vec();
            bytes.append(&mut data);
        }
        bytes
    }

    fn stride() -> Option<usize> {
        None
    }
}

impl BufferType for Vec4 {
    const COMPONENT_TY: AccessorComponentType = AccessorComponentType::Float;
    const TY: AccessorDataType = AccessorDataType::Vec4;

    fn to_bytes(&self) -> Vec<u8> {
        let array = self.to_array();
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(&array));
        for value in array {
            let mut data = value.to_le_bytes().to_vec();
            bytes.append(&mut data);
        }
        bytes
    }

    fn stride() -> Option<usize> {
        None
    }
}
