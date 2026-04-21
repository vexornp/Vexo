//! Layout types for the Vexo UI framework.
//!
//! This module provides the types used for layout computation, including
//! constraints, nodes, and computed results.

use crate::core::{Point, Rect, Size};
use crate::core::Logical;

// ============================================================================
// LAYOUT NODE ID
// ============================================================================

/// Unique identifier for a layout node within a layout tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutNodeId(pub u64);

impl LayoutNodeId {
    /// Create a new layout node ID.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value.
    pub const fn as_raw(&self) -> u64 {
        self.0
    }
}

// ============================================================================
// LAYOUT CONSTRAINTS
// ============================================================================

/// Layout constraints that describe how a widget should be sized.
///
/// These constraints are provided by widgets during the layout phase
/// and used by the layout engine to compute final positions and sizes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConstraints {
    /// Minimum width in logical points.
    pub min_width: f32,
    /// Maximum width in logical points (f32::INFINITY for unbounded).
    pub max_width: f32,
    /// Minimum height in logical points.
    pub min_height: f32,
    /// Maximum height in logical points (f32::INFINITY for unbounded).
    pub max_height: f32,
    /// How much this widget should grow relative to siblings.
    pub flex_grow: f32,
    /// How much this widget should shrink relative to siblings.
    pub flex_shrink: f32,
}

impl Default for LayoutConstraints {
    fn default() -> Self {
        Self {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
            flex_grow: 0.0,
            flex_shrink: 1.0,
        }
    }
}

impl LayoutConstraints {
    /// Create constraints for a fixed-size widget.
    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            min_width: width,
            max_width: width,
            min_height: height,
            max_height: height,
            flex_grow: 0.0,
            flex_shrink: 0.0,
        }
    }

    /// Create constraints for a fixed-size widget using a Size value.
    pub fn fixed_size(size: Size<Logical>) -> Self {
        Self::fixed(size.width, size.height)
    }

    /// Create constraints for a widget that fills available space.
    pub fn fill() -> Self {
        Self {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
            flex_grow: 1.0,
            flex_shrink: 1.0,
        }
    }

    /// Create constraints with specific width, auto height.
    pub fn fixed_width(width: f32) -> Self {
        Self {
            min_width: width,
            max_width: width,
            ..Self::default()
        }
    }

    /// Create constraints with specific height, auto width.
    pub fn fixed_height(height: f32) -> Self {
        Self {
            min_height: height,
            max_height: height,
            ..Self::default()
        }
    }

    /// Check if the width is fixed (min == max).
    pub fn is_fixed_width(&self) -> bool {
        (self.min_width - self.max_width).abs() < f32::EPSILON
    }

    /// Check if the height is fixed (min == max).
    pub fn is_fixed_height(&self) -> bool {
        (self.min_height - self.max_height).abs() < f32::EPSILON
    }
}

// ============================================================================
// FLEX DIRECTION
// ============================================================================

/// Direction of flex layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    /// Layout children in a row (horizontal).
    #[default]
    Row,
    /// Layout children in a column (vertical).
    Column,
    /// Layout children in a row, reversed.
    RowReverse,
    /// Layout children in a column, reversed.
    ColumnReverse,
}

// ============================================================================
// ALIGN ITEMS
// ============================================================================

/// How to align items in the cross axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignItems {
    /// Stretch to fill.
    #[default]
    Stretch,
    /// Align to the start.
    Start,
    /// Align to the end.
    End,
    /// Center in the cross axis.
    Center,
}

// ============================================================================
// LAYOUT NODE
// ============================================================================

/// A node in the layout tree.
///
/// Layout nodes form a tree structure that describes the layout hierarchy.
/// Each node has constraints and optional children.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// Unique identifier for this node.
    pub id: LayoutNodeId,
    /// Layout constraints for this node.
    pub constraints: LayoutConstraints,
    /// Flex direction for container nodes.
    pub direction: FlexDirection,
    /// Alignment for children in the cross axis.
    pub align_items: AlignItems,
    /// Gap between children in logical points.
    pub gap: f32,
    /// Padding around the content.
    pub padding: LayoutPadding,
    /// Child nodes.
    pub children: Vec<LayoutNode>,
}

impl LayoutNode {
    /// Create a new leaf node with the given constraints.
    pub fn leaf(id: LayoutNodeId, constraints: LayoutConstraints) -> Self {
        Self {
            id,
            constraints,
            direction: FlexDirection::default(),
            align_items: AlignItems::default(),
            gap: 0.0,
            padding: LayoutPadding::default(),
            children: Vec::new(),
        }
    }

    /// Create a container node with children.
    pub fn container(id: LayoutNodeId, direction: FlexDirection, children: Vec<LayoutNode>) -> Self {
        Self {
            id,
            constraints: LayoutConstraints::default(),
            direction,
            align_items: AlignItems::default(),
            gap: 0.0,
            padding: LayoutPadding::default(),
            children,
        }
    }

    /// Set the gap between children.
    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Set the alignment for children.
    pub fn with_align(mut self, align: AlignItems) -> Self {
        self.align_items = align;
        self
    }

    /// Set the padding.
    pub fn with_padding(mut self, padding: LayoutPadding) -> Self {
        self.padding = padding;
        self
    }

    /// Check if this is a leaf node (no children).
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

// ============================================================================
// LAYOUT PADDING
// ============================================================================

/// Padding around a layout node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutPadding {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Default for LayoutPadding {
    fn default() -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        }
    }
}

impl LayoutPadding {
    /// Create uniform padding on all sides.
    pub fn uniform(amount: f32) -> Self {
        Self {
            left: amount,
            right: amount,
            top: amount,
            bottom: amount,
        }
    }

