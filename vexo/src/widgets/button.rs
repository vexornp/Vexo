use crate::layout::{AlignItems, JustifyContent, Layout, LayoutContext, LayoutNodeId, LayoutView};
use crate::renderer::UiBatcher;
use crate::core::{Logical, Point, Rect};
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::input::{InputEvent, ButtonState};

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

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_ctx: &mut WidgetContext) -> LayoutNodeId {
        let content_node = self.content.layout(layout_context, widget_ctx);

        // Merge Button's layout with default flex container style
        let layout = Layout {
            flex_direction: self.layout.flex_direction,
            flex_wrap: self.layout.flex_wrap,
            flex_grow: self.layout.flex_grow,
            flex_shrink: self.layout.flex_shrink,
            flex_basis: self.layout.flex_basis,
            justify_content: self.layout.justify_content.or(Some(JustifyContent::Center)),
            align_items: self.layout.align_items.or(Some(AlignItems::Center)),
            ..self.layout.clone()
        };

        layout_context.create_container(&layout, &[content_node])
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        widget_ctx: &mut WidgetContext,
    ) {
        if let Some(layout) = layout_view.get_layout(node) {
            let pos = Point::<Logical>::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            // Button is now a transparent container - use .background() modifier for styling
            let child_ids = layout_view.children(node);
            if let Some(content_node) = child_ids.get(0) {
                self.content.draw(
                    layout_view,
                    *content_node,
                    renderer,
                    pos,
                    focused_id,
                    cursor_blink,
                    widget_ctx,
                );
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
        widget_ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        if let Some(layout) = layout_view.get_layout(node) {
            let x = offset.x + layout.x();
            let y = offset.y + layout.y();

            let rect = Rect::<Logical>::from_xywh(x, y, layout.width(), layout.height());

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
            let child_ids = layout_view.children(node);
            if let Some(content_node) = child_ids.get(0) {
                let content_offset = Point::new(x, y);
                return self.content.on_event(
                    layout_view,
                    *content_node,
                    content_offset,
                    event,
                    focused_id,
                    widget_ctx,
                );
            }
        }

        WidgetResponse::default()
    }
}
