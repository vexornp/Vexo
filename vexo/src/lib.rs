use std::error::Error;
use std::sync::mpsc;

use winit::event_loop::EventLoop;

pub use core::Color;
pub use core::AffineTransform;
pub use uniffi;

mod app;
pub use app::{KeyBindingAction, VexoApp};

pub mod core;
pub mod editor;
pub mod input;
pub mod layout;
mod mouse_tracker;
pub use mouse_tracker::MouseTracker;

mod quad_instance;
pub mod render;
mod frame_builder;
pub use frame_builder::FrameBuilder;
mod resource;
pub mod reactive;
pub mod state;
mod macros;
mod window;
mod text_cache;
mod text_processor;
mod text_pipeline;
pub use window::WindowState;

pub use state::CursorBlinkState;
pub use winit::dpi::PhysicalPosition;

pub use layout::{AlignItems, AlignSelf, Display, FlexDirection, GridAutoFlow, Overflow};

// --- Former retain/ modules (flattened) ---

mod key;
mod id;
mod element_state;
mod element;
mod element_context;
mod event_handler;
mod event_context;
mod render_object;
mod dirty;
mod build_owner;
mod reconcile;
mod hit_test;
mod pipeline;
mod layouter;
mod painter;
mod reconciler;
mod global_key_registry;
mod style;
mod update_result;
mod stateful_widget;
mod child_ops;

pub mod widgets;
pub mod elements;
pub mod render_objects;
pub mod focus;

#[cfg(test)]
mod key_tests;
#[cfg(test)]
mod reconcile_tests;
#[cfg(test)]
mod element_registry_tests;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod e2e_test;
#[cfg(test)]
mod window_integration_test;
#[cfg(test)]
mod build_owner_tests;
#[cfg(test)]
mod stateful_integration_test;

// --- Re-exports from former retain/ ---

pub use key::{Key, GlobalKey, WidgetKey};
pub use id::{ElementKey, RenderObjectKey};
pub use element_state::StateStorage;
pub use element::{Element, ElementRegistry};
pub use element_context::ElementContext;
pub use event_context::EventContext;
pub use render_object::{RenderObject, RenderObjectRegistry, LayoutContext, LayoutResult, PaintContext, HitTestContext};
pub use dirty::DirtyTracking;
pub use build_owner::{BuildOwner, RebuildResult};
pub use reconcile::Reconcilable;
pub use hit_test::HitTestResult;
pub use global_key_registry::{GlobalKeyRegistry, GlobalKeyError};
pub use style::Style;
pub use update_result::UpdateResult;
pub use stateful_widget::{StatefulWidget, BuildContext, StatefulElement, ProxyRenderObject, State, StateContext, SimpleState};
pub use child_ops::{ChildOp, ChildOps};
pub use focus::{FocusManager, FocusNodeId, FocusNodeData, Focus};
pub use widgets::{Widget, Text, Flex, Grid, DecoratedContainer, GestureDetector, MouseRegion, TextEdit, TextEditState, TextEditingController, Transform, WithLayout};
pub use input::SystemCursorKind;
pub use elements::{LeafElement, ContainerElement};
pub use render_objects::{TextRenderObject, ContainerRenderObject, TextEditRenderObject};
pub use pipeline::ThreeTreePipeline;

extern crate alloc;

pub trait Application: Sized + 'static {
    type State;

    fn new() -> Self::State;

    /// Returns a widget tree for the three-tree architecture.
    fn view(state: &mut Self::State, font_system: &mut glyphon::FontSystem) -> Box<dyn Widget>;
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