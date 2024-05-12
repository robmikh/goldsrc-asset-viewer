use glam::Vec4;

use super::add_and_get_index;



#[derive(Copy, Clone, Debug)]
pub struct MaterialIndex(pub usize);
#[derive(Copy, Clone, Debug)]
pub struct TextureIndex(pub usize);
#[derive(Copy, Clone, Debug)]
pub struct ImageIndex(pub usize);
#[derive(Copy, Clone, Debug)]
pub struct SamplerIndex(pub usize);

#[derive(Clone, Debug, Default)]
pub struct Material {
    pub base_color_texture: Option<TextureIndex>,
    pub base_color_factor: Option<Vec4>,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
}

pub struct Texture {
    pub sampler: SamplerIndex,
    pub source: ImageIndex,
}

pub struct Image {
    pub uri: String,
}

#[derive(Copy, Clone, Debug)]
pub enum MagFilter {
    Nearest = 9728,
    Linear = 9729,
}

#[derive(Copy, Clone, Debug)]
pub enum MinFilter {
    Nearest = 9728,
    Linear = 9729,
    NearestMipMapNearest = 9984,
    LinearMipMapNearest = 9985,
    NearestMipMapLinear = 9986,
    LinearMipMapLinear = 9987,
}

#[derive(Copy, Clone, Debug)]
pub enum Wrap {
    ClampToEdge = 33701,
    MirroredRepeat = 33648,
    Repeat = 10497,
}

pub struct Sampler {
    pub mag_filter: MagFilter,
    pub min_filter: MinFilter,
    pub wrap_s: Wrap,
    pub wrap_t: Wrap,
}

pub struct MaterialData {
    materials: Vec<Material>,
    textures: Vec<Texture>,
    images: Vec<Image>,
    samplers: Vec<Sampler>,
}

impl MaterialData {
    pub fn new() -> Self {
        Self {
            materials: Vec::new(),
            textures: Vec::new(),
            images: Vec::new(),
            samplers: Vec::new(),
        }
    }

    pub fn add_material(&mut self, material: Material) -> MaterialIndex {
        MaterialIndex(add_and_get_index(&mut self.materials, material))
    }

    pub fn add_texture(&mut self, texture: Texture) -> TextureIndex {
        TextureIndex(add_and_get_index(&mut self.textures, texture))
    }

    pub fn add_images(&mut self, image: Image) -> ImageIndex {
        ImageIndex(add_and_get_index(&mut self.images, image))
    }

    pub fn add_sampler(&mut self, sampler: Sampler) -> SamplerIndex {
        SamplerIndex(add_and_get_index(&mut self.samplers, sampler))
    }

    pub fn write_materials(&self) -> Vec<String> {
        let mut materials = Vec::new();
        for material in &self.materials {
            let mut extras = Vec::with_capacity(2);
            if let Some(base_color_texture) = material.base_color_texture {
                extras.push(format!(r#""baseColorTexture" : {{
                    "index" : {}
                }}"#, base_color_texture.0));
            }
            if let Some(base_color_factor) = material.base_color_factor {
                extras.push(format!(r#""baseColorFactor" : [ {}, {}, {}, {} ]"#, base_color_factor.x, base_color_factor.y, base_color_factor.z, base_color_factor.w));
            }
            let extras = if extras.is_empty() {
                "".to_owned()
            } else {
                format!("{},\n                    ", extras.join(",\n"))
            };

            materials.push(format!(
                r#"          {{
                "pbrMetallicRoughness" : {{
                    {}"metallicFactor" : {},
                    "roughnessFactor" : {}
                }}
            }}"#,
                extras,
                material.metallic_factor,
                material.roughness_factor,
            ));
        }
        materials
    }

    pub fn write_textures(&self) -> Vec<String> {
        let mut textures = Vec::new();
        for texture in &self.textures {
            textures.push(format!(
                r#"           {{
                "sampler" : {},
                "source" : {}
            }}"#,
                texture.sampler.0,
                texture.source.0
            ));
        }
        textures
    }

    pub fn write_images(&self) -> Vec<String> {
        let mut images = Vec::new();
        for image in &self.images {
            images.push(format!(
                r#"         {{
                "uri" : "{}"
            }}"#,
                image.uri
            ));
        }
        images
    }

    pub fn write_samplers(&self) -> Vec<String> {
        let mut samplers = Vec::new();
        for sampler in &self.samplers {
            samplers.push(format!(r#"{{
                "magFilter" : {},
                "minFilter" : {},
                "wrapS" : {},
                "wrapT" : {}
            }}"#, sampler.mag_filter as usize, sampler.min_filter as usize, sampler.wrap_s as usize, sampler.wrap_t as usize));
        }
        samplers
    }
}