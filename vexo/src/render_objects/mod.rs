//! RenderObject implementations for the retain rendering system.
//!
//! This module provides reusable RenderObject implementations for
//! common widget types. These can be used by widget implementations
//! or directly for testing.
//!
//! # Available RenderObjects
//!
//! - [`TextRenderObject`]: Renders text content
//! - [`ContainerRenderObject`]: Container for child render objects (Flex)

mod clip_rrect;
mod container;
mod decorated_box;
mod image;
mod indexed_stack;
mod offstage;
mod opacity;
mod positioned;
mod scroll_view;
mod text;
mod text_edit;

pub use clip_rrect::ClipRRectRenderObject;
pub use container::ContainerRenderObject;
pub use decorated_box::DecoratedBoxRenderObject;
pub use image::ImageRenderObject;
pub use indexed_stack::IndexedStackRenderObject;
pub use offstage::OffstageRenderObject;
pub use opacity::OpacityRenderObject;
pub use positioned::{PositionedInsets, PositionedRenderObject};
pub use scroll_view::ScrollViewRenderObject;
pub use text::TextRenderObject;
pub use text_edit::TextEditRenderObject;
