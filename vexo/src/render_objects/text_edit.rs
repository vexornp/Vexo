//! TextEditRenderObject implementation.
//!
//! Leaf render object that paints both text content and a blinking cursor,
//! following Flutter's `RenderEditable` pattern.

use std::cell::RefCell;
use std::rc::Rc;

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position, Size};
use crate::editor::Editor;
use crate::layout::{
    Layout, LayoutNodeKey, MeasureContext, TextMeasureContext, DEFAULT_LINE_HEIGHT_MULTIPLIER,
};
use crate::render::RenderCommand;
use crate::style::Style;
use crate::{HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject};

/// Accent blue cursor color.
const CURSOR_COLOR: Color = Color::rgb(0.3, 0.67, 0.97);

/// Semi-transparent blue used for the text selection highlight.
///
/// Drawn behind the text (all quads render before all text in the GPU pass,
/// so this naturally appears underneath the glyphs). The caret is emitted
/// after this rect in the command stream, so the caret renders on top.
const SELECTION_COLOR: Color = Color::rgb(0.3, 0.5, 0.85).with_alpha(0.3);

/// RenderObject for editable text with cursor painting.
///
/// This render object extends the text rendering pattern with cursor support.
/// It emits both `RenderCommand::Text` for the content and
/// `RenderCommand::Caret` for the blinking cursor (when focused and visible).
///
/// # Example
///
/// ```ignore
/// use vexo::render_objects::TextEditRenderObject;
/// use std::rc::Rc;
/// use std::cell::RefCell;
///
/// let obj = TextEditRenderObject::new("Hello", editor_rc)
///     .with_font_size(16.0);
/// ```
pub struct TextEditRenderObject {
    // Text fields (same as TextRenderObject)
    content: String,
    font_size: f32,
    style: Style,
    layout: Layout,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,

    // Cursor fields
    editor: Rc<RefCell<Editor>>,
    is_focused: bool,
    cursor_blink_visible: bool,
}

impl TextEditRenderObject {
    /// Create a new text edit render object.
    pub fn new(content: &str, editor: Rc<RefCell<Editor>>) -> Self {
        Self {
            content: content.to_string(),
            font_size: 16.0,
            style: Style::default(),
            layout: Layout::default(),
            computed_bounds: None,
            layout_node: None,
            editor,
            is_focused: false,
            cursor_blink_visible: false,
        }
    }

    /// Set the font size (builder pattern).
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Set the style (builder pattern).
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the layout (builder pattern).
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

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    /// Whether the widget is focused.
    pub fn is_focused(&self) -> bool {
        self.is_focused
    }

