//! TextRenderObject implementation.

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position, Size};
use crate::layout::{
    Layout, LayoutNodeKey, MeasureContext, TextMeasureContext, TextMeasurer,
    DEFAULT_LINE_HEIGHT_MULTIPLIER, LAYOUT_WIDTH_TOLERANCE,
};
use crate::render::RenderCommand;
use crate::style::Style;
use crate::{HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject};

/// RenderObject for text display.
///
/// This render object handles layout and hit testing for text content.
/// Text rendering is handled separately by glyphon in the pipeline.
///
/// # Example
///
/// ```ignore
/// use vexo::render_objects::TextRenderObject;
///
/// let obj = TextRenderObject::new("Hello World")
///     .with_font_size(24.0);
/// ```
pub struct TextRenderObject {
    content: String,
    font_size: f32,
    /// Line-height multiplier applied to `font_size` when measuring text.
    /// Defaults to [`DEFAULT_LINE_HEIGHT_MULTIPLIER`] (1.2); overridable via
    /// [`with_line_height`].
    ///
    /// [`with_line_height`]: TextRenderObject::with_line_height
    line_height: f32,
    color: Color,
    style: Style,
    layout: Layout,
    computed_bounds: Option<Bounds<Logical>>,
    /// Actual height of the text when wrapped at the computed box width.
    /// Measured during `apply_layout` so `paint` can vertically center the
    /// (possibly multi-line) text correctly instead of assuming a single line.
    measured_text_height: Option<f32>,
    /// Natural (unwrapped) width of the text, measured in `apply_layout`.
    /// Used by `paint` to decide whether to pass a max_width to glyphon:
    /// if the natural width fits within the (integer-floored) box, no wrap
    /// constraint is emitted, avoiding spurious wrapping from Taffy's
    /// subpixel rounding.
    natural_text_width: Option<f32>,
    layout_node: Option<LayoutNodeKey>,
}

impl TextRenderObject {
    /// Create a new text render object.
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
            font_size: 24.0,
            line_height: DEFAULT_LINE_HEIGHT_MULTIPLIER,
            color: Color::BLACK,
            style: Style::default(),
            layout: Layout::default(),
            computed_bounds: None,
            measured_text_height: None,
            natural_text_width: None,
            layout_node: None,
        }
    }

    /// Set the font size.
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Set the line-height multiplier (applied to `font_size`).
    ///
    /// Defaults to [`DEFAULT_LINE_HEIGHT_MULTIPLIER`] (1.2).
    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    /// Set the text color.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the style.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the layout.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
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

    /// Get the text color.
    pub fn color(&self) -> Color {
        self.color
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    /// Set the text content.
    ///
    /// Returns true if the content changed.
    pub fn set_content(&mut self, content: &str) -> bool {
        let changed = self.content != content;
        if changed {
            self.content = content.to_string();
        }
        changed
    }

    /// Set the font size.
    ///
    /// Returns true if the font size changed.
    pub fn set_font_size(&mut self, size: f32) -> bool {
        if (self.font_size - size).abs() > f32::EPSILON {
            self.font_size = size;
            true
        } else {
            false
        }
    }

    /// Set the text color.
    ///
    /// Returns true if the color changed.
    pub fn set_color(&mut self, color: Color) -> bool {
        if self.color != color {
            self.color = color;
            true
        } else {
            false
        }
    }

    /// Set the style configuration.
    ///
    /// Returns true if the style changed.
    pub fn set_style(&mut self, style: Style) -> bool {
        if self.style != style {
            self.style = style;
            true
        } else {
            false
        }
    }

    /// Set the layout configuration.
    ///
    /// Returns true if the layout changed.
    pub fn set_layout(&mut self, layout: Layout) -> bool {
        if self.layout != layout {
            self.layout = layout;
            true
        } else {
            false
        }
    }
}

