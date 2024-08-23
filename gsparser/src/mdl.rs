extern crate bincode;
extern crate byteorder;
extern crate image;
extern crate serde;

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::str;

use byteorder::{LittleEndian, ReadBytesExt};
use serde::Deserialize;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct AnimationValueOffsets {
    offsets: [u16; 6],
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub name: String,
    pub fps: f32,
    pub bone_animations: Vec<BoneAnimation>,
}

#[derive(Clone, Debug)]
pub struct BoneAnimation {
    pub target: usize,
    pub channels: Vec<BoneChannelAnimation>,
}

#[derive(Clone, Debug)]
pub struct BoneChannelAnimation {
    pub target: ComponentTransformTarget,
    pub keyframes: Vec<f32>,
}

#[derive(Copy, Clone, Debug)]
pub enum ComponentTransformTarget {
    Translation(VectorChannel),
    Rotation(VectorChannel),
}

#[derive(Copy, Clone, Debug)]
pub enum VectorChannel {
    X,
    Y,
    Z,
}

impl ComponentTransformTarget {
    fn from_index(index: usize) -> Self {
        if index < 3 {
            ComponentTransformTarget::Translation(VectorChannel::from_index(index))
        } else if index <= 5 {
            ComponentTransformTarget::Rotation(VectorChannel::from_index(index))
        } else {
            panic!()
        }
    }
}

