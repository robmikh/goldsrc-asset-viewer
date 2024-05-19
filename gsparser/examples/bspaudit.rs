extern crate glob;
extern crate gsparser;

use glob::glob;
use gsparser::bsp::BspReader;
use std::env;

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let path = args.get(0).unwrap();

    let search = format!("{}/**/*.bsp", path);
    println!("search: {}", search);
    let bsps = glob(&search).unwrap();
    let mut max = u32::MIN;
    for bsp in bsps {
        let bsp = bsp.unwrap();

        let data = std::fs::read(&bsp).unwrap();
        let reader = BspReader::read(data);
        let textures = reader.read_textures_header();
        let num_textures = textures.num_textures;

        println!("bsp: {}  -  {}", bsp.display(), num_textures);
        max = max.max(num_textures);
    }
    println!("Max num textures: {}", max);
}
