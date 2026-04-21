use crate::layout::{AlignItems, FlexDirection, JustifyContent, Layout};
use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::input::InputEvent;
use taffy::prelude::NodeId;

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

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let mut child_nodes: Vec<NodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(taffy, ctx));
        }

        let style = Layout {
            flex_direction: Some(FlexDirection::Column),
            ..self.layout.clone()
        }
        .to_taffy_style();

        taffy.new_with_children(style, &child_nodes).unwrap()
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
        use crate::utils::Point;

        let layout = taffy.layout(node).unwrap();
        let my_offset = Point::<crate::utils::Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        let child_ids = taffy.children(node).unwrap();
        for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
            child_widget.draw(
                taffy,
                child_node_id,
                renderer,
                my_offset,
                focused_id,
                cursor_blink,
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
        use crate::utils::Point;

        let child_ids = taffy.children(node).unwrap();
        let layout = taffy.layout(node).unwrap();
        let my_offset = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
            let child_response =
                child.on_event(taffy, child_node_id, my_offset, event, focused_id, ctx);

            if child_response.handled || child_response.focus_request.is_some() {
                return child_response;
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

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let mut child_nodes: Vec<NodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(taffy, ctx));
        }

        let style = Layout {
            flex_direction: Some(FlexDirection::Row),
            ..self.layout.clone()
        }
        .to_taffy_style();

        taffy.new_with_children(style, &child_nodes).unwrap()
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
        use crate::utils::Point;

        let layout = taffy.layout(node).unwrap();
        let my_offset = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        let child_ids = taffy.children(node).unwrap();
        for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
            child_widget.draw(
                taffy,
                child_node_id,
                renderer,
                my_offset,
                focused_id,
                cursor_blink,
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
        use crate::utils::Point;

        let child_ids = taffy.children(node).unwrap();
        let layout = taffy.layout(node).unwrap();
        let my_offset = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
            let child_response =
                child.on_event(taffy, child_node_id, my_offset, event, focused_id, ctx);

            if child_response.handled || child_response.focus_request.is_some() {
                return child_response;
            }
        }
        WidgetResponse::default()
    }
}
