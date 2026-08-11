//! Vexo — a cross-platform UI framework in Rust with Flutter's three-tree architecture.
//!
//! Vexo brings [Flutter's](https://flutter.dev) battle-tested rendering model to
//! pure Rust: one codebase, GPU-rendered via [`wgpu`], running natively on iOS,
//! Android, macOS, Windows, and Linux. No webview, no JS/HTML/CSS, no system
//! widget bridge — the same Rust code renders through Metal, Vulkan, DX12, or
//! OpenGL.
//!
//! # Architecture
//!
//! Vexo uses a **retained-mode** three-tree architecture that separates
//! *description* from *lifecycle* from *painting*:
//!
//! 1. **Widget tree** — immutable descriptions of UI (what to show)
//! 2. **Element tree** — mutable lifecycle managers (state + children)
//! 3. **Render object tree** — performs layout and painting (how to show it)
//!
//! Only what actually changed gets rebuilt, reconciled, and repainted — there
//! is no virtual DOM and no global diffing. Reactive [`Signal<T>`] primitives
//! drive targeted rebuilds.
//!
//! # Quickstart
//!
//! ```no_run
//! use vexo::{column, Application, ComponentState, Signal, Text, Widget};
//! use vexo::{FetchError, HttpFetch};
//! use url::Url;
//!
//! #[derive(ComponentState, Default)]
//! struct CounterState {
//!     count: Signal<u32>,
//! }
//!
//! impl Application for CounterState {
//!     type State = Self;
//!
//!     fn new() -> Self::State {
//!         CounterState::default()
//!     }
//!
//!     fn view(state: &mut Self::State) -> Box<dyn Widget> {
//!         let count = state.count.get();
//!         let sig = state.count.clone();
//!         column! {
//!             Text::new(format!("Count: {}", count)),
//!             Text::new("+1").on_press(move || { sig.set(sig.get() + 1); }),
//!         }
//!         .boxed()
//!     }
//! }
//!
//! # struct NoFetch;
//! # impl HttpFetch for NoFetch {
//! #     fn fetch(&self, _url: &Url) -> Result<Vec<u8>, FetchError> {
//! #         Err(FetchError::Network("doctest: network disabled".into()))
//! #     }
//! # }
//! # #[allow(dead_code)]
//! fn main() {
//!     vexo::run_desktop_demo::<CounterState>(std::sync::Arc::new(NoFetch)).unwrap();
//! }
//! ```
//!
//! The [`Application`] trait is the whole contract: `new()` gives you state,
//! `view()` returns a widget tree. State changes propagate through `Signal`s
//! and trigger only the affected subtrees to rebuild.
//!
//! # Key concepts
//!
//! - **Everything is a widget.** Padding is a widget, gestures are a widget,
//!   cursors are a widget, focus is a widget. Composition over imperative APIs.
//! - **Reactive state** — [`Signal<T>`] is the primitive. `set()` triggers
//!   targeted rebuilds of subtrees that read the signal. No diffing.
//! - **Declarative layout** — Taffy flexbox + grid via the [`layout`] module,
//!   with proper text layout through `glyphon`.
//! - **Cross-platform by construction** — one `Application::view()` impl, many
//!   backends. Desktop runs on `winit`; iOS uses Metal via `wgpu`; Android
//!   uses Vulkan via `wgpu`.
//!
//! # Companion crates
//!
//! - [`vexo_uikit`](https://docs.rs/vexo_uikit) — rich UI component library
//!   (buttons, navigation, tab bars, scaffolding) built on `vexo`
//! - [`vexo_fontawesome`](https://docs.rs/vexo_fontawesome) — Font Awesome 6
//!   Free Solid icon widgets
//!
//! See the [project README](https://github.com/vexornp/Vexo) for build
//! instructions, mobile setup, and the roadmap.
//!
//! [`Signal<T>`]: Signal
//! [`wgpu`]: https://docs.rs/wgpu

use std::error::Error;
use std::sync::mpsc;
use std::sync::Arc;

use winit::event_loop::EventLoop;

