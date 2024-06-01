use glam::Vec3;
use gsparser::bsp::{BspClipNode, BspContents, BspNode, BspReader, FromValue};

use crate::export::coordinates::{convert_coordinates, convert_vec3_to_gltf, convert_vec3_to_half_life};

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
        convert_vec3_to_half_life(p1),
        convert_vec3_to_half_life(p2),
        true,
        &node_resolver,
        Vec3::new(0.0, 1.0, 0.0),
    )
    .map(|(pos, index)| (convert_vec3_to_gltf(pos), index))
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
        convert_vec3_to_half_life(p1),
        convert_vec3_to_half_life(p2),
        true,
        &clip_node_resolver,
        Vec3::new(0.0, 0.0, 0.0),
    )
    .map(|x| IntersectionInfo {
        position: convert_vec3_to_gltf(x.position),
        normal: convert_vec3_to_gltf(x.normal),
    })
}

pub fn hittest_clip_node_2(
    reader: &BspReader,
    clip_node_index: usize,
    start: Vec3,
    end: Vec3,
) -> Option<IntersectionInfo> {
    let p1 = convert_vec3_to_half_life(start);
    let p2 = convert_vec3_to_half_life(end);
    let nodes = reader.read_clip_nodes();

    let mut trace = QuakeTrace::default();
    trace.all_solid = true;
    trace.intersection = p2;
    if !trace_hull(reader, nodes, clip_node_index as i16, p1, p2, &mut trace) {
        let intersection = if trace.all_solid || trace.start_solid {
            p1
        } else {
            trace.intersection
        };
        Some(IntersectionInfo {
            position: convert_vec3_to_gltf(intersection),
            normal: convert_vec3_to_gltf(trace.plane.normal),
        })
    } else {
        None
    }
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
    let plane_normal = Vec3::from_array(plane.normal);
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
        //let plane_normal = normal;
        //let normal = if side == 0 { plane_normal } else { -plane_normal };
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
    let normal = if side == 0 {
        plane_normal
    } else {
        -plane_normal
    };

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
    let normal = if side == 0 {
        plane_normal
    } else {
        -plane_normal
    };
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

const DIST_EPSILON: f32 = 0.03125;

#[derive(Default)]
struct QuakePlane {
    normal: Vec3,
    dist: f32,
}

#[derive(Default)]
struct QuakeTrace {
    plane: QuakePlane,
    intersection: Vec3,
    all_solid: bool,
    start_solid: bool,
    in_open: bool,
    in_water: bool,
}

fn trace_hull(
    reader: &BspReader,
    nodes: &[BspClipNode],
    node_index: i16,
    p1: Vec3,
    p2: Vec3,
    trace: &mut QuakeTrace,
) -> bool {
    if node_index < 0 {
        let contents = BspContents::from_value(node_index as i32).unwrap();
        match contents {
            BspContents::Empty => {
                trace.in_open = true;
                trace.all_solid = false;
            }
            BspContents::Solid => trace.start_solid = true,
            BspContents::Water => todo!(),
            BspContents::Slime => todo!(),
            BspContents::Lava => todo!(),
            BspContents::Sky => todo!(),
            BspContents::Origin => todo!(),
            BspContents::Clip => todo!(),
            BspContents::Current0 => todo!(),
            BspContents::Current90 => todo!(),
            BspContents::Current180 => todo!(),
            BspContents::Current270 => todo!(),
            BspContents::CurrentUp => todo!(),
            BspContents::CurrentDown => todo!(),
            BspContents::Translucent => todo!(),
        }
        return true;
    }

    let node = &nodes[node_index as usize];
    let plane = &reader.read_planes()[node.plane() as usize];
    let plane_normal = Vec3::from_array(plane.normal);

    // Distances
    let (t1, t2) = if plane.ty < 3 {
        let t1 = p1.to_array()[plane.ty as usize] - plane.dist;
        let t2 = p2.to_array()[plane.ty as usize] - plane.dist;
        (t1, t2)
    } else {
        let t1 = plane_normal.dot(p1) - plane.dist;
        let t2 = plane_normal.dot(p2) - plane.dist;
        (t1, t2)
    };

    if t1 >= 0.0 && t2 >= 0.0 {
        let child = node.children[0];
        return trace_hull(reader, nodes, child, p1, p2, trace);
    }
    if t1 < 0.0 && t2 < 0.0 {
        let child = node.children[1];
        return trace_hull(reader, nodes, child, p1, p2, trace);
    }

    let frac = if t1 < 0.0 {
        (t1 + DIST_EPSILON) / (t1 - t2)
    } else {
        (t1 - DIST_EPSILON) / (t1 - t2)
    }
    .clamp(0.0, 1.0);
    let mid = p1 + frac * (p2 - p1);
    let side = if t1 >= 0.0 { 0 } else { 1 };

    let child = node.children[side];
    if !trace_hull(reader, nodes, child, p1, mid, trace) {
        return false;
    }

    let child = node.children[1 - side];
    if hull_point_contents(reader, nodes, child, mid) != BspContents::Solid as i16 {
        return trace_hull(reader, nodes, child, mid, p2, trace);
    }

    if trace.all_solid {
        trace.plane.normal = Vec3::ZERO;
        trace.plane.dist = 0.0;
        return false;
    }

    if side == 0 {
        trace.plane.normal = plane_normal;
        trace.plane.dist = plane.dist;
    } else {
        trace.plane.normal = -plane_normal;
        trace.plane.dist = -plane.dist;
    }

    // TODO: Don't hard code
    let clip_node_root = 0;
    let contents =
        BspContents::from_value(hull_point_contents(reader, nodes, clip_node_root, mid) as i32)
            .unwrap();
    assert_ne!(contents, BspContents::Solid);

    trace.intersection = mid;

    return false;
}

fn hull_point_contents(
    reader: &BspReader,
    nodes: &[BspClipNode],
    mut node_index: i16,
    point: Vec3,
) -> i16 {
    while node_index >= 0 {
        let node = nodes[node_index as usize];
        let plane = &reader.read_planes()[node.plane() as usize];
        let plane_normal = Vec3::from_array(plane.normal);

        let dist = if plane.ty < 3 {
            point.to_array()[plane.ty as usize] - plane.dist
        } else {
            plane_normal.dot(point) - plane.dist
        };
        if dist < 0.0 {
            node_index = node.children[1];
        } else {
            node_index = node.children[0];
        }
    }
    node_index
}
