# Remove Redundant Child Storage and Simplify `child_mounted` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove redundant child key storage from elements (`self.children`, `self.child_element`, `self.child_element_id`) by having elements read their children from `ElementContext`. Simplify `child_mounted` to only handle render object linking (its remaining responsibility after child key storage is removed).

**Architecture:** Two changes:
1. **Child key tracking** — Elements currently store children redundantly in internal fields. They'll read children from `ElementContext` (populated by the reconciler from `ElementRegistry::children()`) instead. This eliminates the need for `child_mounted` to store child keys.
2. **Simplify `child_mounted`** — After removing child key storage from elements, `child_mounted` only does render object linking. Simplify its signature to `(child_ro, slot, context)` — removing the `child` parameter since the element no longer stores it.

**Tech Stack:** Rust, slotmaps, existing retain module infrastructure

---

## File Structure

| File | Change | Purpose |
|------|--------|---------|
| `vexo/src/retain/element.rs` | Modify | Simplify `child_mounted` signature |
| `vexo/src/retain/element_context.rs` | Modify | Add `children` field for registry-based child lookup |
| `vexo/src/retain/reconciler.rs` | Modify | Update `child_mounted` call with new signature; populate `children` in context |
| `vexo/src/retain/elements/container.rs` | Modify | Remove `self.children` field; use `context.children()`; simplify `child_mounted` |
| `vexo/src/retain/widgets/decorated_container.rs` | Modify | Remove `self.child_element` field; use `context.children()`; simplify `child_mounted` |
| `vexo/src/retain/widgets/gesture_detector.rs` | Modify | Remove `self.child_element` field; use `context.children()`; simplify `child_mounted` |
| `vexo/src/retain/stateful_widget.rs` | Modify | Remove `self.child_element_id` field; use `context.children()`; simplify `child_mounted` |

---

### Task 1: Add `children` field to ElementContext

**Files:**
- Modify: `vexo/src/retain/element_context.rs`

Elements need to know their children during `rebuild()` but can't hold `&ElementRegistry` (borrow conflict with `with_element`). Solution: the reconciler copies the children list into the context before calling element methods.

- [ ] **Step 1: Add `children` field and accessor to `ElementContext`**

```rust
pub struct ElementContext<'a> {
    pub element_id: ElementKey,
    pub parent: Option<ElementKey>,
    pub children: Vec<ElementKey>,  // NEW: current element's children from registry
    pub state: &'a mut StateStorage,
    pub dirty: &'a mut DirtyTracking,
    pub render_objects: &'a mut RenderObjectRegistry,
    pub build_owner: &'a BuildOwner,
    pub dirty_sender: &'a mpsc::Sender<ElementKey>,
    pub child_ops: &'a mut ChildOps,
}
```

Update `new()` to accept `children: Vec<ElementKey>` as the third parameter (after `parent`).

Add accessor:

```rust
/// Get the children of this element.
///
/// Set by the reconciler before calling element lifecycle methods.
/// Elements use this instead of storing children internally.
pub fn children(&self) -> &[ElementKey] {
    &self.children
}
```

- [ ] **Step 2: Update all `ElementContext::new()` calls in reconciler.rs**

Every call site must pass the children. The reconciler already has `element_registry` in scope. At each call site, read `element_registry.children(element_id).to_vec()` and pass it.

For newly mounted elements (in `mount_element_tree`), pass `Vec::new()` since the element has no children yet.

Call sites in `reconciler.rs`:
1. `mount_element_tree` (line ~409): `Vec::new()` (newly mounted, no children yet)
2. `execute_child_ops` → `ChildOp::Inflate` arm (line ~524): `element_registry.children(parent).to_vec()`
3. `rebuild_element` (line ~591): `element_registry.children(element_id).to_vec()`
4. `unmount_element_tree` (line ~651): `element_registry.children(element_id).to_vec()`
5. Any other call sites in `reconciler.rs` or elsewhere

- [ ] **Step 3: Run `cargo build -p vexo` to verify compilation**

