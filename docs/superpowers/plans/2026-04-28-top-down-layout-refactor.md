# Top-Down Layout Refactor (Flutter-Style)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor retain-mode layout from bottom-up traversal to top-down (Flutter-style) where parent RenderObjects call `ctx.layout_child()` directly, enabling constraint propagation.

**Architecture:** Parent RenderObjects call `ctx.layout_child(child_id)` which recursively lays out children. Constraints flow down from parent to child, sizes flow up from child to parent. The pipeline only initiates layout on the root.

**Tech Stack:** Rust, Taffy layout engine, existing retain-mode infrastructure

---

## Current vs Target Architecture

**Current (Bottom-Up Traversal):**
```
Pipeline.layout_build_recursive(root)
  → Gets children from registry
  → Recursively calls layout_build_recursive(child) for each child
  → Child returns LayoutResult with node ID
  → Parent.layout(ctx, child_node_ids) receives already-built child nodes
```

**Target (Top-Down with Constraints):**
```
Pipeline calls root.layout(ctx)
  → root.layout() calls ctx.layout_child(child_id) for each child
  → ctx.layout_child() recursively calls child.layout(ctx)
  → Parent controls WHEN and WITH WHAT CONSTRAINTS children are laid out
```

---

## Files to Modify

| File | Purpose |
|------|---------|
| `vexo/src/retain/render_object.rs` | LayoutContext with `layout_child()`, RenderObject trait signature |
| `vexo/src/retain/pipeline.rs` | Simplify to just call root.layout() |
| `vexo/src/retain/render_objects/text.rs` | Update signature, no child layout |
| `vexo/src/retain/render_objects/container.rs` | Call `layout_child()` for each child |
| `vexo/src/retain/widgets/background.rs` | Call `layout_child()` for single child |
| `vexo/src/retain/widgets/border.rs` | Call `layout_child()` for single child |
| `vexo/src/retain/widgets/corner_radius.rs` | Call `layout_child()` for single child |

---

## Task 1: Add RenderObjectRegistry Reference to LayoutContext

**Files:**
- Modify: `vexo/src/retain/render_object.rs:47-72`

**Goal:** LayoutContext needs access to the RenderObjectRegistry so it can call `layout_child()` on children.

- [ ] **Step 1: Update LayoutContext struct to hold registry reference**

```rust
use super::id::RenderObjectId;
use super::render_object::RenderObjectRegistry;

/// Context passed to RenderObject.layout().
///
/// Provides access to the layout engine, font system, and render object registry
/// for child layout operations.
pub struct LayoutContext<'a> {
    engine: &'a mut dyn LayoutEngine,
    font_system: &'a mut glyphon::FontSystem,
    render_objects: Option<&'a mut RenderObjectRegistry>,
}

impl<'a> LayoutContext<'a> {
    /// Create a new layout context without registry access.
    pub fn new(engine: &'a mut dyn LayoutEngine, font_system: &'a mut glyphon::FontSystem) -> Self {
        Self {
            engine,
            font_system,
            render_objects: None,
        }
    }

    /// Create a layout context with registry access for child layout.
    pub fn new_with_registry(
        engine: &'a mut dyn LayoutEngine,
        font_system: &'a mut glyphon::FontSystem,
        render_objects: &'a mut RenderObjectRegistry,
    ) -> Self {
        Self {
            engine,
            font_system,
            render_objects: Some(render_objects),
        }
    }

    /// Get the layout engine (mutable for creating nodes).
    pub fn engine(&mut self) -> &mut dyn LayoutEngine {
        self.engine
    }

    /// Get the layout engine (immutable for reading computed layouts).
    pub fn engine_ref(&self) -> &dyn LayoutEngine {
        self.engine
    }

    /// Get the font system.
    pub fn font_system(&mut self) -> &mut glyphon::FontSystem {
        self.font_system
    }
}
```

- [ ] **Step 2: Run tests to verify no breakage**

