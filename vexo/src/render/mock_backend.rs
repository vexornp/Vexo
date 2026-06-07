//! Mock render backend for testing.
//!
//! Records render commands instead of executing them,
//! enabling unit tests to verify rendering output without a GPU.

use crate::core::{Physical, Size};
use crate::frame_builder::FrameBuilder;

/// Mock render backend that records commands for testing.
pub struct MockBackend {
    pub commands: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub last_rect_count: usize,
    pub last_text_count: usize,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBackend {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            width: 800,
            height: 600,
            last_rect_count: 0,
            last_text_count: 0,
        }
    }

    pub fn prepare(&mut self, frame_builder: &FrameBuilder) {
        self.commands.clear();
        self.last_rect_count = frame_builder.quad_count();
        self.last_text_count = frame_builder.text_count();

        for quad in frame_builder.quad_instances() {
            self.commands
                .push(format!("rect at {:?} size {:?}", quad.position, quad.size));
        }

        for text in frame_builder.text_requests() {
            self.commands.push(format!(
                "text '{}' at {:?} size {}",
                text.content, text.position, text.size
            ));
        }
    }

    pub fn render(&mut self) {
        // No-op for testing
    }

    pub fn update_viewport(&mut self, _size: Size<Physical>) {
        // No-op for testing
    }

    pub fn resize(&mut self, _config: crate::render::RenderConfig) {
        // No-op for testing
    }

    pub fn is_ready(&self) -> bool {
        true
    }

    pub fn render_count(&self) -> usize {
        self.commands.len()
    }

    pub fn has_command(&self, pattern: &str) -> bool {
        self.commands.iter().any(|c| c.contains(pattern))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Bounds, Color, Logical};

    #[test]
    fn test_mock_backend_prepare() {
        let mut backend = MockBackend::new();
        let mut frame_builder = FrameBuilder::new();

        frame_builder.add_rect(
            Bounds::<Logical>::from_xywh(0.0, 0.0, 100.0, 50.0),
            Color::RED,
            None,
            0.0,
        );

        backend.prepare(&frame_builder);

        assert_eq!(backend.last_rect_count, 1);
        assert!(backend.has_command("rect"));
    }

    #[test]
    fn test_mock_backend_has_command() {
        let mut backend = MockBackend::new();
        let mut frame_builder = FrameBuilder::new();

        frame_builder.add_text("Hello".to_string(), crate::core::Point::new(10.0, 10.0), 16.0, Color::BLACK, None);

        backend.prepare(&frame_builder);

        assert!(backend.has_command("Hello"));
        assert!(!backend.has_command("World"));
    }

    #[test]
    fn test_mock_backend_render_count() {
        let mut backend = MockBackend::new();
        let mut frame_builder = FrameBuilder::new();

        frame_builder.add_rect(
            Bounds::<Logical>::from_xywh(0.0, 0.0, 100.0, 50.0),
            Color::RED,
            None,
            0.0,
        );
        frame_builder.add_text("Test".to_string(), crate::core::Point::new(10.0, 10.0), 16.0, Color::BLACK, None);

        backend.prepare(&frame_builder);

        assert_eq!(backend.render_count(), 2);
    }
}