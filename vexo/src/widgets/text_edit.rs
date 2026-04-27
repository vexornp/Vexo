use crate::core::{Bounds, Color, Logical, Point, Stroke};
use crate::layout::{Layout, LayoutContext, LayoutNodeId, LayoutView};
use crate::renderer::UiBatcher;
use crate::render::RenderCommand;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::input::{CursorIcon, InputEvent, ButtonState, Key, NamedKey};
use glyphon::{cosmic_text::Motion, Action, SwashCache};

pub struct TextEdit {
    pub editor_id: String,
    pub initial_text: String,
    pub swash_cache: SwashCache,
    pub text_color: Color,
    pub cursor_color: Color,
    pub key: Option<String>,
    pub layout: Layout,
    /// Stored computed layout from the layout phase.
    computed_layout: Option<crate::testable::ComputedLayout>,
}

impl TextEdit {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            editor_id: id.into(),
            initial_text: String::new(),
            swash_cache: SwashCache::new(),
            text_color: Color::WHITE,
            cursor_color: Color::new(0.3, 0.67, 0.97, 1.0), // Accent blue
            key: None,
            layout: Layout::default(),
            computed_layout: None,
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set initial/placeholder text content.
    pub fn content(mut self, text: impl Into<String>) -> Self {
        self.initial_text = text.into();
        self
    }

    pub fn with_cursor_color(mut self, color: Color) -> Self {
        self.cursor_color = color;
        self
    }

    /// Set fixed width.
    pub fn width(mut self, value: f32) -> Self {
        self.layout = self.layout.width(value);
        self
    }

    /// Set fixed height.
    pub fn height(mut self, value: f32) -> Self {
        self.layout = self.layout.height(value);
        self
    }

    /// Set uniform padding on all sides.
    pub fn padding(mut self, value: f32) -> Self {
        self.layout = self.layout.padding(value);
        self
    }

    /// Set uniform margin on all sides.
    pub fn margin(mut self, value: f32) -> Self {
        self.layout = self.layout.margin(value);
        self
    }

    /// Set flex grow factor.
    pub fn flex_grow(mut self, value: f32) -> Self {
        self.layout = self.layout.flex_grow(value);
        self
    }

    /// Set the entire Layout struct.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
}

// ============================================================================
// SEPARATED TRAIT IMPLEMENTATIONS
// ============================================================================

impl crate::testable::Identifiable for TextEdit {
    fn id(&self) -> Option<WidgetId> {
        Some(WidgetId::from_key(&self.editor_id))
    }
}

impl crate::testable::Layout for TextEdit {
    fn constraints(&self) -> crate::testable::LayoutConstraints {
        let mut constraints = crate::testable::LayoutConstraints::from_layout(&self.layout);
        // Default to flex_grow: 1.0 if no sizing is specified
        if self.layout.flex_grow.is_none() && self.layout.width.is_none() && self.layout.height.is_none() {
            constraints.flex_grow = 1.0;
        }
        constraints
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
    }
}

impl crate::testable::Paint for TextEdit {
    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        let layout = match &self.computed_layout {
            Some(l) => l,
            None => return Vec::new(),
        };

        let bounds = Bounds::from_xywh(
            ctx.offset().x + layout.x(),
            ctx.offset().y + layout.y(),
            layout.width(),
            layout.height(),
        );

        let mut commands = Vec::new();

        // Debug border
        commands.push(RenderCommand::rect_with_border(
            bounds,
            Color::BLACK,
            Color::RED,
            1.0,
        ));

        // Editor area
        commands.push(RenderCommand::editor(
            self.editor_id.clone(),
            bounds,
        ));

        // Note: Cursor rendering requires access to editor state (via WidgetContext)
        // which is not available in PaintContext. This is a known limitation.
        // The legacy Widget::draw method handles cursor rendering for now.