Run: `cargo build -p vexo`
Expected: May fail — element implementations still reference internal child fields. That's expected; we'll fix those in subsequent tasks.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/element_context.rs vexo/src/retain/reconciler.rs
git commit -m "refactor: add children field to ElementContext for registry-based child lookup"
```

---

### Task 2: Simplify `child_mounted` signature on Element trait

**Files:**
- Modify: `vexo/src/retain/element.rs`

Since elements will no longer store child keys in `child_mounted`, the `child` parameter is unnecessary. The method only needs the child's render object key for linking, and the slot for multi-child elements.

- [ ] **Step 1: Simplify `child_mounted` signature**

Current:
```rust
fn child_mounted(&mut self, _child: ElementKey, _slot: Option<usize>, _child_ro: Option<super::id::RenderObjectKey>, _context: &mut ElementContext) {}
```

New:
```rust
/// Called by the reconciler after a ChildOp::Inflate is executed,
/// to link the child's render object into the parent's render object tree.
///
/// Elements that own render objects and have children should override this
/// to connect the child's render object to their own.
///
/// The `child_ro` parameter is the child's render object key (if any).
/// The `slot` parameter indicates the position for multi-child elements.
fn child_mounted(&mut self, _slot: Option<usize>, _child_ro: Option<super::id::RenderObjectKey>, _context: &mut ElementContext) {}
```

Removed: `child: ElementKey` parameter (elements no longer store child keys — they read from `context.children()`).

- [ ] **Step 2: Update the reconciler's call site**

In `reconciler.rs`, update the `child_mounted` call in the `ChildOp::Inflate` handler:

Current:
```rust
element_registry.with_element(parent, &mut ctx, |element, ctx| {
    element.child_mounted(child_key, slot, child_ro, ctx);
});
```

New:
```rust
element_registry.with_element(parent, &mut ctx, |element, ctx| {
    element.child_mounted(slot, child_ro, ctx);
});
```

- [ ] **Step 3: Run `cargo build -p vexo`**

Expected: FAIL — four element implementations still have the old `child_mounted` signature with `child` parameter.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/element.rs vexo/src/retain/reconciler.rs
git commit -m "refactor: simplify child_mounted signature — remove child key parameter"
```

---

### Task 3: Remove `self.children` field from ContainerElement and simplify `child_mounted`

**Files:**
- Modify: `vexo/src/retain/elements/container.rs`

- [ ] **Step 1: Remove `self.children` field from `ContainerElement`**

Remove the `children: Vec<ElementKey>` field from the struct. Update `new()` and `with_key()` to not initialize it.

- [ ] **Step 2: Update `MultiChildRenderObjectElement` impl**

`child_elements()` currently returns `&self.children`. Since the field is removed, `child_elements()` returns `&[]` (empty slice). `set_child_elements()` and `add_child_element()` become no-ops.

`insert_child_render_object` and `clear_child_render_objects` only use `render_object_id()`, so they continue to work unchanged.

- [ ] **Step 3: Update `rebuild()` to use `context.children()` instead of `self.children`**

Current:
```rust
let old_len = self.children.len();
let new_len = new_child_widgets.len();
for (i, new_child_widget) in new_child_widgets.into_iter().enumerate() {
    if i < old_len {
        context.update_child(self.children[i], new_child_widget);
    } else {
        context.inflate_child(Some(i), new_child_widget);
    }
}
for i in (new_len..old_len).rev() {
    context.unmount_child(self.children[i]);
}
self.children.truncate(new_len);
```

New:
```rust
let old_children = context.children().to_vec();
let old_len = old_children.len();
let new_len = new_child_widgets.len();
for (i, new_child_widget) in new_child_widgets.into_iter().enumerate() {
    if i < old_len {
        context.update_child(old_children[i], new_child_widget);
    } else {
        context.inflate_child(Some(i), new_child_widget);
    }
}
for i in (new_len..old_len).rev() {
    context.unmount_child(old_children[i]);
}
```

Note: `self.children.truncate(new_len)` is removed — the registry's children list is updated by the `ChildOp::Unmount` handler.

- [ ] **Step 4: Simplify `child_mounted` impl**

Current:
```rust
fn child_mounted(&mut self, child: ElementKey, slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
    // Track the child element key at the given slot position
    if let Some(idx) = slot {
        if idx >= self.children.len() {
            self.children.resize(idx + 1, child);
        } else {
            self.children[idx] = child;
        }
    } else {
        self.children.push(child);
    }
    // Link the child's render object to our render object
    if let Some(child_ro_key) = child_ro {
        self.insert_child_render_object(child_ro_key, context);
    }
}
```

New (child key tracking removed, only render object linking remains):
```rust
fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
    // Link the child's render object to our render object
    if let Some(child_ro_key) = child_ro {
        self.insert_child_render_object(child_ro_key, context);
    }
}
```

- [ ] **Step 5: Update `mount()` comment**

