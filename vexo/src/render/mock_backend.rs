//! Mock render backend for testing.
//!
//! This module provides a mock implementation of RenderBackend
//! that captures render commands without requiring GPU access.

use crate::render::backend::{RenderBackend, RenderConfig, RenderError};
use crate::renderer::UiBatcher;
use glyphon::FontSystem;

/// Mock render backend for testing.
///
/// Captures render commands and tracks rendering state without
/// requiring actual GPU resources.
#[derive(Debug, Default)]
pub struct MockBackend {
    /// Whether the backend is ready to render.
    ready: bool,
    /// Current render configuration.
    config: Option<RenderConfig>,
    /// Number of times prepare was called.
    prepare_count: usize,
    /// Number of times render was called.
    render_count: usize,
    /// Last screen size set.
    last_screen_size: Option<(f32, f32)>,
    /// Number of quad instances in last prepare.
    last_quad_count: usize,
    /// Number of text requests in last prepare.
    last_text_count: usize,
    /// Number of editor requests in last prepare.
    last_editor_count: usize,
}

impl MockBackend {
    /// Create a new mock backend.
    pub fn new() -> Self {
        Self {
            ready: true,
            config: None,
            prepare_count: 0,
            render_count: 0,
            last_screen_size: None,
            last_quad_count: 0,
            last_text_count: 0,
            last_editor_count: 0,
        }
    }

    /// Create a mock backend that is not ready.
    pub fn not_ready() -> Self {
        Self {
            ready: false,
            ..Self::default()
        }
    }

    /// Get the number of times prepare was called.
    pub fn prepare_count(&self) -> usize {
        self.prepare_count
    }

    /// Get the number of times render was called.
    pub fn render_count(&self) -> usize {
        self.render_count
    }

    /// Get the last screen size.
    pub fn last_screen_size(&self) -> Option<(f32, f32)> {
        self.last_screen_size
    }

    /// Get the last quad count.
    pub fn last_quad_count(&self) -> usize {
        self.last_quad_count
    }

    /// Get the last text count.
    pub fn last_text_count(&self) -> usize {
        self.last_text_count
    }

    /// Get the last editor count.
    pub fn last_editor_count(&self) -> usize {
        self.last_editor_count
    }

    /// Get the current config.
    pub fn config(&self) -> Option<&RenderConfig> {
        self.config.as_ref()
    }
}

impl RenderBackend for MockBackend {
    fn prepare(
        &mut self,
        batcher: &mut UiBatcher,
        _font_system: &mut FontSystem,
        config: RenderConfig,
    ) {
        self.prepare_count += 1;
        self.config = Some(config.clone());
        self.last_screen_size = Some((config.width as f32, config.height as f32));
        self.last_quad_count = batcher.quad_instances.len();
        self.last_text_count = batcher.text_requests.len();
        self.last_editor_count = batcher.editor_requests.len();
    }

    fn render(&mut self) -> Result<(), RenderError> {
        if !self.ready {
            return Err(RenderError::SurfaceNotConfigured);
        }
        self.render_count += 1;
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32, scale_factor: f32) {
        self.config = Some(RenderConfig {
            width,
            height,
            scale_factor,
        });
        self.ready = true;
    }

    fn is_ready(&self) -> bool {
        self.ready
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_backend_new() {
        let backend = MockBackend::new();
        assert!(backend.is_ready());
        assert_eq!(backend.prepare_count(), 0);
        assert_eq!(backend.render_count(), 0);
    }

    #[test]
    fn test_mock_backend_not_ready() {
        let backend = MockBackend::not_ready();
        assert!(!backend.is_ready());
    }

    #[test]
    fn test_mock_backend_prepare() {
        let mut backend = MockBackend::new();
        let mut batcher = UiBatcher::new();
        let mut font_system = FontSystem::new();
        let config = RenderConfig {
            width: 1024,
            height: 768,
            scale_factor: 2.0,
        };

        backend.prepare(&mut batcher, &mut font_system, config);

        assert_eq!(backend.prepare_count(), 1);
        assert_eq!(backend.last_screen_size(), Some((1024.0, 768.0)));
        assert_eq!(backend.last_quad_count(), 0);
    }

    #[test]
    fn test_mock_backend_render() {
        let mut backend = MockBackend::new();

        let result = backend.render();
        assert!(result.is_ok());
        assert_eq!(backend.render_count(), 1);
    }

    #[test]
    fn test_mock_backend_render_not_ready() {
        let mut backend = MockBackend::not_ready();

        let result = backend.render();
        assert!(result.is_err());
        assert!(matches!(result, Err(RenderError::SurfaceNotConfigured)));
    }

    #[test]
    fn test_mock_backend_resize() {
        let mut backend = MockBackend::not_ready();

        backend.resize(800, 600, 1.5);

        assert!(backend.is_ready());
        let config = backend.config().unwrap();
        assert_eq!(config.width, 800);
        assert_eq!(config.height, 600);
        assert_eq!(config.scale_factor, 1.5);
    }
}
