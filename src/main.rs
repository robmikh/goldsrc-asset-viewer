extern crate imgui_wgpu;
extern crate wad3parser;
extern crate image;
extern crate nfd;
extern crate clap;
extern crate mdlparser;

mod graphics;
mod wad_viewer;
mod mdl_viewer;

use clap::*;
use imgui::*;
use imgui_wgpu::Renderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::collections::HashMap;
use std::path::Path;
use std::ffi::OsStr;
use std::time::Instant;
use wad3parser::{ WadArchive, WadFileInfo };
use winit::{ ElementState, Event, EventsLoop, KeyboardInput, VirtualKeyCode, WindowEvent, };
use crate::wad_viewer::{WadViewer, load_wad_archive};
use crate::mdl_viewer::{MdlViewer};

pub struct MdlFile {
    pub path: String,
    pub file: mdlparser::MdlFile,
    pub texture_names: Vec<ImString>,
    pub body_part_names: Vec<ImString>,
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

    let adapter = wgpu::Adapter::request(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::Default,
            backends: wgpu::BackendBit::PRIMARY,
        },
    ).unwrap();

    let (mut device, mut queue) = adapter.request_device(&wgpu::DeviceDescriptor {
        extensions: wgpu::Extensions {
            anisotropic_filtering: false,
        },
        limits: wgpu::Limits::default(),
    });

    let mut events_loop = EventsLoop::new();
    let window = winit::Window::new(&events_loop).unwrap();
    window.set_title("goldsrc-asset-viewer");

    let surface = wgpu::Surface::create(&window);

    let mut dpi_factor = window.get_hidpi_factor();
    let mut size = window.get_inner_size().unwrap().to_physical(dpi_factor);

    let mut swap_chain_description = wgpu::SwapChainDescriptor {
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: size.width as u32,
        height: size.height as u32,
        present_mode: wgpu::PresentMode::Vsync,
    };
    let mut swap_chain = device.create_swap_chain(&surface, &swap_chain_description);

    let mut imgui = Context::create();
    imgui.set_ini_filename(None);

    let mut platform = WinitPlatform::init(&mut imgui);

    let font_size = (13.0 * dpi_factor) as f32;
    imgui.io_mut().font_global_scale = (1.0 / dpi_factor) as f32;

    imgui.fonts().add_font(&[
        FontSource::DefaultFontData {
            config: Some(FontConfig {
                size_pixels: font_size,
                ..FontConfig::default()
            }),
        },
    ]);

    platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Rounded);

    let clear_color = wgpu::Color {
        r: 0.1,
        g: 0.2,
        b: 0.3,
        a: 1.0,
    };
    let mut renderer = Renderer::new(&mut imgui, &mut device, &mut queue, swap_chain_description.format, Some(clear_color)).unwrap();

    let mut last_frame = Instant::now();
    //let mut demo_open = true;
    let mut wad_viewer = WadViewer::new();
    let mut mdl_viewer = MdlViewer::new();

    let mut pending_path: Option<String> = None;

    let mut running = true;
    while running {
        let mut new_size = false;
        events_loop.poll_events(|event| {
            match event {
                Event::WindowEvent {
                    event: WindowEvent::Resized(_),
                    ..
                } => {
                    dpi_factor = window.get_hidpi_factor();
                    size = window.get_inner_size().unwrap().to_physical(dpi_factor);
                    new_size = true;
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

            platform.handle_event(imgui.io_mut(), &window, &event);
        });

        if new_size {
            swap_chain_description = wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                format: wgpu::TextureFormat::Bgra8Unorm,
                width: size.width as u32,
                height: size.height as u32,
                present_mode: wgpu::PresentMode::Vsync,
            };

            swap_chain = device.create_swap_chain(&surface, &swap_chain_description);
        }

        let io = imgui.io_mut();
        platform.prepare_frame(io, &window).expect("Failed to start frame");
        last_frame = io.update_delta_time(last_frame);

        let frame = swap_chain.get_next_texture();
        let mut ui = imgui.frame();
        if let Some(new_path) = pending_path {
            file_info = load_file(&new_path);
            pending_path = None;
        }

        {
            ui.main_menu_bar(|| {
                ui.menu(im_str!["File"], true, || {
                    if MenuItem::new(im_str!["Open"])
                        .shortcut(im_str!["Ctrl+O"])
                        .build(&ui) {
                        let result = nfd::open_file_dialog(Some("wad;mdl"), None).unwrap();
                        if let nfd::Response::Okay(new_path) = result {
                            pending_path = Some(new_path.to_string());
                        } 
                    }
                    if MenuItem::new(im_str!["Exit"]).build(&ui) {
                        running = false;
                    }
                });
            });

            match &file_info {
                FileInfo::WadFile(file_info) => wad_viewer.build_ui(&ui, &file_info, &mut device, &mut queue, &mut renderer),
                FileInfo::MdlFile(file_info) => mdl_viewer.build_ui(&ui, &file_info, &mut device, &mut queue, &mut renderer),
                _ => (),
            }
            
            //ui.show_demo_window(&mut demo_open);
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor{ todo: 0 });

        let draw_data = ui.render();
        renderer
            .render(draw_data, &mut device, &mut encoder, &frame.view)
            .unwrap();

        queue.submit(&[encoder.finish()]);
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

    let mut body_part_names = Vec::new();
    for body_part in &mdl_file.body_parts {
        let imgui_str = ImString::new(body_part.name.clone());
        body_part_names.push(imgui_str);
    }

    MdlFile {
        path: path.to_string(),
        file: mdl_file,
        texture_names: texture_names,
        body_part_names: body_part_names,
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