Run: `cargo test -p vexo --lib render_object`
Expected: All tests pass (no functional changes yet)

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/render_object.rs
git commit -m "refactor: add render_objects field to LayoutContext for child layout"
```

---

## Task 2: Add `layout_child()` Method to LayoutContext

**Files:**
- Modify: `vexo/src/retain/render_object.rs` (add methods to LayoutContext impl)

**Goal:** Add methods for parent RenderObjects to lay out their children.

- [ ] **Step 1: Add `layout_child()` and `layout_children()` methods to LayoutContext**

Add these methods to the `impl<'a> LayoutContext<'a>` block:

```rust
    /// Layout a child render object.
    ///
    /// This is the core of top-down layout: the parent calls this method
    /// to lay out each child. The child's layout() method is called,
    /// which may recursively call layout_child() on its own children.
    ///
    /// Returns the LayoutResult containing the child's layout node.
    /// Returns None if the child doesn't exist or no registry is available.
    pub fn layout_child(&mut self, child_id: RenderObjectId) -> Option<LayoutResult> {
        let registry = self.render_objects.as_mut()?;
        let child = registry.get_mut(child_id)?;
        Some(child.layout(self))
    }

    /// Layout multiple children and return their layout node IDs.
    ///
    /// Convenience method for containers with multiple children.
    pub fn layout_children(&mut self, children: &[RenderObjectId]) -> Vec<LayoutNodeId> {
        children
            .iter()
            .filter_map(|child_id| {
                self.layout_child(*child_id).map(|result| result.node)
            })
            .collect()
    }
}
```

- [ ] **Step 2: Run tests to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/render_object.rs
git commit -m "feat: add layout_child() and layout_children() to LayoutContext"
```

---

## Task 3: Update RenderObject Trait Signature

**Files:**
- Modify: `vexo/src/retain/render_object.rs:156-220` (RenderObject trait)

**Goal:** Remove `child_nodes` parameter from `layout()` - parents now get child nodes by calling `ctx.layout_child()` themselves.

- [ ] **Step 1: Update the RenderObject::layout() signature**

Change the trait method signature:

```rust
    /// Perform layout with the layout engine, creating Taffy node(s).
    ///
    /// This method creates the Taffy node for this render object.
    /// For containers and modifiers, call `ctx.layout_child()` to lay out children
    /// and get their node IDs.
    ///
    /// Returns a LayoutResult containing the node ID and size.
    /// The render object should store the node ID for later use in apply_layout().
    fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult;
```

- [ ] **Step 2: Update the trait documentation**

Update the trait docs at lines 132-155 to reflect the new approach:

```rust
/// Persistent render object for layout and painting.
///
/// RenderObjects form the third tree in the three-tree architecture.
/// They persist across frames and are only updated when marked dirty.
///
/// # Layout (Top-Down)
///
/// The `layout` method is called by the parent (or pipeline for root).
/// For containers and modifiers, call `ctx.layout_child()` to recursively
/// lay out children and get their node IDs:
///
/// ```ignore
/// fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
///     // Layout children first (top-down)
///     let child_nodes: Vec<LayoutNodeId> = ctx.layout_children(self.children());
///
///     // Create container node with child nodes
///     let node = ctx.engine().create_container(&self.layout, &child_nodes);
///     LayoutResult { node, size: Size::new(0.0, 0.0) }
/// }
/// ```
///
/// The `apply_layout` method is called after Taffy::compute() to read back
/// computed bounds from the engine.
```

- [ ] **Step 3: Run tests to see compilation errors**

Run: `cargo build -p vexo 2>&1 | head -50`
Expected: Compilation errors in all RenderObject implementations (expected - we'll fix them next)

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/render_object.rs
git commit -m "refactor: remove child_nodes parameter from RenderObject::layout()"
```

---

## Task 4: Update TextRenderObject

**Files:**
- Modify: `vexo/src/retain/render_objects/text.rs`

**Goal:** Update TextRenderObject to use the new signature (simplest case - no children).

- [ ] **Step 1: Update the layout() method signature**

Find the `layout()` method and update it:

