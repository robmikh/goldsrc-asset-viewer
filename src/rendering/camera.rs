use glam::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use wgpu::util::DeviceExt;
pub struct Camera {
    position: Vec3,
    viewport_size: Vec2,

    yaw: f32,
    pitch: f32,
    roll: f32,

    dirty: bool,

    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl Camera {
    pub fn new(
        position: Vec3,
        viewport_size: Vec2,
        bind_group_layout: &wgpu::BindGroupLayout,
        device: &wgpu::Device,
    ) -> Self {
        let mx_total = generate_matrix(
            viewport_size.x / viewport_size.y,
            position,
            Vec3::new(0.0, 0.0, 1.0),
        );
        let mx_ref: &[f32; 16] = mx_total.as_ref();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Globals Uniform Buffer"),
            contents: bytemuck::cast_slice(mx_ref),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: None,
        });

        Self {
            position,
            viewport_size,

            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,

            dirty: false,

            buffer: uniform_buffer,
            bind_group,
        }
    }

    pub fn on_resize(&mut self, viewport_size: Vec2) {
        self.viewport_size = viewport_size;
        self.dirty = true;
    }

    pub fn update(&mut self, queue: &wgpu::Queue) {
        if self.dirty {
            let mx_total = self.generate_matrix();
            let mx_ref: &[f32; 16] = mx_total.as_ref();
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(mx_ref));

            self.dirty = false;
        }
    }

    pub fn set_position(&mut self, position: Vec3) {
        self.position = position;
        self.dirty = true;
    }

    pub fn set_yaw_pitch_roll(&mut self, rotation_in_radians: Vec3) {
        self.yaw = rotation_in_radians.x;
        self.pitch = rotation_in_radians.y;
        self.roll = rotation_in_radians.z;

        if self.yaw > 2.0 * std::f32::consts::PI {
            self.yaw = self.yaw % (2.0 * std::f32::consts::PI);
        } else {
            while self.yaw < 0.0 {
                self.yaw = (2.0 * std::f32::consts::PI) + self.yaw;
            }
        }

        let min = 0.1;
        self.pitch = self.pitch.clamp(
            (std::f32::consts::PI / -2.0) + min,
            (std::f32::consts::PI / 2.0) - min,
        );

        // TODO: Roll validation

        self.dirty = true;
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    pub fn yaw_pitch_roll(&self) -> Vec3 {
        Vec3::new(self.yaw, self.pitch, self.roll)
    }

    pub fn viewport_size(&self) -> Vec2 {
        self.viewport_size
    }

    pub fn facing(&self) -> Vec3 {
        let transform = Mat4::from_euler(glam::EulerRot::YXZ, self.yaw, self.pitch, self.roll);
        let new_facing = transform * Vec4::new(0.0, 0.0, 1.0, 0.0);
        new_facing.xyz().normalize()
    }

    pub fn up(&self) -> Vec3 {
        let transform = Mat4::from_euler(glam::EulerRot::YXZ, self.yaw, self.pitch, self.roll);
        let new_up = transform * Vec4::new(0.0, 1.0, 0.0, 0.0);
        new_up.xyz().normalize()
    }

    pub fn world_pos_and_ray_from_screen_pos(&self, mut pos: Vec2) -> (Vec3, Vec3) {
        let (projection, view) = self.compute_projection_and_view_transforms();

        let target_size = self.viewport_size;
        pos.y = target_size.y - pos.y;
        let ndc = pos * 2.0 / target_size - Vec2::ONE;

        let ndc_to_world = (projection * view).inverse();
        let world_near_plane = ndc_to_world.project_point3(ndc.extend(-1.0));
        let world_far_plane = ndc_to_world.project_point3(ndc.extend(f32::EPSILON));

        let direction = world_far_plane - world_near_plane;
        let length = direction.length();
        let direction = (length.is_finite() && length > 0.0).then_some(direction / length);
        let direction = direction.unwrap();

        (world_near_plane, direction.normalize())
    }

    fn compute_projection_and_view_transforms(&self) -> (Mat4, Mat4) {
        compute_projection_and_view_transforms(
            self.viewport_size.x / self.viewport_size.y,
            self.position,
            self.facing(),
        )
    }

    fn generate_matrix(&self) -> Mat4 {
        let (projection, view) = self.compute_projection_and_view_transforms();
        projection * view
    }
}

fn compute_projection_and_view_transforms(
    aspect_ratio: f32,
    camera_start: Vec3,
    facing: Vec3,
) -> (Mat4, Mat4) {
    let mx_projection = Mat4::perspective_rh(45.0_f32.to_radians(), aspect_ratio, 1.0, 10000.0);
    let mx_view = Mat4::look_to_rh(camera_start, facing, Vec3::new(0.0, 1.0, 0.0));
    (mx_projection, mx_view)
}

fn generate_matrix(aspect_ratio: f32, camera_start: Vec3, facing: Vec3) -> Mat4 {
    let (projection, view) =
        compute_projection_and_view_transforms(aspect_ratio, camera_start, facing);
    projection * view
}
