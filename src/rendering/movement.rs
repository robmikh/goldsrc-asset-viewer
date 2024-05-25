use std::time::Duration;

use glam::Vec3;

// https://www.jwchong.com/hl/movement.html

const MAX_GROUND_SPEED: f32 = 320.0;
const MAX_GROUND_ACCELERATION: f32 = 10.0 * MAX_GROUND_SPEED;

const STOP_SPEED: f32 = 100.0;

pub struct MovingEntity {
    position: Vec3,
    velocity: Vec3,
    velocity_from_gravity: Vec3,
}

impl MovingEntity {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
            velocity_from_gravity: Vec3::ZERO,
        }
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }

    pub fn velocity_from_gravity(&self) -> Vec3 {
        self.velocity_from_gravity
    }

    pub fn update_velocity_ground(&mut self, wish_dir: Vec3, delta: Duration) {
        self.velocity = update_velocity_ground(wish_dir, self.velocity, delta);
    }

    pub fn set_position(&mut self, position: Vec3) {
        self.position = position;
    }

    pub fn set_velocity(&mut self, velocity: Vec3) {
        self.velocity = velocity;
    }

    pub fn set_velocity_from_gravity(&mut self, velocity_from_gravity: Vec3) {
        self.velocity_from_gravity = velocity_from_gravity;
    }
}

// https://www.youtube.com/watch?v=v3zT3Z5apaM
fn update_velocity_ground(wish_dir: Vec3, velocity: Vec3, delta: Duration) -> Vec3 {
    let velocity = friction(velocity, delta);

    let current_speed = velocity.dot(wish_dir);
    let add_speed = (MAX_GROUND_SPEED - current_speed)
        .clamp(0.0, MAX_GROUND_ACCELERATION * delta.as_secs_f32());

    let new_velocity = velocity + add_speed * wish_dir;

    if new_velocity.length() < 1.0 {
        Vec3::ZERO
    } else {
        new_velocity
    }
}

fn friction(velocity: Vec3, delta: Duration) -> Vec3 {
    let speed = velocity.length();

    if speed < 0.1 {
        return velocity;
    }

    let drop = {
        // Everything says friction is normally 1.0 ???
        // Am I using time in the wrong units or the wrong scale?
        let friction = 10.0; // TODO: edge friction
        let control = if speed < STOP_SPEED {
            STOP_SPEED
        } else {
            speed
        };
        control * friction * delta.as_secs_f32()
    };

    let new_speed = {
        let new_speed = speed - drop;
        if new_speed < 0.0 {
            0.0
        } else {
            new_speed
        }
    } / speed;

    velocity * new_speed
}
