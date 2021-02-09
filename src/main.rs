mod graphics;
mod wad_viewer;
mod mdl_viewer;

use futures::executor::block_on;
use clap::*;
use imgui::*;
use imgui_wgpu::{Renderer, RendererConfig, Texture, TextureConfig};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::collections::HashMap;
use std::path::Path;
use std::ffi::OsStr;
use std::time::Instant;
use wad3parser::{ WadArchive, WadFileInfo };
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};
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

    let event_loop = EventLoop::new();
    let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
    let (window, size, surface) = {
        let window = Window::new(&event_loop).unwrap();
        window.set_inner_size(LogicalSize {
            width: 1447.0,
            height: 867.0,
        });
        window.set_title("goldsrc-asset-viewer");
        let size = window.inner_size();
        let surface = unsafe {
            instance.create_surface(&window)
        };
        (window, size, surface)
    };

    let mut hidpi_factor = window.scale_factor();

    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
    })).unwrap();
    let (mut device, mut queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).unwrap();

    let swap_chain_desc = wgpu::SwapChainDescriptor {
        usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: size.width as u32,
        height: size.height as u32,
        present_mode: wgpu::PresentMode::Mailbox,
    };
    let mut swap_chain = device.create_swap_chain(&surface, &swap_chain_desc);

    let mut imgui = imgui::Context::create();
    let mut platform = imgui_winit_support::WinitPlatform::init(&mut imgui);
    platform.attach_window(
        imgui.io_mut(),
        &window,
        imgui_winit_support::HiDpiMode::Default,
    );
    imgui.set_ini_filename(None);

    let font_size = (13.0 * hidpi_factor) as f32;
    imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

    imgui.fonts().add_font(&[FontSource::DefaultFontData {
        config: Some(imgui::FontConfig {
            oversample_h: 1,
            pixel_snap_h: true,
            size_pixels: font_size,
            ..Default::default()
        }),
    }]);

    let clear_color = wgpu::Color {
        r: 0.1,
        g: 0.2,
        b: 0.3,
        a: 1.0,
    };

    let renderer_config = RendererConfig {
        texture_format: swap_chain_desc.format,
        ..Default::default()
    };

    let mut renderer = Renderer::new(&mut imgui, &device, &queue, renderer_config);

    let mut last_frame = Instant::now();
    let mut last_cursor = None;
    //let mut demo_open = true;
    let mut wad_viewer = WadViewer::new();
    let mut mdl_viewer = MdlViewer::new();

    let mut pending_path: Option<String> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = if cfg!(feature = "metal-auto-capture") {
            ControlFlow::Exit
        } else {
            ControlFlow::Poll
        };
        match event {
            Event::MainEventsCleared => window.request_redraw(),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if window_id == window.id() => *control_flow = ControlFlow::Exit,
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => {
                let size = window.inner_size();

                let swap_chain_desc = wgpu::SwapChainDescriptor {
                    usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    width: size.width as u32,
                    height: size.height as u32,
                    present_mode: wgpu::PresentMode::Mailbox,
                };

                swap_chain = device.create_swap_chain(&surface, &swap_chain_desc);
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput {
                    input: winit::event::KeyboardInput {
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
                *control_flow = ControlFlow::Exit;
            }
            Event::RedrawRequested(_) => {
                let now = Instant::now();
                imgui.io_mut().update_delta_time(now - last_frame);
                last_frame = now;

                let frame = match swap_chain.get_current_frame() {
                    Ok(frame) => frame,
                    Err(e) => {
                        eprintln!("dropped frame: {:?}", e);
                        return;
                    }
                };
                platform
                    .prepare_frame(imgui.io_mut(), &window)
                    .expect("Failed to prepare frame");
                let ui = imgui.frame();

                if let Some(new_path) = &pending_path {
                    file_info = load_file(new_path);
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
                                *control_flow = ControlFlow::Wait;
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
        
                let mut encoder: wgpu::CommandEncoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                if last_cursor != Some(ui.mouse_cursor()) {
                    last_cursor = Some(ui.mouse_cursor());
                    platform.prepare_render(&ui, &window);
                }

                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                            attachment: &frame.output.view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: true,
                            },
                        }],
                        depth_stencil_attachment: None,
                    });
    
                    renderer
                        .render(ui.render(), &queue, &device, &mut rpass)
                        .expect("Rendering failed");
                }

                queue.submit(Some(encoder.finish()));
            }
            _ => (),
        };
        platform.handle_event(imgui.io_mut(), &window, &event);
    });    
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