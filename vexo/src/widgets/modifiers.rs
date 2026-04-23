use crate::core::{Logical, Point, Size};
use crate::layout::{LayoutContext, LayoutNodeId, LayoutView};
use crate::renderer::UiBatcher;
use crate::widgets::{Widget, WidgetContext, WidgetId, WidgetResponse};
use crate::input::InputEvent;
use crate::render::RenderCommand;
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
    computed_layout: Option<crate::testable::ComputedLayout>,
}

impl<W, M: Clone + std::fmt::Debug + Send> Background<W, M> {
    pub fn new(child: W, color: Color) -> Self {
        Self {
            child,
            color,
            _marker: PhantomData,
            computed_layout: None,
        }
    }
}

// Identifiable implementation
impl<W: crate::testable::Identifiable, M> crate::testable::Identifiable for Background<W, M> {
    fn id(&self) -> Option<crate::core::WidgetId> {
        self.child.id()
    }
}

// Layout implementation
impl<W: crate::testable::Layout, M> crate::testable::Layout for Background<W, M> {
    fn constraints(&self) -> crate::testable::LayoutConstraints {
        self.child.constraints()
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
        self.child.apply_layout(layout);
    }
}

// Paint implementation
impl<W: crate::testable::Paint, M> crate::testable::Paint for Background<W, M> {
    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<crate::render::RenderCommand> {
        use crate::render::RenderCommand;

        let layout = match &self.computed_layout {
            Some(l) => l,
            None => return Vec::new(),
        };

        let mut commands = Vec::new();

        // Background rect first (behind child)
        let pos = crate::core::Point::new(ctx.offset().x + layout.x(), ctx.offset().y + layout.y());
        let size = crate::core::Size::new(layout.width(), layout.height());
        let bounds = crate::core::Rect::new(pos, size);
        commands.push(RenderCommand::rect(bounds, self.color.into()));

        // Then child
        commands.extend(self.child.paint(ctx));

        commands
    }
}

// Interact implementation
impl<W: crate::testable::Interact<M>, M: Clone + std::fmt::Debug + Send> crate::testable::Interact<M> for Background<W, M> {
    fn on_event(
        &mut self,
        event: &crate::input::InputEvent,
        ctx: &crate::testable::InteractionContext,
    ) -> crate::testable::InteractionResponse<M> {
        self.child.on_event(event, ctx)
    }
}

// Legacy Widget trait implementation for backwards compatibility
impl<W: Widget<M> + crate::testable::Paint, M: Clone + std::fmt::Debug + Send> Widget<M> for Background<W, M> {
    fn key(&self) -> Option<&str> {
        self.child.key()
    }

    fn cursor(&self) -> crate::input::CursorIcon {
        self.child.cursor()
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeId {
        // Layout child, background uses same bounds
        self.child.layout(layout_context, widget_context)
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
        self.child.apply_layout(layout);
    }

    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        crate::testable::Paint::paint(self, ctx)
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
            let pos = Point::<Logical>::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );
            let size = Size::<Logical>::new(layout.width(), layout.height());

            // Draw background rect first (behind child)
            renderer.add_rect(pos.to_array(), size.to_array(), self.color, Color::TRANSPARENT, 0.0, 0.0);

            // Draw child on top - pass original offset since child will add its own layout offset
            self.child.draw(layout_view, node, renderer, offset, focused_id, cursor_blink, widget_context);
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
        // Pass original offset since child will add its own layout offset
        self.child.on_event(layout_view, node, offset, event, focused_id, widget_context)
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
    computed_layout: Option<crate::testable::ComputedLayout>,
}

impl<W, M: Clone + std::fmt::Debug + Send> Border<W, M> {
    pub fn new(child: W, color: Color, width: f32) -> Self {
        Self {
            child,
            color,
            width,
            _marker: PhantomData,
            computed_layout: None,
        }
    }
}

// ============================================================================
// SEPARATED TRAIT IMPLEMENTATIONS
// ============================================================================

// Identifiable implementation
impl<W: crate::testable::Identifiable, M> crate::testable::Identifiable for Border<W, M> {
    fn id(&self) -> Option<crate::core::WidgetId> {
        self.child.id()
    }
}

// Layout implementation
impl<W: crate::testable::Layout, M> crate::testable::Layout for Border<W, M> {
    fn constraints(&self) -> crate::testable::LayoutConstraints {
        self.child.constraints()
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
        self.child.apply_layout(layout);
    }
}

