//! Taffy-based layout engine implementation.
//!
//! This module provides a `LayoutEngine` implementation using the Taffy
//! layout library (CSS Flexbox-style layout).

use crate::core::{Bounds, Size};
use crate::core::Logical;

use super::engine::LayoutEngine;
use super::measurement::{measure_text_node, MeasureCache, MeasureContext};
use super::node::{ComputedLayout, LayoutNodeId};
use super::Layout;

use glyphon::FontSystem;
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
    /// The underlying Taffy tree with measure context support.
    inner: taffy::TaffyTree<MeasureContext>,
    /// Mapping from our LayoutNodeId to Taffy's NodeId.
    node_map: HashMap<LayoutNodeId, TaffyNodeId>,
    /// Mapping from LayoutNodeId to its children (for traversal).
    children_map: HashMap<LayoutNodeId, Vec<LayoutNodeId>>,
    /// Cache for text measurement results.
    cache: MeasureCache,
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
            cache: MeasureCache::new(),
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

    fn create_leaf_with_context(
        &mut self,
        layout: &Layout,
        context: MeasureContext,
    ) -> LayoutNodeId {
        let id = self.generate_id();
        let style = layout.to_taffy_style();
        let taffy_id = self.inner.new_leaf_with_context(style, context).unwrap();
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

    fn compute(
        &mut self,
        root: LayoutNodeId,
        available_size: Size<Logical>,
        font_system: &mut FontSystem,
    ) {
        if let Some(&root_taffy_id) = self.node_map.get(&root) {
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
    }

    fn get_layout(&self, node: LayoutNodeId) -> Option<ComputedLayout> {
        let taffy_id = self.node_map.get(&node)?;
        let layout = self.inner.layout(*taffy_id).ok()?;

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

    fn children(&self, node: LayoutNodeId) -> Vec<LayoutNodeId> {
        self.children_map.get(&node).cloned().unwrap_or_default()
    }

    fn clear(&mut self) {
        self.inner.clear();
        self.node_map.clear();
        self.children_map.clear();
        self.cache.clear();
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
        assert!(!engine.node_map.is_empty());

        engine.clear();
        assert!(engine.node_map.is_empty());
        assert!(engine.children_map.is_empty());
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
        });

        let text_node = engine.create_leaf_with_context(&Layout::default(), context);

        // Compute layout with narrow width
        engine.compute(text_node, Size::new(100.0, 600.0), &mut font_system);

        let layout = engine.get_layout(text_node).unwrap();

        // Text should wrap, so width should be constrained
        assert!(layout.width() <= 100.0, "Text should wrap to fit width");
        // Height should be multiple lines
        assert!(layout.height() > 24.0 * 1.2, "Wrapped text should have multiple lines");
    }
}
