# Taffy Integration for Retain-Mode Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate Taffy layout engine into retain-mode pipeline so RenderObjects create and manage their own Taffy nodes.

**Architecture:** Each RenderObject creates Taffy node(s) during layout, pipeline orchestrates the three-phase flow (build tree → compute → apply), modifiers pass through to children.

**Tech Stack:** Rust, Taffy layout engine, LayoutEngine trait, MeasureContext for text

---

## File Structure

**Files to modify:**
- `vexo/src/retain/render_object.rs` - Add LayoutContext with engine access, LayoutResult type, update trait
- `vexo/src/retain/pipeline.rs` - Wire up Taffy engine, add apply_layout phase
- `vexo/src/retain/render_objects/text.rs` - Implement layout with text measure context
- `vexo/src/retain/widgets/background.rs` - Update BackgroundRenderObject
- `vexo/src/retain/widgets/border.rs` - Update BorderRenderObject
- `vexo/src/retain/widgets/corner_radius.rs` - Update CornerRadiusRenderObject
- `vexo/src/retain/widgets/container.rs` - Update ContainerRenderObject (if exists)

---

### Task 1: Add LayoutResult type and update LayoutContext

**Files:**
- Modify: `vexo/src/retain/render_object.rs`

- [ ] **Step 1: Add LayoutResult type**

```rust
// In vexo/src/retain/render_object.rs, after the imports:

use crate::layout::{LayoutEngine, LayoutNodeId};

// Add after the imports section:

/// Result of a RenderObject's layout operation.
///
/// Contains the Taffy node ID and computed size.
pub struct LayoutResult {
    /// The Taffy node ID for this render object.
    pub node: LayoutNodeId,
    /// The computed size (available after Taffy computation).
    pub size: Size<crate::core::Logical>,
}
```

- [ ] **Step 2: Update LayoutContext to hold engine and font_system**

```rust
// Replace the existing LayoutContext struct and impl:

/// Context passed to RenderObject.layout().
///
/// Provides access to the layout engine and font system for text measurement.
pub struct LayoutContext<'a> {
    engine: &'a mut dyn LayoutEngine,
    font_system: &'a mut glyphon::FontSystem,
}

impl<'a> LayoutContext<'a> {
    /// Create a new layout context.
    pub fn new(engine: &'a mut dyn LayoutEngine, font_system: &'a mut glyphon::FontSystem) -> Self {
        Self { engine, font_system }
    }

    /// Get the layout engine.
    pub fn engine(&mut self) -> &mut dyn LayoutEngine {
        self.engine
    }

    /// Get the font system.
    pub fn font_system(&mut self) -> &mut glyphon::FontSystem {
        self.font_system
    }
}
```

- [ ] **Step 3: Update RenderObject trait signature**

```rust
// Replace the existing RenderObject trait layout method signature:

pub trait RenderObject {
    /// Perform layout with the layout engine, creating Taffy node(s).
    ///
    /// Returns a LayoutResult containing the node ID and size.
    /// The render object should store the node ID for later use in apply_layout().
    fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult;

    /// Apply computed layout from Taffy.
    ///
    /// Called after Taffy::compute() to read back computed bounds.
    /// The render object should read its layout from the engine and update computed_bounds.
    fn apply_layout(&mut self, ctx: &mut LayoutContext);

    // ... rest of trait unchanged (paint, hit_test, children, as_any, as_any_mut, set_child_id)
}
```

- [ ] **Step 4: Run build to check for errors**

Run: `cargo build -p vexo 2>&1 | head -50`
Expected: Compilation errors in files that implement RenderObject (to be fixed in subsequent tasks)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/render_object.rs
git commit -m "feat: add LayoutResult and update LayoutContext with engine access"
```

---

### Task 2: Update TextRenderObject to use Taffy

**Files:**
- Modify: `vexo/src/retain/render_objects/text.rs`

- [ ] **Step 1: Update imports**

```rust
// Update imports at top of file:

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{Layout, LayoutNodeId, MeasureContext, TextMeasureContext};
use crate::render::RenderCommand;
use crate::retain::{HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject};
```

- [ ] **Step 2: Add layout_node field to TextRenderObject**

```rust
// Update struct:

