use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use winit::event::*;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

use winit::{
    application::ApplicationHandler, event_loop::ActiveEventLoop, keyboard::KeyCode,
};

use crate::core::{Physical, Point, Scale, Size};
use crate::{Application, WindowState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBindingAction {
    CloseWindow,
    Message,
}

pub struct VexoApp<A: Application + 'static> {
    receiver: Receiver<KeyBindingAction>,
    sender: Sender<KeyBindingAction>,
    windows: HashMap<WindowId, WindowState<A>>,
}

impl<A: Application + 'static> VexoApp<A> {
    pub fn new(
        _event_loop: &EventLoop,
        receiver: Receiver<KeyBindingAction>,
        sender: Sender<KeyBindingAction>,
    ) -> Self {
        Self {
            receiver,
            sender,
            windows: Default::default(),
        }
    }

    pub fn try_init_framework_state(&mut self, window: Box<dyn Window>) -> Option<WindowId> {
        let window: Arc<dyn Window> = Arc::from(window);
        let window_id = window.id();
        let size = window.surface_size();
        let window_state = self.windows.get(&window_id);
        if size.width > 0 && size.height > 0 && window_state.is_none() {
            println!(
                "SUCCESS: Window ready at {}x{}, scale: {}",
                size.width,
                size.height,
                window.scale_factor()
            );
            let mut state = pollster::block_on(WindowState::new(window.clone())).unwrap();
            state.resize(Size::from_winit(size));
            self.windows.insert(window_id, state);
            return Some(window_id);
        }

        return None;
    }

    fn handle_action_from_proxy(
        &mut self,
        _event_loop: &dyn ActiveEventLoop,
        action: KeyBindingAction,
    ) {
        match action {
            KeyBindingAction::Message => {
                println!("Use wake up")
            }
            _ => {}
        }
    }

    fn create_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
    ) -> Result<WindowId, Box<dyn Error>> {
        let window_attr = WindowAttributes::default();
        let window = event_loop.create_window(window_attr).unwrap();
        let wid = self.try_init_framework_state(window);
        return Result::Ok(wid.unwrap());
    }
}

impl<A: Application + 'static> ApplicationHandler for VexoApp<A> {
    // fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
    //     if !self.windows.is_empty() {
    //         println!("app resumed, already have window");
    //         return;
    //     }

    //     println!("app resumed, create initial window");
    //     let window_attributes = WindowAttributes::default();
    //     let window = event_loop.create_window(window_attributes).unwrap();
    //     self.try_init_framework_state(window);
    // }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let window_state = match self.windows.get_mut(&window_id) {
            Some(ws) => ws,
            None => return,
        };

        match event {
            WindowEvent::SurfaceResized(size) => {
                window_state.resize(Size::from_winit(size));
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                surface_size_writer: _,
            } => {
                window_state.widget_context.scale = Scale::new(scale_factor);
                println!("Scale factor changed to {}", scale_factor);
            }
            WindowEvent::PointerMoved {
                device_id: _,
                position,
                primary: _,
                source: _,
            } => {
                window_state.widget_context.cursor_pos =
                    Point::<Physical>::new(position.x as f32, position.y as f32);
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = window_state.render() {
                    println!("Error drawing window: {err}")
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
                println!("Window closed by user");
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                event_loop.exit();
                println!("Escape pressed, exiting");
            }
            _ => (),
        }

        window_state.handle_window_event(event_loop, window_id, &event);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(action) = self.receiver.try_recv() {
            self.handle_action_from_proxy(event_loop, action);
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &dyn ActiveEventLoop,
        _device_id: Option<DeviceId>,
        _event: DeviceEvent,
    ) {
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        println!("Ready to create surfaces");
        self.create_window(event_loop)
            .expect("Failed to create initial window");
    }
}
