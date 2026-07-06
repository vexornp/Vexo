//! Element implementations for the retain-mode system.
//!
//! This module provides element types that bridge widgets and render objects:
//!
//! - `RenderObjectElement` - Trait for elements that own render objects
//! - `LeafRenderObjectElement` - Element with no children (leaf widgets)
//! - `ContainerElement` - Element with multiple children (container widgets)

mod container;
mod leaf;
mod offstage;
mod opacity;
mod positioned;
mod render_object_element;
mod scroll_view;

pub use container::ContainerElement;
pub use leaf::{LeafElement, LeafRenderObjectElement};
pub use offstage::OffstageElement;
pub use opacity::OpacityElement;
pub use positioned::PositionedElement;
pub use render_object_element::RenderObjectElement;
pub use scroll_view::ScrollViewElement;
