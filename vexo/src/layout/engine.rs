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

use super::node::{LayoutNode, LayoutTree};

// ============================================================================
// LAYOUT ENGINE TRAIT
// ============================================================================

/// Trait for layout engine implementations.
///
/// A layout engine takes a tree of layout nodes and computes the final
/// positions and sizes for each node.
pub trait LayoutEngine {
    /// Build a layout tree from a root node.
    ///
    /// Returns a handle to the built tree that can be used for computation.
    fn build_tree(&mut self, root: LayoutNode) -> LayoutTreeHandle;

    /// Compute layout for the given tree within the available space.
    ///
    /// Returns a `LayoutTree` containing the computed positions and sizes.
    fn compute_layout(
        &mut self,
        tree: LayoutTreeHandle,
        available_size: Size<Logical>,
    ) -> LayoutTree;

    /// Clear the internal state.
    ///
    /// Called when the widget tree is rebuilt and all layout nodes
    /// need to be recreated.
    fn clear(&mut self);
}

/// Handle to a built layout tree within the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutTreeHandle(pub u64);

impl LayoutTreeHandle {
    /// Create a new handle.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
}

// ============================================================================
// LAYOUT ERROR
// ============================================================================

/// Errors that can occur during layout.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutError {
    /// The tree handle is invalid.
    InvalidTreeHandle,
    /// A node was not found.
    NodeNotFound,
    /// The layout computation failed.
    ComputationFailed(String),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::InvalidTreeHandle => write!(f, "Invalid tree handle"),
            LayoutError::NodeNotFound => write!(f, "Node not found"),
            LayoutError::ComputationFailed(msg) => write!(f, "Layout computation failed: {}", msg),
        }
    }
}

impl std::error::Error for LayoutError {}

// ============================================================================
// MOCK LAYOUT ENGINE (FOR TESTING)
// ============================================================================

/// A simple mock layout engine for testing.
///
/// This engine doesn't perform real layout computation - it just
/// assigns fixed sizes or fills available space.
#[cfg(test)]
pub struct MockLayoutEngine {
    next_handle: u64,
}

#[cfg(test)]
impl MockLayoutEngine {
    /// Create a new mock layout engine.
    pub fn new() -> Self {
        Self { next_handle: 0 }
    }
}

#[cfg(test)]
impl LayoutEngine for MockLayoutEngine {
    fn build_tree(&mut self, _root: LayoutNode) -> LayoutTreeHandle {
        let handle = LayoutTreeHandle::new(self.next_handle);
        self.next_handle += 1;
        handle
    }

    fn compute_layout(
        &mut self,
        _tree: LayoutTreeHandle,
        available_size: Size<Logical>,
    ) -> LayoutTree {
        // Simple mock: just return a single layout filling available space
        use super::node::{ComputedLayout, LayoutNodeId};
        let mut result = LayoutTree::new();
        result.push(ComputedLayout::new(
            LayoutNodeId::new(0),
            crate::core::Rect::from_xywh(0.0, 0.0, available_size.width, available_size.height),
        ));
        result
    }

    fn clear(&mut self) {
        // Nothing to clear
    }
}

#[cfg(test)]
impl Default for MockLayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_tree_handle() {
        let h1 = LayoutTreeHandle::new(1);
        let h2 = LayoutTreeHandle::new(2);
        assert_ne!(h1, h2);
        assert_eq!(h1, LayoutTreeHandle::new(1));
    }

    #[test]
    fn test_mock_layout_engine() {
        let mut engine = MockLayoutEngine::new();
        let root = LayoutNode::leaf(
            super::super::node::LayoutNodeId::new(0),
            super::super::node::LayoutConstraints::fill(),
        );
        let handle = engine.build_tree(root);
        let result = engine.compute_layout(handle, Size::new(100.0, 100.0));

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_layout_error_display() {
        let e = LayoutError::InvalidTreeHandle;
        assert_eq!(format!("{}", e), "Invalid tree handle");

        let e = LayoutError::ComputationFailed("test error".to_string());
        assert_eq!(format!("{}", e), "Layout computation failed: test error");
    }
}
