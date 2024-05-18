use glam::Vec3;
use gsparser::bsp::{BspContents, BspNode, BspReader};

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
    hittest_node_for_leaf_impl(reader, nodes, node_index as i16, p1, p2, true)
}

fn hittest_node_for_leaf_impl(
    reader: &BspReader,
    nodes: &[BspNode],
    node_index: i16,
    p1: Vec3,
    p2: Vec3,
    allow_zero: bool,
) -> Option<(Vec3, usize)> {
    let node_index = if node_index > 0 || (node_index == 0 && allow_zero) {
        node_index as usize
    } else {
        let leaf_index = !node_index;
        let leaf = &reader.read_leaves()[leaf_index as usize];
        //println!("returning {:016b} -> {:016b} ({} -> {})", node_index, leaf_index, node_index, leaf_index);
        //if leaf.mark_surfaces > 0 {
        //    return Some(leaf_index as usize);
        //} else {
        //    return None;
        //}
        if leaf.contents() == BspContents::Solid {
            return Some((p1, leaf_index as usize));
        } else {
            return None;
        }
    };

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
        return hittest_node_for_leaf_impl(reader, nodes, child, p1, p2, false);
    }

    let frac = t1 / (t1 - t2);
    let mid = Vec3::new(
        p1.x + frac * (p2.x - p1.x),
        p1.y + frac * (p2.y - p1.y),
        p1.z + frac * (p2.z - p1.z),
    );
    let side = if t1 >= 0.0 { 0 } else { 1 };

    if let Some(hit) = hittest_node_for_leaf_impl(reader, nodes, current_node.children[side], p1, mid, false) {
        return Some(hit);
    }

    let side = 1 - side;
    hittest_node_for_leaf_impl(reader, nodes, current_node.children[side], mid, p2, false)
}
