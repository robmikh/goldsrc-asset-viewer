extern crate imgui_wgpu;
extern crate wad3parser;
extern crate image;
extern crate nfd;
extern crate clap;

use clap::*;
use imgui::*;
use imgui_wgpu::Renderer;
use imgui_winit_support;
use std::collections::HashMap;
use std::env;
use std::time::Instant;
use wad3parser::{ WadArchive, WadFileInfo, TextureType, CharInfo };
use wgpu::winit::{ ElementState, Event, EventsLoop, KeyboardInput, VirtualKeyCode, WindowEvent, };

#[derive(Clone)]
struct TextureBundle {
    pub mip_textures: Vec<MipTexture>,
    pub extra_data: ExtraTextureData,
}

#[derive(Clone)]
struct MipTexture {
    pub texture_id: ImTexture,
    pub width: u32,
    pub height: u32,
}

#[derive(Copy, Clone)]
struct CharMetadata {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone)]
struct FontMetadata {
    pub row_count: u32,
    pub row_height: u32,
    pub char_infos: Vec<CharMetadata>,
}

#[derive(Clone)]
struct ExtraTextureData {
    pub texture_type: TextureType,
    pub font: Option<FontMetadata>,
}

impl TextureBundle {
    fn clear(&mut self, renderer: &mut Renderer) {
        // unbind our previous textures
        for texture in self.mip_textures.drain(..) {
            renderer.textures().remove(texture.texture_id);
        }
        self.extra_data.font = None;
    }
}

impl ExtraTextureData {
    fn new(texture_type: TextureType) -> ExtraTextureData {
        ExtraTextureData {
            texture_type: texture_type,
            font: None,
        }
    }
}

struct WadFile {
    pub path: String,
    pub archive: WadArchive,
    pub files: HashMap<ImString, WadFileInfo>,
    pub file_names: Vec<ImString>,
}

enum FileInfo {
    None,
    WadFile(WadFile),
}

fn main() {
    env_logger::init();

    let mut file_info = FileInfo::None;

    let arg_matches = App::new("goldsrc-asset-viewer")
        .version(crate_version!())
        .author("Robert Mikhayelyan <rob.mikh@outlook.com>")
        .about("A tool to view assets from GoldSource games.")
        .args_from_usage("[file_path] 'Open the specified file.'")
        .get_matches();

    if let Some(path) = arg_matches.value_of("file_path") {
        file_info = FileInfo::WadFile(load_archive(path));
    }

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
    let mut scale: f32 = 1.0;
    let mut new_selection = false;
    let mut font_overlay = false;
    let mut texture_outline = false;

    let mut pending_path: Option<String> = None;

    let mut texture_bundle: Option<TextureBundle> = None;
    if let FileInfo::WadFile(file_info) = &file_info {
        let info = file_info.files.get(&file_info.file_names[selected_file_index as usize]).unwrap();
        texture_bundle = Some(get_texture_bundle(&file_info.archive, &info, &mut device, &mut renderer));
    } 

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
        let force_new_selection = {
            if let Some(new_path) = pending_path {
                file_info = FileInfo::WadFile(load_archive(&new_path));

                selected_file_index = 0;
                pending_path = None;
                true
            } else {
                false
            }
        };

        {
            ui.main_menu_bar(|| {
                ui.menu(im_str!["File"]).build(|| {
                    if ui.menu_item(im_str!["Open"])
                        .shortcut(im_str!["Ctrl+O"])
                        .build() {
                        let result = nfd::open_file_dialog(Some("wad"), None).unwrap();
                        if let nfd::Response::Okay(new_path) = result {
                            pending_path = Some(new_path.to_string());
                        } 
                    }
                    if ui.menu_item(im_str!["Exit"]).build() {
                        running = false;
                    }
                });
            });

            if let FileInfo::WadFile(file_info) = &file_info {
                let file_names = &file_info.file_names.iter().collect::<Vec<_>>();

                ui.window(im_str!["File list"])
                .size((300.0, 400.0), ImGuiCond::FirstUseEver)
                .build(|| {
                    ui.text(im_str!["Path: {}", &file_info.path]);
                    new_selection = ui.list_box(
                        im_str!["Files"], 
                        &mut selected_file_index,
                        &file_names,
                        file_names.len() as i32);
                });

                if new_selection || force_new_selection {
                    // unbind our previous textures
                    if let Some(texture_bundle) = texture_bundle.as_mut() {
                        texture_bundle.clear(&mut renderer);
                    }

                    let info = file_info.files.get(&file_info.file_names[selected_file_index as usize]).unwrap();
                    texture_bundle = Some(get_texture_bundle(&file_info.archive, &info, &mut device, &mut renderer));
                }

                if let Some(texture_bundle) = texture_bundle.as_ref() {
                    ui.window(im_str!["File preview"])
                        .position((500.0, 150.0), ImGuiCond::FirstUseEver)
                        .size((300.0, 300.0), ImGuiCond::FirstUseEver)
                        .build(|| {
                            ui.text(&file_names[selected_file_index as usize]);
                            ui.text(im_str!["Type: {:?} (0x{:X})", texture_bundle.extra_data.texture_type, texture_bundle.extra_data.texture_type as u8]);
                            ui.text(im_str!["Size: {} x {}", texture_bundle.mip_textures[0].width, texture_bundle.mip_textures[0].height]);
                            match texture_bundle.extra_data.texture_type {
                                TextureType::Font => {
                                    if let Some(font_data) = texture_bundle.extra_data.font.as_ref() {
                                        ui.text(im_str!["Row Count: {}", font_data.row_count]);
                                        ui.text(im_str!["Row Height: {}", font_data.row_height]);
                                        ui.checkbox(im_str!["Char Info"], &mut font_overlay);
                                    }
                                },
                                _ => (),
                            }
                            ui.slider_float(im_str!["Scale"], &mut scale, 1.0, 10.0)
                                .build();
                            ui.checkbox(im_str!["Texture outline"], &mut texture_outline);
                            let (x, y) = ui.get_cursor_screen_pos();
                            for texture in &texture_bundle.mip_textures {
                                let (x, y) = ui.get_cursor_screen_pos();
                                ui.image(texture.texture_id, (texture.width as f32 * scale, texture.height as f32 * scale))
                                .build();
                                if texture_outline {
                                    ui.get_window_draw_list()
                                        .add_rect((x, y), (x + ((texture.width as f32) * scale), y + ((texture.height as f32) * scale)), [0.0, 1.0, 0.0, 1.0])
                                        .thickness(2.0)
                                        .build();
                                }
                            }
                            if font_overlay {
                                if let Some(font_data) = texture_bundle.extra_data.font.as_ref() {
                                    let chars = font_data.char_infos.len();
                                    for i in 0..chars {
                                        let font_info = font_data.char_infos[i];
                                        if font_info.width == 0 {
                                            continue;
                                        }
                                        let local_x = font_info.x as f32;
                                        let local_y = font_info.y as f32;

                                        let x = x + (local_x * scale);
                                        let y = y + (local_y * scale);
                                        let width = font_info.width as f32 * scale;
                                        let height = font_data.row_height as f32 * scale;

                                        ui.get_window_draw_list()
                                            .add_rect((x, y), (x + width, y + height), [1.0, 0.0, 0.0, 1.0])
                                            .thickness(2.0)
                                            .build();
                                    }
                                }
                            }
                        });
                }
            }
            
            //ui.show_demo_window(&mut demo_open);
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor{ todo: 0 });

        renderer
            .render(ui, &mut device, &mut encoder, &frame.view)
            .unwrap();

        device.get_queue().submit(&[encoder.finish()]);
    }    
}

