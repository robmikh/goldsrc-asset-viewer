mod cli;
mod gltf;
mod graphics;
mod mdl_viewer;
mod numerics;
mod wad_viewer;

use crate::mdl_viewer::MdlViewer;
use crate::wad_viewer::{load_wad_archive, WadViewer};
use clap::*;
use cli::Cli;
use imgui::*;
use imgui_wgpu::{Renderer, RendererConfig};
use rfd::FileDialog;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use wad3parser::{WadArchive, WadFileInfo};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

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
    WadFile(WadFile),
    MdlFile(MdlFile),
}

fn main() {
    let cli = Cli::parse();

    if cli.export_file_path.is_none() {
        show_ui(cli);
    } else {
        if let Some(file_path) = cli.file_path {
            let file_info = load_file(file_path).unwrap();
            let mdl_file = match file_info {
                FileInfo::MdlFile(file) => file,
                _ => panic!(),
            };
            let export_file_path = cli.export_file_path.unwrap();

            let mut log = if cli.log {
                Some(String::new())
            } else {
                None
            };
            gltf::export::export(&mdl_file.file, export_file_path, log.as_mut()).unwrap();
            if let Some(log) = log {
                std::fs::write("log.txt", log).unwrap();
            }
            println!("Done!");
        } else {
            panic!("Expected input path!");
        }
    }
}

fn show_ui(cli: Cli) {
    env_logger::init();

    let mut file_info = None;

    if let Some(path) = &cli.file_path {
        file_info = load_file(path);
    }

    let event_loop = EventLoop::new();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let (window, size, surface) = {
        let window = Window::new(&event_loop).unwrap();
        window.set_inner_size(LogicalSize::<f32>::new(1447.0, 867.0));
        window.set_title("goldsrc-asset-viewer");
        let size = window.inner_size();
        let surface = unsafe { instance.create_surface(&window).unwrap() };
        (window, size, surface)
    };

    let hidpi_factor = window.scale_factor();

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .unwrap();
    let (mut device, mut queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
            .unwrap();

    let surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8Unorm,
        width: size.width as u32,
        height: size.height as u32,
        present_mode: wgpu::PresentMode::Mailbox,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![wgpu::TextureFormat::Rgba8Unorm],
    };
    surface.configure(&device, &surface_config);

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
        texture_format: surface_config.format,
        ..Default::default()
    };

    let mut renderer = Renderer::new(&mut imgui, &device, &queue, renderer_config);

    let mut last_frame = Instant::now();
    let mut last_cursor = None;
    //let mut demo_open = true;
    let mut wad_viewer = WadViewer::new();
    let mut mdl_viewer = MdlViewer::new();

    let mut pending_path: Option<PathBuf> = None;

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

                let surface_config = wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    width: size.width as u32,
                    height: size.height as u32,
                    present_mode: wgpu::PresentMode::Mailbox,
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    view_formats: vec![wgpu::TextureFormat::Rgba8Unorm],
                };
                surface.configure(&device, &surface_config);
            }
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        input:
                            winit::event::KeyboardInput {
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

                let frame = match surface.get_current_texture() {
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
                        ui.menu("File", || {
                            if ui.menu_item_config("Open").shortcut("Ctrl+O").build() {
                                if let Some(new_path) = FileDialog::new()
                                    .add_filter("Half-Life Assets", &["wad", "mdl"])
                                    .set_directory("/")
                                    .pick_file()
                                {
                                    pending_path = Some(new_path);
                                }
                            }
                            let is_mdl = if let Some(file_info) = file_info.as_ref() {
                                match file_info {
                                    FileInfo::MdlFile(_) => true,
                                    _ => false,
                                }
                            } else {
                                false
                            };
                            if ui.menu_item_config("Export").enabled(is_mdl).build() {
                                if let Some(new_path) = FileDialog::new()
                                    .add_filter("GLTF File", &["gltf"])
                                    .set_directory("/")
                                    .save_file()
                                {
                                    let mdl_file = if let Some(file_info) = file_info.as_ref() {
                                        match file_info {
                                            FileInfo::MdlFile(file) => file,
                                            _ => panic!(),
                                        }
                                    } else {
                                        panic!()
                                    };
                                    let mut log = if cli.log {
                                        Some(String::new())
                                    } else {
                                        None
                                    };
                                    gltf::export::export(&mdl_file.file, new_path, log.as_mut()).unwrap();
                                    if let Some(log) = log {
                                        std::fs::write("log.txt", log).unwrap();
                                    }
                                }
                            }
                            if ui.menu_item("Exit") {
                                *control_flow = ControlFlow::Exit;
                            }
                        });
                    });

                    if let Some(file_info) = file_info.as_ref() {
                        match file_info {
                            FileInfo::WadFile(file_info) => wad_viewer.build_ui(
                                &ui,
                                &file_info,
                                &mut device,
                                &mut queue,
                                &mut renderer,
                            ),
                            FileInfo::MdlFile(file_info) => mdl_viewer.build_ui(
                                &ui,
                                &file_info,
                                &mut device,
                                &mut queue,
                                &mut renderer,
                            ),
                        }
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
                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: None,
                    });

                    renderer
                        .render(imgui.render(), &queue, &device, &mut rpass)
                        .expect("Rendering failed");
                }

                queue.submit(Some(encoder.finish()));
                frame.present();
            }
            _ => (),
        };
        platform.handle_event(imgui.io_mut(), &window, &event);
    });
}

fn get_extension_from_path<P: AsRef<Path>>(path: P) -> Option<String> {
    let path = path.as_ref();
    let extension = path.extension()?;
    let extension_str = extension.to_str()?;
    Some(extension_str.to_owned())
}

fn load_wad_file<P: AsRef<Path>>(path: P) -> WadFile {
    let path = path.as_ref();
    let archive = WadArchive::open(path);
    let (files, file_names) = load_wad_archive(&archive);
    WadFile {
        path: path.display().to_string(),
        archive: archive,
        files: files,
        file_names: file_names,
    }
}

fn load_mdl_file<P: AsRef<Path>>(path: P) -> MdlFile {
    let path = path.as_ref();
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
        path: path.display().to_string(),
        file: mdl_file,
        texture_names: texture_names,
        body_part_names: body_part_names,
    }
}

fn load_file<P: AsRef<Path>>(path: P) -> Option<FileInfo> {
    let path = path.as_ref();
    if let Some(extension) = get_extension_from_path(path) {
        match extension.as_str() {
            "wad" => Some(FileInfo::WadFile(load_wad_file(path))),
            "mdl" => Some(FileInfo::MdlFile(load_mdl_file(path))),
            _ => None,
        }
    } else {
        None
    }
}
