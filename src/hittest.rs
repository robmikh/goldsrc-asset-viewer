use glam::Vec3;
use gsparser::bsp::{BspClipNode, BspContents, BspNode, BspReader, FromValue};

use crate::gltf::coordinates::convert_coordinates;

pub fn hittest_node_for_leaf(
    reader: &BspReader,
    node_index: usize,
    pos: Vec3,
    ray: Vec3,
) -> Option<(Vec3, usize)> {
    let p1 = pos;
    let p2 = pos + (ray * 10000.0);
    let nodes = reader.read_nodes();
    hittest_node_for_leaf_impl(
        reader,
        nodes,
        node_index as i16,
        p1,
        p2,
        true,
        &node_resolver,
        Vec3::new(0.0, 1.0, 0.0),
    )
}

#[derive(Debug)]
pub struct IntersectionInfo {
    pub position: Vec3,
    pub normal: Vec3,
}

pub fn hittest_clip_node(
    reader: &BspReader,
    clip_node_index: usize,
    start: Vec3,
    end: Vec3,
) -> Option<IntersectionInfo> {
    let p1 = start;
    let p2 = end;
    let nodes = reader.read_clip_nodes();
    hittest_node_for_leaf_impl(
        reader,
        nodes,
        clip_node_index as i16,
        p1,
        p2,
        true,
        &clip_node_resolver,
        Vec3::new(0.0, 1.0, 0.0),
    )
}

trait RaycastNode {
    fn plane(&self) -> u32;
    fn children(&self) -> &[i16; 2];
}

impl RaycastNode for BspNode {
    fn plane(&self) -> u32 {
        self.plane
    }

    fn children(&self) -> &[i16; 2] {
        &self.children
    }
}

impl RaycastNode for BspClipNode {
    fn plane(&self) -> u32 {
        self.plane_index as u32
    }

    fn children(&self) -> &[i16; 2] {
        &self.children
    }
}

fn hittest_node_for_leaf_impl<
    T: RaycastNode,
    V,
    F: Fn(&BspReader, i16, bool, Vec3, Vec3) -> ResolvedNode<V>,
>(
    reader: &BspReader,
    nodes: &[T],
    node_index: i16,
    p1: Vec3,
    p2: Vec3,
    allow_zero: bool,
    node_resolver: &F,
    normal: Vec3,
) -> Option<V> {
    let node_index = match node_resolver(reader, node_index, allow_zero, p1, normal) {
        ResolvedNode::NodeIndex(node_index) => node_index,
        ResolvedNode::Leaf(result) => return result,
    };

    let current_node = &nodes[node_index];
    let planes = reader.read_planes();
    let plane = &planes[current_node.plane() as usize];
    let plane_normal = Vec3::from_array(convert_coordinates(plane.normal)).normalize();
    let plane_dist = plane.dist;

    let t1 = plane_normal.dot(p1) - plane_dist;
    let t2 = plane_normal.dot(p2) - plane_dist;

    let side = if t1 >= 0.0 && t2 >= 0.0 {
        Some(0)
    } else if t1 < 0.0 && t2 < 0.0 {
        Some(1)
    } else {
        None
    };

    if let Some(side) = side {
        let child = current_node.children()[side];
        let normal = if side == 0 { plane_normal } else { -plane_normal };
        return hittest_node_for_leaf_impl(
            reader,
            nodes,
            child,
            p1,
            p2,
            false,
            node_resolver,
            normal,
        );
    }

    let frac = t1 / (t1 - t2);
    let mid = Vec3::new(
        p1.x + frac * (p2.x - p1.x),
        p1.y + frac * (p2.y - p1.y),
        p1.z + frac * (p2.z - p1.z),
    );
    let side = if t1 >= 0.0 { 0 } else { 1 };
    let normal = if side == 0 { plane_normal } else { -plane_normal };

    if let Some(hit) = hittest_node_for_leaf_impl(
        reader,
        nodes,
        current_node.children()[side],
        p1,
        mid,
        false,
        node_resolver,
        normal,
    ) {
        return Some(hit);
    }

    let side = 1 - side;
    let normal = if side == 0 { plane_normal } else { -plane_normal };
    hittest_node_for_leaf_impl(
        reader,
        nodes,
        current_node.children()[side],
        mid,
        p2,
        false,
        node_resolver,
        normal,
    )
}

enum ResolvedNode<T> {
    NodeIndex(usize),
    Leaf(Option<T>),
}

fn node_resolver(
    reader: &BspReader,
    node_index: i16,
    allow_zero: bool,
    p1: Vec3,
    _normal: Vec3,
) -> ResolvedNode<(Vec3, usize)> {
    let node_index = if node_index > 0 || (node_index == 0 && allow_zero) {
        node_index as usize
    } else {
        let leaf_index = !node_index;
        let leaf = &reader.read_leaves()[leaf_index as usize];
        if leaf.contents() == BspContents::Solid {
            return ResolvedNode::Leaf(Some((p1, leaf_index as usize)));
        } else {
            return ResolvedNode::Leaf(None);
        }
    };
    ResolvedNode::NodeIndex(node_index)
}

fn clip_node_resolver(
    _reader: &BspReader,
    node_index: i16,
    allow_zero: bool,
    p1: Vec3,
    normal: Vec3,
) -> ResolvedNode<IntersectionInfo> {
    let node_index = if node_index > 0 || (node_index == 0 && allow_zero) {
        node_index as usize
    } else {
        let contents = BspContents::from_value(node_index as i32).unwrap();
        if contents == BspContents::Solid {
            return ResolvedNode::Leaf(Some(IntersectionInfo {
                position: p1,
                normal,
            }));
        } else {
            return ResolvedNode::Leaf(None);
        }
    };
    ResolvedNode::NodeIndex(node_index)
}
