pub mod render;
pub mod world;

use nalgebra::{vector, Vector};
use render::ray_trace_render::RayTraceRender;
use winit::{
    dpi::PhysicalSize,
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::world::{scene::Scene, sphere::Sphere};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub async fn run() {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init_with_level(log::Level::Warn).expect("Could't init logger.");
        } else {
            env_logger::init();
        }
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    // #[cfg(target_arch = "wasm32")]
    // {
    //     use winit::dpi::PhysicalSize;
    //     window.set_inner_size(PhysicalSize::new(450, 400));
    //     use winit::platform::web::WindowExtWebSys;
    //     web_sys::window()
    //         .and_then(|win| win.document())
    //         .and_then(|doc| {
    //             let dst = doc.get_element_by_id("crispy_window")?;
    //             let canvas = web_sys::Element::from(window.canvas());
    //             dst.append_child(&canvas).ok()?;
    //             Some(())
    //         })
    //         .expect("Couldn't append canvas to document body.");
    // }

    let mut renderer = RayTraceRender::new(window).await;

    let mut scene = Scene {
        spheres: vec![Sphere {
            pos: vector![0.0, 0.0, 0.0],
            radius: 5.0,
        }],
    };

    event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == renderer.window().id() => {
            if !renderer.input(event) {
                match event {
                    WindowEvent::Resized(physical_size) => {
                        renderer.resize(*physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        renderer.resize(**new_inner_size);
                    }
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                        ..
                    } => *control_flow = ControlFlow::Exit,
                    _ => {}
                }
            }
        }
        Event::RedrawRequested(window_id) if window_id == renderer.window().id() => {
            renderer.update();
            match renderer.render(&scene) {
                Ok(_) => {}
                Err(wgpu::SurfaceError::Lost) => renderer.resize(renderer.size),
                Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                Err(e) => eprintln!("{:?}", e),
            }
        }
        Event::MainEventsCleared => {
            renderer.window().request_redraw();
        }
        _ => {}
    });
}
