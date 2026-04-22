# Accurate Text Measurement via Taffy Measure Callback

**Date:** 2026-04-22
**Status:** Approved

## Problem

The Text widget currently estimates intrinsic size using a simple formula:

```rust
let intrinsic_width = self.content.len() as f32 * (self.font_size * 0.5);
let intrinsic_height = self.font_size * 1.2;
```

This approach is inaccurate because:
- Character widths vary significantly (e.g., 'i' vs 'W')
- Doesn't handle multi-line text with wrapping
- Doesn't account for font-specific metrics
- Doesn't support bidirectional text, ligatures, or complex shaping

## Solution

Use Taffy's built-in measure callback system (`compute_layout_with_measure`) with `new_leaf_with_context`. This is the standard pattern used by Servo, Bevy, Zed, and Lapce.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Widget Layer                                 │
│  Text::layout() → create_leaf_with_context(TextMeasureContext) │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Layout Layer                                 │
│  TaffyLayoutEngine                                             │
│    - new_leaf_with_context(style, context)                     │
│    - compute_layout_with_measure(measure_fn)                   │
│    - measure_fn(available_space, context) → Size               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Text Measurement                             │
│  TextMeasurer                                                  │
│    - uses FontSystem to shape text                             │
│    - respects available_space for wrapping                     │
│    - returns (width, height)                                   │
└─────────────────────────────────────────────────────────────────┘
```

Taffy's `compute_layout_with_measure` solves the chicken-and-egg problem by calling back into the measure function with the available space at the right moment during layout resolution.

## Data Structures

### TextMeasureContext

Node-specific data attached to Taffy nodes:

```rust
/// Context data for text measurement nodes.
pub struct TextMeasureContext {
    /// The text content to measure.
    pub content: String,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Line height multiplier (default 1.2).
    pub line_height: f32,
}
```

### MeasureContext

Enum for all measurable node types (extensible):

```rust
/// Context for nodes that need custom measurement.
pub enum MeasureContext {
    /// Text node with measurement parameters.
    Text(TextMeasureContext),
    // Future: Image(ImageMeasureContext),
}
```

### LayoutEngine Trait Update

```rust
pub trait LayoutEngine {
    // Existing methods...
    fn create_leaf(&mut self, layout: &Layout) -> LayoutNodeId;

    // New method for nodes with custom measurement
    fn create_leaf_with_context(
        &mut self,
        layout: &Layout,
        context: MeasureContext
    ) -> LayoutNodeId;

    // Updated compute method
    fn compute(
        &mut self,
        root: LayoutNodeId,
        available_size: Size<Logical>,
        font_system: &mut FontSystem,
    );
}
```

## TextMeasurer Implementation

```rust
use glyphon::{Buffer, FontSystem, Metrics, Attrs, Shaping};

/// Measures text dimensions using glyphon/cosmic-text.
pub struct TextMeasurer<'a> {
    font_system: &'a mut FontSystem,
}

impl<'a> TextMeasurer<'a> {
    pub fn new(font_system: &'a mut FontSystem) -> Self {
        Self { font_system }
    }

    /// Measure text with given constraints.
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

        buffer.set_size(
            self.font_system,
            available_width,
            available_height,
        );

        buffer.set_text(
            self.font_system,
            content,
            &Attrs::new(),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(self.font_system, true);

        let mut max_width = 0.0f32;
        let mut total_height = 0.0f32;

        for run in buffer.layout_runs() {
            max_width = max_width.max(run.line_w);
            total_height = total_height.max(run.line_top + run.line_height);
        }

        (max_width, total_height)
    }
}
```

## TaffyLayoutEngine Changes

```rust
use taffy::prelude::*;

