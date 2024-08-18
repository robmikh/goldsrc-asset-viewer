use std::{collections::HashSet, time::Duration};

use bsp::DrawMode;
use glam::{Vec2, Vec3};
use winit::keyboard::KeyCode;

use crate::FileInfo;

pub mod bsp;
mod camera;
mod debug;
mod movement;
mod renderer;

pub trait Renderer {
    fn render(
        &self,
        clear_color: wgpu::Color,
        view: &wgpu::TextureView,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    );

    fn resize(
        &mut self,
        config: &wgpu::SurfaceConfiguration,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    );

    fn build_ui_menu(&mut self, ui: &imgui::Ui);

    fn build_ui(&mut self, ui: &imgui::Ui, file_info: &FileInfo);

    fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        delta: Duration,
        down_keys: &HashSet<KeyCode>,
        mouse_delta: Option<Vec2>,
        // TODO: remove
        file_info: &Option<FileInfo>,
    );

    fn world_pos_and_ray_from_screen_pos(&self, pos: Vec2) -> (Vec3, Vec3);

    fn get_position_and_direction(&self) -> (Vec3, Vec3);

    fn set_debug_point(&mut self, point: Vec3);
    fn set_debug_pyramid(&mut self, point: Vec3, dir: Vec3);
    fn set_draw_mode(&mut self, draw_mode: DrawMode);
    fn get_draw_mode(&self) -> DrawMode;
}
