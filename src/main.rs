mod bsp_viewer;
mod cli;
mod gltf;
mod graphics;
mod hittest;
mod mdl_viewer;
mod numerics;
mod rendering;
mod wad_viewer;

use crate::mdl_viewer::MdlViewer;
use crate::wad_viewer::{load_wad_archive, WadViewer};
use bsp_viewer::BspViewer;
use clap::*;
use cli::Cli;
use glam::Vec2;
use gltf::bsp::{read_textures, read_wad_resources, WadCollection};
use gsparser::bsp::{BspEntity, BspReader};
use gsparser::wad3::{WadArchive, WadFileInfo};
use hittest::hittest_node_for_leaf;
use imgui::*;
use imgui_wgpu::RendererConfig;
use rendering::bsp::BspRenderer;
use rendering::Renderer;
use rfd::FileDialog;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

pub struct MdlFile {
    pub path: String,
    pub file: gsparser::mdl::MdlFile,
    pub texture_names: Vec<ImString>,
    pub body_part_names: Vec<ImString>,
}

pub struct WadFile {
    pub path: String,
    pub archive: WadArchive,
    pub files: HashMap<ImString, WadFileInfo>,
    pub file_names: Vec<ImString>,
}

pub struct BspFile {
    pub path: String,
    pub reader: BspReader,
}

enum FileInfo {
    WadFile(WadFile),
    MdlFile(MdlFile),
    BspFile(BspFile),
}

fn main() {
    let cli = Cli::parse();

    if cli.export_file_path.is_none() {
        show_ui(cli);
    } else {
        if let Some(file_path) = cli.file_path {
            let export_file_path = cli.export_file_path.unwrap();
            let file_info = load_file(file_path).unwrap();
            match file_info {
                FileInfo::MdlFile(file) => export_mdl(&file, &export_file_path, cli.log),
                FileInfo::BspFile(file) => export_bsp(&file, &export_file_path, cli.log),
                _ => panic!(),
            }
            println!("Done!");
        } else {
            panic!("Expected input path!");
        }
    }
}

fn export_mdl(mdl_file: &MdlFile, export_file_path: &PathBuf, log: bool) {
    let mut log = if log { Some(String::new()) } else { None };
    gltf::mdl::export(&mdl_file.file, export_file_path, log.as_mut()).unwrap();
    if let Some(log) = log {
        std::fs::write("log.txt", log).unwrap();
    }
}

fn export_bsp(file: &BspFile, export_file_path: &PathBuf, log: bool) {
    let mut log = if log { Some(String::new()) } else { None };
    let path = PathBuf::from(&file.path).canonicalize().unwrap();
    let game_root_path = get_game_root_path(&path).unwrap();
    gltf::bsp::export(
        game_root_path,
        &file.reader,
        export_file_path,
        true,
        log.as_mut(),
    )
    .unwrap();
    if let Some(log) = log {
        std::fs::write("log.txt", log).unwrap();
    }
}

fn get_game_root_path(path: &Path) -> Option<&Path> {
    path.ancestors().skip(1).find(|x| {
        assert!(x.is_dir(), "{:?}", x);
        let file_stem = x.file_stem().unwrap().to_str().unwrap();
        file_stem == "Half-Life"
    })
}

