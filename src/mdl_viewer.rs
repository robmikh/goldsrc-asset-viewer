use crate::graphics::*;
use crate::MdlFile;
use imgui::*;
use imgui_wgpu::Renderer;
use mdlparser::MdlTexture;

#[derive(Clone)]
pub struct ExtraTextureData {}

#[derive(Copy, Clone)]
struct MdlViewerState {
    pub selected_file_index: i32,
    pub scale: f32,
    pub new_selection: bool,
    pub texture_outline: bool,

    pub new_body_part_selection: bool,
    pub selected_body_part_index: i32,

    pub new_model_selection: bool,
    pub selected_model_index: i32,

    pub new_mesh_selection: bool,
    pub selected_mesh_index: i32,
}

impl MdlViewerState {
    fn new() -> MdlViewerState {
        MdlViewerState {
            selected_file_index: 0,
            scale: 1.0,
            new_selection: false,
            texture_outline: false,
            new_body_part_selection: false,
            selected_body_part_index: 0,
            new_model_selection: false,
            selected_model_index: 0,
            new_mesh_selection: false,
            selected_mesh_index: 0,
        }
    }

    fn copy_state(&mut self, other: &MdlViewerState) {
        self.selected_file_index = other.selected_file_index;
        self.scale = other.scale;
        self.new_selection = other.new_selection;
        self.texture_outline = other.texture_outline;
        self.new_body_part_selection = other.new_body_part_selection;
        self.selected_body_part_index = other.selected_body_part_index;
        self.new_model_selection = other.new_model_selection;
        self.selected_model_index = other.selected_model_index;
        self.new_mesh_selection = other.new_mesh_selection;
        self.selected_mesh_index = other.selected_mesh_index;
    }
}

pub struct MdlViewer {
    state: MdlViewerState,
    texture_bundle: Option<TextureBundle<ExtraTextureData>>,
    last_file_path: String,
}

impl MdlViewer {
    pub fn new() -> MdlViewer {
        MdlViewer {
            state: MdlViewerState::new(),
            texture_bundle: None,
            last_file_path: String::new(),
        }
    }

    fn reset_listbox_index(&mut self) {
        self.state.selected_file_index = 0;
        self.state.selected_body_part_index = 0;
        self.state.selected_model_index = 0;
        self.state.selected_mesh_index = 0;
    }

