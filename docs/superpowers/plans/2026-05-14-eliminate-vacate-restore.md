# Eliminate Vacate/Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace all vacate/restore and take/restore patterns with index-based access (with_element + ChildOps command pattern), eliminating 11 error-prone instances.

**Architecture:** Elements emit child operations (inflate/update/unmount) through a ChildOps accumulator instead of executing them directly against the registry. The pipeline drains ChildOps after each element method call and executes them with full &mut self access. ElementContext loses its element_registry field and Option wrappers. ElementRegistry gains a with_element borrow-safe accessor.

**Tech Stack:** Rust, slotmap crate, existing Vexo retain mode infrastructure

---

## File Structure

| File | Responsibility |
|------|---------------|
| `vexo/src/retain/child_ops.rs` | **New.** ChildOps accumulator and ChildOp enum |
| `vexo/src/retain/element.rs` | ElementRegistry — remove vacate/restore, add with_element/insert/add_child, remove lifecycle methods |
| `vexo/src/retain/element_context.rs` | ElementContext — remove Option wrappers, remove element_registry, add child_ops |
| `vexo/src/retain/pipeline.rs` | ThreeTreePipeline — add child_ops field, add execute_child_ops, replace vacate/restore with with_element |
| `vexo/src/retain/elements/container.rs` | ContainerElement — replace context.inflate_widget/update_child/unmount_child with child_ops |
| `vexo/src/retain/elements/single_child.rs` | SingleChildRenderObjectElement — same as container |
| `vexo/src/retain/elements/multi_child.rs` | MultiChildRenderObjectElement — same as container |
| `vexo/src/retain/elements/render_object_element.rs` | RenderObjectElement trait — add child_mounted default impl |
| `vexo/src/retain/elements/leaf.rs` | LeafElement — no change needed |
| `vexo/src/retain/stateful_widget.rs` | StatefulElement — remove take/restore, use child_ops |
| `vexo/src/retain/mod.rs` | Add `mod child_ops` |

---

### Task 1: Create ChildOps module

**Files:**
- Create: `vexo/src/retain/child_ops.rs`
- Modify: `vexo/src/retain/mod.rs`

- [ ] **Step 1: Create child_ops.rs with ChildOp enum and ChildOps struct**

```rust
use crate::retain::id::ElementKey;
use crate::retain::widgets::Widget;

/// A command emitted by an element to request a child tree operation.
/// The pipeline executes these after the element method returns.
pub enum ChildOp {
    /// Mount a new child element at the given slot
    Inflate {
        slot: Option<usize>,
        widget: Box<dyn Widget>,
        parent: ElementKey,
    },
    /// Update an existing child element with a new widget
    Update {
        child: ElementKey,
        widget: Box<dyn Widget>,
    },
    /// Unmount a child element
    Unmount {
        child: ElementKey,
    },
}

/// Accumulator for child operations emitted during element lifecycle methods.
/// Elements push ops here instead of directly accessing the ElementRegistry.
pub struct ChildOps {
    ops: Vec<ChildOp>,
}

impl ChildOps {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    /// Request inflation of a new child element.
    pub fn inflate(&mut self, slot: Option<usize>, widget: Box<dyn Widget>, parent: ElementKey) {
        self.ops.push(ChildOp::Inflate { slot, widget, parent });
    }

    /// Request update of an existing child element.
    pub fn update(&mut self, child: ElementKey, widget: Box<dyn Widget>) {
        self.ops.push(ChildOp::Update { child, widget });
    }

    /// Request unmount of a child element.
    pub fn unmount(&mut self, child: ElementKey) {
        self.ops.push(ChildOp::Unmount { child });
    }

    /// Drain all pending operations, leaving the accumulator empty.
    pub fn drain(&mut self) -> Vec<ChildOp> {
        std::mem::take(&mut self.ops)
    }

    /// Check if there are any pending operations.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

impl Default for ChildOps {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Add module declaration to mod.rs**

In `vexo/src/retain/mod.rs`, add:

```rust
pub mod child_ops;
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles successfully (child_ops is not yet used, just declared)

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/child_ops.rs vexo/src/retain/mod.rs
git commit -m "feat: add ChildOps module for command-pattern child operations"
```

---

### Task 2: Redesign ElementContext

**Files:**
- Modify: `vexo/src/retain/element_context.rs`

- [ ] **Step 1: Rewrite ElementContext to remove Option wrappers and element_registry, add child_ops**

Replace the entire `ElementContext` struct and its impl block. The new struct has no `Option` wrappers and no `element_registry`. All fields are always present. The `child_ops` field replaces the direct registry access.

The new `ElementContext`:

```rust
use std::sync::mpsc;

