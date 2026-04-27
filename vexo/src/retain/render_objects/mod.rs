//! RenderObject implementations for the retain rendering system.
//!
//! This module provides reusable RenderObject implementations for
//! common widget types. These can be used by widget implementations
//! or directly for testing.
//!
//! # Available RenderObjects
//!
//! - [`TextRenderObject`]: Renders text content
//! - [`ContainerRenderObject`]: Container for child render objects (Column/Row)

mod text;
mod container;

pub use text::TextRenderObject;
pub use container::ContainerRenderObject;