    pub fn build_ui(
        &mut self,
        ui: &Ui,
        file_info: &MdlFile,
        device: &mut wgpu::Device,
        queue: &mut wgpu::Queue,
        renderer: &mut Renderer,
    ) {
        let texture_names = &file_info.texture_names.iter().collect::<Vec<_>>();
        let body_part_names = &file_info.body_part_names.iter().collect::<Vec<_>>();
        let mut force_new_selection = false;

        if self.last_file_path != file_info.path {
            self.last_file_path = file_info.path.clone();
            self.reset_listbox_index();
            force_new_selection = true;
        }

        Window::new("Texture list")
            .size([300.0, 400.0], Condition::FirstUseEver)
            .build(ui, || {
                ui.text(format!("Path: {}", &file_info.path));
                ui.text(format!("Name: {}", &file_info.file.name));
                self.state.new_selection = ui.list_box(
                    "Textures",
                    &mut self.state.selected_file_index,
                    &texture_names,
                    texture_names.len() as i32,
                );
            });

        Window::new("Body part list")
            .size([300.0, 400.0], Condition::FirstUseEver)
            .position([100.0, 500.0], Condition::FirstUseEver)
            .build(ui, || {
                ui.text(format!("Body parts: {}", &file_info.file.body_parts.len()));
                self.state.new_body_part_selection = ui.list_box(
                    "Body parts",
                    &mut self.state.selected_body_part_index,
                    &body_part_names,
                    body_part_names.len() as i32,
                );
            });

        if self.state.new_body_part_selection {
            self.state.selected_model_index = 0;
            self.state.selected_mesh_index = 0;
        }

        if file_info.file.body_parts.len() > 0 {
            let body_part =
                &file_info.file.body_parts[self.state.selected_body_part_index as usize];
            if body_part.models.len() > 0 {
                let model_names = {
                    let mut model_names = Vec::with_capacity(body_part.models.len());
                    for model in &body_part.models {
                        model_names.push(ImString::new(model.name.clone()));
                    }
                    model_names
                };
                let model_names = model_names.iter().collect::<Vec<_>>();
                Window::new("Model list")
                    .size([300.0, 400.0], Condition::FirstUseEver)
                    .position([400.0, 500.0], Condition::FirstUseEver)
                    .build(ui, || {
                        ui.text(format!("Models: {}", model_names.len()));
                        self.state.new_model_selection = ui.list_box(
                            "Models",
                            &mut self.state.selected_model_index,
                            &model_names,
                            model_names.len() as i32,
                        );
                    });

                if self.state.new_model_selection {
                    self.state.selected_mesh_index = 0;
                }

                let model = &body_part.models[self.state.selected_model_index as usize];

                if model.vertices.len() > 0 {
                    Window::new("Model Vertex Data")
                        .size([300.0, 300.0], Condition::FirstUseEver)
                        .position([400.0, 900.0], Condition::FirstUseEver)
                        .build(ui, || {
                            ui.text(format!("Number of vertices: {}", model.vertices.len()));
                            ui.text("x, y, z");
                            for vertex in &model.vertices {
                                ui.text(format!("{}, {}, {}", vertex[0], vertex[1], vertex[2]));
                            }
                        });
                }

                if model.normals.len() > 0 {
                    Window::new("Model Normal Data")
                        .size([300.0, 300.0], Condition::FirstUseEver)
                        .position([700.0, 900.0], Condition::FirstUseEver)
                        .build(ui, || {
                            ui.text(format!("Number of normals: {}", model.normals.len()));
                            ui.text("x, y, z");
                            for normal in &model.normals {
                                ui.text(format!("{}, {}, {}", normal[0], normal[1], normal[2]));
                            }
                        });
                }

                if model.meshes.len() > 0 {
                    let mesh_names = {
                        let mut mesh_names = Vec::with_capacity(body_part.models.len());
                        for (i, _) in model.meshes.iter().enumerate() {
                            mesh_names.push(ImString::new(format!("Mesh {}", i + 1)));
                        }
                        mesh_names
                    };
                    let mesh_names = mesh_names.iter().collect::<Vec<_>>();
                    Window::new("Mesh list")
                        .size([300.0, 400.0], Condition::FirstUseEver)
                        .position([700.0, 500.0], Condition::FirstUseEver)
                        .build(ui, || {
                            ui.text(format!("Meshes: {}", mesh_names.len()));
                            self.state.new_mesh_selection = ui.list_box(
                                "Models",
                                &mut self.state.selected_mesh_index,
                                &mesh_names,
                                mesh_names.len() as i32,
                            );
                        });

                    let mesh = &model.meshes[self.state.selected_mesh_index as usize];
                    Window::new("Mesh info")
                        .size([300.0, 400.0], Condition::FirstUseEver)
                        .position([1000.0, 500.0], Condition::FirstUseEver)
                        .build(ui, || {
                            ui.text(format!("Vertices: {}", mesh.triverts_count));
                            ui.text(format!("Skin Reference: {}", mesh.skin_ref));
                            ui.text(format!("Normals: {}", mesh.normal_count));
                            ui.text("v, n, s, t");
                            for sequence in &mesh.sequences {
                                ui.text(format!("{:?}", sequence.ty));
                                for vertex in &sequence.triverts {
                                    ui.text(format!(
                                        "  {}, {}, {}, {}",
                                        vertex.vertex_index, vertex.normal_index, vertex.s, vertex.t
                                    ));
                                }
                            }
                        });
                }
            }
        }

        if self.state.new_selection || force_new_selection {
            // unbind our previous textures
            if let Some(texture_bundle) = self.texture_bundle.as_mut() {
                texture_bundle.clear(renderer);
            }

            let info = &file_info.file.textures[self.state.selected_file_index as usize];
            self.texture_bundle = Some(get_texture_bundle(&info, device, queue, renderer));
        }

        let mut temp_state = self.state.clone();
        if let Some(texture_bundle) = self.texture_bundle.as_ref() {
            Window::new("Texture preview")
                .position([500.0, 150.0], Condition::FirstUseEver)
                .size([300.0, 300.0], Condition::FirstUseEver)
                .horizontal_scrollbar(true)
                .build(ui, || {
                    ui.text(&texture_names[temp_state.selected_file_index as usize]);
                    ui.text(format!(
                        "Size: {} x {}",
                        texture_bundle.mip_textures[0].width, texture_bundle.mip_textures[0].height
                    ));
                    Slider::new("Scale", 1.0, 10.0).build(ui, &mut temp_state.scale);
                    ui.checkbox("Texture outline", &mut temp_state.texture_outline);
                    for texture in &texture_bundle.mip_textures {
                        let [x, y] = ui.cursor_screen_pos();
                        Image::new(
                            texture.texture_id,
                            [
                                texture.width as f32 * temp_state.scale,
                                texture.height as f32 * temp_state.scale,
                            ],
                        )
                        .build(ui);
                        if temp_state.texture_outline {
                            ui.get_window_draw_list()
                                .add_rect(
                                    [x, y],
                                    [
                                        x + ((texture.width as f32) * temp_state.scale),
                                        y + ((texture.height as f32) * temp_state.scale),
                                    ],
                                    [0.0, 1.0, 0.0, 1.0],
                                )
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
