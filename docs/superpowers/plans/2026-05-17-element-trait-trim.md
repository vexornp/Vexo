# Element Trait Trim Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove 3 dead methods (`add_child`, `has_children`, `visit_children`) from the `Element` trait and all their implementations.

**Architecture:** Pure deletion — remove trait method declarations and their impls across 10 files. No new code, no structural changes.

**Tech Stack:** Rust, cargo test

---

### Task 1: Remove dead methods from the Element trait definition

**Files:**
- Modify: `vexo/src/retain/element.rs:50-51` (remove `add_child`)
- Modify: `vexo/src/retain/element.rs:62-65` (remove `has_children`)
- Modify: `vexo/src/retain/element.rs:29-30` (remove `visit_children`)
- Modify: `vexo/src/retain/element.rs:69-72` (add TODO comment on `child_mounted`)

- [ ] **Step 1: Remove `visit_children` from the Element trait**

In `vexo/src/retain/element.rs`, delete lines 29-30:

```rust
    /// Visit children for traversal.
    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element));
```

- [ ] **Step 2: Remove `add_child` from the Element trait**

In `vexo/src/retain/element.rs`, delete lines 50-51:

```rust
    /// Add a child element key.
    fn add_child(&mut self, _child_key: ElementKey) {}
```

- [ ] **Step 3: Remove `has_children` from the Element trait**

In `vexo/src/retain/element.rs`, delete lines 62-65:

```rust
    /// Check if this element has children that need reconciliation.
    fn has_children(&self) -> bool {
        false
    }
```

- [ ] **Step 4: Add TODO comment on `child_mounted`**

In `vexo/src/retain/element.rs`, replace the `child_mounted` doc comment (lines 67-71) with:

```rust
    /// Called by the pipeline after a ChildOp::Inflate is executed,
    /// notifying the parent of the new child's key and render object.
    /// Elements that track children internally should override this.
    /// The `child_ro` parameter is the child's render object key (if any),
    /// used for linking the render object tree.
    ///
    /// TODO: This overlaps with rebuild() — elements that override rebuild()
    /// already manage their children there. Consider removing in a future pass.
```

- [ ] **Step 5: Verify compilation fails (expected)**

Run: `cargo build -p vexo 2>&1 | head -30`
Expected: Compilation errors — all impls of the removed methods now have orphaned fn definitions.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/element.rs
git commit -m "refactor: remove add_child, has_children, visit_children from Element trait"
```

---

### Task 2: Remove dead method impls from LeafRenderObjectElement

**Files:**
- Modify: `vexo/src/retain/elements/leaf.rs:129-131` (remove `visit_children` impl)
- Modify: `vexo/src/retain/elements/leaf.rs:145-153` (remove `on_event` impl — wait, keep this, it's not dead)
- Modify: `vexo/src/retain/elements/leaf.rs:299-312` (remove `test_leaf_element_no_children` test)

- [ ] **Step 1: Remove `visit_children` impl from LeafRenderObjectElement**

In `vexo/src/retain/elements/leaf.rs`, delete lines 129-131:

```rust
    fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {
        // Leaf elements have no children
    }
```

- [ ] **Step 2: Remove `test_leaf_element_no_children` test**

In `vexo/src/retain/elements/leaf.rs`, delete lines 299-312 (the entire test):

```rust
    #[test]
    fn test_leaf_element_no_children() {
        use crate::retain::element::ElementRegistry;

        let element = LeafRenderObjectElement::new();
        let registry = ElementRegistry::new();
        let mut count = 0;

        element.visit_children(&registry, &mut |_child| {
            count += 1;
        });

        assert_eq!(count, 0);
    }
```

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/elements/leaf.rs
git commit -m "refactor: remove dead Element method impls from LeafRenderObjectElement"
```

---

### Task 3: Remove dead method impls from ContainerElement

**Files:**
- Modify: `vexo/src/retain/elements/container.rs:160-166` (remove `visit_children` impl)
- Modify: `vexo/src/retain/elements/container.rs:190-192` (remove `add_child` impl)
- Modify: `vexo/src/retain/elements/container.rs:250-252` (remove `has_children` impl)

- [ ] **Step 1: Remove `visit_children` impl from ContainerElement**

In `vexo/src/retain/elements/container.rs`, delete lines 160-166:

```rust
    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        for &child_id in &self.children {
            if let Some(child) = registry.get(child_id) {
                visitor(child);
            }
        }
    }
```

- [ ] **Step 2: Remove `add_child` impl from ContainerElement**

In `vexo/src/retain/elements/container.rs`, delete lines 190-192:

```rust
    fn add_child(&mut self, child_id: ElementKey) {
        self.children.push(child_id);
    }
```

- [ ] **Step 3: Remove `has_children` impl from ContainerElement**

In `vexo/src/retain/elements/container.rs`, delete lines 250-252:

```rust
    fn has_children(&self) -> bool {
        true
    }
```

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/elements/container.rs
git commit -m "refactor: remove dead Element method impls from ContainerElement"
```

---

### Task 4: Remove dead method impls from DecoratedContainerElement

**Files:**
- Modify: `vexo/src/retain/widgets/decorated_container.rs:312-318` (remove `visit_children` impl)
- Modify: `vexo/src/retain/widgets/decorated_container.rs:341-343` (remove `add_child` impl)
- Modify: `vexo/src/retain/widgets/decorated_container.rs:389-391` (remove `has_children` impl)

- [ ] **Step 1: Remove `visit_children` impl from DecoratedContainerElement**

In `vexo/src/retain/widgets/decorated_container.rs`, delete lines 312-318:

```rust
    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        if let Some(child_id) = self.child_element {
            if let Some(child) = registry.get(child_id) {
                visitor(child);
            }
        }
    }
