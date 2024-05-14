use std::{collections::HashSet, time::Duration};

use winit::event::VirtualKeyCode;

pub mod bsp;

pub trait Renderer {
    fn render(&self,
        clear_color: wgpu::Color,
        view: &wgpu::TextureView,
        device: &wgpu::Device,
        queue: &wgpu::Queue,);

    fn resize(&mut self,
        config: &wgpu::SurfaceConfiguration,
        device: &wgpu::Device,
        queue: &wgpu::Queue,);

    fn update(&mut self, 
        device: &wgpu::Device,
        queue: &wgpu::Queue,delta: Duration, down_keys: &HashSet<VirtualKeyCode>);
}