pub use core::AffineTransform;
pub use core::Color;
pub use core::{KeyboardAnimation, KeyboardAnimationSource, KeyboardInsetSource};
pub use core::{Logical, Size};
pub use glyphon;
pub use image_data::{ImageData, ImageDataError};
pub use image_cache::{FetchError, HttpFetch, ImageCache, ImageCacheProxy, LoadState, WinitImageCacheProxy};
pub use uniffi;

mod app;
pub use app::VexoApp;

pub mod animation;

pub use animation::{
    AnimationController, AnimationDirection, AnimationTicker, ColorTween, CubicBezierCurve,
    CurvedAnimation, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, FloatTween,
    FrictionSimulation, LinearCurve, Simulation, SpringDescription, SpringSimulation, TickHandle,
    Tolerance, Tween,
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
mod image_cache;
mod image_data;
mod image_instance;
mod quad_instance;
mod shadow_math;
pub mod render;
pub use frame_builder::FrameBuilder;
pub mod reactive;
pub mod resource;
pub use component_state_derive::ComponentState;
pub use vexo_macros::{column, row};
pub mod view_builder;
pub use view_builder::{build_array, build_block, build_either, build_optional};
pub use reactive::Signal;
mod macros;
pub mod state;
mod text_cache;
mod text_pipeline;
mod text_processor;
mod text_overflow;
mod user_event;
pub use user_event::VexoUserEvent;
mod window;
pub use window::WindowState;

pub use state::CursorBlinkState;
pub use winit::dpi::PhysicalPosition;

pub use layout::{
    AlignContent, AlignItems, AlignSelf, Display, EdgeInsets, FlexDirection, FlexWrap,
    GridAutoFlow, GridPlacement, JustifyContent, Layout, Overflow, Position, TrackSizing,
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
mod root_signal_cascade_test;
#[cfg(test)]
mod window_integration_test;
#[cfg(test)]
mod text_max_lines_integration_test;

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
};
#[deprecated(
    since = "0.x",
    note = "Internal API — framework-managed, not for direct use"
)]
pub use render_objects::{
    ContainerRenderObject, ImageRenderObject, ProxyRenderObject, TextEditRenderObject,
    TextRenderObject,
};
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
pub use style::{BoxShadow, Style};
pub use update_result::UpdateResult;
pub use widgets::{
    Brightness, ChildPush, ClipRRect, DecoratedBox, FadeTransition, FractionalTranslation,
    GestureDetector, Grid, Image, IndexedStack, MediaQuery, MediaQueryData, MediaQueryMutator,
    MultiChild, Offstage, Opacity, Orientation, Positioned, RemoveEdges, SafeArea, ScrollController,
    ScrollView, Shared, SlideDirection, SlideTransition, Spacer, Stack, Text, TextEdit, TextEditState,
    TextEditingController, Theme, ThemeData, Transform, Widget, WithLayout,
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
        let app_view = A::view(state);
        crate::widgets::RootMediaQuery::new(app_view).boxed()
    }
}

pub fn run_desktop_demo<A: Application + 'static>(
    image_fetcher: Arc<dyn crate::image_cache::HttpFetch>,
) -> Result<(), Box<dyn Error>> {
    // Initialize logger with debug level for retain mode by default
    // Override with RUST_LOG environment variable if needed
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    let event_loop = EventLoop::new()?;
    let proxy = event_loop.create_proxy();
    let (sender, receiver) = mpsc::channel::<VexoUserEvent>();
    let winit_proxy = crate::image_cache::WinitImageCacheProxy::new(proxy, sender);
    let image_cache = Arc::new(crate::image_cache::ImageCache::new(
        image_fetcher,
        Arc::new(winit_proxy),
    ));

    let app = VexoApp::<A>::new(image_cache, receiver);

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
    image_fetcher: Arc<dyn crate::image_cache::HttpFetch>,
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
    let proxy = event_loop.create_proxy();
    let (sender, receiver) = mpsc::channel::<VexoUserEvent>();
    let winit_proxy = crate::image_cache::WinitImageCacheProxy::new(proxy, sender);
    let image_cache = Arc::new(crate::image_cache::ImageCache::new(
        image_fetcher,
        Arc::new(winit_proxy),
    ));

    let app = VexoApp::<A>::new(image_cache, receiver);
    Ok(event_loop.run_app(app)?)
}
