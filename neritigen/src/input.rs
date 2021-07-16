use nalgebra as na;
use winit::event::{DeviceEvent, ElementState, Event, VirtualKeyCode, WindowEvent};

#[derive(Default)]
pub struct InputState {
    pub mouse: na::Vector2<f64>,
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub escape: bool,
}

impl InputState {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn handle_event<T>(&mut self, event: &Event<T>) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input, .. },
                ..
            }
            | Event::DeviceEvent {
                event: DeviceEvent::Key(input),
                ..
            } => {
                if let Some(var) = input.virtual_keycode.and_then(|k| self.keymap(k)) {
                    *var = input.state == ElementState::Pressed;
                }
            }

            Event::DeviceEvent {
                event: DeviceEvent::Motion { axis: 0, value },
                ..
            } => self.mouse.x += *value,

            Event::DeviceEvent {
                event: DeviceEvent::Motion { axis: 1, value },
                ..
            } => self.mouse.y += *value,

            _ => (),
        }
    }

    fn keymap(&mut self, key_code: VirtualKeyCode) -> Option<&mut bool> {
        Some(match key_code {
            VirtualKeyCode::W => &mut self.forward,
            VirtualKeyCode::S => &mut self.backward,
            VirtualKeyCode::A => &mut self.left,
            VirtualKeyCode::D => &mut self.right,
            VirtualKeyCode::Space => &mut self.up,
            VirtualKeyCode::LControl => &mut self.down,
            VirtualKeyCode::Escape => &mut self.escape,
            _ => None?,
        })
    }

    pub fn movement(&self) -> na::Vector3<f64> {
        na::Vector3::new(
            self.forward as i8 - self.backward as i8,
            self.left as i8 - self.right as i8,
            self.up as i8 - self.down as i8,
        )
        .cast()
    }
}
