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
        Quat::from_euler(
            EulerRot::YXZ,
            self.rotation.y,
            self.rotation.x,
            self.rotation.z,
        )
        .normalize()
    }

    pub fn to_mat4(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.get_rotation_quat(), self.translation)
    }
}
