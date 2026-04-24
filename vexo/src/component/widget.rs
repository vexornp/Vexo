//! Bridge between Component trait and Widget trait.

use crate::component::{Component, KeyPath};
use crate::core::{Logical, Point, WidgetId};
use crate::input::CursorIcon;
use crate::input::InputEvent;
use crate::layout::{Layout, LayoutContext, LayoutNodeId, LayoutView};
use crate::render::RenderCommand;
use crate::renderer::UiBatcher;
use crate::testable::ComputedLayout;
use crate::testable::PaintContext;
use crate::widgets::{Widget, WidgetContext, WidgetResponse};
use crate::CursorBlinkState;

/// Widget wrapper that hosts a Component.
pub struct ComponentWidget<C: Component> {
    state: C::State,
    storage_key: String,
    key_path: KeyPath,
    cached_view: Option<Box<dyn Widget<C::Message>>>,
    computed_layout: Option<ComputedLayout>,
}

impl<C: Component> ComponentWidget<C> {
    pub fn new(storage_key: impl Into<String>) -> Self {
        let storage_key = storage_key.into();
        let key_path = KeyPath::root().child(&storage_key);
        Self {
            state: C::initial_state(),
            storage_key,
            key_path,
            cached_view: None,
            computed_layout: None,
        }
    }

    pub fn with_state(mut self, state: C::State) -> Self {
        self.state = state;
        self
    }

    pub fn storage_key(&self) -> &str {
        &self.storage_key
    }

    pub fn state(&self) -> &C::State {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut C::State {
        &mut self.state
    }
}

impl<C: Component> Widget<C::Output> for ComponentWidget<C> {
    fn key(&self) -> Option<&str> {
        Some(&self.storage_key)
    }

    fn layout_props(&self) -> Layout {
        if let Some(ref view) = self.cached_view {
            view.layout_props()
        } else {
            Layout::default()
        }
    }

    fn cursor(&self) -> CursorIcon {
        if let Some(ref view) = self.cached_view {
            view.cursor()
        } else {
            CursorIcon::Default
        }
    }

    fn layout(
        &mut self,
        layout_ctx: &mut LayoutContext,
        widget_ctx: &mut WidgetContext,
    ) -> LayoutNodeId {
        let mut component_ctx = widget_ctx.create_component_context(self.key_path.clone());

        let view = C::view(&self.state, &mut component_ctx);
        self.cached_view = Some(view);

        if let Some(ref mut view) = self.cached_view {
            view.layout(layout_ctx, widget_ctx)
        } else {
            layout_ctx.create_leaf(&Layout::default())
        }
    }

    fn apply_layout(&mut self, layout: ComputedLayout) {
        self.computed_layout = Some(layout);
        if let Some(ref mut view) = self.cached_view {
            view.apply_layout(layout);
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        if let Some(ref view) = self.cached_view {
            view.paint(ctx)
        } else {
            Vec::new()
        }
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &CursorBlinkState,
        widget_ctx: &mut WidgetContext,
    ) {
        if let Some(ref view) = self.cached_view {
            view.draw(
                layout_view,
                node,
                renderer,
                offset,
                focused_id,
                cursor_blink,
                widget_ctx,
            );
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
    ) -> WidgetResponse<C::Output> {
        let response = if let Some(ref mut view) = self.cached_view {
            view.on_event(
                layout_view,
                node,
                offset,
                event,
                focused_id,
                widget_ctx,
            )
        } else {
            return WidgetResponse::default();
        };

        if let Some(internal_msg) = response.message {
            C::update(&mut self.state, internal_msg.clone());
            let output_msg = C::map_message(internal_msg, &self.state);

            WidgetResponse {
                message: output_msg,
                focus_request: response.focus_request,
                handled: response.handled,
                clear_focus: response.clear_focus,
                cursor: response.cursor,
            }
        } else {
            WidgetResponse {
                message: None,
                focus_request: response.focus_request,
                handled: response.handled,
                clear_focus: response.clear_focus,
                cursor: response.cursor,
            }
        }
    }
}

// Enable Box<dyn Widget<C::Output>> pattern
impl<C: Component> Widget<C::Output> for Box<ComponentWidget<C>> {
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
    ) -> LayoutNodeId {
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
        node: LayoutNodeId,
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
        node: LayoutNodeId,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_ctx: &mut WidgetContext,
    ) -> WidgetResponse<C::Output> {
        (**self).on_event(layout_view, node, offset, event, focused_id, widget_ctx)
    }
}
