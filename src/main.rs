extern crate imgui_wgpu;
extern crate wad3parser;
extern crate image;

use imgui::*;
use imgui_wgpu::Renderer;
use imgui_winit_support;
use std::collections::HashMap;
use std::env;
use std::time::Instant;
use wad3parser::{ WadArchive, WadFileInfo, TextureType, };
use wgpu::winit::{ ElementState, Event, EventsLoop, KeyboardInput, VirtualKeyCode, WindowEvent, };

struct Texture {
    pub texture_id: ImTexture,
    pub width: u32,
    pub height: u32,
}

fn main() {
    env_logger::init();

    let args = env::args().collect::<Vec<_>>();
    let path = &args[1];

    let archive = WadArchive::open(&path);
    let file_infos = &archive.files;
    let mut files = HashMap::new();
    let mut file_names = Vec::new();
    for info in file_infos {
        let imgui_str = ImString::new(info.name.to_string());
        file_names.push(imgui_str.clone());
        files.insert(imgui_str, info);
    }
    // Because of course this can't be simple...
    let file_names = &file_names.iter().collect::<Vec<_>>();

    let instance = wgpu::Instance::new();
    let adapter = instance.get_adapter(&wgpu::AdapterDescriptor{
        power_preference: wgpu::PowerPreference::LowPower,
    });
    let mut device = adapter.request_device(&wgpu::DeviceDescriptor {
        extensions: wgpu::Extensions {
            anisotropic_filtering: false,
        },
        limits: wgpu::Limits::default(),
    });

    let mut events_loop = EventsLoop::new();
    let window = wgpu::winit::Window::new(&events_loop).unwrap();
    window.set_title("goldsrc-asset-viewer");

    let surface = instance.create_surface(&window);

    let mut dpi_factor = window.get_hidpi_factor();
    let mut size = window.get_inner_size().unwrap().to_physical(dpi_factor);

    let mut swap_chain_description = wgpu::SwapChainDescriptor {
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: size.width as u32,
        height: size.height as u32,
    };
    let mut swap_chain = device.create_swap_chain(&surface, &swap_chain_description);

    let mut imgui = ImGui::init();
    imgui.set_ini_filename(None);

    let font_size = (13.0 * dpi_factor) as f32;
    imgui.set_font_global_scale((1.0 / dpi_factor) as f32);

    imgui.fonts().add_default_font_with_config(
        ImFontConfig::new()
            .oversample_h(1)
            .pixel_snap_h(true)
            .size_pixels(font_size),
    );

    imgui_winit_support::configure_keys(&mut imgui);

    let clear_color = wgpu::Color {
        r: 0.1,
        g: 0.2,
        b: 0.3,
        a: 1.0,
    };
    let mut renderer = Renderer::new(&mut imgui, &mut device, swap_chain_description.format, Some(clear_color)).unwrap();

    let mut last_frame = Instant::now();
    //let mut demo_open = true;
    let mut selected_file_index: i32 = 0;
    let mut new_selection = false;

    let info = files.get(file_names[selected_file_index as usize]).unwrap();
    let mut texture_type = info.texture_type;  
    let mut textures = {
        let decoded_images = get_textures(&archive, &info);
        let mut textures = Vec::with_capacity(decoded_images.len());

        for decoded_image in decoded_images {
            let (texture_width, texture_height) = decoded_image.dimensions();
            let texture = create_imgui_texture(&mut device, renderer.texture_layout(), decoded_image);
            let texture_id = renderer.textures().insert(texture);

            textures.push(Texture {
                texture_id: texture_id,
                width: texture_width,
                height: texture_height,
            });
        }

        textures
    };
    let mut scale: f32 = 1.0;

    let mut running = true;
    while running {
        events_loop.poll_events(|event| {
            match event {
                Event::WindowEvent {
                    event: WindowEvent::Resized(_),
                    ..
                } => {
                    dpi_factor = window.get_hidpi_factor();
                    size = window.get_inner_size().unwrap().to_physical(dpi_factor);

                    swap_chain_description = wgpu::SwapChainDescriptor {
                        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                        format: wgpu::TextureFormat::Bgra8Unorm,
                        width: size.width as u32,
                        height: size.height as u32,
                    };

                    swap_chain = device.create_swap_chain(&surface, &swap_chain_description);
                },
                Event::WindowEvent {
                    event: WindowEvent::KeyboardInput {
                        input: KeyboardInput {
                            virtual_keycode: Some(VirtualKeyCode::Escape),
                            state: ElementState::Pressed,
                            ..
                        },
                        ..
                    },
                    ..
                }
                | Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    running = false;
                },
                _ => (),
            }

            imgui_winit_support::handle_event(&mut imgui, &event, dpi_factor, dpi_factor);
        });

        let now = Instant::now();
        let delta = now - last_frame;
        let delta_seconds = delta.as_secs() as f32 + delta.subsec_nanos() as f32 / 1_000_000_000.0;
        last_frame = now;

        imgui_winit_support::update_mouse_cursor(&imgui, &window);

        let frame = swap_chain.get_next_texture();
        let frame_size = imgui_winit_support::get_frame_size(&window, dpi_factor).unwrap();
        let ui = imgui.frame(frame_size, delta_seconds);

        {
            ui.window(im_str!["{}", path])
                .size((300.0, 400.0), ImGuiCond::FirstUseEver)
                .build(|| {
                    new_selection = ui.list_box(im_str!["Files"], &mut selected_file_index, file_names, files.len() as i32);
                });

            if new_selection {
                // unbind our previous textures
                for texture_entry in textures.drain(..) {
                    renderer.textures().remove(texture_entry.texture_id);
                }

                let info = files.get(file_names[selected_file_index as usize]).unwrap();
                texture_type = info.texture_type;

                let decoded_images = get_textures(&archive, &info);
                for decoded_image in decoded_images {
                    let (texture_width, texture_height) = decoded_image.dimensions();
                    let texture = create_imgui_texture(&mut device, renderer.texture_layout(), decoded_image);
                    let texture_id = renderer.textures().insert(texture);

                    textures.push(Texture {
                        texture_id: texture_id,
                        width: texture_width,
                        height: texture_height,
                    });
                }
            }

            ui.window(im_str!["File preview"])
                .position((500.0, 150.0), ImGuiCond::FirstUseEver)
                .size((300.0, 300.0), ImGuiCond::FirstUseEver)
                .build(|| {
                    ui.text(file_names[selected_file_index as usize]);
                    ui.text(im_str!["Type: {:?}", texture_type]);
                    ui.text(im_str!["Size: {} x {}", textures[0].width, textures[0].height]);
                    match texture_type {
                        TextureType::Font => {
                            ui.text(im_str!["Row Count: {}", "idk"]);
                        },
                        _ => (),
                    }
                    ui.slider_float(im_str!["Scale"], &mut scale, 1.0, 10.0)
                        .build();
                    for texture in &textures {
                        ui.image(texture.texture_id, (texture.width as f32 * scale, texture.height as f32 * scale))
                        .build();
                    }
                });
            
            //ui.show_demo_window(&mut demo_open);
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor{ todo: 0 });

        renderer
            .render(ui, &mut device, &mut encoder, &frame.view)
            .unwrap();

        device.get_queue().submit(&[encoder.finish()]);
    }    
}

