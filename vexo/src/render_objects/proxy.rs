//! Pass-through render object for `StatefulElement` and `InheritedElement`.
//!
//! `StatefulElement` and `InheritedElement` don't render themselves — they
//! delegate painting to their child. `ProxyRenderObject` is a **true layout
//! pass-through**: it does NOT create a Taffy node of its own. Instead, it
//! returns the child's Taffy node from `layout()`, so the layout engine
//! links the grandparent directly to the grandchild — no intervening
//! container to size to content and break the fill chain.
//!
//! This matches `Offstage`-onstage, `Opacity`, and `FractionalTranslation`.
//!
//! - No paint commands (invisible)
//! - Bounds-based hit test (reads the shared Taffy node's computed bounds)
//! - `is_pass_through() == true` (no owned Taffy node to clean up on removal)
//!
//! # Coordinate handling in the painter / hit test
//!
//! Because the proxy shares its child's Taffy node, both the proxy and its
//! child read the *same* `computed_bounds` (origin relative to the Taffy
//! *grandparent*). The painter and hit test apply a correction for
//! pass-through ROs — subtracting `position_in_parent` when recursing into
//! children — to avoid double-counting the shared offset. See
//! `painter::paint_recursive` and `hit_test::hit_test_recursive`.

use std::any::Any;

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{Layout, LayoutNodeKey};
use crate::render::RenderCommand;
use crate::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey,
};

/// Proxy render object for `StatefulElement` and `InheritedWidget`.
///
/// See module docs for the full pass-through semantics.
pub struct ProxyRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    /// The child's Taffy node, returned to the parent so the grandparent
    /// links the grandchild directly. `None` until `layout()` runs.
    child_layout_node: Option<LayoutNodeKey>,
}

impl ProxyRenderObject {
    /// Create a new `ProxyRenderObject`.
    pub fn new() -> Self {
        Self {
            child: None,
            computed_bounds: None,
            child_layout_node: None,
        }
    }
}

impl Default for ProxyRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for ProxyRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // Pass-through: return the child's node directly. No intervening
        // container — the grandparent links the grandchild's Taffy node.
        //
        // StatefulElement and InheritedElement always have exactly one
        // child. The defensive `None` case creates a throwaway zero-size
        // leaf to avoid panicking on framework edge cases.
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

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        Vec::new()
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
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
        // Invalidate the cached child layout node — the new child has a
        // different (or not-yet-created) Taffy node. Without this, the
        // layout traversal sees `layout_node() == Some(stale)` and skips
        // `layout()`, so the new child's node is never linked into the
        // parent's Taffy tree. This is the root cause of the "avatar never
        // renders after push/pop" bug: when NetworkImage swaps its child
        // from Spacer to Image while offstage, the ProxyRenderObject's
        // stale `child_layout_node` prevents the Image from ever being
        // laid out.
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
}
