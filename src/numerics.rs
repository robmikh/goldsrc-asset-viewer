use glam::{Quat, Vec3, Vec4};

pub trait ToVec3 {
    fn to_vec3(&self) -> Vec3;
}

impl ToVec3 for Vec4 {
    fn to_vec3(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

pub trait ToVec4 {
    fn to_vec4(&self) -> Vec4;
}

impl ToVec4 for Quat {
    fn to_vec4(&self) -> Vec4 {
        Vec4::new(self.x, self.y, self.z, self.w)
    }
}

impl ToVec4 for Vec3 {
    fn to_vec4(&self) -> Vec4 {
        Vec4::new(self.x, self.y, self.z, 0.0)
    }
}
