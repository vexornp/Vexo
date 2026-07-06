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
//! - `layout()` creates a pass-through Taffy container (Column + Stretch) with the child linked
//! - `children()` returns `&[child]` so the pipeline traverses it normally
//!
//! This matches Flutter's `Offstage` widget semantics.

use std::any::Any;

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey};
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

    /// Layout node in Taffy.
    layout_node: Option<LayoutNodeKey>,
}

impl OffstageRenderObject {
    /// Create a new offstage render object.
    pub fn new(offstage: bool) -> Self {
        Self {
            offstage,
            child: None,
            computed_bounds: None,
            layout_node: None,
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
            // Offstage: create a zero-size leaf node. Do NOT link the child into
            // the Taffy tree — the child's own layout node persists in the engine
            // (created on a previous onstage frame or never created if it was
            // offstage from the start), but it is not part of our subtree.
            let leaf_layout = Layout::default().width(0.0).height(0.0);
            match self.layout_node {
                Some(existing) => {
                    ctx.engine().set_style(existing, &leaf_layout);
                    ctx.engine().set_children(existing, &[]);
                    LayoutResult {
                        node: existing,
                        size: Size::zero(),
                    }
                }
                None => {
                    let node = ctx.engine().create_container(&leaf_layout, &[]);
                    self.layout_node = Some(node);
                    LayoutResult {
                        node,
                        size: Size::zero(),
                    }
                }
            }
        } else {
            // Onstage: pass-through container, child sizes the parent naturally.
            let layout = Layout::default()
                .flex_direction(FlexDirection::Column)
                .align(AlignItems::Stretch);

            match self.layout_node {
                Some(existing) => {
                    ctx.engine().set_style(existing, &layout);
                    ctx.engine().set_children(existing, child_nodes);
                    LayoutResult {
                        node: existing,
                        size: Size::zero(),
                    }
                }
                None => {
                    let node = ctx.engine().create_container(&layout, child_nodes);
                    self.layout_node = Some(node);
                    LayoutResult {
                        node,
                        size: Size::zero(),
                    }
                }
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
        // No own paint commands. When offstage, children() returns &[] so the
        // painter never recurses into the child. When onstage, the child is
        // painted by the pipeline's normal traversal.
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
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
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

        // Pass a child node, but offstage should ignore it
        let result = ro.layout(&mut ctx, &[]);

        assert!(ro.layout_node.is_some());
        assert_eq!(ro.layout_node, Some(result.node));
        // Offstage produces zero size
        assert_eq!(result.size, Size::zero());
    }

    #[test]
    fn test_offstage_layout_onstage_passes_child() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create a child leaf node
        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };

        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        let result = ro.layout(&mut ctx, &[child_node]);

        assert!(ro.layout_node.is_some());
        assert_eq!(ro.layout_node, Some(result.node));
    }
}
