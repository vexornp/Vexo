//! Element implementations for the retain-mode system.
//!
//! This module provides element types that bridge widgets and render objects:
//!
//! - `RenderObjectElement` - Trait for elements that own render objects
//! - `LeafRenderObjectElement` - Element with no children (leaf widgets)
//! - `ContainerElement` - Element with multiple children (container widgets)
//! - `SingleChildRenderObjectElement` - Trait for elements with one child
//! - `MultiChildRenderObjectElement` - Trait for elements with multiple children

mod render_object_element;
mod single_child;
mod multi_child;
mod leaf;
mod container;

pub use render_object_element::RenderObjectElement;
pub use single_child::SingleChildRenderObjectElement;
pub use multi_child::MultiChildRenderObjectElement;
pub use leaf::{LeafRenderObjectElement, LeafElement};
pub use container::ContainerElement;