```rust
    fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
        // Create measure context for text
        let measure_ctx = MeasureContext::Text(TextMeasureContext {
            content: self.content.clone(),
            font_size: self.font_size,
            line_height: 1.2,
        });

        // Create leaf node with text measurement
        let node = ctx.engine().create_leaf_with_context(&Layout::default(), measure_ctx);
        self.layout_node = Some(node);

        LayoutResult {
            node,
            size: Size::new(0.0, 0.0), // Will be filled by apply_layout
        }
    }
```

- [ ] **Step 2: Run tests for text render object**

Run: `cargo test -p vexo --lib render_objects::text`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/render_objects/text.rs
git commit -m "refactor: update TextRenderObject::layout() to new signature"
```

---

## Task 5: Update ContainerRenderObject

**Files:**
- Modify: `vexo/src/retain/render_objects/container.rs:80-97`

**Goal:** ContainerRenderObject now calls `ctx.layout_children()` to lay out its children.

- [ ] **Step 1: Update the layout() method**

```rust
    fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
        // Layout all children (top-down)
        let child_nodes: Vec<LayoutNodeId> = ctx.layout_children(&self.children);

        // Create container layout with flex direction
        let layout = if self.is_row {
            Layout::default().flex_direction(FlexDirection::Row)
        } else {
            Layout::default().flex_direction(FlexDirection::Column)
        };

        // Create container node with children
        let node = ctx.engine().create_container(&layout, &child_nodes);
        self.layout_node = Some(node);

        LayoutResult {
            node,
            size: Size::new(0.0, 0.0), // Will be filled by apply_layout
        }
    }
```

- [ ] **Step 2: Run tests for container render object**

Run: `cargo test -p vexo --lib render_objects::container`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/render_objects/container.rs
git commit -m "refactor: ContainerRenderObject calls ctx.layout_children() for top-down layout"
```

---

## Task 6: Update BackgroundRenderObject

**Files:**
- Modify: `vexo/src/retain/widgets/background.rs:111-133`

**Goal:** BackgroundRenderObject calls `ctx.layout_child()` for its single child.

- [ ] **Step 1: Update the layout() method**

```rust
    fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
        // Layout child if present (top-down)
        match &self.child {
            Some(child_id) => {
                let result = ctx.layout_child(*child_id)
                    .expect("Child render object should exist");
                self.layout_node = Some(result.node);
                result
            }
            None => {
                // No child, create empty leaf
                let node = ctx.engine().create_leaf(&Layout::default());
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::new(0.0, 0.0),
                }
            }
        }
    }
```

- [ ] **Step 2: Run tests for background render object**

Run: `cargo test -p vexo --lib widgets::background`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/background.rs
git commit -m "refactor: BackgroundRenderObject calls ctx.layout_child() for top-down layout"
```

---

## Task 7: Update BorderRenderObject

**Files:**
- Modify: `vexo/src/retain/widgets/border.rs:121-143`

**Goal:** BorderRenderObject calls `ctx.layout_child()` for its single child.

- [ ] **Step 1: Update the layout() method**

```rust
    fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
        // Layout child if present (top-down)
        match &self.child {
            Some(child_id) => {
                let result = ctx.layout_child(*child_id)
                    .expect("Child render object should exist");
                self.layout_node = Some(result.node);
                result
            }
            None => {
                // No child, create empty leaf
                let node = ctx.engine().create_leaf(&Layout::default());
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::new(0.0, 0.0),
                }
            }
        }
    }
```

- [ ] **Step 2: Run tests for border render object**

Run: `cargo test -p vexo --lib widgets::border`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/border.rs
git commit -m "refactor: BorderRenderObject calls ctx.layout_child() for top-down layout"
```

---

## Task 8: Update CornerRadiusRenderObject

**Files:**
- Modify: `vexo/src/retain/widgets/corner_radius.rs:106-128`

**Goal:** CornerRadiusRenderObject calls `ctx.layout_child()` for its single child.

- [ ] **Step 1: Update the layout() method**

```rust
    fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
        // Layout child if present (top-down)
        match &self.child {
            Some(child_id) => {
                let result = ctx.layout_child(*child_id)
                    .expect("Child render object should exist");
                self.layout_node = Some(result.node);
                result
            }
            None => {
                // No child, create empty leaf
                let node = ctx.engine().create_leaf(&Layout::default());
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::new(0.0, 0.0),
                }
            }
        }
    }
```

