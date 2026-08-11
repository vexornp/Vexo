use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use winit::event::{DeviceEvent, DeviceId};
use winit::window::{Window, WindowAttributes, WindowId};

use winit::{application::ApplicationHandler, event_loop::ActiveEventLoop};

use crate::core::Size;
use crate::image_cache::ImageCache;
use crate::VexoUserEvent;
use crate::{Application, WindowState};

/// The main application handler.
///
/// Holds the `ImageCache` (shared across all windows) and the per-window
/// `WindowState` map. The `proxy_wake_up` handler drains the
/// `VexoUserEvent` channel (woken by `EventLoopProxy::wake_up()` from
/// background threads) and requests a new frame on every window when an
/// `ImageLoaded` event arrives.
pub struct VexoApp<A: Application + 'static> {
    image_cache: Arc<ImageCache>,
    receiver: Receiver<VexoUserEvent>,
    windows: HashMap<WindowId, WindowState<A>>,
}

impl<A: Application + 'static> VexoApp<A> {
    pub fn new(image_cache: Arc<ImageCache>, receiver: Receiver<VexoUserEvent>) -> Self {
        Self {
            image_cache,
            receiver,
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
            let mut state =
                pollster::block_on(WindowState::new(window.clone(), self.image_cache.clone()))
                    .unwrap();
            state.resize(Size::from_winit(size));
            self.windows.insert(window_id, state);
            return Some(window_id);
        }

        None
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

    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                VexoUserEvent::ImageLoaded(url) => {
                    log::debug!("ImageLoaded event: {}", url);
                    for state in self.windows.values_mut() {
                        state.request_frame();
                    }
                }
            }
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
            state.poll_idle_frame_drivers();
        }
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        println!("Ready to create surfaces");
        self.create_window(event_loop)
            .expect("Failed to create initial window");
    }
}
