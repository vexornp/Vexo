use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::Color;
use glyphon::{cosmic_text::Motion, Action, SwashCache};
use taffy::prelude::NodeId;
use taffy::Style;
use winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    keyboard::{Key, NamedKey},
};

pub struct TextEdit {
    pub editor_id: String,
    pub initial_text: String,
    pub swash_cache: SwashCache,
    pub text_color: Color,
    pub cursor_color: Color,
    pub key: Option<String>,
}

impl TextEdit {
    pub fn new(id: impl Into<String>, initial_text: impl Into<String>) -> Self {
        Self {
            editor_id: id.into(),
            initial_text: initial_text.into(),
            swash_cache: SwashCache::new(),
            text_color: Color::WHITE,
            cursor_color: Color::new(0.3, 0.67, 0.97, 1.0), // Accent blue
            key: None,
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn with_cursor_color(mut self, color: Color) -> Self {
        self.cursor_color = color;
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for TextEdit {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        // TextEdit has no intrinsic size - use flex_grow to fill available space
        taffy
            .new_leaf(Style {
                flex_grow: 1.0,
                ..Default::default()
            })
            .unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
        use crate::utils::{Logical, Point, Rect, Size};

        let layout = taffy.layout(node).unwrap();
        let pos: Point<Logical> = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );
        let size: Size<Logical> = Size::new(layout.size.width, layout.size.height);

        // Debug border
        let debug_color = crate::Color::RED;
        renderer.add_rect(pos.to_array(), size.to_array(), crate::Color::BLACK, debug_color, 1.0, 0.0);

        let editor_arc = ctx.get_or_create_editor(&self.editor_id, &self.initial_text);
        let mut editor_ref = editor_arc.borrow_mut();

        editor_ref.set_size(&mut ctx.font_system, size.width, size.height);
        editor_ref.shape_as_needed(&mut ctx.font_system, true);

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

    fn on_event(
        &mut self,
        _taffy: &taffy::TaffyTree,
        _node: NodeId,
        _offset: crate::utils::Point<crate::utils::Logical>,
        _event: &winit::event::WindowEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        // Derive our WidgetId from the editor_id (explicit key)
        let my_id = WidgetId::from_key(&self.editor_id);
        let is_focused = focused_id == Some(my_id);

        if !is_focused {
            // Check for click to grab focus
            if let WindowEvent::PointerButton {
                state: winit::event::ElementState::Pressed,
                ..
            } = _event
            {
                let layout = _taffy.layout(_node).unwrap();
                // Add offset to get absolute position
                let abs_x = _offset.x + layout.location.x;
                let abs_y = _offset.y + layout.location.y;
                let rect = crate::utils::Rect::from_xywh(
                    abs_x,
                    abs_y,
                    layout.size.width,
                    layout.size.height,
                );

                let logical_pos = ctx.cursor_pos.to_logical(ctx.scale.factor());
                if rect.contains(&logical_pos) {
                    return WidgetResponse {
                        message: None,
                        focus_request: Some(my_id),
                        handled: true,
                        clear_focus: false,
                    };
                }
            }
            return WidgetResponse::default();
        }

        // We are focused, so handle keyboard input
        let editor_rc = ctx.get_or_create_editor(&self.editor_id, &self.initial_text);
        let mut editor_ref = editor_rc.borrow_mut();

        let mut _ctrl_pressed = false;
        let mut _mouse_x: f64 = 0.0;
        let mut _mouse_y: f64 = 0.0;
        let _mouse_left = ElementState::Released;

        match _event {
            WindowEvent::ModifiersChanged(modifiers) => {
                _ctrl_pressed = modifiers.state().control_key();
            }
            WindowEvent::PointerButton {
                state: winit::event::ElementState::Pressed,
                ..
            } => {
                // Check if click is inside our bounds
                let layout = _taffy.layout(_node).unwrap();
                let abs_x = _offset.x + layout.location.x;
                let abs_y = _offset.y + layout.location.y;
                let rect = crate::utils::Rect::from_xywh(
                    abs_x,
                    abs_y,
                    layout.size.width,
                    layout.size.height,
                );

                let logical_pos = ctx.cursor_pos.to_logical(ctx.scale.factor());
                if rect.contains(&logical_pos) {
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
            WindowEvent::KeyboardInput { event, .. } => {
                let KeyEvent {
                    logical_key, state, ..
                } = event;

                if state.is_pressed() {
                    match logical_key {
                        Key::Named(NamedKey::ArrowLeft) => {
                            editor_ref.action(&mut ctx.font_system, Action::Motion(Motion::Left));
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            editor_ref.action(&mut ctx.font_system, Action::Motion(Motion::Right));
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            editor_ref.action(&mut ctx.font_system, Action::Motion(Motion::Up));
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            editor_ref.action(&mut ctx.font_system, Action::Motion(Motion::Down));
                        }
                        Key::Named(NamedKey::Home) => {
                            editor_ref.action(&mut ctx.font_system, Action::Motion(Motion::Home));
                        }
                        Key::Named(NamedKey::End) => {
                            editor_ref.action(&mut ctx.font_system, Action::Motion(Motion::End));
                        }
                        Key::Named(NamedKey::PageUp) => {
                            editor_ref.action(&mut ctx.font_system, Action::Motion(Motion::PageUp));
                        }
                        Key::Named(NamedKey::PageDown) => {
                            editor_ref
                                .action(&mut ctx.font_system, Action::Motion(Motion::PageDown));
                        }
                        Key::Named(NamedKey::Escape) => {
                            editor_ref.action(&mut ctx.font_system, Action::Escape);
                        }
                        Key::Named(NamedKey::Enter) => {
                            editor_ref.action(&mut ctx.font_system, Action::Enter);
                        }
                        Key::Named(NamedKey::Backspace) => {
                            editor_ref.action(&mut ctx.font_system, Action::Backspace);
                        }
                        Key::Named(NamedKey::Delete) => {
                            editor_ref.action(&mut ctx.font_system, Action::Delete);
                        }
                        Key::Character(text) => {
                            if _ctrl_pressed {
                                // Handle Ctrl + Char
                                match text.as_str() {
                                    "c" => {
                                        // TODO: Copy
                                    }
                                    "v" => {
                                        // TOOD: Paste
                                    }
                                    "x" => {
                                        // TODO: Cut
                                    }
                                    _ => {
                                        // Ignore other Ctrl + Char combinations
                                    }
                                }
                            } else {
                                // Normal character input
                                for c in text.chars() {
                                    if c.is_control() {
                                        // Ignore control characters
                                        continue;
                                    }
                                    editor_ref.action(&mut ctx.font_system, Action::Insert(c));
                                }
                            }
                        }
                        _ => {
                            // Ignore other keys
                        }
                    }
                }
            }
            _ => {}
        }

        editor_ref.shape_as_needed(&mut ctx.font_system, true);

        WidgetResponse {
            message: None,
            focus_request: None,
            handled: true,
            clear_focus: false,
        }
    }
}
