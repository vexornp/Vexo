//! Grid container widget for 2D layouts.

use crate::core::{Logical, Point, Size};
use crate::input::InputEvent;
use crate::layout::{Layout, TrackSizing};
use crate::renderer::UiBatcher;
use crate::widgets::{Widget, WidgetContext, WidgetId, WidgetResponse};
use taffy::prelude::NodeId;

/// Grid container for 2D layouts with rows and columns.
pub struct Grid<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
    pub layout: Layout,
}

impl<M: Clone + std::fmt::Debug + Send> Grid<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
            layout: Layout::default(),
        }
    }

    /// Define column sizes.
    pub fn columns(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.layout = self.layout.columns(sizes);
        self
    }

    /// Define row sizes.
    pub fn rows(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.layout = self.layout.rows(sizes);
        self
    }

    /// Set gap between cells.
    pub fn gap(mut self, value: f32) -> Self {
        self.layout = self.layout.gap(value);
        self
    }

    /// Set gap using a Size value.
    pub fn gap_size(mut self, size: Size<Logical>) -> Self {
        self.layout = self.layout.gap_size(size);
        self
    }

    /// Set horizontal and vertical gap separately.
    pub fn gap_each(mut self, width: f32, height: f32) -> Self {
        self.layout = self.layout.gap_each(width, height);
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

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl<M: Clone + std::fmt::Debug + Send> Default for Grid<M> {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Grid<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let mut child_nodes: Vec<NodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(taffy, ctx));
        }

        // Build grid style
        let mut style = self.layout.clone().to_taffy_style();
        style.display = taffy::prelude::Display::Grid;

        taffy.new_with_children(style, &child_nodes).unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
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
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
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
