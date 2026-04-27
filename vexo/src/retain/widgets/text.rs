//! Text widget - displays a string.

use super::{Element, Key, Widget};
use super::super::RenderObject;
use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{LayoutContext, PaintContext, HitTestContext};

/// Text widget - displays a string.
#[derive(Clone)]
pub struct Text {
    key: Option<Key>,
    content: String,
}

impl Text {
    /// Create a new text widget.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            key: None,
            content: content.into(),
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the text content.
    pub fn content(&self) -> &str {
        &self.content
    }
}

impl Widget for Text {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(crate::retain::elements::LeafElement::new())
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(TextRenderObject::new(&self.content))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// RenderObject for text display.
pub struct TextRenderObject {
    content: String,
    computed_bounds: Option<Bounds<Logical>>,
}

impl TextRenderObject {
    /// Create a new text render object.
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
            computed_bounds: None,
        }
    }

    /// Get the text content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for TextRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // TODO: Use font system for accurate measurement
        // For now, estimate based on content length
        let estimated_width = (self.content.len() as f32 * 10.0).min(constraints.max_width);
        let estimated_height = 20.0_f32.min(constraints.max_height);

        let size = Size::new(
            estimated_width.max(constraints.min_width),
            estimated_height.max(constraints.min_height),
        );

        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Text is handled separately via glyphon
        // Return empty for now - text collection happens in pipeline
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_widget_creation() {
        let widget = Text::new("Hello");
        assert_eq!(widget.content(), "Hello");
    }

    #[test]
    fn test_text_widget_with_key() {
        let widget = Text::new("Hello").with_key("greeting");
        assert_eq!(widget.key(), Some(Key::new("greeting")));
    }

    #[test]
    fn test_text_widget_clone() {
        let widget = Text::new("Hello").with_key("greeting");
        let cloned = widget.clone();

        assert_eq!(widget.content(), cloned.content());
        assert_eq!(widget.key(), cloned.key());
    }
}