use glam::Vec3;
use gsparser::bsp::{BspNode, BspReader};

use crate::gltf::coordinates::convert_coordinates;

pub fn hittest_node_for_leaf(
    reader: &BspReader,
    node_index: usize,
    pos: Vec3,
    ray: Vec3,
) -> Option<usize> {
    let p1 = pos;
    let p2 = pos + (ray * 10000.0);
    let nodes = reader.read_nodes();
    hittest_node_for_leaf_impl(reader, nodes, node_index, p1, p2)
}

fn hittest_node_for_leaf_impl(
    reader: &BspReader,
    nodes: &[BspNode],
    node_index: usize,
    p1: Vec3,
    p2: Vec3,
) -> Option<usize> {
    let current_node = &nodes[node_index];
    let planes = reader.read_planes();
    let plane = &planes[current_node.plane as usize];
    let plane_normal = Vec3::from_array(convert_coordinates(plane.normal));
    let plane_dist = plane.dist;

    let t1 = plane_normal.dot(p1) - plane_dist;
    let t2 = plane_normal.dot(p2) - plane_dist;

    let child = if t1 >= 0.0 && t2 >= 0.0 {
        let child = current_node.children[0];
        Some(child)
    } else if t1 < 0.0 && t2 < 0.0 {
        let child = current_node.children[1];
        Some(child)
    } else {
        None
    };

    if let Some(child) = child {
        if child > 0 {
            return hittest_node_for_leaf_impl(reader, nodes, child as usize, p1, p2);
        } else {
            let leaf_index = !child;
            let leaf = &reader.read_leaves()[leaf_index as usize];
            if leaf.mark_surfaces > 0 {
                return Some(leaf_index as usize);
            } else {
                return None;
            }
        }
    }

    let frac = t1 / (t1 - t2);
    let mid = Vec3::new(
        p1.x + frac * (p2.x - p1.x),
        p1.y + frac * (p2.y - p1.y),
        p1.z + frac * (p2.z - p1.z),
    );
    let side = if t1 >= 0.0 { 0 } else { 1 };
    let node_index = {
        let child = current_node.children[side];
        if child > 0 {
            Some(child as usize)
        } else {
            // TODO: How to tell if empty? Everything reports CONTENTS_EMPTY...
            let leaf_index = !child;
            let leaf = &reader.read_leaves()[leaf_index as usize];
            if leaf.mark_surfaces > 0 {
                return Some(leaf_index as usize);
            } else {
                None
            }
        }
    };

    if let Some(node_index) = node_index {
        if let Some(hit) = hittest_node_for_leaf_impl(reader, nodes, node_index, p1, mid) {
            return Some(hit);
        }
    }

    let side = 1 - side;
    let node_index = {
        let child = current_node.children[side];
        if child > 0 {
            child as usize
        } else {
            // TODO: How to tell if empty? Everything reports CONTENTS_EMPTY...
            let leaf_index = !child;
            let leaf = &reader.read_leaves()[leaf_index as usize];
            if leaf.mark_surfaces > 0 {
                return Some(leaf_index as usize);
            } else {
                return None;
            }
        }
    };
    hittest_node_for_leaf_impl(reader, nodes, node_index, mid, p2)
}
