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
use crate::layout::{ComputedLayout, Layout, LayoutNodeId};

// ============================================================================
// LAYOUT ENGINE TRAIT
// ============================================================================

/// Trait for layout engine implementations.
///
/// A layout engine provides immediate-mode layout operations where widgets
/// create nodes incrementally during recursive traversal.
pub trait LayoutEngine {
    /// Create a leaf node (no children).
    ///
    /// Returns a handle to reference this node later.
    fn create_leaf(&mut self, layout: &Layout) -> LayoutNodeId;

    /// Create a container node with children.
    ///
    /// Returns a handle to reference this node later.
    fn create_container(&mut self, layout: &Layout, children: &[LayoutNodeId]) -> LayoutNodeId;

    /// Compute layout for all nodes.
    ///
    /// Must be called after all nodes are created and before `get_layout()`.
    fn compute(&mut self, root: LayoutNodeId, available_size: Size<Logical>);

    /// Get the computed layout for a node.
    ///
    /// Returns `None` if `compute()` hasn't been called or node doesn't exist.
    fn get_layout(&self, node: LayoutNodeId) -> Option<ComputedLayout>;

    /// Get children of a node.
    ///
    /// Used by container widgets to traverse their children during draw and event handling.
    fn children(&self, node: LayoutNodeId) -> Vec<LayoutNodeId>;

    /// Clear all nodes.
    ///
    /// Called when the widget tree is rebuilt and all layout nodes
    /// need to be recreated.
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
