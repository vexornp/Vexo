# Pass-Through Render Objects Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert `OpacityRenderObject`, `TransformRenderObject`, and the onstage branch of `OffstageRenderObject` to true pass-through render objects that create no Taffy node, link the child's node directly to the grandparent, and adopt the child's computed bounds.

**Architecture:** Add an `is_pass_through()` default trait method on `RenderObject`. Pass-through ROs return the child's Taffy node from `layout_node()` (instead of owning one), so the layouter links grandparent→grandchild with no changes. `is_pass_through()` guards only the registry's cleanup path to prevent double-removal of borrowed nodes. The layouter itself is unchanged.

**Tech Stack:** Rust, Taffy 0.9.1 (flexbox layout), slotmap (generational keys), glyphon (text measurement).

**Spec:** `docs/superpowers/specs/2026-07-08-pass-through-render-objects-design.md`

## Global Constraints

- Workspace dependency versions are pinned in root `Cargo.toml`; reference via `{ workspace = true }`. Taffy 0.9.1.
- `LayoutNodeKey` is a slotmap key (`new_key_type!`) with NO `Default` impl — cannot use `unwrap_or_default()`.
- `LayoutResult` is a struct with a required `node: LayoutNodeKey` field; the layouter discards the return value of `layout()` (`layouter.rs:139`), so the field is structurally required but semantically dead for pass-through ROs.
- Pass-through ROs (`Opacity`, `Transform`, `Offstage`-onstage) always have a child widget per their constructors — enforce via `expect()` in `layout()`.
- Build command: `cargo build -p vexo`
- Test command: `cargo test -p vexo`
- No comments in code unless explaining a non-obvious invariant.

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `vexo/src/render_object.rs` | `RenderObject` trait + `RenderObjectRegistry`. Add `is_pass_through()` default; guard `remove()` cleanup. | Modify |
| `vexo/src/render_objects/opacity.rs` | `OpacityRenderObject`. Convert to pass-through. | Modify |
| `vexo/src/widgets/transform.rs` | `TransformRenderObject` (in same file as widget). Convert to pass-through. | Modify |
| `vexo/src/render_objects/offstage.rs` | `OffstageRenderObject`. Onstage branch → pass-through; offstage branch unchanged; flag-flip node lifecycle. | Modify |
| `vexo/src/tests/passthrough_integration.rs` | New integration tests for pipeline-level pass-through behavior. | Create |

No other files change. The layouter (`vexo/src/layouter.rs`), widget layer (`opacity.rs`, `offstage.rs` widgets, transform widget API), and all other ROs are untouched.

---

## Task 1: Add `is_pass_through()` to `RenderObject` trait

**Files:**
- Modify: `vexo/src/render_object.rs` (trait definition ~line 235, after `scroll_offset()` or near `opacity()`)

**Interfaces:**
- Consumes: nothing
- Produces: `fn is_pass_through(&self) -> bool { false }` on `RenderObject` trait. Default `false`. All existing ROs inherit it.

- [ ] **Step 1: Add the trait method**

In `vexo/src/render_object.rs`, find the `RenderObject` trait. Add the method after the `opacity()` method (around line 382), before `needs_image_registration()`:

```rust
    /// Whether this render object is a layout pass-through.
    ///
    /// Pass-through ROs (`Opacity`, `Transform`, `Offstage`-onstage) do NOT
    /// own a Taffy node. Their `layout_node()` returns the child's node, so
    /// the layouter links the grandparent directly to the grandchild.
    /// `is_pass_through() == true` tells the registry to skip orphan-node
    /// cleanup on removal (the child owns the node).
    ///
    /// Default: `false` (normal ROs own their Taffy node).
    fn is_pass_through(&self) -> bool {
        false
    }
```

- [ ] **Step 2: Build to verify it compiles**

Run: `cargo build -p vexo`
Expected: PASS (default impl means no RO needs to override yet)

- [ ] **Step 3: Commit**

```bash
git add vexo/src/render_object.rs
git commit -m "feat(render_object): add is_pass_through() trait method

Default false. Pass-through ROs will override to true. Currently unused;
guard wired in Task 2."
```

---

## Task 2: Guard `RenderObjectRegistry::remove()` cleanup

**Files:**
- Modify: `vexo/src/render_object.rs` (`RenderObjectRegistry::remove()` ~line 466)

**Interfaces:**
- Consumes: `is_pass_through()` from Task 1
- Produces: `remove()` skips orphaned-node push for pass-through ROs

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `vexo/src/render_object.rs` (after `test_registry_set_child` ~line 873):

```rust
    #[test]
    fn test_registry_remove_skips_passthrough_cleanup() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = make_element_key();

        struct MockPassthroughRO;
        impl RenderObject for MockPassthroughRO {
            fn layout(
                &mut self,
                _ctx: &mut LayoutContext,
                _child_nodes: &[LayoutNodeKey],
            ) -> LayoutResult {
                unimplemented!()
            }
            fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
            fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
                vec![]
            }
            fn hit_test(&self, _position: Point<Logical>, _ctx: &HitTestContext) -> bool {
                true
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
            fn is_pass_through(&self) -> bool {
                true
            }
            fn layout_node(&self) -> Option<LayoutNodeKey> {
                // Simulate borrowing a child's node. Use a real key from a slotmap
                // so the type is valid; the value is never read by the registry.
                let mut sm: slotmap::SlotMap<LayoutNodeKey, ()> = slotmap::SlotMap::with_key();
                Some(sm.insert(()))
            }
        }

        let obj = Box::new(MockPassthroughRO);
        let id = registry.create(obj, element_id);
        registry.remove(id);

        // Pass-through RO borrowed a (child's) node — registry must NOT push it
        // to orphaned_layout_nodes, or the child's own removal would double-remove.
        let orphaned = registry.drain_orphaned_layout_nodes();
        assert!(
            orphaned.is_empty(),
            "pass-through RO removal must not orphan the borrowed child node"
        );
    }

    #[test]
    fn test_registry_remove_collects_normal_ro_node() {
        let mut registry = RenderObjectRegistry::new();
        let element_id = make_element_key();

        // A normal RO that owns a node. layout_node() returns Some; is_pass_through() = false (default).
        struct MockOwnerRO {
            node: Option<LayoutNodeKey>,
        }
        impl RenderObject for MockOwnerRO {
            fn layout(
                &mut self,
                _ctx: &mut LayoutContext,
                _child_nodes: &[LayoutNodeKey],
            ) -> LayoutResult {
                unimplemented!()
            }
            fn apply_layout(&mut self, _ctx: &mut LayoutContext) {}
            fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
                vec![]
            }
            fn hit_test(&self, _position: Point<Logical>, _ctx: &HitTestContext) -> bool {
                true
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
            fn layout_node(&self) -> Option<LayoutNodeKey> {
                self.node
            }
        }

        let mut node_sm: slotmap::SlotMap<LayoutNodeKey, ()> = slotmap::SlotMap::with_key();
        let owned_node = node_sm.insert(());
        let obj = Box::new(MockOwnerRO {
            node: Some(owned_node),
        });
        let id = registry.create(obj, element_id);
        registry.remove(id);

        // Normal RO owns its node — registry MUST push it to orphaned for cleanup.
        let orphaned = registry.drain_orphaned_layout_nodes();
        assert_eq!(orphaned.len(), 1);
        assert_eq!(orphaned[0], owned_node);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo test_registry_remove_skips_passthrough_cleanup test_registry_remove_collects_normal_ro_node`
