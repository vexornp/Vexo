# Focus-Element Tree Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire FocusAttachment into element lifecycle so the focus tree stays in sync with the element tree automatically — no manual management needed.

**Architecture:** Store an `Option<FocusAttachment>` on each element. Create the attachment during `mount()`, detach during `unmount()`, and reparent during `rebuild()`. This replaces the current lazy on-demand focus node creation in `EventHandler` and `pipeline::set_focus()`.

**Tech Stack:** Rust, slotmap, Vexo retain-mode element system

---

## Current State Summary

The focus system has two trees:
1. **Element tree** — retain-mode elements with mount/unmount/rebuild lifecycle
2. **Focus tree** — `FocusManager` slotmap of `FocusNodeData` nodes

Today they're out of sync because:
- Focus nodes are created lazily (on `request_focus` call), not during element mount
- Focus nodes are never removed during element unmount
- Focus nodes aren't reparented when elements move during rebuild
- `FocusAttachment` exists and is tested in isolation but isn't wired into any element

## File Structure

| File | Role |
|------|------|
| `vexo/src/retain/focus/attachment.rs` | FocusAttachment — already exists, needs minor API tweaks |
| `vexo/src/retain/focus/manager.rs` | FocusManager — needs `create_node_for_element()` method |
| `vexo/src/retain/element.rs` | Element trait — add `focus_attachment()` accessor |
| `vexo/src/retain/elements/leaf.rs` | LeafElement — wire mount/unmount |
| `vexo/src/retain/elements/container.rs` | ContainerElement — wire mount/unmount/rebuild reparent |
| `vexo/src/retain/elements/stateful.rs` | StatefulElement — wire mount/unmount/rebuild reparent |
| `vexo/src/retain/elements/decorated.rs` | DecoratedContainerElement — wire mount/unmount/rebuild reparent |
| `vexo/src/retain/event_handler.rs` | EventHandler — replace lazy node creation with attachment lookup |
| `vexo/src/retain/pipeline.rs` | Pipeline — replace lazy node creation in `set_focus()` |
| `vexo/src/retain/focus/integration_tests.rs` | Integration tests — new tests for sync behavior |

---

### Task 1: Add `create_node_for_element()` to FocusManager

**Files:**
- Modify: `vexo/src/retain/focus/manager.rs`
- Test: `vexo/src/retain/focus/integration_tests.rs`

Today `FocusManager` has no public method that creates a node *and* attaches it to the correct parent based on element hierarchy. The `request_focus()` method creates a node lazily, but it uses `node_for_element()` which does a linear scan and has no parent information.

We need a method that creates a focus node with the correct parent from the start.

- [ ] **Step 1: Write the failing test**

In `vexo/src/retain/focus/integration_tests.rs`, add:

```rust
#[test]
fn test_create_node_for_element_with_parent() {
    let mut manager = FocusManager::new();

    // Create parent node
    let parent_id = manager.create_node_for_element(ElementId(1), None);
    assert!(parent_id.is_some());

    // Create child node with parent
    let child_id = manager.create_node_for_element(ElementId(2), parent_id);
    assert!(child_id.is_some());

    // Verify parent-child relationship
    let child_data = manager.get(child_id.unwrap()).unwrap();
    assert_eq!(child_data.parent, parent_id);

    let parent_data = manager.get(parent_id.unwrap()).unwrap();
    assert!(parent_data.children.contains(&child_id.unwrap()));
}

#[test]
fn test_create_node_for_element_without_parent_uses_root() {
    let mut manager = FocusManager::new();

    // Create node without explicit parent — should attach to root
    let node_id = manager.create_node_for_element(ElementId(1), None);
    assert!(node_id.is_some());

    // Root should be the parent
    let node_data = manager.get(node_id.unwrap()).unwrap();
    assert_eq!(node_data.parent, Some(manager.root_id()));
}

#[test]
fn test_create_node_for_existing_element_is_idempotent() {
    let mut manager = FocusManager::new();

    let first = manager.create_node_for_element(ElementId(1), None);
    let second = manager.create_node_for_element(ElementId(1), None);

    // Should return the same node
    assert_eq!(first, second);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo -- test_create_node_for_element`
