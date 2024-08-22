mod bsp_viewer;
mod cli;
mod export;
mod graphics;
mod hittest;
mod mdl_viewer;
mod mouse;
mod numerics;
mod rendering;
mod util;
mod wad_viewer;

use crate::mdl_viewer::MdlViewer;
use crate::wad_viewer::{load_wad_archive, WadViewer};
use clap::*;
use cli::Cli;
use export::bsp::{decode_atlas, read_textures, read_wad_resources, WadCollection};
use glam::Vec2;
use gsparser::bsp::{BspEntity, BspReader};
use gsparser::wad3::{WadArchive, WadFileInfo};
use imgui::*;
use imgui_wgpu::RendererConfig;
use mouse::{MouseInputController, MouseInputMode};
use rendering::bsp::{BspRenderer, DrawMode};
use rendering::Renderer;
use rfd::FileDialog;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use winit::event::DeviceEvent;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

const WINDOW_TITLE: &str = "goldsrc-asset-viewer";

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
    export::mdl::export(&mdl_file.file, export_file_path, log.as_mut()).unwrap();
    if let Some(log) = log {
        std::fs::write("log.txt", log).unwrap();
    }
}

fn export_bsp(file: &BspFile, export_file_path: &PathBuf, log: bool) {
    let mut log = if log { Some(String::new()) } else { None };
    let path = PathBuf::from(&file.path).canonicalize().unwrap();
    let game_root_path = get_game_root_path(&path).unwrap();
    export::bsp::export(game_root_path, &file.reader, export_file_path, log.as_mut()).unwrap();
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
    let mut renderer;

    if let Some(path) = &cli.file_path {
        file_info = load_file(path);
    }

    let event_loop = EventLoop::new().unwrap();
    let window = Window::new(&event_loop).unwrap();
    let _ = window.request_inner_size(LogicalSize::<f32>::new(1447.0, 867.0));
    if let Some(path) = &cli.file_path {
        window.set_title(&format!("{} - {}", WINDOW_TITLE, path.display()));
    } else {
        window.set_title(WINDOW_TITLE);
    }
    let size = window.inner_size();
    let wgpu_backend = if cfg!(target_os = "windows") {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::all()
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        //backends: wgpu::Backends::all(),
        backends: wgpu_backend,
        ..Default::default()
    });
    let surface = instance.create_surface(&window).unwrap();

    let hidpi_factor = window.scale_factor();

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .unwrap();
    let adapter_info = adapter.get_info();
    println!("Adapter: {:#?}", adapter_info);
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
        desired_maximum_frame_latency: 0,
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

    let mut pending_path: Option<PathBuf> = None;

    let mut mouse_controller = MouseInputController::new();
    let mut down_keys = HashSet::<KeyCode>::new();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop
        .run(|event, target| {
            let mut should_exit = false;
            let mut mouse_event = false;
            match event {
                Event::AboutToWait => window.request_redraw(),
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    window_id,
                } if window_id == window.id() => should_exit = true,
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
                        desired_maximum_frame_latency: 0,
                    };
                    surface.configure(&device, &surface_config);
                    if let Some(renderer) = renderer.as_mut() {
                        renderer.resize(&surface_config, &device, &queue);
                    }

                    let size = Vec2::new(size.width as f32, size.height as f32);
                    mouse_controller.on_resize(size);
                }
                Event::WindowEvent {
                    event:
                        WindowEvent::KeyboardInput {
                            event:
                                winit::event::KeyEvent {
                                    physical_key: PhysicalKey::Code(KeyCode::Escape),
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
                    should_exit = true;
                }
                Event::WindowEvent {
                    event:
                        WindowEvent::KeyboardInput {
                            event:
                                winit::event::KeyEvent {
                                    physical_key,
                                    state,
                                    ..
                                },
                            ..
                        },
                    ..
                } => {
                    if let PhysicalKey::Code(keycode) = physical_key {
                        let was_down = down_keys.get(&keycode).is_some();

                        if state == ElementState::Pressed {
                            down_keys.insert(keycode);
                        } else {
                            down_keys.remove(&keycode);
                        }

                        // TODO: Consolodate keyboard key up/down logic
                        if keycode == KeyCode::KeyB
                            && was_down
                            && down_keys.contains(&KeyCode::ShiftLeft)
                        {
                            let new_input_mode = match mouse_controller.input_mode() {
                                MouseInputMode::Cursor => MouseInputMode::CameraLook,
                                MouseInputMode::CameraLook => MouseInputMode::Cursor,
                            };
                            println!("Mouse input mode switched to {:?}", new_input_mode);
                            mouse_controller.set_input_mode(&window, new_input_mode);
                        }
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::CursorMoved { position, .. },
                    ..
                } => {
                    let position = Vec2::new(position.x as f32, position.y as f32);
                    mouse_controller.on_mouse_move(position);
                    mouse_event = true;
                }
                Event::DeviceEvent {
                    event: DeviceEvent::MouseMotion { delta },
                    ..
                } => {
                    let delta = Vec2::new(delta.0 as f32, delta.1 as f32);
                    mouse_controller.on_mouse_motion(delta);
                    mouse_event = true;
                }
                Event::WindowEvent {
                    event: WindowEvent::MouseInput { state, button, .. },
                    ..
                } => {
                    mouse_event = true;
                    if mouse_controller.input_mode() == MouseInputMode::Cursor {
                        if state == ElementState::Released
                            && button == winit::event::MouseButton::Left
                        {
                            if down_keys.contains(&KeyCode::ShiftLeft) {
                                if let Some(renderer) = renderer.as_mut() {
                                    renderer.process_shift_left_click(
                                        mouse_controller.mouse_position(),
                                        &file_info,
                                    );
                                }
                            }
                        } else if state == ElementState::Released
                            && button == winit::event::MouseButton::Right
                        {
                            if down_keys.contains(&KeyCode::ShiftLeft) {
                                if let Some(renderer) = renderer.as_mut() {
                                    renderer.process_shift_right_click(
                                        mouse_controller.mouse_position(),
                                        &file_info,
                                    );
                                }
                            }
                        }
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
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
                        window.set_title(&format!("{} - {}", WINDOW_TITLE, new_path.display()));
                        renderer = load_renderer(
                            file_info.as_ref(),
                            &device,
                            &queue,
                            surface_config.clone(),
                        );
                        pending_path = None;
                    }

                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());

                    // Rendering
                    let clear_op = if let Some(renderer) = renderer.as_mut() {
                        let mouse_delta = {
                            let mut mouse_delta = None;
                            if mouse_controller.input_mode() == MouseInputMode::CameraLook {
                                mouse_delta = mouse_controller.take_mouse_delta();
                            }
                            mouse_delta
                        };

                        let new_map = renderer.update(
                            &device,
                            &queue,
                            delta,
                            &down_keys,
                            mouse_delta,
                            &file_info,
                        );
                        if let Some((new_map, landmark, old_origin)) = new_map {
                            println!("Changing level to {}...", new_map);

                            let old_file = {
                                let file_info = file_info.take().unwrap();
                                match file_info {
                                    FileInfo::BspFile(file) => file,
                                    _ => panic!(),
                                }
                            };

                            let map_path = {
                                let mut path =
                                    PathBuf::from(&old_file.path).canonicalize().unwrap();
                                path.set_file_name(format!("{}.bsp", new_map));
                                path
                            };

                            file_info = load_file(map_path);
                            renderer.load_file(&file_info, &landmark, old_origin, &device, &queue);

                            // Change the timestamp to cut out the load time
                            last_frame = Instant::now();
                        }
                        renderer.render(clear_color, &view, &device, &queue);
                        wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }
                    } else {
                        wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear_color),
                            store: wgpu::StoreOp::Store,
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
                                        let mut log =
                                            if cli.log { Some(String::new()) } else { None };
                                        export::mdl::export(&mdl_file.file, new_path, log.as_mut())
                                            .unwrap();
                                        if let Some(log) = log {
                                            std::fs::write("log.txt", log).unwrap();
                                        }
                                    }
                                }
                                let is_bsp = if let Some(file_info) = file_info.as_ref() {
                                    match file_info {
                                        FileInfo::BspFile(_) => true,
                                        _ => false,
                                    }
                                } else {
                                    false
                                };
                                if ui
                                    .menu_item_config("Export light data")
                                    .enabled(is_bsp)
                                    .build()
                                {
                                    if let Some(new_path) = FileDialog::new()
                                        .add_filter("Binary Data", &["bin"])
                                        .set_directory("/")
                                        .save_file()
                                    {
                                        let bsp_file = if let Some(file_info) = file_info.as_ref() {
                                            match file_info {
                                                FileInfo::BspFile(file) => file,
                                                _ => panic!(),
                                            }
                                        } else {
                                            panic!()
                                        };
                                        export::bsp::export_light_data(&bsp_file.reader, new_path)
                                            .unwrap();
                                    }
                                }
                                if ui
                                    .menu_item_config("Export entity data")
                                    .enabled(is_bsp)
                                    .build()
                                {
                                    if let Some(new_path) = FileDialog::new()
                                        .add_filter("Text", &["txt"])
                                        .set_directory("/")
                                        .save_file()
                                    {
                                        let bsp_file = if let Some(file_info) = file_info.as_ref() {
                                            match file_info {
                                                FileInfo::BspFile(file) => file,
                                                _ => panic!(),
                                            }
                                        } else {
                                            panic!()
                                        };
                                        let entities = BspEntity::parse_entities(
                                            bsp_file.reader.read_entities(),
                                        );
                                        let entities_string = format!("{:#?}", entities);
                                        std::fs::write(new_path, entities_string).unwrap();
                                    }
                                }
                                if ui.menu_item("Exit") {
                                    should_exit = true;
                                }
                            });

                            if let Some(renderer) = renderer.as_mut() {
                                ui.menu("View", || {
                                    ui.menu("Draw mode", || {
                                        let mut draw_mode = renderer.get_draw_mode();
                                        if ui
                                            .menu_item_config("Texture")
                                            .selected(draw_mode == DrawMode::Texture)
                                            .build()
                                        {
                                            draw_mode = DrawMode::Texture;
                                        }
                                        if ui
                                            .menu_item_config("Lightmap")
                                            .selected(draw_mode == DrawMode::Lightmap)
                                            .build()
                                        {
                                            draw_mode = DrawMode::Lightmap;
                                        }
                                        if ui
                                            .menu_item_config("Lit Texture")
                                            .selected(draw_mode == DrawMode::LitTexture)
                                            .build()
                                        {
                                            draw_mode = DrawMode::LitTexture;
                                        }
                                        renderer.set_draw_mode(draw_mode);
                                    });
                                });
                            }

                            if let Some(renderer) = renderer.as_mut() {
                                renderer.build_ui_menu(ui);
                            }
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
                                FileInfo::BspFile(_) => {
                                    if let Some(renderer) = renderer.as_mut() {
                                        renderer.build_ui(ui, file_info);
                                    }
                                }
                            }
                        }
                    }

                    let mut encoder: wgpu::CommandEncoder = device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

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
                            timestamp_writes: None,
                            occlusion_query_set: None,
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
            if should_exit {
                target.exit();
            }
            if mouse_controller.input_mode() == MouseInputMode::Cursor || !mouse_event {
                platform.handle_event(imgui.io_mut(), &window, &event);
            }
        })
        .unwrap();
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
                let lightmap_atlas = decode_atlas(&file.reader);
                let map_models =
                    export::bsp::convert_models(&file.reader, &textures, &lightmap_atlas);

                let renderer =
                    BspRenderer::new(&file.reader, &map_models, &textures, device, queue, config);

                Some(Box::new(renderer))
            }
        }
    } else {
        None
    }
}