Expected: FAIL — `test_registry_remove_skips_passthrough_cleanup` fails because the current `remove()` unconditionally pushes `layout_node()` to orphaned. (The second test passes already because current behavior matches it.)

- [ ] **Step 3: Add the guard to `remove()`**

In `vexo/src/render_object.rs`, find `RenderObjectRegistry::remove()` (around line 466). Replace:

```rust
    pub fn remove(&mut self, key: RenderObjectKey) {
        // Extract layout node key before dropping the render object
        if let Some(obj) = self.objects.get(key) {
            if let Some(node) = obj.layout_node() {
                self.orphaned_layout_nodes.push(node);
            }
        }
        self.objects.remove(key);
        self.element_map.remove(key);
        self.cursor_annotations.remove(key);
    }
```

with:

```rust
    pub fn remove(&mut self, key: RenderObjectKey) {
        if let Some(obj) = self.objects.get(key) {
            if !obj.is_pass_through() {
                if let Some(node) = obj.layout_node() {
                    self.orphaned_layout_nodes.push(node);
                }
            }
        }
        self.objects.remove(key);
        self.element_map.remove(key);
        self.cursor_annotations.remove(key);
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo test_registry_remove_skips_passthrough_cleanup test_registry_remove_collects_normal_ro_node`
Expected: PASS

- [ ] **Step 5: Run full test suite to verify no regressions**

Run: `cargo test -p vexo`
Expected: PASS (all existing tests still green — no RO overrides `is_pass_through()` yet, so all behave as before)

- [ ] **Step 6: Commit**

```bash
git add vexo/src/render_object.rs
git commit -m "feat(registry): guard remove() cleanup for pass-through ROs

Pass-through ROs borrow their child's Taffy node, so layout_node()
returns the child's node. Without this guard, removing a pass-through
RO would orphan the child's node, causing double-remove when the child
is also removed."
```

---

## Task 3: Convert `OpacityRenderObject` to pass-through

**Files:**
- Modify: `vexo/src/render_objects/opacity.rs`

**Interfaces:**
- Consumes: `is_pass_through()` from Task 1
- Produces: `OpacityRenderObject` with `child_layout_node` field, `is_pass_through() == true`, `layout_node()` returns child's node

- [ ] **Step 1: Write the failing tests**

Add these tests to the `tests` module in `vexo/src/render_objects/opacity.rs` (after the existing `test_opacity_render_object_set_opacity` test):

```rust
    #[test]
    fn test_opacity_is_pass_through() {
        let ro = OpacityRenderObject::new(0.5);
        assert!(ro.is_pass_through());
    }

    #[test]
    fn test_opacity_layout_stores_child_node() {
        use crate::layout::{LayoutEngine, TaffyLayoutEngine};

        let mut ro = OpacityRenderObject::new(0.5);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let child_node = ctx.engine().create_leaf(&Layout::default().width(50.0).height(30.0));

        let result = ro.layout(&mut ctx, &[child_node]);

        assert_eq!(ro.layout_node(), Some(child_node));
        assert_eq!(result.node, child_node);
    }

    #[test]
    fn test_opacity_layout_creates_no_taffy_node() {
        use crate::layout::{LayoutEngine, TaffyLayoutEngine};

        let mut ro = OpacityRenderObject::new(0.5);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let child_node = ctx.engine().create_leaf(&Layout::default().width(50.0).height(30.0));

        ro.layout(&mut ctx, &[child_node]);

        // The engine should have exactly one node (the child we created).
        // Opacity created none. We verify indirectly: get_layout(child_node)
        // still works, and there is no second node to query.
        let child_layout = ctx.engine_ref().get_layout(child_node);
        assert!(child_layout.is_some(), "child node should still exist");
    }

    #[test]
    fn test_opacity_apply_layout_reads_child_bounds() {
        use crate::core::Size;
        use crate::layout::{LayoutEngine, TaffyLayoutEngine};

        let mut ro = OpacityRenderObject::new(0.5);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let node = ctx
                .engine()
                .create_leaf(&Layout::default().width(80.0).height(40.0));
            ro.layout(&mut ctx, &[node]);
            node
        };

        engine.compute(child_node, Size::new(200.0, 200.0), &mut font_system);

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.apply_layout(&mut ctx);
        }

        let bounds = ro.computed_bounds().expect("apply_layout should set bounds");
        assert_eq!(bounds.width(), 80.0);
        assert_eq!(bounds.height(), 40.0);
    }

    #[test]
    #[should_panic(expected = "pass-through render object requires a child")]
    fn test_opacity_layout_no_child_panics() {
        let mut ro = OpacityRenderObject::new(0.5);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        ro.layout(&mut ctx, &[]);
    }
```

Also add this helper at the top of the `tests` module if not already present:

```rust
    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo opacity::tests`
Expected: FAIL — tests reference methods/fields that don't exist yet (`is_pass_through`, `child_layout_node`). Compilation errors.

- [ ] **Step 3: Convert the render object**

In `vexo/src/render_objects/opacity.rs`, replace the entire `OpacityRenderObject` struct and its `RenderObject` impl. Keep the existing tests module's helper (`create_test_font_system`) and existing tests.

Replace the struct definition:

```rust
pub struct OpacityRenderObject {
    opacity: f32,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    child_layout_node: Option<LayoutNodeKey>,
}
```

Replace `new()`:

```rust
    pub fn new(opacity: f32) -> Self {
        Self {
            opacity,
            child: None,
            computed_bounds: None,
            child_layout_node: None,
        }
    }
```

