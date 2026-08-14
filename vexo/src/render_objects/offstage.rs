//! Render object for Offstage — hides a child subtree while keeping it mounted.
//!
//! When `offstage == true`:
//! - `layout()` creates a zero-size Taffy leaf node (child not linked into layout)
//! - `children()` returns `&[]` so the painter, hit tester, and layouter skip the child
//! - The child element and its render object remain registered in their registries,
//!   so all state (ComponentState, focus, TextEditingControllers) is preserved.
//! - The child's own Taffy node persists in the engine (orphan cleanup only happens
//!   on render-object removal, not on `children()` filtering).
//!
//! When `offstage == false`:
//! - `layout()` is pass-through: stores the child's node in `child_layout_node`,
//!   creates no Taffy node of its own, `is_pass_through() == true`
//! - `children()` returns `&[child]` so the pipeline traverses it normally
//!
//! This matches Flutter's `Offstage` widget semantics.

use std::any::Any;

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{Layout, LayoutNodeKey};
use crate::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey,
};

/// Render object for Offstage — hides its child subtree when `offstage` is true.
///
/// The child element stays mounted (state preserved); only layout, paint, and
/// hit-testing are skipped. See module docs for details.
pub struct OffstageRenderObject {
    /// Whether the child is offstage (hidden).
    offstage: bool,

    /// Child render object ID.
    child: Option<RenderObjectKey>,

    /// Computed bounds from layout.
    computed_bounds: Option<Bounds<Logical>>,

    owned_node: Option<LayoutNodeKey>,
    child_layout_node: Option<LayoutNodeKey>,
}

impl OffstageRenderObject {
    /// Create a new offstage render object.
    pub fn new(offstage: bool) -> Self {
        Self {
            offstage,
            child: None,
            computed_bounds: None,
            owned_node: None,
            child_layout_node: None,
        }
    }

    /// Set the offstage flag. Returns true if it changed.
    pub fn set_offstage(&mut self, offstage: bool) -> bool {
        if self.offstage != offstage {
            self.offstage = offstage;
            true
        } else {
            false
        }
    }

    /// Whether the child is currently offstage.
    pub fn is_offstage(&self) -> bool {
        self.offstage
    }
}

