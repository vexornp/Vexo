//! Grid container widget for 2D layouts.

use crate::core::{Logical, Point, Size, WidgetId};
use crate::input::InputEvent;
use crate::layout::{Display, Layout, LayoutContext, LayoutNodeId, LayoutView, TrackSizing};
use crate::render::RenderCommand;
use crate::renderer::UiBatcher;
use crate::widgets::{Widget, WidgetContext, WidgetResponse};

/// Grid container for 2D layouts with rows and columns.
pub struct Grid<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
    pub layout: Layout,
    computed_layout: Option<crate::widget::ComputedLayout>,
}

impl<M: Clone + std::fmt::Debug + Send> Grid<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
            layout: Layout::default(),
            computed_layout: None,
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

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeId {
        let mut child_nodes: Vec<LayoutNodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(layout_context, widget_context));
        }

        // Build grid layout with display: Grid
        let layout = Layout {
            display: Some(Display::Grid),
            ..self.layout.clone()
        };

        layout_context.create_container(&layout, &child_nodes)
    }

    fn apply_layout(&mut self, layout: crate::widget::ComputedLayout) {
        self.computed_layout = Some(layout);
    }

    fn paint(&self, ctx: &mut crate::widget::PaintContext) -> Vec<RenderCommand> {
        crate::widget::Paint::paint(self, ctx)
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        widget_context: &mut WidgetContext,
    ) {
        if let Some(layout) = layout_view.get_layout(node) {
            let my_offset = Point::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            let child_ids = layout_view.children(node);
            for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
                child_widget.draw(
                    layout_view,
                    child_node_id,
                    renderer,
                    my_offset,
                    focused_id,
                    cursor_blink,
                    widget_context,
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
        widget_context: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        if let Some(layout) = layout_view.get_layout(node) {
            let child_ids = layout_view.children(node);
            let my_offset = Point::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
                let child_response =
                    child.on_event(layout_view, child_node_id, my_offset, event, focused_id, widget_context);

                if child_response.handled || child_response.focus_request.is_some() {
                    return child_response;
                }
            }
        }
        WidgetResponse::default()
    }
}

// ============================================================================
// SEPARATED TRAIT IMPLEMENTATIONS
// ============================================================================

impl<M: Clone + std::fmt::Debug + Send> crate::widget::Identifiable for Grid<M> {
    fn id(&self) -> Option<WidgetId> {
        self.key.as_ref().map(|k| WidgetId::from_key(k))
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::widget::Layout for Grid<M> {
    fn constraints(&self) -> crate::widget::LayoutConstraints {
        crate::widget::LayoutConstraints::from_layout(&self.layout)
    }

    fn apply_layout(&mut self, layout: crate::widget::ComputedLayout) {
        self.computed_layout = Some(layout);
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::widget::Paint for Grid<M> {
    fn paint(&self, _ctx: &mut crate::widget::PaintContext) -> Vec<RenderCommand> {
        // Grid is a transparent container - children paint themselves
        Vec::new()
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::widget::Interact<M> for Grid<M> {
    fn on_event(
        &mut self,
        _event: &InputEvent,
        _ctx: &crate::widget::InteractionContext,
    ) -> crate::widget::InteractionResponse<M> {
        // Grid delegates event handling to children via legacy Widget trait
        crate::widget::InteractionResponse::default()
    }
}
