use std::{
    collections::HashMap,
    fmt::Write,
    path::{Path, PathBuf},
};

use glam::{Vec2, Vec3};
use gltf::{
    animation::Animations,
    buffer::BufferWriter,
    export::write_gltf,
    material::{
        Image, MagFilter, Material, MaterialData, MinFilter, PbrMetallicRoughness, Texture, Wrap,
    },
    node::{MeshIndex, Node, Nodes},
    skin::Skins,
    vertex_def, Mesh, Model,
};
use gsparser::{
    bsp::{
        BspEdge, BspEntity, BspFace, BspLeaf, BspNode, BspReader, BspSurfaceEdge, BspTextureInfo,
        BspVertex,
    },
    wad3::{MipmapedTextureData, WadArchive, WadFileInfo},
};

use crate::export::coordinates::convert_coordinates;

const QUIVER_PREFIX: &'static str = "\\quiver\\";
const LIGHTMAP_SCALE: usize = 16;

vertex_def! {
    ModelVertex {
        ("POSITION") pos: [f32; 3],
        ("NORMAL") normal: [f32; 3],
        ("TEXCOORD_0") uv: [f32; 2],
        ("TEXCOORD_1") lightmap_uv: [f32; 2],
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
struct SharedVertex {
    vertex: usize,
    texture: usize,
}

pub struct TextureInfo {
    pub name: String,
    pub image_data: MipmapedTextureData,
}

impl TextureInfo {
    fn new(name: String, image_data: MipmapedTextureData) -> Self {
        Self { name, image_data }
    }
}

pub fn export<P: AsRef<Path>, T: AsRef<Path>>(
    game_root: T,
    reader: &BspReader,
    export_file_path: P,
    mut log: Option<&mut String>,
) -> std::io::Result<()> {
    let export_file_path = export_file_path.as_ref();
    if let Some(log) = &mut log {
        writeln!(log, "Export path: {}", export_file_path.display()).unwrap();
        log_bsp(reader, log).unwrap();
    }

    let game_root = game_root.as_ref();

    let mut wad_resources = WadCollection::new();
    read_wad_resources(reader, game_root, &mut wad_resources);

    let textures = read_textures(reader, &wad_resources);
    let lightmap_atlas = decode_atlas(reader);
    let model = convert(reader, &textures, &lightmap_atlas);

    let mut buffer_writer = BufferWriter::new();

    let mut material_data = MaterialData::new();
    let sampler = material_data.add_sampler(gltf::material::Sampler {
        mag_filter: MagFilter::Linear,
        min_filter: MinFilter::LinearMipMapLinear,
        wrap_s: Wrap::Repeat,
        wrap_t: Wrap::Repeat,
    });
    for texture in &textures {
        let image = material_data.add_images(Image {
            uri: format!("{}.png", &texture.name),
        });
        let texture = material_data.add_texture(Texture {
            sampler,
            source: image,
        });
        material_data.add_material(Material {
            pbr_metallic_roughness: PbrMetallicRoughness {
                base_color_texture: Some(gltf::material::BaseColorTexture { index: texture }),
                metallic_factor: 0.0,
                roughness_factor: 1.0,
                ..Default::default()
            },
            ..Default::default()
        });
    }

    let skins = Skins::new();
    let animations = Animations::new(0);
    let mut nodes = Nodes::new(1);
    let scene_root = nodes.add_node(Node {
        mesh: Some(MeshIndex(0)),
        ..Default::default()
    });

    let buffer_name = "data.bin";
    let gltf_text = write_gltf(
        gltf::document::BufferSource::Uri(buffer_name),
        &mut buffer_writer,
        &model,
        &material_data,
        scene_root,
        &nodes,
        &skins,
        &animations,
    );

    let path = export_file_path;
    let data_path = if let Some(parent_path) = path.parent() {
        let mut data_path = parent_path.to_owned();
        data_path.push(buffer_name);
        data_path
    } else {
        PathBuf::from(buffer_name)
    };

    std::fs::write(path, gltf_text)?;
    std::fs::write(data_path, buffer_writer.to_inner())?;

    // Write textures
    let mut texture_path = if let Some(parent_path) = path.parent() {
        let mut data_path = parent_path.to_owned();
        data_path.push("something");
        data_path
    } else {
        PathBuf::from("something")
    };
    for texture in textures {
        texture_path.set_file_name(format!("{}.png", &texture.name));
        texture
            .image_data
            .image
            .save_with_format(&texture_path, image::ImageFormat::Png)
            .unwrap();
    }

    Ok(())
}

pub fn read_wad_resources<P: AsRef<Path>>(
    reader: &BspReader,
    game_root: P,
    wad_resources: &mut WadCollection,
) {
    let entities = BspEntity::parse_entities(reader.read_entities());
    let game_root = game_root.as_ref();
    for entity in &entities {
        if let Some(value) = entity.0.get("wad") {
            for wad_path in value.split(';') {
                assert!(wad_path.starts_with(QUIVER_PREFIX));
                let wad_path = &wad_path[QUIVER_PREFIX.len()..];
                let mut path = game_root.to_owned();
                path.push(wad_path);
                if !path.exists() {
                    println!("WARNING: Could not find \"{}\"", path.display());
                    continue;
                }
                let archive = WadArchive::open(path);
                wad_resources.add(archive);
            }
        }
    }
}

pub fn read_textures(reader: &BspReader, wad_resources: &WadCollection) -> Vec<TextureInfo> {
    let texture_reader = reader.read_textures();
    let mut textures = Vec::with_capacity(texture_reader.len());
    for i in 0..texture_reader.len() {
        let reader = texture_reader.get(i).unwrap();
        let name = reader.get_image_name();
        let texture_info = if reader.has_local_image_data() {
            let len = reader.raw_data().len();
            let mut data = vec![0u8; len];
            data.as_mut_slice().copy_from_slice(reader.raw_data());
            let mut reader = std::io::Cursor::new(&data);
            let texture_data = WadArchive::decode_mipmaped_image_from_reader(&mut reader);
            TextureInfo::new(name.to_owned(), texture_data)
        } else {
            let search_name = name.to_uppercase();
            let texture_data =
                if let Some((archive, file)) = wad_resources.find(search_name.as_str()) {
                    //println!("Found \"{}\"!", name);
                    let texture_data = archive.decode_mipmaped_image(file);
                    Some(texture_data)
                } else {
                    println!("Couldn't find \"{}\"", name);
                    None
                };
            TextureInfo::new(name.to_owned(), texture_data.unwrap())
        };
        textures.push(texture_info);
    }
    textures
}

fn convert(
    reader: &BspReader,
    textures: &[TextureInfo],
    lightmap_atlas: &LightmapAtlas,
) -> Model<ModelVertex> {
    let mut indices = Vec::new();
    let mut vertices = Vec::new();
    let mut meshes = Vec::new();
    let mut vertex_map = HashMap::<SharedVertex, usize>::new();
    convert_node(
        reader,
        reader.read_nodes(),
        0,
        true,
        &mut indices,
        &mut vertices,
        &mut vertex_map,
        &mut meshes,
        textures,
        lightmap_atlas,
    );

    Model {
        indices,
        vertices,
        meshes,
    }
}

pub fn convert_models(
    reader: &BspReader,
    textures: &[TextureInfo],
    lightmap_atlas: &LightmapAtlas,
) -> Vec<Model<ModelVertex>> {
    let bsp_models = reader.read_models();

    let mut models = Vec::with_capacity(bsp_models.len());
    for bsp_model in bsp_models {
        let node_index = bsp_model.head_nodes[0] as i16;
        let mut indices = Vec::new();
        let mut vertices = Vec::new();
        let mut meshes = Vec::new();
        let mut vertex_map = HashMap::<SharedVertex, usize>::new();
        convert_node(
            reader,
            reader.read_nodes(),
            node_index,
            node_index == 0,
            &mut indices,
            &mut vertices,
            &mut vertex_map,
            &mut meshes,
            textures,
            lightmap_atlas,
        );

        models.push(Model {
            indices,
            vertices,
            meshes,
        });
    }

    models
}

fn process_indexed_triangles(
    triangle_list: &[SharedVertex],
    face: &BspFace,
    normal: [f32; 3],
    bsp_vertices: &[BspVertex],
    textures: &[TextureInfo],
    texture_infos: &[BspTextureInfo],
    indices: &mut Vec<u32>,
    vertices: &mut Vec<ModelVertex>,
    vertex_map: &mut HashMap<SharedVertex, usize>,
    lightmap_atlas: &LightmapAtlas,
    lightmap_index: usize,
) {
    assert!(
        triangle_list.len() % 3 == 0,
        "Vertices are not a multiple of 3: {}",
        triangle_list.len()
    );
    let texture_info = &texture_infos[face.texture_info as usize];
    let texture = &textures[texture_info.texture_index as usize].image_data;
    let mut process_trivert = |trivert| {
        let index = if let Some(index) = vertex_map.get(trivert) {
            *index
        } else {
            let hl_pos = bsp_vertices[trivert.vertex as usize].to_array();
            let pos = convert_coordinates(hl_pos);
            let pos_vec = Vec3::from_array(pos);
            let s = Vec3::from_array(convert_coordinates(texture_info.s));
            let t = Vec3::from_array(convert_coordinates(texture_info.t));
            let uv = [
                pos_vec.dot(s) + texture_info.s_shift,
                pos_vec.dot(t) + texture_info.t_shift,
            ];
            assert_eq!(
                uv[0],
                Vec3::from_array(hl_pos).dot(Vec3::from_array(texture_info.s))
                    + texture_info.s_shift
            );
            assert_eq!(
                uv[1],
                Vec3::from_array(hl_pos).dot(Vec3::from_array(texture_info.t))
                    + texture_info.t_shift
            );

            let normal = convert_coordinates(normal);

            let lightmap_atlas_size =
                Vec2::new(lightmap_atlas.width as f32, lightmap_atlas.height as f32)
                    * Vec2::new(16.0, 16.0);
            let lightmap_image = &lightmap_atlas.images[lightmap_index];
            let lightmap_offset = Vec2::new(lightmap_image.x as f32, lightmap_image.y as f32);
            let lightmap_uv = (Vec2::from_array(uv) / LIGHTMAP_SCALE as f32) + lightmap_offset;
            let lightmap_uv = (lightmap_uv / lightmap_atlas_size).to_array();
            //let lightmap_uv = [lightmap_uv[1], lightmap_uv[0]];
            let uv = [
                uv[0] / texture.image_width as f32,
                uv[1] / texture.image_height as f32,
            ];

            let index = vertices.len();
            vertices.push(ModelVertex {
                pos,
                normal,
                uv,
                lightmap_uv,
            });
            vertex_map.insert(*trivert, index);
            index
        };
        indices.push(index as u32);
    };

    for trivert in triangle_list {
        process_trivert(trivert);
    }
}

pub struct WadCollection {
    wads: Vec<WadArchive>,
}

impl WadCollection {
    pub fn new() -> Self {
        Self { wads: Vec::new() }
    }

    pub fn add(&mut self, archive: WadArchive) {
        self.wads.push(archive);
    }

    pub fn find(&self, key: &str) -> Option<(&WadArchive, &WadFileInfo)> {
        for wad in &self.wads {
            if let Some(file) = wad.files.iter().find(|x| x.name.as_str() == key) {
                return Some((wad, file));
            }
        }
        None
    }
}

fn read_vertex_index(edge_index: &BspSurfaceEdge, edges: &[BspEdge]) -> u32 {
    let edge_vertex_index: usize = if edge_index.0 > 0 { 0 } else { 1 };
    let edge_index = edge_index.0.abs() as usize;
    let edge = &edges[edge_index];
    edge.vertices[edge_vertex_index] as u32
}

fn convert_node(
    reader: &BspReader,
    nodes: &[BspNode],
    node_index: i16,
    allow_zero: bool,
    indices: &mut Vec<u32>,
    vertices: &mut Vec<ModelVertex>,
    vertex_map: &mut HashMap<SharedVertex, usize>,
    meshes: &mut Vec<Mesh>,
    textures: &[TextureInfo],
    lightmap_atlas: &LightmapAtlas,
) {
    let node_index = if node_index > 0 || (node_index == 0 && allow_zero) {
        node_index as usize
    } else {
        let leaf_index = !node_index;
        let leaf = &reader.read_leaves()[leaf_index as usize];
        convert_leaf(
            reader,
            leaf,
            indices,
            vertices,
            vertex_map,
            meshes,
            textures,
            lightmap_atlas,
        );
        return;
    };

    let current_node = &nodes[node_index];
    convert_node(
        reader,
        nodes,
        current_node.children[0],
        false,
        indices,
        vertices,
        vertex_map,
        meshes,
        textures,
        lightmap_atlas,
    );
    convert_node(
        reader,
        nodes,
        current_node.children[1],
        false,
        indices,
        vertices,
        vertex_map,
        meshes,
        textures,
        lightmap_atlas,
    );
}

fn convert_leaf(
    reader: &BspReader,
    leaf: &BspLeaf,
    indices: &mut Vec<u32>,
    vertices: &mut Vec<ModelVertex>,
    vertex_map: &mut HashMap<SharedVertex, usize>,
    meshes: &mut Vec<Mesh>,
    textures: &[TextureInfo],
    lightmap_atlas: &LightmapAtlas,
) {
    let bsp_vertices = reader.read_vertices();
    let texture_infos = reader.read_texture_infos();
    let mark_surfaces = reader.read_mark_surfaces();
    let faces = reader.read_faces();
    let edges = reader.read_edges();
    let surface_edges = reader.read_surface_edges();
    let planes = reader.read_planes();

    let mark_surfaces_range = leaf.first_mark_surface..leaf.first_mark_surface + leaf.mark_surfaces;
    for mark_surface_index in mark_surfaces_range {
        let mark_surface = &mark_surfaces[mark_surface_index as usize];
        let face_index = mark_surface.0 as usize;
        let face = &faces[face_index];

        if face.texture_info == 0 {
            continue;
        }

        let surface_edges_range =
            face.first_edge as usize..face.first_edge as usize + face.edges as usize;
        let surface_edges = &surface_edges[surface_edges_range];

        let plane = &planes[face.plane as usize];

        let first_vertex = read_vertex_index(&surface_edges[0], edges);

        let mut triangle_list = Vec::new();
        let to_shared_vertex = |index: u32, face: &BspFace| -> SharedVertex {
            SharedVertex {
                vertex: index as usize,
                texture: face.texture_info as usize,
            }
        };
        for i in 0..surface_edges.len() - 2 {
            triangle_list.push(to_shared_vertex(
                read_vertex_index(&surface_edges[i + 2], edges),
                face,
            ));
            triangle_list.push(to_shared_vertex(
                read_vertex_index(&surface_edges[i + 1], edges),
                face,
            ));
            triangle_list.push(to_shared_vertex(first_vertex, face));
        }
        let start = indices.len();
        process_indexed_triangles(
            &triangle_list,
            face,
            plane.normal,
            bsp_vertices,
            textures,
            texture_infos,
            indices,
            vertices,
            vertex_map,
            lightmap_atlas,
            face_index,
        );
        let end = indices.len();

        meshes.push(Mesh {
            indices_range: start..end,
            texture_index: texture_infos[face.texture_info as usize].texture_index as usize,
        });
    }
}

fn log_bsp(reader: &BspReader, log: &mut String) -> std::fmt::Result {
    writeln!(log, "Nodes:")?;
    for (i, node) in reader.read_nodes().iter().enumerate() {
        writeln!(log, "  Node {}", i)?;
        writeln!(log, "    plane: {}", node.plane)?;
        writeln!(
            log,
            "    children: [ {}, {} ]",
            node.children[0], node.children[1]
        )?;
        writeln!(
            log,
            "    mins: [ {}, {}, {} ]",
            node.mins[0], node.mins[1], node.mins[2]
        )?;
        writeln!(
            log,
            "    maxs: [ {}, {}, {} ]",
            node.maxs[0], node.maxs[1], node.maxs[2]
        )?;
        writeln!(log, "    first_face: {}", node.first_face)?;
        writeln!(log, "    faces: {}", node.faces)?;
    }

    writeln!(log, "Leaves:")?;
    for (i, leaf) in reader.read_leaves().iter().enumerate() {
        writeln!(log, "  Leaf {}", i)?;
        writeln!(log, "    contents: {:?}", leaf.contents)?;
        writeln!(log, "    vis_offset: {}", leaf.vis_offset)?;
        writeln!(
            log,
            "    mins: [ {}, {}, {} ]",
            leaf.mins[0], leaf.mins[1], leaf.mins[2]
        )?;
        writeln!(
            log,
            "    maxs: [ {}, {}, {} ]",
            leaf.maxs[0], leaf.maxs[1], leaf.maxs[2]
        )?;
        writeln!(log, "    first_mark_surface: {}", leaf.first_mark_surface)?;
        writeln!(log, "    mark_surfaces: {}", leaf.mark_surfaces)?;
        writeln!(
            log,
            "    ambient_levels: [ {}, {}, {}, {} ]",
            leaf.ambient_levels[0],
            leaf.ambient_levels[1],
            leaf.ambient_levels[2],
            leaf.ambient_levels[3]
        )?;
    }

    writeln!(log, "Mark Surfaces:")?;
    for (i, surface) in reader.read_mark_surfaces().iter().enumerate() {
        writeln!(log, "  Mark Surface {}", i)?;
        writeln!(log, "    index: {}", surface.0)?;
    }

    writeln!(log, "Surface Edges:")?;
    for (i, edge) in reader.read_surface_edges().iter().enumerate() {
        writeln!(log, "  Surface Edge {}", i)?;
        writeln!(log, "    index: {}", edge.0)?;
    }

    writeln!(log, "Faces:")?;
    for (i, face) in reader.read_faces().iter().enumerate() {
        writeln!(log, "  Face {}", i)?;
        writeln!(log, "    plane: {}", face.plane)?;
        writeln!(log, "    plane_side: {}", face.plane_side)?;
        writeln!(log, "    first_edge: {}", face.first_edge)?;
        writeln!(log, "    edges: {}", face.edges)?;
        writeln!(log, "    texture_info: {}", face.texture_info)?;
        writeln!(
            log,
            "    styles: [ {}, {}, {}, {} ]",
            face.styles[0], face.styles[1], face.styles[2], face.styles[3]
        )?;
        writeln!(log, "    lightmap_offset: {}", face.lightmap_offset)?;
    }

    writeln!(log, "Edges:")?;
    for (i, edge) in reader.read_edges().iter().enumerate() {
        writeln!(log, "  Edge {}", i)?;
        writeln!(
            log,
            "    vertices: [ {}, {} ]",
            edge.vertices[0], edge.vertices[1]
        )?;
    }

    writeln!(log, "Vertices:")?;
    for (i, vertex) in reader.read_vertices().iter().enumerate() {
        writeln!(log, "  Vertex {}", i)?;
        writeln!(
            log,
            "    vertices: [ {}, {}, {} ]",
            vertex.x, vertex.y, vertex.z
        )?;
    }

    writeln!(log, "Textures:")?;
    let texture_reader = reader.read_textures();
    for i in 0..texture_reader.len() {
        let reader = texture_reader.get(i).unwrap();
        let name = reader.get_image_name();
        writeln!(
            log,
            "  {} - {} - {}",
            i,
            name,
            reader.has_local_image_data()
        )?;
    }

    let entities = reader.read_entities();
    let entities = BspEntity::parse_entities(entities);
    writeln!(log, "Entities:")?;
    for (i, entity) in entities.iter().enumerate() {
        writeln!(log, "  Entity {}", i)?;
        for (key, value) in &entity.0 {
            writeln!(log, "    {}: {}", key, value)?;
        }
    }

    writeln!(log, "Models:")?;
    let models = reader.read_models();
    for (i, model) in models.iter().enumerate() {
        writeln!(log, "  Model {}", i)?;
        writeln!(
            log,
            "    mins: [ {}, {}, {} ]",
            model.mins[0], model.mins[1], model.mins[2]
        )?;
        writeln!(
            log,
            "    maxs: [ {}, {}, {} ]",
            model.maxs[0], model.maxs[1], model.maxs[2]
        )?;
        writeln!(
            log,
            "    origin: [ {}, {}, {} ]",
            model.origin[0], model.origin[1], model.origin[2]
        )?;
        writeln!(
            log,
            "    head_nodes: [ {}, {}, {}, {} ]",
            model.head_nodes[0], model.head_nodes[1], model.head_nodes[2], model.head_nodes[3]
        )?;
        writeln!(log, "    vis_leaves: {}", model.vis_leaves)?;
        writeln!(log, "    first_face: {}", model.first_face)?;
        writeln!(log, "    faces: {}", model.faces)?;
    }

    writeln!(log, "Texture Infos:")?;
    let texture_infos = reader.read_texture_infos();
    for (i, info) in texture_infos.iter().enumerate() {
        writeln!(log, "  Texture Info {}", i)?;
        writeln!(
            log,
            "    s: [ {}, {}, {} ]",
            info.s[0], info.s[1], info.s[2]
        )?;
        writeln!(log, "    s_shift: {}", info.s_shift)?;
        writeln!(
            log,
            "    t: [ {}, {}, {} ]",
            info.t[0], info.t[1], info.t[2]
        )?;
        writeln!(log, "    t_shift: {}", info.t_shift)?;
        writeln!(log, "    texture_index: {}", info.texture_index)?;
        writeln!(log, "    flags: {}", info.flags)?;
    }

    writeln!(log, "Planes:")?;
    let planes = reader.read_planes();
    for (i, plane) in planes.iter().enumerate() {
        writeln!(log, "  Plane {}", i)?;
        writeln!(
            log,
            "    normal: [ {}, {}, {} ]",
            plane.normal[0], plane.normal[1], plane.normal[2]
        )?;
        writeln!(log, "    dist: {}", plane.dist)?;
        writeln!(log, "    ty: {}", plane.ty)?;
    }

    Ok(())
}

pub fn export_light_data<P: AsRef<Path>>(
    reader: &BspReader,
    export_path: P,
) -> std::io::Result<()> {
    let export_path = export_path.as_ref();

    let data = reader.read_lighting_data();
    std::fs::write(export_path, data)?;

    let atlas = decode_atlas(reader);

    let mut export_path = export_path.to_owned();
    export_path.set_file_name("atlas.png");
    let atlas_pixel_width = atlas.width * 16;
    let atlas_pixel_height = atlas.height * 16;
    let pixel_data =
        image::RgbImage::from_vec(atlas_pixel_width, atlas_pixel_height, atlas.data).unwrap();
    pixel_data
        .save_with_format(export_path, image::ImageFormat::Png)
        .unwrap();

    Ok(())
}

pub fn decode_atlas(reader: &BspReader) -> LightmapAtlas {
    let face_datas = decode_face_lightmaps(reader);
    let atlas = construct_atlas(&face_datas);
    atlas
}

struct LightmapFaceData<'a> {
    width: u32,
    height: u32,
    data: &'a [u8],
}

fn decode_face_lightmaps<'a>(reader: &'a BspReader) -> Vec<LightmapFaceData> {
    let data = reader.read_lighting_data();
    let faces = reader.read_faces();
    let mut lightmap_face_data = Vec::with_capacity(faces.len());

    let texture_infos = reader.read_texture_infos();
    let surface_edges = reader.read_surface_edges();
    let edges = reader.read_edges();
    let vertices = reader.read_vertices();
    for face in faces {
        let texture_info = &texture_infos[face.texture_info as usize];

        // Collect the vertices
        let mut face_vertices = Vec::new();
        for i in 0..face.edges {
            let surface_edge = &surface_edges[face.first_edge as usize + i as usize];
            let edge_index = surface_edge.0.abs();
            let edge = &edges[edge_index as usize];

            let vertex_index = if surface_edge.0 >= 0 {
                edge.vertices[0]
            } else {
                edge.vertices[1]
            };
            let vertex = &vertices[vertex_index as usize];

            face_vertices.push(Vec3::new(vertex.x, vertex.y, vertex.z));
        }

        let mut uvs = Vec::new();
        for vertex in &face_vertices {
            let u = vertex.dot(Vec3::from_array(texture_info.s)) + texture_info.s_shift;
            let v = vertex.dot(Vec3::from_array(texture_info.t)) + texture_info.t_shift;
            uvs.push(Vec2::new(u, v));
        }

        // Find mins and maxs
        let mut maxs = [f32::MIN, f32::MIN];
        let mut mins = [f32::MAX, f32::MAX];
        for vertex in &uvs {
            let vertex_data = vertex.to_array();
            for k in 0..2 {
                if vertex_data[k] < mins[k] {
                    mins[k] = vertex_data[k];
                }
                if vertex_data[k] > maxs[k] {
                    maxs[k] = vertex_data[k];
                }
            }
        }

        let mut imaxs = [0i32, 0];
        let mut imins = [0i32, 0];
        for k in 0..2 {
            imins[k] = (mins[k] / LIGHTMAP_SCALE as f32).floor() as i32;
            imaxs[k] = (maxs[k] / LIGHTMAP_SCALE as f32).ceil() as i32;
        }

        let width = imaxs[0] - imins[0] + 1;
        let height = imaxs[1] - imins[1] + 1;

        let data_start = face.lightmap_offset as usize;
        let data_end = data_start + (width * height * 3) as usize;
        let face_lightmap = &data[data_start..data_end];

        lightmap_face_data.push(LightmapFaceData {
            width: width as u32,
            height: height as u32,
            data: face_lightmap,
        });
    }

    lightmap_face_data
}