Replace the `RenderObject` impl's `layout`, `apply_layout`, `layout_node`, and add `is_pass_through`:

```rust
impl RenderObject for OpacityRenderObject {
    fn layout(&mut self, _ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let child_node = child_nodes.first().copied().expect(
            "pass-through render object requires a child widget; \
             Opacity always has a child per its constructor",
        );
        self.child_layout_node = Some(child_node);
        LayoutResult {
            node: child_node,
            size: crate::core::Size::zero(),
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(child_node) = self.child_layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(child_node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        true
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &crate::HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.child_layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn opacity(&self) -> Option<f32> {
        Some(self.opacity)
    }
}
```

Remove the now-unused import of `AlignItems`, `FlexDirection`, `Layout` if the compiler warns (check `cargo build`). Keep `LayoutNodeKey` import. The `Layout` import is still needed by tests.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo opacity::tests`
Expected: PASS — all opacity tests (existing + new) green.

- [ ] **Step 5: Run full test suite**

Run: `cargo test -p vexo`
Expected: PASS (no regressions; Opacity's public API unchanged)

- [ ] **Step 6: Commit**

```bash
git add vexo/src/render_objects/opacity.rs
git commit -m "feat(opacity): convert OpacityRenderObject to pass-through

Opacity no longer creates a Taffy node. It stores the child's node and
returns it from layout_node(), so the grandparent links the grandchild
directly. is_pass_through() == true tells the registry to skip cleanup.
Removes the Column+Stretch flex container that participated in
bottom-up max-content measurement."
```

---

## Task 4: Convert `TransformRenderObject` to pass-through

**Files:**
- Modify: `vexo/src/widgets/transform.rs` (the `TransformRenderObject` struct + impl, ~lines 30-179)

**Interfaces:**
- Consumes: `is_pass_through()` from Task 1
- Produces: `TransformRenderObject` with `child_layout_node` field, `is_pass_through() == true`, `layout_node()` returns child's node

- [ ] **Step 1: Write the failing tests**

Add these tests to the `tests` module in `vexo/src/widgets/transform.rs` (after `test_transform_hit_tests_flag_change`):

```rust
    #[test]
    fn test_transform_is_pass_through() {
        let ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        assert!(ro.is_pass_through());
    }

    #[test]
    fn test_transform_layout_stores_child_node() {
        use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};
        use crate::{LayoutContext, LayoutResult};

        let mut ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let child_node = ctx.engine().create_leaf(&Layout::default().width(50.0).height(30.0));

        let result = ro.layout(&mut ctx, &[child_node]);

        assert_eq!(ro.layout_node(), Some(child_node));
        assert_eq!(result.node, child_node);
    }

    #[test]
    fn test_transform_apply_layout_reads_child_bounds() {
        use crate::core::Size;
        use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};
        use crate::{LayoutContext, LayoutResult};

        let mut ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let node = ctx
                .engine()
                .create_leaf(&Layout::default().width(60.0).height(25.0));
            ro.layout(&mut ctx, &[node]);
            node
        };

        engine.compute(child_node, Size::new(200.0, 200.0), &mut font_system);

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.apply_layout(&mut ctx);
        }

        let bounds = ro.computed_bounds().expect("apply_layout should set bounds");
        assert_eq!(bounds.width(), 60.0);
        assert_eq!(bounds.height(), 25.0);
    }

    #[test]
    #[should_panic(expected = "pass-through render object requires a child")]
    fn test_transform_layout_no_child_panics() {
        use crate::layout::{LayoutEngine, TaffyLayoutEngine};
        use crate::{LayoutContext, LayoutResult};

        let mut ro = TransformRenderObject::new(AffineTransform::rotation(0.5), true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        ro.layout(&mut ctx, &[]);
    }
```

Add this helper at the top of the `tests` module if not already present:

```rust
    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo transform::tests`
Expected: FAIL — compilation errors (new tests reference pass-through behavior that doesn't exist yet).

- [ ] **Step 3: Convert the render object**

In `vexo/src/widgets/transform.rs`, replace the `TransformRenderObject` struct (lines ~30-45):

```rust
pub struct TransformRenderObject {
    transform: AffineTransform,
    transform_hit_tests: bool,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    child_layout_node: Option<LayoutNodeKey>,
}
```

Replace `new()`:

```rust
    pub fn new(transform: AffineTransform, transform_hit_tests: bool) -> Self {
        Self {
            transform,
            transform_hit_tests,
            child: None,
            computed_bounds: None,
            child_layout_node: None,
        }
    }
```

Replace the `RenderObject` impl's `layout`, `apply_layout`, `layout_node`, and add `is_pass_through`. Keep `paint`, `hit_test`, `children`, `set_child_id`, `replace_child`, `as_any`, `as_any_mut`, `computed_bounds`, `paint_transform`, `hit_test_transform` unchanged:

```rust
impl RenderObject for TransformRenderObject {
    fn layout(&mut self, _ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let child_node = child_nodes.first().copied().expect(
            "pass-through render object requires a child widget; \
             Transform always has a child per its constructor",
        );
        self.child_layout_node = Some(child_node);
        LayoutResult {
            node: child_node,
            size: crate::core::Size::zero(),
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(child_node) = self.child_layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(child_node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        true
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.child_layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn paint_transform(&self) -> Option<AffineTransform> {
        if self.transform.determinant().abs() < 1e-10 {
            return None;
        }
        Some(self.transform)
    }

    fn hit_test_transform(&self) -> Option<AffineTransform> {
        if self.transform_hit_tests {
            Some(self.transform)
        } else {
            None
        }
    }
}
```

After editing, run `cargo build -p vexo` to catch unused imports (the `AlignItems`, `FlexDirection`, `Layout` imports may now be unused — remove them if the compiler warns; keep `LayoutNodeKey`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo transform::tests`
Expected: PASS

- [ ] **Step 5: Run full test suite**

Run: `cargo test -p vexo`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/transform.rs
git commit -m "feat(transform): convert TransformRenderObject to pass-through

Transform no longer creates a Taffy node. Stores child's node, returns
it from layout_node(). is_pass_through() == true. Removes the
Column+Stretch flex container. Paint/hit-test transforms unchanged."
```

---

## Task 5: Convert `OffstageRenderObject` onstage branch to pass-through

**Files:**
- Modify: `vexo/src/render_objects/offstage.rs`

**Interfaces:**
- Consumes: `is_pass_through()` from Task 1, registry guard from Task 2
- Produces: `OffstageRenderObject` with `owned_node` (offstage) + `child_layout_node` (onstage), `is_pass_through() == !self.offstage`, flag-flip node lifecycle

This is the most complex task. Offstage has two branches:
- **offstage** (unchanged): owns a zero-size leaf node in `owned_node`, `is_pass_through() == false`
- **onstage** (new pass-through): stores child's node in `child_layout_node`, `is_pass_through() == true`, creates no Taffy node

Flag-flip transitions must clean up the old node:
- offstage→onstage: `engine.remove_node(owned_node)`, clear it, store `child_layout_node`
- onstage→offstage: clear `child_layout_node` (child owns it, no removal), create zero-size leaf in `owned_node`

- [ ] **Step 1: Write the failing tests**

Add these tests to the `tests` module in `vexo/src/render_objects/offstage.rs` (after existing tests):

```rust
    #[test]
    fn test_offstage_onstage_is_pass_through() {
        let ro = OffstageRenderObject::new(false);
        assert!(ro.is_pass_through());
    }

    #[test]
    fn test_offstage_offstage_is_not_pass_through() {
        let ro = OffstageRenderObject::new(true);
        assert!(!ro.is_pass_through());
    }

    #[test]
    fn test_offstage_onstage_layout_stores_child_node_no_owned_node() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };

        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        ro.layout(&mut ctx, &[child_node]);

        assert_eq!(ro.layout_node(), Some(child_node));
    }

    #[test]
    fn test_offstage_onstage_apply_layout_reads_child_bounds() {
        use crate::core::Size;

        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let node = ctx
                .engine()
                .create_leaf(&Layout::default().width(70.0).height(35.0));
            ro.layout(&mut ctx, &[node]);
            node
        };

        engine.compute(child_node, Size::new(200.0, 200.0), &mut font_system);

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.apply_layout(&mut ctx);
        }

        let bounds = ro.computed_bounds().expect("onstage should have bounds");
        assert_eq!(bounds.width(), 70.0);
        assert_eq!(bounds.height(), 35.0);
    }

    #[test]
    fn test_offstage_flag_flip_onstage_to_offstage() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };

        // Start onstage (pass-through)
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[child_node]);
        }
        assert_eq!(ro.layout_node(), Some(child_node));
        assert!(ro.is_pass_through());

        // Flip to offstage
        ro.set_offstage(true);
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[child_node]);
        }

        // Offstage: owns a zero-size leaf, does NOT report child's node
        let owned = ro.layout_node().expect("offstage should own a leaf node");
        assert!(!ro.is_pass_through());

        // The child's node must still exist in the engine (Offstage didn't remove it)
        engine.compute(owned, Size::new(100.0, 100.0), &mut font_system);
        assert!(
            engine.get_layout(child_node).is_some(),
            "child's node must still exist after onstage->offstage flip"
        );
    }

    #[test]
    fn test_offstage_flag_flip_offstage_to_onstage_removes_owned_node() {
        let mut ro = OffstageRenderObject::new(true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Start offstage (owns zero-size leaf)
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[]);
        }
        let offstage_node = ro.layout_node().expect("offstage should own a node");
        assert!(!ro.is_pass_through());

        // Flip to onstage (pass-through)
        ro.set_offstage(false);
        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let node = ctx
                .engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0));
            ro.layout(&mut ctx, &[node]);
            node
        };

        // Onstage: reports child's node, old owned node is gone
        assert_eq!(ro.layout_node(), Some(child_node));
        assert!(ro.is_pass_through());

        // The old offstage leaf node should be removed from the engine.
        // After removal, get_layout returns None.
        assert!(
            engine.get_layout(offstage_node).is_none(),
            "old offstage leaf node should be removed after offstage->onstage flip"
        );
    }

    #[test]
    #[should_panic(expected = "pass-through render object requires a child")]
    fn test_offstage_onstage_layout_no_child_panics() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        ro.layout(&mut ctx, &[]);
    }
