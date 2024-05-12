use super::{add_and_get_index, buffer::AccessorIndex, node::NodeIndex};

#[derive(Copy, Clone, Debug)]
pub struct SkinIndex(pub usize);

pub struct Skin {
    pub inverse_bind_matrices: AccessorIndex,
    pub joints: Vec<NodeIndex>,
}

pub struct Skins {
    skins: Vec<Skin>,
}

impl Skins {
    pub fn new() -> Self {
        Self { skins: Vec::new() }
    }

    pub fn add_skin(&mut self, skin: Skin) -> SkinIndex {
        let index = add_and_get_index(&mut self.skins, skin);
        SkinIndex(index)
    }

    pub fn is_empty(&self) -> bool {
        self.skins.is_empty()
    }

    pub fn write_skins(&self) -> Vec<String> {
        let mut skins = Vec::new();
        for skin in &self.skins {
            let mut joints = Vec::with_capacity(skin.joints.len());
            for joint in &skin.joints {
                joints.push(format!("                           {}", joint.0));
            }
            let joints = joints.join(",\n");

            skins.push(format!(
                r#"          {{
                        "inverseBindMatrices" : {},
                        "joints" : [
        {}
                        ]
                    }}"#,
                skin.inverse_bind_matrices.0, joints
            ));
        }
        skins
    }
}
