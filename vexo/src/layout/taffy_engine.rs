//! Taffy-based layout engine implementation.
//!
//! This module provides a `LayoutEngine` implementation using the Taffy
//! layout library (CSS Flexbox-style layout).

use crate::core::{Rect, Size};
use crate::core::Logical;

use super::engine::LayoutEngine;
use super::node::{ComputedLayout, LayoutNodeId};
use super::Layout;

use std::collections::HashMap;
use taffy::prelude::{AvailableSpace, NodeId as TaffyNodeId};

// ============================================================================
// TAFFY LAYOUT ENGINE
// ============================================================================

/// Layout engine implementation using Taffy.
///
/// This engine wraps the Taffy library and provides a `LayoutEngine`
/// implementation using CSS Flexbox-style layout.
pub struct TaffyLayoutEngine {
    /// The underlying Taffy tree.
    inner: taffy::TaffyTree,
    /// Mapping from our LayoutNodeId to Taffy's NodeId.
    node_map: HashMap<LayoutNodeId, TaffyNodeId>,
    /// Mapping from LayoutNodeId to its children (for traversal).
    children_map: HashMap<LayoutNodeId, Vec<LayoutNodeId>>,
    /// Counter for generating unique node IDs.
    next_id: u64,
}

impl TaffyLayoutEngine {
    /// Create a new Taffy-based layout engine.
    pub fn new() -> Self {
        Self {
            inner: taffy::TaffyTree::new(),
            node_map: HashMap::new(),
            children_map: HashMap::new(),
            next_id: 0,
        }
    }

    /// Generate a new unique LayoutNodeId.
    fn generate_id(&mut self) -> LayoutNodeId {
        let id = LayoutNodeId::new(self.next_id);
        self.next_id += 1;
        id
    }
}

impl Default for TaffyLayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine for TaffyLayoutEngine {
    fn create_leaf(&mut self, layout: &Layout) -> LayoutNodeId {
        let id = self.generate_id();
        let style = layout.to_taffy_style();
        let taffy_id = self.inner.new_leaf(style).unwrap();
        self.node_map.insert(id, taffy_id);
        id
    }

    fn create_container(&mut self, layout: &Layout, children: &[LayoutNodeId]) -> LayoutNodeId {
        let id = self.generate_id();
        let style = layout.to_taffy_style();

        // Map our LayoutNodeIds to Taffy NodeIds
        let child_taffy_ids: Vec<TaffyNodeId> = children
            .iter()
            .filter_map(|c| self.node_map.get(c).copied())
            .collect();

        let taffy_id = self.inner.new_with_children(style, &child_taffy_ids).unwrap();
        self.node_map.insert(id, taffy_id);
        self.children_map.insert(id, children.to_vec());
        id
    }

    fn compute(&mut self, root: LayoutNodeId, available_size: Size<Logical>) {
        if let Some(&root_taffy_id) = self.node_map.get(&root) {
            let _ = self.inner.compute_layout(
                root_taffy_id,
                taffy::Size {
                    width: AvailableSpace::Definite(available_size.width),
                    height: AvailableSpace::Definite(available_size.height),
                },
            );
        }
    }

    fn get_layout(&self, node: LayoutNodeId) -> Option<ComputedLayout> {
        let taffy_id = self.node_map.get(&node)?;
        let layout = self.inner.layout(*taffy_id).ok()?;

        Some(ComputedLayout::new(
            node,
            Rect::from_xywh(
                layout.location.x,
                layout.location.y,
                layout.size.width,
                layout.size.height,
            ),
        ))
    }

    fn children(&self, node: LayoutNodeId) -> Vec<LayoutNodeId> {
        self.children_map.get(&node).cloned().unwrap_or_default()
    }

    fn clear(&mut self) {
        self.inner.clear();
        self.node_map.clear();
        self.children_map.clear();
        self.next_id = 0;
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::FlexDirection;

    #[test]
    fn test_create_leaf() {
        let mut engine = TaffyLayoutEngine::new();

        let layout = Layout::default().width(100.0).height(50.0);
        let node_id = engine.create_leaf(&layout);

        engine.compute(node_id, Size::new(200.0, 200.0));

        let computed = engine.get_layout(node_id).unwrap();
        assert_eq!(computed.width(), 100.0);
        assert_eq!(computed.height(), 50.0);
    }

    #[test]
    fn test_create_container() {
        let mut engine = TaffyLayoutEngine::new();

        // Create two leaf children
        let child1 = engine.create_leaf(&Layout::default().width(50.0).height(50.0));
        let child2 = engine.create_leaf(&Layout::default().width(75.0).height(50.0));

        // Create a row container
        let parent = engine.create_container(
            &Layout::default().flex_direction(FlexDirection::Row),
            &[child1, child2],
        );

        engine.compute(parent, Size::new(200.0, 100.0));

        // Check that children are laid out horizontally
        let child1_layout = engine.get_layout(child1).unwrap();
        let child2_layout = engine.get_layout(child2).unwrap();

        // Second child should be to the right of first child
        assert!(child2_layout.x() >= child1_layout.x() + child1_layout.width());
        assert_eq!(child1_layout.width(), 50.0);
        assert_eq!(child2_layout.width(), 75.0);
    }

    #[test]
    fn test_children() {
        let mut engine = TaffyLayoutEngine::new();

        let child1 = engine.create_leaf(&Layout::default());
        let child2 = engine.create_leaf(&Layout::default());
        let parent = engine.create_container(&Layout::default(), &[child1, child2]);

        let children = engine.children(parent);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], child1);
        assert_eq!(children[1], child2);
    }

    #[test]
    fn test_clear() {
        let mut engine = TaffyLayoutEngine::new();

        let _node = engine.create_leaf(&Layout::default());
        assert!(!engine.node_map.is_empty());

        engine.clear();
        assert!(engine.node_map.is_empty());
        assert!(engine.children_map.is_empty());
    }
}
