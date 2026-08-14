//! DecoratedBoxRenderObject: a true pass-through proxy that paints `Style`.
//!
//! Like `ProxyRenderObject`, this render object does NOT own a Taffy node —
//! `layout()` returns the child's node so the grandparent links the
//! grandchild directly. Unlike `ProxyRenderObject`, this RO additionally
//! paints `Style` (background, border, corner radius, shadows) against its
//! `computed_bounds` (which equals the child's bounds, since they share the
//! Taffy node).
//!
//! `is_pass_through() == true` tells the painter / hit-tester to apply the
//! pass-through coordinate correction (subtract `position_in_parent` when
//! recursing into the child) and tells `RenderObjectRegistry::remove()` to
//! skip orphan-node cleanup. See `ProxyRenderObject` docstring in
//! `crate::stateful_widget` for the full rationale.

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{Layout, LayoutNodeKey};
use crate::render::RenderCommand;
use crate::render_object::{HitTestContext, LayoutContext, PaintContext, RenderObject};
use crate::style::Style;
use crate::{LayoutResult, RenderObjectKey};

/// Render object for `DecoratedBox`. True pass-through proxy that paints `Style`.
///
/// See module docs for details.
pub struct DecoratedBoxRenderObject {
    child: Option<RenderObjectKey>,
    style: Style,
    computed_bounds: Option<Bounds<Logical>>,
    /// The child's Taffy node, returned to the parent so the grandparent
    /// links the grandchild directly. `None` until `layout()` runs.
    child_layout_node: Option<LayoutNodeKey>,
}

impl DecoratedBoxRenderObject {
    /// Create a new `DecoratedBoxRenderObject` with the given style.
    pub fn new(style: Style) -> Self {
        Self {
            child: None,
            style,
            computed_bounds: None,
            child_layout_node: None,
        }
    }

    /// Set the style, returning `true` if it changed.
    ///
    /// Used by `Widget::update_render_object()` to detect whether a paint
    /// invalidation is needed.
    pub fn set_style(&mut self, style: Style) -> bool {
        if self.style != style {
            self.style = style;
            true
        } else {
            false
        }
    }

    /// Get the style (read accessor, used by tests).
    pub fn style(&self) -> &Style {
        &self.style
    }
}

impl Default for DecoratedBoxRenderObject {
    fn default() -> Self {
        Self::new(Style::default())
    }
}