- [ ] **Step 2: Run tests for corner radius render object**

Run: `cargo test -p vexo --lib widgets::corner_radius`
Expected: Tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/corner_radius.rs
git commit -m "refactor: CornerRadiusRenderObject calls ctx.layout_child() for top-down layout"
```

---

## Task 9: Simplify Pipeline Layout Method

**Files:**
- Modify: `vexo/src/retain/pipeline.rs:301-367`

**Goal:** Remove `layout_build_recursive()` - pipeline now just calls `root.layout()` with registry access.

- [ ] **Step 1: Update the layout() method**

Replace the existing `layout()` method:

```rust
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

        // Phase 1: Build Taffy tree (top-down from root)
        // Parent RenderObjects call ctx.layout_child() to lay out children
        {
            let mut ctx = LayoutContext::new_with_registry(
                engine,
                font_system,
                &mut self.render_objects,
            );

            if let Some(root) = ctx.render_objects.as_mut().unwrap().get_mut(root_id) {
                root.layout(&mut ctx);
            }
        }

        // Phase 2: Compute layout with Taffy
        if let Some(root_node) = self.get_layout_node(root_id) {
            engine.compute(root_node, available_size, font_system);
        }

        // Phase 3: Apply computed layouts back to render objects
        {
            let ctx = LayoutContext::new(engine, font_system);
            self.apply_layout_recursive(root_id, &ctx);
        }

        // Clear dirty flags
        self.dirty.drain_layout().for_each(drop);
    }
```

- [ ] **Step 2: Remove the `layout_build_recursive()` method**

Delete the entire `layout_build_recursive()` method (lines ~343-367).

- [ ] **Step 3: Run tests to verify pipeline works**

Run: `cargo test -p vexo --lib pipeline`
Expected: Tests pass

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/pipeline.rs
git commit -m "refactor: simplify pipeline to call root.layout() for top-down layout"
```

---

## Task 10: Update All Tests

**Files:**
- Modify: `vexo/src/retain/render_object.rs` (tests)
- Modify: `vexo/src/retain/render_objects/text.rs` (tests)
- Modify: `vexo/src/retain/render_objects/container.rs` (tests)
- Modify: `vexo/src/retain/widgets/background.rs` (tests)
- Modify: `vexo/src/retain/widgets/border.rs` (tests)
- Modify: `vexo/src/retain/widgets/corner_radius.rs` (tests)
- Modify: `vexo/src/retain/hit_test.rs` (tests)

**Goal:** Update all test calls from `.layout(&mut ctx, &[])` to `.layout(&mut ctx)`.

- [ ] **Step 1: Find all test calls that need updating**

Run: `grep -rn "\.layout(&mut ctx," vexo/src/retain/`
Expected: List of files with old signature calls

- [ ] **Step 2: Update each file's test calls**

For each file found, replace:
- `.layout(&mut ctx, &[])` → `.layout(&mut ctx)`
- `.layout(&mut ctx, &[node])` → `.layout(&mut ctx)` (child nodes now obtained via ctx.layout_child())

- [ ] **Step 3: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "test: update all tests to use new layout() signature"
```

---

## Task 11: Verify End-to-End

**Files:**
- Run: `cargo run -p desktop_demo`

**Goal:** Verify the border is still visible and layout works correctly.

- [ ] **Step 1: Build and run desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Window opens, press 'R' for retain mode, border visible around text

- [ ] **Step 2: Verify no errors in console**

Expected: No panic or error messages

- [ ] **Step 3: Final commit if needed**

```bash
git status
# If any uncommitted changes:
git add -A
git commit -m "fix: final adjustments for top-down layout"
```

---

## Summary

| Component | Before | After |
|-----------|--------|-------|
| Pipeline | `layout_build_recursive()` traverses children first | Calls `root.layout()` only |
| Parent RenderObject | Receives `child_nodes` parameter | Calls `ctx.layout_child()` directly |
| LayoutContext | No registry access | Has `render_objects` reference for child layout |
| Constraint Flow | None (children already built) | Parent controls child layout timing |
