use crate::graphics::*;
use crate::BspFile;
use gsparser::mdl::MdlTexture;
use imgui::*;
use imgui_wgpu::Renderer;

#[derive(Clone)]
pub struct ExtraTextureData {}

#[derive(Copy, Clone)]
struct BspViewerState {
    pub selected_file_index: i32,
}

impl BspViewerState {
    fn new() -> Self {
        Self {
            selected_file_index: 0,
        }
    }

    fn copy_state(&mut self, other: &Self) {
        self.selected_file_index = other.selected_file_index;
    }
}

pub struct BspViewer {
    state: BspViewerState,
    last_file_path: String,
}

impl BspViewer {
    pub fn new() -> Self {
        Self {
            state: BspViewerState::new(),
            last_file_path: String::new(),
        }
    }

    fn reset_listbox_index(&mut self) {
        self.state.selected_file_index = 0;
    }

    pub fn build_ui(
        &mut self,
        ui: &Ui,
        file_info: &BspFile,
        device: &mut wgpu::Device,
        queue: &mut wgpu::Queue,
        renderer: &mut Renderer,
    ) {
        let mut force_new_selection = false;

        if self.last_file_path != file_info.path {
            self.last_file_path = file_info.path.clone();
            self.reset_listbox_index();
            force_new_selection = true;
        }

        ui.window("Texture list")
            .size([300.0, 400.0], Condition::FirstUseEver)
            .build(|| {
                ui.text(format!("Path: {}", &file_info.path));
            });
    }
}

pub fn get_texture_bundle(
    texture: &MdlTexture,
    device: &mut wgpu::Device,
    queue: &mut wgpu::Queue,
    renderer: &mut Renderer,
) -> TextureBundle<ExtraTextureData> {
    let width = texture.width;
    let height = texture.height;

    let texture = create_imgui_texture(device, queue, renderer, texture.image_data.clone());
    let texture_id = renderer.textures.insert(texture);

    let textures = vec![MipTexture {
        texture_id: texture_id,
        width: width,
        height: height,
    }];

    TextureBundle {
        mip_textures: textures,
        extra_data: ExtraTextureData {},
    }
}
