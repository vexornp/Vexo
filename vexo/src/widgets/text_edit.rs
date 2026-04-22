use crate::core::{Logical, Point, Rect, Size};
use crate::layout::{Layout, LayoutContext, LayoutNodeId, LayoutView};
use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::Color;
use crate::input::{InputEvent, ButtonState, Key, NamedKey};
use glyphon::{cosmic_text::Motion, Action, SwashCache};

pub struct TextEdit {
    pub editor_id: String,
    pub initial_text: String,
    pub swash_cache: SwashCache,
    pub text_color: Color,
    pub cursor_color: Color,
    pub key: Option<String>,
    pub layout: Layout,
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

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for TextEdit {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
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
            let pos: Point<Logical> = Point::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );
            let size: Size<Logical> = Size::new(layout.width(), layout.height());

            // Debug border
            let debug_color = crate::Color::RED;
            renderer.add_rect(pos.to_array(), size.to_array(), crate::Color::BLACK, debug_color, 1.0, 0.0);

            let editor_arc = widget_context.get_or_create_editor(&self.editor_id, &self.initial_text);
            let mut editor_ref = editor_arc.borrow_mut();

            editor_ref.set_size(&mut widget_context.font_system, size);
            editor_ref.shape_as_needed(&mut widget_context.font_system, true);

            renderer.add_editor_request(
                &self.editor_id,
                Rect::new(pos, size),
            );

            // Render cursor if focused and visible
            let my_id = WidgetId::from_key(&self.editor_id);
            let is_focused = focused_id == Some(my_id);

            if is_focused && cursor_blink.is_visible() {
                // Get cursor position from the editor
                if let Some((cursor_x, cursor_y)) = editor_ref.cursor_position() {
                    // cursor_position returns coordinates relative to the buffer
                    // Convert to absolute position within the widget
                    let abs_cursor_x = pos.x + cursor_x as f32;
                    let abs_cursor_y = pos.y + cursor_y as f32;

                    // Get line height from the buffer metrics
                    let buffer = editor_ref.buffer();
                    let line_height = buffer.metrics().line_height;

                    // Draw vertical bar cursor (2 logical pixels wide)
                    let cursor_width = 2.0;
                    let cursor_height = line_height;

                    renderer.add_rect(
                        [abs_cursor_x, abs_cursor_y],
                        [cursor_width, cursor_height],
                        self.cursor_color,
                        crate::Color::TRANSPARENT, // No border
                        0.0, // No border width
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

        if !is_focused {
            // Check for click to grab focus
            if let InputEvent::PointerButton {
                state: ButtonState::Pressed,
                position,
                ..
            } = event
            {
                if let Some(layout) = layout_view.get_layout(node) {
                    // Add offset to get absolute position
                    let abs_x = offset.x + layout.x();
                    let abs_y = offset.y + layout.y();
                    let rect = Rect::from_xywh(
                        abs_x,
                        abs_y,
                        layout.width(),
                        layout.height(),
                    );

                    if rect.contains(position) {
                        return WidgetResponse {
                            message: None,
                            focus_request: Some(my_id),
                            handled: true,
                            clear_focus: false,
                        };
                    }
                }
            }
            return WidgetResponse::default();
        }

        // We are focused, so handle keyboard input
        let editor_rc = widget_context.get_or_create_editor(&self.editor_id, &self.initial_text);
        let mut editor_ref = editor_rc.borrow_mut();

        match event {
            InputEvent::ModifiersChanged { modifiers } => {
                // Store modifiers for later use if needed
                let _ctrl_pressed = modifiers.control;
            }
            InputEvent::PointerButton {
                state: ButtonState::Pressed,
                position,
                ..
            } => {
                // Check if click is inside our bounds
                if let Some(layout) = layout_view.get_layout(node) {
                    let abs_x = offset.x + layout.x();
                    let abs_y = offset.y + layout.y();
                    let rect = Rect::from_xywh(
                        abs_x,
                        abs_y,
                        layout.width(),
                        layout.height(),
                    );

                    if rect.contains(position) {
                        // Click inside - retain focus
                        return WidgetResponse {
                            message: None,
                            focus_request: Some(my_id),
                            handled: true,
                            clear_focus: false,
                        };
                    }
                    // Click outside - don't handle, let framework clear focus
                    return WidgetResponse::default();
                }
            }
            InputEvent::Keyboard {
                key,
                state: ButtonState::Pressed,
                text,
                modifiers,
                ..
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

        WidgetResponse {
            message: None,
            focus_request: None,
            handled: true,
            clear_focus: false,
        }
    }
}