//! Element implementations for the retain-mode system.
//!
//! This module provides element types that bridge widgets and render objects:
//!
//! - `RenderObjectElement` - Trait for elements that own render objects
//! - `LeafRenderObjectElement` - Element with no children (leaf widgets)
//! - `ContainerElement` - Element with multiple children (container widgets)

mod render_object_element;
mod leaf;
mod container;
mod scroll_view;
mod opacity;

pub use render_object_element::RenderObjectElement;
pub use leaf::{LeafRenderObjectElement, LeafElement};
pub use container::ContainerElement;
pub use scroll_view::ScrollViewElement;
pub use opacity::OpacityElement;