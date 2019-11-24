use imgui::*;
use imgui_wgpu::Renderer;

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
            renderer.textures().remove(texture.texture_id);
        }
    }
}

pub fn create_imgui_texture(
    device: &mut wgpu::Device,
    queue: &mut wgpu::Queue,
    bind_group_layout: &wgpu::BindGroupLayout,
    image: image::ImageBuffer<image::Bgra<u8>, Vec<u8>>,
) -> imgui_wgpu::WgpuTexture {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        lod_min_clamp: -100.0,
        lod_max_clamp: 100.0,
        compare_function: wgpu::CompareFunction::Always,
    });

    let (width, height) = image.dimensions();
    let texture_extent = wgpu::Extent3d {
        width: width as u32,
        height: height as u32,
        depth: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        size: texture_extent,
        array_layer_count: 1,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm, // This should be bgra... something is wrong either here, in imgui-wgpu, or in wad3parser(likely)
        usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
    });

    let image_data = image.into_vec();
    let temp_buffer = device
        .create_buffer_mapped(image_data.len(), wgpu::BufferUsage::COPY_SRC)
        .fill_from_slice(&image_data);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { todo: 0 });
    encoder.copy_buffer_to_texture(
        wgpu::BufferCopyView {
            buffer: &temp_buffer,
            offset: 0,
            row_pitch: 4 * width,
            image_height: height,
        }, 
        wgpu::TextureCopyView {
            texture: &texture,
            mip_level: 0,
            array_layer: 0,
            origin: wgpu::Origin3d {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
        },
        texture_extent,
    );
    queue.submit(&[encoder.finish()]);

    imgui_wgpu::WgpuTexture::new(texture, sampler, bind_group_layout, device)
}