Replace:
```rust
// Mount children via child_ops (emit Inflate commands)
// The pipeline will execute them after mount() returns,
// then call child_mounted() to notify us of each new child's key.
```

With:
```rust
// Mount children via child_ops (emit Inflate commands)
// The reconciler will execute them after mount() returns,
// then call child_mounted() to link each child's render object.
```

- [ ] **Step 6: Run `cargo build -p vexo`**

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/elements/container.rs
git commit -m "refactor: remove self.children from ContainerElement, simplify child_mounted"
```

---

### Task 4: Remove `self.child_element` field from DecoratedContainerElement and simplify `child_mounted`

**Files:**
- Modify: `vexo/src/retain/widgets/decorated_container.rs`

- [ ] **Step 1: Remove `self.child_element` field from `DecoratedContainerElement`**

Remove the `child_element: Option<ElementKey>` field. Update `new()` and constructors.

- [ ] **Step 2: Update `SingleChildRenderObjectElement` impl**

`child_element()` currently returns `self.child_element`. Change to return `None`. `set_child_element()` becomes a no-op.

`insert_child_render_object` and `remove_child_render_object` only use `render_object_id()`, so they continue to work unchanged.

- [ ] **Step 3: Update `rebuild()` to use `context.children()` instead of `self.child_element`**

Current:
```rust
if let Some(child_widget) = self.get_child_widget() {
    match self.child_element {
        Some(old_child) => {
            context.update_child(old_child, child_widget.clone_boxed());
        }
        None => {
            context.inflate_child(None, child_widget.clone_boxed());
        }
    }
} else if let Some(old_child) = self.child_element {
    context.unmount_child(old_child);
    self.child_element = None;
}
```

New:
```rust
let old_child = context.children().first().copied();
if let Some(child_widget) = self.get_child_widget() {
    match old_child {
        Some(old_child_key) => {
            context.update_child(old_child_key, child_widget.clone_boxed());
        }
        None => {
            context.inflate_child(None, child_widget.clone_boxed());
        }
    }
} else if let Some(old_child_key) = old_child {
    context.unmount_child(old_child_key);
}
```

- [ ] **Step 4: Simplify `child_mounted` impl**

Current:
```rust
fn child_mounted(&mut self, child: ElementKey, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
    self.child_element = Some(child);
    // Link the child's render object to our render object
    if let Some(child_ro_key) = child_ro {
        self.insert_child_render_object(child_ro_key, context);
    }
}
```

New:
```rust
fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
    // Link the child's render object to our render object
    if let Some(child_ro_key) = child_ro {
        self.insert_child_render_object(child_ro_key, context);
    }
}
```

- [ ] **Step 5: Update `mount()` comment**

Replace:
```rust
// Mount single child via child_ops (emit Inflate command)
// The pipeline will execute it after mount() returns,
// then call child_mounted() to notify us of the new child's key.
```

With:
```rust
// Mount single child via child_ops (emit Inflate command)
// The reconciler will execute it after mount() returns,
// then call child_mounted() to link the child's render object.
```

- [ ] **Step 6: Run `cargo build -p vexo`**

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/widgets/decorated_container.rs
git commit -m "refactor: remove self.child_element from DecoratedContainerElement, simplify child_mounted"
```

---

### Task 5: Remove `self.child_element` field from GestureDetectorElement and simplify `child_mounted`

**Files:**
- Modify: `vexo/src/retain/widgets/gesture_detector.rs`

Same pattern as Task 4 (single-child element).

- [ ] **Step 1: Remove `self.child_element` field**

- [ ] **Step 2: Update `SingleChildRenderObjectElement` impl**

`child_element()` returns `None`, `set_child_element()` is no-op.

- [ ] **Step 3: Update `rebuild()` to use `context.children()`**

```rust
let old_child = context.children().first().copied();
```

Same pattern as DecoratedContainerElement.

- [ ] **Step 4: Simplify `child_mounted` impl**

Same as DecoratedContainerElement — remove `self.child_element = Some(child)`, keep only render object linking.

- [ ] **Step 5: Update `mount()` comment**

- [ ] **Step 6: Run `cargo build -p vexo`**

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/widgets/gesture_detector.rs
git commit -m "refactor: remove self.child_element from GestureDetectorElement, simplify child_mounted"
```

---

### Task 6: Remove `self.child_element_id` from StatefulElement and simplify `child_mounted`

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs`

StatefulElement is special — `child_mounted` also sets `self.render_object_id = child_ro` (delegation). This stays because StatefulElement genuinely needs to know its child's render object to return from `render_object()`. But the child key storage (`self.child_element_id`) is removed.

