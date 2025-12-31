use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use taffy::prelude::{auto, length, AlignItems, Display, JustifyContent, NodeId, Rect, Size};
use taffy::Style;
use winit::event::WindowEvent;
pub struct Button<M: Clone + std::fmt::Debug + Send> {
    pub content: Box<dyn Widget<M>>,
    pub on_press: M,
    pub background_color: [f32; 3],
    pub padding: f32,
    pub key: Option<String>,
}

impl<M: Clone + std::fmt::Debug + Send> Button<M> {
    pub fn new(content: Box<dyn Widget<M>>, on_press: M) -> Self {
        Self {
            content,
            on_press,
            background_color: [0.2, 0.2, 0.2],
            padding: 10.0,
            key: None,
        }
    }

    pub fn color(mut self, color: [f32; 3]) -> Self {
        self.background_color = color;
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Button<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        // push content index (single child)
        ctx.push_index(1);
        let content_node = self.content.layout(taffy, ctx);
        ctx.pop();
        let node = taffy
            .new_with_children(
                Style {
                    display: Display::Flex,
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    padding: Rect {
                        left: length(self.padding),
                        right: length(self.padding),
                        top: length(self.padding),
                        bottom: length(self.padding),
                    },
                    size: Size {
                        width: auto(),
                        height: auto(),
                    },
                    ..Default::default()
                },
                &[content_node],
            )
            .unwrap();

        ctx.record_node_widget(node);
        node
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;
        renderer.add_rect(
            x,
            y,
            layout.size.width,
            layout.size.height,
            self.background_color,
        );

        let child_ids = taffy.children(node).unwrap();
        if let Some(content_node) = child_ids.get(0) {
            let content_offset = (x, y);
            self.content.draw(
                taffy,
                *content_node,
                renderer,
                content_offset,
                focused_id,
                ctx,
            );
        }
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        cursor_pos: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        let layout = taffy.layout(node).unwrap();
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;
        let width = layout.size.width;
        let height = layout.size.height;

        let is_over = cursor_pos.0 >= x
            && cursor_pos.0 <= x + width
            && cursor_pos.1 >= y
            && cursor_pos.1 <= y + height;

        // Handle mobile touch events here if needed
        if let WindowEvent::Touch(touch) = event {
            if touch.phase == winit::event::TouchPhase::Started {
                let touch_x = touch.location.x as f32;
                let touch_y = touch.location.y as f32;
                if touch_x >= x && touch_x <= x + width && touch_y >= y && touch_y <= y + height {
                    return WidgetResponse {
                        message: Some(self.on_press.clone()),
                        focus_request: None,
                        handled: true,
                    };
                }
            }
        }

        // 1. CLICK HANDLING
        if is_over {
            if let WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } = event
            {
                return WidgetResponse {
                    message: Some(self.on_press.clone()),
                    focus_request: None,
                    handled: true,
                };
            }
        }

        // 2. CHILD EVENT PROPAGATION
        let child_ids = taffy.children(node).unwrap();
        if let Some(content_node) = child_ids.get(0) {
            let content_offset = (x, y);
            return self.content.on_event(
                taffy,
                *content_node, // Pass event to the content node
                content_offset,
                event,
                cursor_pos,
                focused_id,
                ctx,
            );
        }

        WidgetResponse::default()
    }
}