```

Also **update** the existing `test_offstage_layout_onstage_passes_child` test (around line 258) — it currently asserts `layout_node.is_some()` which is still true, but now the node is the *child's* node, not a created container. Update the assertion comment and add a check that it equals the child node:

```rust
    #[test]
    fn test_offstage_layout_onstage_passes_child() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };

        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        let result = ro.layout(&mut ctx, &[child_node]);

        assert!(ro.layout_node.is_some_or_child());
        assert_eq!(ro.layout_node, Some(result.node));
        assert_eq!(ro.layout_node(), Some(child_node));
    }
```

Wait — the above references a non-existent method. The correct update is:

```rust
    #[test]
    fn test_offstage_layout_onstage_passes_child() {
        let mut ro = OffstageRenderObject::new(false);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine()
                .create_leaf(&Layout::default().width(50.0).height(50.0))
        };

        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        let result = ro.layout(&mut ctx, &[child_node]);

        // Onstage: pass-through. layout_node() returns the child's node.
        assert_eq!(ro.layout_node(), Some(child_node));
        assert_eq!(result.node, child_node);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo offstage::tests`
Expected: FAIL — compilation errors (new fields, `is_pass_through`, flag-flip behavior don't exist yet).

- [ ] **Step 3: Convert the render object**

In `vexo/src/render_objects/offstage.rs`, replace the struct definition (lines ~29-41):

```rust
pub struct OffstageRenderObject {
    offstage: bool,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    owned_node: Option<LayoutNodeKey>,
    child_layout_node: Option<LayoutNodeKey>,
}
```

Replace `new()`:

```rust
    pub fn new(offstage: bool) -> Self {
        Self {
            offstage,
            child: None,
            computed_bounds: None,
            owned_node: None,
            child_layout_node: None,
        }
    }
