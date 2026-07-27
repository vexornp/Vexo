use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use winit::event::{DeviceEvent, DeviceId};
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

use winit::{application::ApplicationHandler, event_loop::ActiveEventLoop};

use crate::core::Size;
use crate::{Application, WindowState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBindingAction {
    CloseWindow,
    Message,
}

pub struct VexoApp<A: Application + 'static> {
    receiver: Receiver<KeyBindingAction>,
    #[allow(dead_code)]
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

        None
    }

    fn handle_action_from_proxy(
        &mut self,
        _event_loop: &dyn ActiveEventLoop,
        action: KeyBindingAction,
    ) {
        if action == KeyBindingAction::Message {
            println!("Use wake up")
        }
    }

    fn create_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
    ) -> Result<WindowId, Box<dyn Error>> {
        let window_attr = WindowAttributes::default();
        let window = event_loop.create_window(window_attr).unwrap();
        let wid = self.try_init_framework_state(window);
        Result::Ok(wid.unwrap())
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
        event: winit::event::WindowEvent,
    ) {
        let Some(window_state) = self.windows.get_mut(&window_id) else {
            return;
        };

        window_state.handle_window_event(event_loop, &event);
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

    fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
        for state in self.windows.values_mut() {
            if state.check_cursor_blink() {
                state.request_frame();
            }
            // Keep the event loop alive while animations are active. On iOS,
            // request_redraw() called from within RedrawRequested (i.e. from
            // inside the CADisplayLink callback) doesn't reliably re-arm the
            // display link for the next vsync, so navigation push/pop
            // animations stall after the first frame. Re-requesting here from
            // about_to_wait — the standard winit hook for continuous
            // animation — keeps the display link firing until the animation
            // completes.
            if state.animation_ticker().has_active() {
                state.request_frame();
            }
            // Break the keyboard-dismiss deadlock. When the user taps outside
            // a focused TextEdit, set_ime_allowed(false) fires during render
            // (inside RedrawRequested), but the keyboard-source poll already
            // ran. request_redraw from inside RedrawRequested doesn't re-arm
            // the display link on iOS, and both existing frame drivers are
            // dead (cursor blink off — unfocused; ticker inactive — tween not
            // started). Without this check the render loop stalls: the OS
            // keyboard slides down while our input view freezes, then the
            // tween starts late (after the keyboard is gone). See
            // WindowState::keyboard_inset_changed() for the full analysis.
            //
            // On iOS, the CADisplayLink (started by sync_display_link below)
            // also keeps frames flowing, but this request_frame() ensures the
            // first frame after the notification fires immediately rather than
            // waiting for the next vsync.
            if state.keyboard_inset_changed() {
                state.request_frame();
            }
        }
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        println!("Ready to create surfaces");
        self.create_window(event_loop)
            .expect("Failed to create initial window");
    }
}