fn show_ui(cli: Cli) {
    env_logger::init();

    let mut file_info = None;
    let mut renderer = None;

    if let Some(path) = &cli.file_path {
        file_info = load_file(path);
    }

    let event_loop = EventLoop::new();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        //backends: wgpu::Backends::DX12,
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

    let mut surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8Unorm,
        width: size.width as u32,
        height: size.height as u32,
        present_mode: wgpu::PresentMode::Mailbox,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![wgpu::TextureFormat::Rgba8Unorm],
    };
    surface.configure(&device, &surface_config);

    renderer = load_renderer(file_info.as_ref(), &device, &queue, surface_config.clone());

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

    let mut imgui_renderer =
        imgui_wgpu::Renderer::new(&mut imgui, &device, &queue, renderer_config);

    let mut last_frame = Instant::now();
    let mut last_cursor = None;
    let mut wad_viewer = WadViewer::new();
    let mut mdl_viewer = MdlViewer::new();
    let mut bsp_viewer = BspViewer::new();

    let mut pending_path: Option<PathBuf> = None;

    let mut current_mouse_position = Vec2::new(0.0, 0.0);
    let mut down_keys = HashSet::<VirtualKeyCode>::new();
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

                surface_config = wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    width: size.width as u32,
                    height: size.height as u32,
                    present_mode: wgpu::PresentMode::Mailbox,
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    view_formats: vec![wgpu::TextureFormat::Rgba8Unorm],
                };
                surface.configure(&device, &surface_config);
                if let Some(renderer) = renderer.as_mut() {
                    renderer.resize(&surface_config, &device, &queue);
                }
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
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        input:
                            winit::event::KeyboardInput {
                                virtual_keycode,
                                state,
                                ..
                            },
                        ..
                    },
                ..
            } => {
                if let Some(keycode) = virtual_keycode {
                    if state == ElementState::Pressed {
                        down_keys.insert(keycode);
                    } else {
                        down_keys.remove(&keycode);
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                current_mouse_position = Vec2::new(position.x as f32, position.y as f32);
            }
            Event::WindowEvent {
                event: WindowEvent::MouseInput { state, button, .. },
                ..
            } => {
                if state == ElementState::Released && button == winit::event::MouseButton::Left {
                    if down_keys.contains(&VirtualKeyCode::LShift) {
                        if let Some(renderer) = renderer.as_ref() {
                            let (pos, ray) =
                                renderer.world_pos_and_ray_from_screen_pos(current_mouse_position);

                            println!("pos: {:?}    ray: {:?}", pos, ray);
                            if let Some(file_info) = file_info.as_ref() {
                                match file_info {
                                    FileInfo::BspFile(file_info) => {
                                        if let Some(leaf_index) =
                                            hittest_node_for_leaf(&file_info.reader, 0, pos, ray)
                                        {
                                            println!("Hit something... {}", leaf_index);
                                            // Get the face indices from the leaf
                                            let leaf = &file_info.reader.read_leaves()[leaf_index];
                                            let mark_surfaces_range = leaf.first_mark_surface
                                                ..leaf.first_mark_surface + leaf.mark_surfaces;

                                            let mark_surfaces =
                                                file_info.reader.read_mark_surfaces();
                                            let mut leaf_faces = HashSet::new();
                                            for mark_surface_index in mark_surfaces_range {
                                                let mark_surface =
                                                    &mark_surfaces[mark_surface_index as usize];
                                                leaf_faces.insert(mark_surface.0 as usize);
                                            }

                                            // Build entity map
                                            let entities = BspEntity::parse_entities(
                                                file_info.reader.read_entities(),
                                            );
                                            let mut model_to_entity = Vec::new();
                                            for (entity_index, entity) in
                                                entities.iter().enumerate()
                                            {
                                                if let Some(value) = entity.0.get("model") {
                                                    if value.starts_with('*') {
                                                        let model_ref: usize = value
                                                            .trim_start_matches('*')
                                                            .parse()
                                                            .unwrap();
                                                        model_to_entity
                                                            .push((model_ref, entity_index));
                                                    }
                                                }
                                            }

                                            let models = file_info.reader.read_models();
                                            let mut found = None;
                                            'find_entity: for (model_index, entity_index) in
                                                model_to_entity
                                            {
                                                let model = models[model_index];
                                                let faces_start = model.first_face as usize;
                                                let faces_len = model.faces as usize;
                                                let faces_end = faces_start + faces_len;
                                                for i in faces_start..faces_end {
                                                    if leaf_faces.contains(&i) {
                                                        found = Some(entity_index);
                                                        break 'find_entity;
                                                    }
                                                }
                                            }

                                            if let Some(entity_index) = found {
                                                println!("woop");
                                                bsp_viewer.select_entity(entity_index as i32);
                                            }
                                        }
                                    }
                                    _ => (),
                                }
                            }
                        }
                    }
                }
            }
            Event::RedrawRequested(_) => {
                let now = Instant::now();
                let delta = now - last_frame;
                last_frame = now;

                let frame = match surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(e) => {
                        eprintln!("dropped frame: {:?}", e);
                        return;
                    }
                };

                if let Some(new_path) = &pending_path {
                    file_info = load_file(new_path);
                    renderer =
                        load_renderer(file_info.as_ref(), &device, &queue, surface_config.clone());
                    pending_path = None;
                }

                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Rendering
                let clear_op = if let Some(renderer) = renderer.as_mut() {
                    renderer.update(&device, &queue, delta, &down_keys);
                    let position = renderer.get_position();
                    bsp_viewer.set_position(position);
                    renderer.render(clear_color, &view, &device, &queue);
                    wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    }
                } else {
                    wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: true,
                    }
                };

                // UI
                imgui.io_mut().update_delta_time(delta);
                platform
                    .prepare_frame(imgui.io_mut(), &window)
                    .expect("Failed to prepare frame");
                let ui = imgui.frame();

                {
                    ui.main_menu_bar(|| {
                        ui.menu("File", || {
                            if ui.menu_item_config("Open").shortcut("Ctrl+O").build() {
                                if let Some(new_path) = FileDialog::new()
                                    .add_filter("Half-Life Assets", &["wad", "mdl", "bsp"])
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
                                    let mut log = if cli.log { Some(String::new()) } else { None };
                                    gltf::mdl::export(&mdl_file.file, new_path, log.as_mut())
                                        .unwrap();
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
                                &mut imgui_renderer,
                            ),
                            FileInfo::MdlFile(file_info) => mdl_viewer.build_ui(
                                &ui,
                                &file_info,
                                &mut device,
                                &mut queue,
                                &mut imgui_renderer,
                            ),
                            FileInfo::BspFile(file_info) => bsp_viewer.build_ui(
                                &ui,
                                &file_info,
                                &mut device,
                                &mut queue,
                                &mut imgui_renderer,
                            ),
                            _ => todo!(),
                        }
                    }
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
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: clear_op,
                        })],
                        depth_stencil_attachment: None,
                    });

                    imgui_renderer
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
    let mdl_file = gsparser::mdl::MdlFile::open(path);

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

