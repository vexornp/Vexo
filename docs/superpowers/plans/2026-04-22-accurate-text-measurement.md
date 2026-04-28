# Accurate Text Measurement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement accurate text intrinsic size measurement using Taffy's measure callback system with glyphon/cosmic-text shaping.

**Architecture:** Create a measurement module with `TextMeasurer` that shapes text using glyphon's `Buffer` and `layout_runs()`. Update `TaffyLayoutEngine` to use `TaffyTree<MeasureContext>` with `compute_layout_with_measure()`. The measure function is called by Taffy during layout with the actual available space, enabling accurate wrapping-aware text sizing.

**Tech Stack:** Rust, Taffy layout engine, glyphon/cosmic-text for text shaping

---

## File Structure

| File | Purpose |
|------|---------|
| `vexo/src/layout/measurement.rs` | **New**: `TextMeasurer`, `MeasureCache`, `MeasureCacheKey`, `MeasureContext`, `TextMeasureContext` |
| `vexo/src/layout/engine.rs` | Update `LayoutEngine` trait with `create_leaf_with_context()` and new `compute()` signature |
| `vexo/src/layout/context.rs` | Update `LayoutContext` with `create_leaf_with_context()` |
| `vexo/src/layout/taffy_engine.rs` | Implement context nodes, measure callback, cache integration |
| `vexo/src/layout/mod.rs` | Export new types |
| `vexo/src/widgets/text.rs` | Add `line_height` field, use `create_leaf_with_context()` |
| `vexo/src/lib.rs` | Pass `font_system` to `layout_engine.compute()` |

---

### Task 1: Create measurement module with core types

**Files:**
- Create: `vexo/src/layout/measurement.rs`

- [ ] **Step 1: Create the measurement.rs file with all types**