pub struct TextRenderObject {
    content: String,
    font_size: f32,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeId>,
}
```

- [ ] **Step 3: Update constructor**

```rust
// Update new():

pub fn new(content: &str) -> Self {
    Self {
        content: content.to_string(),
        font_size: 16.0,
        computed_bounds: None,
        layout_node: None,
    }
}
```

- [ ] **Step 4: Implement layout() with text measure context**

```rust
// Replace existing layout() implementation:

fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
    // Create measure context for text
    let measure_ctx = MeasureContext::Text(TextMeasureContext {
        content: self.content.clone(),
        font_size: self.font_size,
        line_height: 1.2,
    });

    // Create leaf node with text measurement
    let node = ctx.engine().create_leaf_with_context(
        &Layout::default(),
        measure_ctx,
    );

    // Store node for apply_layout
    self.layout_node = Some(node);

    LayoutResult {
        node,
        size: Size::new(0.0, 0.0), // Will be filled by apply_layout
    }
}
```

- [ ] **Step 5: Implement apply_layout()**

```rust
// Add after layout():

fn apply_layout(&mut self, ctx: &mut LayoutContext) {
    if let Some(node) = self.layout_node {
        if let Some(computed) = ctx.engine().get_layout(node) {
            self.computed_bounds = Some(computed.bounds);
        }
    }
}
```

- [ ] **Step 6: Run build to verify**

Run: `cargo build -p vexo 2>&1 | head -30`
Expected: Fewer errors (TextRenderObject now compiles)

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/render_objects/text.rs
git commit -m "feat: TextRenderObject uses Taffy for layout with text measurement"
```

---

### Task 3: Update BackgroundRenderObject for new trait

**Files:**
- Modify: `vexo/src/retain/widgets/background.rs`

- [ ] **Step 1: Update imports**

```rust
// Add to imports:

use crate::layout::LayoutNodeId;
use crate::retain::{LayoutResult, RenderObject};
```

- [ ] **Step 2: Add layout_node field**

```rust
// Update BackgroundRenderObject struct:

pub struct BackgroundRenderObject {
    color: Color,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeId>,
}
```

- [ ] **Step 3: Update constructor**

```rust
// Update new():

pub fn new(color: Color) -> Self {
    Self {
        color,
        child: None,
        computed_bounds: None,
        layout_node: None,
    }
}
```

- [ ] **Step 4: Update layout() to pass through to child**

```rust
// Replace existing layout() implementation:

fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
    // Background is a pass-through modifier - it uses the child's layout
    // The child will be laid out by the pipeline's recursive traversal
    // We just need to create a placeholder node for ourselves
    
    let node = ctx.engine().create_leaf(&Layout::default());
    self.layout_node = Some(node);
    
    LayoutResult {
        node,
        size: Size::new(0.0, 0.0),
    }
}
```

- [ ] **Step 5: Implement apply_layout()**

```rust
// Add after layout():

fn apply_layout(&mut self, ctx: &mut LayoutContext) {
    // Background uses child's bounds
    // Child's apply_layout is called by pipeline traversal
    // We'll get bounds from our layout_node after Taffy computes
    if let Some(node) = self.layout_node {
        if let Some(computed) = ctx.engine().get_layout(node) {
            self.computed_bounds = Some(computed.bounds);
        }
    }
}
```

- [ ] **Step 6: Run build to verify**

