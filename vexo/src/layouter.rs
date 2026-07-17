//! Layout operations for the incremental Taffy tree.
//!
//! The layouter persists the Taffy tree across frames. On each frame,
//! it only updates the render objects that are dirty (marked as needing
//! layout), then calls `engine.compute()` which leverages Taffy's
//! per-node caching to skip clean subtrees.
//!
//! # Incremental Layout Protocol
//!
//! 1. For each dirty render object, call `layout()` which either:
//!    - Creates a new Taffy node (first frame, `layout_node` is `None`)
//!    - Updates the existing Taffy node in place (`layout_node` is `Some`)
//! 2. Call `engine.compute()` — Taffy recomputes only dirty nodes
//! 3. Apply computed layouts to all render objects whose Taffy nodes
//!    were recomputed

use crate::core::{Logical, SafeAreaSource, Size};
use crate::dirty::DirtyTracking;
use crate::id::RenderObjectKey;
use crate::layout::{EdgeInsets, LayoutEngine, LayoutNodeKey};
use crate::render_object::{LayoutContext, RenderObjectRegistry};

/// Zero-sized struct holding layout-related associated functions.
pub struct Layouter;

impl Layouter {
    /// Perform layout using the Taffy layout engine.
    ///
    /// Processes render objects that are in the dirty set. For objects
    /// that already have Taffy nodes, updates them in place. For objects
    /// being laid out for the first time, creates new Taffy nodes.
    ///
    /// Taffy's per-node caching ensures clean subtrees are skipped
    /// during `compute()`.
    pub fn layout(
        render_objects: &mut RenderObjectRegistry,
        dirty: &mut DirtyTracking,
        available_size: Size<Logical>,
        engine: &mut dyn LayoutEngine,
        font_system: &mut glyphon::FontSystem,
        safe_area_source: SafeAreaSource,
    ) {
        // Clean up orphaned layout nodes from render objects removed during reconciliation
        let orphaned_nodes = render_objects.drain_orphaned_layout_nodes();
        for node_key in orphaned_nodes {
            engine.remove_node(node_key);
        }

        if dirty.is_layout_empty() {
            return;
        }

        let dirty_count = dirty.layout_count();
        let total_objects = render_objects.len();

        log::debug!(
            "[IncrementalLayout] layout() - Processing {} dirty objects out of {} total",
            dirty_count,
            total_objects
        );

        // Get the root render object
        let root_id = match render_objects.root() {
            Some(id) => id,
            None => return,
        };

        // Phase 0: Top-down safe-area pre-pass.
        //
        // Propagate "effective" insets (global insets minus ancestors'
        // claims) from root to leaves so `SafeAreaRenderObject::layout()`
        // pads by the *remaining* insets, not the raw global ones. This
        // prevents double-consumption: when a sibling bar (e.g. TabBarView's
        // tab bar) owns the bottom edge, the page content's `SafeArea`
        // (wrapped in `SafeAreaClaim::bottom`) sees `bottom == 0` and adds
        // no bottom padding.
        //
        // The walk runs every layout pass. Rotation marks all ROs dirty
        // (`mark_all_needs_layout`), so new global insets are picked up
        // without a widget rebuild. The walk is O(n) — same as Phase 3
        // (`apply_layout_recursive`) which already walks the entire tree.
        {
            let global_insets = safe_area_source.get();
            Self::resolve_effective_safe_area(render_objects, root_id, global_insets);
        }

        // Phase 1: Update dirty render objects in the Taffy tree.
        // We must process in bottom-up order because parent containers
        // need their children's LayoutNodeKeys to exist before they can
        // create/update their own Taffy nodes.
        {
            let mut ctx = LayoutContext::new(engine, font_system);
            ctx.set_safe_area_source(safe_area_source.clone());
            let dirty_keys: Vec<RenderObjectKey> = dirty.drain_layout().collect();
            Self::layout_dirty_recursive(render_objects, root_id, &dirty_keys, &mut ctx);
        }

        // Phase 2: Compute layout with Taffy (only dirty nodes are recomputed)
        if let Some(root_node) = Self::get_layout_node(render_objects, root_id) {
            // Set root to fill available space (CSS html { width: 100%; height: 100% })
            engine.set_root_size(root_node);
            engine.compute(root_node, available_size, font_system);
        }

        // Phase 3: Apply computed layouts to all render objects
        // TODO: optimize to only apply to recomputed subtrees
        {
            let mut ctx = LayoutContext::new(engine, font_system);
            ctx.set_safe_area_source(safe_area_source);
            Self::apply_layout_recursive(render_objects, root_id, &mut ctx);
        }

        log::debug!("[IncrementalLayout] layout() complete");
    }

