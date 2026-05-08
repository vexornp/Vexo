use std::error::Error;
use std::sync::mpsc;

use winit::event_loop::EventLoop;

pub use core::Color;
pub use uniffi;

mod app;
pub use app::{KeyBindingAction, VexoApp};

pub mod core;
mod editor;
pub mod input;
pub mod layout;
mod macros;
mod quad_instance;
pub mod render;
mod renderer;
pub use renderer::UiBatcher;
pub mod component;
mod resource;
pub mod reactive;
pub mod retain;
pub mod state;
pub mod testable;
mod utils;
pub mod widgets;
mod window;
mod text_cache;
mod text_processor;
mod frame_context;
mod render_pipeline;
pub use window::WindowState;
pub use frame_context::FrameContext;

use widgets::Widget;
pub use widgets::WidgetExt;
pub use state::CursorBlinkState;
pub use winit::dpi::PhysicalPosition;

pub use layout::AlignItems;

extern crate alloc;

pub trait Application: Sized + 'static {
    type Message: Clone + std::fmt::Debug + Send;
    type State;

    fn new() -> Self::State;
    fn update(state: &mut Self::State, message: Self::Message);
    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>>;

    /// Retain-mode view (optional, for migration).
    ///
    /// Returns a retain-mode widget tree. Applications can implement this
    /// to migrate to the three-tree architecture incrementally.
    ///
    /// Default implementation returns `None`, allowing existing applications
    /// to continue using immediate-mode rendering without changes.
    fn retain_view(state: &Self::State) -> Option<Box<dyn retain::Widget<Self::Message>>> {
        let _ = state;
        None
    }
}

pub fn run_desktop_demo<A: Application + 'static>() -> Result<(), Box<dyn Error>> {
    // Initialize logger with debug level for retain mode by default
    // Override with RUST_LOG environment variable if needed
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
        .init();

    let event_loop = EventLoop::new()?;
    let (sender, receiver) = mpsc::channel();

    {
        // Wire the user event from another thread.
        let _event_loop_proxy = event_loop.create_proxy();
        let _sender = sender.clone();
        std::thread::spawn(move || {
            // Wake up the `event_loop` once every second and dispatch a custom event
            // from a different thread.
            println!("Starting to send user event every second");
            // loop {
            //     let _ = sender.send(KeyBindingAction::Message);
            //     event_loop_proxy.wake_up();
            //     std::thread::sleep(std::time::Duration::from_secs(1));
            // }
        });
    }

    let app = VexoApp::<A>::new(&event_loop, receiver, sender);

    // let event_loop = winit::event_loop::EventLoop::with_user_event().build()?;
    // let mut app = crate::VexoApp::<A>::new();
    // event_loop.run_app(&mut app)?;
    Result::Ok(event_loop.run_app(app)?)
}