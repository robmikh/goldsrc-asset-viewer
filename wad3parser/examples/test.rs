extern crate wad3parser;

use std::env;
use wad3parser::{ WadArchive, TextureType };

fn main() {
    let args = env::args().collect::<Vec<_>>();
    let path = &args[1];
    let search = &args[2];
    let search = search.trim();

    let archive = WadArchive::open(&path);
    let file_info = &archive.files;
    for info in file_info {
        let name = &info.name;
        println!("{} - {:?}", name, info.texture_type);

        if name == search {
            if info.texture_type == TextureType::Decal || info.texture_type == TextureType::MipmappedImage {
                let image_data = match info.texture_type {
                    TextureType::Decal => archive.decode_decal(&info),
                    TextureType::MipmappedImage => archive.decode_mipmaped_image(&info),
                    _ => panic!("New texture type! {:?}", info.texture_type),
                };

                image_data.image.save("test.png").unwrap();
                image_data.mipmap1.save("test_mipmap1.png").unwrap();
                image_data.mipmap2.save("test_mipmap2.png").unwrap();
                image_data.mipmap3.save("test_mipmap3.png").unwrap();
            } else {
                let image_data = match info.texture_type {
                    TextureType::Image => archive.decode_image(&info),
                    TextureType::Font => archive.decode_font(&info),
                    _ => panic!("New texture type! {:?}", info.texture_type),
                };

                image_data.image.save("test.png").unwrap();
            }
        }
    }
}