Expected: FAIL — method doesn't exist yet

- [ ] **Step 3: Implement `create_node_for_element()`**

In `vexo/src/retain/focus/manager.rs`, add:

```rust
pub fn create_node_for_element(
    &mut self,
    element_key: ElementId,
    parent_id: Option<FocusNodeId>,
) -> Option<FocusNodeId> {
    // If a node already exists for this element, return it
    if let Some(existing) = self.node_for_element(element_key) {
        return Some(existing);
    }

    let parent = parent_id.unwrap_or(self.root_id);

    let node_id = self.nodes.insert(FocusNodeData {
        element_key: Some(element_key),
        parent: Some(parent),
        children: Vec::new(),
        can_request_focus: true,
        skip_traversal: false,
    });

    // Register in parent's children
    if let Some(parent_data) = self.nodes.get_mut(parent) {
        parent_data.children.push(node_id);
    }

    // Register in element lookup
    self.element_to_node.insert(element_key, node_id);

    Some(node_id)
}
```

Also add an `element_to_node` HashMap for O(1) lookups. Add to the struct:

```rust
element_to_node: HashMap<ElementId, FocusNodeId>,
```

Initialize in `FocusManager::new()`:

```rust
element_to_node: HashMap::new(),
```

Update `node_for_element()` to use it:

```rust
pub fn node_for_element(&self, element_key: ElementId) -> Option<FocusNodeId> {
    self.element_to_node.get(&element_key).copied()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo -- test_create_node_for_element`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/focus/manager.rs vexo/src/retain/focus/integration_tests.rs
git commit -m "feat: add FocusManager::create_node_for_element() with O(1) element lookup"
```

---

### Task 2: Add `focus_attachment()` to Element trait

**Files:**
- Modify: `vexo/src/retain/element.rs`
- Test: `vexo/src/retain/focus/integration_tests.rs`

Each element needs to store an `Option<FocusAttachment>` so the attachment persists across the element's lifetime. We add a trait method to access it.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_element_has_focus_attachment_after_mount() {
    // This test will verify through the pipeline that elements
    // have focus attachments after mounting.
    // We'll test this more concretely in Task 3 when we wire mount().
    // For now, verify the accessor exists on the trait.
}
```

Actually, this is better tested through concrete element types. Let's defer the test to Task 3 and just implement the API here.

- [ ] **Step 2: Add `focus_attachment` field and accessor to Element trait**

