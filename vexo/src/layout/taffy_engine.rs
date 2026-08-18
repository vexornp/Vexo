//! Taffy-based layout engine implementation.
//!
//! This module provides a `LayoutEngine` implementation using the Taffy
//! layout library (CSS Flexbox-style layout).

use slotmap::SlotMap;

use crate::core::Logical;
use crate::core::{Bounds, Size};

use super::engine::LayoutEngine;
use super::measurement::{measure_text_node, MeasureCache, MeasureContext};
use super::node::{ComputedLayout, LayoutNodeKey};
use super::Layout;

use glyphon::FontSystem;
use taffy::prelude::{AvailableSpace, NodeId as TaffyNodeId};

// ============================================================================
// TAFFY LAYOUT ENGINE
// ============================================================================

/// Per-node metadata stored alongside Taffy's tree.
///
/// Co-locating `taffy_id` and `children` in one entry keeps identity and
/// topology in sync structurally: removing the slot frees both fields
/// atomically, so there is no second map that can drift out of sync.
struct NodeEntry {
    /// The corresponding Taffy node id.
    taffy_id: TaffyNodeId,
    /// Children of this node, in our own key space.
    children: Vec<LayoutNodeKey>,
}

/// Layout engine implementation using Taffy.
///
/// This engine wraps the Taffy library and provides a `LayoutEngine`
/// implementation using CSS Flexbox-style layout. The Taffy tree persists
/// across frames, enabling incremental updates via `set_style()`,
/// `set_context()`, `add_child()`, etc.
pub struct TaffyLayoutEngine {
    /// The underlying Taffy tree with measure context support.
    inner: taffy::TaffyTree<MeasureContext>,
    /// Per-node metadata: Taffy id + children, keyed by LayoutNodeKey.
    nodes: SlotMap<LayoutNodeKey, NodeEntry>,
    /// Cache for text measurement results.
    cache: MeasureCache,
}

impl TaffyLayoutEngine {
    /// Create a new Taffy-based layout engine.
    pub fn new() -> Self {
        Self {
            inner: taffy::TaffyTree::new(),
            nodes: SlotMap::with_key(),
            cache: MeasureCache::new(),
        }
    }
}

impl Default for TaffyLayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine for TaffyLayoutEngine {
    // ========================================================================
    // Node creation
    // ========================================================================

    fn create_leaf(&mut self, layout: &Layout) -> LayoutNodeKey {
        let style = layout.to_taffy_style();
        let taffy_id = self.inner.new_leaf(style).unwrap();
        self.nodes.insert(NodeEntry {
            taffy_id,
            children: Vec::new(),
        })
    }

    fn create_leaf_with_context(
        &mut self,
        layout: &Layout,
        context: MeasureContext,
    ) -> LayoutNodeKey {
        let style = layout.to_taffy_style();
        let taffy_id = self.inner.new_leaf_with_context(style, context).unwrap();
        self.nodes.insert(NodeEntry {
            taffy_id,
            children: Vec::new(),
        })
    }

    fn create_container(&mut self, layout: &Layout, children: &[LayoutNodeKey]) -> LayoutNodeKey {
        let style = layout.to_taffy_style();

        let child_taffy_ids: Vec<TaffyNodeId> = children
            .iter()
            .filter_map(|k| self.nodes.get(*k).map(|e| e.taffy_id))
            .collect();

        let taffy_id = self
            .inner
            .new_with_children(style, &child_taffy_ids)
            .unwrap();
        self.nodes.insert(NodeEntry {
            taffy_id,
            children: children.to_vec(),
        })
    }

    // ========================================================================
    // Incremental updates
    // ========================================================================

    fn set_style(&mut self, node: LayoutNodeKey, layout: &Layout) {
        let taffy_id = match self.nodes.get(node).map(|e| e.taffy_id) {
            Some(id) => id,
            None => return,
        };
        let style = layout.to_taffy_style();
        let _ = self.inner.set_style(taffy_id, style);
        // Taffy's set_style() internally calls mark_dirty
    }

    fn set_context(&mut self, node: LayoutNodeKey, context: MeasureContext) {
        let taffy_id = match self.nodes.get(node).map(|e| e.taffy_id) {
            Some(id) => id,
            None => return,
        };
        let _ = self.inner.set_node_context(taffy_id, Some(context));
        // Taffy's set_node_context() internally calls mark_dirty
    }