- [ ] **Step 1: Remove `self.child_element_id` field from `StatefulElement`**

Remove `child_element_id: Option<ElementKey>` from the struct. Update `new()`.

- [ ] **Step 2: Update `update()` to use `context.children()` instead of `self.child_element_id`**

Current:
```rust
// Reconcile child via child_ops
match self.child_element_id {
    Some(old_child) => {
        context.update_child(old_child, child_widget);
    }
    None => {
        context.inflate_child(None, child_widget);
    }
}
```

New:
```rust
let old_child = context.children().first().copied();
match old_child {
    Some(old_child_key) => {
        context.update_child(old_child_key, child_widget);
    }
    None => {
        context.inflate_child(None, child_widget);
    }
}
```

- [ ] **Step 3: Update `rebuild_from_state()` to use `context.children()`**

Same pattern — replace `self.child_element_id` with `context.children().first().copied()`.

- [ ] **Step 4: Update `unmount()` to use `context.children()`**

Current:
```rust
if let Some(child_id) = self.child_element_id {
    context.unmount_child(child_id);
}
```

New:
```rust
if let Some(child_key) = context.children().first().copied() {
    context.unmount_child(child_key);
}
```

- [ ] **Step 5: Simplify `child_mounted` impl**

Current:
```rust
fn child_mounted(&mut self, child: ElementKey, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, _context: &mut ElementContext) {
    self.child_element_id = Some(child);
    // StatefulElement delegates its render_object_id to its child's render object
    self.render_object_id = child_ro;
}
```

New (child key storage removed, RO delegation stays):
```rust
fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, _context: &mut ElementContext) {
    // StatefulElement delegates its render_object_id to its child's render object
    self.render_object_id = child_ro;
}
```

- [ ] **Step 6: Run `cargo build -p vexo`**

- [ ] **Step 7: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "refactor: remove self.child_element_id from StatefulElement, simplify child_mounted"
```

---

### Task 7: Update doc comments and clean up references

**Files:**
- Various files with `child_mounted` references in comments

- [ ] **Step 1: Update `ElementRegistry::add_child` doc comment**

In `element.rs`, change:
```rust
/// Called by the pipeline after executing a ChildOp::Inflate.
```
To:
```rust
/// Called by the reconciler after executing a ChildOp::Inflate.
```

- [ ] **Step 2: Update reconciler doc comments**

In `reconciler.rs`:
- Update `execute_child_ops` doc: change "notifies the parent via `child_mounted`" to "calls `child_mounted` to link the child's render object"
- Update `mount_element_tree` doc: remove "or call child_mounted"
- Update the comment at line ~449 about StatefulElement delegation

- [ ] **Step 3: Search for any remaining stale references**

Run: `grep -r "child_mounted" vexo/src/`
Verify all references are accurate and up-to-date.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/
git commit -m "refactor: update child_mounted doc comments"
```

---

### Task 8: Build and test

**Files:**
- None (verification only)

- [ ] **Step 1: Run full build**

Run: `cargo build`
Expected: PASS

- [ ] **Step 2: Run tests**

Run: `cargo test`
Expected: PASS

- [ ] **Step 3: Run desktop demo to verify runtime behavior**

Run: `cargo run -p desktop_demo`
Expected: App launches and renders correctly (visual verification)

- [ ] **Step 4: Commit if any fixes were needed**

---

## Self-Review

### What this plan achieves
- Removes **3 redundant fields** (`self.children`, `self.child_element`, `self.child_element_id`) — elements read from `context.children()` instead
- Simplifies `child_mounted` signature — removes `child` parameter since elements no longer store child keys
- `child_mounted` now has a **single clear responsibility**: render object linking
- No ancestor walks, no heuristics, no implicit contracts — the notification pattern stays explicit

### What stays the same
- `child_mounted` remains on the `Element` trait — it's a clear, explicit notification
- StatefulElement still delegates `render_object_id` in `child_mounted` — this is genuinely needed
- The reconciler still calls `child_mounted` after `ChildOp::Inflate` — same flow, simpler data

### Placeholder scan
No TBDs, TODOs, or "implement later" patterns.

### Type consistency
- `context.children()` returns `&[ElementKey]` — matches `ElementRegistry::children()` return type
- `child_mounted` signature: `(slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext)` — consistent across trait and all impls
- `ElementContext::new()` gains `children: Vec<ElementKey>` — all call sites updated in Task 1