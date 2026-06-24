//! Rich UI component library built on vexo.
//!
//! Provides platform-adaptive widgets like Button, Toggle, TabView, etc.
//! Components compose vexo's base widgets and adapt their appearance
//! based on the current platform (Desktop vs Mobile).

// Re-exports from vexo that uikit consumers commonly need
pub use vexo::Component;
pub use vexo::ComponentState;
pub use vexo::Signal;
pub use vexo::Color;
pub use vexo::Widget;

pub mod platform;
pub use platform::Platform;

pub mod theme;
