extern crate wad3parser;
extern crate glob;

use glob::glob;
use std::env;
use wad3parser::{ WadArchive, TextureType };

fn main() {
    let args = env::args().collect::<Vec<_>>();
    let path = &args[1];

    let search = format!("{}/**/*.wad", path);
    println!("search: {}", search);
    let wads = glob(&search).unwrap();
    for wad in wads {
        let wad = wad.unwrap();
        let wad_path = wad.to_string_lossy();
        println!("wad: {}", wad_path);

        let archive = WadArchive::open(&wad_path);
        let file_infos = &archive.files;
        for info in file_infos {
            let  name = &info.name;
            if info.texture_type == TextureType::Font {
                println!("{} - {:?}", name, info.texture_type);

                let texture_data = archive.decode_font(&info);
            }
        }
    }
}