    /// Recursively layout dirty render objects in bottom-up order.
    ///
    /// Walks the entire render object tree but only calls `layout()`
    /// on objects that are in the dirty set or that don't yet have
    /// Taffy nodes. This ensures bottom-up ordering (children before
    /// parents) which is required because parent containers need
    /// their children's LayoutNodeKeys.
    fn layout_dirty_recursive(
        render_objects: &mut RenderObjectRegistry,
        id: RenderObjectKey,
        dirty_keys: &[RenderObjectKey],
        ctx: &mut LayoutContext,
    ) {
        // First, collect children so we can recurse without borrowing
        let children: Vec<RenderObjectKey> = render_objects
            .get(id)
            .map(|obj| obj.children().to_vec())
            .unwrap_or_default();

        // Recurse into children first (bottom-up)
        for child_id in &children {
            Self::layout_dirty_recursive(render_objects, *child_id, dirty_keys, ctx);
        }

        // Check if this render object needs layout
        let needs_layout = dirty_keys.contains(&id)
            || render_objects
                .get(id)
                .map(|obj| obj.layout_node().is_none())
                .unwrap_or(false);

        if !needs_layout {
            return;
        }

        // Collect child layout nodes (now that children have been processed)
        let child_nodes: Vec<LayoutNodeKey> = children
            .iter()
            .filter_map(|&child_key| render_objects.get(child_key).and_then(|c| c.layout_node()))
            .collect();

        if let Some(obj) = render_objects.get_mut(id) {
            obj.layout(ctx, &child_nodes);
        }
    }

    /// Top-down pre-pass: propagate effective safe-area insets from root to
    /// leaves, applying each render object's claim.
    ///
    /// For each render object:
    /// 1. `set_effective_safe_area(parent_effective)` — store what *this* RO
    ///    should see (SafeAreaRenderObject reads this in `layout()`).
    /// 2. Read `safe_area_claim()` — which edges this RO declares "owned".
    /// 3. Compute `child_effective = claim.remove_from(parent_effective)` —
    ///    claimed edges zeroed for descendants.
    /// 4. Recurse into children with `child_effective`.
    ///
    /// Pass-through ROs (Opacity, SafeAreaClaim, etc.) transparently
    /// propagate: they don't store effective insets (no-op default), and
    /// their claim (if any) reduces insets for their subtree.
    fn resolve_effective_safe_area(
        render_objects: &mut RenderObjectRegistry,
        id: RenderObjectKey,
        parent_effective: EdgeInsets,
    ) {
        let (claim, children) = {
            let ro = match render_objects.get_mut(id) {
                Some(ro) => ro,
                None => return,
            };
            ro.set_effective_safe_area(parent_effective);
            let claim = ro.safe_area_claim();
            let children: Vec<RenderObjectKey> = ro.children().to_vec();
            (claim, children)
        };

        let child_effective = claim.remove_from(parent_effective);
        for child_id in children {
            Self::resolve_effective_safe_area(render_objects, child_id, child_effective);
        }
    }

    /// Get the layout node ID from a render object.
    pub(crate) fn get_layout_node(
        render_objects: &RenderObjectRegistry,
        id: RenderObjectKey,
    ) -> Option<LayoutNodeKey> {
        render_objects.get(id).and_then(|obj| obj.layout_node())
    }

    /// Recursively apply computed layouts.
    pub(crate) fn apply_layout_recursive(
        render_objects: &mut RenderObjectRegistry,
        id: RenderObjectKey,
        ctx: &mut LayoutContext,
    ) {
        let children: Vec<RenderObjectKey> = render_objects
            .get(id)
            .map(|obj| obj.children().to_vec())
            .unwrap_or_default();

        if let Some(obj) = render_objects.get_mut(id) {
            obj.apply_layout(ctx);
        }

        for child_id in children {
            Self::apply_layout_recursive(render_objects, child_id, ctx);
        }
    }
}