    /// Whether the cursor blink is currently visible.
    pub fn cursor_blink_visible(&self) -> bool {
        self.cursor_blink_visible
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

    /// Set whether the widget is focused.
    ///
    /// Returns true if the focus state changed.
    pub fn set_focused(&mut self, focused: bool) -> bool {
        let changed = self.is_focused != focused;
        if changed {
            self.is_focused = focused;
        }
        changed
    }

    /// Set whether the cursor blink is currently visible.
    ///
    /// Returns true if the blink visibility changed.
    pub fn set_cursor_blink_visible(&mut self, visible: bool) -> bool {
        let changed = self.cursor_blink_visible != visible;
        if changed {
            self.cursor_blink_visible = visible;
        }
        changed
    }
}

impl RenderObject for TextEditRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let measure_ctx = MeasureContext::Text(TextMeasureContext {
            content: self.content.clone(),
            font_size: self.font_size,
            line_height: DEFAULT_LINE_HEIGHT_MULTIPLIER,
            font_family: None,
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

                // Sync the editor buffer width to the computed layout width.
                // Editor auto-maintains this constraint across all subsequent
                // mutations, so cursor_position() stays correct for wrapped text.
                let width = computed.bounds.width();
                if width > 0.0 {
                    self.editor
                        .borrow_mut()
                        .set_layout_width(ctx.font_system(), width);
                }
            }
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        let bounds = match &self.computed_bounds {
            Some(b) => b,
            None => return vec![],
        };

        let mut commands = Vec::new();

        let pos: Position<Logical, Absolute> = ctx.absolute_position();

        // Compute vertical centering offset when the layout box is taller
        // than the text's intrinsic height.
        let text_height = {
            let editor = self.editor.borrow();
            let mut h = 0.0f32;
            for run in editor.buffer().layout_runs() {
                h = h.max(run.line_top + run.line_height);
            }
            if h == 0.0 {
                self.font_size * DEFAULT_LINE_HEIGHT_MULTIPLIER
            } else {
                h
            }
        };
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

        // 4.5. Draw selection highlight (behind text, above background/border).
        //
        // The algorithm mirrors cosmic_text::Editor::render's selection pass:
        // for each layout run overlapping the selection, walk the glyphs and
        // accumulate the x-range that intersects the selection on that line,
        // then emit one RenderCommand::Rect per line.
        //
        // Coordinate space: glyph.x/w and run.line_top/line_height are in the
        // buffer's layout space, which (because the buffer's scale is 1.0)
        // equals logical pixels. We add pos (logical) and vertical_offset
        // (logical) the same way the caret code does below.
        {
            let editor = self.editor.borrow();
            if let Some((start, end)) = editor.selection_bounds() {
                let buffer = editor.buffer();
                // Buffer layout width — used to extend the highlight to the end
                // of the line when the selection spans multiple lines.
                let buffer_width = buffer.size().0.unwrap_or(bounds.width()) as i32;

                for run in buffer.layout_runs() {
                    let line_i = run.line_i;
                    if line_i < start.line || line_i > end.line {
                        continue;
                    }

                    let mut range_opt: Option<(i32, i32)> = None;
                    for glyph in run.glyphs {
                        let cluster = &run.text[glyph.start..glyph.end];
                        // NOTE: cosmic-text uses grapheme_indices here for precise
                        // emoji/grapheme-cluster boundaries. We use char_indices
                        // (std) to avoid a direct unicode-segmentation dependency;
                        // this is slightly less precise for multi-codepoint grapheme
                        // clusters (e.g. family emoji) but correct for all BMP text.
                        let total = cluster.char_indices().count().max(1);
                        let mut c_x = glyph.x;
                        let c_w = glyph.w / total as f32;
                        for (i, c) in cluster.char_indices() {
                            let c_start = glyph.start + i;
                            let c_end = glyph.start + i + c.len_utf8();
                            let in_sel = (start.line != line_i || c_end > start.index)
                                && (end.line != line_i || c_start < end.index);
                            if in_sel {
                                range_opt = Some(match range_opt.take() {
                                    Some((min, max)) => {
                                        (min.min(c_x as i32), max.max((c_x + c_w) as i32))
                                    }
                                    None => (c_x as i32, (c_x + c_w) as i32),
                                });
                            } else if let Some((min, max)) = range_opt.take() {
                                // Gap in the selection on this line — flush.
                                emit_selection_rect(
                                    &mut commands,
                                    pos,
                                    vertical_offset,
                                    min,
                                    max,
                                    run.line_top,
                                    run.line_height,
                                );
                            }
                            c_x += c_w;
                        }
                    }

                    // Highlight empty lines that are inside a multi-line selection.
                    if run.glyphs.is_empty() && end.line > line_i {
                        range_opt = Some((0, buffer_width));
                    }

                    if let Some((mut min, mut max)) = range_opt.take() {
                        // If the selection continues past this line, extend the
                        // highlight to the end of the line. (RTL special-casing
                        // is skipped for v1 — we always extend `max`.)
                        if end.line > line_i {
                            max = buffer_width;
                        }
                        emit_selection_rect(
                            &mut commands,
                            pos,
                            vertical_offset,
                            min,
                            max,
                            run.line_top,
                            run.line_height,
                        );
                    }
                }
            }
        }

        // 5. Emit text render command (vertically centered)
        let text_pos = Point::new(pos.x, pos.y + vertical_offset);
        commands.push(RenderCommand::Text {
            content: self.content.clone(),
            position: text_pos,
            font_size: self.font_size,
            color: Color::BLACK,
            font_family: None,
            max_width: Some(bounds.width()),
        });

        // 6. Emit cursor render command if focused and blink visible
        if self.is_focused && self.cursor_blink_visible {
            let editor = self.editor.borrow();
            if let Some((cursor_x, cursor_y)) = editor.cursor_position() {
                let line_height = editor.buffer().metrics().line_height;

                // Convert cursor position to absolute coordinates (with vertical centering)
                let abs_x = cursor_x as f32 + pos.x;
                let abs_y = cursor_y as f32 + pos.y + vertical_offset;

                commands.push(RenderCommand::Caret {
                    position: Point::new(abs_x, abs_y),
                    height: line_height,
                    color: CURSOR_COLOR,
                });
            }
        }

        commands
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

/// Emit a single selection-highlight rectangle.
///
/// `min_x`/`max_x` are in the buffer's layout space (== logical pixels, since
/// the buffer scale is 1.0). `line_top` and `line_height` are likewise in
/// buffer space. They are offset by the render object's absolute `pos` and
/// the text vertical-centering `vertical_offset` to produce absolute logical
/// coordinates — the same convention used by the caret.
fn emit_selection_rect(
    commands: &mut Vec<RenderCommand>,
    pos: Position<Logical, Absolute>,
    vertical_offset: f32,
    min_x: i32,
    max_x: i32,
    line_top: f32,
    line_height: f32,
) {
    let w = (max_x - min_x).max(0) as f32;
    if w <= 0.0 || line_height <= 0.0 {
        return;
    }
    let x = pos.x + min_x as f32;
    let y = pos.y + vertical_offset + line_top;
    commands.push(RenderCommand::Rect {
        bounds: Bounds::from_xywh(x, y, w, line_height),
        fill: SELECTION_COLOR,
        stroke: None,
        corner_radius: 0.0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutEngine, TaffyLayoutEngine};
    use glyphon::{Attrs, Edit, Shaping};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    fn create_test_editor() -> Rc<RefCell<Editor>> {
        let mut font_system = create_test_font_system();
        let metrics = glyphon::Metrics::new(16.0, 20.0);
        let mut raw_editor = glyphon::Editor::new(glyphon::Buffer::new_empty(metrics));
        raw_editor.with_buffer_mut(|buffer| {
            buffer.set_text(
                &mut font_system,
                "Hello",
                &Attrs::new(),
                Shaping::Advanced,
                None,
            );
        });
        raw_editor.with_buffer_mut(|buffer| {
            buffer.shape_until_scroll(&mut font_system, true);
        });
        Rc::new(RefCell::new(Editor::new(raw_editor)))
    }

    #[test]
    fn test_text_edit_render_object_new() {
        let editor = create_test_editor();
        let obj = TextEditRenderObject::new("Hello", editor);
        assert_eq!(obj.content(), "Hello");
        assert_eq!(obj.font_size(), 16.0); // default
        assert!(obj.computed_bounds().is_none());
        assert!(!obj.is_focused());
        assert!(!obj.cursor_blink_visible());
    }

    #[test]
    fn test_text_edit_render_object_with_font_size() {
        let editor = create_test_editor();
        let obj = TextEditRenderObject::new("Hello", editor).with_font_size(24.0);
        assert_eq!(obj.font_size(), 24.0);
    }

    #[test]
    fn test_text_edit_render_object_set_content() {
        let editor = create_test_editor();
        let mut obj = TextEditRenderObject::new("Hello", editor);
        assert!(obj.set_content("World"));
        assert_eq!(obj.content(), "World");
        assert!(!obj.set_content("World")); // No change
    }

    #[test]
    fn test_text_edit_render_object_set_font_size() {
        let editor = create_test_editor();
        let mut obj = TextEditRenderObject::new("Hello", editor);
        assert!(obj.set_font_size(32.0));
        assert_eq!(obj.font_size(), 32.0);
        assert!(!obj.set_font_size(32.0)); // No change
    }

    #[test]
    fn test_text_edit_render_object_set_focused() {
        let editor = create_test_editor();
        let mut obj = TextEditRenderObject::new("Hello", editor);
        assert!(obj.set_focused(true));
        assert!(obj.is_focused());
        assert!(!obj.set_focused(true)); // No change
        assert!(obj.set_focused(false));
        assert!(!obj.is_focused());
    }

    #[test]
    fn test_text_edit_render_object_set_cursor_blink_visible() {
        let editor = create_test_editor();
        let mut obj = TextEditRenderObject::new("Hello", editor);
        assert!(obj.set_cursor_blink_visible(true));
        assert!(obj.cursor_blink_visible());
        assert!(!obj.set_cursor_blink_visible(true)); // No change
        assert!(obj.set_cursor_blink_visible(false));
        assert!(!obj.cursor_blink_visible());
    }

    #[test]
    fn test_text_edit_render_object_layout_creates_node() {
        let editor = create_test_editor();
        let mut obj = TextEditRenderObject::new("Hello World", editor);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = obj.layout(&mut ctx, &[]);

        assert!(obj.layout_node.is_some());
        assert_eq!(obj.layout_node, Some(result.node));
    }

    #[test]
    fn test_text_edit_render_object_hit_test_no_layout() {
        let editor = create_test_editor();
        let obj = TextEditRenderObject::new("Test", editor);

        // Without layout, computed_bounds is None, so hit test should fail
        assert!(!obj.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_text_edit_render_object_paint_no_layout() {
        let editor = create_test_editor();
        let obj = TextEditRenderObject::new("Test", editor);

        // Paint returns empty without layout (computed_bounds is None)
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert!(result.is_empty());
    }

    #[test]
    fn test_text_edit_render_object_paint_no_cursor_when_not_focused() {
        let editor = create_test_editor();
        let mut obj = TextEditRenderObject::new("Hello", editor);
        obj.set_focused(false);
        obj.set_cursor_blink_visible(true);

        // Full layout: create node, compute, apply
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        {
            let mut layout_ctx = LayoutContext::new(&mut engine, &mut font_system);
            let _ = obj.layout(&mut layout_ctx, &[]);
        }
        let root = engine.create_leaf(&Layout::default());
        engine.compute(root, Size::new(200.0, 50.0), &mut font_system);
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            obj.apply_layout(&mut ctx);
        }

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);
        // Should only have Text command, no Caret
        let caret_count = result
            .iter()
            .filter(|c| matches!(c, RenderCommand::Caret { .. }))
            .count();
        assert_eq!(caret_count, 0, "Should not emit Caret when not focused");
        assert!(
            result
                .iter()
                .any(|c| matches!(c, RenderCommand::Text { .. })),
            "Should emit Text"
        );
    }

    #[test]
    fn test_text_edit_render_object_paint_no_cursor_when_blink_off() {
        let editor = create_test_editor();
        let mut obj = TextEditRenderObject::new("Hello", editor);
        obj.set_focused(true);
        obj.set_cursor_blink_visible(false);

        // Full layout: create node, compute, apply
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        {
            let mut layout_ctx = LayoutContext::new(&mut engine, &mut font_system);
            let _ = obj.layout(&mut layout_ctx, &[]);
        }
        let root = engine.create_leaf(&Layout::default());
        engine.compute(root, Size::new(200.0, 50.0), &mut font_system);
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            obj.apply_layout(&mut ctx);
        }

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);
        // Should only have Text command, no Caret
        let caret_count = result
            .iter()
            .filter(|c| matches!(c, RenderCommand::Caret { .. }))
            .count();
        assert_eq!(
            caret_count, 0,
            "Should not emit Caret when blink not visible"
        );
        assert!(
            result
                .iter()
                .any(|c| matches!(c, RenderCommand::Text { .. })),
            "Should emit Text"
        );
    }

    #[test]
    fn test_text_edit_render_object_paint_with_cursor() {
        let editor = create_test_editor();
        let mut obj = TextEditRenderObject::new("Hello", editor);
        obj.set_focused(true);
        obj.set_cursor_blink_visible(true);

        // Full layout: create node, compute, apply
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        {
            let mut layout_ctx = LayoutContext::new(&mut engine, &mut font_system);
            let _ = obj.layout(&mut layout_ctx, &[]);
        }
        let root = engine.create_leaf(&Layout::default());
        engine.compute(root, Size::new(200.0, 50.0), &mut font_system);
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            obj.apply_layout(&mut ctx);
        }

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);
        // Should have Text + Caret commands
        assert!(
            result
                .iter()
                .any(|c| matches!(c, RenderCommand::Text { .. })),
            "Should emit Text"
        );
        assert!(
            result
                .iter()
                .any(|c| matches!(c, RenderCommand::Caret { .. })),
            "Should emit Caret when focused and blink visible"
        );

        // Verify the Caret command
        if let Some(RenderCommand::Caret { color, .. }) = result
            .iter()
            .find(|c| matches!(c, RenderCommand::Caret { .. }))
        {
            assert_eq!(*color, CURSOR_COLOR);
        } else {
            panic!("Expected Caret command");
        }
    }

    #[test]
    fn test_text_edit_render_object_set_style_change_detection() {
        let editor = create_test_editor();
        let style1 = crate::Style::new().background(crate::core::Color::WHITE);
        let style2 = crate::Style::new().background(crate::core::Color::BLUE);
        let style2_dup = style2.clone();
        let mut ro = TextEditRenderObject::new("Hello", editor).with_style(style1);
        assert!(ro.set_style(style2));
        assert!(!ro.set_style(style2_dup));
    }

    #[test]
    fn test_text_edit_render_object_set_layout_change_detection() {
        let editor = create_test_editor();
        let layout1 = Layout::default().padding(8.0);
        let layout2 = Layout::default().padding(16.0);
        let layout2_dup = layout2.clone();
        let mut ro = TextEditRenderObject::new("Hello", editor).with_layout(layout1);
        assert!(ro.set_layout(layout2));
        assert!(!ro.set_layout(layout2_dup));
    }
}