    fn add_child(&mut self, parent: LayoutNodeKey, child: LayoutNodeKey) {
        let parent_id = match self.nodes.get(parent).map(|e| e.taffy_id) {
            Some(id) => id,
            None => return,
        };
        let child_id = match self.nodes.get(child).map(|e| e.taffy_id) {
            Some(id) => id,
            None => return,
        };
        let _ = self.inner.add_child(parent_id, child_id);
        if let Some(entry) = self.nodes.get_mut(parent) {
            entry.children.push(child);
        }
    }

    fn remove_child(&mut self, parent: LayoutNodeKey, child: LayoutNodeKey) {
        let parent_id = match self.nodes.get(parent).map(|e| e.taffy_id) {
            Some(id) => id,
            None => return,
        };
        let child_id = match self.nodes.get(child).map(|e| e.taffy_id) {
            Some(id) => id,
            None => return,
        };
        let _ = self.inner.remove_child(parent_id, child_id);
        if let Some(entry) = self.nodes.get_mut(parent) {
            entry.children.retain(|&k| k != child);
        }
    }

    fn set_children(&mut self, parent: LayoutNodeKey, children: &[LayoutNodeKey]) {
        let parent_id = match self.nodes.get(parent).map(|e| e.taffy_id) {
            Some(id) => id,
            None => return,
        };
        let child_taffy_ids: Vec<TaffyNodeId> = children
            .iter()
            .filter_map(|k| self.nodes.get(*k).map(|e| e.taffy_id))
            .collect();
        let _ = self.inner.set_children(parent_id, &child_taffy_ids);
        if let Some(entry) = self.nodes.get_mut(parent) {
            entry.children = children.to_vec();
        }
    }

    fn remove_node(&mut self, node: LayoutNodeKey) {
        if let Some(entry) = self.nodes.remove(node) {
            let _ = self.inner.remove(entry.taffy_id);
        }
    }

    fn mark_dirty(&mut self, node: LayoutNodeKey) {
        if let Some(taffy_id) = self.nodes.get(node).map(|e| e.taffy_id) {
            let _ = self.inner.mark_dirty(taffy_id);
        }
    }

    fn is_dirty(&self, node: LayoutNodeKey) -> bool {
        match self.nodes.get(node).map(|e| e.taffy_id) {
            Some(taffy_id) => self.inner.dirty(taffy_id).unwrap_or(true),
            None => true,
        }
    }

    // ========================================================================
    // Computation and readback
    // ========================================================================

    fn set_root_size(&mut self, root: LayoutNodeKey) {
        let taffy_id = match self.nodes.get(root).map(|e| e.taffy_id) {
            Some(id) => id,
            None => return,
        };
        if let Ok(existing_style) = self.inner.style(taffy_id).cloned() {
            let root_style = taffy::Style {
                size: taffy::geometry::Size {
                    width: taffy::style::LengthPercentage::percent(1.0).into(),
                    height: taffy::style::LengthPercentage::percent(1.0).into(),
                },
                ..existing_style
            };
            let _ = self.inner.set_style(taffy_id, root_style);
        }
    }

    fn compute(
        &mut self,
        root: LayoutNodeKey,
        available_size: Size<Logical>,
        font_system: &mut FontSystem,
    ) {
        let root_taffy_id = match self.nodes.get(root).map(|e| e.taffy_id) {
            Some(id) => id,
            None => return,
        };
        let cache = &mut self.cache;
        let _ = self.inner.compute_layout_with_measure(
            root_taffy_id,
            taffy::Size {
                width: AvailableSpace::Definite(available_size.width),
                height: AvailableSpace::Definite(available_size.height),
            },
            |known_dimensions, available_space, _node_id, node_context, _style| {
                measure_text_node(
                    known_dimensions,
                    available_space,
                    node_context,
                    font_system,
                    cache,
                )
            },
        );
    }

    fn get_layout(&self, node: LayoutNodeKey) -> Option<ComputedLayout> {
        let taffy_id = self.nodes.get(node)?.taffy_id;
        let layout = self.inner.layout(taffy_id).ok()?;

        Some(ComputedLayout::new(
            node,
            Bounds::from_xywh(
                layout.location.x,
                layout.location.y,
                layout.size.width,
                layout.size.height,
            ),
        ))
    }

    fn children(&self, node: LayoutNodeKey) -> Vec<LayoutNodeKey> {
        self.nodes
            .get(node)
            .map(|e| e.children.clone())
            .unwrap_or_default()
    }

