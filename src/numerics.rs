use glam::{Quat, Vec4};

pub trait ToVec4 {
    fn to_vec4(&self) -> Vec4;
}

impl ToVec4 for Quat {
    fn to_vec4(&self) -> Vec4 {
        Vec4::new(self.x, self.y, self.z, self.w)
    }
}
