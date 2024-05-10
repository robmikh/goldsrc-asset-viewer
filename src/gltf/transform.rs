use glam::{EulerRot, Mat4, Quat, Vec3};

pub struct ComponentTransform {
    pub translation: Vec3,
    pub rotation: Vec3,
}

impl ComponentTransform {
    pub fn new(translation: Vec3, rotation: Vec3) -> Self {
        Self {
            translation,
            rotation,
        }
    }

    pub fn get_rotation_quat(&self) -> Quat {
        quat_from_euler(self.rotation)
    }

    pub fn to_mat4(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.get_rotation_quat(), self.translation)
    }
}

pub fn quat_from_euler(euler: Vec3) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        euler.y,
        euler.x,
        euler.z,
    )
    .normalize()
}