        commands
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::testable::Interact<M> for TextEdit {
    fn on_event(
        &mut self,
        event: &InputEvent,
        ctx: &crate::testable::InteractionContext,
    ) -> crate::testable::InteractionResponse<M> {
        let my_id = WidgetId::from_key(&self.editor_id);
        let is_focused = ctx.is_focused(my_id);

        // Check for click to grab/retain focus
        if let InputEvent::PointerButton {
            state: ButtonState::Pressed,
            ..
        } = event
        {
            if ctx.is_pointer_inside() {
                return crate::testable::InteractionResponse {
                    cursor: Some(CursorIcon::Text),
                    ..crate::testable::InteractionResponse::request_focus(my_id)
                };
            }
        }

        if !is_focused {
            // Return text cursor if pointer is inside bounds (for hover)
            let cursor = if ctx.is_pointer_inside() {
                Some(CursorIcon::Text)
            } else {
                None
            };
            return crate::testable::InteractionResponse {
                cursor,
                ..crate::testable::InteractionResponse::default()
            };
        }

        // Note: Full keyboard handling requires access to editor state and font_system
        // (via WidgetContext) which is not available in InteractionContext.
        // This is a known limitation. The legacy Widget::on_event method handles
        // keyboard input for now.

        // Mark as handled if focused (for focus retention)
        crate::testable::InteractionResponse {
            cursor: Some(CursorIcon::Text),
            ..crate::testable::InteractionResponse::handled()
        }
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for TextEdit {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Text
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeId {
        // Use Layout properties, defaulting to flex_grow: 1.0 if not specified
        let layout = if self.layout.flex_grow.is_none() && self.layout.width.is_none() && self.layout.height.is_none() {
            Layout::default().flex_grow(1.0)
        } else if self.layout.flex_grow.is_none() {
            self.layout.clone()
        } else {
            self.layout.clone()
        };

        layout_context.create_leaf(&layout)
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
    }

    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        crate::testable::Paint::paint(self, ctx)
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        widget_context: &mut WidgetContext,
    ) {
        if let Some(layout) = layout_view.get_layout(node) {
            let bounds = Bounds::from_xywh(
                offset.x + layout.x(),
                offset.y + layout.y(),
                layout.width(),
                layout.height(),
            );

            // Debug border
            let debug_color = crate::Color::RED;
            renderer.add_rect(bounds, crate::Color::BLACK, Some(Stroke::with_color(debug_color)), 0.0);

            let editor_arc = widget_context.get_or_create_editor(&self.editor_id, &self.initial_text);
            let mut editor_ref = editor_arc.borrow_mut();

            editor_ref.set_size(&mut widget_context.font_system, bounds.size());
            editor_ref.shape_as_needed(&mut widget_context.font_system, true);

            renderer.add_editor_request(
                &self.editor_id,
                bounds,
            );

            // Render cursor if focused and visible
            let my_id = WidgetId::from_key(&self.editor_id);
            let is_focused = focused_id == Some(my_id);

            if is_focused && cursor_blink.is_visible() {
                // Get cursor position from the editor
                if let Some((cursor_x, cursor_y)) = editor_ref.cursor_position() {
                    // cursor_position returns coordinates relative to the buffer
                    // Convert to absolute position within the widget
                    let abs_cursor_x = bounds.left + cursor_x as f32;
                    let abs_cursor_y = bounds.top + cursor_y as f32;

                    // Get line height from the buffer metrics
                    let buffer = editor_ref.buffer();
                    let line_height = buffer.metrics().line_height;

                    // Draw vertical bar cursor (2 logical pixels wide)
                    let cursor_width = 2.0;
                    let cursor_height = line_height;

                    let cursor_bounds = Bounds::from_xywh(abs_cursor_x, abs_cursor_y, cursor_width, cursor_height);
                    renderer.add_rect(
                        cursor_bounds,
                        self.cursor_color,
                        None, // No border
                        0.0, // No corner radius
                    );
                }
            }
        }
    }

    fn on_event(
        &mut self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_context: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        // Derive our WidgetId from the editor_id (explicit key)
        let my_id = WidgetId::from_key(&self.editor_id);
        let is_focused = focused_id == Some(my_id);

        // Helper to check if position is inside our bounds
        let bounds_check = |position: &Point<Logical>| -> bool {
            if let Some(layout) = layout_view.get_layout(node) {
                let abs_x = offset.x + layout.x();
                let abs_y = offset.y + layout.y();
                let bounds = Bounds::from_xywh(abs_x, abs_y, layout.width(), layout.height());
                bounds.contains(position)
            } else {
                false
            }
        };

        // Handle PointerMoved - return text cursor when hovering
        if let InputEvent::PointerMoved { position } = event {
            if bounds_check(position) {
                return WidgetResponse {
                    message: None,
                    focus_request: None,
                    handled: false,
                    clear_focus: false,
                    cursor: Some(CursorIcon::Text),
                };
            }
            return WidgetResponse::<M>::default();
        }

        if !is_focused {
            // Check for click to grab focus
            if let InputEvent::PointerButton {
                state: ButtonState::Pressed,
                position,
                ..
            } = event
            {
                if bounds_check(position) {
                    return WidgetResponse {
                        message: None,
                        focus_request: Some(my_id),
                        handled: true,
                        clear_focus: false,
                        cursor: None,
                    };
                }
            }
            return WidgetResponse::default();
        }

        // We are focused, so handle keyboard input
        let editor_rc = widget_context.get_or_create_editor(&self.editor_id, &self.initial_text);
        let mut editor_ref = editor_rc.borrow_mut();

        match event {
            InputEvent::PointerButton {
                state: ButtonState::Pressed,
                position,
                ..
            } => {
                if bounds_check(position) {
                    // Move cursor to click position
                    if let Some(layout) = layout_view.get_layout(node) {
                        let widget_origin = Point::<Logical>::new(
                            offset.x + layout.x(),
                            offset.y + layout.y(),
                        );
                        let relative = *position - widget_origin;
                        let physical = relative.to_physical(widget_context.scale);

                        let buffer = editor_ref.buffer();
                        if let Some(cursor) = buffer.hit(physical.x, physical.y) {
                            editor_ref.set_cursor(cursor);
                        }
                    }

                    return WidgetResponse {
                        message: None,
                        focus_request: Some(my_id),
                        handled: true,
                        clear_focus: false,
                        cursor: None,
                    };
                }
                // Click outside - don't handle, let framework clear focus
                return WidgetResponse::default();
            }
            InputEvent::Keyboard {
                key,
                state: ButtonState::Pressed,
                text,
                modifiers,
            } => {
                let ctrl_pressed = modifiers.control;

                match key {
                    Key::Named(NamedKey::ArrowLeft) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Motion(Motion::Left));
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Motion(Motion::Right));
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Motion(Motion::Up));
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Motion(Motion::Down));
                    }
                    Key::Named(NamedKey::Home) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Motion(Motion::Home));
                    }
                    Key::Named(NamedKey::End) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Motion(Motion::End));
                    }
                    Key::Named(NamedKey::PageUp) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Motion(Motion::PageUp));
                    }
                    Key::Named(NamedKey::PageDown) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Motion(Motion::PageDown));
                    }
                    Key::Named(NamedKey::Escape) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Escape);
                    }
                    Key::Named(NamedKey::Enter) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Enter);
                    }
                    Key::Named(NamedKey::Backspace) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Backspace);
                    }
                    Key::Named(NamedKey::Delete) => {
                        editor_ref.action(&mut widget_context.font_system, Action::Delete);
                    }
                    Key::Character(ch) => {
                        if ctrl_pressed {
                            // Handle Ctrl + Char
                            match ch.as_str() {
                                "c" => {
                                    // TODO: Copy
                                }
                                "v" => {
                                    // TODO: Paste
                                }
                                "x" => {
                                    // TODO: Cut
                                }
                                _ => {
                                    // Ignore other Ctrl + Char combinations
                                }
                            }
                        } else {
                            // Normal character input - use the text field if available
                            if let Some(text) = text {
                                for c in text.chars() {
                                    if c.is_control() {
                                        // Ignore control characters
                                        continue;
                                    }
                                    editor_ref.action(&mut widget_context.font_system, Action::Insert(c));
                                }
                            }
                        }
                    }
                    _ => {
                        // Ignore other keys
                    }
                }
            }
            _ => {}
        }

        editor_ref.shape_as_needed(&mut widget_context.font_system, true);

        // For keyboard events, don't set cursor - cursor is based on hover position, not focus
        WidgetResponse {
            message: None,
            focus_request: None,
            handled: true,
            clear_focus: false,
            cursor: None,
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testable::{Identifiable, Layout, Paint, Interact, InteractionContext};

    #[test]
    fn test_text_edit_implements_separated_traits() {
        let text_edit = TextEdit::new("test-editor");

        // Should implement Identifiable
        let id = text_edit.id();
        assert!(id.is_some());
        assert_eq!(id.unwrap(), WidgetId::from_key("test-editor"));
    }

    #[test]
    fn test_text_edit_layout_constraints_default_flex_grow() {
        let text_edit = TextEdit::new("test-editor");
        let constraints = text_edit.constraints();

        // Default should have flex_grow: 1.0
        assert_eq!(constraints.flex_grow, 1.0);
    }

    #[test]
    fn test_text_edit_layout_constraints_with_fixed_size() {
        let text_edit = TextEdit::new("test-editor")
            .width(200.0)
            .height(50.0);
        let constraints = text_edit.constraints();

        // With fixed size, flex_grow should be 0
        assert_eq!(constraints.flex_grow, 0.0);
        assert!(constraints.is_fixed_width());
        assert!(constraints.is_fixed_height());
    }

    #[test]
    fn test_text_edit_apply_layout() {
        let mut text_edit = TextEdit::new("test-editor");

        assert!(text_edit.computed_layout.is_none());

        let layout = crate::testable::ComputedLayout::new(
            crate::core::Bounds::from_xywh(10.0, 20.0, 200.0, 50.0)
        );
        crate::testable::Layout::apply_layout(&mut text_edit, layout);

        assert!(text_edit.computed_layout.is_some());
        let stored = text_edit.computed_layout.unwrap();
        assert_eq!(stored.x(), 10.0);
        assert_eq!(stored.y(), 20.0);
        assert_eq!(stored.width(), 200.0);
        assert_eq!(stored.height(), 50.0);
    }

    #[test]
    fn test_text_edit_paint_returns_commands() {
        let mut text_edit = TextEdit::new("test-editor");

        // Without computed layout, should return empty
        let mut ctx = crate::testable::PaintContext::default();
        let commands = crate::testable::Paint::paint(&text_edit, &mut ctx);
        assert!(commands.is_empty());

        // With computed layout, should return commands
        crate::testable::Layout::apply_layout(
            &mut text_edit,
            crate::testable::ComputedLayout::new(
                crate::core::Bounds::from_xywh(0.0, 0.0, 200.0, 50.0)
            )
        );

        let commands = crate::testable::Paint::paint(&text_edit, &mut ctx);
        assert_eq!(commands.len(), 2); // border rect + editor command
    }

    #[test]
    fn test_text_edit_interact_click_requests_focus() {
        let mut text_edit = TextEdit::new("test-editor");

        // Create context with pointer inside bounds
        let ctx = InteractionContext::new(
            Point::new(50.0, 25.0),
            None,
            Bounds::from_xywh(0.0, 0.0, 100.0, 50.0),
            crate::core::Scale::new(1.0),
        );

        // Click inside should request focus
        let event = InputEvent::PointerButton {
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Pressed,
            position: Point::new(50.0, 25.0),
        };

        let response: crate::testable::InteractionResponse<()> =
            Interact::on_event(&mut text_edit, &event, &ctx);
        assert!(response.handled);
        assert!(response.focus_request.is_some());
    }

    #[test]
    fn test_text_edit_interact_outside_click_ignored() {
        let mut text_edit = TextEdit::new("test-editor");

        // Create context with pointer outside bounds
        let ctx = InteractionContext::new(
            Point::new(150.0, 25.0), // Outside the bounds
            None,
            Bounds::from_xywh(0.0, 0.0, 100.0, 50.0),
            crate::core::Scale::new(1.0),
        );

        let event = InputEvent::PointerButton {
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Pressed,
            position: Point::new(150.0, 25.0),
        };

        let response: crate::testable::InteractionResponse<()> =
            Interact::on_event(&mut text_edit, &event, &ctx);
        assert!(!response.handled);
    }
}