use crate::core::{Logical, Point};
use crate::layout::{AlignItems, FlexDirection, JustifyContent, Layout, LayoutContext, LayoutNodeId, LayoutView};
use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::input::InputEvent;

pub struct Column<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
    pub layout: Layout,
}

impl<M: Clone + std::fmt::Debug + Send> Column<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
            layout: Layout::default(),
        }
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the entire Layout struct.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
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

    /// Set gap between children.
    pub fn gap(mut self, value: f32) -> Self {
        self.layout = self.layout.gap(value);
        self
    }

    /// Enable flex wrapping.
    pub fn flex_wrap(mut self) -> Self {
        self.layout = self.layout.flex_wrap();
        self
    }

    /// Set justify content.
    pub fn justify(mut self, value: JustifyContent) -> Self {
        self.layout = self.layout.justify(value);
        self
    }

    /// Set align items.
    pub fn align(mut self, value: AlignItems) -> Self {
        self.layout = self.layout.align(value);
        self
    }

    /// Set flex grow factor.
    pub fn flex_grow(mut self, value: f32) -> Self {
        self.layout = self.layout.flex_grow(value);
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

    /// Fill available space (sets flex_grow to 1.0).
    pub fn fill(mut self) -> Self {
        self.layout = self.layout.flex_grow(1.0);
        self
    }
}

impl<M: Clone + std::fmt::Debug + Send> Default for Column<M> {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Column<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, ctx: &mut LayoutContext, widget_ctx: &mut WidgetContext) -> LayoutNodeId {
        let mut child_nodes: Vec<LayoutNodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(ctx, widget_ctx));
        }

        let layout = Layout {
            flex_direction: Some(FlexDirection::Column),
            ..self.layout.clone()
        };

        ctx.create_container(&layout, &child_nodes)
    }

    fn draw(
        &self,
        ctx: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        widget_ctx: &mut WidgetContext,
    ) {
        if let Some(layout) = ctx.get_layout(node) {
            let my_offset = Point::<Logical>::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            let child_ids = ctx.children(node);
            for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
                child_widget.draw(
                    ctx,
                    child_node_id,
                    renderer,
                    my_offset,
                    focused_id,
                    cursor_blink,
                    widget_ctx,
                );
            }
        }
    }

    fn on_event(
        &mut self,
        ctx: &LayoutView,
        node: LayoutNodeId,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        if let Some(layout) = ctx.get_layout(node) {
            let child_ids = ctx.children(node);
            let my_offset = Point::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
                let child_response =
                    child.on_event(ctx, child_node_id, my_offset, event, focused_id, widget_ctx);

                if child_response.handled || child_response.focus_request.is_some() {
                    return child_response;
                }
            }
        }
        WidgetResponse::default()
    }
}

pub struct Row<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
    pub layout: Layout,
}

impl<M: Clone + std::fmt::Debug + Send> Row<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
            layout: Layout::default(),
        }
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the entire Layout struct.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
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

    /// Set gap between children.
    pub fn gap(mut self, value: f32) -> Self {
        self.layout = self.layout.gap(value);
        self
    }

    /// Enable flex wrapping.
    pub fn flex_wrap(mut self) -> Self {
        self.layout = self.layout.flex_wrap();
        self
    }

    /// Set justify content.
    pub fn justify(mut self, value: JustifyContent) -> Self {
        self.layout = self.layout.justify(value);
        self
    }

    /// Set align items.
    pub fn align(mut self, value: AlignItems) -> Self {
        self.layout = self.layout.align(value);
        self
    }

    /// Set flex grow factor.
    pub fn flex_grow(mut self, value: f32) -> Self {
        self.layout = self.layout.flex_grow(value);
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

    /// Fill available space (sets flex_grow to 1.0).
    pub fn fill(mut self) -> Self {
        self.layout = self.layout.flex_grow(1.0);
        self
    }
}

impl<M: Clone + std::fmt::Debug + Send> Default for Row<M> {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Row<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, ctx: &mut LayoutContext, widget_ctx: &mut WidgetContext) -> LayoutNodeId {
        let mut child_nodes: Vec<LayoutNodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(ctx, widget_ctx));
        }

        let layout = Layout {
            flex_direction: Some(FlexDirection::Row),
            ..self.layout.clone()
        };

        ctx.create_container(&layout, &child_nodes)
    }

    fn draw(
        &self,
        ctx: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        widget_ctx: &mut WidgetContext,
    ) {
        if let Some(layout) = ctx.get_layout(node) {
            let my_offset = Point::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            let child_ids = ctx.children(node);
            for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
                child_widget.draw(
                    ctx,
                    child_node_id,
                    renderer,
                    my_offset,
                    focused_id,
                    cursor_blink,
                    widget_ctx,
                );
            }
        }
    }

    fn on_event(
        &mut self,
        ctx: &LayoutView,
        node: LayoutNodeId,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        if let Some(layout) = ctx.get_layout(node) {
            let child_ids = ctx.children(node);
            let my_offset = Point::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
                let child_response =
                    child.on_event(ctx, child_node_id, my_offset, event, focused_id, widget_ctx);

                if child_response.handled || child_response.focus_request.is_some() {
                    return child_response;
                }
            }
        }
        WidgetResponse::default()
    }
}