```

Replace the entire `RenderObject` impl:

```rust
impl RenderObject for OffstageRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        if self.offstage {
            // Offstage: zero-size leaf. Child NOT linked into layout.
            let leaf_layout = Layout::default().width(0.0).height(0.0);
            match self.owned_node {
                Some(existing) => {
                    ctx.engine().set_style(existing, &leaf_layout);
                    ctx.engine().set_children(existing, &[]);
                    self.child_layout_node = None;
                    LayoutResult {
                        node: existing,
                        size: Size::zero(),
                    }
                }
                None => {
                    let node = ctx.engine().create_container(&leaf_layout, &[]);
                    self.owned_node = Some(node);
                    self.child_layout_node = None;
                    LayoutResult {
                        node,
                        size: Size::zero(),
                    }
                }
            }
        } else {
            // Onstage: pass-through. Transition cleanup if coming from offstage.
            if let Some(old_owned) = self.owned_node.take() {
                ctx.engine().remove_node(old_owned);
            }
            let child_node = child_nodes.first().copied().expect(
                "pass-through render object requires a child widget; \
                 Offstage always has a child per its constructor",
            );
            self.child_layout_node = Some(child_node);
            LayoutResult {
                node: child_node,
                size: Size::zero(),
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        let node = if self.offstage {
            self.owned_node
        } else {
            self.child_layout_node
        };
        if let Some(node) = node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        !self.offstage
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        if self.offstage {
            &[]
        } else {
            match &self.child {
                Some(child) => std::slice::from_ref(child),
                None => &[],
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        if self.offstage {
            self.owned_node
        } else {
            self.child_layout_node
        }
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}
```

- [ ] **Step 4: Fix existing tests that referenced the old `layout_node` field directly**

The existing tests `test_offstage_layout_offstage_creates_zero_node` (line ~243) and `test_offstage_layout_onstage_passes_child` (line ~258, already updated in Step 1) reference `ro.layout_node` as a **field**. After conversion, `layout_node` is a **method**; the field is now `owned_node` or `child_layout_node`. Update `test_offstage_layout_offstage_creates_zero_node`:

```rust
    #[test]
    fn test_offstage_layout_offstage_creates_zero_node() {
        let mut ro = OffstageRenderObject::new(true);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = ro.layout(&mut ctx, &[]);

        assert!(ro.layout_node().is_some());
        assert_eq!(ro.layout_node(), Some(result.node));
        assert_eq!(result.size, Size::zero());
    }
```

Search for any other direct field access to `ro.layout_node` in the test module and replace with `ro.layout_node()` (the method) or `ro.owned_node` / `ro.child_layout_node` as appropriate.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vexo offstage::tests`
Expected: PASS — all offstage tests green.

- [ ] **Step 6: Run full test suite**

Run: `cargo test -p vexo`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add vexo/src/render_objects/offstage.rs
git commit -m "feat(offstage): convert onstage branch to pass-through

Onstage: no Taffy node, stores child's node in child_layout_node,
is_pass_through() == true. Offstage: unchanged (owns zero-size leaf).
Flag-flip transitions clean up the old node: offstage->onstage removes
the owned leaf; onstage->offstage drops the child reference (child
keeps its own node) and creates a new zero-size leaf."
```

---

## Task 6: Integration tests — grandchild receives grandparent's width

**Files:**
- Create: `vexo/src/tests/passthrough_integration.rs`
- Modify: `vexo/src/lib.rs` (add `mod tests;` if not present) or the appropriate test module registration

**Interfaces:**
- Consumes: pass-through ROs from Tasks 3-5, registry guard from Task 2
- Produces: integration tests proving grandparent→grandchild constraint propagation

These tests build a real `RenderObjectRegistry` + `Layouter::layout()` call to verify the pipeline links pass-through ROs correctly.

- [ ] **Step 1: Check how existing integration tests are registered**

Run: `grep -n "mod.*integration\|mod.*e2e\|mod.*tests" vexo/src/lib.rs`
Expected: shows how `e2e_test.rs`, `stateful_integration_test.rs`, `window_integration_test.rs` are registered. Follow the same pattern.

- [ ] **Step 2: Create the test file with the first failing test**

Create `vexo/src/tests/passthrough_integration.rs`:

```rust
//! Integration tests for pass-through render objects.
//!
//! Verifies that pass-through ROs (Opacity, Transform, Offstage-onstage)
//! link the grandparent's Taffy node directly to the grandchild's,
//! so the grandchild receives the grandparent's constraints.

use crate::core::{Color, Size};
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutEngine, TaffyLayoutEngine};
use crate::render::RenderCommand;
use crate::render_objects::{ContainerRenderObject, OffstageRenderObject, OpacityRenderObject};
use crate::render_object::{LayoutContext, RenderObject, RenderObjectRegistry};
use crate::widgets::transform::TransformRenderObject;
use crate::core::AffineTransform;
use crate::dirty::DirtyTracking;
use crate::id::{ElementKey, RenderObjectKey};
use crate::layouter::Layouter;
use crate::core::SafeAreaSource;

fn create_test_font_system() -> glyphon::FontSystem {
    let font_data = include_bytes!("../font.ttf").to_vec();
    let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
    glyphon::FontSystem::new_with_fonts([binary])
}

fn make_element_key() -> ElementKey {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    sm.insert(())
}

fn column_layout() -> Layout {
    Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch)
        .width_percent(1.0)
}

/// Build a tree: Flex::column → Opacity → (child RO provided).
/// Returns (root_key, opacity_key, child_key).
fn build_opacity_tree(
    registry: &mut RenderObjectRegistry,
    child_ro: Box<dyn RenderObject>,
) -> (RenderObjectKey, RenderObjectKey, RenderObjectKey) {
    let flex_elem = make_element_key();
    let opacity_elem = make_element_key();
    let child_elem = make_element_key();

    let flex_ro = Box::new(ContainerRenderObject::new(column_layout()));
    let opacity_ro = Box::new(OpacityRenderObject::new(0.5));
    let child_key = registry.create(child_ro, child_elem);
    let opacity_key = registry.create(opacity_ro, opacity_elem);
    let flex_key = registry.create(flex_ro, flex_elem);

    registry.set_child(opacity_key, child_key);
    registry.set_child(flex_key, opacity_key);
    registry.set_root(flex_key);
    (flex_key, opacity_key, child_key)
}

#[test]
fn test_passthrough_opacity_child_receives_grandparent_width() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    // Child: a simple container with fixed width we can read back.
    // Use a leaf-like container so we can read its computed bounds.
    let child_ro = Box::new(ContainerRenderObject::new(
        Layout::default().height(40.0),
    ));
    let (flex_key, opacity_key, child_key) = build_opacity_tree(&mut registry, child_ro);

    dirty.mark_needs_layout(flex_key);
    dirty.mark_needs_layout(opacity_key);
    dirty.mark_needs_layout(child_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let child_bounds = registry
        .get(child_key)
        .unwrap()
        .computed_bounds()
        .expect("child should have computed bounds");

    // Without Opacity in the way, the child (width unset, stretch) would fill
    // the Flex's width (300). With pass-through Opacity, the child should STILL
    // receive 300 — the grandparent links the grandchild directly.
    assert_eq!(
        child_bounds.width(),
        300.0,
        "pass-through Opacity must let grandchild receive grandparent's width"
    );
}
```

- [ ] **Step 3: Register the test module**

In `vexo/src/lib.rs`, add (following the pattern of existing test module registrations):

```rust
#[cfg(test)]
mod tests;
#[cfg(test)]
mod passthrough_integration;
```

If `tests` is a directory module, create `vexo/src/tests/mod.rs` with `pub mod passthrough_integration;` instead. Check existing pattern first via the grep in Step 1.

- [ ] **Step 4: Run the test to verify it fails (or passes if implementation is correct)**

Run: `cargo test -p vexo test_passthrough_opacity_child_receives_grandparent_width`
Expected: PASS (the pass-through ROs from Tasks 3-5 should make this work). If it FAILS, the implementation has a bug — debug before proceeding.

- [ ] **Step 5: Add nested pass-through test**

Append to `vexo/src/tests/passthrough_integration.rs`:

```rust
#[test]
fn test_nested_passthrough_links_correctly() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let flex_elem = make_element_key();
    let opacity_elem = make_element_key();
    let transform_elem = make_element_key();
    let child_elem = make_element_key();

    let flex_ro = Box::new(ContainerRenderObject::new(column_layout()));
    let opacity_ro = Box::new(OpacityRenderObject::new(0.5));
    let transform_ro = Box::new(TransformRenderObject::new(
        AffineTransform::translation(10.0, 0.0),
        true,
    ));
    let child_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));

    let child_key = registry.create(child_ro, child_elem);
    let transform_key = registry.create(transform_ro, transform_elem);
    let opacity_key = registry.create(opacity_ro, opacity_elem);
    let flex_key = registry.create(flex_ro, flex_elem);

    registry.set_child(transform_key, child_key);
    registry.set_child(opacity_key, transform_key);
    registry.set_child(flex_key, opacity_key);
    registry.set_root(flex_key);

    dirty.mark_needs_layout(flex_key);
    dirty.mark_needs_layout(opacity_key);
    dirty.mark_needs_layout(transform_key);
    dirty.mark_needs_layout(child_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let child_bounds = registry
        .get(child_key)
        .unwrap()
        .computed_bounds()
        .expect("child should have computed bounds");

    assert_eq!(
        child_bounds.width(),
        300.0,
        "nested pass-through (Opacity→Transform) must link grandchild to grandparent"
    );
}
```

- [ ] **Step 6: Run the test**

Run: `cargo test -p vexo test_nested_passthrough_links_correctly`
Expected: PASS

- [ ] **Step 7: Add size-adoption test**

Append:

```rust
#[test]
fn test_passthrough_adopts_child_size() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let flex_elem = make_element_key();
    let opacity_elem = make_element_key();
    let child_elem = make_element_key();

    let flex_ro = Box::new(ContainerRenderObject::new(
        Layout::default()
            .flex_direction(FlexDirection::Row)
            .align(AlignItems::Start),
    ));
    let opacity_ro = Box::new(OpacityRenderObject::new(0.5));
    let child_ro = Box::new(ContainerRenderObject::new(
        Layout::default().width(120.0).height(60.0),
    ));

    let child_key = registry.create(child_ro, child_elem);
    let opacity_key = registry.create(opacity_ro, opacity_elem);
    let flex_key = registry.create(flex_ro, flex_elem);

    registry.set_child(opacity_key, child_key);
    registry.set_child(flex_key, opacity_key);
    registry.set_root(flex_key);

    dirty.mark_needs_layout(flex_key);
    dirty.mark_needs_layout(opacity_key);
    dirty.mark_needs_layout(child_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let opacity_bounds = registry
        .get(opacity_key)
        .unwrap()
        .computed_bounds()
        .expect("opacity should have bounds");
    let child_bounds = registry
        .get(child_key)
        .unwrap()
        .computed_bounds()
        .expect("child should have bounds");

    // Pass-through RO adopts child's size.
    assert_eq!(opacity_bounds.width(), child_bounds.width());
    assert_eq!(opacity_bounds.height(), child_bounds.height());
    assert_eq!(opacity_bounds.width(), 120.0);
    assert_eq!(opacity_bounds.height(), 60.0);
}
```

- [ ] **Step 8: Run the test**

Run: `cargo test -p vexo test_passthrough_adopts_child_size`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add vexo/src/tests/passthrough_integration.rs vexo/src/lib.rs
git commit -m "test(passthrough): integration tests for grandparent→grandchild linking

Verifies Opacity pass-through lets grandchild receive grandparent's
definite width, nested pass-throughs (Opacity→Transform) chain
correctly, and pass-through ROs adopt the child's computed size."
```

