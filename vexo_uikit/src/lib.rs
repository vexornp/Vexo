//! Rich UI component library built on vexo.
//!
//! Provides platform-adaptive widgets like Button, Toggle, TabView, etc.
//! Components compose vexo's base widgets and adapt their appearance
//! based on the current platform (Desktop vs Mobile).

// Re-exports from vexo that uikit consumers commonly need
pub use vexo::Color;
pub use vexo::Component;
pub use vexo::ComponentState;
pub use vexo::Signal;
pub use vexo::Widget;

pub mod platform;
pub use platform::Platform;

pub mod theme;

pub mod button;
pub use button::{Button, ButtonState, ButtonVariant};

pub mod navigation;
pub use navigation::{
    base_fx_alpha, NavigationController, NavigationStackView, NavigationStackViewState,
};

pub mod transitions;
pub use transitions::{
    default_desktop_transition, default_mobile_transition, default_transition, TransitionCtx,
    TransitionDir,
};
