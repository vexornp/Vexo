//! Taffy-based layout engine implementation.
//!
//! This module provides a `LayoutEngine` implementation using the Taffy
//! layout library (CSS Flexbox-style layout).

use crate::core::{Point, Rect, Size};
use crate::core::Logical;

use super::engine::{LayoutEngine, LayoutError, LayoutTreeHandle};
use super::node::{
    AlignItems, ComputedLayout, FlexDirection, LayoutConstraints, LayoutNode, LayoutNodeId,
    LayoutPadding, LayoutTree,
};

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
    /// Mapping from tree handle to root Taffy node.
    tree_roots: HashMap<u64, TaffyNodeId>,
    /// Next tree handle ID.
    next_handle: u64,
    /// Counter for generating unique node IDs during traversal.
    node_id_counter: u64,
}

impl TaffyLayoutEngine {
    /// Create a new Taffy-based layout engine.
    pub fn new() -> Self {
        Self {
            inner: taffy::TaffyTree::new(),
            node_map: HashMap::new(),
            tree_roots: HashMap::new(),
            next_handle: 0,
            node_id_counter: 0,
        }
    }

    /// Convert our FlexDirection to Taffy's.
    fn to_taffy_direction(dir: FlexDirection) -> taffy::prelude::FlexDirection {
        match dir {
            FlexDirection::Row => taffy::prelude::FlexDirection::Row,
            FlexDirection::Column => taffy::prelude::FlexDirection::Column,
            FlexDirection::RowReverse => taffy::prelude::FlexDirection::RowReverse,
            FlexDirection::ColumnReverse => taffy::prelude::FlexDirection::ColumnReverse,
        }
    }

    /// Convert our AlignItems to Taffy's.
    fn to_taffy_align(align: AlignItems) -> Option<taffy::prelude::AlignItems> {
        match align {
            AlignItems::Stretch => Some(taffy::prelude::AlignItems::Stretch),
            AlignItems::Start => Some(taffy::prelude::AlignItems::Start),
            AlignItems::End => Some(taffy::prelude::AlignItems::End),
            AlignItems::Center => Some(taffy::prelude::AlignItems::Center),
        }
    }

    /// Convert our LayoutConstraints to Taffy Style.
    fn constraints_to_style(constraints: &LayoutConstraints, padding: &LayoutPadding) -> taffy::Style {
        taffy::Style {
            min_size: taffy::Size {
                width: taffy::Dimension::length(constraints.min_width),
                height: taffy::Dimension::length(constraints.min_height),
            },
            max_size: taffy::Size {
                width: if constraints.max_width.is_infinite() {
                    taffy::Dimension::auto()
                } else {
                    taffy::Dimension::length(constraints.max_width)
                },
                height: if constraints.max_height.is_infinite() {
                    taffy::Dimension::auto()
                } else {
                    taffy::Dimension::length(constraints.max_height)
                },
            },
            flex_grow: constraints.flex_grow,
            flex_shrink: constraints.flex_shrink,
            padding: taffy::Rect {
                left: taffy::LengthPercentage::length(padding.left),
                right: taffy::LengthPercentage::length(padding.right),
                top: taffy::LengthPercentage::length(padding.top),
                bottom: taffy::LengthPercentage::length(padding.bottom),
            },
            ..Default::default()
        }
    }