use crate::retain::build_owner::BuildOwner;
use crate::retain::child_ops::ChildOps;
use crate::retain::dirty::DirtyTracking;
use crate::retain::id::{ElementKey, RenderObjectKey};
use crate::retain::render_object::RenderObjectRegistry;
use crate::retain::state::StateStorage;

/// Context passed to element lifecycle methods.
///
/// Elements use `child_ops` to request child tree operations instead of
/// directly accessing the ElementRegistry. The pipeline executes the
/// operations after the element method returns.
pub struct ElementContext<'a> {
    pub element_id: ElementKey,
    pub parent: Option<ElementKey>,
    pub render_object: Option<RenderObjectKey>,
    pub state: &'a mut StateStorage,
    pub dirty: &'a mut DirtyTracking,
    pub render_objects: &'a mut RenderObjectRegistry,
    pub build_owner: &'a BuildOwner,
    pub dirty_sender: &'a mpsc::Sender<ElementKey>,
    pub child_ops: &'a mut ChildOps,
}

impl<'a> ElementContext<'a> {
    pub fn new(
        element_id: ElementKey,
        parent: Option<ElementKey>,
        render_object: Option<RenderObjectKey>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
        render_objects: &'a mut RenderObjectRegistry,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a mpsc::Sender<ElementKey>,
        child_ops: &'a mut ChildOps,
    ) -> Self {
        Self {
            element_id,
            parent,
            render_object,
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
        }
    }

    /// Request inflation of a new child element.
    pub fn inflate_child(&mut self, slot: Option<usize>, widget: Box<dyn crate::retain::widgets::Widget>) {
        self.child_ops.inflate(slot, widget, self.element_id);
    }

    /// Request update of an existing child element.
    pub fn update_child(&mut self, child: ElementKey, widget: Box<dyn crate::retain::widgets::Widget>) {
        self.child_ops.update(child, widget);
    }

    /// Request unmount of a child element.
    pub fn unmount_child(&mut self, child: ElementKey) {
        self.child_ops.unmount(child);
    }

    /// Mark this element as needing rebuild.
    pub fn mark_dirty(&mut self) {
        let _ = self.dirty_sender.send(self.element_id);
    }

    /// Get or create state for this element.
    pub fn get_or_create_state<S: 'static + Clone + Send>(&mut self, initial: S) -> S {
        self.state.get_or_create(self.element_id, initial)
    }

    /// Remove render object associated with this element.
    pub fn remove_render_object(&mut self, render_object_id: RenderObjectKey) {
        self.render_objects.remove(render_object_id);
    }
}
```

Note: Remove all `take_*` and `restore_*` methods. Remove the old `inflate_widget`, `update_child`, `unmount_child` methods that directly accessed the registry. The new convenience methods (`inflate_child`, `update_child`, `unmount_child`) delegate to `child_ops`.

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: Compilation errors in files that use the old ElementContext API (pipeline.rs, container.rs, etc.). This is expected — we'll fix them in subsequent tasks.

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/element_context.rs
git commit -m "refactor: redesign ElementContext without Option wrappers or element_registry"
```

---

### Task 3: Add with_element, insert, add_child to ElementRegistry; remove vacate/restore and lifecycle methods

**Files:**
- Modify: `vexo/src/retain/element.rs`

- [ ] **Step 1: Add with_element method to ElementRegistry**

Add this method to the `impl ElementRegistry` block:

