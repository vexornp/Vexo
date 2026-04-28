//! TextRenderObject implementation.

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{HitTestContext, LayoutContext, PaintContext, RenderObject};

/// RenderObject for text display.
///
/// This render object handles layout and hit testing for text content.
/// Text rendering is handled separately by glyphon in the pipeline.
///
/// # Example
///
/// ```ignore
/// use vexo::retain::render_objects::TextRenderObject;
///
/// let obj = TextRenderObject::new("Hello World")
///     .with_font_size(24.0);
/// ```
pub struct TextRenderObject {
    content: String,
    font_size: f32,
    computed_bounds: Option<Bounds<Logical>>,
}

impl TextRenderObject {
    /// Create a new text render object.
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
            font_size: 16.0,
            computed_bounds: None,
        }
    }

    /// Set the font size.
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Get the text content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the font size.
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for TextRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // Estimate text size based on content
        // TODO: Integrate with font system for accurate measurement
        let char_width = self.font_size * 0.6; // Approximate character width
        let line_height = self.font_size * 1.2;

        let estimated_width = (self.content.len() as f32 * char_width).min(constraints.max_width);
        let estimated_height = line_height.min(constraints.max_height);

        let size = Size::new(
            estimated_width.max(constraints.min_width),
            estimated_height.max(constraints.min_height),
        );

        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Emit text render command for glyphon processing
        match &self.computed_bounds {
            Some(bounds) => {
                vec![RenderCommand::Text {
                    content: self.content.clone(),
                    position: bounds.position(),
                    font_size: self.font_size,
                    color: crate::core::Color::BLACK,
                    max_width: Some(bounds.width()),
                }]
            }
            None => vec![],
        }
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_render_object_new() {
        let obj = TextRenderObject::new("Hello");
        assert_eq!(obj.content(), "Hello");
        assert_eq!(obj.font_size(), 16.0); // default
        assert!(obj.computed_bounds().is_none());
    }

    #[test]
    fn test_text_render_object_with_font_size() {
        let obj = TextRenderObject::new("Hello").with_font_size(24.0);
        assert_eq!(obj.font_size(), 24.0);
    }

    #[test]
    fn test_text_render_object_layout() {
        let mut obj = TextRenderObject::new("Hello World");
        let constraints = LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 200.0,
            max_height: 50.0,
            ..LayoutConstraints::default()
        };
        let mut ctx = LayoutContext::mock();

        let size = obj.layout(constraints, &mut ctx);

        assert!(size.width > 0.0);
        assert!(size.height > 0.0);
        assert!(size.width <= 200.0);
        assert!(size.height <= 50.0);
        assert!(obj.computed_bounds().is_some());
    }

    #[test]
    fn test_text_render_object_layout_respects_min() {
        let mut obj = TextRenderObject::new("Hi");
        let constraints = LayoutConstraints {
            min_width: 100.0,
            min_height: 50.0,
            max_width: 200.0,
            max_height: 100.0,
            ..LayoutConstraints::default()
        };
        let mut ctx = LayoutContext::mock();

        let size = obj.layout(constraints, &mut ctx);

        assert!(size.width >= 100.0); // min_width
        assert!(size.height >= 50.0); // min_height
    }

    #[test]
    fn test_text_render_object_layout_with_font_size() {
        let mut obj = TextRenderObject::new("Hello").with_font_size(32.0);
        let constraints = LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 1000.0,
            max_height: 100.0,
            ..LayoutConstraints::default()
        };
        let mut ctx = LayoutContext::mock();

        let size = obj.layout(constraints, &mut ctx);

        // Larger font should result in larger height (line_height = font_size * 1.2)
        assert!(size.height > 16.0 * 1.2); // larger than default
    }

    #[test]
    fn test_text_render_object_hit_test_inside() {
        let mut obj = TextRenderObject::new("Test");
        let constraints = LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 100.0,
            max_height: 50.0,
            ..LayoutConstraints::default()
        };
        let mut ctx = LayoutContext::mock();

        obj.layout(constraints, &mut ctx);

        // Should hit inside bounds
        assert!(obj.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()));
        assert!(obj.hit_test(Point::new(0.0, 0.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_text_render_object_hit_test_outside() {
        let mut obj = TextRenderObject::new("Test");
        let constraints = LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 100.0,
            max_height: 50.0,
            ..LayoutConstraints::default()
        };
        let mut ctx = LayoutContext::mock();

        obj.layout(constraints, &mut ctx);

        // Should miss outside bounds
        assert!(!obj.hit_test(Point::new(200.0, 200.0), &HitTestContext::mock()));
        assert!(!obj.hit_test(Point::new(-10.0, 0.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_text_render_object_hit_test_no_layout() {
        let obj = TextRenderObject::new("Test");

        // Without layout, computed_bounds is None, so hit test should fail
        assert!(!obj.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_text_render_object_paint() {
        let obj = TextRenderObject::new("Test");

        // Paint returns empty (text handled by glyphon)
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert!(result.is_empty());
    }
}