Run: `cargo build -p vexo 2>&1 | head -30`
Expected: Fewer errors

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/widgets/background.rs
git commit -m "feat: BackgroundRenderObject updated for Taffy layout"
```

---

### Task 4: Update BorderRenderObject for new trait

**Files:**
- Modify: `vexo/src/retain/widgets/border.rs`

- [ ] **Step 1: Update imports**

```rust
// Add to imports:

use crate::layout::LayoutNodeId;
use crate::retain::{LayoutResult, RenderObject};
```

- [ ] **Step 2: Add layout_node field and update methods**

Follow the same pattern as BackgroundRenderObject in Task 3:
- Add `layout_node: Option<LayoutNodeId>` field
- Update `new()` to initialize it
- Update `layout()` to create placeholder node
- Add `apply_layout()` implementation

- [ ] **Step 2: Run build to verify**

Run: `cargo build -p vexo 2>&1 | head -30`

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/border.rs
git commit -m "feat: BorderRenderObject updated for Taffy layout"
```

---

### Task 5: Update CornerRadiusRenderObject for new trait

**Files:**
- Modify: `vexo/src/retain/widgets/corner_radius.rs`

- [ ] **Step 1: Update following same pattern as Background**

- Add `layout_node: Option<LayoutNodeId>` field
- Update `new()` to initialize it
- Update `layout()` to create placeholder node
- Add `apply_layout()` implementation

- [ ] **Step 2: Run build to verify**

Run: `cargo build -p vexo 2>&1 | head -30`

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/corner_radius.rs
git commit -m "feat: CornerRadiusRenderObject updated for Taffy layout"
```

---

### Task 6: Update pipeline to use Taffy engine

**Files:**
- Modify: `vexo/src/retain/pipeline.rs`

- [ ] **Step 1: Update layout() method**

```rust
// Replace the existing layout() method:

/// Perform layout using Taffy layout engine.
///
/// Three-phase layout:
/// 1. Build Taffy tree (each RenderObject creates nodes)
/// 2. Compute layout with Taffy
/// 3. Apply computed layouts back to RenderObjects
pub fn layout(
    &mut self,
    available_size: Size<Logical>,
    engine: &mut dyn crate::layout::LayoutEngine,
    font_system: &mut glyphon::FontSystem,
) {
    // Get the root render object
    let root_id = match self.render_objects.root() {
        Some(id) => id,
        None => return,
    };

    // Phase 1: Build Taffy tree (creates nodes)
    let mut ctx = LayoutContext::new(engine, font_system);
    let _result = self.layout_build_recursive(root_id, &mut ctx);

    // Phase 2: Compute layout with Taffy
    // The root node is stored in the root render object
    if let Some(root_node) = self.get_layout_node(root_id) {
        engine.compute(root_node, available_size, font_system);
    }

    // Phase 3: Apply computed layouts back to render objects
    self.apply_layout_recursive(root_id, &mut ctx);

    // Clear dirty flags
    self.dirty.drain_layout().for_each(drop);
}
```

- [ ] **Step 2: Add helper methods**

```rust
// Add after layout():

/// Recursively build Taffy tree by calling layout() on each RenderObject.
fn layout_build_recursive(
    &mut self,
    id: RenderObjectId,
    ctx: &mut LayoutContext,
) -> LayoutResult {
    // Layout children first (bottom-up for node creation)
    let children: Vec<RenderObjectId> = self.render_objects.get(id)
        .map(|obj| obj.children().to_vec())
        .unwrap_or_default();

    // Layout children recursively
    for child_id in children {
        self.layout_build_recursive(child_id, ctx);
    }

    // Now layout this object
    if let Some(obj) = self.render_objects.get_mut(id) {
        obj.layout(ctx)
    } else {
        // Fallback: create empty node
        let node = ctx.engine().create_leaf(&Layout::default());
        LayoutResult { node, size: Size::new(0.0, 0.0) }
    }
}

/// Get the layout node ID from a render object.
fn get_layout_node(&self, id: RenderObjectId) -> Option<LayoutNodeId> {
    // This requires RenderObject to expose its layout_node
    // For now, we'll need to add a method to the trait
    None // TODO: Implement after adding method to trait
}

