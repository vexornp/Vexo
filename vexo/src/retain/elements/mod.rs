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

pub use render_object_element::RenderObjectElement;
pub use leaf::{LeafRenderObjectElement, LeafElement};
pub use container::ContainerElement;