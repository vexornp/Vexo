use crate::renderer::UiBatcher;
use crate::utils::{is_location_inside_quad, PhysicalLocation, TaffyQuad};
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Color;
use crate::Widget;
use taffy::prelude::{auto, length, AlignItems, Display, JustifyContent, NodeId, Rect, Size};
use taffy::Style;
use winit::event::WindowEvent;

pub struct Button<M: Clone + std::fmt::Debug + Send> {
    pub content: Box<dyn Widget<M>>,
    pub on_press: M,
    pub background_color: Color,
    pub padding: f32,
    pub key: Option<String>,
}

impl<M: Clone + std::fmt::Debug + Send> Button<M> {
    pub fn new(content: Box<dyn Widget<M>>, on_press: M) -> Self {
        Self {
            content,
            on_press,
            background_color: Color::rgb(0.2, 0.2, 0.2),
            padding: 10.0,
            key: None,
        }
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.background_color = color.into();
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
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        use crate::utils::{Point, Size};

        let layout = taffy.layout(node).unwrap();

        let pos = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );
        let size = Size::new(layout.size.width, layout.size.height);

        let color = self.background_color;
        let border_color = crate::Color::BLACK;
        let border_width = 1.0;

        renderer.add_rect(pos.to_array(), size.to_array(), color, border_color, border_width);

        let child_ids = taffy.children(node).unwrap();
        if let Some(content_node) = child_ids.get(0) {
            self.content.draw(
                taffy,
                *content_node,
                renderer,
                pos,
                focused_id,
                ctx,
            );
        }
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
        use crate::utils::{Point, TaffyQuad};

        let layout = taffy.layout(node).unwrap();
        let x = offset.x + layout.location.x;
        let y = offset.y + layout.location.y;

        let taffy_quad = TaffyQuad::from(x, y, layout.size);

        // Handle pointer events
        if let WindowEvent::PointerButton {
            state: winit::event::ElementState::Pressed,
            position,
            ..
        } = event
        {
            let location = PhysicalLocation::new(*position);
            let is_mouse_over = is_location_inside_quad(&location, &ctx.scale, &taffy_quad);
            if is_mouse_over {
                return WidgetResponse {
                    message: Some(self.on_press.clone()),
                    focus_request: None,
                    handled: true,
                };
            }
        }

        // Child event propagation
        let child_ids = taffy.children(node).unwrap();
        if let Some(content_node) = child_ids.get(0) {
            let content_offset = Point::new(x, y);
            return self.content.on_event(
                taffy,
                *content_node,
                content_offset,
                event,
                focused_id,
                ctx,
            );
        }

        WidgetResponse::default()
    }
}