    fn clear(&mut self) {
        self.inner.clear();
        self.nodes.clear();
        self.cache.clear();
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::FlexDirection;

    fn create_test_font_system() -> FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_create_leaf() {
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let layout = Layout::default().width(100.0).height(50.0);
        let node_id = engine.create_leaf(&layout);

        engine.compute(node_id, Size::new(200.0, 200.0), &mut font_system);

        let computed = engine.get_layout(node_id).unwrap();
        assert_eq!(computed.width(), 100.0);
        assert_eq!(computed.height(), 50.0);
    }

    #[test]
    fn test_create_container() {
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create two leaf children
        let child1 = engine.create_leaf(&Layout::default().width(50.0).height(50.0));
        let child2 = engine.create_leaf(&Layout::default().width(75.0).height(50.0));

        // Create a row container
        let parent = engine.create_container(
            &Layout::default().flex_direction(FlexDirection::Row),
            &[child1, child2],
        );

        engine.compute(parent, Size::new(200.0, 100.0), &mut font_system);

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
        assert!(!engine.nodes.is_empty());

        engine.clear();
        assert!(engine.nodes.is_empty());
    }

    #[test]
    fn test_text_widget_accurate_layout() {
        use super::super::measurement::TextMeasureContext;

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create a text node with known content
        let context = MeasureContext::Text(TextMeasureContext {
            content: "Hello World".to_string(),
            font_size: 24.0,
            line_height: 1.2,
            font_family: None,
            max_lines: None,
        });

        let text_node = engine.create_leaf_with_context(&Layout::default(), context);

        // Compute layout with available space
        engine.compute(text_node, Size::new(800.0, 600.0), &mut font_system);

        let layout = engine.get_layout(text_node).unwrap();

        // The width should be accurate based on actual glyph widths
        // "Hello World" at 24px should be roughly 100-150px wide
        assert!(layout.width() > 50.0, "Text width should be reasonable");
        assert!(layout.width() < 300.0, "Text width should not be excessive");
        assert!(layout.height() > 0.0, "Text height should be positive");
    }

    #[test]
    fn test_text_widget_with_wrapping() {
        use super::super::measurement::TextMeasureContext;

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create a text node with long content
        let context = MeasureContext::Text(TextMeasureContext {
            content: "This is a long text that should wrap when constrained".to_string(),
            font_size: 24.0,
            line_height: 1.2,
            font_family: None,
            max_lines: None,
        });

        let text_node = engine.create_leaf_with_context(&Layout::default(), context);

        // Compute layout with narrow width
        engine.compute(text_node, Size::new(100.0, 600.0), &mut font_system);

        let layout = engine.get_layout(text_node).unwrap();

        // Text should wrap, so width should be constrained
        assert!(layout.width() <= 100.0, "Text should wrap to fit width");
        // Height should be multiple lines
        assert!(
            layout.height() > 24.0 * 1.2,
            "Wrapped text should have multiple lines"
        );
    }

    // ========================================================================
    // Incremental update tests
    // ========================================================================

    #[test]
    fn test_set_style_updates_layout() {
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create a leaf with initial size
        let node = engine.create_leaf(&Layout::default().width(100.0).height(50.0));
        engine.compute(node, Size::new(200.0, 200.0), &mut font_system);

        let computed = engine.get_layout(node).unwrap();
        assert_eq!(computed.width(), 100.0);
        assert_eq!(computed.height(), 50.0);

        // Update style to a larger size
        engine.set_style(node, &Layout::default().width(150.0).height(75.0));
        assert!(engine.is_dirty(node));

        // Recompute
        engine.compute(node, Size::new(200.0, 200.0), &mut font_system);

        let computed = engine.get_layout(node).unwrap();
        assert_eq!(computed.width(), 150.0);
        assert_eq!(computed.height(), 75.0);
        assert!(!engine.is_dirty(node));
    }

    #[test]
    fn test_set_context_updates_measurement() {
        use super::super::measurement::TextMeasureContext;

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create a text node
        let ctx = MeasureContext::Text(TextMeasureContext {
            content: "Hello".to_string(),
            font_size: 24.0,
            line_height: 1.2,
            font_family: None,
            max_lines: None,
        });
        let node = engine.create_leaf_with_context(&Layout::default(), ctx);
        engine.compute(node, Size::new(800.0, 600.0), &mut font_system);

        let layout1 = engine.get_layout(node).unwrap();
        let width1 = layout1.width();

        // Change text content to something longer
        let ctx2 = MeasureContext::Text(TextMeasureContext {
            content: "Hello World Longer Text".to_string(),
            font_size: 24.0,
            line_height: 1.2,
            font_family: None,
            max_lines: None,
        });
        engine.set_context(node, ctx2);
        assert!(engine.is_dirty(node));

        // Recompute
        engine.compute(node, Size::new(800.0, 600.0), &mut font_system);

        let layout2 = engine.get_layout(node).unwrap();
        assert!(
            layout2.width() > width1,
            "Longer text should have wider layout"
        );
        assert!(!engine.is_dirty(node));
    }

    #[test]
    fn test_add_child() {
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create a container with one child
        let child1 = engine.create_leaf(&Layout::default().width(50.0).height(50.0));
        let parent = engine.create_container(
            &Layout::default().flex_direction(FlexDirection::Row),
            &[child1],
        );
        engine.compute(parent, Size::new(200.0, 100.0), &mut font_system);

        let parent_layout = engine.get_layout(parent).unwrap();
        assert_eq!(parent_layout.width(), 50.0);

        // Add a second child
        let child2 = engine.create_leaf(&Layout::default().width(75.0).height(50.0));
        engine.add_child(parent, child2);
        assert!(engine.is_dirty(parent));

        // Recompute
        engine.compute(parent, Size::new(200.0, 100.0), &mut font_system);

        let parent_layout = engine.get_layout(parent).unwrap();
        assert_eq!(parent_layout.width(), 125.0); // 50 + 75

        // Verify children map was updated
        let children = engine.children(parent);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], child1);
        assert_eq!(children[1], child2);
    }

    #[test]
    fn test_remove_child() {
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create a container with two children
        let child1 = engine.create_leaf(&Layout::default().width(50.0).height(50.0));
        let child2 = engine.create_leaf(&Layout::default().width(75.0).height(50.0));
        let parent = engine.create_container(
            &Layout::default().flex_direction(FlexDirection::Row),
            &[child1, child2],
        );
        engine.compute(parent, Size::new(200.0, 100.0), &mut font_system);

        // Remove one child
        engine.remove_child(parent, child2);
        assert!(engine.is_dirty(parent));

        // Recompute
        engine.compute(parent, Size::new(200.0, 100.0), &mut font_system);

        let parent_layout = engine.get_layout(parent).unwrap();
        assert_eq!(parent_layout.width(), 50.0); // only child1 remains

        // Verify children map was updated
        let children = engine.children(parent);
        assert_eq!(children.len(), 1);
        assert_eq!(children[0], child1);
    }

    #[test]
    fn test_remove_node() {
        let mut engine = TaffyLayoutEngine::new();

        let child = engine.create_leaf(&Layout::default().width(50.0).height(50.0));
        let parent = engine.create_container(
            &Layout::default().flex_direction(FlexDirection::Row),
            &[child],
        );

        // Remove the child node entirely
        engine.remove_node(child);

        // The child should no longer be accessible
        assert!(engine.get_layout(child).is_none());
        assert!(engine.nodes.get(child).is_none());

        // Parent's children map should not contain the removed child
        // (But note: Taffy may still hold the node in its tree until
        //  the parent's child list is updated via remove_child/set_children)
    }

    #[test]
    fn test_set_children_reorder() {
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child1 = engine.create_leaf(&Layout::default().width(50.0).height(50.0));
        let child2 = engine.create_leaf(&Layout::default().width(75.0).height(50.0));
        let parent = engine.create_container(
            &Layout::default().flex_direction(FlexDirection::Row),
            &[child1, child2],
        );
        engine.compute(parent, Size::new(200.0, 100.0), &mut font_system);

        // Reverse children order
        engine.set_children(parent, &[child2, child1]);
        assert!(engine.is_dirty(parent));

        engine.compute(parent, Size::new(200.0, 100.0), &mut font_system);

        let c1_layout = engine.get_layout(child1).unwrap();
        let c2_layout = engine.get_layout(child2).unwrap();

        // Now child2 should be first (at x=0), child1 second (at x=75)
        assert_eq!(c2_layout.x(), 0.0);
        assert!(c1_layout.x() >= 75.0);
    }

    #[test]
    fn test_mark_dirty_propagates() {
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child = engine.create_leaf(&Layout::default().width(50.0).height(50.0));
        let parent = engine.create_container(
            &Layout::default().flex_direction(FlexDirection::Row),
            &[child],
        );
        let grandparent = engine.create_container(
            &Layout::default().flex_direction(FlexDirection::Column),
            &[parent],
        );
        engine.compute(grandparent, Size::new(200.0, 200.0), &mut font_system);

        // Everything should be clean after compute
        assert!(!engine.is_dirty(child));
        assert!(!engine.is_dirty(parent));
        assert!(!engine.is_dirty(grandparent));

        // Mark child dirty — should propagate to parent and grandparent
        engine.mark_dirty(child);
        assert!(engine.is_dirty(child));
        assert!(engine.is_dirty(parent));
        assert!(engine.is_dirty(grandparent));
    }
}
