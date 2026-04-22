use crate::core::{Logical, Point, Size};
use crate::layout::{LayoutContext, LayoutNodeId, LayoutView};
use crate::renderer::UiBatcher;
use crate::widgets::{Widget, WidgetContext, WidgetId, WidgetResponse};
use crate::input::InputEvent;
use crate::Color;
use std::marker::PhantomData;

/// Extension trait providing visual modifier chaining.
///
/// All widgets automatically implement this trait via blanket impl.
pub trait WidgetExt<M: Clone + std::fmt::Debug + Send>: Widget<M> + Sized {
    /// Draw a colored background behind the widget.
    fn background(self, color: Color) -> Background<Self, M> {
        Background::new(self, color)
    }

    /// Draw a border around the widget.
    fn border(self, color: Color, width: f32) -> Border<Self, M> {
        Border::new(self, color, width)
    }

    /// Apply rounded corners to the widget.
    fn corner_radius(self, radius: f32) -> CornerRadius<Self, M> {
        CornerRadius::new(self, radius)
    }

    /// Box this widget for use in containers.
    fn boxed(self) -> Box<dyn Widget<M>>
    where
        Self: 'static,
    {
        Box::new(self)
    }
}

// Blanket implementation: all Widget types get WidgetExt methods
impl<M: Clone + std::fmt::Debug + Send, W: Widget<M>> WidgetExt<M> for W {}

// ============================================================================
// Background Modifier
// ============================================================================

/// Draws a colored background behind a child widget.
pub struct Background<W, M> {
    child: W,
    color: Color,
    _marker: PhantomData<M>,
}

impl<W, M: Clone + std::fmt::Debug + Send> Background<W, M> {
    pub fn new(child: W, color: Color) -> Self {
        Self {
            child,
            color,
            _marker: PhantomData,
        }
    }
}

impl<W: Widget<M>, M: Clone + std::fmt::Debug + Send> Widget<M> for Background<W, M> {
    fn key(&self) -> Option<&str> {
        self.child.key()
    }

    fn layout(&mut self, ctx: &mut LayoutContext, widget_ctx: &mut WidgetContext) -> LayoutNodeId {
        // Layout child, background uses same bounds
        self.child.layout(ctx, widget_ctx)
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
            let pos = Point::<Logical>::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );
            let size = Size::<Logical>::new(layout.width(), layout.height());

            // Draw background rect first (behind child)
            renderer.add_rect(pos.to_array(), size.to_array(), self.color, Color::TRANSPARENT, 0.0, 0.0);

            // Draw child on top - pass original offset since child will add its own layout offset
            self.child.draw(ctx, node, renderer, offset, focused_id, cursor_blink, widget_ctx);
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
        // Pass original offset since child will add its own layout offset
        self.child.on_event(ctx, node, offset, event, focused_id, widget_ctx)
    }
}

// ============================================================================
// Border Modifier
// ============================================================================

/// Draws a border around a child widget.
pub struct Border<W, M> {
    child: W,
    color: Color,
    width: f32,
    _marker: PhantomData<M>,
}

impl<W, M: Clone + std::fmt::Debug + Send> Border<W, M> {
    pub fn new(child: W, color: Color, width: f32) -> Self {
        Self {
            child,
            color,
            width,
            _marker: PhantomData,
        }
    }
}

impl<W: Widget<M>, M: Clone + std::fmt::Debug + Send> Widget<M> for Border<W, M> {
    fn key(&self) -> Option<&str> {
        self.child.key()
    }

    fn layout(&mut self, ctx: &mut LayoutContext, widget_ctx: &mut WidgetContext) -> LayoutNodeId {
        // Layout child, border uses same bounds
        self.child.layout(ctx, widget_ctx)
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
            let pos = Point::<Logical>::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );
            let size = Size::<Logical>::new(layout.width(), layout.height());

            // Draw child first - pass original offset since child will add its own layout offset
            self.child.draw(ctx, node, renderer, offset, focused_id, cursor_blink, widget_ctx);

            // Draw border on top (transparent fill, colored border)
            renderer.add_rect(pos.to_array(), size.to_array(), Color::TRANSPARENT, self.color, self.width, 0.0);
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
        // Pass original offset since child will add its own layout offset
        self.child.on_event(ctx, node, offset, event, focused_id, widget_ctx)
    }
}

// ============================================================================
// CornerRadius Modifier
// ============================================================================

/// Applies rounded corners to a child widget's background/border.
pub struct CornerRadius<W, M> {
    child: W,
    radius: f32,
    _marker: PhantomData<M>,
}

impl<W, M: Clone + std::fmt::Debug + Send> CornerRadius<W, M> {
    pub fn new(child: W, radius: f32) -> Self {
        Self {
            child,
            radius,
            _marker: PhantomData,
        }
    }
}

impl<W: Widget<M>, M: Clone + std::fmt::Debug + Send> Widget<M> for CornerRadius<W, M> {
    fn key(&self) -> Option<&str> {
        self.child.key()
    }

    fn layout(&mut self, ctx: &mut LayoutContext, widget_ctx: &mut WidgetContext) -> LayoutNodeId {
        self.child.layout(ctx, widget_ctx)
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
        // Push radius onto context stack
        renderer.push_corner_radius(self.radius);

        // Draw child with radius context set
        self.child.draw(ctx, node, renderer, offset, focused_id, cursor_blink, widget_ctx);

        // Pop radius from context stack
        renderer.pop_corner_radius();
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
        // Pass original offset since child will add its own layout offset
        self.child.on_event(ctx, node, offset, event, focused_id, widget_ctx)
    }
}