impl RenderObject for DecoratedBoxRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // Pass-through: return the child's node directly. No intervening
        // container — the grandparent links the grandchild's Taffy node.
        //
        // DecoratedBox always has exactly one child. The defensive `None`
        // case creates a throwaway zero-size leaf to avoid panicking on
        // framework edge cases. Mirrors `ProxyRenderObject::layout()`.
        match child_nodes.first() {
            Some(&child_node) => {
                self.child_layout_node = Some(child_node);
                LayoutResult {
                    node: child_node,
                    size: Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_leaf(&Layout::default());
                self.child_layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::zero(),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.child_layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        true
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        let bounds = match self.computed_bounds {
            Some(b) => b,
            None => return Vec::new(),
        };
        crate::painter::paint_style(&self.style, bounds, ctx)
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match self.computed_bounds {
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
        self.child_layout_node = None;
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
            self.child_layout_node = None;
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.child_layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        if self.style.clip {
            self.computed_bounds
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bounds;
    use crate::core::Color;
    use crate::layout::TaffyLayoutEngine;
    use crate::style::BoxShadow;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_decorated_box_layout_returns_child_node() {
        let mut ro = DecoratedBoxRenderObject::new(Style::default());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        // Create a child Taffy node the way the pipeline would: by calling
        // engine.create_leaf and passing the key as a child_nodes entry.
        let child_node = ctx
            .engine()
            .create_leaf(&Layout::default().width(50.0).height(50.0));
        let result = ro.layout(&mut ctx, &[child_node]);

        assert_eq!(
            result.node, child_node,
            "layout() must return the child's node"
        );
        assert_eq!(
            ro.layout_node(),
            Some(child_node),
            "layout_node() must return the child's node after layout()"
        );
    }

    #[test]
    fn test_decorated_box_layout_no_child_creates_throwaway_node() {
        let mut ro = DecoratedBoxRenderObject::new(Style::default());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = ro.layout(&mut ctx, &[]);

        // Should not panic; should return some node and store it.
        assert!(ro.layout_node().is_some());
        assert_eq!(ro.layout_node(), Some(result.node));
    }

    #[test]
    fn test_decorated_box_is_pass_through() {
        let ro = DecoratedBoxRenderObject::new(Style::default());
        assert!(
            ro.is_pass_through(),
            "DecoratedBoxRenderObject must be pass-through"
        );
    }

    #[test]
    fn test_decorated_box_paint_no_bounds_returns_empty() {
        let ro = DecoratedBoxRenderObject::new(Style::new().background(Color::RED));
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert!(
            cmds.is_empty(),
            "paint() with no computed_bounds must return empty"
        );
    }

    #[test]
    fn test_decorated_box_paint_with_background_emits_one_command() {
        let mut ro = DecoratedBoxRenderObject::new(Style::new().background(Color::RED));
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert_eq!(cmds.len(), 1, "background only → 1 command");
    }

    #[test]
    fn test_decorated_box_paint_with_background_and_border() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);
        let mut ro = DecoratedBoxRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert_eq!(cmds.len(), 2, "background + border → 2 commands");
    }

    #[test]
    fn test_decorated_box_paint_with_corner_radius() {
        let style = Style::new().background(Color::RED).corner_radius(8.0);
        let mut ro = DecoratedBoxRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert_eq!(
            cmds.len(),
            3,
            "push radius + background + pop radius → 3 commands"
        );
    }

    #[test]
    fn test_decorated_box_paint_empty_style() {
        let mut ro = DecoratedBoxRenderObject::new(Style::new());
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert!(cmds.is_empty(), "empty style → 0 commands");
    }

    #[test]
    fn test_decorated_box_paint_with_shadow() {
        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK).blur(8.0));
        let mut ro = DecoratedBoxRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        // shadow + background = 2 (no corner radius)
        assert_eq!(cmds.len(), 2, "shadow + background → 2 commands");
    }

    #[test]
    fn test_decorated_box_set_style_change_detection() {
        let mut ro = DecoratedBoxRenderObject::new(Style::default());

        // Setting the same default style → no change
        assert!(!ro.set_style(Style::default()));

        // Setting a different style → change
        let style = Style::new().background(Color::RED);
        assert!(ro.set_style(style.clone()));

        // Setting the same style again → no change
        assert!(!ro.set_style(style));
    }

    #[test]
    fn test_decorated_box_clip_bounds_no_clip() {
        let ro = DecoratedBoxRenderObject::new(Style::new());
        assert!(
            ro.clip_bounds().is_none(),
            "no clip → clip_bounds() is None"
        );
    }

    #[test]
    fn test_decorated_box_clip_bounds_with_clip_no_bounds() {
        let ro = DecoratedBoxRenderObject::new(Style::new().clip());
        assert!(
            ro.clip_bounds().is_none(),
            "clip set but no computed_bounds → clip_bounds() is None"
        );
    }

    #[test]
    fn test_decorated_box_clip_bounds_with_clip_and_bounds() {
        let mut ro = DecoratedBoxRenderObject::new(Style::new().clip());
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        assert_eq!(
            ro.clip_bounds(),
            Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0)),
            "clip + bounds → clip_bounds() returns the bounds"
        );
    }

    #[test]
    fn test_decorated_box_hit_test_no_bounds() {
        let ro = DecoratedBoxRenderObject::new(Style::default());
        assert!(
            !ro.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()),
            "no bounds → hit_test false"
        );
    }

    #[test]
    fn test_decorated_box_hit_test_inside_bounds() {
        let mut ro = DecoratedBoxRenderObject::new(Style::default());
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        assert!(
            ro.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()),
            "point inside bounds → hit_test true"
        );
    }

    #[test]
    fn test_decorated_box_hit_test_outside_bounds() {
        let mut ro = DecoratedBoxRenderObject::new(Style::default());
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        assert!(
            !ro.hit_test(Point::new(200.0, 200.0), &HitTestContext::mock()),
            "point outside bounds → hit_test false"
        );
    }
}