fn load_bsp_file<P: AsRef<Path>>(path: P) -> BspFile {
    let path = path.as_ref();
    let data = std::fs::read(path).unwrap();
    let reader = BspReader::read(data);

    BspFile {
        path: path.display().to_string(),
        reader,
    }
}

fn load_file<P: AsRef<Path>>(path: P) -> Option<FileInfo> {
    let path = path.as_ref();
    if let Some(extension) = get_extension_from_path(path) {
        match extension.as_str() {
            "wad" => Some(FileInfo::WadFile(load_wad_file(path))),
            "mdl" => Some(FileInfo::MdlFile(load_mdl_file(path))),
            "bsp" => Some(FileInfo::BspFile(load_bsp_file(path))),
            _ => None,
        }
    } else {
        None
    }
}

fn load_renderer(
    file_info: Option<&FileInfo>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
) -> Option<Box<dyn Renderer>> {
    if let Some(file_info) = file_info {
        match file_info {
            FileInfo::WadFile(_) => None,
            FileInfo::MdlFile(_) => None,
            FileInfo::BspFile(file) => {
                let path = PathBuf::from(&file.path).canonicalize().unwrap();
                let game_root_path = get_game_root_path(&path).unwrap();

                let mut wad_resources = WadCollection::new();
                read_wad_resources(&file.reader, &game_root_path, &mut wad_resources);

                let textures = read_textures(&file.reader, &wad_resources);
                let model = gltf::bsp::convert(&file.reader, &textures, true);

                let renderer =
                    BspRenderer::new(&file.reader, &model, &textures, device, queue, config);

                Some(Box::new(renderer))
            }
        }
    } else {
        None
    }
}
