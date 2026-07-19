use std::error::Error;
use std::sync::mpsc;

use winit::event_loop::EventLoop;

pub use core::AffineTransform;
pub use core::Color;
pub use glyphon;
pub use image_data::{ImageData, ImageDataError};
pub use uniffi;

mod app;
pub use app::{KeyBindingAction, VexoApp};

pub mod animation;

pub use animation::{
    AnimationController, AnimationDirection, AnimationTicker, ColorTween, CubicBezierCurve,
    CurvedAnimation, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, FloatTween, LinearCurve,
    TickHandle, Tween,
};
pub mod core;
pub mod editor;
pub mod input;
pub mod gestures;
pub mod layout;
mod mouse_tracker;
pub use mouse_tracker::MouseTracker;

mod frame_builder;
pub mod image_atlas;
mod image_data;
mod image_instance;
mod quad_instance;
pub mod render;
pub use frame_builder::FrameBuilder;
pub mod reactive;
pub mod resource;
pub use component_state_derive::ComponentState;
pub use reactive::Signal;
mod macros;
pub mod state;
mod text_cache;
mod text_pipeline;
mod text_processor;
mod window;
pub use window::WindowState;

pub use state::CursorBlinkState;
pub use winit::dpi::PhysicalPosition;

pub use layout::{
    AlignItems, AlignSelf, Display, FlexDirection, GridAutoFlow, Layout, Overflow,
    DEFAULT_LINE_HEIGHT_MULTIPLIER, LAYOUT_WIDTH_TOLERANCE,
};

/// Platform service abstractions (clipboard, etc.).
pub mod platform;
pub use platform::Clipboard;

// --- Former retain/ modules (flattened) ---

mod build_owner;
mod child_ops;
mod dirty;
mod element;
mod element_context;
mod element_state;
mod event_context;
mod event_handler;
mod global_key_registry;
pub mod inherited_registry;
pub mod inherited_widget;
mod hit_test;
mod id;
mod key;
mod layouter;
mod painter;
mod pipeline;
mod reconcile;
mod reconciler;
mod render_object;
mod stateful_widget;
mod style;
mod update_result;

pub mod elements;
pub mod focus;
pub mod render_objects;
pub mod widgets;

#[cfg(test)]
mod animation_flow_tests;
#[cfg(test)]
mod build_owner_tests;
#[cfg(test)]
mod e2e_test;
#[cfg(test)]
mod element_registry_tests;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod key_tests;
#[cfg(test)]
mod passthrough_integration;
#[cfg(test)]
mod reconcile_tests;
#[cfg(test)]
mod stateful_integration_test;
#[cfg(test)]
mod inherited_integration_test;
#[cfg(test)]
mod window_integration_test;

// --- Re-exports from former retain/ ---

#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use build_owner::{BuildOwner, RebuildResult};
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use child_ops::{ChildOp, ChildOps};
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use dirty::DirtyTracking;
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use element::{Element, ElementRegistry};
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use element_context::ElementContext;
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use element_state::StateStorage;
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use elements::{ContainerElement, LeafElement};
pub use event_context::EventContext;
pub use focus::{Focus, FocusElement, FocusManager, FocusNodeData, FocusNodeId};
pub use global_key_registry::{GlobalKeyError, GlobalKeyRegistry};
pub use hit_test::HitTestResult;
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use id::{ElementKey, RenderObjectKey};
pub use input::{MouseCursor, SystemCursorKind};
pub use key::{GlobalKey, Key, WidgetKey};
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use pipeline::ThreeTreePipeline;
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use reconcile::Reconcilable;
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use render_object::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectRegistry,
    SafeAreaClaimEdges,
};
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use render_objects::{
    ContainerRenderObject, ImageRenderObject, TextEditRenderObject, TextRenderObject,
};
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use stateful_widget::ProxyRenderObject;
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use stateful_widget::SimpleState;
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use stateful_widget::StatefulElement;
pub use stateful_widget::{Component, ComponentState, LifecycleContext, RenderContext};
pub use style::Style;
pub use update_result::UpdateResult;
pub use widgets::{
    Column, DecoratedContainer, FadeTransition, Flex, FractionalTranslation, GestureDetector, Grid,
    Image, IndexedStack, Offstage, Opacity, Positioned, Row, SafeArea, SafeAreaClaim,
    ScrollController, ScrollView, SlideDirection, SlideTransition, Stack, Text, TextEdit,
    TextEditState, TextEditingController, Theme, ThemeData, Transform, Widget, WithLayout,
};

extern crate alloc;

pub trait Application: Sized + 'static {
    type State: ComponentState + Default;

    fn new() -> Self::State;

    /// Returns a widget tree for the three-tree architecture.
    fn view(state: &mut Self::State) -> Box<dyn Widget>;

    /// Register additional fonts (e.g. icon fonts) with the window's
    /// `FontSystem`.
    ///
    /// Called once during window initialization, after the embedded default
    /// font has been loaded. The default implementation is a no-op.
    ///
    /// Use [`crate::resource::register_font`] to add font bytes; the family
    /// name embedded in the font file is what
    /// [`crate::Text::with_font_family`] references.
    fn register_fonts(_font_system: &mut glyphon::FontSystem) {}
}

/// Root component that bridges the `Application` trait into the widget tree.
///
/// Follows Flutter's design where the root widget is a regular `StatefulWidget`.
/// The `Application::State` becomes the `Component::State`, so `Signal` fields
/// are automatically wired by `StatefulElement::mount()` — when a `Signal::set()`
/// fires, the element is marked dirty and `Application::view()` is re-called
/// through the normal `perform_rebuilds()` pipeline.
pub(crate) struct RootComponent<A: Application> {
    _phantom: std::marker::PhantomData<A>,
}

impl<A: Application> Clone for RootComponent<A> {
    fn clone(&self) -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<A: Application> Default for RootComponent<A> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<A: Application> Component for RootComponent<A> {
    type State = A::State;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        A::view(state)
    }
}

pub fn run_desktop_demo<A: Application + 'static>() -> Result<(), Box<dyn Error>> {
    // Initialize logger with debug level for retain mode by default
    // Override with RUST_LOG environment variable if needed
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

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

/// Run the framework on Android.
///
/// Mirrors [`run_desktop_demo`] except for `EventLoop` construction: on
/// Android, winit requires the [`AndroidApp`] handle (delivered to
/// `android_main`) to be associated with the `EventLoopBuilder` via
/// [`EventLoopBuilderExtAndroid::with_android_app`]. After that, the
/// `VexoApp` event handler and the three-tree pipeline are reused
/// unchanged from the desktop path.
///
/// Logging is routed to logcat via `android_logger` (the `log` facade
/// is shared with desktop's `env_logger` — only the init differs).
///
/// [`AndroidApp`]: android_activity::AndroidApp
/// [`EventLoopBuilderExtAndroid::with_android_app`]: winit::platform::android::EventLoopBuilderExtAndroid::with_android_app
#[cfg(target_os = "android")]
pub fn run_android_demo<A: Application + 'static>(
    app: android_activity::AndroidApp,
) -> Result<(), Box<dyn Error>> {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    // Route `log::` output to logcat. `init_once` is idempotent so this
    // is safe even if the host crate already initialized a logger.
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("vexo")
            .with_filter(android_logger::FilterBuilder::new().parse("debug").build()),
    );

    let event_loop = EventLoop::builder()
        .with_android_app(app)
        .build()?;
    let (sender, receiver) = mpsc::channel();
    let app = VexoApp::<A>::new(&event_loop, receiver, sender);
    Ok(event_loop.run_app(app)?)
}