fn get_decoded_data(
    archive: &WadArchive, 
    info: &WadFileInfo,
) -> (Vec<image::ImageBuffer<image::Bgra<u8>, Vec<u8>>>, ExtraTextureData) {
    let mut extra_data = ExtraTextureData::new(info.texture_type);
    let datas = {
        if info.texture_type == TextureType::Decal || info.texture_type == TextureType::MipmappedImage {
            let image_data = archive.decode_mipmaped_image(&info);
            vec![image_data.image, image_data.mipmap1, image_data.mipmap2, image_data.mipmap3]
        } else if info.texture_type == TextureType::Image {
            let image_data = archive.decode_image(&info);
            vec![image_data.image]
        } else if info.texture_type == TextureType::Font {
            let font_data = archive.decode_font(&info);

            let chars = font_data.font_info.len();
            let mut char_infos = vec![CharMetadata{ x: 0, y: 0, width: 0, height: 0 }; chars];
            for i in 0..chars {
                let char_info = font_data.font_info[i];
                if char_info.width == 0 {
                    continue;
                }
                let row_area = font_data.row_height * 256;
                let row = char_info.offset / row_area;
                let offset = char_info.offset - (row_area * row);

                let x = offset;
                let y = (font_data.row_height * row);
                let width = char_info.width;
                let height = font_data.row_height;

                char_infos[i].x = x;
                char_infos[i].y = y;
                char_infos[i].width = width;
                char_infos[i].height = height;
            }

            extra_data.font = Some(FontMetadata {
                row_count: font_data.row_count,
                row_height: font_data.row_height,
                char_infos: char_infos,
            });
            vec![font_data.image]
        } else {
            panic!("New texture type! {:?}", info.texture_type);
        }
    };
    (datas, extra_data)
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


fn get_texture_bundle(
    archive: &WadArchive, 
    info: &WadFileInfo,
    device: &mut wgpu::Device,
    renderer: &mut Renderer,
) -> TextureBundle {
    let (decoded_images, texture_data) = get_decoded_data(&archive, &info);
    let mut textures = Vec::with_capacity(decoded_images.len());

    for decoded_image in decoded_images {
        let (texture_width, texture_height) = decoded_image.dimensions();
        let texture = create_imgui_texture(device, renderer.texture_layout(), decoded_image);
        let texture_id = renderer.textures().insert(texture);

        textures.push(MipTexture {
            texture_id: texture_id,
            width: texture_width,
            height: texture_height,
        });
    }

    TextureBundle {
        mip_textures: textures, 
        extra_data: texture_data,
    }
}

fn load_archive(path: &str) -> WadFile {
    let archive = WadArchive::open(path);
    let (files, file_names) = load_file(&archive);
    WadFile {
        path: path.to_string(),
        archive: archive,
        files: files,
        file_names: file_names,
    }
}

fn load_file(archive: &WadArchive) -> (HashMap<ImString, WadFileInfo>, Vec<ImString>) {
    let file_infos = &archive.files;
    let mut files = HashMap::<ImString, WadFileInfo>::new();
    let mut file_names = Vec::new();
    for info in file_infos {
        let imgui_str = ImString::new(info.name.to_string());
        file_names.push(imgui_str.clone());
        files.insert(imgui_str, info.clone());
    }

    (files, file_names)
}