impl VectorChannel {
    fn from_index(index: usize) -> Self {
        match index {
            0 | 3 => VectorChannel::X,
            1 | 4 => VectorChannel::Y,
            2 | 5 => VectorChannel::Z,
            _ => panic!(),
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct EncodedAnimationValue {
    pub valid: u8,
    pub total: u8,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union AnimationValue {
    pub encoded_value: EncodedAnimationValue,
    pub value: i16,
}

#[allow(dead_code)]
#[derive(Copy, Clone, Deserialize, Debug)]
pub struct AnimationSequenceGroup {
    pub label: [u8; 32],
    pub name: [[u8; 8]; 8],
    pub unused_1: i32,
    pub unused_2: i32,
}

impl AnimationSequenceGroup {
    pub fn name(&self) -> &[u8; 64] {
        unsafe { std::mem::transmute(&self.name) }
    }
}

#[allow(dead_code)]
#[derive(Copy, Clone, Deserialize, Debug)]
pub struct AnimationSequence {
    pub name: [u8; 32],
    pub fps: f32,
    pub flags: i32,
    pub activity: i32,
    pub activity_weight: i32,
    pub num_events: u32,
    pub event_offset: u32,
    pub num_frames: u32,
    pub num_pivots: u32,
    pub pivot_offset: u32,
    pub motion_type: i32,
    pub motion_bone: u32,
    pub linear_movement: [f32; 3],
    pub auto_move_pos_offset: u32,
    pub auto_move_angle_offset: u32,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub num_blends: u32,
    pub animation_offset: u32,
    pub blend_type: [i32; 2],
    pub blend_start: [f32; 2],
    pub blend_end: [f32; 2],
    pub blend_parent: i32,
    pub sequence_group: i32,
    pub entry_node: i32,
    pub exit_node: i32,
    pub node_flags: i32,
    pub next_sequence: i32,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct MdlMeshVertex {
    pub vertex_index: u32,
    pub normal_index: u32,
    pub s: u32,
    pub t: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MdlMeshSequenceType {
    TriangleStrip,
    TriangleFan,
}

#[derive(Clone, Debug)]
pub struct MdlMeshSequence {
    pub ty: MdlMeshSequenceType,
    pub triverts: Vec<MdlMeshVertex>,
}

#[derive(Clone, Debug)]
pub struct MdlMesh {
    pub sequences: Vec<MdlMeshSequence>,
    pub triverts_count: u32,
    pub skin_ref: u32,
    pub normal_count: u32,
}

#[derive(Clone, Debug)]
pub struct MdlModel {
    pub name: String,
    pub meshes: Vec<MdlMesh>,
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub vertex_bone_indices: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct MdlBodyPart {
    pub name: String,
    pub models: Vec<MdlModel>,
}

#[derive(Clone, Debug)]
pub struct MdlTexture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub image_data: image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct MdlFile {
    pub name: String,
    pub textures: Vec<MdlTexture>,
    pub body_parts: Vec<MdlBodyPart>,
    pub bones: Vec<BoneHeader>, // TODO: Change
    pub animation_sequences: Vec<AnimationSequence>,
    pub animation_sequence_groups: Vec<AnimationSequenceGroup>,
    pub animations: Vec<Animation>,
    header: MdlHeader,
    raw_data: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoneHeader {
    pub name: [u8; 32],
    pub parent: i32,
    pub flags: u32,
    pub bone_controller: [i32; 6],
    pub value: [f32; 6],
    pub scale: [f32; 6],
}

#[allow(dead_code)]
#[derive(Copy, Clone, Deserialize, Debug)]
struct TextureHeader {
    name: [[u8; 8]; 8],
    flags: u32,
    width: u32,
    height: u32,
    offset: u32,
}

impl TextureHeader {
    fn name(&self) -> &[u8; 64] {
        unsafe { std::mem::transmute(&self.name) }
    }

    fn name_string(&self) -> String {
        let name = self.name();
        let name_string = String::from_utf8_lossy(name);
        let name_string = name_string.trim_matches(char::from(0));
        name_string.to_string()
    }
}

#[allow(dead_code)]
#[derive(Copy, Clone, Deserialize, Debug)]
struct MeshHeader {
    trivert_count: u32,
    trivert_offset: u32,
    skin_ref: u32,
    normal_count: u32,
    normal_offset: u32,
}

#[derive(Copy, Clone, Deserialize, Debug)]
struct VertexHeader {
    vertex_index: u16,
    normal_index: u16,
    s: u16,
    t: u16,
}

#[allow(dead_code)]
#[derive(Copy, Clone, Deserialize, Debug)]
struct BodyPartHeader {
    name: [[u8; 8]; 8],
    model_count: u32,
    base: u32,
    model_offset: u32,
}

impl BodyPartHeader {
    fn name(&self) -> &[u8; 64] {
        unsafe { std::mem::transmute(&self.name) }
    }

    fn name_string(&self) -> String {
        let name = self.name();
        let name_string = String::from_utf8_lossy(name);
        let name_string = name_string.trim_matches(char::from(0));
        name_string.to_string()
    }
}

#[allow(dead_code)]
#[derive(Copy, Clone, Deserialize, Debug)]
struct ModelHeader {
    name: [[u8; 8]; 8],
    model_type: u32,
    bounding_radius: f32,
    mesh_count: u32,
    mesh_offset: u32,
    vertex_count: u32,
    vertex_info_offset: u32,
    vertex_offset: u32,
    normal_count: u32,
    normal_info_offset: u32,
    normal_offset: u32,
    groups_count: u32,
    groups_offset: u32,
}

impl ModelHeader {
    fn name(&self) -> &[u8; 64] {
        unsafe { std::mem::transmute(&self.name) }
    }

    fn name_string(&self) -> String {
        let name = self.name();
        let name_string = String::from_utf8_lossy(name);
        let name_string = name_string.trim_matches(char::from(0));
        name_string.to_string()
    }
}

#[allow(dead_code)]
#[derive(Copy, Clone, Deserialize, Debug)]
struct MdlHeader {
    id: u32,
    version: u32,

    name: [[u8; 8]; 8],
    data_length: u32,

    eye_position: [f32; 3],
    hull_min: [f32; 3],
    hull_max: [f32; 3],

    view_bbmin: [f32; 3],
    view_bbmax: [f32; 3],

    flags: u32,

    bone_count: u32,
    bone_offset: u32,

    bone_controller_count: u32,
    bone_controller_offset: u32,

    hit_box_count: u32,
    hit_box_offset: u32,

    anim_seq_count: u32,
    anim_seq_offset: u32,

    seq_group_count: u32,
    seq_group_offset: u32,

    texture_count: u32,
    texture_offset: u32,
    texture_data_index: u32,

    skin_ref_count: u32,
    skin_families_count: u32,
    skin_offset: u32,

    body_part_count: u32,
    body_part_offset: u32,

    attachment_count: u32,
    attachment_offset: u32,

    sound_table: u32,
    sound_offset: u32,
    sound_groups: u32,
    sound_group_offset: u32,

    transitions_count: u32,
    transition_offset: u32,
}

impl MdlHeader {
    fn name(&self) -> &[u8; 64] {
        unsafe { std::mem::transmute(&self.name) }
    }

    fn name_string(&self) -> String {
        let name = self.name();
        let name_string = str::from_utf8(name).unwrap();
        let name_string = name_string.trim_matches(char::from(0));
        name_string.to_string()
    }
}

impl MdlFile {
    pub fn open<P: AsRef<Path>>(mdl_path: P) -> MdlFile {
        let mdl_path = mdl_path.as_ref();
        let file = File::open(mdl_path).unwrap();
        let file_size = file.metadata().unwrap().len();
        let mut file = BufReader::new(file);

        let mut header: MdlHeader = bincode::deserialize_from(&mut file).unwrap();
        let file_name = header.name_string();

        let textures = if header.texture_count == 0 {
            let mut texture_mdl_path = mdl_path.to_owned();
            let file_stem = texture_mdl_path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            texture_mdl_path.set_file_name(format!("{}t.mdl", file_stem));
            let texture_mdl_path = texture_mdl_path.into_os_string();
            let texture_mdl_path = texture_mdl_path.into_string().unwrap();

            let file = File::open(texture_mdl_path).unwrap();
            //let file_size = file.metadata().unwrap().len();
            let mut file = BufReader::new(file);
            let texture_header: MdlHeader = bincode::deserialize_from(&mut file).unwrap();

            header.texture_count = texture_header.texture_count;
            header.texture_offset = texture_header.texture_offset;
            header.texture_data_index = texture_header.texture_data_index;
            read_textures(&mut file, &texture_header)
        } else {
            read_textures(&mut file, &header)
        };

        let body_parts = {
            let mut body_part_headers = Vec::new();

            file.seek(SeekFrom::Start(header.body_part_offset as u64))
                .unwrap();
            for _ in 0..header.body_part_count {
                let body_header: BodyPartHeader = bincode::deserialize_from(&mut file).unwrap();

                body_part_headers.push(body_header);
            }

            let mut body_parts = Vec::new();
            for body_header in body_part_headers {
                // Model
                file.seek(SeekFrom::Start(body_header.model_offset as u64))
                    .unwrap();
                let mut model_headers = Vec::new();
                for _ in 0..body_header.model_count {
                    let model_header: ModelHeader = bincode::deserialize_from(&mut file).unwrap();
                    model_headers.push(model_header);
                }

                let mut models = Vec::new();
                for model_header in model_headers {
                    // Model Vertex
                    let mut vertices = Vec::new();
                    file.seek(SeekFrom::Start(model_header.vertex_offset as u64))
                        .unwrap();
                    for _ in 0..model_header.vertex_count {
                        let mut vertex = [0f32; 3];
                        vertex[0] = file.read_f32::<LittleEndian>().unwrap();
                        vertex[1] = file.read_f32::<LittleEndian>().unwrap();
                        vertex[2] = file.read_f32::<LittleEndian>().unwrap();

                        vertices.push(vertex);
                    }

                    // Model Normal
                    let mut normals = Vec::new();
                    file.seek(SeekFrom::Start(model_header.normal_offset as u64))
                        .unwrap();
                    for _ in 0..model_header.normal_count {
                        let mut normal = [0f32; 3];
                        normal[0] = file.read_f32::<LittleEndian>().unwrap();
                        normal[1] = file.read_f32::<LittleEndian>().unwrap();
                        normal[2] = file.read_f32::<LittleEndian>().unwrap();

                        normals.push(normal);
                    }

                    // Model Vertex bone indices
                    let mut vertex_bone_indices = Vec::new();
                    file.seek(SeekFrom::Start(model_header.vertex_info_offset as u64))
                        .unwrap();
                    for _ in 0..model_header.vertex_count {
                        let index = file.read_u8().unwrap();
                        vertex_bone_indices.push(index);
                    }

                    // Mesh
                    let mut mesh_headers = Vec::new();
                    file.seek(SeekFrom::Start(model_header.mesh_offset as u64))
                        .unwrap();
                    for _ in 0..model_header.mesh_count {
                        let mesh_header: MeshHeader = bincode::deserialize_from(&mut file).unwrap();
                        mesh_headers.push(mesh_header);
                    }

                    let mut meshes = Vec::new();
                    for mesh_header in mesh_headers {
                        // Mesh Vertex
                        file.seek(SeekFrom::Start(mesh_header.trivert_offset as u64))
                            .unwrap();
                        let mut sequences = Vec::new();
                        let mut total_triverts = 0;
                        let mut num_triverts: i16 = bincode::deserialize_from(&mut file).unwrap();
                        while num_triverts != 0 {
                            {
                                // Positive means triangle strip, negative means triangle fan
                                let (sequence_ty, num_triverts) = if num_triverts > 0 {
                                    (MdlMeshSequenceType::TriangleStrip, num_triverts as usize)
                                } else {
                                    (MdlMeshSequenceType::TriangleFan, -num_triverts as usize)
                                };
                                let mut triverts = Vec::with_capacity(num_triverts);
                                for _ in 0..num_triverts {
                                    let vertex_header: VertexHeader =
                                        bincode::deserialize_from(&mut file).unwrap();
                                    let vertex = MdlMeshVertex {
                                        vertex_index: vertex_header.vertex_index as u32,
                                        normal_index: vertex_header.normal_index as u32,
                                        s: vertex_header.s as u32,
                                        t: vertex_header.t as u32,
                                    };
                                    triverts.push(vertex);
                                }
                                total_triverts += triverts.len();
                                sequences.push(MdlMeshSequence {
                                    ty: sequence_ty,
                                    triverts,
                                });
                            }
                            num_triverts = bincode::deserialize_from(&mut file).unwrap();
                        }
                        // Why don't these match?
                        //assert_eq!(total_triverts, mesh_header.trivert_count as usize);

                        meshes.push(MdlMesh {
                            sequences,
                            triverts_count: total_triverts as u32,
                            skin_ref: mesh_header.skin_ref,
                            normal_count: mesh_header.normal_count,
                        });
                    }

                    models.push(MdlModel {
                        name: model_header.name_string(),
                        meshes: meshes,
                        vertices: vertices,
                        normals: normals,
                        vertex_bone_indices,
                    })
                }

                body_parts.push(MdlBodyPart {
                    name: body_header.name_string(),
                    models: models,
                });
            }

            body_parts
        };

        // Bones
        let bones = {
            let mut bones = Vec::new();

            file.seek(SeekFrom::Start(header.bone_offset as u64))
                .unwrap();
            for _ in 0..header.bone_count {
                let body_header: BoneHeader = bincode::deserialize_from(&mut file).unwrap();

                bones.push(body_header);
            }

            bones
        };

        // Animation sequences
        let sequences = {
            let mut sequences = Vec::new();

            file.seek(SeekFrom::Start(header.anim_seq_offset as u64))
                .unwrap();
            for _ in 0..header.anim_seq_count {
                let sequence: AnimationSequence = bincode::deserialize_from(&mut file).unwrap();
                sequences.push(sequence);
            }

            sequences
        };

        // Animation sequence groups
        let sequence_groups = {
            let mut sequence_groups = Vec::new();

            file.seek(SeekFrom::Start(header.seq_group_offset as u64))
                .unwrap();
            for _ in 0..header.seq_group_count {
                let group: AnimationSequenceGroup = bincode::deserialize_from(&mut file).unwrap();
                sequence_groups.push(group);
            }

            sequence_groups
        };

        // Copy file data
        let file_data = {
            file.seek(SeekFrom::Start(0)).unwrap();
            let mut file_data = vec![0u8; file_size as usize];
            file.read(&mut file_data).unwrap();
            file_data
        };

        // Animations
        let mut animations = Vec::new();
        for animated_sequence in &sequences {
            let name = null_terminated_bytes_to_str(&animated_sequence.name).unwrap();

            // TODO: Load other files
            if animated_sequence.sequence_group == 0 {
                //println!("  {}", name);

                let sequence_group = &sequence_groups[animated_sequence.sequence_group as usize];
                assert_eq!(sequence_group.unused_2, 0);
                let animation_offset = /*sequence_group.unused_2 as usize +*/ animated_sequence.animation_offset as usize;
                let animation_data = &file_data[animation_offset..];
                let animation_value_offsets_ptr =
                    animation_data.as_ptr() as *const AnimationValueOffsets;

                let mut bone_animations = Vec::new();
                for i in 0..bones.len() {
                    //println!("    Bone {}:", i);

                    let animation_value_offsets =
                        unsafe { animation_value_offsets_ptr.add(i).as_ref().unwrap() };
                    let animation_data =
                        { animation_value_offsets as *const AnimationValueOffsets as *const u8 };

                    let mut channels = Vec::new();
                    for (j, offset) in animation_value_offsets.offsets.iter().enumerate() {
                        if *offset != 0 {
                            let anim_value_ptr = unsafe {
                                animation_data.add(*offset as usize) as *const AnimationValue
                            };
                            let scale = bones[i].scale[j];

                            //println!("      ({})", scale);
                            //print!("      ");
                            let mut keyframes = Vec::new();
                            let target = ComponentTransformTarget::from_index(j);
                            for frame in 0..animated_sequence.num_frames as i32 {
                                let mut value =
                                    unsafe { decode_animation_frame(anim_value_ptr, frame, scale) };
                                value += bones[i].value[j];
                                //print!("{}:{}, ", frame, value);
                                keyframes.push(value);
                            }
                            //println!();

                            channels.push(BoneChannelAnimation { target, keyframes })
                        }
                    }

                    if !channels.is_empty() {
                        bone_animations.push(BoneAnimation {
                            target: i,
                            channels,
                        })
                    }
                }

                animations.push(Animation {
                    name: name.to_owned(),
                    fps: animated_sequence.fps,
                    bone_animations,
                })
            }
        }

        MdlFile {
            name: file_name,
            textures: textures,
            body_parts: body_parts,
            bones,
            animation_sequences: sequences,
            animation_sequence_groups: sequence_groups,
            animations,
            header: header,
            raw_data: file_data,
        }
    }

    // TODO: Remove
    pub fn raw_data(&self) -> &[u8] {
        &self.raw_data
    }
}

fn read_textures<T: Read + Seek>(mut reader: &mut T, header: &MdlHeader) -> Vec<MdlTexture> {
    let num_textures = header.texture_count as usize;
    let mut texture_headers = Vec::with_capacity(num_textures);
    reader
        .seek(SeekFrom::Start(header.texture_offset as u64))
        .unwrap();
    for _ in 0..num_textures {
        let texture_header: TextureHeader = bincode::deserialize_from(&mut reader).unwrap();
        texture_headers.push(texture_header);
    }

    let mut textures = Vec::with_capacity(num_textures);
    for texture_header in &texture_headers {
        let name_string = texture_header.name_string();

        let mut image_data = vec![0u8; (texture_header.width * texture_header.height) as usize];
        reader
            .seek(SeekFrom::Start(texture_header.offset as u64))
            .unwrap();
        reader.read_exact(image_data.as_mut_slice()).unwrap();

        let mut palette_data = [0u8; 256 * 3];
        reader.read_exact(&mut palette_data).unwrap();

        let converted_image = create_image(
            &image_data,
            &palette_data,
            texture_header.width,
            texture_header.height,
        );

        textures.push(MdlTexture {
            name: name_string.to_string(),
            width: texture_header.width,
            height: texture_header.height,
            image_data: converted_image,
        });
    }

    textures
}

// TODO: Consolodate these image decoders into one crate
fn create_image(
    image_data: &[u8],
    palette_data: &[u8],
    texture_width: u32,
    texture_height: u32,
) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    let mut image_rgba_data = Vec::<u8>::new();
    for palette_index in image_data {
        let index = (*palette_index as usize) * 3;
        let r_color = palette_data[index + 0];
        let g_color = palette_data[index + 1];
        let b_color = palette_data[index + 2];

        if r_color == 0 && g_color == 0 && b_color == 255 {
            image_rgba_data.push(0);
            image_rgba_data.push(0);
            image_rgba_data.push(0);
            image_rgba_data.push(0);
        } else {
            image_rgba_data.push(r_color);
            image_rgba_data.push(g_color);
            image_rgba_data.push(b_color);
            image_rgba_data.push(255);
        }
    }

    image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_vec(
        texture_width,
        texture_height,
        image_rgba_data,
    )
    .unwrap()
}

#[derive(Debug)]
pub struct NullTerminatedStrError {
    pub end: usize,
    pub str_error: std::str::Utf8Error,
}

impl std::fmt::Display for NullTerminatedStrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for NullTerminatedStrError {}

pub fn null_terminated_bytes_to_str<'a>(bytes: &'a [u8]) -> std::result::Result<&'a str, NullTerminatedStrError> {
    let end = bytes.iter().position(|x| *x == 0).unwrap_or(bytes.len());
    match std::str::from_utf8(&bytes[..end]) {
        Ok(string) => Ok(string),
        Err(err) => {
            Err(NullTerminatedStrError {
                end,
                str_error: err
            })
        }
    }
}

// TODO: This code is bananas, write a safer version
unsafe fn decode_animation_frame(
    mut anim_value_ptr: *const AnimationValue,
    frame: i32,
    scale: f32,
) -> f32 {
    let mut k = frame;

    while (*anim_value_ptr).encoded_value.total as i32 <= k {
        k -= (*anim_value_ptr).encoded_value.total as i32;
        anim_value_ptr = anim_value_ptr.add((*anim_value_ptr).encoded_value.valid as usize + 1);
    }

    let value = if (*anim_value_ptr).encoded_value.valid as i32 > k {
        (*anim_value_ptr.add(k as usize + 1)).value
    } else {
        (*anim_value_ptr.add((*anim_value_ptr).encoded_value.valid as usize)).value
    };
    //let value = u16::MAX - value;
    value as f32 * scale
}
