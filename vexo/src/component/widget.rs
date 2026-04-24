//! Component widget bridge.
//! This module will be implemented in a future task.

use crate::widgets::Widget;
use crate::render::RenderCommand;

/// Widget wrapper for components (placeholder).
pub struct ComponentWidget<M> {
    _marker: std::marker::PhantomData<M>,
}

impl<M: Clone + std::fmt::Debug + Send + 'static> Widget<M> for ComponentWidget<M> {
    fn key(&self) -> Option<&str> {
        None
    }

    fn layout(
        &mut self,
        _layout_ctx: &mut crate::layout::LayoutContext,
        _widget_ctx: &mut crate::widgets::WidgetContext,
    ) -> crate::layout::LayoutNodeId {
        unimplemented!("ComponentWidget::layout not yet implemented")
    }

    fn apply_layout(&mut self, _layout: crate::testable::ComputedLayout) {
        unimplemented!("ComponentWidget::apply_layout not yet implemented")
    }

    fn paint(&self, _ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        unimplemented!("ComponentWidget::paint not yet implemented")
    }

    fn draw(
        &self,
        _layout_view: &crate::layout::LayoutView,
        _node: crate::layout::LayoutNodeId,
        _renderer: &mut crate::renderer::UiBatcher,
        _offset: crate::core::Point<crate::core::Logical>,
        _focused_id: Option<crate::core::WidgetId>,
        _cursor_blink: &crate::CursorBlinkState,
        _widget_ctx: &mut crate::widgets::WidgetContext,
    ) {
        unimplemented!("ComponentWidget::draw not yet implemented")
    }

    fn on_event(
        &mut self,
        _layout_view: &crate::layout::LayoutView,
        _node: crate::layout::LayoutNodeId,
        _offset: crate::core::Point<crate::core::Logical>,
        _event: &crate::input::InputEvent,
        _focused_id: Option<crate::core::WidgetId>,
        _widget_ctx: &mut crate::widgets::WidgetContext,
    ) -> crate::widgets::WidgetResponse<M> {
        unimplemented!("ComponentWidget::on_event not yet implemented")
    }
}
