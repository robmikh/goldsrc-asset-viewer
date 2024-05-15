use std::collections::HashMap;

use crate::graphics::*;
use crate::BspFile;
use glam::Vec3;
use gsparser::bsp::BspEntity;
use gsparser::mdl::MdlTexture;
use imgui::*;
use imgui_wgpu::Renderer;

#[derive(Clone)]
pub struct ExtraTextureData {}

#[derive(Copy, Clone)]
struct BspViewerState {
    pub selected_entity_index: i32,
    pub position: Vec3,
}

impl BspViewerState {
    fn new() -> Self {
        Self {
            selected_entity_index: -1,
            position: Vec3::ZERO,
        }
    }

    fn copy_state(&mut self, other: &Self) {
        self.selected_entity_index = other.selected_entity_index;
        self.position = other.position;
    }
}

pub struct BspViewer {
    state: BspViewerState,
    last_file_path: String,
    cached_entities: Vec<HashMap<String, String>>,
    entities: String,
}

impl BspViewer {
    pub fn new() -> Self {
        Self {
            state: BspViewerState::new(),
            last_file_path: String::new(),
            cached_entities: Vec::new(),
            entities: String::new(),
        }
    }

    fn reset_listbox_index(&mut self) {
        self.state.selected_entity_index = 0;
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

            self.cached_entities = BspEntity::parse_entities(file_info.reader.read_entities()).iter().map(|x| {
                let mut result = HashMap::new();
                for (key, value) in &x.0 {
                    result.insert((*key).to_owned(), (*value).to_owned());
                }
                result
            }).collect();

            self.entities = format!("{:#?}", self.cached_entities);
        }

        ui.window("Map Info")
            .size([300.0, 400.0], Condition::FirstUseEver)
            .build(|| {
                ui.text(format!("Path: {}", &file_info.path));
                ui.text("Position:");
                ui.text(format!("  x: {}", self.state.position.x));
                ui.text(format!("  y: {}", self.state.position.y));
                ui.text(format!("  z: {}", self.state.position.z));
                ui.text("Selected Entity:");
                if self.state.selected_entity_index >= 0 && (self.state.selected_entity_index as usize) < self.cached_entities.len() {
                    let entity = &self.cached_entities[self.state.selected_entity_index as usize];
                    let text = format!("{:#?}", entity);
                    ui.text(text);
                }
            });

        ui.window("Entities")
            .position([50.0, 475.0], Condition::FirstUseEver)
            .size([300.0, 400.0], Condition::FirstUseEver)
            .build(|| {
                ui.text(&self.entities);
            });
    }

    pub fn select_entity(&mut self, index: i32) {
        self.state.selected_entity_index = index;
    }

    pub fn set_position(&mut self, position: Vec3) {
        self.state.position = position;
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
