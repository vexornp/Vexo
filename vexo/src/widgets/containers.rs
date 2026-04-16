use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use glyphon::Color;
use taffy::prelude::{length, Display, FlexDirection, NodeId, Size};
use taffy::Style;

pub struct Column<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
    pub align_items: taffy::prelude::AlignItems,
}

impl<M: Clone + std::fmt::Debug + Send> Column<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
            align_items: taffy::prelude::AlignItems::Start,
        }
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }

    pub fn align_items(mut self, align: taffy::prelude::AlignItems) -> Self {
        self.align_items = align;
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
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
        taffy
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    align_items: Some(self.align_items),
                    gap: Size {
                        width: length(0.0),
                        height: length(10.0),
                    },
                    ..Default::default()
                },
                &child_nodes,
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
        event: &winit::event::WindowEvent,
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
}

impl<M: Clone + std::fmt::Debug + Send> Row<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
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
        taffy
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    gap: Size {
                        width: length(10.0),
                        height: length(0.0),
                    },
                    ..Default::default()
                },
                &child_nodes,
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
        event: &winit::event::WindowEvent,
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