    /// Create horizontal padding (left and right).
    pub fn horizontal(amount: f32) -> Self {
        Self {
            left: amount,
            right: amount,
            ..Self::default()
        }
    }

    /// Create vertical padding (top and bottom).
    pub fn vertical(amount: f32) -> Self {
        Self {
            top: amount,
            bottom: amount,
            ..Self::default()
        }
    }
}

// ============================================================================
// COMPUTED LAYOUT
// ============================================================================

/// The computed layout result for a node.
///
/// After the layout engine computes positions and sizes, each node
/// receives a ComputedLayout with its final bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComputedLayout {
    /// The ID of the node this layout belongs to.
    pub id: LayoutNodeId,
    /// The computed bounds (position and size) in logical coordinates.
    pub bounds: Rect<Logical>,
}

impl ComputedLayout {
    /// Create a new computed layout.
    pub fn new(id: LayoutNodeId, bounds: Rect<Logical>) -> Self {
        Self { id, bounds }
    }

    /// Get the position.
    pub fn position(&self) -> Point<Logical> {
        self.bounds.origin
    }

    /// Get the size.
    pub fn size(&self) -> Size<Logical> {
        self.bounds.size
    }

    /// Get the x coordinate.
    pub fn x(&self) -> f32 {
        self.bounds.origin.x
    }

    /// Get the y coordinate.
    pub fn y(&self) -> f32 {
        self.bounds.origin.y
    }

    /// Get the width.
    pub fn width(&self) -> f32 {
        self.bounds.size.width
    }

    /// Get the height.
    pub fn height(&self) -> f32 {
        self.bounds.size.height
    }
}

// ============================================================================
// LAYOUT TREE
// ============================================================================

/// A complete layout tree with computed results.
#[derive(Debug, Clone)]
pub struct LayoutTree {
    /// All computed layouts, indexed by node ID.
    pub layouts: Vec<ComputedLayout>,
}

impl LayoutTree {
    /// Create an empty layout tree.
    pub fn new() -> Self {
        Self { layouts: Vec::new() }
    }

    /// Create a layout tree with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            layouts: Vec::with_capacity(capacity),
        }
    }

    /// Add a computed layout.
    pub fn push(&mut self, layout: ComputedLayout) {
        self.layouts.push(layout);
    }

    /// Find a layout by node ID.
    pub fn find(&self, id: LayoutNodeId) -> Option<&ComputedLayout> {
        self.layouts.iter().find(|l| l.id == id)
    }

    /// Get the number of layouts.
    pub fn len(&self) -> usize {
        self.layouts.len()
    }

    /// Check if the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }
}

impl Default for LayoutTree {
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
    fn test_layout_constraints_fixed() {
        let c = LayoutConstraints::fixed(100.0, 50.0);
        assert!(c.is_fixed_width());
        assert!(c.is_fixed_height());
        assert_eq!(c.min_width, 100.0);
        assert_eq!(c.max_width, 100.0);
    }

    #[test]
    fn test_layout_constraints_fill() {
        let c = LayoutConstraints::fill();
        assert!(!c.is_fixed_width());
        assert!(!c.is_fixed_height());
        assert_eq!(c.flex_grow, 1.0);
    }

    #[test]
    fn test_layout_node_leaf() {
        let node = LayoutNode::leaf(
            LayoutNodeId::new(1),
            LayoutConstraints::fixed(100.0, 50.0),
        );
        assert!(node.is_leaf());
        assert!(node.children.is_empty());
    }

    #[test]
    fn test_layout_node_container() {
        let child1 = LayoutNode::leaf(LayoutNodeId::new(2), LayoutConstraints::fill());
        let child2 = LayoutNode::leaf(LayoutNodeId::new(3), LayoutConstraints::fill());
        let parent = LayoutNode::container(
            LayoutNodeId::new(1),
            FlexDirection::Column,
            vec![child1, child2],
        );

        assert!(!parent.is_leaf());
        assert_eq!(parent.children.len(), 2);
        assert_eq!(parent.direction, FlexDirection::Column);
    }

    #[test]
    fn test_computed_layout() {
        let layout = ComputedLayout::new(
            LayoutNodeId::new(1),
            Rect::from_xywh(10.0, 20.0, 100.0, 50.0),
        );

        assert_eq!(layout.x(), 10.0);
        assert_eq!(layout.y(), 20.0);
        assert_eq!(layout.width(), 100.0);
        assert_eq!(layout.height(), 50.0);
    }

    #[test]
    fn test_layout_padding() {
        let p = LayoutPadding::uniform(10.0);
        assert_eq!(p.left, 10.0);
        assert_eq!(p.right, 10.0);
        assert_eq!(p.top, 10.0);
        assert_eq!(p.bottom, 10.0);

        let p = LayoutPadding::horizontal(5.0);
        assert_eq!(p.left, 5.0);
        assert_eq!(p.right, 5.0);
        assert_eq!(p.top, 0.0);
    }

    #[test]
    fn test_layout_tree() {
        let mut tree = LayoutTree::new();
        assert!(tree.is_empty());

        tree.push(ComputedLayout::new(
            LayoutNodeId::new(1),
            Rect::from_xywh(0.0, 0.0, 100.0, 100.0),
        ));
        tree.push(ComputedLayout::new(
            LayoutNodeId::new(2),
            Rect::from_xywh(0.0, 0.0, 50.0, 50.0),
        ));

        assert_eq!(tree.len(), 2);
        assert!(tree.find(LayoutNodeId::new(1)).is_some());
        assert!(tree.find(LayoutNodeId::new(99)).is_none());
    }
}
