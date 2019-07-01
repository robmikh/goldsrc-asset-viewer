use crate::graphics::{create_imgui_texture, TextureBundle, MipTexture};
use crate::WadFile;
use imgui::*;
use imgui_wgpu::Renderer;
use std::collections::HashMap;
use wad3parser::{ WadArchive, WadFileInfo, TextureType, CharInfo };

#[derive(Clone)]
pub struct FontMetadata {
    pub row_count: u32,
    pub row_height: u32,
    pub char_infos: [CharInfo; 256],
}

#[derive(Clone)]
pub struct ExtraTextureData {
    pub texture_type: TextureType,
    pub font: Option<FontMetadata>,
}

impl ExtraTextureData {
    pub fn new(texture_type: TextureType) -> ExtraTextureData {
        ExtraTextureData {
            texture_type: texture_type,
            font: None,
        }
    }
}

#[derive(Copy, Clone)]
struct WadViewerState {
    pub selected_file_index: i32,
    pub scale: f32,
    pub new_selection: bool,
    pub font_overlay: bool,
    pub texture_outline: bool,
}

impl WadViewerState {
    fn new() -> WadViewerState {
        WadViewerState {
            selected_file_index: 0,
            scale: 1.0,
            new_selection: false,
            font_overlay: false,
            texture_outline: false,
        }
    }

    fn copy_state(&mut self, other: &WadViewerState) {
        self.selected_file_index = other.selected_file_index;
        self.scale = other.scale;
        self.new_selection = other.new_selection;
        self.font_overlay = other.font_overlay;
        self.texture_outline = other.texture_outline;
    }
}

pub struct WadViewer {
    state: WadViewerState,
    texture_bundle: Option<TextureBundle<ExtraTextureData>>,
}

impl WadViewer {
    pub fn new() -> WadViewer {
        WadViewer {
            state: WadViewerState::new(),
            texture_bundle: None,
        }
    }

    pub fn pre_warm(&mut self, file_info: &WadFile, device: &mut wgpu::Device, renderer: &mut Renderer) {
        let info = file_info.files.get(&file_info.file_names[self.state.selected_file_index as usize]).unwrap();
        self.texture_bundle = Some(get_texture_bundle(&file_info.archive, &info, device, renderer));
    }

    pub fn reset_listbox_index(&mut self) {
        self.state.selected_file_index = 0;
    }

    pub fn build_ui(&mut self, ui: &Ui, file_info: &WadFile, device: &mut wgpu::Device, renderer: &mut Renderer, force_new_selection: bool) {
        let file_names = &file_info.file_names.iter().collect::<Vec<_>>();

        ui.window(im_str!["File list"])
        .size((300.0, 400.0), ImGuiCond::FirstUseEver)
        .build(|| {
            ui.text(im_str!["Path: {}", &file_info.path]);
            self.state.new_selection = ui.list_box(
                im_str!["Files"], 
                &mut self.state.selected_file_index,
                &file_names,
                file_names.len() as i32);
        });

        if self.state.new_selection || force_new_selection {
            // unbind our previous textures
            if let Some(texture_bundle) = self.texture_bundle.as_mut() {
                texture_bundle.clear(renderer);
            }

            let info = file_info.files.get(&file_info.file_names[self.state.selected_file_index as usize]).unwrap();
            self.texture_bundle = Some(get_texture_bundle(&file_info.archive, &info, device, renderer));
        }

        let mut temp_state = self.state.clone();
        if let Some(texture_bundle) = self.texture_bundle.as_ref() {
            ui.window(im_str!["File preview"])
                .position((500.0, 150.0), ImGuiCond::FirstUseEver)
                .size((300.0, 300.0), ImGuiCond::FirstUseEver)
                .horizontal_scrollbar(true)
                .build(|| {
                    ui.text(&file_names[temp_state.selected_file_index as usize]);
                    ui.text(im_str!["Type: {:?} (0x{:X})", texture_bundle.extra_data.texture_type, texture_bundle.extra_data.texture_type as u8]);
                    ui.text(im_str!["Size: {} x {}", texture_bundle.mip_textures[0].width, texture_bundle.mip_textures[0].height]);
                    match texture_bundle.extra_data.texture_type {
                        TextureType::Font => {
                            if let Some(font_data) = texture_bundle.extra_data.font.as_ref() {
                                ui.text(im_str!["Row Count: {}", font_data.row_count]);
                                ui.text(im_str!["Row Height: {}", font_data.row_height]);
                                ui.checkbox(im_str!["Char Info"], &mut temp_state.font_overlay);
                            }
                        },
                        _ => (),
                    }
                    ui.slider_float(im_str!["Scale"], &mut temp_state.scale, 1.0, 10.0)
                        .build();
                    ui.checkbox(im_str!["Texture outline"], &mut temp_state.texture_outline);
                    let (x, y) = ui.get_cursor_screen_pos();
                    for texture in &texture_bundle.mip_textures {
                        let (x, y) = ui.get_cursor_screen_pos();
                        ui.image(texture.texture_id, (texture.width as f32 * temp_state.scale, texture.height as f32 * temp_state.scale))
                        .build();
                        if temp_state.texture_outline {
                            ui.get_window_draw_list()
                                .add_rect((x, y), (x + ((texture.width as f32) * temp_state.scale), y + ((texture.height as f32) * temp_state.scale)), [0.0, 1.0, 0.0, 1.0])
                                .thickness(2.0)
                                .build();
                        }
                    }
                    if temp_state.font_overlay {
                        if let Some(font_data) = texture_bundle.extra_data.font.as_ref() {
                            let chars = font_data.char_infos.len();
                            for i in 0..chars {
                                let font_info = font_data.char_infos[i];
                                if font_info.width == 0 {
                                    continue;
                                }
                                let local_x = font_info.x as f32 * temp_state.scale;
                                let local_y = font_info.y as f32 * temp_state.scale;
                                let width = font_info.width as f32 * temp_state.scale;
                                let height = font_data.row_height as f32 * temp_state.scale;

                                let x = x + local_x;
                                let y = y + local_y;

                                ui.get_window_draw_list()
                                    .add_rect((x, y), (x + width, y + height), [1.0, 0.0, 0.0, 1.0])
                                    .thickness(2.0)
                                    .build();
                            }
                        }
                    }
                });
        }
        self.state.copy_state(&temp_state);
    }
}

pub fn get_decoded_data(
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

            extra_data.font = Some(FontMetadata {
                row_count: font_data.row_count,
                row_height: font_data.row_height,
                char_infos: font_data.font_info,
            });
            vec![font_data.image]
        } else {
            panic!("New texture type! {:?}", info.texture_type);
        }
    };
    (datas, extra_data)
}

pub fn get_texture_bundle(
    archive: &WadArchive, 
    info: &WadFileInfo,
    device: &mut wgpu::Device,
    renderer: &mut Renderer,
) -> TextureBundle<ExtraTextureData> {
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

pub fn load_wad_archive(archive: &WadArchive) -> (HashMap<ImString, WadFileInfo>, Vec<ImString>) {
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

