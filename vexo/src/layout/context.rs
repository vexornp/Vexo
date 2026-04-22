//! Layout context types for widget interaction.
//!
//! This module provides `LayoutContext` and `LayoutView` types that widgets
//! use to interact with the layout engine during layout, draw, and event handling.

use super::{ComputedLayout, Layout, LayoutEngine, LayoutNodeId};
use super::measurement::MeasureContext;

// ============================================================================
// LAYOUT CONTEXT
// ============================================================================

/// Context for widget layout operations.
///
/// Provides mutable access to the layout engine during the layout phase.
/// Widgets use this to create nodes and retrieve computed layouts.
pub struct LayoutContext<'a> {
    engine: &'a mut dyn LayoutEngine,
}

impl<'a> LayoutContext<'a> {
    /// Create a new layout context wrapping a layout engine.
    pub fn new(engine: &'a mut dyn LayoutEngine) -> Self {
        Self { engine }
    }

    /// Create a leaf node (no children).
    ///
    /// Returns a handle to reference this node later.
    pub fn create_leaf(&mut self, layout: &Layout) -> LayoutNodeId {
        self.engine.create_leaf(layout)
    }

    /// Create a leaf node with custom measurement context.
    ///
    /// Used for nodes like text that need accurate intrinsic size calculation.
    pub fn create_leaf_with_context(
        &mut self,
        layout: &Layout,
        context: MeasureContext,
    ) -> LayoutNodeId {
        self.engine.create_leaf_with_context(layout, context)
    }

    /// Create a container node with children.
    ///
    /// Returns a handle to reference this node later.
    pub fn create_container(
        &mut self,
        layout: &Layout,
        children: &[LayoutNodeId],
    ) -> LayoutNodeId {
        self.engine.create_container(layout, children)
    }

    /// Get the computed layout for a node.
    ///
    /// Returns `None` if layout hasn't been computed or node doesn't exist.
    pub fn get_layout(&self, node: LayoutNodeId) -> Option<ComputedLayout> {
        self.engine.get_layout(node)
    }

    /// Get children of a node.
    ///
    /// Used by container widgets to traverse their children.
    pub fn children(&self, node: LayoutNodeId) -> Vec<LayoutNodeId> {
        self.engine.children(node)
    }
}

// ============================================================================
// LAYOUT VIEW
// ============================================================================

/// Read-only view of the layout engine.
///
/// Used during draw and event handling when widgets only need to
/// query computed layouts, not create new nodes.
pub struct LayoutView<'a> {
    engine: &'a dyn LayoutEngine,
}

impl<'a> LayoutView<'a> {
    /// Create a new layout view wrapping a layout engine.
    pub fn new(engine: &'a dyn LayoutEngine) -> Self {
        Self { engine }
    }

    /// Get the computed layout for a node.
    ///
    /// Returns `None` if layout hasn't been computed or node doesn't exist.
    pub fn get_layout(&self, node: LayoutNodeId) -> Option<ComputedLayout> {
        self.engine.get_layout(node)
    }

    /// Get children of a node.
    ///
    /// Used by container widgets to traverse their children.
    pub fn children(&self, node: LayoutNodeId) -> Vec<LayoutNodeId> {
        self.engine.children(node)
    }
}
