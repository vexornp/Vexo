//! Layout engine abstraction for the Vexo UI framework.
//!
//! This module provides the `LayoutEngine` trait that abstracts layout
//! computation from any specific implementation. This enables:
//!
//! - Swapping layout algorithms (Taffy, custom, etc.)
//! - Mocking layout for testing
//! - Decoupling widgets from the layout engine

use crate::core::Size;
use crate::core::Logical;
use crate::layout::{ComputedLayout, Layout, LayoutNodeKey};
use crate::layout::measurement::MeasureContext;
use glyphon::FontSystem;

// ============================================================================
// LAYOUT ENGINE TRAIT
// ============================================================================

/// Trait for layout engine implementations.
///
/// A layout engine provides layout operations for widgets. The engine
/// maintains a persistent tree of layout nodes across frames, enabling
/// incremental updates: nodes are created once and updated in place when
/// properties change, avoiding full-tree rebuilds.
pub trait LayoutEngine {
    // === Node creation (first frame / new render objects) ===

    /// Create a leaf node (no children).
    fn create_leaf(&mut self, layout: &Layout) -> LayoutNodeKey;

    /// Create a leaf node with custom measurement context.
    fn create_leaf_with_context(
        &mut self,
        layout: &Layout,
        context: MeasureContext,
    ) -> LayoutNodeKey;

    /// Create a container node with children.
    fn create_container(&mut self, layout: &Layout, children: &[LayoutNodeKey]) -> LayoutNodeKey;

    // === Incremental updates (subsequent frames) ===

    /// Update the style on an existing node.
    ///
    /// Marks the node dirty so Taffy recomputes its layout. More efficient
    /// than destroying and recreating the node.
    fn set_style(&mut self, node: LayoutNodeKey, layout: &Layout);

    /// Update the measure context on an existing leaf node.
    ///
    /// Used when text content or font size changes. Marks the node dirty.
    fn set_context(&mut self, node: LayoutNodeKey, context: MeasureContext);

    /// Add a child to a parent node.
    ///
    /// Marks the parent dirty. Use when a container gains a new child.
    fn add_child(&mut self, parent: LayoutNodeKey, child: LayoutNodeKey);

    /// Remove a child from a parent node.
    ///
    /// Marks the parent dirty. Use when a container loses a child.
    fn remove_child(&mut self, parent: LayoutNodeKey, child: LayoutNodeKey);

    /// Set the complete child list for a container node.
    ///
    /// Replaces all existing children. Marks the parent dirty.
    /// Use when children are reordered or multiple children change at once.
    fn set_children(&mut self, parent: LayoutNodeKey, children: &[LayoutNodeKey]);

    /// Remove a node entirely from the engine.
    ///
    /// Use when a render object is unmounted. The node and its children
    /// are removed from Taffy.
    fn remove_node(&mut self, node: LayoutNodeKey);

    /// Mark a node as dirty (clear its cache and propagate to ancestors).
    ///
    /// Use when constraints or available size change without a style/context
    /// change (e.g., window resize).
    fn mark_dirty(&mut self, node: LayoutNodeKey);

    /// Check if a node needs recomputation.
    ///
    /// Returns true if the node's cache was invalidated (dirty).
    /// After `compute()`, dirty nodes become clean.
    fn is_dirty(&self, node: LayoutNodeKey) -> bool;

    // === Computation and readback ===

    /// Compute layout for all nodes.
    ///
    /// Only recomputes dirty nodes and their descendants. Clean subtrees
    /// return cached results.
    fn compute(
        &mut self,
        root: LayoutNodeKey,
        available_size: Size<Logical>,
        font_system: &mut FontSystem,
    );

    /// Get the computed layout for a node.
    fn get_layout(&self, node: LayoutNodeKey) -> Option<ComputedLayout>;

    /// Get children of a node.
    fn children(&self, node: LayoutNodeKey) -> Vec<LayoutNodeKey>;

    /// Clear all nodes.
    ///
    /// Used only for full-tree rebuilds (e.g., when the root widget type
    /// changes completely).
    fn clear(&mut self);
}

// ============================================================================
// LAYOUT ERROR
// ============================================================================

/// Errors that can occur during layout.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutError {
    /// A node was not found.
    NodeNotFound,
    /// The layout computation failed.
    ComputationFailed(String),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::NodeNotFound => write!(f, "Node not found"),
            LayoutError::ComputationFailed(msg) => write!(f, "Layout computation failed: {}", msg),
        }
    }
}

impl std::error::Error for LayoutError {}