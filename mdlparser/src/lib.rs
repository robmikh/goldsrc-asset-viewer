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
pub struct MdlFile {
    raw_data: Vec<u8>,
}

impl MdlFile {
    pub fn open(mdl_path: &str) -> MdlFile {
        let file = File::open(mdl_path).unwrap();
        let file_size = file.metadata().unwrap().len();
        let mut file = BufReader::new(file);

        file.seek(SeekFrom::Start(0)).unwrap();
        let mut file_data = vec![0u8; file_size as usize];
        file.read(&mut file_data).unwrap();

        MdlFile {
            raw_data: file_data,
        }

    }
}