// Paint implementation
impl<W: crate::testable::Paint, M> crate::testable::Paint for Border<W, M> {
    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<crate::render::RenderCommand> {
        use crate::core::Rect;
        use crate::render::RenderCommand;

        let layout = match &self.computed_layout {
            Some(l) => l,
            None => return Vec::new(),
        };

        let mut commands = Vec::new();

        // Paint child first
        commands.extend(self.child.paint(ctx));

        // Then border on top
        let pos = crate::core::Point::new(ctx.offset().x + layout.x(), ctx.offset().y + layout.y());
        let size = crate::core::Size::new(layout.width(), layout.height());
        let bounds = Rect::new(pos, size);
        commands.push(RenderCommand::rect_with_border(
            bounds,
            Color::TRANSPARENT.into(),  // transparent fill
            self.color.into(),          // border color
            self.width,                 // border width
        ));

        commands
    }
}

// Interact implementation
impl<W: crate::testable::Interact<M>, M: Clone + std::fmt::Debug + Send> crate::testable::Interact<M> for Border<W, M> {
    fn on_event(
        &mut self,
        event: &crate::input::InputEvent,
        ctx: &crate::testable::InteractionContext,
    ) -> crate::testable::InteractionResponse<M> {
        self.child.on_event(event, ctx)
    }
}

// Legacy Widget trait implementation for backwards compatibility
impl<W: Widget<M> + crate::testable::Paint, M: Clone + std::fmt::Debug + Send> Widget<M> for Border<W, M> {
    fn key(&self) -> Option<&str> {
        self.child.key()
    }

    fn cursor(&self) -> crate::input::CursorIcon {
        self.child.cursor()
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeId {
        // Layout child, border uses same bounds
        self.child.layout(layout_context, widget_context)
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
        self.child.apply_layout(layout);
    }

    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        crate::testable::Paint::paint(self, ctx)
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
            let pos = Point::<Logical>::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );
            let size = Size::<Logical>::new(layout.width(), layout.height());

            // Draw child first - pass original offset since child will add its own layout offset
            self.child.draw(layout_view, node, renderer, offset, focused_id, cursor_blink, widget_context);

            // Draw border on top (transparent fill, colored border)
            renderer.add_rect(pos.to_array(), size.to_array(), Color::TRANSPARENT, self.color, self.width, 0.0);
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
        // Pass original offset since child will add its own layout offset
        self.child.on_event(layout_view, node, offset, event, focused_id, widget_context)
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
    computed_layout: Option<crate::testable::ComputedLayout>,
}

impl<W, M: Clone + std::fmt::Debug + Send> CornerRadius<W, M> {
    pub fn new(child: W, radius: f32) -> Self {
        Self {
            child,
            radius,
            _marker: PhantomData,
            computed_layout: None,
        }
    }
}

// Identifiable implementation
impl<W: crate::testable::Identifiable, M> crate::testable::Identifiable for CornerRadius<W, M> {
    fn id(&self) -> Option<crate::core::WidgetId> {
        self.child.id()
    }
}

// Layout implementation
impl<W: crate::testable::Layout, M> crate::testable::Layout for CornerRadius<W, M> {
    fn constraints(&self) -> crate::testable::LayoutConstraints {
        self.child.constraints()
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
        self.child.apply_layout(layout);
    }
}

// Paint implementation
impl<W: crate::testable::Paint, M> crate::testable::Paint for CornerRadius<W, M> {
    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<crate::render::RenderCommand> {
        use crate::render::RenderCommand;

        let mut commands = Vec::new();

        // Push corner radius
        commands.push(RenderCommand::PushCornerRadius { radius: self.radius });

        // Paint child
        commands.extend(self.child.paint(ctx));

        // Pop corner radius
        commands.push(RenderCommand::PopCornerRadius);

        commands
    }
}

// Interact implementation
impl<W: crate::testable::Interact<M>, M: Clone + std::fmt::Debug + Send> crate::testable::Interact<M> for CornerRadius<W, M> {
    fn on_event(
        &mut self,
        event: &crate::input::InputEvent,
        ctx: &crate::testable::InteractionContext,
    ) -> crate::testable::InteractionResponse<M> {
        self.child.on_event(event, ctx)
    }
}

