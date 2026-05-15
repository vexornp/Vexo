//! Layout operations extracted from ThreeTreePipeline.
//!
//! This module provides the `Layouter` struct (zero-sized) that holds
//! layout-related methods as associated functions. The pipeline delegates
//! to these functions for the layout phase of the rendering lifecycle.

use crate::core::{Logical, Size};
use crate::layout::{Layout, LayoutEngine, LayoutNodeKey};
use crate::retain::dirty::DirtyTracking;
use crate::retain::id::RenderObjectKey;
use crate::retain::render_object::{LayoutContext, LayoutResult, RenderObjectRegistry};

/// Zero-sized struct holding layout-related associated functions.
///
/// This is a pure extraction from `ThreeTreePipeline` -- no state, no behavior
/// beyond what the pipeline already implemented. Methods take explicit
/// parameters instead of accessing `self` fields.
pub struct Layouter;

impl Layouter {
    /// Perform layout using the Taffy layout engine.
    ///
    /// Three-phase layout:
    /// 1. Build Taffy tree (each RenderObject creates nodes)
    /// 2. Compute layout with Taffy
    /// 3. Apply computed layouts back to RenderObjects
    ///
    /// # Arguments
    ///
    /// * `render_objects` - Registry of render objects (third tree)
    /// * `dirty` - Dirty tracking for incremental updates
    /// * `available_size` - The size available for the root render object
    /// * `engine` - Layout engine for node creation and computation
    /// * `font_system` - Font system for text measurement
    pub fn layout(
        render_objects: &mut RenderObjectRegistry,
        dirty: &mut DirtyTracking,
        available_size: Size<Logical>,
        engine: &mut dyn LayoutEngine,
        font_system: &mut glyphon::FontSystem,
    ) {
        let dirty_layout_count = dirty.layout_count();
        let total_objects = render_objects.len();

        log::debug!(
            "[RetainMode] layout() - Processing {} dirty objects out of {} total",
            dirty_layout_count,
            total_objects
        );

        // Get the root render object
        let root_id = match render_objects.root() {
            Some(id) => id,
            None => return,
        };

        // Phase 1: Build Taffy tree (bottom-up: children first, then parent)
        // The pipeline traverses children first, collects their node IDs,
        // then passes them to the parent's layout method.
        {
            let mut ctx = LayoutContext::new(engine, font_system);
            Self::layout_build_recursive(render_objects, root_id, &mut ctx);
        }

        // Phase 2: Compute layout with Taffy
        if let Some(root_node) = Self::get_layout_node(render_objects, root_id) {
            engine.compute(root_node, available_size, font_system);
        }

        // Phase 3: Apply computed layouts back to render objects
        {
            let ctx = LayoutContext::new(engine, font_system);
            Self::apply_layout_recursive(render_objects, root_id, &ctx);
        }

        // Clear dirty flags
        dirty.drain_layout().for_each(drop);

        log::debug!("[RetainMode] layout() complete - dirty flags cleared");
    }

    /// Recursively build Taffy tree (bottom-up: children first).
    pub(crate) fn layout_build_recursive(
        render_objects: &mut RenderObjectRegistry,
        id: RenderObjectKey,
        ctx: &mut LayoutContext,
    ) -> LayoutResult {
        // Get children
        let children: Vec<RenderObjectKey> = render_objects
            .get(id)
            .map(|obj| obj.children().to_vec())
            .unwrap_or_default();

        // Layout children first (bottom-up)
        let child_nodes: Vec<LayoutNodeKey> = children
            .iter()
            .map(|child_id| Self::layout_build_recursive(render_objects, *child_id, ctx).node)
            .collect();

        // Now layout this object with child nodes
        if let Some(obj) = render_objects.get_mut(id) {
            obj.layout(ctx, &child_nodes)
        } else {
            // Fallback: create empty node
            let node = ctx.engine().create_leaf(&Layout::default());
            LayoutResult {
                node,
                size: Size::new(0.0, 0.0),
            }
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
        ctx: &LayoutContext,
    ) {
        // Get children first
        let children: Vec<RenderObjectKey> = render_objects
            .get(id)
            .map(|obj| obj.children().to_vec())
            .unwrap_or_default();

        // Apply to this object
        if let Some(obj) = render_objects.get_mut(id) {
            obj.apply_layout(ctx);
        }

        // Recursively apply to children
        for child_id in children {
            Self::apply_layout_recursive(render_objects, child_id, ctx);
        }
    }
}