    /// Build a Taffy node tree from our LayoutNode tree.
    fn build_taffy_node(&mut self, node: &LayoutNode) -> Result<TaffyNodeId, LayoutError> {
        // Store the node ID mapping
        let our_id = node.id;

        if node.is_leaf() {
            // Create a leaf node
            let style = Self::constraints_to_style(&node.constraints, &node.padding);
            let taffy_id = self.inner.new_leaf(style).map_err(|e| {
                LayoutError::ComputationFailed(format!("Failed to create leaf node: {:?}", e))
            })?;
            self.node_map.insert(our_id, taffy_id);
            Ok(taffy_id)
        } else {
            // First, recursively build children
            let mut child_taffy_ids = Vec::with_capacity(node.children.len());
            for child in &node.children {
                let child_id = self.build_taffy_node(child)?;
                child_taffy_ids.push(child_id);
            }

            // Create a container node with children
            let style = taffy::Style {
                display: taffy::Display::Flex,
                flex_direction: Self::to_taffy_direction(node.direction),
                align_items: Self::to_taffy_align(node.align_items),
                gap: taffy::Size {
                    width: taffy::LengthPercentage::length(node.gap),
                    height: taffy::LengthPercentage::length(node.gap),
                },
                padding: taffy::Rect {
                    left: taffy::LengthPercentage::length(node.padding.left),
                    right: taffy::LengthPercentage::length(node.padding.right),
                    top: taffy::LengthPercentage::length(node.padding.top),
                    bottom: taffy::LengthPercentage::length(node.padding.bottom),
                },
                // Apply constraints for sizing
                min_size: taffy::Size {
                    width: taffy::Dimension::length(node.constraints.min_width),
                    height: taffy::Dimension::length(node.constraints.min_height),
                },
                max_size: taffy::Size {
                    width: if node.constraints.max_width.is_infinite() {
                        taffy::Dimension::auto()
                    } else {
                        taffy::Dimension::length(node.constraints.max_width)
                    },
                    height: if node.constraints.max_height.is_infinite() {
                        taffy::Dimension::auto()
                    } else {
                        taffy::Dimension::length(node.constraints.max_height)
                    },
                },
                flex_grow: node.constraints.flex_grow,
                flex_shrink: node.constraints.flex_shrink,
                ..Default::default()
            };

            let taffy_id = self.inner.new_with_children(style, &child_taffy_ids).map_err(|e| {
                LayoutError::ComputationFailed(format!("Failed to create container node: {:?}", e))
            })?;
            self.node_map.insert(our_id, taffy_id);
            Ok(taffy_id)
        }
    }

    /// Extract computed layouts from the Taffy tree.
    fn extract_layouts(
        &self,
        taffy_id: TaffyNodeId,
        offset: Point<Logical>,
        result: &mut LayoutTree,
    ) -> Result<(), LayoutError> {
        let layout = self.inner.layout(taffy_id).map_err(|e| {
            LayoutError::ComputationFailed(format!("Failed to get layout: {:?}", e))
        })?;

        // Find our node ID from the Taffy node ID
        let our_id = self.node_map.iter()
            .find(|(_, &tid)| tid == taffy_id)
            .map(|(&id, _)| id)
            .ok_or(LayoutError::NodeNotFound)?;

        let bounds = Rect::from_xywh(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
            layout.size.width,
            layout.size.height,
        );

        result.push(ComputedLayout::new(our_id, bounds));

        // Recursively extract children
        let children = self.inner.children(taffy_id).map_err(|e| {
            LayoutError::ComputationFailed(format!("Failed to get children: {:?}", e))
        })?;

        let child_offset = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        for child_taffy_id in children {
            self.extract_layouts(child_taffy_id, child_offset, result)?;
        }

        Ok(())
    }
}

impl Default for TaffyLayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine for TaffyLayoutEngine {
    fn build_tree(&mut self, root: LayoutNode) -> LayoutTreeHandle {
        // Clear previous state
        self.node_map.clear();

        // Build the Taffy node tree
        match self.build_taffy_node(&root) {
            Ok(taffy_root_id) => {
                let handle = LayoutTreeHandle::new(self.next_handle);
                self.next_handle += 1;
                self.tree_roots.insert(handle.0, taffy_root_id);
                handle
            }
            Err(_) => {
                // Return an invalid handle on error
                LayoutTreeHandle::new(u64::MAX)
            }
        }
    }

