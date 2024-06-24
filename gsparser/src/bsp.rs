// Sources:
// https://developer.valvesoftware.com/wiki/BSP_(GoldSrc)

use std::collections::HashMap;

use serde::Deserialize;

use crate::mdl::null_terminated_bytes_to_str;

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
pub struct BspFace {
    pub plane: u16,
    pub plane_side: u16,
    pub first_edge: u32,
    pub edges: u16,
    pub texture_info: u16,
    pub styles: [u8; 4],
    pub lightmap_offset: i32,
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

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct BspMarkSurface(pub u16);

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct BspSurfaceEdge(pub i32);

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BspEdge {
    pub vertices: [u16; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BspVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Deserialize, Debug)]
pub struct BspTextureHeader {
    pub num_textures: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BspMipTextureHeader {
    pub name: [u8; 16],
    pub width: u32,
    pub height: u32,
    pub offsets: [u32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BspTextureInfo {
    pub s: [f32; 3],
    pub s_shift: f32,
    pub t: [f32; 3],
    pub t_shift: f32,
    pub texture_index: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BspPlane {
    pub normal: [f32; 3],
    pub dist: f32,
    pub ty: i32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BspModel {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub origin: [f32; 3],
    pub head_nodes: [i32; 4],
    pub vis_leaves: i32,
    pub first_face: i32,
    pub faces: i32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BspClipNode {
    pub plane_index: i32,
    pub children: [i16; 2],
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

    pub fn read_mark_surfaces(&self) -> &[BspMarkSurface] {
        self.read_lump(LUMP_MARKSURFACES)
    }

    pub fn read_faces(&self) -> &[BspFace] {
        self.read_lump(LUMP_FACES)
    }

    pub fn read_edges(&self) -> &[BspEdge] {
        self.read_lump(LUMP_EDGES)
    }

    pub fn read_surface_edges(&self) -> &[BspSurfaceEdge] {
        self.read_lump(LUMP_SURFEDGES)
    }

    pub fn read_vertices(&self) -> &[BspVertex] {
        self.read_lump(LUMP_VERTICES)
    }

    pub fn read_textures<'a>(&'a self) -> BspTextureReader<'a> {
        let raw_data = self.read_lump_raw(LUMP_TEXTURES);

        let header: BspTextureHeader = bincode::deserialize(&raw_data).unwrap();
        let offsets_start = std::mem::size_of::<BspTextureHeader>();
        let offsets_end =
            offsets_start + (std::mem::size_of::<i32>() * header.num_textures as usize);
        let offsets_data = &raw_data[offsets_start..offsets_end];
        let offsets = unsafe {
            let ptr = offsets_data.as_ptr() as *const i32;
            std::slice::from_raw_parts(ptr, header.num_textures as usize)
        };

        BspTextureReader::new(offsets, raw_data)
    }

    pub fn read_textures_header(&self) -> BspTextureHeader {
        let raw_data = self.read_lump_raw(LUMP_TEXTURES);

        let header: BspTextureHeader = bincode::deserialize(&raw_data).unwrap();
        header
    }

    pub fn read_texture_infos(&self) -> &[BspTextureInfo] {
        self.read_lump(LUMP_TEXINFO)
    }

    pub fn read_planes(&self) -> &[BspPlane] {
        self.read_lump(LUMP_PLANES)
    }

    pub fn read_entities(&self) -> &str {
        null_terminated_bytes_to_str(self.read_lump(LUMP_ENTITIES))
    }

    pub fn read_models(&self) -> &[BspModel] {
        self.read_lump(LUMP_MODELS)
    }

    pub fn read_clip_nodes(&self) -> &[BspClipNode] {
        self.read_lump(LUMP_CLIPNODES)
    }

    pub fn read_lighting_data(&self) -> &[u8] {
        self.read_lump_raw(LUMP_LIGHTING)
    }

    fn read_lump_raw(&self, index: usize) -> &[u8] {
        let lump_header = self.header.lumps[index];
        let start = lump_header.offset as usize;
        let end = start + lump_header.len as usize;
        let lump_data = &self.data[start..end];
        lump_data
    }

    fn read_lump<T: Sized>(&self, index: usize) -> &[T] {
        let lump_data = &self.read_lump_raw(index);
        let len = lump_data.len() / std::mem::size_of::<T>();
        unsafe {
            let ptr = lump_data.as_ptr() as *const T;
            std::slice::from_raw_parts(ptr, len)
        }
    }
}

pub trait FromValue<T: Sized + Copy>: Sized {
    fn from_value(value: T) -> Option<Self>;
}

impl BspLeaf {
    pub fn contents(&self) -> BspContents {
        BspContents::from_value(self.contents).unwrap()
    }
}

impl BspVertex {
    pub fn to_array(&self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

pub struct BspTextureReader<'a> {
    offsets: &'a [i32],
    lump_data: &'a [u8],
}

impl<'a> BspTextureReader<'a> {
    fn new(offsets: &'a [i32], lump_data: &'a [u8]) -> Self {
        Self { offsets, lump_data }
    }

    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    pub fn get(&self, index: usize) -> Option<BspMipTextureReader<'a>> {
        let data = self.get_raw_data(index)?;
        let header_ptr = data.as_ptr() as *const BspMipTextureHeader;
        let header = unsafe { header_ptr.as_ref() }?;
        Some(BspMipTextureReader::new(header, data))
    }

    fn get_raw_data(&self, index: usize) -> Option<&'a [u8]> {
        let offset = *self.offsets.get(index)? as usize;
        let end = *self
            .offsets
            .get(index + 1)
            .unwrap_or(&(self.lump_data.len() as i32)) as usize;
        let data = &self.lump_data[offset..end];
        Some(data)
    }
}

pub struct BspMipTextureReader<'a> {
    header: &'a BspMipTextureHeader,
    data: &'a [u8],
}

impl<'a> BspMipTextureReader<'a> {
    const MIP_LEVELS: [usize; 4] = [1, 2, 4, 8];

    fn new(header: &'a BspMipTextureHeader, data: &'a [u8]) -> Self {
        Self { header, data }
    }

    pub fn raw_data(&self) -> &[u8] {
        &self.data
    }

    pub fn header(&self) -> &BspMipTextureHeader {
        self.header
    }

    pub fn get_image_name(&self) -> &'a str {
        null_terminated_bytes_to_str(&self.header.name)
    }

    pub fn has_local_image_data(&self) -> bool {
        let offset = self.header.offsets[0];
        offset != 0
    }

    pub fn get_image(&self, index: usize) -> Option<BspBitmap<'a>> {
        let offset = *self.header.offsets.get(index)? as usize;
        if offset == 0 {
            return None;
        }
        let mip_level = Self::MIP_LEVELS.get(index)?;
        let width = self.header.width as usize / mip_level;
        let height = self.header.height as usize / mip_level;
        let len = width * height;
        Some(BspBitmap::new(
            width,
            height,
            &self.data[offset..offset + len],
        ))
    }

