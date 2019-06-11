use crate::graphics::*;
use crate::MdlFile;
use imgui::*;
use imgui_wgpu::Renderer;
use mdlparser::{ MdlTexture };

#[derive(Clone)]
pub struct ExtraTextureData {
}

#[derive(Copy, Clone)]
struct MdlViewerState {
    pub selected_file_index: i32,
    pub scale: f32,
    pub new_selection: bool,
    pub texture_outline: bool,
}

impl MdlViewerState {
    fn new() -> MdlViewerState {
        MdlViewerState {
            selected_file_index: 0,
            scale: 1.0,
            new_selection: false,
            texture_outline: false,
        }
    }

    fn copy_state(&mut self, other: &MdlViewerState) {
        self.selected_file_index = other.selected_file_index;
        self.scale = other.scale;
        self.new_selection = other.new_selection;
        self.texture_outline = other.texture_outline;
    }
}

pub struct MdlViewer {
    state: MdlViewerState,
    texture_bundle: Option<TextureBundle<ExtraTextureData>>,
}

impl MdlViewer {
    pub fn new() -> MdlViewer {
        MdlViewer {
            state: MdlViewerState::new(),
            texture_bundle: None,
        }
    }

    pub fn pre_warm(&mut self, file_info: &MdlFile, device: &mut wgpu::Device, renderer: &mut Renderer) {
        let info = &file_info.file.textures[self.state.selected_file_index as usize];
        self.texture_bundle = Some(get_texture_bundle(&info, device, renderer));
    }

    pub fn reset_listbox_index(&mut self) {
        self.state.selected_file_index = 0;
    }

    pub fn build_ui(&mut self, ui: &Ui, file_info: &MdlFile, device: &mut wgpu::Device, renderer: &mut Renderer, force_new_selection: bool) {
        let texture_names = &file_info.texture_names.iter().collect::<Vec<_>>();

        ui.window(im_str!["Texture list"])
        .size((300.0, 400.0), ImGuiCond::FirstUseEver)
        .build(|| {
            ui.text(im_str!["Path: {}", &file_info.path]);
            ui.text(im_str!["Name: {}", &file_info.file.name]);
            self.state.new_selection = ui.list_box(
                im_str!["Textures"], 
                &mut self.state.selected_file_index,
                &texture_names,
                texture_names.len() as i32);
        });

        if self.state.new_selection || force_new_selection {
            // unbind our previous textures
            if let Some(texture_bundle) = self.texture_bundle.as_mut() {
                texture_bundle.clear(renderer);
            }

            let info = &file_info.file.textures[self.state.selected_file_index as usize];
            self.texture_bundle = Some(get_texture_bundle(&info, device, renderer));
        }

        let mut temp_state = self.state.clone();
        if let Some(texture_bundle) = self.texture_bundle.as_ref() {
            ui.window(im_str!["Texture preview"])
                .position((500.0, 150.0), ImGuiCond::FirstUseEver)
                .size((300.0, 300.0), ImGuiCond::FirstUseEver)
                .horizontal_scrollbar(true)
                .build(|| {
                    ui.text(&texture_names[temp_state.selected_file_index as usize]);
                    ui.text(im_str!["Size: {} x {}", texture_bundle.mip_textures[0].width, texture_bundle.mip_textures[0].height]);
                    ui.slider_float(im_str!["Scale"], &mut temp_state.scale, 1.0, 10.0)
                        .build();
                    ui.checkbox(im_str!["Texture outline"], &mut temp_state.texture_outline);
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
                });
        }
        self.state.copy_state(&temp_state);
    }
}

pub fn get_texture_bundle(
    texture: &MdlTexture,
    device: &mut wgpu::Device,
    renderer: &mut Renderer,
) -> TextureBundle<ExtraTextureData> {
    let width = texture.width;
    let height = texture.height;

    let texture = create_imgui_texture(device, renderer.texture_layout(), texture.image_data.clone());
    let texture_id = renderer.textures().insert(texture);

    let textures = vec![
        MipTexture {
            texture_id: texture_id,
            width: width,
            height: height,
        }
    ];

    TextureBundle {
        mip_textures: textures, 
        extra_data: ExtraTextureData {},
    }
}