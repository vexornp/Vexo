use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::Color;
use glyphon::{cosmic_text::Motion, Action, SwashCache};
use taffy::prelude::{length, NodeId};
use taffy::Style;
use winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    keyboard::{Key, NamedKey},
};

pub struct Text {
    pub content: String,
    pub font_size: f32,
    pub color: Color,
    pub key: Option<String>,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            font_size: 24.0,
            color: Color::BLACK,
            key: None,
        }
    }

    /// Set the font size.
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Text {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        // Calculate intrinsic size based on content and font size
        let width_guess = self.content.len() as f32 * (self.font_size * 0.5);
        let height_guess = self.font_size * 1.2;

        let node = taffy
            .new_leaf(Style {
                size: taffy::Size {
                    width: length(width_guess),
                    height: length(height_guess),
                },
                ..Default::default()
            })
            .unwrap();

        ctx.record_node_widget(node);
        node
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        use crate::utils::Point;

        let layout = taffy.layout(node).unwrap();
        let pos = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        renderer.add_text(self.content.clone(), pos, self.font_size, self.color);
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: crate::utils::Point<crate::utils::Logical>,
        event: &winit::event::WindowEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        WidgetResponse::default()
    }
}

pub struct TextEdit {
    pub editor_id: String,
    pub initial_text: String,
    pub swash_cache: SwashCache,
    pub text_color: Color,
    pub key: Option<String>,
}

impl TextEdit {
    pub fn new(id: impl Into<String>, initial_text: impl Into<String>) -> Self {
        Self {
            editor_id: id.into(),
            initial_text: initial_text.into(),
            swash_cache: SwashCache::new(),
            text_color: Color::WHITE,
            key: None,
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
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
        let node_id = taffy
            .new_leaf(Style {
                flex_grow: 1.0,
                ..Default::default()
            })
            .unwrap();
        ctx.record_node_widget(node_id);
        node_id
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        _focused_id: Option<WidgetId>,
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

        let _text_color = crate::Color::WHITE;
        let _cursor_color = crate::Color::WHITE;
        let _selection_color = crate::Color::new(1.0, 1.0, 1.0, 0.2);
        let _selected_text_color = crate::Color::rgb(0.627, 0.627, 1.0);

        let mut _cache = SwashCache::new();
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
        // Determine our widget id from the node->widget mapping
        let my_id = ctx.get_widget_id(_node);
        let is_focused = focused_id == my_id;

        if !is_focused {
            // Check for click to grab focus
            if let WindowEvent::PointerButton {
                state: winit::event::ElementState::Pressed,
                ..
            } = _event
            {
                let layout = _taffy.layout(_node).unwrap();
                let rect = crate::utils::Rect::<crate::utils::Logical>::from_layout(layout.location, layout.size);

                let logical_pos = ctx.cursor_pos.to_logical(ctx.scale.factor());
                if rect.contains(&logical_pos) {
                    return WidgetResponse {
                        message: None,
                        focus_request: my_id,
                        handled: true,
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
            WindowEvent::PointerButton { device_id: _, .. } => {
                // if *button == MouseButton::Left {
                //     if state.is_pressed() {
                //         let layout = _taffy.layout(_node).unwrap();
                //         let x = _offset.x + layout.location.x;
                //         let y = _offset.y + layout.location.y;
                //         let width = layout.size.width;
                //         let height = layout.size.height;

                //         let relative_physical_x =
                //             (_mouse_x.round() as i32).saturating_sub(width as i32);
                //         let relative_physical_y =
                //             (_mouse_y.round() as i32).saturating_sub(height as i32);

                //         // Handle mouse click
                //         editor_ref.action(
                //             &mut ctx.font_system,
                //             Action::Click {
                //                 x: relative_physical_x,
                //                 y: relative_physical_y,
                //             },
                //         );
                //     }
                // }
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
        }
    }
}
