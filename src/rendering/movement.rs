use std::time::Duration;

use glam::Vec3;

const MAX_GROUND_SPEED: f32 = 320.0;
const MAX_GROUND_ACCELERATION: f32 = 10.0 * MAX_GROUND_SPEED;

pub struct MovingEntity {
    position: Vec3,
    velocity: Vec3,
}

impl MovingEntity {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
        }
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }

    pub fn update_velocity_ground(&mut self, wish_dir: Vec3, delta: Duration) {
        self.velocity = update_velocity_ground(wish_dir, self.velocity, delta);
    }

    pub fn set_position(&mut self, position: Vec3) {
        self.position = position;
    }
}

// https://www.youtube.com/watch?v=v3zT3Z5apaM
fn update_velocity_ground(wish_dir: Vec3, velocity: Vec3, delta: Duration) -> Vec3 {
    let velocity = friction(velocity, delta);

    let current_speed = velocity.dot(wish_dir);
    let add_speed = (MAX_GROUND_SPEED - current_speed)
        .clamp(0.0, MAX_GROUND_ACCELERATION * delta.as_secs_f32());

    velocity + add_speed * wish_dir
}

fn friction(velocity: Vec3, delta: Duration) -> Vec3 {
    // TODO
    velocity - (velocity * (Vec3::new(10.0, 0.0, 10.0) * delta.as_secs_f32()))
}
