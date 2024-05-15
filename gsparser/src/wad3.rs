extern crate bincode;
extern crate byteorder;
extern crate image;
extern crate serde;

use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::str;

use byteorder::{LittleEndian, ReadBytesExt};
use serde::Deserialize;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TextureType {
    Decal = 0x40,
    Image = 0x42,
    MipmappedImage = 0x43,
    Font = 0x46,
}

#[derive(Clone)]
pub struct WadFileInfo {
    pub name: String,
    pub texture_type: TextureType,
    info: WadDirectory,
}

pub struct WadArchive {
    pub files: Vec<WadFileInfo>,
    raw_data: Vec<u8>,
}

#[derive(Clone)]
pub struct TextureData {
    pub image_width: u32,
    pub image_height: u32,
    pub image: image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
}

#[derive(Clone)]
pub struct MipmapedTextureData {
    pub image_width: u32,
    pub image_height: u32,
    pub image: image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    pub mipmap1: image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    pub mipmap2: image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    pub mipmap3: image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
}

#[derive(Copy, Clone)]
pub struct CharInfo {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl CharInfo {
    fn default() -> CharInfo {
        CharInfo {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }
}

#[derive(Clone)]
pub struct FontData {
    pub image_width: u32,
    pub image_height: u32,
    pub row_count: u32,
    pub row_height: u32,
    pub font_info: [CharInfo; 256],
    pub image: image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
}

#[derive(Copy, Clone, Deserialize)]
struct WadHeader {
    magic: [u8; 4],
    num_dir: u32,
    dir_offset: u32,
}

#[allow(dead_code)]
#[derive(Copy, Clone, Deserialize)]
struct WadDirectory {
    file_position: u32,
    disk_size: u32,
    sizes: u32,
    dir_type: u8,
    compression: bool,
    _dummy: i16,
    name: [u8; 16],
}

#[allow(dead_code)]
#[derive(Copy, Clone, Deserialize)]
struct MipmappedTextureHeader {
    name: [u8; 16],
    width: u32,
    height: u32,
    image_offset: u32,
    mipmap1_offset: u32,
    mipmap2_offset: u32,
    mipmap3_offset: u32,
}

#[derive(Copy, Clone, Deserialize)]
struct TextureHeader {
    width: u32,
    height: u32,
}

#[derive(Copy, Clone, Deserialize)]
struct FontHeader {
    width: u32,
    height: u32,
    row_count: u32,
    row_height: u32,
    font_data: [[u8; 32]; 32],
}

impl FontHeader {
    fn font_data(&self) -> &[u8; 1024] {
        unsafe { std::mem::transmute(&self.font_data) }
    }
}

impl WadArchive {
    pub fn open<P: AsRef<Path>>(wad_path: P) -> WadArchive {
        let file = File::open(wad_path).unwrap();
        let file_size = file.metadata().unwrap().len();
        let mut file = BufReader::new(file);

        let file_infos = Self::read_file_infos(&mut file);

        file.seek(SeekFrom::Start(0)).unwrap();
        let mut file_data = vec![0u8; file_size as usize];
        file.read(&mut file_data).unwrap();

        WadArchive {
            files: file_infos,
            raw_data: file_data,
        }
    }

    pub fn from_bytes(wad_bytes: Vec<u8>) -> Self {
        let mut reader = std::io::Cursor::new(&wad_bytes);
        let file_infos = Self::read_file_infos(&mut reader);
        Self {
            files: file_infos,
            raw_data: wad_bytes,
        }
    }

    fn read_file_infos<R: Read + Seek>(mut reader: R) -> Vec<WadFileInfo> {
        let header: WadHeader = bincode::deserialize_from(&mut reader).unwrap();
        assert_eq!(header.magic[0], 87); // 'W' in ASCII
        assert_eq!(header.magic[1], 65); // 'A' in ASCII
        assert_eq!(header.magic[2], 68); // 'D' in ASCII
        assert_eq!(header.magic[3], 51); // '3' in ASCII

        let mut file_infos = Vec::new();
        reader
            .seek(SeekFrom::Start(header.dir_offset as u64))
            .unwrap();
        for _i in 0..header.num_dir {
            let wad_dir: WadDirectory = bincode::deserialize_from(&mut reader).unwrap();
            let name = str::from_utf8(&wad_dir.name).unwrap();
            let name = name.trim_matches(char::from(0));
            let texture_type = match wad_dir.dir_type {
                0x40 => TextureType::Decal,
                0x42 => TextureType::Image,
                0x43 => TextureType::MipmappedImage,
                0x46 => TextureType::Font,
                _ => panic!("unknown dir type! {} - 0x{:X}", name, wad_dir.dir_type),
            };
            file_infos.push(WadFileInfo {
                name: name.to_string(),
                texture_type: texture_type,
                info: wad_dir,
            });
        }

        file_infos
    }

