use std::collections::HashMap;

use crate::{
    export::{bsp::ModelVertex, coordinates::convert_coordinates},
    logic::entity::{Entity, ModelReference},
    BspFile,
};
use glam::Vec3;
use gltf::Model;
use gsparser::bsp::{BspEntity, BspReader};
use imgui::*;

#[derive(Copy, Clone)]
pub struct BspViewerState {
    pub selected_entity_index: i32,
    pub position: Vec3,
    pub direction: Vec3,
    pub noclip: bool,
    pub gravity: bool,
    pub render_all: bool,
    pub disable_level_change: bool,
}

impl BspViewerState {
    fn new() -> Self {
        Self {
            selected_entity_index: -1,
            position: Vec3::ZERO,
            direction: Vec3::ZERO,
            noclip: false,
            gravity: true,
            render_all: false,
            disable_level_change: true,
        }
    }
}

pub struct BspViewer {
    state: BspViewerState,
    cached_entities: Vec<HashMap<String, String>>,
    entities: Vec<String>,
    entity_positions: HashMap<usize, Vec3>,
}

impl BspViewer {
    pub fn new() -> Self {
        Self {
            state: BspViewerState::new(),
            cached_entities: Vec::new(),
            entities: Vec::new(),
            entity_positions: HashMap::new(),
        }
    }

    fn reset_listbox_index(&mut self) {
        self.state.selected_entity_index = 0;
    }

    pub fn state(&self) -> &BspViewerState {
        &self.state
    }

    pub fn build_menu(&mut self, ui: &Ui) {
        ui.menu("Game", || {
            if ui
                .menu_item_config("Noclip")
                .selected(self.state.noclip)
                .build()
            {
                self.state.noclip = !self.state.noclip;
            }

            if ui
                .menu_item_config("Gravity")
                .selected(self.state.gravity)
                .build()
            {
                self.state.gravity = !self.state.gravity;
            }

            if ui
                .menu_item_config("Render All")
                .selected(self.state.render_all)
                .build()
            {
                self.state.render_all = !self.state.render_all;
            }

            if ui
                .menu_item_config("Disable Level Change")
                .selected(self.state.disable_level_change)
                .build()
            {
                self.state.disable_level_change = !self.state.disable_level_change;
            }
        });
    }

    pub fn set_new_file(
        &mut self,
        reader: &BspReader,
        entities: &[Entity],
        models: &[Model<ModelVertex>],
    ) {
        self.reset_listbox_index();

        self.cached_entities = BspEntity::parse_entities(reader.read_entities_str())
            .iter()
            .map(|x| {
                let mut result = HashMap::new();
                for (key, value) in &x.0 {
                    result.insert((*key).to_owned(), (*value).to_owned());
                }
                result
            })
            .collect();

        self.entities = self
            .cached_entities
            .iter()
            .map(|x| format!("{:#?}", x))
            .collect();

        self.entity_positions = HashMap::new();
        for (i, entity) in entities.iter().enumerate() {
            let position = {
                let mut model_position = None;
                if let Some(model_ref) = entity.model.as_ref() {
                    if let ModelReference::Index(model_index) = model_ref {
                        let model = &models[*model_index];
                        // Average the offests
                        let offset = model
                            .vertices
                            .iter()
                            .map(|x| Vec3::from_array(x.pos))
                            .sum::<Vec3>()
                            / model.vertices.len() as f32;
                        model_position = Some(offset);
                    }
                }

                let origin = entity.origin.map(|hl_origin| {
                    let coord = convert_coordinates(hl_origin);
                    Vec3::new(coord[0] as f32, coord[1] as f32, coord[2] as f32)
                });

                if model_position.is_some() && origin.is_some() {
                    Some(model_position.unwrap() + origin.unwrap())
                } else if model_position.is_some() {
                    model_position
                } else {
                    origin
                }
            };

            if let Some(position) = position {
                self.entity_positions.insert(i, position);
            }
        }
    }

    pub fn build_ui(&mut self, ui: &Ui, file_info: &BspFile) -> Option<Vec3> {
        let mut new_position = None;

        ui.window("Map Info")
            .position([25.0, 25.0], Condition::FirstUseEver)
            .size([300.0, 400.0], Condition::FirstUseEver)
            .build(|| {
                ui.text(format!("Path: {}", &file_info.path));
                ui.text("Position:");
                ui.text(format!("  x: {}", self.state.position.x));
                ui.text(format!("  y: {}", self.state.position.y));
                ui.text(format!("  z: {}", self.state.position.z));
                ui.text("Direction:");
                ui.text(format!("  x: {}", self.state.direction.x));
                ui.text(format!("  y: {}", self.state.direction.y));
                ui.text(format!("  z: {}", self.state.direction.z));
                ui.text("Selected Entity:");
                if self.state.selected_entity_index >= 0
                    && (self.state.selected_entity_index as usize) < self.cached_entities.len()
                {
                    let entity = &self.cached_entities[self.state.selected_entity_index as usize];
                    let text = format!("{:#?}", entity);
                    ui.text(text);
                }
            });

        ui.window("Entities")
            .position([25.0, 450.0], Condition::FirstUseEver)
            .size([300.0, 400.0], Condition::FirstUseEver)
            .build(|| {
                let mut value = self.state.selected_entity_index;
                if ui.input_int("Entity Index", &mut value).build() {
                    if value < 0 || value < self.entities.len() as i32 {
                        self.state.selected_entity_index = value;
                    }
                }

                if self.state.selected_entity_index >= 0 {
                    if let Some(position) = self
                        .entity_positions
                        .get(&(self.state.selected_entity_index as usize))
                    {
                        if ui.button("Jump To Entity") {
                            new_position = Some(*position);
                        }
                    }
                }

                let entities: Vec<_> = self.entities.iter().collect();
                if ui.list_box(
                    "Entities",
                    &mut self.state.selected_entity_index,
                    &entities,
                    self.entities.len() as i32,
                ) {}
            });

        new_position
    }

    pub fn select_entity(&mut self, index: i32) {
        self.state.selected_entity_index = index;
    }

    pub fn set_position(&mut self, position: Vec3, facing: Vec3) {
        self.state.position = position;
        self.state.direction = facing;
    }

    pub fn build_spawn_window(&self, ui: &Ui, spawns: &[(Vec3, f32)]) -> Option<usize> {
        let mut pressed = None;
        ui.window("Spawns")
            .position([1140.0, 25.0], Condition::FirstUseEver)
            .size([300.0, 150.0], Condition::FirstUseEver)
            .build(|| {
                for (i, (position, angle)) in spawns.iter().enumerate() {
                    if ui.button(format!(
                        "{}, {}, {} ({})",
                        position.x, position.y, position.z, angle
                    )) {
                        pressed = Some(i);
                    }
                }
            });
        pressed
    }
}
