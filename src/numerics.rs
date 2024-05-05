use glam::{Vec3, Vec4};

pub trait ToVec3 {
    fn to_vec3(&self) -> Vec3;
}

impl ToVec3 for Vec4 {
    fn to_vec3(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}
