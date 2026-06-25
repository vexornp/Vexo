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

mod text;
mod container;
mod text_edit;
mod scroll_view;
mod image;
mod opacity;

pub use text::TextRenderObject;
pub use container::ContainerRenderObject;
pub use text_edit::TextEditRenderObject;
pub use scroll_view::ScrollViewRenderObject;
pub use image::ImageRenderObject;
pub use opacity::OpacityRenderObject;
