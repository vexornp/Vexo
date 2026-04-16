use crate::renderer::UiBatcher;
use crate::utils::{Logical, Point, Size};
use crate::widgets::{Widget, WidgetContext, WidgetId, WidgetResponse};
use crate::Color;
use std::marker::PhantomData;
use taffy::prelude::{auto, length, NodeId};
use taffy::{Rect as TaffyRect, Size as TaffySize, Style};
use winit::event::WindowEvent;

/// Frame size constraint - can be fixed, flexible, or auto.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameSize {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
}

impl FrameSize {
    /// Fixed width and height.
    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            width: Some(width),
            height: Some(height),
            ..Default::default()
        }
    }

    /// Fixed width only, height auto.
    pub fn width(width: f32) -> Self {
        Self {
            width: Some(width),
            ..Default::default()
        }
    }

    /// Fixed height only, width auto.
    pub fn height(height: f32) -> Self {
        Self {
            height: Some(height),
            ..Default::default()
        }
    }
}

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

    /// Apply fixed size constraints to the widget.
    ///
    /// # Examples
    /// ```
    /// // Fixed width and height
    /// text!("Hello").frame(100.0, 50.0)
    ///
    /// // Fixed width only
    /// text!("Hello").frame_width(200.0)
    ///
    /// // Fixed height only
    /// text!("Hello").frame_height(30.0)
    /// ```
    fn frame(self, width: f32, height: f32) -> Frame<Self, M> {
        Frame::new(self, FrameSize::fixed(width, height))
    }

    /// Apply fixed width, height is auto-sized.
    fn frame_width(self, width: f32) -> Frame<Self, M> {
        Frame::new(self, FrameSize::width(width))
    }

    /// Apply fixed height, width is auto-sized.
    fn frame_height(self, height: f32) -> Frame<Self, M> {
        Frame::new(self, FrameSize::height(height))
    }

    /// Apply frame with full constraints.
    fn frame_with(self, constraints: FrameSize) -> Frame<Self, M> {
        Frame::new(self, constraints)
    }

    /// Box this widget for use in containers.
    ///
    /// Allows modifiers to be chained without manual `Box::new()` wrapping:
    /// ```
    /// text!("Hello")
    ///     .padding(10.0)
    ///     .background(Color::RED)
    ///     .boxed()
    /// ```
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
        let child_node = self.child.layout(taffy, ctx);

        taffy
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
            .unwrap()
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

        // Draw child on top - pass original offset since child will add its own layout.location
        self.child.draw(taffy, node, renderer, offset, focused_id, ctx);
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
        // Pass original offset since child will add its own layout.location
        self.child.on_event(taffy, node, offset, event, focused_id, ctx)
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

        // Draw child first - pass original offset since child will add its own layout.location
        self.child.draw(taffy, node, renderer, offset, focused_id, ctx);

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
        // Pass original offset since child will add its own layout.location
        self.child.on_event(taffy, node, offset, event, focused_id, ctx)
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
        // CornerRadius just delegates to child - radius is for future clipping implementation
        self.child.draw(taffy, node, renderer, offset, focused_id, ctx);
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
        // Pass original offset since child will add its own layout.location
        self.child.on_event(taffy, node, offset, event, focused_id, ctx)
    }
}

// ============================================================================
// Frame Modifier
// ============================================================================

/// Applies size constraints to a child widget.
///
/// Unlike SwiftUI's frame which can also handle alignment within a larger frame,
/// this modifier focuses on size constraints via Taffy's layout system.
pub struct Frame<W, M> {
    child: W,
    constraints: FrameSize,
    _marker: PhantomData<M>,
}

impl<W, M: Clone + std::fmt::Debug + Send> Frame<W, M> {
    pub fn new(child: W, constraints: FrameSize) -> Self {
        Self {
            child,
            constraints,
            _marker: PhantomData,
        }
    }
}

impl<W: Widget<M>, M: Clone + std::fmt::Debug + Send> Widget<M> for Frame<W, M> {
    fn key(&self) -> Option<&str> {
        self.child.key()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let child_node = self.child.layout(taffy, ctx);

        taffy
            .new_with_children(
                Style {
                    display: taffy::prelude::Display::Flex,
                    align_items: Some(taffy::prelude::AlignItems::Stretch),
                    justify_content: Some(taffy::prelude::JustifyContent::Center),
                    size: TaffySize {
                        width: self.constraints.width.map(length).unwrap_or(auto()),
                        height: self.constraints.height.map(length).unwrap_or(auto()),
                    },
                    min_size: TaffySize {
                        width: self.constraints.min_width.map(length).unwrap_or(auto()),
                        height: self.constraints.min_height.map(length).unwrap_or(auto()),
                    },
                    max_size: TaffySize {
                        width: self.constraints.max_width.map(length).unwrap_or(auto()),
                        height: self.constraints.max_height.map(length).unwrap_or(auto()),
                    },
                    ..Default::default()
                },
                &[child_node],
            )
            .unwrap()
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

        // Draw child at offset (frame size is handled by Taffy layout)
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

        let child_ids = taffy.children(node).unwrap();
        if let Some(child_node) = child_ids.get(0) {
            return self.child.on_event(taffy, *child_node, pos, event, focused_id, ctx);
        }

        WidgetResponse::default()
    }
}