impl RenderObject for OffstageRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        if self.offstage {
            // Offstage: zero-size leaf. Child NOT linked into layout.
            let leaf_layout = Layout::default().width(0.0).height(0.0);
            match self.owned_node {
                Some(existing) => {
                    ctx.engine().set_style(existing, &leaf_layout);
                    ctx.engine().set_children(existing, &[]);
                    self.child_layout_node = None;
                    LayoutResult {
                        node: existing,
                        size: Size::zero(),
                    }
                }
                None => {
                    let node = ctx.engine().create_container(&leaf_layout, &[]);
                    self.owned_node = Some(node);
                    self.child_layout_node = None;
                    LayoutResult {
                        node,
                        size: Size::zero(),
                    }
                }
            }
        } else {
            // Onstage: pass-through. Transition cleanup if coming from offstage.
            if let Some(old_owned) = self.owned_node.take() {
                ctx.engine().remove_node(old_owned);
            }
            let child_node = child_nodes.first().copied().expect(
                "pass-through render object requires a child widget; \
                 Offstage always has a child per its constructor",
            );
            self.child_layout_node = Some(child_node);
            LayoutResult {
                node: child_node,
                size: Size::zero(),
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        let node = if self.offstage {
            self.owned_node
        } else {
            self.child_layout_node
        };
        if let Some(node) = node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        !self.offstage
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        if self.offstage {
            &[]
        } else {
            match &self.child {
                Some(child) => std::slice::from_ref(child),
                None => &[],
            }
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
        self.child_layout_node = None;
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
            self.child_layout_node = None;
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        if self.offstage {
            self.owned_node
        } else {
            self.child_layout_node
        }
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutEngine, TaffyLayoutEngine};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_offstage_render_object_initial() {
        let ro = OffstageRenderObject::new(true);
        assert!(ro.is_offstage());
        assert_eq!(ro.children(), &[] as &[RenderObjectKey]);
    }

    #[test]
    fn test_offstage_render_object_onstage() {
        let mut ro = OffstageRenderObject::new(false);
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let child = sm.insert(());
        ro.set_child_id(child);

        assert!(!ro.is_offstage());
        assert_eq!(ro.children(), &[child]);
    }

    #[test]
    fn test_offstage_set_offstage_flag() {
        let mut ro = OffstageRenderObject::new(false);
        assert!(ro.set_offstage(true));
        assert!(ro.is_offstage());
        assert!(!ro.set_offstage(true)); // no change

        assert!(ro.set_offstage(false));
        assert!(!ro.is_offstage());
    }

    #[test]
    fn test_offstage_children_switches_with_flag() {
        let mut ro = OffstageRenderObject::new(false);
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let child = sm.insert(());
        ro.set_child_id(child);

        // Onstage: child visible
        assert_eq!(ro.children(), &[child]);

        // Offstage: child hidden
        ro.set_offstage(true);
        assert_eq!(ro.children(), &[] as &[RenderObjectKey]);

        // Back onstage: child visible again
        ro.set_offstage(false);
        assert_eq!(ro.children(), &[child]);
    }

    #[test]
    fn test_offstage_layout_offstage_creates_zero_node() {
        let mut ro = OffstageRenderObject::new(true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = ro.layout(&mut ctx, &[]);

        assert!(ro.layout_node().is_some());
        assert_eq!(ro.layout_node(), Some(result.node));
        assert_eq!(result.size, Size::zero());
    }

    #[test]
    fn test_offstage_layout_onstage_passes_child() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };

        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        let result = ro.layout(&mut ctx, &[child_node]);

        // Onstage: pass-through. layout_node() returns the child's node.
        assert_eq!(ro.layout_node(), Some(child_node));
        assert_eq!(result.node, child_node);
    }

    #[test]
    fn test_offstage_onstage_is_pass_through() {
        let ro = OffstageRenderObject::new(false);
        assert!(ro.is_pass_through());
    }

    #[test]
    fn test_offstage_offstage_is_not_pass_through() {
        let ro = OffstageRenderObject::new(true);
        assert!(!ro.is_pass_through());
    }

    #[test]
    fn test_offstage_onstage_layout_stores_child_node_no_owned_node() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };

        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        ro.layout(&mut ctx, &[child_node]);

        assert_eq!(ro.layout_node(), Some(child_node));
    }

    #[test]
    fn test_offstage_onstage_apply_layout_reads_child_bounds() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let node = ctx
                .engine()
                .create_leaf(&Layout::default().width(70.0).height(35.0));
            ro.layout(&mut ctx, &[node]);
            node
        };

        engine.compute(child_node, Size::new(200.0, 200.0), &mut font_system);

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.apply_layout(&mut ctx);
        }

        let bounds = ro.computed_bounds().expect("onstage should have bounds");
        assert_eq!(bounds.width(), 70.0);
        assert_eq!(bounds.height(), 35.0);
    }

    #[test]
    fn test_offstage_flag_flip_onstage_to_offstage() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };

        // Start onstage (pass-through)
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[child_node]);
        }
        assert_eq!(ro.layout_node(), Some(child_node));
        assert!(ro.is_pass_through());

        // Flip to offstage
        ro.set_offstage(true);
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[child_node]);
        }

        // Offstage: owns a zero-size leaf, does NOT report child's node
        let owned = ro.layout_node().expect("offstage should own a leaf node");
        assert!(!ro.is_pass_through());

        // The child's node must still exist in the engine (Offstage didn't remove it)
        engine.compute(owned, Size::new(100.0, 100.0), &mut font_system);
        assert!(
            engine.get_layout(child_node).is_some(),
            "child's node must still exist after onstage->offstage flip"
        );
    }

    #[test]
    fn test_offstage_flag_flip_offstage_to_onstage_removes_owned_node() {
        let mut ro = OffstageRenderObject::new(true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Start offstage (owns zero-size leaf)
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[]);
        }
        let offstage_node = ro.layout_node().expect("offstage should own a node");
        assert!(!ro.is_pass_through());

        // Flip to onstage (pass-through)
        ro.set_offstage(false);
        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let node = ctx
                .engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0));
            ro.layout(&mut ctx, &[node]);
            node
        };

        // Onstage: reports child's node, old owned node is gone
        assert_eq!(ro.layout_node(), Some(child_node));
        assert!(ro.is_pass_through());

        // The old offstage leaf node should be removed from the engine.
        // After removal, get_layout returns None.
        assert!(
            engine.get_layout(offstage_node).is_none(),
            "old offstage leaf node should be removed after offstage->onstage flip"
        );
    }

    #[test]
    #[should_panic(expected = "pass-through render object requires a child")]
    fn test_offstage_onstage_layout_no_child_panics() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        ro.layout(&mut ctx, &[]);
    }
}
