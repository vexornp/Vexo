//! Rendering abstractions for the Vexo UI framework.
//!
//! This module provides the rendering layer that sits between widgets and
//! the GPU backend. It uses the RenderCommand pattern to decouple widget
//! painting from actual rendering.
//!
//! # Architecture
//!
//! The rendering layer consists of:
//! - `RenderCommand` - Immutable draw instructions produced by widgets
//! - `RenderBackend` - Trait for rendering implementations
//! - `WgpuBackend` - Production GPU rendering via wgpu
//! - `MockBackend` - Testing backend without GPU dependencies
//!
//! # Example
//!
//! ```
//! use vexo::render::{RenderBackend, RenderCommand, MockBackend};
//! use vexo::core::{Bounds, Logical, Color};
//!
//! let mut backend = MockBackend::new();
//! // Backend can be used for testing without GPU
//! ```

mod backend;
mod command;
mod command_processor;
mod mock_backend;
mod wgpu_backend;

pub use backend::{RenderBackend, RenderConfig, RenderError};
pub use command::{RenderCommand, RenderCommandList, Stroke};
pub use command_processor::process_commands;
pub use mock_backend::MockBackend;
pub use wgpu_backend::WgpuBackend;
