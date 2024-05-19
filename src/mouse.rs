use glam::Vec2;
use winit::{dpi::PhysicalPosition, window::Window};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MouseInputMode {
    Cursor,
    CameraLook,
}

pub struct MouseInputController {
    input_mode: MouseInputMode,
    manual_lock: Option<Vec2>,
    mouse_position: Vec2,
}

impl MouseInputController {
    pub fn new() -> Self {
        Self {
            input_mode: MouseInputMode::Cursor,
            manual_lock: None,
            mouse_position: Vec2::ZERO,
        }
    }

    pub fn on_resize(&mut self, new_size: Vec2) {
        if self.manual_lock.is_some() {
            self.manual_lock = Some(new_size / Vec2::new(2.0, 2.0));
        }
    }

    pub fn on_mouse_move(&mut self, window: &Window, new_position: Vec2) {
        self.mouse_position = new_position;
        if let Some(position) = self.manual_lock.as_ref() {
            let position = PhysicalPosition::new(position.x as u32, position.y as u32);
            window.set_cursor_position(position).unwrap();
        }
    }

    pub fn input_mode(&self) -> MouseInputMode {
        self.input_mode
    }

    pub fn mouse_position(&self) -> Vec2 {
        self.mouse_position
    }

    pub fn set_input_mode(&mut self, window: &Window, input_mode: MouseInputMode) {
        self.set_cursor_lock(window, input_mode == MouseInputMode::CameraLook);
        self.input_mode = input_mode;
    }

    fn set_cursor_lock(&mut self, window: &Window, lock: bool) {
        if lock {
            if window
                .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                .is_err()
            {
                window
                    .set_cursor_grab(winit::window::CursorGrabMode::Confined)
                    .unwrap();
                let size = window.inner_size();
                self.manual_lock =
                    Some(Vec2::new(size.width as f32 / 2.0, size.height as f32 / 2.0));
            } else {
                self.manual_lock = None;
            }
        } else {
            window
                .set_cursor_grab(winit::window::CursorGrabMode::None)
                .unwrap();
            self.manual_lock = None;
        }
        window.set_cursor_visible(!lock);
    }
}
