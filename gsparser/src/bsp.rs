// Sources:
// https://developer.valvesoftware.com/wiki/BSP_(GoldSrc)

use serde::Deserialize;

macro_rules! enum_with_value {
    ($name:ident : $value_ty:ty { $($var_name:ident = $var_value:literal),* $(,)* }) => {
        #[repr($value_ty)]
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        pub enum $name {
            $(
                $var_name = $var_value,
            )*
        }

        impl FromValue<$value_ty> for $name {
            fn from_value(value: $value_ty) -> Option<Self> {
                match value {
                    $(
                        $var_value => Some($name::$var_name),
                    )*
                    _ => None
                }
            }
        }
    };
}

const LUMP_ENTITIES: usize = 0;
const LUMP_PLANES: usize = 1;
const LUMP_TEXTURES: usize = 2;
const LUMP_VERTICES: usize = 3;
const LUMP_VISIBILITY: usize = 4;
const LUMP_NODES: usize = 5;
const LUMP_TEXINFO: usize = 6;
const LUMP_FACES: usize = 7;
const LUMP_LIGHTING: usize = 8;
const LUMP_CLIPNODES: usize = 9;
const LUMP_LEAVES: usize = 10;
const LUMP_MARKSURFACES: usize = 11;
const LUMP_EDGES: usize = 12;
const LUMP_SURFEDGES: usize = 13;
const LUMP_MODELS: usize = 14;
const HEADER_LUMPS: usize = 15;

#[repr(C)]
#[derive(Copy, Clone, Deserialize, Debug)]
struct BspHeader {
    version: i32,
    lumps: [BspLumpHeader; HEADER_LUMPS],
}

#[repr(C)]
#[derive(Copy, Clone, Deserialize, Debug)]
struct BspLumpHeader {
    offset: i32,
    len: i32,
}

#[repr(C)]
#[derive(Copy, Clone, Deserialize, Debug)]
struct BspFace {
    plane: u16,
    plane_side: u16,
    first_edge: u32,
    edges: u16,
    texture_info: u16,
    styles: [u8; 4],
    lightmap_offset: i32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BspNode {
    pub plane: u32,
    pub children: [i16; 2],
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    pub first_face: u16,
    pub faces: u16,
}

enum_with_value!(BspContents : i32 {
    Empty = -1,
    Solid = -2,
    Water = -3,
    Slime = -4,
    Lava = -5,
    Sky = -6,
    Origin = -7,
    Clip = -8,
    Current0 = -9,
    Current90 = -10,
    Current180 = -11,
    Current270 = -12,
    CurrentUp = -13,
    CurrentDown = -14,
    Translucent = -15,
});

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BspLeaf {
    pub contents: i32,
    pub vis_offset: i32,
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    pub first_mark_surface: u16,
    pub mark_surfaces: u16,
    pub ambient_levels: [u8; 4],
}

// TODO: Borrow data
pub struct BspReader {
    header: BspHeader,
    data: Vec<u8>,
}

impl BspReader {
    pub fn read(data: Vec<u8>) -> Self {
        let header: BspHeader = bincode::deserialize(&data).unwrap();
        assert_eq!(header.version, 30);
        Self { header, data }
    }

    pub fn read_nodes(&self) -> &[BspNode] {
        self.read_lump(LUMP_NODES)
    }

    pub fn read_leaves(&self) -> &[BspLeaf] {
        self.read_lump(LUMP_LEAVES)
    }

    fn read_lump<T: Sized>(&self, index: usize) -> &[T] {
        let lump_header = self.header.lumps[index];
        let start = lump_header.offset as usize;
        let end = start + lump_header.len as usize;
        let lump_data = &self.data[start..end];

        let len = lump_header.len as usize / std::mem::size_of::<T>();
        unsafe {
            let ptr = lump_data.as_ptr() as *const T;
            std::slice::from_raw_parts(ptr, len)
        }
    }
}

trait FromValue<T: Sized + Copy>: Sized {
    fn from_value(value: T) -> Option<Self>;
}

impl BspLeaf {
    pub fn contents(&self) -> BspContents {
        BspContents::from_value(self.contents).unwrap()
    }
}