    pub fn decode_decal(&self, file_info: &WadFileInfo) -> MipmapedTextureData {
        assert_eq!(file_info.texture_type, TextureType::Decal);

        let mut reader = self.get_file_data(file_info);
        let texture_header: MipmappedTextureHeader =
            bincode::deserialize_from(&mut reader).unwrap();

        let (image_data, mipmap1_data, mipmap2_data, mipmap3_data) =
            read_mipmapped_image_data(&texture_header, &mut reader);

        let num_colors = reader.read_u16::<LittleEndian>().unwrap();
        assert_eq!(num_colors, 256);

        let converted_image =
            create_image_greyscale(&image_data, texture_header.width, texture_header.height);
        let converted_mipmap1 = create_image_greyscale(
            &mipmap1_data,
            texture_header.width / 2,
            texture_header.height / 2,
        );
        let converted_mipmap2 = create_image_greyscale(
            &mipmap2_data,
            texture_header.width / 4,
            texture_header.height / 4,
        );
        let converted_mipmap3 = create_image_greyscale(
            &mipmap3_data,
            texture_header.width / 8,
            texture_header.height / 8,
        );

        MipmapedTextureData {
            image_width: texture_header.width,
            image_height: texture_header.height,
            image: converted_image,
            mipmap1: converted_mipmap1,
            mipmap2: converted_mipmap2,
            mipmap3: converted_mipmap3,
        }
    }

    pub fn decode_mipmaped_image(&self, file_info: &WadFileInfo) -> MipmapedTextureData {
        // the only decal in half-life is LOGO in tempdecal.wad, and it has the same layout as a mipmapped image.
        assert!(
            file_info.texture_type == TextureType::MipmappedImage
                || file_info.texture_type == TextureType::Decal
        );

        let mut reader = self.get_file_data(file_info);
        Self::decode_mipmaped_image_from_reader(&mut reader)
    }

    pub fn decode_mipmaped_image_from_reader<R: Read + Seek>(mut reader: R) -> MipmapedTextureData {
        let texture_header: MipmappedTextureHeader =
            bincode::deserialize_from(&mut reader).unwrap();

        let (image_data, mipmap1_data, mipmap2_data, mipmap3_data) =
            read_mipmapped_image_data(&texture_header, &mut reader);

        let num_colors = reader.read_u16::<LittleEndian>().unwrap();
        let mut palette_data = vec![0u8; (3 * num_colors) as usize];
        reader.read_exact(&mut palette_data).unwrap();

        let converted_image = create_image(
            &image_data,
            &palette_data,
            texture_header.width,
            texture_header.height,
        );
        let converted_mipmap1 = create_image(
            &mipmap1_data,
            &palette_data,
            texture_header.width / 2,
            texture_header.height / 2,
        );
        let converted_mipmap2 = create_image(
            &mipmap2_data,
            &palette_data,
            texture_header.width / 4,
            texture_header.height / 4,
        );
        let converted_mipmap3 = create_image(
            &mipmap3_data,
            &palette_data,
            texture_header.width / 8,
            texture_header.height / 8,
        );

        MipmapedTextureData {
            image_width: texture_header.width,
            image_height: texture_header.height,
            image: converted_image,
            mipmap1: converted_mipmap1,
            mipmap2: converted_mipmap2,
            mipmap3: converted_mipmap3,
        }
    }

    pub fn decode_image(&self, file_info: &WadFileInfo) -> TextureData {
        assert_eq!(file_info.texture_type, TextureType::Image);

        let mut reader = self.get_file_data(file_info);

        let texture_header: TextureHeader = bincode::deserialize_from(&mut reader).unwrap();

        let mut image_data = vec![0u8; (texture_header.width * texture_header.height) as usize];
        reader.read_exact(image_data.as_mut_slice()).unwrap();

        let num_colors = reader.read_u16::<LittleEndian>().unwrap();
        let mut palette_data = vec![0u8; (3 * num_colors) as usize];
        reader.read_exact(&mut palette_data).unwrap();

        let converted_image = create_image(
            &image_data,
            &palette_data,
            texture_header.width,
            texture_header.height,
        );

        TextureData {
            image_width: texture_header.width,
            image_height: texture_header.height,
            image: converted_image,
        }
    }

    pub fn decode_font(&self, file_info: &WadFileInfo) -> FontData {
        assert_eq!(file_info.texture_type, TextureType::Font);

        let mut reader = self.get_file_data(file_info);

        let mut texture_header: FontHeader = bincode::deserialize_from(&mut reader).unwrap();
        // half-life uses 256 width fonts
        texture_header.width = 256;

        let font_data = texture_header.font_data().to_vec();
        let mut font_data_reader = Cursor::new(&font_data);
        let mut font_info = [CharInfo::default(); 256];
        for i in 0..256 {
            let offset = font_data_reader.read_u16::<LittleEndian>().unwrap() as u32;
            let width = font_data_reader.read_u16::<LittleEndian>().unwrap() as u32;

            let row_area = texture_header.row_height * 256;
            let row = offset / row_area;
            let offset = offset - (row_area * row);

            let x = offset;
            let y = texture_header.row_height * row;
            let width = width;
            let height = texture_header.row_height;

            font_info[i] = CharInfo {
                x: x,
                y: y,
                width: width,
                height: height,
            };
        }

        let mut image_data = vec![0u8; (texture_header.width * texture_header.height) as usize];
        reader.read_exact(image_data.as_mut_slice()).unwrap();

        let num_colors = reader.read_u16::<LittleEndian>().unwrap();
        let mut palette_data = vec![0u8; (num_colors * 3) as usize];
        // We use read instead of read_exact here as a workaround for FONT2 in fonts.wad.
        // Otherwise we would hit the end of the file before we read enough bytes.
        reader.read(palette_data.as_mut_slice()).unwrap();

        let converted_image = create_image_with_alpha_key(
            &image_data,
            &palette_data,
            texture_header.width,
            texture_header.height,
            255,
        );

        FontData {
            image_width: texture_header.width,
            image_height: texture_header.height,
            row_count: texture_header.row_count,
            row_height: texture_header.row_height,
            font_info: font_info,
            image: converted_image,
        }
    }

