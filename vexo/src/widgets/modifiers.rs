use crate::renderer::UiBatcher;
use crate::utils::{Logical, Point, Size};
use crate::widgets::{Widget, WidgetContext, WidgetId, WidgetResponse};
use crate::Color;
use std::marker::PhantomData;
use taffy::prelude::{auto, length, NodeId};
use taffy::{Rect as TaffyRect, Size as TaffySize, Style};
use winit::event::WindowEvent;

/// Extension trait providing SwiftUI-style modifier chaining.
///
/// All widgets automatically implement this trait via blanket impl.
pub trait WidgetExt<M: Clone + std::fmt::Debug + Send>: Widget<M> + Sized {
    /// Add uniform padding around the widget.
    fn padding(self, amount: f32) -> Padding<Self, M> {
        Padding::uniform(self, amount)
    }

    /// Add horizontal padding (left and right).
    fn padding_horizontal(self, amount: f32) -> Padding<Self, M> {
        Padding::horizontal(self, amount)
    }

    /// Add vertical padding (top and bottom).
    fn padding_vertical(self, amount: f32) -> Padding<Self, M> {
        Padding::vertical(self, amount)
    }

    /// Add asymmetric padding with specific values for each side.
    fn padding_each(self, left: f32, right: f32, top: f32, bottom: f32) -> Padding<Self, M> {
        Padding::new(self, left, right, top, bottom)
    }

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
}

// Blanket implementation: all Widget types get WidgetExt methods
impl<M: Clone + std::fmt::Debug + Send, W: Widget<M>> WidgetExt<M> for W {}

// ============================================================================
// Padding Modifier
// ============================================================================

/// Adds padding around a child widget using Taffy layout.
pub struct Padding<W, M> {
    child: W,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    _marker: PhantomData<M>,
}

impl<W, M: Clone + std::fmt::Debug + Send> Padding<W, M> {
    pub fn new(child: W, left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            child,
            left,
            right,
            top,
            bottom,
            _marker: PhantomData,
        }
    }

    pub fn uniform(child: W, amount: f32) -> Self {
        Self::new(child, amount, amount, amount, amount)
    }

    pub fn horizontal(child: W, amount: f32) -> Self {
        Self::new(child, amount, amount, 0.0, 0.0)
    }

    pub fn vertical(child: W, amount: f32) -> Self {
        Self::new(child, 0.0, 0.0, amount, amount)
    }
}

impl<W: Widget<M>, M: Clone + std::fmt::Debug + Send> Widget<M> for Padding<W, M> {
    fn key(&self) -> Option<&str> {
        self.child.key()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        // Layout child first
        ctx.push_index(1);
        let child_node = self.child.layout(taffy, ctx);
        ctx.pop();

        // Create padding wrapper node
        let node = taffy
            .new_with_children(
                Style {
                    padding: TaffyRect {
                        left: length(self.left),
                        right: length(self.right),
                        top: length(self.top),
                        bottom: length(self.bottom),
                    },
                    size: TaffySize {
                        width: auto(),
                        height: auto(),
                    },
                    ..Default::default()
                },
                &[child_node],
            )
            .unwrap();

        ctx.record_node_widget(node);
        node
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let pos = Point::<Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        // Draw child at offset (padding is handled by Taffy layout)
        let child_ids = taffy.children(node).unwrap();
        if let Some(child_node) = child_ids.get(0) {
            self.child.draw(taffy, *child_node, renderer, pos, focused_id, ctx);
        }
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: Point<Logical>,
        event: &WindowEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        let layout = taffy.layout(node).unwrap();
        let pos = Point::<Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        // Delegate to child
        let child_ids = taffy.children(node).unwrap();
        if let Some(child_node) = child_ids.get(0) {
            return self.child.on_event(taffy, *child_node, pos, event, focused_id, ctx);
        }

        WidgetResponse::default()
    }
}

// ============================================================================
// Placeholder structs for Background, Border, CornerRadius
// (Will be implemented in Task 2)
// ============================================================================

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

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        // Layout child, background uses same bounds
        self.child.layout(taffy, ctx)
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let pos = Point::<Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );
        let size = Size::<Logical>::new(layout.size.width, layout.size.height);

        // Draw background rect first (behind child)
        renderer.add_rect(pos.to_array(), size.to_array(), self.color, Color::TRANSPARENT, 0.0, 0.0);

        // Draw child on top
        self.child.draw(taffy, node, renderer, pos, focused_id, ctx);
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: Point<Logical>,
        event: &WindowEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        let layout = taffy.layout(node).unwrap();
        let pos = Point::<Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );
        self.child.on_event(taffy, node, pos, event, focused_id, ctx)
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

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        // Layout child, border uses same bounds
        self.child.layout(taffy, ctx)
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let pos = Point::<Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );
        let size = Size::<Logical>::new(layout.size.width, layout.size.height);

        // Draw child first
        self.child.draw(taffy, node, renderer, pos, focused_id, ctx);

        // Draw border on top (transparent fill, colored border)
        renderer.add_rect(pos.to_array(), size.to_array(), Color::TRANSPARENT, self.color, self.width, 0.0);
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: Point<Logical>,
        event: &WindowEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        let layout = taffy.layout(node).unwrap();
        let pos = Point::<Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );
        self.child.on_event(taffy, node, pos, event, focused_id, ctx)
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

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        self.child.layout(taffy, ctx)
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let pos = Point::<Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );
        self.child.draw(taffy, node, renderer, pos, focused_id, ctx);
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: Point<Logical>,
        event: &WindowEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        let layout = taffy.layout(node).unwrap();
        let pos = Point::<Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );
        self.child.on_event(taffy, node, pos, event, focused_id, ctx)
    }
}