// Legacy Widget trait implementation for backwards compatibility
impl<W: Widget<M> + crate::testable::Paint, M: Clone + std::fmt::Debug + Send> Widget<M> for CornerRadius<W, M> {
    fn key(&self) -> Option<&str> {
        self.child.key()
    }

    fn cursor(&self) -> crate::input::CursorIcon {
        self.child.cursor()
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeId {
        self.child.layout(layout_context, widget_context)
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
        self.child.apply_layout(layout);
    }

    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        crate::testable::Paint::paint(self, ctx)
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
        // Push radius onto context stack
        renderer.push_corner_radius(self.radius);

        // Draw child with radius context set
        self.child.draw(layout_view, node, renderer, offset, focused_id, cursor_blink, widget_context);

        // Pop radius from context stack
        renderer.pop_corner_radius();
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
        // Pass original offset since child will add its own layout offset
        self.child.on_event(layout_view, node, offset, event, focused_id, widget_context)
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testable::{Identifiable, Layout, Paint, PaintContext, LayoutConstraints, ComputedLayout};
    use crate::core::{Color as CoreColor, Rect, WidgetId};

    /// Test widget that implements all separated traits.
    struct TestWidget {
        id: WidgetId,
        computed_layout: Option<ComputedLayout>,
    }

    impl TestWidget {
        fn new(id: &str) -> Self {
            Self {
                id: WidgetId::from_key(id),
                computed_layout: None,
            }
        }
    }

    impl Identifiable for TestWidget {
        fn id(&self) -> Option<WidgetId> {
            Some(self.id)
        }
    }

    impl Layout for TestWidget {
        fn constraints(&self) -> LayoutConstraints {
            LayoutConstraints::fixed(100.0, 50.0)
        }

        fn apply_layout(&mut self, layout: ComputedLayout) {
            self.computed_layout = Some(layout);
        }
    }

    impl Paint for TestWidget {
        fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
            vec![]
        }
    }

    impl<M: Clone + std::fmt::Debug + Send> crate::testable::Interact<M> for TestWidget {
        fn on_event(
            &mut self,
            _event: &crate::input::InputEvent,
            _ctx: &crate::testable::InteractionContext,
        ) -> crate::testable::InteractionResponse<M> {
            crate::testable::InteractionResponse::default()
        }
    }

    #[test]
    fn test_border_implements_separated_traits() {
        let child = TestWidget::new("test-child");
        let border: Border<TestWidget, ()> = Border::new(child, Color::BLACK, 2.0);

        // Should implement Identifiable - delegates to child
        let id = border.id();
        assert!(id.is_some());

        // Should implement Layout - delegates to child
        let constraints = border.constraints();
        assert!(constraints.is_fixed_width());
        assert_eq!(constraints.min_width, 100.0);
    }

    #[test]
    fn test_border_paint_order() {
        let mut child = TestWidget::new("test-child");
        // Set up a computed layout
        child.apply_layout(ComputedLayout::new(Rect::from_xywh(0.0, 0.0, 100.0, 50.0)));

        let mut border: Border<TestWidget, ()> = Border::new(child, Color::BLACK, 2.0);
        border.computed_layout = Some(ComputedLayout::new(Rect::from_xywh(0.0, 0.0, 100.0, 50.0)));

        let mut ctx = PaintContext::default();
        let commands = border.paint(&mut ctx);

        // Should have 1 command (the border rect, since child returns empty)
        assert_eq!(commands.len(), 1);
    }

    #[test]
    fn test_background_implements_separated_traits() {
        let child = TestWidget::new("test-child");
        let background: Background<TestWidget, ()> = Background::new(child, Color::RED);

        // Should implement Identifiable - delegates to child
        let id = background.id();
        assert!(id.is_some());

        // Should implement Layout - delegates to child
        let constraints = background.constraints();
        assert!(constraints.is_fixed_width());
    }

    #[test]
    fn test_corner_radius_implements_separated_traits() {
        let child = TestWidget::new("test-child");
        let corner_radius: CornerRadius<TestWidget, ()> = CornerRadius::new(child, 10.0);

        // Should implement Identifiable - delegates to child
        let id = corner_radius.id();
        assert!(id.is_some());

        // Should implement Layout - delegates to child
        let constraints = corner_radius.constraints();
        assert!(constraints.is_fixed_width());
    }
}