    fn get_file_data(&self, file_info: &WadFileInfo) -> Cursor<&[u8]> {
        let file_data = {
            let raw_data = self.raw_data.as_slice();
            let start_index = file_info.info.file_position as usize;
            let end_index = start_index + file_info.info.disk_size as usize;
            &raw_data[start_index..end_index]
        };
        Cursor::new(file_data)
    }
}

fn create_image(
    image_data: &[u8],
    palette_data: &[u8],
    texture_width: u32,
    texture_height: u32,
) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    let mut image_rgba_data = Vec::<u8>::new();
    for palette_index in image_data {
        let index = (*palette_index as usize) * 3;
        let r_color = palette_data[index + 0];
        let g_color = palette_data[index + 1];
        let b_color = palette_data[index + 2];

        if r_color == 0 && g_color == 0 && b_color == 255 {
            image_rgba_data.push(0);
            image_rgba_data.push(0);
            image_rgba_data.push(0);
            image_rgba_data.push(0);
        } else {
            image_rgba_data.push(r_color);
            image_rgba_data.push(g_color);
            image_rgba_data.push(b_color);
            image_rgba_data.push(255);
        }
    }

    image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_vec(
        texture_width,
        texture_height,
        image_rgba_data,
    )
    .unwrap()
}

fn create_image_with_alpha_key(
    image_data: &[u8],
    palette_data: &[u8],
    texture_width: u32,
    texture_height: u32,
    alpha_key: u8,
) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    let mut image_rgba_data = Vec::<u8>::new();
    for palette_index in image_data {
        let index = (*palette_index as usize) * 3;
        let r_color = palette_data[index + 0];
        let g_color = palette_data[index + 1];
        let b_color = palette_data[index + 2];

        if (r_color == 0 && g_color == 0 && b_color == 255) || *palette_index == alpha_key {
            image_rgba_data.push(0);
            image_rgba_data.push(0);
            image_rgba_data.push(0);
            image_rgba_data.push(0);
        } else {
            image_rgba_data.push(b_color);
            image_rgba_data.push(g_color);
            image_rgba_data.push(r_color);
            image_rgba_data.push(255);
        }
    }

    image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_vec(
        texture_width,
        texture_height,
        image_rgba_data,
    )
    .unwrap()
}

fn create_image_greyscale(
    image_data: &[u8],
    texture_width: u32,
    texture_height: u32,
) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    let mut image_rgba_data = Vec::<u8>::new();
    for value in image_data {
        image_rgba_data.push(*value);
        image_rgba_data.push(*value);
        image_rgba_data.push(*value);
        image_rgba_data.push(255);
    }

    image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_vec(
        texture_width,
        texture_height,
        image_rgba_data,
    )
    .unwrap()
}

fn read_mipmapped_image_data<R: Read + Seek>(
    texture_header: &MipmappedTextureHeader,
    mut reader: R,
) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut image_data = vec![0u8; (texture_header.width * texture_header.height) as usize];
    let mut mipmap1_data =
        vec![0u8; ((texture_header.width / 2) * (texture_header.height / 2)) as usize];
    let mut mipmap2_data =
        vec![0u8; ((texture_header.width / 4) * (texture_header.height / 4)) as usize];
    let mut mipmap3_data =
        vec![0u8; ((texture_header.width / 8) * (texture_header.height / 8)) as usize];

    reader
        .seek(SeekFrom::Start(texture_header.image_offset as u64))
        .unwrap();
    reader.read_exact(image_data.as_mut_slice()).unwrap();
    reader
        .seek(SeekFrom::Start(texture_header.mipmap1_offset as u64))
        .unwrap();
    reader.read_exact(mipmap1_data.as_mut_slice()).unwrap();
    reader
        .seek(SeekFrom::Start(texture_header.mipmap2_offset as u64))
        .unwrap();
    reader.read_exact(mipmap2_data.as_mut_slice()).unwrap();
    reader
        .seek(SeekFrom::Start(texture_header.mipmap3_offset as u64))
        .unwrap();
    reader.read_exact(mipmap3_data.as_mut_slice()).unwrap();

    (image_data, mipmap1_data, mipmap2_data, mipmap3_data)
}