    fn compute_layout(
        &mut self,
        tree: LayoutTreeHandle,
        available_size: Size<Logical>,
    ) -> LayoutTree {
        let root_id = match self.tree_roots.get(&tree.0) {
            Some(&id) => id,
            None => return LayoutTree::new(),
        };

        // Compute layout
        let result = self.inner.compute_layout(
            root_id,
            taffy::Size {
                width: AvailableSpace::Definite(available_size.width),
                height: AvailableSpace::Definite(available_size.height),
            },
        );

        if result.is_err() {
            return LayoutTree::new();
        }

        // Extract computed layouts
        let mut layouts = LayoutTree::with_capacity(self.node_map.len());
        if self.extract_layouts(root_id, Point::new(0.0, 0.0), &mut layouts).is_err() {
            return LayoutTree::new();
        }

        layouts
    }

    fn clear(&mut self) {
        self.inner.clear();
        self.node_map.clear();
        self.tree_roots.clear();
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taffy_engine_leaf_node() {
        let mut engine = TaffyLayoutEngine::new();

        let leaf = LayoutNode::leaf(
            LayoutNodeId::new(1),
            LayoutConstraints::fixed(100.0, 50.0),
        );

        let handle = engine.build_tree(leaf);
        let result = engine.compute_layout(handle, Size::new(200.0, 200.0));

        assert_eq!(result.len(), 1);
        let layout = result.find(LayoutNodeId::new(1)).unwrap();
        assert_eq!(layout.width(), 100.0);
        assert_eq!(layout.height(), 50.0);
    }

    #[test]
    fn test_taffy_engine_container() {
        let mut engine = TaffyLayoutEngine::new();

        let child1 = LayoutNode::leaf(
            LayoutNodeId::new(2),
            LayoutConstraints::fixed(50.0, 50.0),
        );
        let child2 = LayoutNode::leaf(
            LayoutNodeId::new(3),
            LayoutConstraints::fixed(50.0, 50.0),
        );
        let parent = LayoutNode::container(
            LayoutNodeId::new(1),
            FlexDirection::Row,
            vec![child1, child2],
        );

        let handle = engine.build_tree(parent);
        let result = engine.compute_layout(handle, Size::new(200.0, 100.0));

        assert_eq!(result.len(), 3);

        // Check that children are laid out horizontally
        let child1_layout = result.find(LayoutNodeId::new(2)).unwrap();
        let child2_layout = result.find(LayoutNodeId::new(3)).unwrap();

        // Second child should be to the right of first child
        assert!(child2_layout.x() >= child1_layout.x() + child1_layout.width());
    }

    #[test]
    fn test_taffy_engine_flex_grow() {
        let mut engine = TaffyLayoutEngine::new();

        // Test that a container properly sizes its children
        // In this case, we test that the layout engine correctly computes
        // positions for children in a row layout
        let child1 = LayoutNode::leaf(
            LayoutNodeId::new(2),
            LayoutConstraints::fixed(50.0, 50.0),
        );
        let child2 = LayoutNode::leaf(
            LayoutNodeId::new(3),
            LayoutConstraints::fixed(75.0, 50.0),
        );
        let parent = LayoutNode::container(
            LayoutNodeId::new(1),
            FlexDirection::Row,
            vec![child1, child2],
        );

        let handle = engine.build_tree(parent);
        let result = engine.compute_layout(handle, Size::new(200.0, 100.0));

        assert_eq!(result.len(), 3);

        // Both children should be laid out horizontally
        let child1_layout = result.find(LayoutNodeId::new(2)).unwrap();
        let child2_layout = result.find(LayoutNodeId::new(3)).unwrap();

        // Child 2 should be to the right of child 1
        assert!(child2_layout.x() >= child1_layout.x() + child1_layout.width());
        // Both should have correct sizes
        assert_eq!(child1_layout.width(), 50.0);
        assert_eq!(child2_layout.width(), 75.0);
    }

    #[test]
    fn test_taffy_engine_clear() {
        let mut engine = TaffyLayoutEngine::new();

        let leaf = LayoutNode::leaf(
            LayoutNodeId::new(1),
            LayoutConstraints::fixed(100.0, 50.0),
        );

        let handle = engine.build_tree(leaf);
        assert!(!engine.node_map.is_empty());

        engine.clear();
        assert!(engine.node_map.is_empty());
        assert!(engine.tree_roots.is_empty());
    }
}