/// Recursively apply computed layouts.
fn apply_layout_recursive(&mut self, id: RenderObjectId, ctx: &mut LayoutContext) {
    // Apply to this object
    if let Some(obj) = self.render_objects.get_mut(id) {
        obj.apply_layout(ctx);
    }

    // Recursively apply to children
    let children: Vec<RenderObjectId> = self.render_objects.get(id)
        .map(|obj| obj.children().to_vec())
        .unwrap_or_default();

    for child_id in children {
        self.apply_layout_recursive(child_id, ctx);
    }
}
```

- [ ] **Step 3: Add layout_node() method to RenderObject trait**

```rust
// In render_object.rs, add to RenderObject trait:

/// Get the layout node ID (for pipeline to use).
fn layout_node(&self) -> Option<LayoutNodeId> {
    None
}
```

- [ ] **Step 4: Implement layout_node() in each RenderObject**

Add to TextRenderObject, BackgroundRenderObject, BorderRenderObject, CornerRadiusRenderObject:

```rust
fn layout_node(&self) -> Option<LayoutNodeId> {
    self.layout_node
}
```

- [ ] **Step 5: Update get_layout_node() in pipeline**

```rust
// Update the helper:

fn get_layout_node(&self, id: RenderObjectId) -> Option<LayoutNodeId> {
    self.render_objects.get(id).and_then(|obj| obj.layout_node())
}
```

- [ ] **Step 6: Run build to verify**

Run: `cargo build -p vexo 2>&1 | head -50`

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/pipeline.rs vexo/src/retain/render_object.rs
git commit -m "feat: pipeline uses Taffy engine for retain-mode layout"
```

---

### Task 7: Update window.rs to pass font_system to pipeline

**Files:**
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Update render_retain() to pass font_system**

Find the call to `pipeline.layout()` and update it to pass `font_system`:

```rust
// In render_retain(), update the layout call:

// 8. Layout dirty render objects
pipeline.layout(logical_size, self.layout_engine.as_mut(), &mut self.widget_context.font_system);
```

- [ ] **Step 2: Run build to verify**

Run: `cargo build -p vexo 2>&1 | head -30`

- [ ] **Step 3: Commit**

```bash
git add vexo/src/window.rs
git commit -m "feat: pass font_system to retain pipeline layout"
```

---

### Task 8: Run tests and verify

- [ ] **Step 1: Run all tests**

Run: `cargo test -p vexo -- --nocapture 2>&1 | tail -50`
Expected: All tests pass

- [ ] **Step 2: Run desktop demo**

Run: `cargo run -p desktop_demo`
Expected: App starts, press 'R' shows retain mode with properly sized widgets

- [ ] **Step 3: Manual verification**

1. Immediate mode still works (default view)
2. Press 'R' - retain mode shows blue background sized to text
3. Border is visible around the text
4. Text is rendered correctly

- [ ] **Step 4: Final commit if needed**

```bash
git add -A
git commit -m "fix: any remaining issues from Taffy integration"
```

---

## Summary

This plan integrates Taffy layout engine into retain-mode:

1. **Task 1**: Add LayoutResult type and update LayoutContext with engine access
2. **Task 2**: Update TextRenderObject to use Taffy with text measurement
3. **Task 3**: Update BackgroundRenderObject for new trait
4. **Task 4**: Update BorderRenderObject for new trait
5. **Task 5**: Update CornerRadiusRenderObject for new trait
6. **Task 6**: Update pipeline to use Taffy engine (three-phase layout)
7. **Task 7**: Update window.rs to pass font_system
8. **Task 8**: Test and verify

After completion:
- Text widgets get accurate intrinsic size from font measurement
- Modifier widgets (Background, Border) size to their content
- Layout is computed properly with Taffy's two-pass algorithm
