pub mod export;

const ELEMENT_ARRAY_BUFFER: usize = 34963;
const ARRAY_BUFFER: usize = 34962;

trait BufferType: Sized {
    const COMPONENT_TY: usize;
    const TY: &'static str;
    const MIN: Self;
    const MAX: Self;
    fn to_bytes(&self) -> Vec<u8>;
    fn data_max(&self, other: &Self) -> Self;
    fn data_min(&self, other: &Self) -> Self;
    fn stride() -> Option<usize>;
    fn write_value(&self) -> String;
}

trait BufferTypeEx: Sized {
    fn find_min_max(data: &[Self]) -> (Self, Self);
}

struct BufferSlice<T> {
    offset: usize,
    byte_len: usize,
    min: T,
    max: T,
    target: usize,
}

impl<T: BufferType + Copy> BufferSlice<T> {
    pub fn record(buffer: &mut Vec<u8>, data: &[T], target: usize) -> Self {
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
            min,
            max,
            target,
        }
    }

    pub fn get_min_max_values(&self) -> (T, T) {
        (self.min, self.max)
    }
}

pub trait BufferViewAndAccessorSource {
    fn write_buffer_view(&self) -> String;
    fn write_accessor(
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
        if let Some(stride) = T::stride() {
            format!(
                r#"   {{
                "buffer" : {},
                "byteOffset" : {},
                "byteLength" : {},
                "byteStride" : {},
                "target" : {}
            }}"#,
                0, self.offset, self.byte_len, stride, self.target
            )
        } else {
            format!(
                r#"   {{
            "buffer" : {},
            "byteOffset" : {},
            "byteLength" : {},
            "target" : {}
        }}"#,
                0, self.offset, self.byte_len, self.target
            )
        }
    }

    fn write_accessor(
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

impl<T: BufferType> BufferTypeEx for T {
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
    const MIN: Self = u16::MIN;
    const MAX: Self = u16::MAX;

    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    fn data_max(&self, other: &Self) -> Self {
        (*self).max(*other)
    }

    fn data_min(&self, other: &Self) -> Self {
        (*self).min(*other)
    }

    fn stride() -> Option<usize> {
        None
    }

    fn write_value(&self) -> String {
        format!(" [ {} ]", self)
    }
}

impl BufferType for u32 {
    const COMPONENT_TY: usize = 5125;
    const TY: &'static str = "SCALAR";
    const MIN: Self = u32::MIN;
    const MAX: Self = u32::MAX;

    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    fn data_max(&self, other: &Self) -> Self {
        (*self).max(*other)
    }

    fn data_min(&self, other: &Self) -> Self {
        (*self).min(*other)
    }

    fn stride() -> Option<usize> {
        None
    }

    fn write_value(&self) -> String {
        format!(" [ {} ]", self)
    }
}

impl BufferType for f32 {
    const COMPONENT_TY: usize = 5126;
    const TY: &'static str = "SCALAR";
    const MIN: Self = f32::MIN;
    const MAX: Self = f32::MAX;

    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    fn data_max(&self, other: &Self) -> Self {
        (*self).max(*other)
    }

    fn data_min(&self, other: &Self) -> Self {
        (*self).min(*other)
    }

    fn stride() -> Option<usize> {
        None
    }

    fn write_value(&self) -> String {
        format!(" [ {} ]", self)
    }
}

impl BufferType for [f32; 2] {
    const COMPONENT_TY: usize = 5126;
    const TY: &'static str = "VEC2";
    const MIN: Self = [f32::MIN, f32::MIN];
    const MAX: Self = [f32::MAX, f32::MAX];

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(self));
        for value in self {
            let mut data = value.to_le_bytes().to_vec();
            bytes.append(&mut data);
        }
        bytes
    }

    fn data_max(&self, other: &Self) -> Self {
        [self[0].data_max(&other[0]), self[1].data_max(&other[1])]
    }

    fn data_min(&self, other: &Self) -> Self {
        [self[0].data_min(&other[0]), self[1].data_min(&other[1])]
    }

    fn stride() -> Option<usize> {
        Some(std::mem::size_of::<Self>())
    }

    fn write_value(&self) -> String {
        format!(" [ {}, {} ]", self[0], self[1])
    }
}

impl BufferType for [f32; 3] {
    const COMPONENT_TY: usize = 5126;
    const TY: &'static str = "VEC3";
    const MIN: Self = [f32::MIN, f32::MIN, f32::MIN];
    const MAX: Self = [f32::MAX, f32::MAX, f32::MAX];

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(self));
        for value in self {
            let mut data = value.to_le_bytes().to_vec();
            bytes.append(&mut data);
        }
        bytes
    }

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

    fn stride() -> Option<usize> {
        Some(std::mem::size_of::<Self>())
    }

    fn write_value(&self) -> String {
        format!(" [ {}, {}, {} ]", self[0], self[1], self[2])
    }
}