pub struct TaffyLayoutEngine {
    inner: TaffyTree<MeasureContext>,  // Now generic over context
    node_map: HashMap<LayoutNodeId, TaffyNodeId>,
    children_map: HashMap<LayoutNodeId, Vec<LayoutNodeId>>,
    next_id: u64,
    cache: MeasureCache,  // Added for performance
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
        context: MeasureContext
    ) -> LayoutNodeId {
        let id = self.generate_id();
        let style = layout.to_taffy_style();
        let taffy_id = self.inner.new_leaf_with_context(style, context).unwrap();
        self.node_map.insert(id, taffy_id);
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
}
```

## Measure Function

```rust
fn measure_text_node(
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    node_context: Option<&mut MeasureContext>,
    font_system: &mut FontSystem,
    cache: &mut MeasureCache,
) -> Size<f32> {
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
                    height: known_dimensions.height.unwrap_or(text_ctx.font_size * text_ctx.line_height),
                };
            }

            // Convert AvailableSpace to Option<f32>
            let available_width = match available_space.width {
                AvailableSpace::Definite(w) if w > 0.0 => Some(w),
                AvailableSpace::Definite(_) => Some(1.0),
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
```

## Text Widget Changes

### Text struct update

Add `line_height` field to the Text struct:

```rust
pub struct Text {
    pub content: String,
    pub font_size: f32,
    pub color: Color,
    pub key: Option<String>,
    pub layout: Layout,
    pub line_height: f32,  // New field
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            font_size: 24.0,
            color: Color::BLACK,
            key: None,
            layout: Layout::default(),
            line_height: 1.2,  // Default line height multiplier
        }
    }

    /// Set custom line height multiplier.
    pub fn line_height(mut self, multiplier: f32) -> Self {
        self.line_height = multiplier;
        self
    }
}
```

### Widget implementation

```rust
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Text {
    fn layout(
        &mut self,
        layout_context: &mut LayoutContext,
        widget_context: &mut WidgetContext
    ) -> LayoutNodeId {
        let measure_context = MeasureContext::Text(TextMeasureContext {
            content: self.content.clone(),
            font_size: self.font_size,
            line_height: self.line_height,
        });

        let layout = self.layout.clone();
        layout_context.create_leaf_with_context(&layout, measure_context)
    }
}
```

## Performance: Measurement Caching

Text shaping is expensive. Taffy may call the measure function multiple times per node during layout resolution. A cache avoids redundant measurements.

```rust
use std::hash::{Hash, Hasher};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MeasureCacheKey {
    content_hash: u64,
    font_size_bits: u64,
    line_height_bits: u64,
    available_width_bits: u64,
    available_height_bits: u64,
}

impl MeasureCacheKey {
    fn new(
        content: &str,
        font_size: f32,
        line_height: f32,
        available_width: Option<f32>,
        available_height: Option<f32>,
    ) -> Self {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
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

struct MeasureCache {
    entries: HashMap<MeasureCacheKey, (f32, f32)>,
    max_entries: usize,
}

impl MeasureCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_entries: 1000,
        }
    }

    fn get(&self, key: &MeasureCacheKey) -> Option<(f32, f32)> {
        self.entries.get(key).copied()
    }

    fn insert(&mut self, key: MeasureCacheKey, size: (f32, f32)) {
        if self.entries.len() >= self.max_entries {
            // Simple eviction: clear all entries
            self.entries.clear();
        }
        self.entries.insert(key, size);
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}
```

Cache invalidation:
- Content changes → different hash → cache miss → re-measure
- Font size/line height changes → different key → re-measure
- Available space changes → different key → re-measure
- Frame-to-frame with same text/constraints → cache hit

The cache is stored in `TaffyLayoutEngine` and cleared on `clear()`.

## Error Handling

| Case | Handling |
|------|----------|
| Empty text | Returns width 0, height = font_size * line_height |
| Zero/negative available space | Minimum width of 1.0 |
| Explicit dimensions | Override measurement via `known_dimensions` |
| Missing font | cosmic-text uses font fallback (tofu characters) |

## Testing

### Unit Tests

- `test_measure_single_line`: Verify basic measurement
- `test_measure_with_wrapping`: Verify wrapping behavior
- `test_measure_multiline`: Verify `\n` handling
- `test_measure_empty`: Verify empty text edge case
- `test_measure_cache_hit`: Verify cache effectiveness

### Integration Tests

- `test_text_widget_accurate_layout`: End-to-end verification

## Implementation Plan

### Files to Modify

| File | Changes |
|------|---------|
| `vexo/src/layout/engine.rs` | Add `create_leaf_with_context()` to `LayoutEngine` trait |
| `vexo/src/layout/taffy_engine.rs` | Implement context nodes, measure callback, cache |
| `vexo/src/layout/mod.rs` | Export `MeasureContext`, `TextMeasureContext` |
| `vexo/src/widgets/text.rs` | Use `create_leaf_with_context()` instead of estimation |
| `vexo/src/lib.rs` | Pass `font_system` to `layout_engine.compute()` |
| `vexo/src/layout/measurement.rs` | **New file**: `TextMeasurer`, `MeasureCache` |

### Implementation Order

1. Create `measurement.rs` with `TextMeasurer` and `MeasureCache`
2. Update `LayoutEngine` trait with `create_leaf_with_context()`
3. Update `TaffyLayoutEngine` to support context nodes and measure callback
4. Update `Text` widget to use new API
5. Update `WindowState::render()` to pass `font_system`
6. Add tests

### Backward Compatibility

- `create_leaf()` still works for non-text nodes (Container, Button, etc.)
- Existing widgets unchanged except `Text`
- `LayoutEngine::compute()` signature change requires updating call sites