impl RenderObject for TextRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let measure_ctx = MeasureContext::Text(TextMeasureContext {
            content: self.content.clone(),
            font_size: self.font_size,
            line_height: self.line_height,
        });

        let layout = self.layout.clone();

        match self.layout_node {
            Some(existing) => {
                // Incremental: update measure context on existing node
                ctx.engine().set_context(existing, measure_ctx);
                ctx.engine().set_style(existing, &layout);
                LayoutResult {
                    node: existing,
                    size: Size::new(0.0, 0.0),
                }
            }
            None => {
                // First frame: create new node
                let node = ctx.engine().create_leaf_with_context(&layout, measure_ctx);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::new(0.0, 0.0),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
                // Measure the actual (possibly wrapped) text height at the
                // final box width. Paint tells glyphon to wrap at
                // `bounds.width()`, so the centered text must use the same
                // wrapped height — otherwise multi-line text is centered as a
                // single line and overflows the box, overlapping siblings.
                let mut measurer = TextMeasurer::new(ctx.font_system());
                let natural =
                    measurer.measure(&self.content, self.font_size, self.line_height, None, None);
                // Taffy floors layout widths to integers, so a text whose
                // natural width is e.g. 41.51 may receive box_w=41, which
                // would spuriously trigger wrapping. Treat the box as
                // unbounded when the natural width fits within tolerance.
                let box_w = computed.bounds.width();
                let effective_max = if natural.width <= box_w + LAYOUT_WIDTH_TOLERANCE {
                    None
                } else {
                    Some(box_w)
                };
                let size = measurer.measure(
                    &self.content,
                    self.font_size,
                    self.line_height,
                    effective_max,
                    None,
                );
                self.measured_text_height = Some(size.height);
                self.natural_text_width = Some(natural.width);
            }
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Emit text render command for glyphon processing
        match &self.computed_bounds {
            Some(bounds) => {
                let mut commands = Vec::new();
                let pos: Position<Logical, Absolute> = ctx.absolute_position();

                // Compute vertical centering offset when the layout box is taller
                // than the text's actual height. Use the wrapped height measured
                // in `apply_layout` (at `bounds.width()`), falling back to a
                // single line if measurement is unavailable.
                let text_height = self
                    .measured_text_height
                    .unwrap_or(self.font_size * self.line_height);
                let vertical_offset = ((bounds.height() - text_height) / 2.0).max(0.0);

                let absolute_bounds = Bounds::new(
                    pos.x,
                    pos.y,
                    pos.x + bounds.width(),
                    pos.y + bounds.height(),
                );

                // 1. Push corner radius if set (affects all subsequent rects)
                if let Some(ref cr) = self.style.corner_radius {
                    commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
                }

                // 2. Draw background first (behind text)
                if let Some(bg_color) = self.style.background {
                    commands.push(RenderCommand::rect(absolute_bounds, bg_color));
                }

                // 3. Draw border on top (after background)
                if let Some(ref border) = self.style.border {
                    commands.push(RenderCommand::rect_with_border(
                        absolute_bounds,
                        Color::TRANSPARENT,
                        border.color,
                        border.width,
                    ));
                }

                // 4. Pop corner radius
                if self.style.corner_radius.is_some() {
                    commands.push(RenderCommand::PopCornerRadius);
                }

                // 5. Draw text on top of decorations (vertically centered)
                let text_pos = Point::new(pos.x, pos.y + vertical_offset);
                // Match apply_layout's tolerance: Taffy floors layout widths
                // to integers, so a box slightly narrower than the natural
                // text width must not trigger wrapping at paint time. Pass
                // None (no wrap) when the natural width fits within tolerance.
                let max_width = match self.natural_text_width {
                    Some(natural_w) if natural_w <= bounds.width() + LAYOUT_WIDTH_TOLERANCE => None,
                    _ => Some(bounds.width()),
                };
                commands.push(RenderCommand::Text {
                    content: self.content.clone(),
                    position: text_pos,
                    font_size: self.font_size,
                    color: self.color,
                    max_width,
                });

                commands
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

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        if self.style.clip {
            self.computed_bounds
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_text_render_object_new() {
        let obj = TextRenderObject::new("Hello");
        assert_eq!(obj.content(), "Hello");
        assert_eq!(obj.font_size(), 24.0); // default
        assert!(obj.computed_bounds().is_none());
    }

    #[test]
    fn test_text_render_object_with_font_size() {
        let obj = TextRenderObject::new("Hello").with_font_size(24.0);
        assert_eq!(obj.font_size(), 24.0);
    }

    #[test]
    fn test_text_render_object_default_color_is_black() {
        let obj = TextRenderObject::new("Hello");
        assert_eq!(obj.color(), Color::BLACK);
    }

    #[test]
    fn test_text_render_object_with_color() {
        let obj = TextRenderObject::new("Hello").with_color(Color::RED);
        assert_eq!(obj.color(), Color::RED);
    }

    #[test]
    fn test_text_render_object_set_color_change_detection() {
        let mut ro = TextRenderObject::new("Hello");
        // default is BLACK, setting RED should report changed
        assert!(ro.set_color(Color::RED));
        // setting same value again should report unchanged
        assert!(!ro.set_color(Color::RED));
        // setting back to BLACK should report changed
        assert!(ro.set_color(Color::BLACK));
    }

    #[test]
    fn test_text_render_object_paint_emits_color() {
        let mut ro = TextRenderObject::new("Hello").with_color(Color::BLUE);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Find the Text command and verify its color
        let text_cmd = cmds.iter().find_map(|c| match c {
            RenderCommand::Text { color, .. } => Some(*color),
            _ => None,
        });
        assert_eq!(text_cmd, Some(Color::BLUE));
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
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            obj.apply_layout(&mut ctx);
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

    #[test]
    fn test_text_render_object_with_style_background_paint() {
        let style = crate::Style::new().background(crate::core::Color::RED);
        let mut ro = TextRenderObject::new("Hello").with_style(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert!(
            cmds.len() >= 2,
            "expected at least 2 commands, got {}",
            cmds.len()
        );
    }

    #[test]
    fn test_text_render_object_set_style_change_detection() {
        let style1 = crate::Style::new().background(crate::core::Color::RED);
        let style2 = crate::Style::new().background(crate::core::Color::BLUE);
        let style2_dup = style2.clone();
        let mut ro = TextRenderObject::new("Hello").with_style(style1);
        assert!(ro.set_style(style2));
        assert!(!ro.set_style(style2_dup));
    }

    #[test]
    fn test_text_render_object_set_layout_change_detection() {
        let layout1 = Layout::default().padding(8.0);
        let layout2 = Layout::default().padding(16.0);
        let layout2_dup = layout2.clone();
        let mut ro = TextRenderObject::new("Hello").with_layout(layout1);
        assert!(ro.set_layout(layout2));
        assert!(!ro.set_layout(layout2_dup));
    }
}