```rust
//! Text measurement types for accurate intrinsic size calculation.
//!
//! This module provides types for measuring text dimensions using
//! glyphon/cosmic-text shaping, integrated with Taffy's measure callback.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use glyphon::{Attrs, Buffer, FontSystem, Metrics, Shaping};

// ============================================================================
// MEASURE CONTEXT TYPES
// ============================================================================

/// Context data for text measurement nodes.
#[derive(Debug, Clone)]
pub struct TextMeasureContext {
    /// The text content to measure.
    pub content: String,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Line height multiplier (default 1.2).
    pub line_height: f32,
}

/// Context for nodes that need custom measurement.
#[derive(Debug, Clone)]
pub enum MeasureContext {
    /// Text node with measurement parameters.
    Text(TextMeasureContext),
}

// ============================================================================
// TEXT MEASURER
// ============================================================================

/// Measures text dimensions using glyphon/cosmic-text.
pub struct TextMeasurer<'a> {
    font_system: &'a mut FontSystem,
}

impl<'a> TextMeasurer<'a> {
    /// Create a new text measurer with the given font system.
    pub fn new(font_system: &'a mut FontSystem) -> Self {
        Self { font_system }
    }

    /// Measure text with given constraints.
    ///
    /// Returns (width, height) in logical pixels.
    ///
    /// # Arguments
    /// - `content`: Text to measure
    /// - `font_size`: Font size in logical pixels
    /// - `line_height`: Line height multiplier
    /// - `available_width`: Available width for wrapping (None = infinite)
    /// - `available_height`: Available height (None = infinite)
    pub fn measure(
        &mut self,
        content: &str,
        font_size: f32,
        line_height: f32,
        available_width: Option<f32>,
        available_height: Option<f32>,
    ) -> (f32, f32) {
        let metrics = Metrics::new(font_size, font_size * line_height);
        let mut buffer = Buffer::new(self.font_system, metrics);

        // Set size constraints for wrapping
        buffer.set_size(self.font_system, available_width, available_height);

        // Set and shape the text
        buffer.set_text(self.font_system, content, &Attrs::new(), Shaping::Advanced);
        buffer.shape_until_scroll(self.font_system, true);

        // Calculate dimensions from layout runs
        let mut max_width = 0.0f32;
        let mut total_height = 0.0f32;

        for run in buffer.layout_runs() {
            max_width = max_width.max(run.line_w);
            total_height = total_height.max(run.line_top + run.line_height);
        }

        (max_width, total_height)
    }
}

// ============================================================================
// MEASURE CACHE
// ============================================================================

/// Cache key for measurement results.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeasureCacheKey {
    content_hash: u64,
    font_size_bits: u64,
    line_height_bits: u64,
    available_width_bits: u64,
    available_height_bits: u64,
}

impl MeasureCacheKey {
    /// Create a new cache key.
    pub fn new(
        content: &str,
        font_size: f32,
        line_height: f32,
        available_width: Option<f32>,
        available_height: Option<f32>,
    ) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        let content_hash = hasher.finish();

        Self {
            content_hash,
            font_size_bits: font_size.to_bits(),
            line_height_bits: line_height.to_bits(),
            available_width_bits: available_width.map(|f| f.to_bits()).unwrap_or(u64::MAX),
            available_height_bits: available_height.map(|f| f.to_bits()).unwrap_or(u64::MAX),
        }
    }
}

/// Cache for measurement results to avoid redundant text shaping.
pub struct MeasureCache {
    entries: HashMap<MeasureCacheKey, (f32, f32)>,
    max_entries: usize,
}

impl MeasureCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: 1000,
        }
    }

    /// Get a cached measurement.
    pub fn get(&self, key: &MeasureCacheKey) -> Option<(f32, f32)> {
        self.entries.get(key).copied()
    }

    /// Insert a measurement into the cache.
    pub fn insert(&mut self, key: MeasureCacheKey, size: (f32, f32)) {
        if self.entries.len() >= self.max_entries {
            // Simple eviction: clear all entries
            self.entries.clear();
        }
        self.entries.insert(key, size);
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for MeasureCache {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// MEASURE FUNCTION
// ============================================================================

/// Measure a text node for Taffy's measure callback.
///
/// This function is called by Taffy during layout computation with
/// the actual available space, enabling accurate text sizing.
pub fn measure_text_node(
    known_dimensions: taffy::prelude::Size<Option<f32>>,
    available_space: taffy::prelude::Size<taffy::prelude::AvailableSpace>,
    node_context: Option<&mut MeasureContext>,
    font_system: &mut FontSystem,
    cache: &mut MeasureCache,
) -> taffy::prelude::Size<f32> {
    use taffy::prelude::{AvailableSpace, Size};

    // If both dimensions are explicitly set, use them
    if let Size { width: Some(w), height: Some(h) } = known_dimensions {
        return Size { width: w, height: h };
    }

    let Some(context) = node_context else {
        return Size::ZERO;
    };

    match context {
        MeasureContext::Text(text_ctx) => {
            // Handle empty text
            if text_ctx.content.is_empty() {
                return Size {
                    width: known_dimensions.width.unwrap_or(0.0),
                    height: known_dimensions.height
                        .unwrap_or(text_ctx.font_size * text_ctx.line_height),
                };
            }

            // Convert AvailableSpace to Option<f32>
            let available_width = match available_space.width {
                AvailableSpace::Definite(w) if w > 0.0 => Some(w),
                AvailableSpace::Definite(_) => Some(1.0), // Minimum width
                _ => None,
            };
            let available_height = match available_space.height {
                AvailableSpace::Definite(h) => Some(h),
                _ => None,
            };

            // Check cache
            let key = MeasureCacheKey::new(
                &text_ctx.content,
                text_ctx.font_size,
                text_ctx.line_height,
                available_width,
                available_height,
            );

            if let Some((w, h)) = cache.get(&key) {
                return Size {
                    width: known_dimensions.width.unwrap_or(w),
                    height: known_dimensions.height.unwrap_or(h),
                };
            }

            // Measure and cache
            let mut measurer = TextMeasurer::new(font_system);
            let (w, h) = measurer.measure(
                &text_ctx.content,
                text_ctx.font_size,
                text_ctx.line_height,
                available_width,
                available_height,
            );
            cache.insert(key, (w, h));

            Size {
                width: known_dimensions.width.unwrap_or(w),
                height: known_dimensions.height.unwrap_or(h),
            }
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_font_system() -> FontSystem {
        let font_data = include_bytes!("../../resource/file/font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_measure_single_line() {
        let mut font_system = create_test_font_system();
        let mut measurer = TextMeasurer::new(&mut font_system);

        let (w, h) = measurer.measure("Hello", 24.0, 1.2, None, None);

        assert!(w > 0.0, "Width should be positive");
        assert!(h > 0.0, "Height should be positive");
        assert!(h < 24.0 * 1.5, "Height should be close to line height");
    }

    #[test]
    fn test_measure_with_wrapping() {
        let mut font_system = create_test_font_system();
        let mut measurer = TextMeasurer::new(&mut font_system);

        let (w1, h1) = measurer.measure("Hello World", 24.0, 1.2, None, None);
        let (w2, h2) = measurer.measure("Hello World", 24.0, 1.2, Some(50.0), None);

        // Wrapped text should be narrower but taller
        assert!(w2 < w1, "Wrapped text should be narrower");
        assert!(h2 > h1, "Wrapped text should be taller");
    }

    #[test]
    fn test_measure_multiline() {
        let mut font_system = create_test_font_system();
        let mut measurer = TextMeasurer::new(&mut font_system);

        let (w, h) = measurer.measure("Line1\nLine2\nLine3", 24.0, 1.2, None, None);

        // Should have height for 3 lines
        assert!(h >= 24.0 * 1.2 * 3.0, "Height should accommodate 3 lines");
    }

    #[test]
    fn test_measure_empty() {
        let mut font_system = create_test_font_system();
        let mut measurer = TextMeasurer::new(&mut font_system);

        let (w, h) = measurer.measure("", 24.0, 1.2, None, None);

        assert_eq!(w, 0.0, "Empty text should have zero width");
        assert!(h > 0.0, "Empty text should still have line height");
    }

    #[test]
    fn test_cache_hit() {
        let mut cache = MeasureCache::new();

        let key = MeasureCacheKey::new("test", 24.0, 1.2, None, None);
        cache.insert(key.clone(), (100.0, 30.0));

        let result = cache.get(&key);
        assert_eq!(result, Some((100.0, 30.0)));
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = MeasureCache::new();
        cache.max_entries = 2;

        cache.insert(MeasureCacheKey::new("a", 24.0, 1.2, None, None), (1.0, 1.0));
        cache.insert(MeasureCacheKey::new("b", 24.0, 1.2, None, None), (2.0, 2.0));
        cache.insert(MeasureCacheKey::new("c", 24.0, 1.2, None, None), (3.0, 3.0));

        // Cache should have been cleared when exceeding max_entries
        assert_eq!(cache.entries.len(), 1);
    }
}
```