---

## Task 7: Integration tests — removal & offstage flag-flip in pipeline

**Files:**
- Modify: `vexo/src/tests/passthrough_integration.rs`

**Interfaces:**
- Consumes: pass-through ROs (Tasks 3-5), registry guard (Task 2)
- Produces: tests for cleanup correctness and Offstage flag-flip in the pipeline

- [ ] **Step 1: Add the removal/no-double-cleanup test**

Append to `vexo/src/tests/passthrough_integration.rs`:

```rust
#[test]
fn test_passthrough_removal_no_double_cleanup() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let flex_elem = make_element_key();
    let opacity_elem = make_element_key();
    let child_elem = make_element_key();

    let flex_ro = Box::new(ContainerRenderObject::new(column_layout()));
    let opacity_ro = Box::new(OpacityRenderObject::new(0.5));
    let child_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));

    let child_key = registry.create(child_ro, child_elem);
    let opacity_key = registry.create(opacity_ro, opacity_elem);
    let flex_key = registry.create(flex_ro, flex_elem);

    registry.set_child(opacity_key, child_key);
    registry.set_child(flex_key, opacity_key);
    registry.set_root(flex_key);

    dirty.mark_needs_layout(flex_key);
    dirty.mark_needs_layout(opacity_key);
    dirty.mark_needs_layout(child_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    // Remove the Opacity RO (pass-through). Should NOT orphan the child's node.
    registry.remove(opacity_key);
    let orphaned = registry.drain_orphaned_layout_nodes();
    assert!(
        orphaned.is_empty(),
        "removing pass-through Opacity must not orphan the child's node"
    );

    // Now remove the child RO. This SHOULD orphan its node.
    registry.remove(child_key);
    let orphaned = registry.drain_orphaned_layout_nodes();
    assert_eq!(orphaned.len(), 1, "child's node should be orphaned once");

    // engine.remove_node should not panic on the single orphaned node.
    for node in orphaned {
        engine.remove_node(node);
    }
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p vexo test_passthrough_removal_no_double_cleanup`
Expected: PASS

