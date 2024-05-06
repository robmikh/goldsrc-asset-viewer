use glam::Mat4;

pub mod export;
mod transform;

const ELEMENT_ARRAY_BUFFER: usize = 34963;
const ARRAY_BUFFER: usize = 34962;

trait BufferType: Sized {
    const COMPONENT_TY: usize;
    const TY: &'static str;
    fn to_bytes(&self) -> Vec<u8>;
    fn stride() -> Option<usize>;
}

trait BufferTypeMinMax: BufferType {
    const MIN: Self;
    const MAX: Self;
    fn data_max(&self, other: &Self) -> Self;
    fn data_min(&self, other: &Self) -> Self;
    fn write_value(&self) -> String;
}

trait BufferTypeEx: Sized {
    fn find_min_max(data: &[Self]) -> (Self, Self);
}

struct MinMax<T> {
    min: T,
    max: T,
}

struct BufferSlice<T> {
    offset: usize,
    byte_len: usize,
    min_max: Option<MinMax<T>>,
    target: Option<usize>,
}

impl<T: BufferType + Copy> BufferSlice<T> {
    pub fn record(buffer: &mut Vec<u8>, data: &[T], target: usize) -> Self {
        let offset = buffer.len();
        for face in data {
            let mut face_bytes = face.to_bytes();
            buffer.append(&mut face_bytes);
        }
        let byte_len = buffer.len() - offset;
        Self {
            offset,
            byte_len,
            min_max: None,
            target: Some(target),
        }
    }
    pub fn record_without_target(buffer: &mut Vec<u8>, data: &[T]) -> Self {
        let offset = buffer.len();
        for face in data {
            let mut face_bytes = face.to_bytes();
            buffer.append(&mut face_bytes);
        }
        let byte_len = buffer.len() - offset;
        Self {
            offset,
            byte_len,
            min_max: None,
            target: None,
        }
    }

    pub fn get_min_max_values(&self) -> Option<(T, T)> {
        if let Some(min_max) = self.min_max.as_ref() {
            Some((min_max.min, min_max.max))
        } else {
            None
        }
    }
}

impl<T: BufferTypeMinMax + Copy> BufferSlice<T> {
    pub fn record_with_min_max(buffer: &mut Vec<u8>, data: &[T], target: usize) -> Self {
        let offset = buffer.len();
        let mut max = T::MIN;
        let mut min = T::MAX;
        for face in data {
            max = face.data_max(&max);
            min = face.data_min(&min);
            let mut face_bytes = face.to_bytes();
            buffer.append(&mut face_bytes);
        }
        let byte_len = buffer.len() - offset;
        Self {
            offset,
            byte_len,
            min_max: Some(MinMax { min, max }),
            target: Some(target),
        }
    }
}

pub trait BufferViewAndAccessorSource {
    fn write_buffer_view(&self) -> String;
    fn write_accessor(&self, view_index: usize, byte_offset: usize, count: usize) -> String;
    fn write_accessor_with_min_max(
        &self,
        view_index: usize,
        byte_offset: usize,
        count: usize,
        min: &str,
        max: &str,
    ) -> String;
}

impl<T: BufferType> BufferViewAndAccessorSource for BufferSlice<T> {
    fn write_buffer_view(&self) -> String {
        let extras = {
            let mut extras = Vec::with_capacity(2);
            if let Some(stride) = T::stride() {
                extras.push(format!(r#""byteStride" : {}"#, stride));
            }
            if let Some(target) = self.target {
                extras.push(format!(r#""target" : {}"#, target));
            }
            if extras.is_empty() {
                "".to_owned()
            } else {
                let extras = extras.join(",\n");
                format!(",\n{}", extras)
            }
        };
        format!(
            r#"   {{
        "buffer" : {},
        "byteOffset" : {},
        "byteLength" : {}{}
    }}"#,
            0, self.offset, self.byte_len, extras
        )
    }

    fn write_accessor(&self, view_index: usize, byte_offset: usize, count: usize) -> String {
        format!(
            r#"   {{
            "bufferView" : {},
            "byteOffset" : {},
            "componentType" : {},
            "count" : {},
            "type" : "{}"
        }}"#,
            view_index,
            byte_offset,
            T::COMPONENT_TY,
            count,
            T::TY,
        )
    }

    fn write_accessor_with_min_max(
        &self,
        view_index: usize,
        byte_offset: usize,
        count: usize,
        min: &str,
        max: &str,
    ) -> String {
        format!(
            r#"   {{
            "bufferView" : {},
            "byteOffset" : {},
            "componentType" : {},
            "count" : {},
            "type" : "{}",
            "max" : {},
            "min" : {}
        }}"#,
            view_index,
            byte_offset,
            T::COMPONENT_TY,
            count,
            T::TY,
            max,
            min
        )
    }
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

impl BufferType for u16 {
    const COMPONENT_TY: usize = 5123;
    const TY: &'static str = "SCALAR";

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
    const COMPONENT_TY: usize = 5125;
    const TY: &'static str = "SCALAR";

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
    const COMPONENT_TY: usize = 5126;
    const TY: &'static str = "SCALAR";

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
    const COMPONENT_TY: usize = 5121;
    const TY: &'static str = "SCALAR";

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
    const COMPONENT_TY: usize = 5126;
    const TY: &'static str = "VEC2";

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
    const COMPONENT_TY: usize = 5126;
    const TY: &'static str = "VEC3";

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
    const COMPONENT_TY: usize = 5126;
    const TY: &'static str = "VEC4";

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
    const COMPONENT_TY: usize = 5121;
    const TY: &'static str = "VEC4";

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
    const COMPONENT_TY: usize = 5126;
    const TY: &'static str = "MAT4";

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
