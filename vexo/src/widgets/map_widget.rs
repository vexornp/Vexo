//! Message mapping widget for converting between message types.

use crate::core::{Logical, Point, WidgetId};
use crate::input::{CursorIcon, InputEvent};
use crate::layout::{Layout, LayoutContext, LayoutNodeKey, LayoutView};
use crate::render::RenderCommand;
use crate::renderer::UiBatcher;
use crate::testable::{ComputedLayout, PaintContext};
use crate::widgets::{Widget, WidgetContext, WidgetResponse};
use crate::CursorBlinkState;

/// A widget wrapper that maps messages from one type to another.
///
/// This is useful when a child widget produces messages of one type,
/// but the parent expects messages of a different type.
///
/// For wrapping `ComponentWidget`, prefer the `component!` macro which
/// simplifies the boilerplate.
///
/// # Example
///
/// ```ignore
/// use vexo::widgets::MapWidget;
///
/// // Wrap a component that emits CounterOutput to produce Message::CounterOutput
/// let mapped = MapWidget::new(
///     Box::new(component_widget),
///     |output| Message::CounterOutput(output),
/// );
///
/// // Or use the component! macro for ComponentWidget:
/// use vexo::component;
/// let counter = component!(CounterComponent, "counter", |output| Message::CounterOutput(output));
/// ```
pub struct MapWidget<M1, M2, F>
where
    M1: Clone + std::fmt::Debug + Send,
    M2: Clone + std::fmt::Debug + Send,
    F: Fn(M1) -> M2 + Send,
{
    inner: Box<dyn Widget<M1>>,
    mapper: F,
    computed_layout: Option<ComputedLayout>,
}

impl<M1, M2, F> MapWidget<M1, M2, F>
where
    M1: Clone + std::fmt::Debug + Send,
    M2: Clone + std::fmt::Debug + Send,
    F: Fn(M1) -> M2 + Send,
{
    /// Create a new MapWidget that wraps `inner` and transforms messages using `mapper`.
    pub fn new(inner: Box<dyn Widget<M1>>, mapper: F) -> Self {
        Self {
            inner,
            mapper,
            computed_layout: None,
        }
    }
}

impl<M1, M2, F> Widget<M2> for MapWidget<M1, M2, F>
where
    M1: Clone + std::fmt::Debug + Send,
    M2: Clone + std::fmt::Debug + Send,
    F: Fn(M1) -> M2 + Send,
{
    fn key(&self) -> Option<&str> {
        self.inner.key()
    }

    fn layout_props(&self) -> Layout {
        self.inner.layout_props()
    }

    fn cursor(&self) -> CursorIcon {
        self.inner.cursor()
    }

    fn layout(
        &mut self,
        layout_ctx: &mut LayoutContext,
        widget_ctx: &mut WidgetContext,
    ) -> LayoutNodeKey {
        self.inner.layout(layout_ctx, widget_ctx)
    }

    fn apply_layout(&mut self, layout: ComputedLayout) {
        self.computed_layout = Some(layout);
        self.inner.apply_layout(layout);
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        self.inner.paint(ctx)
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeKey,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &CursorBlinkState,
        widget_ctx: &mut WidgetContext,
    ) {
        self.inner.draw(
            layout_view,
            node,
            renderer,
            offset,
            focused_id,
            cursor_blink,
            widget_ctx,
        );
    }

    fn on_event(
        &mut self,
        layout_view: &LayoutView,
        node: LayoutNodeKey,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_ctx: &mut WidgetContext,
    ) -> WidgetResponse<M2> {
        let response = self.inner.on_event(
            layout_view,
            node,
            offset,
            event,
            focused_id,
            widget_ctx,
        );

        let mapped_message = response.message.map(&self.mapper);

        WidgetResponse {
            message: mapped_message,
            focus_request: response.focus_request,
            handled: response.handled,
            clear_focus: response.clear_focus,
            cursor: response.cursor,
        }
    }
}

// Enable Box<dyn Widget<M2>> pattern
impl<M1, M2, F> Widget<M2> for Box<MapWidget<M1, M2, F>>
where
    M1: Clone + std::fmt::Debug + Send,
    M2: Clone + std::fmt::Debug + Send,
    F: Fn(M1) -> M2 + Send,
{
    fn key(&self) -> Option<&str> {
        (**self).key()
    }

    fn layout_props(&self) -> Layout {
        (**self).layout_props()
    }

    fn cursor(&self) -> CursorIcon {
        (**self).cursor()
    }

    fn layout(
        &mut self,
        layout_ctx: &mut LayoutContext,
        widget_ctx: &mut WidgetContext,
    ) -> LayoutNodeKey {
        (**self).layout(layout_ctx, widget_ctx)
    }

    fn apply_layout(&mut self, layout: ComputedLayout) {
        (**self).apply_layout(layout)
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        (**self).paint(ctx)
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeKey,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &CursorBlinkState,
        widget_ctx: &mut WidgetContext,
    ) {
        (**self).draw(layout_view, node, renderer, offset, focused_id, cursor_blink, widget_ctx)
    }

    fn on_event(
        &mut self,
        layout_view: &LayoutView,
        node: LayoutNodeKey,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_ctx: &mut WidgetContext,
    ) -> WidgetResponse<M2> {
        (**self).on_event(layout_view, node, offset, event, focused_id, widget_ctx)
    }
}
