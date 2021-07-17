use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use ng_render::Renderer;

mod input;
mod player;

use input::InputState;
use player::Player;

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Hello, triangle!")
        .build(&event_loop)
        .unwrap();
    let window = Arc::new(window);

    let mut renderer = Renderer::new(window.clone()).unwrap();

    let mut input_state = InputState::new();
    let mut player = Player::new();
    player.position = [-2.0, -2.0, 2.0].into();
    player.yaw = 0.125;
    player.pitch = -0.125;

    let mut next_tick = Instant::now();
    let tick_duration = Duration::new(0, 1_000_000_000 / 60);

    event_loop.run(move |event, _event_loop_target, control_flow| {
        input_state.handle_event(&event);

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if window_id == window.id() => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                window_id,
            } if window_id == window.id() => {
                window.request_redraw();
            }
            Event::MainEventsCleared => {
                if Instant::now() > next_tick {
                    player.turn((0.001 * std::mem::take(&mut input_state.mouse)).cast());
                    player.go((0.02 * input_state.movement()).cast());
                    next_tick += tick_duration;
                    *control_flow = if input_state.escape {
                        ControlFlow::Exit
                    } else {
                        ControlFlow::Poll
                    };
                } else {
                    window.request_redraw();
                    *control_flow = ControlFlow::WaitUntil(next_tick);
                }
            }
            Event::RedrawRequested(window_id) if window_id == window.id() => {
                let player_matrix = player.isometry().to_homogeneous().into();
                renderer.draw(player_matrix).unwrap();
            }
            _ => (),
        }
    });
}
