extern crate serde;
extern crate bincode;
extern crate image;
extern crate byteorder;

use std::io::{Cursor, BufReader, Read, Seek, SeekFrom};
use std::fs::File;
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
    name_offset: u32,
    flags: u32,
    width: u32,
    height: u32,
    offset: u32,
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

        let header : MdlHeader = bincode::deserialize_from(&mut file).unwrap();
        let file_name = header.name_string();

        let num_textures = header.texture_count as usize;
        let mut textures = Vec::with_capacity(num_textures);
        for i in 0..num_textures {
            file.seek(SeekFrom::Start(header.texture_offset as u64)).unwrap();
            let texture_header: TextureHeader = bincode::deserialize_from(&mut file).unwrap();
            file.seek(SeekFrom::Start(texture_header.name_offset as u64)).unwrap();

            let mut name_data = Vec::new();
            while (true) {
                let current_char = file.read_u8().unwrap();

                if current_char == 0 {
                    break;
                }

                name_data.push(current_char);
            }

            let name_string = str::from_utf8(&name_data).unwrap();
            textures.push(MdlTexture {
                name: name_string.to_string(),
            });
        }

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