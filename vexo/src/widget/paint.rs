//! Paint trait for widget rendering.

use crate::core::{Point, Rect, WidgetId, Logical};
use crate::render::RenderCommand;

/// Context provided to widgets during the paint phase.
///
/// PaintContext provides information about the current rendering state
/// and accumulates render commands produced by widgets.
#[derive(Debug)]
pub struct PaintContext {
    /// Current offset for child positioning.
    offset: Point<Logical>,
    /// Stack of clipping regions.
    clip_stack: Vec<Rect<Logical>>,
    /// Stack of corner radius contexts.
    corner_radius_stack: Vec<f32>,
    /// Currently focused widget (if any).
    focused_widget: Option<WidgetId>,
    /// Whether the cursor is currently visible (for blinking).
    cursor_blink_visible: bool,
    /// Accumulated render commands.
    commands: Vec<RenderCommand>,
}

impl Default for PaintContext {
    fn default() -> Self {
        Self {
            offset: Point::new(0.0, 0.0),
            clip_stack: Vec::new(),
            corner_radius_stack: Vec::new(),
            focused_widget: None,
            cursor_blink_visible: true,
            commands: Vec::new(),
        }
    }
}

impl PaintContext {
    /// Create a new paint context.
    pub fn new(focused_widget: Option<WidgetId>, cursor_blink_visible: bool) -> Self {
        Self {
            focused_widget,
            cursor_blink_visible,
            ..Self::default()
        }
    }

    /// Add a render command.
    pub fn add(&mut self, command: RenderCommand) {
        self.commands.push(command);
    }

    /// Add multiple render commands.
    pub fn extend(&mut self, commands: Vec<RenderCommand>) {
        self.commands.extend(commands);
    }

    /// Get the current offset.
    pub fn offset(&self) -> Point<Logical> {
        self.offset
    }

    /// Push a new offset onto the stack.
    pub fn push_offset(&mut self, offset: Point<Logical>) {
        self.offset = self.offset + offset;
    }

    /// Pop the offset from the stack.
    pub fn pop_offset(&mut self, previous: Point<Logical>) {
        self.offset = previous;
    }

    /// Check if a widget is currently focused.
    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.focused_widget == Some(id)
    }

    /// Check if cursor blink is visible.
    pub fn is_cursor_visible(&self) -> bool {
        self.cursor_blink_visible
    }

    /// Get the current corner radius from the context stack.
    pub fn current_corner_radius(&self) -> f32 {
        self.corner_radius_stack.last().copied().unwrap_or(0.0)
    }

    /// Push a corner radius onto the context stack.
    pub fn push_corner_radius(&mut self, radius: f32) {
        self.corner_radius_stack.push(radius);
    }

    /// Pop a corner radius from the context stack.
    pub fn pop_corner_radius(&mut self) {
        self.corner_radius_stack.pop();
    }

    /// Push a clipping region.
    pub fn push_clip(&mut self, bounds: Rect<Logical>) {
        self.clip_stack.push(bounds);
        self.commands.push(RenderCommand::PushClip { bounds });
    }

    /// Pop a clipping region.
    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
        self.commands.push(RenderCommand::PopClip);
    }

    /// Consume the context and return the accumulated commands.
    pub fn into_commands(self) -> Vec<RenderCommand> {
        self.commands
    }

    /// Get a reference to the accumulated commands.
    pub fn commands(&self) -> &[RenderCommand] {
        &self.commands
    }

    /// Clear all accumulated commands.
    pub fn clear(&mut self) {
        self.commands.clear();
    }
}

/// Trait for widgets that render visual content.
///
/// Widgets implement `paint` to generate render commands that describe
/// what should be drawn. The commands are collected and submitted to
/// the rendering backend.
///
/// # Example
///
/// ```
/// use vexo::widget::{Paint, PaintContext};
/// use vexo::render::RenderCommand;
/// use vexo::core::{Color, Rect, Logical};
///
/// struct RedRect;
///
/// impl Paint for RedRect {
///     fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
///         vec![RenderCommand::rect(
///             Rect::<Logical>::from_xywh(0.0, 0.0, 100.0, 100.0),
///             Color::RED,
///         )]
///     }
/// }
/// ```
pub trait Paint {
    /// Generate render commands for this widget.
    ///
    /// Called during the paint phase after layout has been computed.
    /// Widgets should use their stored `ComputedLayout` to determine
    /// position and size.
    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand>;
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paint_context_default() {
        let ctx = PaintContext::default();
        assert!(ctx.commands().is_empty());
        assert_eq!(ctx.current_corner_radius(), 0.0);
    }

    #[test]
    fn test_paint_context_add_command() {
        let mut ctx = PaintContext::default();
        ctx.add(RenderCommand::rect(
            Rect::from_xywh(0.0, 0.0, 100.0, 100.0),
            crate::core::Color::RED,
        ));

        assert_eq!(ctx.commands().len(), 1);
    }

    #[test]
    fn test_paint_context_corner_radius() {
        let mut ctx = PaintContext::default();
        assert_eq!(ctx.current_corner_radius(), 0.0);

        ctx.push_corner_radius(10.0);
        assert_eq!(ctx.current_corner_radius(), 10.0);

        ctx.push_corner_radius(20.0);
        assert_eq!(ctx.current_corner_radius(), 20.0);

        ctx.pop_corner_radius();
        assert_eq!(ctx.current_corner_radius(), 10.0);
    }

    #[test]
    fn test_paint_context_focus() {
        let id = WidgetId::from_key("test");
        let ctx = PaintContext::new(Some(id), true);

        assert!(ctx.is_focused(id));
        assert!(!ctx.is_focused(WidgetId::from_key("other")));
        assert!(ctx.is_cursor_visible());
    }

    #[test]
    fn test_paint_context_into_commands() {
        let mut ctx = PaintContext::default();
        ctx.add(RenderCommand::rect(
            Rect::from_xywh(0.0, 0.0, 100.0, 100.0),
            crate::core::Color::RED,
        ));

        let commands = ctx.into_commands();
        assert_eq!(commands.len(), 1);
    }
}
