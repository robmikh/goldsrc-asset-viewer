extern crate imgui_wgpu;
extern crate wad3parser;
extern crate image;
extern crate nfd;
extern crate clap;
extern crate mdlparser;

mod graphics;
mod wad_viewer;

use clap::*;
use imgui::*;
use imgui_wgpu::Renderer;
use imgui_winit_support;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::ffi::OsStr;
use std::time::Instant;
use wad3parser::{ WadArchive, WadFileInfo, TextureType, CharInfo };
use wgpu::winit::{ ElementState, Event, EventsLoop, KeyboardInput, VirtualKeyCode, WindowEvent, };
use crate::wad_viewer::{WadViewer, load_wad_archive};

pub struct MdlFile {
    pub path: String,
    pub file: mdlparser::MdlFile,
    pub texture_names: Vec<ImString>,
}

pub struct WadFile {
    pub path: String,
    pub archive: WadArchive,
    pub files: HashMap<ImString, WadFileInfo>,
    pub file_names: Vec<ImString>,
}

enum FileInfo {
    None,
    WadFile(WadFile),
    MdlFile(MdlFile),
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
        file_info = load_file(&path);
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
    let mut wad_viewer = WadViewer::new();

    let mut pending_path: Option<String> = None;

    if let FileInfo::WadFile(file_info) = &file_info {
        wad_viewer.pre_warm(&file_info, &mut device, &mut renderer);
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
                file_info = load_file(&new_path);

                wad_viewer.reset_listbox_index();
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
                        let result = nfd::open_file_dialog(Some("wad;mdl"), None).unwrap();
                        if let nfd::Response::Okay(new_path) = result {
                            pending_path = Some(new_path.to_string());
                        } 
                    }
                    if ui.menu_item(im_str!["Exit"]).build() {
                        running = false;
                    }
                });
            });

            match &file_info {
                FileInfo::WadFile(file_info) => {
                    wad_viewer.build_ui(&ui, &file_info, &mut device, &mut renderer, force_new_selection);    
                },
                FileInfo::MdlFile(file_info) => {
                    let texture_names = &file_info.texture_names.iter().collect::<Vec<_>>();

                    let mut dummy = 0;
                    let mut dummy2 = false;
                    ui.window(im_str!["Texture list"])
                    .size((300.0, 400.0), ImGuiCond::FirstUseEver)
                    .build(|| {
                        ui.text(im_str!["Path: {}", &file_info.path]);
                        ui.text(im_str!["Name: {}", &file_info.file.name]);
                        dummy2 = ui.list_box(
                            im_str!["Textures"], 
                            &mut dummy,
                            &texture_names,
                            texture_names.len() as i32);
                    });
                },
                _ => (),
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

fn get_extension_from_path(path: &str) -> Option<&str> {
    Path::new(path)
        .extension()
        .and_then(OsStr::to_str)
}

fn load_wad_file(path: &str) -> WadFile {
    let archive = WadArchive::open(path);
    let (files, file_names) = load_wad_archive(&archive);
    WadFile {
        path: path.to_string(),
        archive: archive,
        files: files,
        file_names: file_names,
    }
}

fn load_mdl_file(path: &str) -> MdlFile {
    let mdl_file = mdlparser::MdlFile::open(path);

    let mut texture_names = Vec::new();
    for texture in &mdl_file.textures {
        let imgui_str = ImString::new(texture.name.clone());
        texture_names.push(imgui_str);
    }

    MdlFile {
        path: path.to_string(),
        file: mdl_file,
        texture_names: texture_names,
    }
}

fn load_file(path: &str) -> FileInfo {
    let mut file_info = FileInfo::None;
    if let Some(extension) = get_extension_from_path(&path) {
        match extension {
            "wad" => file_info = FileInfo::WadFile(load_wad_file(path)),
            "mdl" => file_info = FileInfo::MdlFile(load_mdl_file(path)),
            _ => (),
        }
    }
    file_info
}