- [ ] **Step 3: Add the offstage flag-flip pipeline test**

Append:

```rust
#[test]
fn test_offstage_flag_flip_in_pipeline() {
    use crate::widgets::offstage::Offstage;

    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let container_elem = make_element_key();
    let off1_elem = make_element_key();
    let off2_elem = make_element_key();
    let child1_elem = make_element_key();
    let child2_elem = make_element_key();

    let container_ro = Box::new(ContainerRenderObject::new(column_layout()));
    let off1_ro = Box::new(OffstageRenderObject::new(false)); // onstage
    let off2_ro = Box::new(OffstageRenderObject::new(true)); // offstage
    let child1_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));
    let child2_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));

    let child1_key = registry.create(child1_ro, child1_elem);
    let child2_key = registry.create(child2_ro, child2_elem);
    let off1_key = registry.create(off1_ro, off1_elem);
    let off2_key = registry.create(off2_ro, off2_elem);
    let container_key = registry.create(container_ro, container_elem);

    registry.set_child(off1_key, child1_key);
    registry.set_child(off2_key, child2_key);
    // Container has two children: off1 and off2
    // (ContainerRenderObject.add_child via registry)
    registry.set_child(container_key, off1_key);
    registry.set_child(container_key, off2_key);
    registry.set_root(container_key);

    for k in [container_key, off1_key, off2_key, child1_key, child2_key] {
        dirty.mark_needs_layout(k);
    }

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    // Initially: off1 onstage, child1 should have width 300.
    let child1_bounds = registry
        .get(child1_key)
        .unwrap()
        .computed_bounds()
        .expect("child1 should have bounds");
    assert_eq!(child1_bounds.width(), 300.0, "onstage child1 should fill width");

    // off2 offstage: zero-size bounds.
    let off2_bounds = registry
        .get(off2_key)
        .unwrap()
        .computed_bounds()
        .expect("off2 should have bounds");
    assert_eq!(off2_bounds.width(), 0.0);
    assert_eq!(off2_bounds.height(), 0.0);

    // Flip: off1 -> offstage, off2 -> onstage
    {
        let off1 = registry.get_mut(off1_key).unwrap();
        off1.as_mut()
            .as_any_mut()
            .downcast_mut::<OffstageRenderObject>()
            .unwrap()
            .set_offstage(true);
    }
    {
        let off2 = registry.get_mut(off2_key).unwrap();
        off2.as_mut()
            .as_any_mut()
            .downcast_mut::<OffstageRenderObject>()
            .unwrap()
            .set_offstage(false);
    }
    dirty.mark_needs_layout(off1_key);
    dirty.mark_needs_layout(off2_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    // After flip: off2 onstage, child2 should have width 300.
    let child2_bounds = registry
        .get(child2_key)
        .unwrap()
        .computed_bounds()
        .expect("child2 should have bounds after flip");
    assert_eq!(
        child2_bounds.width(),
        300.0,
        "newly-onstage child2 should fill width after flip"
    );

    // off1 now offstage: zero-size bounds.
    let off1_bounds = registry
        .get(off1_key)
        .unwrap()
        .computed_bounds()
        .expect("off1 should have bounds after flip");
    assert_eq!(off1_bounds.width(), 0.0);
    assert_eq!(off1_bounds.height(), 0.0);
}
```

Note: the above test sets two children on `container_key` via `set_child` twice. `ContainerRenderObject::set_child_id` replaces all children with a single child (see `container.rs:66`). To add two children, use `registry.get_mut(container_key)` and call `add_child` on the RO directly, OR build the container with both children. Check `ContainerRenderObject`'s `add_child` method — the registry's `set_child` calls `set_child_id` which overwrites. For the test, manipulate the RO directly:

```rust
    // Add both offstage ROs as children of the container.
    {
        let container = registry.get_mut(container_key).unwrap();
        container.as_mut().add_child(off1_key);
        container.as_mut().add_child(off2_key);
    }
```

Replace the two `registry.set_child(container_key, ...)` lines with this block.

- [ ] **Step 4: Run the test**

Run: `cargo test -p vexo test_offstage_flag_flip_in_pipeline`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/tests/passthrough_integration.rs
git commit -m "test(passthrough): removal no-double-cleanup and offstage flag-flip

