//! Rendering abstractions for the Vexo UI framework.
//!
//! This module provides the rendering layer that sits between widgets and
//! the GPU backend. It uses the RenderCommand pattern to decouple widget
//! painting from actual rendering.

mod backend;
mod command;
mod mock_backend;
mod wgpu_backend;

pub use backend::{RenderBackend, RenderConfig, RenderError};
pub use command::{RenderCommand, RenderCommandList, Stroke};
pub use mock_backend::MockBackend;
pub use wgpu_backend::WgpuBackend;
