use std::time::Duration;

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

    fn update(&mut self, delta: Duration);
}