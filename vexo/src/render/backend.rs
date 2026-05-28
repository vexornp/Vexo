//! Render backend abstraction.
//!
//! This module provides a trait for render backends, enabling testing
//! without GPU dependencies and supporting multiple rendering implementations.

use glyphon::FontSystem;

use crate::core::{Physical, Scale, Size};
use crate::frame_builder::FrameBuilder;

/// Configuration for the render backend.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Physical size in screen pixels.
    pub size: Size<Physical>,
    /// DPI scale factor.
    pub scale: Scale,
}

impl RenderConfig {
    /// Create a new render config.
    pub fn new(size: Size<Physical>, scale: Scale) -> Self {
        Self { size, scale }
    }

    /// Get width as u32 for GPU APIs.
    pub fn width(&self) -> u32 {
        self.size.width_u32()
    }

    /// Get height as u32 for GPU APIs.
    pub fn height(&self) -> u32 {
        self.size.height_u32()
    }

    /// Get scale factor as f32 for GPU APIs.
    pub fn scale_factor(&self) -> f32 {
        self.scale.factor()
    }

    /// Get screen size as [f32; 2] for GPU uniforms.
    pub fn screen_size_array(&self) -> [f32; 2] {
        self.size.to_array()
    }
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            size: Size::new(800.0, 600.0),
            scale: Scale::default(),
        }
    }
}

/// Trait for render backends.
///
/// This abstraction enables:
/// - Testing rendering without GPU
/// - Multiple rendering implementations (wgpu, mock, etc.)
/// - Clear separation of concerns
pub trait RenderBackend {
    /// Prepare render data from the frame_builder.
    ///
    /// This method processes the accumulated render commands and
    /// prepares them for rendering.
    fn prepare(
        &mut self,
        frame_builder: &mut FrameBuilder,
        font_system: &mut FontSystem,
        config: RenderConfig,
    );

    /// Execute the render pass.
    ///
    /// This method submits the prepared data to the GPU.
    fn render(&mut self) -> Result<(), RenderError>;

    /// Resize the render surface.
    fn resize(&mut self, config: RenderConfig);

    /// Check if the backend is ready to render.
    fn is_ready(&self) -> bool;
}

/// Errors that can occur during rendering.
#[derive(Debug, Clone)]
pub enum RenderError {
    /// The surface is not configured.
    SurfaceNotConfigured,
    /// Failed to acquire the next texture.
    AcquireFailed(String),
    /// Failed to prepare text rendering.
    TextPrepareFailed(String),
    /// GPU error.
    GpuError(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::SurfaceNotConfigured => write!(f, "Surface not configured"),
            RenderError::AcquireFailed(msg) => write!(f, "Failed to acquire texture: {}", msg),
            RenderError::TextPrepareFailed(msg) => write!(f, "Text prepare failed: {}", msg),
            RenderError::GpuError(msg) => write!(f, "GPU error: {}", msg),
        }
    }
}

impl std::error::Error for RenderError {}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_config_default() {
        let config = RenderConfig::default();
        assert_eq!(config.width(), 800);
        assert_eq!(config.height(), 600);
        assert_eq!(config.scale_factor(), 1.0);
    }

    #[test]
    fn test_render_error_display() {
        let err = RenderError::SurfaceNotConfigured;
        assert_eq!(format!("{}", err), "Surface not configured");

        let err = RenderError::AcquireFailed("timeout".to_string());
        assert_eq!(format!("{}", err), "Failed to acquire texture: timeout");
    }
}