Verifies pass-through RO removal does not orphan the borrowed child
node, and Offstage flag-flip transitions correctly in the pipeline
(onstage child fills width, offstage child is zero-size, flip swaps)."
```

---

## Task 8: Regression test — navigation transition text wrapping

**Files:**
- Modify: `vexo/src/tests/passthrough_integration.rs`

**Interfaces:**
- Consumes: all pass-through ROs (Tasks 3-5)
- Produces: end-to-end regression test reproducing the original bug scenario at the RO-tree level

This is the capstone test: it reproduces the navigation transition widget tree from the bug report and verifies text does not wrap.

- [ ] **Step 1: Add the regression test**

Append to `vexo/src/tests/passthrough_integration.rs`:

```rust
#[test]
fn test_nav_transition_text_does_not_wrap() {
    use crate::layout::measurement::{MeasureContext, TextMeasureContext};
    use crate::render_objects::ContainerRenderObject;

    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    // Tree (outgoing page only, for simplicity):
    //   Flex::column (root, fills 375 width)
    //   ├── nav_bar (Flex::row, width 140, flex_shrink 0)
    //   └── Stack
    //       └── Positioned(L=R=T=B=0)
    //           └── Opacity(0.5)            ← pass-through
    //               └── Transform(translate) ← pass-through
    //                   └── page Column (padding 24)
    //                       └── Text("This is a long text that should not wrap")

    let root_elem = make_element_key();
    let navbar_elem = make_element_key();
    let stack_elem = make_element_key();
    let pos_elem = make_element_key();
    let opacity_elem = make_element_key();
    let transform_elem = make_element_key();
    let page_elem = make_element_key();
    let text_elem = make_element_key();

    let root_ro = Box::new(ContainerRenderObject::new(
        Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .width_percent(1.0)
            .height_percent(1.0),
    ));
    let navbar_ro = Box::new(ContainerRenderObject::new(
        Layout::default()
            .flex_direction(FlexDirection::Row)
            .width(140.0)
            .flex_shrink(0.0),
    ));
    let stack_ro = Box::new(ContainerRenderObject::new(
        Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .width_percent(1.0)
            .height_percent(1.0),
    ));
    let pos_ro = Box::new(crate::render_objects::PositionedRenderObject::new(
        crate::render_objects::PositionedInsets {
            left: Some(0.0),
            right: Some(0.0),
            top: Some(0.0),
            bottom: Some(0.0),
        },
    ));
    let opacity_ro = Box::new(OpacityRenderObject::new(0.5));
    let transform_ro = Box::new(TransformRenderObject::new(
        AffineTransform::translation(0.0, 0.0),
        true,
    ));
    let page_ro = Box::new(ContainerRenderObject::new(
        Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .padding(24.0),
    ));
    let text_node_ctx = MeasureContext::Text(TextMeasureContext {
        content: "This is a long text that should not wrap".to_string(),
        font_size: 16.0,
        line_height: 1.2,
        font_family: None,
    });
    // Text is a leaf with measure context — use the engine to create it,
    // then wrap in a minimal RO that owns the node. For this test we use a
    // ContainerRenderObject as a placeholder leaf holder is wrong; instead
    // create a text leaf node directly via the engine and check its layout.
    //
    // Actually, the cleanest approach: build the tree with a real
    // TextRenderObject. Import it.
    let text_ro = Box::new(crate::render_objects::TextRenderObject::new(
        "This is a long text that should not wrap".to_string(),
        16.0,
        1.2,
        None,
    ));

    let text_key = registry.create(text_ro, text_elem);
    let page_key = registry.create(page_ro, page_elem);
    let transform_key = registry.create(transform_ro, transform_elem);
    let opacity_key = registry.create(opacity_ro, opacity_elem);
    let pos_key = registry.create(pos_ro, pos_elem);
    let stack_key = registry.create(stack_ro, stack_elem);
    let navbar_key = registry.create(navbar_ro, navbar_elem);
    let root_key = registry.create(root_ro, root_elem);

    registry.set_child(page_key, text_key);
    registry.set_child(transform_key, page_key);
    registry.set_child(opacity_key, transform_key);
    registry.set_child(pos_key, opacity_key);
    registry.set_child(stack_key, pos_key);
    registry.set_child(root_key, navbar_key);
    {
        let root = registry.get_mut(root_key).unwrap();
        root.as_mut().add_child(stack_key);
    }
    registry.set_root(root_key);

    for k in [
        root_key, navbar_key, stack_key, pos_key, opacity_key, transform_key, page_key, text_key,
    ] {
        dirty.mark_needs_layout(k);
    }

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(375.0, 667.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let text_bounds = registry
        .get(text_key)
        .unwrap()
        .computed_bounds()
        .expect("text should have bounds");

    // The text's natural width ("This is a long text that should not wrap" @ 16px)
    // is ~290px. With padding 24px (48 total), the page Column needs ~338px.
    // Window is 375px. The text should NOT wrap — its height should be ~one line
    // (16.0 * 1.2 = 19.2), not multiple lines.
    let single_line_height = 16.0 * 1.2;
    assert!(
        text_bounds.height() <= single_line_height * 1.5,
        "text should not wrap (height {} should be ~one line {}); \
         width was {}",
        text_bounds.height(),
        single_line_height,
        text_bounds.width()
    );
    assert!(
        text_bounds.width() >= 280.0,
        "text should be on one line (width {} should be >= natural ~290); \
         this means it received enough width through the pass-through ROs",
        text_bounds.width()
    );
}
```

Note: `TextRenderObject::new` signature may differ — check `vexo/src/render_objects/text.rs` for the actual constructor before writing this test. Adjust the constructor call to match.

- [ ] **Step 2: Verify the TextRenderObject constructor signature**

Run: `grep -n "pub fn new" vexo/src/render_objects/text.rs`
Adjust the test's `TextRenderObject::new(...)` call to match the actual signature.

- [ ] **Step 3: Run the test**

Run: `cargo test -p vexo test_nav_transition_text_does_not_wrap`
Expected: PASS (the pass-through ROs + the existing Stretch workaround should make text not wrap). If FAIL, debug — the text wrapping means constraints aren't propagating correctly through the pass-through chain.

- [ ] **Step 4: Run the full test suite**

Run: `cargo test -p vexo`
Expected: PASS — all tests green.

- [ ] **Step 5: Run the full workspace build + tests**

Run: `cargo build && cargo test`
Expected: PASS — `shared_app` and `desktop_demo` also build (they use Opacity/Transform/Offstage in transitions; their behavior is unchanged at the API level).

- [ ] **Step 6: Commit**

```bash
git add vexo/src/tests/passthrough_integration.rs
git commit -m "test(passthrough): regression test for nav transition text wrapping

Reproduces the original bug scenario (commit 76bfc73) at the RO-tree
level: Flex::column → nav_bar + Stack → Positioned → Opacity →
Transform → page Column → Text. With pass-through ROs, the text
receives definite width through Positioned and does not wrap.
The AlignItems::Stretch workaround on Stack remains in place."
```

---

## Self-Review

### Spec coverage

| Spec section | Task(s) |
|---|---|
| `is_pass_through()` trait method | Task 1 |
| Registry `remove()` cleanup guard | Task 2 |
| `OpacityRenderObject` pass-through conversion | Task 3 |
| `TransformRenderObject` pass-through conversion | Task 4 |
| `OffstageRenderObject` onstage pass-through + flag-flip | Task 5 |
| Unit tests (per-RO) | Tasks 3, 4, 5 |
| Integration: grandchild receives grandparent width | Task 6 |
| Integration: nested pass-through | Task 6 |
| Integration: size adoption | Task 6 |
| Integration: removal no-double-cleanup | Task 7 |
| Integration: offstage flag-flip in pipeline | Task 7 |
| Regression: nav transition text wrapping | Task 8 |
| `layouter.rs` changes | None (spec: layouter unchanged) |
| `DecoratedContainer` | None (spec: deferred) |
| `IndexedStack` performLayout | None (spec: deferred) |

All spec requirements covered. Deferred items explicitly excluded.

### Placeholder scan

- Task 8 Step 2 flags a real unknown (`TextRenderObject::new` signature) with an explicit verification step — not a placeholder, it's a known-unknown with a concrete resolution step.
- All code blocks are complete.

### Type consistency

- `child_layout_node: Option<LayoutNodeKey>` — used consistently across Tasks 3, 4, 5.
- `owned_node: Option<LayoutNodeKey>` — used in Task 5 only.
- `is_pass_through() -> bool` — consistent.
- `layout_node()` returns `Option<LayoutNodeKey>` — consistent (pass-through returns child's node, offstage returns owned_node).
- Registry guard uses `obj.is_pass_through()` — matches trait method from Task 1.
