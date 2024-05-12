use glam::Vec3;
use gsparser::mdl::VectorChannel;

// Half-Life's coordinate system uses:
//    X is forward
//    Y is left
//    Z is up
//    (https://github.com/malortie/assimp/wiki/MDL:-Half-Life-1-file-format#notes)
// GLTF's coordinate system uses:
//    X is left (-X is right)
//    Y is up
//    Z is forward
//    (https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#coordinate-system-and-units)
pub fn convert_coordinates<T: Copy>(half_life_xyz: [T; 3]) -> [T; 3] {
    [half_life_xyz[1], half_life_xyz[2], half_life_xyz[0]]
}

pub fn write_and_convert_channel(base: &mut Vec3, channel: VectorChannel, value: f32) {
    match channel {
        // HL X => GLTF Z
        VectorChannel::X => base.z = value,
        // HL Y => GLTF X
        VectorChannel::Y => base.x = value,
        // HL Z => GLTF Y
        VectorChannel::Z => base.y = value,
    }
}