```rust
/// Call a closure with mutable access to an element and an external context.
///
/// This replaces the vacate/restore pattern. The element is accessed via
/// SlotMap::get_mut(), and the context is a separate parameter — Rust can
/// prove they're disjoint because they're different arguments.
///
/// Returns None if the key is invalid or the slot is empty.
pub fn with_element<C, R>(
    &mut self,
    key: ElementKey,
    context: &mut C,
    f: impl FnOnce(&mut Box<dyn Element>, &mut C) -> R,
) -> Option<R> {
    let element = self.slots.get_mut(key)?.as_mut()?;
    Some(f(element, context))
}
```

- [ ] **Step 2: Add insert method (insert without lifecycle call)**

Add this method to `impl ElementRegistry`:

```rust
/// Insert an element into the registry and set up parent metadata.
/// Does NOT call element.mount() — the pipeline handles lifecycle.
pub fn insert(
    &mut self,
    element: Box<dyn Element>,
    parent: Option<ElementKey>,
) -> ElementKey {
    let key = self.slots.insert(Some(element));
    self.parent_map.insert(key, parent);
    if let Some(p) = parent {
        self.children_map.entry(p).or_insert_with(Vec::new).push(key);
    } else {
        self.root = Some(key);
    }
    key
}
```

- [ ] **Step 3: Add add_child method**

Add this method to `impl ElementRegistry`:

```rust
/// Add a child to a parent's children list at the given slot position.
/// Called by the pipeline after executing a ChildOp::Inflate.
pub fn add_child(&mut self, parent: ElementKey, child: ElementKey, slot: Option<usize>) {
    let children = self.children_map.entry(parent).or_insert_with(Vec::new);
    if let Some(idx) = slot {
        if idx >= children.len() {
            children.resize(idx + 1, child);
        } else {
            children[idx] = child;
        }
    } else {
        children.push(child);
    }
}
```

- [ ] **Step 4: Remove vacate, restore, VacatedElement, mount_element, update_child, inflate_widget**

