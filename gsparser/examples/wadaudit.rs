extern crate glob;

use glob::glob;
use gsparser::wad3::{TextureType, WadArchive};
use std::env;

fn main() {
    let args = env::args().collect::<Vec<_>>();
    let path = &args[1];

    let search = format!("{}/**/*.wad", path);
    println!("search: {}", search);
    let wads = glob(&search).unwrap();
    for wad in wads {
        let wad = wad.unwrap();
        println!("wad: {}", wad.display());

        let archive = WadArchive::open(&wad);
        let file_infos = &archive.files;
        for info in file_infos {
            let name = &info.name;
            if info.texture_type == TextureType::Font {
                println!("{} - {:?}", name, info.texture_type);

                let _texture_data = archive.decode_font(&info);
            }
        }
    }
}
