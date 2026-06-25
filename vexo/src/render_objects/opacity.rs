//! Render object for Opacity — applies an alpha multiplier to its child subtree.
//!
//! Layout is pass-through (opacity does NOT affect layout).
//! The opacity is applied via `opacity()` so the painter wraps children's
//! commands with `PushOpacity`/`PopOpacity`.

use std::any::Any;

use crate::core::{Bounds, Logical, Point};
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey};
use crate::{
    LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey,
};

/// Render object for Opacity — applies an alpha multiplier to its child subtree.
///
/// Layout is pass-through (opacity does NOT affect layout).
/// The opacity value is exposed via `opacity()` so the pipeline can wrap
/// children's paint commands with `PushOpacity`/`PopOpacity`.
pub struct OpacityRenderObject {
    /// The opacity value (0.0 = fully transparent, 1.0 = fully opaque).
    opacity: f32,

    /// Child render object ID.
    child: Option<RenderObjectKey>,

    /// Computed bounds from layout.
    computed_bounds: Option<Bounds<Logical>>,

    /// Layout node in Taffy.
    layout_node: Option<LayoutNodeKey>,
}

impl OpacityRenderObject {
    /// Create a new opacity render object.
    pub fn new(opacity: f32) -> Self {
        Self {
            opacity,
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }

    /// Set the opacity value.
    /// Returns true if it changed.
    pub fn set_opacity(&mut self, opacity: f32) -> bool {
        if (self.opacity - opacity).abs() > f32::EPSILON {
            self.opacity = opacity;
            true
        } else {
            false
        }
    }
}

impl RenderObject for OpacityRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // Pass-through layout: opacity does NOT affect layout.
        // The child occupies its original space regardless of the opacity.
        let layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch);

        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &layout);
                ctx.engine().set_children(existing, child_nodes);
                LayoutResult { node: existing, size: crate::core::Size::zero() }
            }
            None => {
                let node = ctx.engine().create_container(&layout, child_nodes);
                self.layout_node = Some(node);
                LayoutResult { node, size: crate::core::Size::zero() }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        // The opacity is applied via opacity(), not via paint commands.
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &crate::HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
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

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn opacity(&self) -> Option<f32> {
        Some(self.opacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opacity_render_object_opacity() {
        let ro = OpacityRenderObject::new(0.5);
        assert_eq!(ro.opacity(), Some(0.5));
    }

    #[test]
    fn test_opacity_render_object_zero() {
        let ro = OpacityRenderObject::new(0.0);
        assert_eq!(ro.opacity(), Some(0.0));
    }

    #[test]
    fn test_opacity_render_object_full() {
        let ro = OpacityRenderObject::new(1.0);
        assert_eq!(ro.opacity(), Some(1.0));
    }

    #[test]
    fn test_opacity_render_object_set_opacity() {
        let mut ro = OpacityRenderObject::new(0.5);
        assert!(ro.set_opacity(0.7));
        assert_eq!(ro.opacity(), Some(0.7));
        assert!(!ro.set_opacity(0.7)); // no change
    }
}
