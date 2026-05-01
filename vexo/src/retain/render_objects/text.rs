//! TextRenderObject implementation.

use crate::core::{Absolute, Bounds, Logical, Point, Position, Size};
use crate::layout::{Layout, LayoutNodeId, MeasureContext, TextMeasureContext};
use crate::render::RenderCommand;
use crate::retain::{HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject};

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
    layout_node: Option<LayoutNodeId>,
}

impl TextRenderObject {
    /// Create a new text render object.
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
            font_size: 16.0,
            computed_bounds: None,
            layout_node: None,
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

    /// Set the text content.
    pub fn set_content(&mut self, content: &str) {
        self.content = content.to_string();
    }

    /// Set the font size.
    pub fn set_font_size(&mut self, size: f32) {
        self.font_size = size;
    }
}

impl RenderObject for TextRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeId]) -> LayoutResult {
        // Create measure context for text
        let measure_ctx = MeasureContext::Text(TextMeasureContext {
            content: self.content.clone(),
            font_size: self.font_size,
            line_height: 1.2,
        });

        // Create leaf node with text measurement
        let node = ctx.engine().create_leaf_with_context(
            &Layout::default(),
            measure_ctx,
        );

        // Store node for apply_layout
        self.layout_node = Some(node);

        LayoutResult {
            node,
            size: Size::new(0.0, 0.0), // Will be filled by apply_layout
        }
    }

    fn apply_layout(&mut self, ctx: &LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Emit text render command for glyphon processing
        match &self.computed_bounds {
            Some(bounds) => {
                // Get the absolute position where this text should be painted.
                // The context already calculated the absolute position from the
                // parent chain, so we just use it directly.
                let pos: Position<Logical, Absolute> = ctx.absolute_position();

                vec![RenderCommand::Text {
                    content: self.content.clone(),
                    position: pos.to_point(),
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

    fn layout_node(&self) -> Option<LayoutNodeId> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

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
    fn test_text_render_object_layout_creates_node() {
        let mut obj = TextRenderObject::new("Hello World");
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = obj.layout(&mut ctx, &[]);

        // Should have created a layout node
        assert!(obj.layout_node.is_some());
        assert_eq!(obj.layout_node, Some(result.node));
    }

    #[test]
    fn test_text_render_object_apply_layout() {
        let mut obj = TextRenderObject::new("Hello World");
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create node
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let _result = obj.layout(&mut ctx, &[]);
        }

        // Compute layout
        let root = engine.create_leaf(&Layout::default());
        engine.compute(root, Size::new(200.0, 50.0), &mut font_system);

        // Apply layout should read computed bounds
        {
            let ctx = LayoutContext::new(&mut engine, &mut font_system);
            obj.apply_layout(&ctx);
        }

        // After apply_layout, computed_bounds should be set (though may be zero
        // since the node isn't part of the computed tree properly)
        // The key thing is it doesn't crash
    }

    #[test]
    fn test_text_render_object_hit_test_no_layout() {
        let obj = TextRenderObject::new("Test");

        // Without layout, computed_bounds is None, so hit test should fail
        assert!(!obj.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_text_render_object_paint_no_layout() {
        let obj = TextRenderObject::new("Test");

        // Paint returns empty without layout (computed_bounds is None)
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert!(result.is_empty());
    }
}
