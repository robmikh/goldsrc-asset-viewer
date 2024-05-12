use glam::{Vec3, Vec4};

use super::{add_and_get_index, skin::SkinIndex};

#[derive(Copy, Clone, Debug)]
pub struct MeshIndex(pub usize);
#[derive(Copy, Clone, Debug)]
pub struct NodeIndex(pub usize);

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub mesh: Option<MeshIndex>,
    pub skin: Option<SkinIndex>,
    pub name: Option<String>,
    pub translation: Option<Vec3>,
    pub rotation: Option<Vec4>,
    pub children: Vec<NodeIndex>,
}

pub struct Nodes {
    nodes: Vec<Node>,
}

impl Nodes {
    pub fn new(capacity: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(capacity),
        }
    }

    pub fn add_node(&mut self, node: Node) -> NodeIndex {
        let index = add_and_get_index(&mut self.nodes, node);
        NodeIndex(index)
    }

    pub fn write_nodes(&self) -> Vec<String> {
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let mut fields = Vec::new();
            if let Some(mesh) = node.mesh {
                fields.push(format!(r#"            "mesh" : {}"#, mesh.0));
            }
            if let Some(skin) = node.skin {
                fields.push(format!(r#"            "skin" : {}"#, skin.0));
            }
            if let Some(name) = node.name.as_ref() {
                fields.push(format!(r#"            "name" : "{}""#, name));
            }
            if let Some(translation) = node.translation {
                fields.push(format!(
                    r#"            "translation" : [ {}, {}, {} ]"#,
                    translation.x, translation.y, translation.z
                ));
            }
            if let Some(rotation) = node.rotation {
                fields.push(format!(
                    r#"            "rotation" : [ {}, {}, {}, {} ]"#,
                    rotation.x, rotation.y, rotation.z, rotation.w
                ));
            }
            if !node.children.is_empty() {
                let mut children = Vec::new();
                for child in &node.children {
                    children.push(child.0.to_string());
                }
                let children = children.join(", ");
                fields.push(format!(r#"            "children" : [ {} ]"#, children));
            }
            let fields = fields.join(",\n");
            nodes.push(format!(
                r#"        {{
{}
        }}"#,
                fields
            ));
        }
        nodes
    }
}
