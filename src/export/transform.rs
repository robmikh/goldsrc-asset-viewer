use glam::{EulerRot, Quat, Vec3};

pub fn quat_from_euler(euler: Vec3) -> Quat {
    Quat::from_euler(EulerRot::YXZ, euler.y, euler.x, euler.z).normalize()
}