struct LightmapAtlasImage {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

pub struct LightmapAtlas {
    width: u32,
    height: u32,
    data: Vec<u8>,
    images: Vec<LightmapAtlasImage>,
}

fn construct_atlas(face_datas: &[LightmapFaceData]) -> LightmapAtlas {
    // Allocate atlas
    let (atlas_width, atlas_height) = {
        let width = (face_datas.len() as f64).sqrt() as usize;
        let remaining = face_datas.len() - (width * width);
        let extra_whole_rows = remaining / width;
        let extras = remaining % width;
        let height = if remaining > 0 {
            let extra_row = if extras > 0 { 1 } else { 0 };
            width + extra_whole_rows + extra_row
        } else {
            width
        };
        (width, height)
    };
    assert!(
        (atlas_width * atlas_height) >= face_datas.len(),
        "Failed: ({} x {}) >= {}",
        atlas_width,
        atlas_height,
        face_datas.len()
    );
    let bytes_per_pixel = 3;
    let image_width_in_pixels = 16;
    let image_height_in_pixels = 16;
    let atlas_image_stride = image_width_in_pixels * bytes_per_pixel;
    let atlas_stride = atlas_width * atlas_image_stride;
    let atlas_len = atlas_stride * (atlas_height * image_height_in_pixels);
    let mut atlas_data = vec![0u8; atlas_len];

    let mut images = Vec::with_capacity(face_datas.len());

    // Build the atlas
    for (i, face_data) in face_datas.iter().enumerate() {
        let x = (i % atlas_width) * image_width_in_pixels;
        let y = (i / atlas_width) * image_height_in_pixels;
        let atlas_offset = (y * atlas_stride) + (x * bytes_per_pixel);
        //let atlas_offset = (((i / atlas_width) * atlas_stride) * 16) + ((i % atlas_width) * atlas_image_stride);

        let data_stride = face_data.width as usize * bytes_per_pixel;
        let rows = face_data.height as usize;
        for row in 0..rows {
            let atlas_row_start = atlas_offset + (row * atlas_stride);
            let atlas_row_end = atlas_row_start + data_stride;
            let dest = &mut atlas_data[atlas_row_start..atlas_row_end];
            let source_start = row * data_stride;
            let source_end = source_start + data_stride;
            let source = &face_data.data[source_start..source_end];
            dest.copy_from_slice(source);
        }

        images.push(LightmapAtlasImage {
            x: x as u32,
            y: y as u32,
            width: face_data.width,
            height: face_data.height,
        })
    }

    LightmapAtlas {
        width: atlas_width as u32,
        height: atlas_height as u32,
        data: atlas_data,
        images,
    }
}

impl LightmapAtlas {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
