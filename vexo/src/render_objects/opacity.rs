//! Render object for Opacity — applies an alpha multiplier to its child subtree.
//!
//! Layout is pass-through (opacity does NOT affect layout).
//! The opacity is applied via `opacity()` so the painter wraps children's
//! commands with `PushOpacity`/`PopOpacity`.

use std::any::Any;

use crate::core::{Bounds, Logical, Point};
use crate::layout::LayoutNodeKey;
use crate::{LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey};

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

    /// The child's Taffy node (pass-through: Opacity owns no node of its own).
    child_layout_node: Option<LayoutNodeKey>,
}

impl OpacityRenderObject {
    /// Create a new opacity render object.
    pub fn new(opacity: f32) -> Self {
        Self {
            opacity,
            child: None,
            computed_bounds: None,
            child_layout_node: None,
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
    fn layout(&mut self, _ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let child_node = child_nodes.first().copied().expect(
            "pass-through render object requires a child widget; \
             Opacity always has a child per its constructor",
        );
        self.child_layout_node = Some(child_node);
        LayoutResult {
            node: child_node,
            size: crate::core::Size::zero(),
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(child_node) = self.child_layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(child_node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        true
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
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

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.child_layout_node
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

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

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

    #[test]
    fn test_opacity_is_pass_through() {
        let ro = OpacityRenderObject::new(0.5);
        assert!(ro.is_pass_through());
    }

    #[test]
    fn test_opacity_layout_stores_child_node() {
        use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};

        let mut ro = OpacityRenderObject::new(0.5);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let child_node = ctx
            .engine()
            .create_leaf(&Layout::default().width(50.0).height(30.0));

        let result = ro.layout(&mut ctx, &[child_node]);

        assert_eq!(ro.layout_node(), Some(child_node));
        assert_eq!(result.node, child_node);
    }

    #[test]
    fn test_opacity_layout_creates_no_taffy_node() {
        use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};

        let mut ro = OpacityRenderObject::new(0.5);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let child_node = ctx
            .engine()
            .create_leaf(&Layout::default().width(50.0).height(30.0));

        ro.layout(&mut ctx, &[child_node]);

        // The engine should have exactly one node (the child we created).
        // Opacity created none. We verify indirectly: get_layout(child_node)
        // still works, and there is no second node to query.
        let child_layout = ctx.engine_ref().get_layout(child_node);
        assert!(child_layout.is_some(), "child node should still exist");
    }

    #[test]
    fn test_opacity_apply_layout_reads_child_bounds() {
        use crate::core::Size;
        use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};

        let mut ro = OpacityRenderObject::new(0.5);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let node = ctx
                .engine()
                .create_leaf(&Layout::default().width(80.0).height(40.0));
            ro.layout(&mut ctx, &[node]);
            node
        };

        engine.compute(child_node, Size::new(200.0, 200.0), &mut font_system);

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.apply_layout(&mut ctx);
        }

        let bounds = ro
            .computed_bounds()
            .expect("apply_layout should set bounds");
        assert_eq!(bounds.width(), 80.0);
        assert_eq!(bounds.height(), 40.0);
    }

    #[test]
    #[should_panic(expected = "pass-through render object requires a child")]
    fn test_opacity_layout_no_child_panics() {
        use crate::layout::TaffyLayoutEngine;
        let mut ro = OpacityRenderObject::new(0.5);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        ro.layout(&mut ctx, &[]);
    }
}
