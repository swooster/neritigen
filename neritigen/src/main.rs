use std::sync::Arc;

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use ng_render::Renderer;

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Hello, blank window!")
        .build(&event_loop)
        .unwrap();
    let window = Arc::new(window);

    let mut renderer = Renderer::new(window.clone());

    event_loop.run(move |event, _event_loop_target, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if window_id == window.id() => {
                *control_flow = ControlFlow::Exit;
            }
            Event::RedrawRequested(window_id) if window_id == window.id() => {
                renderer.draw();
            }
            _ => (),
        }
    });
}
