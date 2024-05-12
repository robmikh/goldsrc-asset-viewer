use crate::enum_with_str;

use super::{add_and_get_index, buffer::AccessorIndex, node::NodeIndex, AsStr, GltfTargetPath};

#[derive(Copy, Clone, Debug)]
pub struct ChannelIndex(pub usize);
#[derive(Copy, Clone, Debug)]
pub struct SamplerIndex(pub usize);
#[derive(Copy, Clone, Debug)]
pub struct AnimationIndex(pub usize);

pub struct Animation {
    channels: Vec<Channel>,
    name: String,
    samplers: Vec<Sampler>,
}

pub struct Channel {
    pub sampler: SamplerIndex,
    pub target: ChannelTarget,
}

pub struct ChannelTarget {
    pub node: NodeIndex,
    pub path: GltfTargetPath,
}

enum_with_str!(AnimationInterpolation {
    Linear : "LINEAR",
});

pub struct Sampler {
    pub input: AccessorIndex,
    pub interpolation: AnimationInterpolation,
    pub output: AccessorIndex,
}

pub struct Animations {
    animations: Vec<Animation>,
}

impl Animation {
    pub fn new(name: String) -> Self {
        Self {
            channels: Vec::new(),
            name,
            samplers: Vec::new(),
        }
    }

    pub fn add_sampler(&mut self, sampler: Sampler) -> SamplerIndex {
        SamplerIndex(add_and_get_index(&mut self.samplers, sampler))
    }

    pub fn add_channel(&mut self, channel: Channel) -> ChannelIndex {
        ChannelIndex(add_and_get_index(&mut self.channels, channel))
    }

    pub fn write(&self) -> String {
        let mut channels = Vec::new();
        for channel in &self.channels {
            channels.push(format!(
                r#"           {{
                "sampler" : {},
                "target" : {{
                    "node" : {},
                    "path" : "{}"
                }}
            }}"#,
                channel.sampler.0,
                channel.target.node.0,
                channel.target.path.get_gltf_str()
            ));
        }
        let channels = channels.join(",\n");

        let mut samplers = Vec::new();
        for sampler in &self.samplers {
            samplers.push(format!(
                r#"           {{
                "input" : {},
                "interpolation" : "{}",
                "output" : {}
            }}"#,
                sampler.input.0, sampler.interpolation.as_str(), sampler.output.0,
            ));
        }
        let samplers = samplers.join(",\n");

        format!(
            r#"        {{
            "channels" : [
{}
            ],
            "name" : "{}",
            "samplers" : [
{}
            ]
        }}"#,
            channels, &self.name, samplers
        )
    }
}

impl Animations {
    pub fn new(capacity: usize) -> Self {
        Self { animations: Vec::with_capacity(capacity) }
    }

    pub fn add_animation(&mut self, animation: Animation) -> AnimationIndex {
        AnimationIndex(add_and_get_index(&mut self.animations, animation))
    }

    pub fn is_empty(&self) -> bool {
        self.animations.is_empty()
    }

    pub fn write_animations(&self) -> Vec<String> {
        let mut animations = Vec::new();
        for animation in &self.animations {
            animations.push(animation.write());
        }
        animations
    }
}