extern crate serde;
extern crate bincode;
extern crate image;
extern crate byteorder;

use std::io::{Cursor, BufReader, Read, Seek, SeekFrom};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str;

use byteorder::{LittleEndian, ReadBytesExt};
use serde::Deserialize;

#[derive(Clone)]
pub struct MdlTexture {
    pub name: String,
}

#[derive(Clone)]
pub struct MdlFile {
    pub name: String,
    pub textures: Vec<MdlTexture>,
    header: MdlHeader,
    raw_data: Vec<u8>,
}

#[derive(Copy, Clone, Deserialize)]
struct TextureHeader {
    name: [[u8; 8]; 8],
    flags: u32,
    width: u32,
    height: u32,
    offset: u32,
}

impl TextureHeader {
    fn name(&self) -> &[u8; 64] {
        unsafe { std::mem::transmute(&self.name) }
    }

    fn name_string(&self) -> String {
        let name = self.name();
        let name_string = String::from_utf8_lossy(name);
        let name_string = name_string.trim_matches(char::from(0));
        name_string.to_string()
    }
}

#[derive(Copy, Clone, Deserialize)]
struct MdlHeader {
    id: u32,
    version: u32,

    name: [[u8; 8]; 8],
    data_length: u32,

    eye_position: [f32; 3],
    hull_min: [f32; 3],
    hull_max: [f32; 3],

    view_bbmin: [f32; 3],
    view_bbmax: [f32; 3],

    flags: u32,

    bone_count: u32,
    bone_offset: u32,

    bone_controller_count: u32,
    bone_controller_offset :u32,

    hit_box_count: u32,
    hit_box_offset: u32,

    anim_seq_count: u32,
    anim_seq_offset: u32,

    seq_group_count: u32,
    seq_group_offset: u32,

    texture_count: u32,
    texture_offset: u32,
    texture_data_index: u32,

    skin_ref_count: u32,
    skin_families_count: u32,
    skin_offset: u32,

    body_part_count: u32,
    body_part_offset: u32,

    attachment_count: u32,
    attachment_offset: u32,

    sound_table: u32,
    sound_offset: u32,
    sound_groups: u32,
    sound_group_offset: u32,

    transitions_count: u32,
    transition_offset: u32,
}

impl MdlHeader {
    fn name(&self) -> &[u8; 64] {
        unsafe { std::mem::transmute(&self.name) }
    }

    fn name_string(&self) -> String {
        let name = self.name();
        let name_string = str::from_utf8(name).unwrap();
        let name_string = name_string.trim_matches(char::from(0));
        name_string.to_string()
    }
}

impl MdlFile {
    pub fn open(mdl_path: &str) -> MdlFile {
        let file = File::open(mdl_path).unwrap();
        let file_size = file.metadata().unwrap().len();
        let mut file = BufReader::new(file);

        let mut header : MdlHeader = bincode::deserialize_from(&mut file).unwrap();
        let file_name = header.name_string();

        let textures = if header.texture_count == 0 {
            let mut texture_mdl_path = PathBuf::from(mdl_path);
            let file_stem = texture_mdl_path.file_stem().unwrap().to_str().unwrap();
            texture_mdl_path.set_file_name(format!("{}t.mdl", file_stem));
            let texture_mdl_path = texture_mdl_path.into_os_string();
            let texture_mdl_path = texture_mdl_path.into_string().unwrap();

            let file = File::open(texture_mdl_path).unwrap();
            let file_size = file.metadata().unwrap().len();
            let mut file = BufReader::new(file);
            let texture_header: MdlHeader = bincode::deserialize_from(&mut file).unwrap();

            header.texture_count = texture_header.texture_count;
            header.texture_offset = texture_header.texture_offset;
            header.texture_data_index = texture_header.texture_data_index;
            read_textures(&mut file, &texture_header)
        } else {
            read_textures(&mut file, &header)
        };

        file.seek(SeekFrom::Start(0)).unwrap();
        let mut file_data = vec![0u8; file_size as usize];
        file.read(&mut file_data).unwrap();

        MdlFile {
            name: file_name,
            textures: textures,
            header: header,
            raw_data: file_data,
        }
    }
}

fn read_textures<T: Read + Seek>(mut reader: &mut T, header: &MdlHeader) -> Vec<MdlTexture> {
    let num_textures = header.texture_count as usize;
    let mut textures = Vec::with_capacity(num_textures);
    reader.seek(SeekFrom::Start(header.texture_offset as u64)).unwrap();
    for i in 0..num_textures {
        let texture_header: TextureHeader = bincode::deserialize_from(&mut reader).unwrap();

        let name_string = texture_header.name_string();
        textures.push(MdlTexture {
            name: name_string.to_string(),
        });
    }
    textures
}