fn get_textures(
    archive: &WadArchive, 
    info: &WadFileInfo,
) -> Vec<image::ImageBuffer<image::Bgra<u8>, Vec<u8>>> {
    if info.texture_type == TextureType::Decal || info.texture_type == TextureType::MipmappedImage {
        let image_data = archive.decode_mipmaped_image(&info);
        vec![image_data.image, image_data.mipmap1, image_data.mipmap2, image_data.mipmap3]
    } else if info.texture_type == TextureType::Image {
        let image_data = archive.decode_image(&info);
        vec![image_data.image]
    } else if info.texture_type == TextureType::Font {
        let image_data = archive.decode_font(&info);
        vec![image_data.image]
    } else {
        panic!("New texture type! {:?}", info.texture_type);
    }
}

fn create_imgui_texture(
    device: &mut wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    image: image::ImageBuffer<image::Bgra<u8>, Vec<u8>>,
) -> imgui_wgpu::Texture {
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
        usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::TRANSFER_DST,
    });

    let image_data = image.into_vec();
    let temp_buffer = device
        .create_buffer_mapped(image_data.len(), wgpu::BufferUsage::TRANSFER_SRC)
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
    device.get_queue().submit(&[encoder.finish()]);

    imgui_wgpu::Texture::new(texture, sampler, bind_group_layout, device)
}