Delete the following from `impl ElementRegistry`:
- `vacate()` method
- `restore()` method
- `VacatedElement` type (if it's a separate struct)
- `mount_element()` method
- `update_child()` method
- `inflate_widget()` method (if it exists as a registry method)

Keep: `get()`, `get_mut()`, `contains()`, `parent()`, `children()`, `depth()`, `root()`, `set_root()`, `unmount()`, `remove()`.

- [ ] **Step 5: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: Compilation errors in pipeline.rs and element files that call the removed methods. This is expected.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/element.rs
git commit -m "refactor: add with_element/insert/add_child, remove vacate/restore from ElementRegistry"
```

---

### Task 4: Add child_mounted to Element trait

**Files:**
- Modify: `vexo/src/retain/element.rs` (the Element trait definition)

- [ ] **Step 1: Add child_mounted default method to Element trait**

Add to the `Element` trait:

```rust
/// Called by the pipeline after a ChildOp::Inflate is executed,
/// notifying the parent of the new child's key.
/// Elements that track children internally should override this.
fn child_mounted(&mut self, _child: ElementKey, _slot: Option<usize>) {
    // Default: no-op
}
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles (default impl means existing types don't need to change yet)

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/element.rs
git commit -m "feat: add child_mounted callback to Element trait"
```

---

### Task 5: Rewrite ThreeTreePipeline

**Files:**
- Modify: `vexo/src/retain/pipeline.rs`

This is the largest change. The pipeline gains a `child_ops` field, an `execute_child_ops` method, and all vacate/restore calls are replaced with `with_element`.

- [ ] **Step 1: Add child_ops field to ThreeTreePipeline**

Add `child_ops: ChildOps` field to the `ThreeTreePipeline` struct. Update `new()` to initialize it with `ChildOps::new()`.

- [ ] **Step 2: Add execute_child_ops method**

Add this method to `impl ThreeTreePipeline`:

```rust
/// Execute all pending child operations emitted by element lifecycle methods.
fn execute_child_ops(&mut self) {
    let ops = self.child_ops.drain();
    for op in ops {
        match op {
            ChildOp::Inflate { slot, widget, parent } => {
                let child_key = self.mount_element_tree(Some(parent), widget);
                self.element_registry.add_child(parent, child_key, slot);
                // Notify parent element of new child key
                self.element_registry.with_element(parent, &mut (), |element, _| {
                    element.child_mounted(child_key, slot);
                });
            }
            ChildOp::Update { child, widget } => {
                self.rebuild_element(child, widget);
            }
            ChildOp::Unmount { child } => {
                self.unmount_element_tree(child);
            }
        }
    }
}
```

- [ ] **Step 3: Add mount_element_tree method**

This replaces the old `ElementRegistry::mount_element`. It inserts the element, then calls `mount()` via `with_element`:

```rust
/// Mount a new element tree from a widget.
fn mount_element_tree(&mut self, parent: Option<ElementKey>, widget: Box<dyn Widget>) -> ElementKey {
    let element = widget.create_element();
    let element_key = self.element_registry.insert(element, parent);

    let mut ctx = ElementContext::new(
        element_key,
        parent,
        None, // render_object set during mount
        &mut self.state,
        &mut self.dirty,
        &mut self.render_objects,
        &self.build_owner,
        &self.dirty_sender,
        &mut self.child_ops,
    );
    self.element_registry.with_element(element_key, &mut ctx, |element, ctx| {
        element.mount(ctx);
    });
    self.execute_child_ops();

    // Set render object root if this is the root element
    if parent.is_none() {
        if let Some(ro_id) = self.element_registry.get(element_key).and_then(|el| el.render_object()) {
            self.render_objects.set_root(ro_id);
        }
    }

    element_key
}
```

- [ ] **Step 4: Rewrite unmount_element_tree**

Replace the vacate/restore pattern with `with_element`:

```rust
/// Unmount an element tree recursively.
fn unmount_element_tree(&mut self, element_id: ElementKey) {
    // Recursively unmount children first
    let children = self.element_registry.children(element_id).to_vec();
    for child_id in children {
        self.unmount_element_tree(child_id);
    }

    let parent = self.element_registry.parent(element_id);
    let render_object_id = self.element_registry.get(element_id)
        .and_then(|el| el.render_object());

    let mut ctx = ElementContext::new(
        element_id,
        parent,
        render_object_id,
        &mut self.state,
        &mut self.dirty,
        &mut self.render_objects,
        &self.build_owner,
        &self.dirty_sender,
        &mut self.child_ops,
    );
    self.element_registry.with_element(element_id, &mut ctx, |element, ctx| {
        if let Some(ro_id) = render_object_id {
            ctx.remove_render_object(ro_id);
        }
        element.unmount(ctx);
    });

    self.state.remove(element_id);
    self.element_registry.unmount(element_id);
}
```

- [ ] **Step 5: Rewrite perform_rebuilds**

Replace vacate/restore with `with_element`:

```rust
fn perform_rebuilds(&mut self) {
    self.drain_dirty_channel();
    if !self.build_owner.has_pending_rebuilds() { return; }

    self.build_owner.sort_dirty_by_depth(|id| self.element_registry.depth(id));
    let dirty_ids: Vec<ElementKey> = self.build_owner.drain_dirty_sorted();

    for element_id in dirty_ids {
        if !self.element_registry.contains(element_id) { continue; }
        if !self.build_owner.enter_build_scope(element_id) { continue; }

        let parent = self.element_registry.parent(element_id);
        let mut ctx = ElementContext::new(
            element_id,
            parent,
            None,
            &mut self.state,
            &mut self.dirty,
            &mut self.render_objects,
            &self.build_owner,
            &self.dirty_sender,
            &mut self.child_ops,
        );

        self.element_registry.with_element(element_id, &mut ctx, |element, ctx| {
            element.rebuild_from_state(ctx);
        });

        self.execute_child_ops();
        self.build_owner.exit_build_scope(element_id);
    }
}
```

- [ ] **Step 6: Rewrite rebuild_root**

Replace vacate/restore with `with_element`:

```rust
fn rebuild_root(&mut self, root_id: ElementKey, widget: Box<dyn Widget>) {
    let parent = self.element_registry.parent(root_id);
    let widget_as_any: Box<dyn Any> = Box::new(widget.clone_boxed());

    let mut ctx = ElementContext::new(
        root_id,
        parent,
        None,
        &mut self.state,
        &mut self.dirty,
        &mut self.render_objects,
        &self.build_owner,
        &self.dirty_sender,
        &mut self.child_ops,
    );

    self.element_registry.with_element(root_id, &mut ctx, |element, ctx| {
        element.rebuild(widget_as_any, ctx);
    });

    self.execute_child_ops();
}
```

- [ ] **Step 7: Rewrite reconcile_element**

Replace vacate/restore with `with_element`:

```rust
fn reconcile_element(&mut self, element_id: ElementKey, widget: Box<dyn Widget>) {
    let parent = self.element_registry.parent(element_id);
    let widget_as_any: Box<dyn Any> = Box::new(widget.clone_boxed());

    let mut ctx = ElementContext::new(
        element_id,
        parent,
        None,
        &mut self.state,
        &mut self.dirty,
        &mut self.render_objects,
        &self.build_owner,
        &self.dirty_sender,
        &mut self.child_ops,
    );

    self.element_registry.with_element(element_id, &mut ctx, |element, ctx| {
        element.rebuild(widget_as_any, ctx);
    });

    self.execute_child_ops();
}
```

- [ ] **Step 8: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: Compilation errors in element implementation files (container.rs, single_child.rs, etc.) that still use the old context API. This is expected — we'll fix them in subsequent tasks.

- [ ] **Step 9: Commit**

```bash
git add vexo/src/retain/pipeline.rs
git commit -m "refactor: rewrite ThreeTreePipeline with with_element and execute_child_ops"
```

---

### Task 6: Update ContainerElement to use child_ops

**Files:**
- Modify: `vexo/src/retain/elements/container.rs`

- [ ] **Step 1: Update ContainerElement::mount to use child_ops**

Replace direct `context.inflate_widget()` calls with `context.inflate_child()`:

In `mount()`, for each child widget, replace:
```rust
// OLD
let child_id = context.inflate_widget(&child_widget, Some(i));
self.children.push(child_id);
```
With:
```rust
// NEW
context.inflate_child(Some(i), child_widget);
// child_mounted callback will push the key to self.children
```

- [ ] **Step 2: Implement child_mounted for ContainerElement**

```rust
fn child_mounted(&mut self, child: ElementKey, _slot: Option<usize>) {
    self.children.push(child);
}
```

- [ ] **Step 3: Update ContainerElement::rebuild to use child_ops**

Replace direct registry calls with child_ops commands:

```rust
// OLD
context.update_child(Some(*existing), child_widget.clone_boxed(), Some(i));
// NEW
context.update_child(*existing, child_widget.clone_boxed());

// OLD
let new_id = context.inflate_widget(child_widget, Some(i));
self.children.push(new_id);
// NEW
context.inflate_child(Some(i), child_widget.clone_boxed());

// OLD
context.unmount_child(child);
// NEW
context.unmount_child(child);
```

Note: `update_child` on the new context takes `(child_key, widget)` — no `Option` or `slot` for the update case. `unmount_child` takes just the key.

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles (container.rs fixed)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/elements/container.rs
git commit -m "refactor: update ContainerElement to use child_ops"
```

---

### Task 7: Update SingleChildRenderObjectElement to use child_ops

**Files:**
- Modify: `vexo/src/retain/elements/single_child.rs`

- [ ] **Step 1: Update mount to use child_ops**

Replace `context.inflate_widget()` with `context.inflate_child()`:

```rust
// OLD
let child_id = context.inflate_widget(&child_widget, None);
self.child = Some(child_id);
// NEW
context.inflate_child(None, child_widget);
// child_mounted callback will set self.child
```

- [ ] **Step 2: Implement child_mounted**

```rust
fn child_mounted(&mut self, child: ElementKey, _slot: Option<usize>) {
    self.child = Some(child);
}
```

- [ ] **Step 3: Update rebuild to use child_ops**

Replace `context.update_child()` / `context.inflate_widget()` / `context.unmount_child()` with the new context methods:

```rust
// For update:
context.update_child(self.child.unwrap(), new_widget);

// For inflate:
context.inflate_child(None, new_widget);

// For unmount:
context.unmount_child(self.child.unwrap());
self.child = None;
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/elements/single_child.rs
git commit -m "refactor: update SingleChildRenderObjectElement to use child_ops"
```

---

### Task 8: Update MultiChildRenderObjectElement to use child_ops

**Files:**
- Modify: `vexo/src/retain/elements/multi_child.rs`

- [ ] **Step 1: Update mount to use child_ops**

Replace `context.inflate_widget()` calls with `context.inflate_child()`:

```rust
// OLD
let child_id = context.inflate_widget(&child_widget, Some(i));
self.children.push(child_id);
// NEW
context.inflate_child(Some(i), child_widget);
// child_mounted callback will push the key
```

- [ ] **Step 2: Implement child_mounted**

```rust
fn child_mounted(&mut self, child: ElementKey, _slot: Option<usize>) {
    self.children.push(child);
}
```

- [ ] **Step 3: Update rebuild to use child_ops**

Same pattern as ContainerElement — replace direct registry calls with child_ops commands.

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/elements/multi_child.rs
git commit -m "refactor: update MultiChildRenderObjectElement to use child_ops"
```

---

### Task 9: Update RenderObjectElement trait impls

**Files:**
- Modify: `vexo/src/retain/elements/render_object_element.rs`

- [ ] **Step 1: Review and update any direct registry access in render object element methods**

The `RenderObjectElement` trait methods (`mount`, `update`, `unmount`) may access the render object registry through the context. Since `render_objects` is still on `ElementContext` (just no longer wrapped in `Option`), most code should work with minimal changes — just remove any `take_*`/`restore_*` calls for `render_objects`.

Replace:
```rust
// OLD
let render_objects = context.render_objects.take().unwrap();
// ... use render_objects ...
context.render_objects = Some(render_objects);
```
With:
```rust
// NEW — direct access, no Option
context.render_objects.some_method();
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/elements/render_object_element.rs
git commit -m "refactor: update RenderObjectElement to use direct context access"
```

---

### Task 10: Update StatefulElement to remove take/restore

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs`

- [ ] **Step 1: Remove all take/restore patterns in StatefulElement**

StatefulElement currently takes `context.render_objects`, `context.build_owner`, etc. out of their `Option` wrappers, uses them, then restores them. With the new `ElementContext` (no `Option` wrappers), these become direct field accesses.

Replace all patterns like:
```rust
// OLD
let build_owner = context.build_owner.take().unwrap();
// ... use build_owner ...
context.build_owner = Some(build_owner);
```
With:
```rust
// NEW — direct access
context.build_owner.some_method();
```

- [ ] **Step 2: Update StatefulElement::rebuild and StatefulElement::update to use child_ops**

If StatefulElement calls `context.inflate_widget()` or `context.update_child()` for its child, replace with `context.inflate_child()` / `context.update_child()` / `context.unmount_child()`.

- [ ] **Step 3: Implement child_mounted for StatefulElement**

```rust
fn child_mounted(&mut self, child: ElementKey, _slot: Option<usize>) {
    self.child = Some(child);
}
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "refactor: update StatefulElement to remove take/restore and use child_ops"
```

---

### Task 11: Full build and test

**Files:**
- All modified files

- [ ] **Step 1: Full build**

Run: `cargo build -p vexo`
Expected: Clean build with no errors or warnings

- [ ] **Step 2: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 3: Run desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Application runs normally, renders the demo UI

- [ ] **Step 4: Commit if any fixes were needed**

```bash
git add -A
git commit -m "fix: resolve compilation issues from vacate/restore elimination"
```

---

### Task 12: Clean up dead code

**Files:**
- Modify: `vexo/src/retain/element.rs`
- Modify: `vexo/src/retain/element_context.rs`
- Any other files with unused imports or dead code

- [ ] **Step 1: Remove unused imports and dead code**

Run: `cargo clippy -p vexo 2>&1 | grep "unused\|dead_code\|warning"`
Fix any warnings found.

- [ ] **Step 2: Verify clean build**

Run: `cargo build -p vexo`
Expected: No warnings

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore: clean up dead code after vacate/restore elimination"
```