```

- [ ] **Step 2: Remove `add_child` impl from DecoratedContainerElement**

In `vexo/src/retain/widgets/decorated_container.rs`, delete lines 341-343:

```rust
    fn add_child(&mut self, child_id: ElementKey) {
        self.child_element = Some(child_id);
    }
```

- [ ] **Step 3: Remove `has_children` impl from DecoratedContainerElement**

In `vexo/src/retain/widgets/decorated_container.rs`, delete lines 389-391:

```rust
    fn has_children(&self) -> bool {
        self.child_element.is_some()
    }
```

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/widgets/decorated_container.rs
git commit -m "refactor: remove dead Element method impls from DecoratedContainerElement"
```

---

### Task 5: Remove dead method impls from GestureDetectorElement

**Files:**
- Modify: `vexo/src/retain/widgets/gesture_detector.rs:273-279` (remove `visit_children` impl)
- Modify: `vexo/src/retain/widgets/gesture_detector.rs:319-321` (remove `add_child` impl)
- Modify: `vexo/src/retain/widgets/gesture_detector.rs:357-359` (remove `has_children` impl)

- [ ] **Step 1: Remove `visit_children` impl from GestureDetectorElement**

In `vexo/src/retain/widgets/gesture_detector.rs`, delete lines 273-279:

```rust
    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        if let Some(child_id) = self.child_element {
            if let Some(child) = registry.get(child_id) {
                visitor(child);
            }
        }
    }
```

- [ ] **Step 2: Remove `add_child` impl from GestureDetectorElement**

In `vexo/src/retain/widgets/gesture_detector.rs`, delete lines 319-321:

```rust
    fn add_child(&mut self, child_id: ElementKey) {
        self.child_element = Some(child_id);
    }
```

- [ ] **Step 3: Remove `has_children` impl from GestureDetectorElement**

In `vexo/src/retain/widgets/gesture_detector.rs`, delete lines 357-359:

```rust
    fn has_children(&self) -> bool {
        self.child_element.is_some()
    }
```

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/widgets/gesture_detector.rs
git commit -m "refactor: remove dead Element method impls from GestureDetectorElement"
```

---

### Task 6: Remove dead method impls from StatefulElement

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs:429-435` (remove `visit_children` impl)
- Modify: `vexo/src/retain/stateful_widget.rs:449-451` (remove `has_children` impl)

- [ ] **Step 1: Remove `visit_children` impl from StatefulElement**

In `vexo/src/retain/stateful_widget.rs`, delete lines 429-435:

```rust
    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        if let Some(child_id) = self.child_element_id {
            if let Some(child) = registry.get(child_id) {
                visitor(child);
            }
        }
    }
```

- [ ] **Step 2: Remove `has_children` impl from StatefulElement**

In `vexo/src/retain/stateful_widget.rs`, delete lines 449-451:

```rust
    fn has_children(&self) -> bool {
        self.child_element_id.is_some()
    }
```

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "refactor: remove dead Element method impls from StatefulElement"
```

---

### Task 7: Remove dead method impls from test mock elements

**Files:**
- Modify: `vexo/src/retain/element_registry_tests.rs:12` (remove `visit_children` from MockElement)
- Modify: `vexo/src/retain/reconcile_tests.rs:51` (remove `visit_children` from MockElement)
- Modify: `vexo/src/retain/reconcile.rs:161` (remove `visit_children` from MockElement in test module)
- Modify: `vexo/src/retain/widgets/mod.rs:200` (remove `visit_children` from TestElement in test module)

- [ ] **Step 1: Remove `visit_children` from MockElement in element_registry_tests.rs**

In `vexo/src/retain/element_registry_tests.rs`, delete line 12:

```rust
    fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {}
```

- [ ] **Step 2: Remove `visit_children` from MockElement in reconcile_tests.rs**

In `vexo/src/retain/reconcile_tests.rs`, delete line 51:

```rust
    fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {}
```

- [ ] **Step 3: Remove `visit_children` from MockElement in reconcile.rs test module**

In `vexo/src/retain/reconcile.rs`, delete line 161:

```rust
        fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {}
```

- [ ] **Step 4: Remove `visit_children` from TestElement in widgets/mod.rs test module**

In `vexo/src/retain/widgets/mod.rs`, delete line 200:

```rust
        fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {}
```

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/element_registry_tests.rs vexo/src/retain/reconcile_tests.rs vexo/src/retain/reconcile.rs vexo/src/retain/widgets/mod.rs
git commit -m "refactor: remove dead Element method impls from test mock elements"
```

---

### Task 8: Build and test

- [ ] **Step 1: Build the project**

Run: `cargo build -p vexo`
Expected: Clean build with no errors.

- [ ] **Step 2: Run tests**

Run: `cargo test -p vexo`
Expected: All tests pass.

- [ ] **Step 3: Commit (if any fixes were needed)**

Only if Step 1 or Step 2 required fixes:

```bash
git add -u
git commit -m "fix: resolve compilation/test issues from Element trait trim"
```