- [ ] **Step 2: Run tests to verify the module compiles and tests pass**

Run: `cargo test -p vexo --lib layout::measurement`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/layout/measurement.rs
git commit -m "feat(layout): add text measurement module with TextMeasurer and MeasureCache"
```

---

### Task 2: Update LayoutEngine trait

**Files:**
- Modify: `vexo/src/layout/engine.rs`

- [ ] **Step 1: Update the LayoutEngine trait with new method and updated compute signature**

Replace the entire file content:

```rust
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
use crate::layout::measurement::MeasureContext;
use glyphon::FontSystem;

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

    /// Create a leaf node with custom measurement context.
    ///
    /// Used for nodes like text that need accurate intrinsic size calculation.
    /// The measure context is passed to Taffy's measure callback during layout.
    fn create_leaf_with_context(
        &mut self,
        layout: &Layout,
        context: MeasureContext,
    ) -> LayoutNodeId;

    /// Create a container node with children.
    ///
    /// Returns a handle to reference this node later.
    fn create_container(&mut self, layout: &Layout, children: &[LayoutNodeId]) -> LayoutNodeId;

    /// Compute layout for all nodes.
    ///
    /// Must be called after all nodes are created and before `get_layout()`.
    /// The font_system is used for text measurement during layout.
    fn compute(
        &mut self,
        root: LayoutNodeId,
        available_size: Size<Logical>,
        font_system: &mut FontSystem,
    );

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
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add vexo/src/layout/engine.rs
git commit -m "feat(layout): add create_leaf_with_context to LayoutEngine trait"
```

---

### Task 3: Update LayoutContext

**Files:**
- Modify: `vexo/src/layout/context.rs`

- [ ] **Step 1: Add create_leaf_with_context method to LayoutContext**

Replace the entire file content:

```rust
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
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add vexo/src/layout/context.rs
git commit -m "feat(layout): add create_leaf_with_context to LayoutContext"
```

---

### Task 4: Update TaffyLayoutEngine

**Files:**
- Modify: `vexo/src/layout/taffy_engine.rs`

- [ ] **Step 1: Update TaffyLayoutEngine to support context nodes and measure callback**

Replace the entire file content:

```rust
//! Taffy-based layout engine implementation.
//!
//! This module provides a `LayoutEngine` implementation using the Taffy
//! layout library (CSS Flexbox-style layout).

