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

use crate::core::{Logical, Size};
use crate::dirty::DirtyTracking;
use crate::id::RenderObjectKey;
use crate::layout::{LayoutEngine, LayoutNodeKey};
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

        // Phase 1: Update dirty render objects in the Taffy tree.
        // We must process in bottom-up order because parent containers
        // need their children's LayoutNodeKeys to exist before they can
        // create/update their own Taffy nodes.
        {
            let mut ctx = LayoutContext::new(engine, font_system);
            let dirty_keys: Vec<RenderObjectKey> = dirty.drain_layout().collect();
            Self::layout_dirty_recursive(render_objects, root_id, &dirty_keys, &mut ctx);
            // Return value intentionally ignored — propagation happens internally
        }

        // Phase 1.5 (debug only): Verify pass-through RO cache invariant.
        //
        // After layout_dirty_recursive, every pass-through RO's cached
        // `child_layout_node` must match its child's actual `layout_node()`.
        // A mismatch means the cache went stale — the child's Taffy node
        // changed but the parent wasn't notified, which orphans the new
        // node and causes 0×0 bounds (the "avatar never renders" bug class).
        //
        // Also verify that every child key in `children()` refers to an RO
        // that still exists in the registry — a stale key means `remove_child`
        // wasn't called during unmount.
        #[cfg(debug_assertions)]
        Self::assert_ro_tree_consistency(render_objects, root_id);

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
            Self::apply_layout_recursive(render_objects, root_id, &mut ctx);
        }

        log::debug!("[IncrementalLayout] layout() complete");
    }

    /// Recursively layout dirty render objects in bottom-up order.
    ///
    /// Walks the entire render object tree but only calls `layout()`
    /// on objects that are in the dirty set, that don't yet have
    /// Taffy nodes, or whose children's layout nodes changed.
    /// This ensures bottom-up ordering (children before parents)
    /// which is required because parent containers need their
    /// children's LayoutNodeKeys.
    ///
    /// Returns `true` if this RO's `layout_node()` changed (was created
    /// or replaced). Pass-through ROs (ProxyRenderObject, etc.) return
    /// the child's node from `layout()`, so when the child's node changes,
    /// the pass-through's `layout_node()` also changes. The parent must
    /// then call `layout()` to pick up the new node and re-link. Without
    /// this propagation, a child swap deep in a chain of pass-through ROs
    /// (e.g. Component→Component→NetworkImage→Image) orphans the new
    /// child's Taffy node — the node exists but is never linked into
    /// the root tree, so Taffy computes 0×0 bounds for it.
    fn layout_dirty_recursive(
        render_objects: &mut RenderObjectRegistry,
        id: RenderObjectKey,
        dirty_keys: &[RenderObjectKey],
        ctx: &mut LayoutContext,
    ) -> bool {
        // First, collect children so we can recurse without borrowing
        let children: Vec<RenderObjectKey> = render_objects
            .get(id)
            .map(|obj| obj.children().to_vec())
            .unwrap_or_default();

        // Recurse into children first (bottom-up)
        let mut child_node_changed = false;
        for child_id in &children {
            if Self::layout_dirty_recursive(render_objects, *child_id, dirty_keys, ctx) {
                child_node_changed = true;
            }
        }

        // Snapshot the layout node before layout() so we can detect changes
        let old_node = render_objects.get(id).and_then(|obj| obj.layout_node());

        // Check if this render object needs layout
        let needs_layout = dirty_keys.contains(&id) || old_node.is_none() || child_node_changed;

        if !needs_layout {
            return false;
        }

        // Collect child layout nodes (now that children have been processed)
        let child_nodes: Vec<LayoutNodeKey> = children
            .iter()
            .filter_map(|&child_key| render_objects.get(child_key).and_then(|c| c.layout_node()))
            .collect();

        if let Some(obj) = render_objects.get_mut(id) {
            obj.layout(ctx, &child_nodes);
        }

        // Return true if our layout_node changed — parent must re-link
        let new_node = render_objects.get(id).and_then(|obj| obj.layout_node());
        old_node != new_node
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

    /// Debug-only assertion: verify the RO tree is internally consistent.
    ///
    /// Checks two invariants:
    ///
    /// 1. **No stale child references:** Every child key returned by `children()`
    ///    must refer to an RO that exists in the registry. A stale key means
    ///    `remove_child` wasn't called when the child was unmounted.
    ///
    /// 2. **Pass-through cache consistency:** For pass-through ROs (those with
    ///    `is_pass_through() == true`), the cached `layout_node()` must match
    ///    the child's actual `layout_node()`. A mismatch means the child's
    ///    Taffy node changed but the parent's cache wasn't invalidated —
    ///    the "avatar never renders" bug class.
    #[cfg(debug_assertions)]
    fn assert_ro_tree_consistency(render_objects: &RenderObjectRegistry, root_id: RenderObjectKey) {
        fn check(reg: &RenderObjectRegistry, id: RenderObjectKey, path: &mut Vec<RenderObjectKey>) {
            path.push(id);
            let ro = match reg.get(id) {
                Some(ro) => ro,
                None => {
                    panic!(
                        "RO tree consistency: RO {:?} referenced as child but not in registry. Path: {:?}",
                        id, path
                    );
                }
            };

            let children: Vec<RenderObjectKey> = ro.children().to_vec();

            // Check pass-through cache consistency
            if ro.is_pass_through() {
                let cached_node = ro.layout_node();
                let child_node = children
                    .first()
                    .and_then(|&c| reg.get(c).and_then(|child_ro| child_ro.layout_node()));
                assert_eq!(
                    cached_node, child_node,
                    "RO tree consistency: pass-through RO {:?} has stale layout_node cache. \
                     Cached={:?}, child's actual={:?}. Path: {:?}. \
                     This means the child's Taffy node changed but the parent wasn't notified \
                     — the new node will be orphaned (0×0 bounds).",
                    id, cached_node, child_node, path
                );
            }

            // Recurse into children
            for &child_id in &children {
                check(reg, child_id, path);
            }
            path.pop();
        }

        let mut path = Vec::new();
        check(render_objects, root_id, &mut path);
    }
}