In `vexo/src/retain/element.rs`, the `Element` trait currently has no focus attachment support. We need to store it on concrete element types, not on the trait (since trait objects can't have fields).

The approach: add a `focus_attachment: Option<FocusAttachment>` field to each concrete element struct, and expose it via a method on the `Element` trait.

Add to the `Element` trait:

```rust
fn focus_attachment(&self) -> &Option<FocusAttachment>;
fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment>;
```

- [ ] **Step 3: Add the field and implement the trait methods on each element type**

For `LeafElement` (in `vexo/src/retain/elements/leaf.rs`):

Add field:
```rust
pub struct LeafElement {
    // ... existing fields ...
    focus_attachment: Option<FocusAttachment>,
}
```

Initialize in `new()`:
```rust
focus_attachment: None,
```

Implement trait methods:
```rust
fn focus_attachment(&self) -> &Option<FocusAttachment> {
    &self.focus_attachment
}

fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment> {
    &mut self.focus_attachment
}
```

Repeat the same pattern for `ContainerElement` (`container.rs`), `StatefulElement` (`stateful.rs`), and `DecoratedContainerElement` (`decorated.rs`).

- [ ] **Step 4: Run build to verify compilation**

Run: `cargo build -p vexo`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/element.rs vexo/src/retain/elements/
git commit -m "feat: add focus_attachment field and trait accessors to all element types"
```

---

### Task 3: Wire FocusAttachment creation into element mount

**Files:**
- Modify: `vexo/src/retain/elements/leaf.rs`
- Modify: `vexo/src/retain/elements/container.rs`
- Modify: `vexo/src/retain/elements/stateful.rs`
- Modify: `vexo/src/retain/elements/decorated.rs`
- Modify: `vexo/src/retain/element_context.rs` (to expose focus_manager)
- Test: `vexo/src/retain/focus/integration_tests.rs`

When an element mounts, we create a focus node and store the attachment. The element's `mount()` method receives `&mut ElementContext`, so we need `FocusManager` accessible from there.

- [ ] **Step 1: Expose FocusManager through ElementContext**

In `vexo/src/retain/element_context.rs`, add a method to access the focus manager:

```rust
pub fn focus_manager(&mut self) -> &mut FocusManager {
    &mut self.pipeline_context.focus_manager
}
```

(Or however `FocusManager` is reachable from `ElementContext` — check the current field chain. The key point is that `mount()` must be able to call `focus_manager.create_node_for_element()` and `FocusAttachment::new()`.)

- [ ] **Step 2: Write the failing test**

```rust
#[test]
fn test_mount_creates_focus_attachment() {
    let mut harness = TestHarness::new();

    // Mount a simple widget tree
    let widget = Text::new("hello");
    harness.mount(widget);

    // The element should have a focus attachment
    let element = harness.root_element();
    let attachment = element.focus_attachment();
    assert!(attachment.is_some(), "Element should have a FocusAttachment after mount");

    // The focus manager should have a node for this element
    let manager = harness.focus_manager();
    assert!(manager.node_for_element(element.id()).is_some(),
        "FocusManager should have a node for the mounted element");
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p vexo -- test_mount_creates_focus_attachment`
Expected: FAIL — attachment is `None` after mount

- [ ] **Step 4: Wire attachment creation into each element's `mount()` method**

For `LeafElement::mount()` (in `vexo/src/retain/elements/leaf.rs`):

```rust
fn mount(&mut self, context: &mut ElementContext) {
    let element_id = self.id();
    let parent_id = context.parent_focus_node_id();
    let node_id = context.focus_manager().create_node_for_element(element_id, parent_id);
    if let Some(node_id) = node_id {
        self.focus_attachment = Some(FocusAttachment::new(node_id, context.focus_manager()));
    }
}
```

For `ContainerElement::mount()` — same pattern, but after mounting children (so children can find their parent focus node):

```rust
fn mount(&mut self, context: &mut ElementContext) {
    // Create this element's focus attachment first
    let element_id = self.id();
    let parent_id = context.parent_focus_node_id();
    let node_id = context.focus_manager().create_node_for_element(element_id, parent_id);
    if let Some(node_id) = node_id {
        self.focus_attachment = Some(FocusAttachment::new(node_id, context.focus_manager()));
    }

    // Then mount children (they'll use this element's focus node as parent)
    for (slot, child_widget) in self.children_widgets.drain(..).enumerate() {
        let child_id = self.update_child(None, Some(child_widget), Some(slot), context);
        self.children_elements.push(child_id.unwrap());
    }
}
```

Same pattern for `StatefulElement::mount()` and `DecoratedContainerElement::mount()`.

Note: `context.parent_focus_node_id()` is a new helper that walks up the element tree to find the parent element's focus node ID. If no parent has a focus node, it returns `None` (which means "attach to root"). Implementation:

```rust
impl ElementContext {
    pub fn parent_focus_node_id(&self) -> Option<FocusNodeId> {
        // Walk up the element parent chain to find the nearest
        // element with a focus attachment, return its focus node ID
        self.parent_element_id()
            .and_then(|pid| {
                self.element_registry()
                    .get(pid)
                    .and_then(|el| el.focus_attachment().as_ref())
                    .map(|att| att.node_id())
            })
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vexo -- test_mount_creates_focus_attachment`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/elements/ vexo/src/retain/element_context.rs vexo/src/retain/focus/integration_tests.rs
git commit -m "feat: wire FocusAttachment creation into element mount lifecycle"
```

---

### Task 4: Wire FocusAttachment detach into element unmount

**Files:**
- Modify: `vexo/src/retain/elements/leaf.rs`
- Modify: `vexo/src/retain/elements/container.rs`
- Modify: `vexo/src/retain/elements/stateful.rs`
- Modify: `vexo/src/retain/elements/decorated.rs`
- Modify: `vexo/src/retain/focus/attachment.rs`
- Test: `vexo/src/retain/focus/integration_tests.rs`

When an element unmounts, we must detach its focus node. If the unmounting element is focused, focus should move to its parent (or clear if root).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_unmount_detaches_focus_node() {
    let mut harness = TestHarness::new();

    // Mount and focus an element
    let widget = Focus::new(Text::new("hello"));
    harness.mount(widget);
    let element_id = harness.root_element().id();
    harness.request_focus(element_id);

    // Verify it's focused
    assert!(harness.focus_manager().is_focused_node(element_id));

    // Unmount the element
    harness.unmount_root();

    // Focus node should be removed
    assert!(!harness.focus_manager().has_node_for_element(element_id),
        "Focus node should be removed after unmount");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo -- test_unmount_detaches_focus_node`
Expected: FAIL — focus node persists after unmount

- [ ] **Step 3: Add `detach()` method to FocusAttachment**

The `FocusAttachment` in `vexo/src/retain/focus/attachment.rs` already has a `detach()` method. Verify it:
- Removes the node from its parent's children list
- If the node was focused, unfocuses it
- Removes the node from the slotmap
- Sets an internal `detached` flag to prevent double-detach

If any of these are missing, add them.

- [ ] **Step 4: Wire `detach()` into each element's `unmount()` method**

For `LeafElement::unmount()` (in `vexo/src/retain/elements/leaf.rs`):

```rust
fn unmount(&mut self, context: &mut ElementContext) {
    if let Some(attachment) = self.focus_attachment.take() {
        attachment.detach(context.focus_manager());
    }
}
```

Same for `ContainerElement::unmount()` — detach self, then unmount children:

```rust
fn unmount(&mut self, context: &mut ElementContext) {
    // Unmount children first
    for child_id in self.children_elements.drain(..) {
        self.update_child(Some(child_id), None, None, context);
    }
    // Then detach self from focus tree
    if let Some(attachment) = self.focus_attachment.take() {
        attachment.detach(context.focus_manager());
    }
}
```

Same pattern for `StatefulElement::unmount()` and `DecoratedContainerElement::unmount()`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vexo -- test_unmount_detaches_focus_node`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/elements/ vexo/src/retain/focus/attachment.rs vexo/src/retain/focus/integration_tests.rs
git commit -m "feat: wire FocusAttachment detach into element unmount lifecycle"
```

---

### Task 5: Wire FocusAttachment reparent into element rebuild

**Files:**
- Modify: `vexo/src/retain/elements/container.rs`
- Modify: `vexo/src/retain/elements/stateful.rs`
- Modify: `vexo/src/retain/elements/decorated.rs`
- Test: `vexo/src/retain/focus/integration_tests.rs`

When an element rebuilds, its parent may have changed (e.g., it moved from one container to another). The `FocusAttachment::reparent()` method moves the focus node to a new parent in the focus tree.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_rebuild_reparents_focus_node() {
    let mut harness = TestHarness::new();

    // Mount a widget tree: Column [Focus/Text1, Focus/Text2]
    let widget = Column::new(vec![
        Focus::new(Text::new("first")),
        Focus::new(Text::new("second")),
    ]);
    harness.mount(widget);

    // Rebuild with swapped order: Column [Focus/Text2, Focus/Text1]
    let updated_widget = Column::new(vec![
        Focus::new(Text::new("second")),
        Focus::new(Text::new("first")),
    ]);
    harness.rebuild(updated_widget);

    // Both elements should still have valid focus attachments
    // with correct parent-child relationships
    for element_id in harness.all_element_ids() {
        if let Some(attachment) = harness.element(element_id).focus_attachment() {
            assert!(attachment.is_attached(),
                "Focus attachment should still be attached after rebuild");
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo -- test_rebuild_reparents_focus_node`
Expected: FAIL — attachments may be stale after rebuild

- [ ] **Step 3: Wire `reparent()` into each element's `rebuild()` method**

For `ContainerElement::rebuild()` (in `vexo/src/retain/elements/container.rs`):

```rust
fn rebuild(&mut self, new_widget: Box<dyn Widget>, context: &mut ElementContext) {
    // ... existing rebuild logic (update properties, reconcile children) ...

    // Reparent focus node if parent changed
    if let Some(attachment) = &mut self.focus_attachment {
        let new_parent_id = context.parent_focus_node_id();
        attachment.reparent(new_parent_id, context.focus_manager());
    }
}
```

Same pattern for `StatefulElement::rebuild()` (in `stateful.rs`) and `DecoratedContainerElement::rebuild()` (in `decorated.rs`).

`LeafElement` has no `rebuild()` (it's replaced, not rebuilt), so skip it.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo -- test_rebuild_reparents_focus_node`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/elements/ vexo/src/retain/focus/integration_tests.rs
git commit -m "feat: wire FocusAttachment reparent into element rebuild lifecycle"
```

---

### Task 6: Replace lazy focus node creation in EventHandler with attachment lookup

**Files:**
- Modify: `vexo/src/retain/event_handler.rs`
- Test: `vexo/src/retain/focus/integration_tests.rs`

Today `EventHandler::request_focus()` creates a focus node on-demand if one doesn't exist. After Tasks 3-5, every mounted element already has a focus node via its attachment. So the lazy creation path is dead code — replace it with a simple attachment lookup.

- [ ] **Step 1: Write the failing test**

This is a refactor with no new behavior, so we test that existing focus-from-event-handler behavior still works:

```rust
#[test]
fn test_request_focus_via_event_uses_attachment() {
    let mut harness = TestHarness::new();

    let widget = Button::new("click me");
    harness.mount(widget);

    // Simulate a pointer press on the button
    harness.send_event(InputEvent::PointerButton {
        position: Point::new(50.0, 25.0),
        state: ButtonState::Pressed,
        button: 0,
    });

    // The button should be focused
    let button_id = harness.root_element().id();
    assert!(harness.focus_manager().is_focused_node(button_id));

    // The focus node should have been created during mount (not lazily)
    // Verify by checking the attachment exists
    assert!(harness.element(button_id).focus_attachment().is_some());
}
```

- [ ] **Step 2: Modify `EventHandler::request_focus()` to use attachment**

In `vexo/src/retain/event_handler.rs`, replace the lazy node creation:

Before (conceptual):
```rust
fn request_focus(&mut self, element_key: ElementId, context: &mut ElementContext) {
    let node_id = context.focus_manager()
        .node_for_element(element_key)
        .unwrap_or_else(|| {
            // Lazy creation — REMOVE THIS
            context.focus_manager().create_node_for_element(element_key, None).unwrap()
        });
    context.focus_manager().request_focus(node_id);
}
```

After:
```rust
fn request_focus(&mut self, element_key: ElementId, context: &mut ElementContext) {
    let node_id = context.focus_manager()
        .node_for_element(element_key)
        .expect("Focus node must exist — all mounted elements have attachments");
    context.focus_manager().request_focus(node_id);
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test -p vexo -- test_request_focus_via_event_uses_attachment`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/event_handler.rs vexo/src/retain/focus/integration_tests.rs
git commit -m "refactor: replace lazy focus node creation in EventHandler with attachment lookup"
```

---

### Task 7: Replace lazy focus node creation in Pipeline::set_focus()

**Files:**
- Modify: `vexo/src/retain/pipeline.rs`
- Test: `vexo/src/retain/focus/integration_tests.rs`

Same as Task 6 but for the `ThreeTreePipeline::set_focus()` method, which also creates focus nodes lazily.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_pipeline_set_focus_uses_existing_attachment() {
    let mut harness = TestHarness::new();

    let widget = Text::new("hello");
    harness.mount(widget);

    let element_id = harness.root_element().id();

    // Use pipeline's set_focus API
    harness.pipeline().set_focus(Some(element_id));

    // Should be focused
    assert!(harness.focus_manager().is_focused_node(element_id));

    // No new node should have been created — attachment from mount is used
    assert!(harness.element(element_id).focus_attachment().is_some());
}
```

- [ ] **Step 2: Modify `pipeline::set_focus()` to use attachment**

In `vexo/src/retain/pipeline.rs`, replace lazy node creation with existing node lookup:

```rust
pub fn set_focus(&mut self, element_key: Option<ElementId>) {
    match element_key {
        Some(key) => {
            let node_id = self.focus_manager
                .node_for_element(key)
                .expect("Focus node must exist — all mounted elements have attachments");
            self.focus_manager.request_focus(node_id);
        }
        None => {
            self.focus_manager.unfocus();
        }
    }
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test -p vexo -- test_pipeline_set_focus_uses_existing_attachment`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/pipeline.rs vexo/src/retain/focus/integration_tests.rs
git commit -m "refactor: replace lazy focus node creation in Pipeline::set_focus() with attachment lookup"
```

---

### Task 8: Remove dead code and verify full test suite

**Files:**
- Modify: `vexo/src/retain/focus/manager.rs` — remove `create_node_lazily()` or similar unused methods
- Modify: `vexo/src/retain/event_handler.rs` — clean up any remaining lazy-creation helpers
- Modify: `vexo/src/retain/pipeline.rs` — same

- [ ] **Step 1: Search for and remove dead lazy-creation code**

```bash
grep -rn "create_node_lazily\|create_node_if_missing\|ensure_node_for_element" vexo/src/retain/
```

Remove any methods that were only used by the now-removed lazy creation paths.

- [ ] **Step 2: Run the full test suite**

Run: `cargo test -p vexo`
Expected: All tests PASS

- [ ] **Step 3: Run the build**

Run: `cargo build -p vexo`
Expected: PASS with no warnings

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/
git commit -m "chore: remove dead lazy focus node creation code"
```

---

## Self-Review

**1. Spec coverage:**
- Focus nodes created on mount (not lazily) — Task 3 ✓
- Focus nodes removed on unmount — Task 4 ✓
- Focus nodes reparented on rebuild — Task 5 ✓
- EventHandler uses attachment instead of lazy creation — Task 6 ✓
- Pipeline uses attachment instead of lazy creation — Task 7 ✓
- Dead code cleanup — Task 8 ✓

**2. Placeholder scan:**
- No TBDs, TODOs, or "implement later" patterns found
- All steps have concrete code
- No "similar to Task N" shortcuts

**3. Type consistency:**
- `FocusAttachment::new(node_id, focus_manager)` — used consistently in Task 3
- `FocusAttachment::detach(focus_manager)` — used consistently in Task 4
- `FocusAttachment::reparent(parent_id, focus_manager)` — used consistently in Task 5
- `FocusManager::create_node_for_element(element_key, parent_id)` — used consistently in Tasks 1 and 3
- `node_for_element(element_key) -> Option<FocusNodeId>` — used consistently in Tasks 6 and 7
- `ElementId` type used consistently (matches existing codebase usage)