use crate::core::{Rect, Size};
use crate::core::Logical;
use glyphon::FontSystem;

use super::engine::LayoutEngine;
use super::measurement::{measure_text_node, MeasureCache, MeasureContext};
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
    /// The underlying Taffy tree with measure context support.
    inner: taffy::TaffyTree<MeasureContext>,
    /// Mapping from our LayoutNodeId to Taffy's NodeId.
    node_map: HashMap<LayoutNodeId, TaffyNodeId>,
    /// Mapping from LayoutNodeId to its children (for traversal).
    children_map: HashMap<LayoutNodeId, Vec<LayoutNodeId>>,
    /// Counter for generating unique node IDs.
    next_id: u64,
    /// Cache for text measurement results.
    cache: MeasureCache,
}

impl TaffyLayoutEngine {
    /// Create a new Taffy-based layout engine.
    pub fn new() -> Self {
        Self {
            inner: taffy::TaffyTree::new(),
            node_map: HashMap::new(),
            children_map: HashMap::new(),
            next_id: 0,
            cache: MeasureCache::new(),
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
                    measure_text_node(known_dimensions, available_space, node_context, font_system, cache)
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

    fn create_test_font_system() -> FontSystem {
        let font_data = include_bytes!("../../resource/file/font.ttf").to_vec();
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
    fn test_create_leaf_with_context() {
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        use super::super::measurement::TextMeasureContext;

        let context = MeasureContext::Text(TextMeasureContext {
            content: "Hello".to_string(),
            font_size: 24.0,
            line_height: 1.2,
        });

        let node_id = engine.create_leaf_with_context(&Layout::default(), context);

        engine.compute(node_id, Size::new(800.0, 600.0), &mut font_system);

        let computed = engine.get_layout(node_id).unwrap();
        // Should have some positive dimensions from text measurement
        assert!(computed.width() > 0.0);
        assert!(computed.height() > 0.0);
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
            &Layout::default().row(),
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

        let node = engine.create_leaf(&Layout::default());
        assert!(!engine.node_map.is_empty());

        engine.clear();
        assert!(engine.node_map.is_empty());
        assert!(engine.children_map.is_empty());
    }
}
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 3: Run tests**

Run: `cargo test -p vexo --lib layout::taffy_engine`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add vexo/src/layout/taffy_engine.rs
git commit -m "feat(layout): implement measure callback in TaffyLayoutEngine"
```

---

### Task 5: Update layout module exports

**Files:**
- Modify: `vexo/src/layout/mod.rs`

- [ ] **Step 1: Add measurement module and export new types**

Replace the file content:

```rust
//! Layout abstractions for the Vexo UI framework.
//!
//! This module provides the layout layer that sits between widgets and
//! the layout engine. It defines:
//!
//! - `LayoutEngine` trait for layout computation
//! - `LayoutNodeId` for node handles
//! - `ComputedLayout` for layout results
//! - `TaffyLayoutEngine` implementation
//! - `Layout` struct for CSS-style layout properties
//! - `LayoutContext` and `LayoutView` for widget interaction
//!
//! # Architecture
//!
//! The layout abstraction enables:
//! - Testing layout without Taffy dependency
//! - Swapping to different layout algorithms
//! - Centralized layout logic (not scattered in widgets)
//!
//! # Example
//!
//! ```
//! use vexo::layout::{LayoutEngine, TaffyLayoutEngine, Layout};
//!
//! let mut engine = TaffyLayoutEngine::new();
//!
//! // Or use the Layout struct for CSS-style properties
//! let layout = Layout::default()
//!     .padding(10.0)
//!     .margin(5.0)
//!     .flex_grow(1.0);
//! ```

mod context;
mod engine;
mod measurement;
mod node;
mod style;
mod taffy_engine;

pub use context::{LayoutContext, LayoutView};
pub use engine::{LayoutEngine, LayoutError};
pub use measurement::{
    MeasureCache,
    MeasureCacheKey,
    MeasureContext,
    TextMeasureContext,
    TextMeasurer,
};
pub use node::{
    AlignItems as NodeAlignItems,
    ComputedLayout,
    FlexDirection as NodeFlexDirection,
    LayoutConstraints,
    LayoutNodeId,
    LayoutPadding,
};
pub use style::{
    AlignContent,
    AlignItems,
    Dimension,
    Display,
    EdgeInsets,
    FlexDirection,
    FlexWrap,
    GridPlacement,
    Inset,
    JustifyContent,
    Layout,
    Position,
    TrackSizing,
};
pub use taffy_engine::TaffyLayoutEngine;
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add vexo/src/layout/mod.rs
git commit -m "feat(layout): export measurement types from layout module"
```

---

### Task 6: Update Text widget

**Files:**
- Modify: `vexo/src/widgets/text.rs`

- [ ] **Step 1: Update Text widget with line_height field and use create_leaf_with_context**

Replace the entire file content:

```rust
use crate::core::{Logical, Point};
use crate::layout::{Layout, LayoutContext, LayoutNodeId, LayoutView, MeasureContext, TextMeasureContext};
use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::Color;
use crate::input::InputEvent;

pub struct Text {
    pub content: String,
    pub font_size: f32,
    pub color: Color,
    pub key: Option<String>,
    pub layout: Layout,
    pub line_height: f32,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            font_size: 24.0,
            color: Color::BLACK,
            key: None,
            layout: Layout::default(),
            line_height: 1.2,
        }
    }

    /// Set the font size.
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set fixed width.
    pub fn width(mut self, value: f32) -> Self {
        self.layout = self.layout.width(value);
        self
    }

    /// Set fixed height.
    pub fn height(mut self, value: f32) -> Self {
        self.layout = self.layout.height(value);
        self
    }

    /// Set uniform padding on all sides.
    pub fn padding(mut self, value: f32) -> Self {
        self.layout = self.layout.padding(value);
        self
    }

    /// Set uniform margin on all sides.
    pub fn margin(mut self, value: f32) -> Self {
        self.layout = self.layout.margin(value);
        self
    }

    /// Set flex grow factor.
    pub fn flex_grow(mut self, value: f32) -> Self {
        self.layout = self.layout.flex_grow(value);
        self
    }

    /// Set the entire Layout struct.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Set custom line height multiplier.
    ///
    /// Default is 1.2. A value of 1.5 gives 50% extra spacing between lines.
    pub fn line_height(mut self, multiplier: f32) -> Self {
        self.line_height = multiplier;
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Text {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeId {
        // Create measurement context for accurate text sizing
        let measure_context = MeasureContext::Text(TextMeasureContext {
            content: self.content.clone(),
            font_size: self.font_size,
            line_height: self.line_height,
        });

        // Create node with context - Taffy will call measure during compute
        layout_context.create_leaf_with_context(&self.layout, measure_context)
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        _cursor_blink: &crate::CursorBlinkState,
        widget_context: &mut WidgetContext,
    ) {
        if let Some(layout) = layout_view.get_layout(node) {
            let pos = Point::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            renderer.add_text(self.content.clone(), pos, self.font_size, self.color);
        }
    }

    fn on_event(
        &mut self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_context: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        WidgetResponse::default()
    }
}
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add vexo/src/widgets/text.rs
git commit -m "feat(widgets): use measure callback for accurate Text intrinsic size"
```

---

### Task 7: Update WindowState to pass font_system

**Files:**
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Update the compute call in render() to pass font_system**

Find the line in `WindowState::render()`:
```rust
self.layout_engine.compute(new_root_node_id, logical_size);
```

Replace with:
```rust
self.layout_engine.compute(new_root_node_id, logical_size, &mut self.widget_context.font_system);
```

- [ ] **Step 2: Run cargo check to verify compilation**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 3: Run the desktop demo to verify it works**

Run: `cargo run -p desktop_demo`
Expected: Application runs without errors

- [ ] **Step 4: Commit**

```bash
git add vexo/src/lib.rs
git commit -m "feat: pass font_system to layout_engine.compute for text measurement"
```

---

### Task 8: Add integration test

**Files:**
- Modify: `vexo/src/layout/taffy_engine.rs` (add test)

- [ ] **Step 1: Add integration test for text widget layout**

Add to the tests module in `vexo/src/layout/taffy_engine.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify**

Run: `cargo test -p vexo --lib layout::taffy_engine`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/layout/taffy_engine.rs
git commit -m "test(layout): add integration tests for text measurement"
```

---

### Task 9: Final verification and cleanup

- [ ] **Step 1: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 2: Run the desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Application runs correctly with text displayed

- [ ] **Step 3: Run clippy for linting**

Run: `cargo clippy -p vexo`
Expected: No warnings

- [ ] **Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: address any remaining issues"
```
