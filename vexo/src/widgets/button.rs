use crate::layout::{AlignItems, JustifyContent, Layout};
use crate::renderer::UiBatcher;
use crate::utils::{Logical, Point, Rect};
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::input::{InputEvent, ButtonState};
use taffy::prelude::NodeId;

pub struct Button<M: Clone + std::fmt::Debug + Send> {
    pub content: Box<dyn Widget<M>>,
    pub on_press: M,
    pub key: Option<String>,
    pub layout: Layout,
}

impl<M: Clone + std::fmt::Debug + Send> Button<M> {
    pub fn new(content: Box<dyn Widget<M>>, on_press: M) -> Self {
        Self {
            content,
            on_press,
            key: None,
            layout: Layout::default(),
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
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
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Button<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let content_node = self.content.layout(taffy, ctx);

        // Merge Button's layout with default flex container style
        let style = Layout {
            flex_direction: self.layout.flex_direction,
            flex_wrap: self.layout.flex_wrap,
            flex_grow: self.layout.flex_grow,
            flex_shrink: self.layout.flex_shrink,
            flex_basis: self.layout.flex_basis,
            justify_content: self.layout.justify_content.or(Some(JustifyContent::Center)),
            align_items: self.layout.align_items.or(Some(AlignItems::Center)),
            ..self.layout.clone()
        }
        .to_taffy_style();

        taffy.new_with_children(style, &[content_node]).unwrap()
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
