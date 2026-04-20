use crate::renderer::UiBatcher;
use crate::utils::{Logical, Point, Rect};
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::input::{InputEvent, ButtonState};
use taffy::prelude::{auto, AlignItems, Display, JustifyContent, NodeId};
use taffy::Size as TaffySize;
use taffy::Style;

pub struct Button<M: Clone + std::fmt::Debug + Send> {
    pub content: Box<dyn Widget<M>>,
    pub on_press: M,
    pub key: Option<String>,
}

impl<M: Clone + std::fmt::Debug + Send> Button<M> {
    pub fn new(content: Box<dyn Widget<M>>, on_press: M) -> Self {
        Self {
            content,
            on_press,
            key: None,
        }
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
        let content_node = self.content.layout(taffy, ctx);
        taffy
            .new_with_children(
                Style {
                    display: Display::Flex,
                    align_items: Some(AlignItems::Center),
                    justify_content: Some(JustifyContent::Center),
                    size: TaffySize {
                        width: auto(),
                        height: auto(),
                    },
                    ..Default::default()
                },
                &[content_node],
            )
            .unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        _cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();

        let pos = Point::<crate::utils::Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        // Button is now a transparent container - use .background() modifier for styling
        let child_ids = taffy.children(node).unwrap();
        if let Some(content_node) = child_ids.get(0) {
            self.content.draw(
                taffy,
                *content_node,
                renderer,
                pos,
                focused_id,
                _cursor_blink,
                ctx,
            );
        }
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: crate::utils::Point<crate::utils::Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        let layout = taffy.layout(node).unwrap();
        let x = offset.x + layout.location.x;
        let y = offset.y + layout.location.y;

        let rect = Rect::<Logical>::from_xywh(x, y, layout.size.width, layout.size.height);

        // Handle pointer events
        if let InputEvent::PointerButton {
            state: ButtonState::Pressed,
            position,
            ..
        } = event
        {
            if rect.contains(position) {
                return WidgetResponse {
                    message: Some(self.on_press.clone()),
                    focus_request: None,
                    handled: true,
                    clear_focus: true, // Clear focus from other widgets
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