    pub fn read_palette(&self) -> BspPaletteReader<'a> {
        let last_image_offset = self.header.offsets[3] as usize;
        let mip_level = Self::MIP_LEVELS[3];
        let width = self.header.width as usize / mip_level;
        let height = self.header.height as usize / mip_level;
        let image_len = width * height;

        let palette_offset = last_image_offset + image_len + 2;
        let palette_len = 256 * 3;

        BspPaletteReader::new(&self.data[palette_offset..palette_offset + palette_len])
    }
}

pub struct BspPaletteReader<'a> {
    data: &'a [u8],
}

impl<'a> BspPaletteReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    pub fn get(&self, index: usize) -> BspPixel {
        let offset = index * 3;
        let data = &self.data[offset..offset + 3];
        BspPixel {
            r: data[0],
            g: data[1],
            b: data[2],
        }
    }
}

pub struct BspPixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub struct BspBitmap<'a> {
    width: usize,
    height: usize,
    data: &'a [u8],
}

impl<'a> BspBitmap<'a> {
    fn new(width: usize, height: usize, data: &'a [u8]) -> Self {
        Self {
            width,
            height,
            data,
        }
    }

    pub fn decode(&self, palette_reader: &BspPaletteReader<'a>) -> Vec<BspPixel> {
        todo!()
    }
}

pub struct BspEntity<'a>(pub HashMap<&'a str, &'a str>);

impl<'a> BspEntity<'a> {
    pub fn parse_entities(source: &'a str) -> Vec<BspEntity<'a>> {
        let mut entities = Vec::new();
        let mut current_entity = None;
        for line in source.lines() {
            if line == "{" {
                current_entity = Some(BspEntity(HashMap::new()));
            } else if line == "}" {
                let entity = current_entity.take().unwrap();
                entities.push(entity);
            } else {
                let entity = current_entity.as_mut().unwrap();
                let mut split = line.split("\" \"");
                let key = split.next().unwrap().trim_matches('\"');
                let value = split.next().unwrap().trim_matches('\"');
                entity.0.insert(&key, value);
            }
        }
        entities
    }
}
