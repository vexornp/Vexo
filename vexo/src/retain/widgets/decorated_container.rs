//! Decorated container widget - applies visual decorations to a child.
//!
//! This widget bundles multiple decorations (background, border, corner radius)
//! into a single element and render object, reducing overhead compared to
//! chaining multiple modifier widgets.

use std::any::Any;

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position, Size};
use crate::layout::{Layout, LayoutNodeId};
use crate::render::RenderCommand;
use crate::retain::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext,
    RenderObject, RenderObjectId,
};
use crate::retain::style::Style;

// ============================================================================
// DecoratedContainerRenderObject
// ============================================================================

/// Render object for DecoratedContainer - handles all decorations in a single pass.
///
/// This render object paints background, border, and corner radius together,
/// avoiding the overhead of multiple nested render objects.
pub struct DecoratedContainerRenderObject {
    /// Current style configuration.
    style: Style,

    /// Child render object ID.
    child: Option<RenderObjectId>,

    /// Computed bounds from layout.
    computed_bounds: Option<Bounds<Logical>>,

    /// Layout node in Taffy.
    layout_node: Option<LayoutNodeId>,
}

impl DecoratedContainerRenderObject {
    /// Create a new decorated container render object with the given style.
    pub fn new(style: Style) -> Self {
        Self {
            style,
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }

    /// Set the style configuration.
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    /// Get the current style.
    pub fn style(&self) -> &Style {
        &self.style
    }
}

impl RenderObject for DecoratedContainerRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeId]) -> LayoutResult {
        // DecoratedContainer is a pass-through for layout - uses child's bounds
        match child_nodes.first() {
            Some(child_node) => {
                self.layout_node = Some(*child_node);
                LayoutResult {
                    node: *child_node,
                    size: Size::zero(),
                }
            }
            None => {
                // No child, create empty leaf
                let node = ctx.engine().create_leaf(&Layout::default());
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::zero(),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        let bounds = match &self.computed_bounds {
            Some(b) => b,
            None => return vec![],
        };

        let mut commands = Vec::new();
        let pos: Position<Logical, Absolute> = ctx.absolute_position();

        let absolute_bounds = Bounds::new(
            pos.x,
            pos.y,
            pos.x + bounds.width(),
            pos.y + bounds.height(),
        );

        // 1. Push corner radius if set (affects all subsequent rects)
        if let Some(ref cr) = self.style.corner_radius {
            commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
        }

        // 2. Draw background first (behind child)
        if let Some(bg_color) = self.style.background {
            commands.push(RenderCommand::rect(absolute_bounds, bg_color));
        }

        // 3. Draw border on top (after background)
        if let Some(ref border) = self.style.border {
            commands.push(RenderCommand::rect_with_border(
                absolute_bounds,
                Color::TRANSPARENT,
                border.color,
                border.width,
            ));
        }

        // 4. Pop corner radius
        if self.style.corner_radius.is_some() {
            commands.push(RenderCommand::PopCornerRadius);
        }

        commands
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectId] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectId) {
        self.child = Some(child);
    }

    fn layout_node(&self) -> Option<LayoutNodeId> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}
