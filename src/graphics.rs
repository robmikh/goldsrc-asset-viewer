use imgui::*;
use imgui_wgpu::{Renderer, RendererConfig, Texture, TextureConfig};
use image::{ImageBuffer, Bgra};
use wgpu::SamplerDescriptor;

#[derive(Clone)]
pub struct TextureBundle<T> {
    pub mip_textures: Vec<MipTexture>,
    pub extra_data: T,
}

#[derive(Clone)]
pub struct MipTexture {
    pub texture_id: TextureId,
    pub width: u32,
    pub height: u32,
}

impl <T> TextureBundle<T> {
    pub fn clear(&mut self, renderer: &mut Renderer) {
        // unbind our previous textures
        for texture in self.mip_textures.drain(..) {
            renderer.textures.remove(texture.texture_id);
        }
    }
}

pub fn create_imgui_texture(
    device: &mut wgpu::Device,
    queue: &mut wgpu::Queue,
    renderer: &mut imgui_wgpu::Renderer,
    image: ImageBuffer<Bgra<u8>, Vec<u8>>,
) -> imgui_wgpu::Texture {
    let (width, height) = image.dimensions();
    let raw_data = image.into_raw();
    let texture_config = TextureConfig {
        size: wgpu::Extent3d {
            width,
            height,
            ..Default::default()
        },
        label: Some("sprite texture"),
        sampler_desc: SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        },
        ..Default::default()
    };
    let texture = Texture::new(&device, &renderer, texture_config);
    texture.write(&queue, &raw_data, width, height);
    texture
}