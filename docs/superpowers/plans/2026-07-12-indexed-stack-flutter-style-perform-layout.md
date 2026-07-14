# IndexedStack Flutter-Style `performLayout` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `IndexedStack` lay out only the visible child (matching Flutter's `RenderIndexedStack.performLayout`), instead of laying out all children where offstage ones become zero-size leaves that still participate in the Taffy tree.

**Architecture:** Introduce a dedicated `IndexedStackRenderObject` that filters its Taffy `set_children()` call to include only the child at `index`. Offstage children's zero-size leaf nodes (owned by their `OffstageRenderObject`) are never linked into the IndexedStack's Taffy node, so Taffy's `compute()` never visits them. The visible child, already pass-through through its `Offstage` wrapper, links its page's Taffy node directly to the IndexedStack's node — receiving the parent's constraints through the single-child `Column + Stretch + 100%/100%` layout. The widget reuses `ContainerElement` (no new element type): `ContainerElement::rebuild()` already calls `self.widget.update_render_object(ro)` and marks the RO dirty on `UpdateResult::LAYOUT`. When `IndexedStack::update_render_object()` calls `set_index()` and it returns true, the RO is marked LAYOUT-dirty, and the next layout pass re-runs `IndexedStackRenderObject::layout()` with the new index, re-filtering `set_children()`.

**Tech Stack:** Rust, Taffy 0.9.1 (flexbox layout), slotmap (generational keys), glyphon (text measurement).

**Spec:** `docs/vexo_vs_flutter_render_object_architecture.md` (finding, section "IndexedStack special case"), `docs/superpowers/specs/2026-07-08-pass-through-render-objects-design.md` (non-goals — this plan implements the deferred item).

## Global Constraints

- Workspace dependency versions are pinned in root `Cargo.toml`; reference via `{ workspace = true }`. Taffy 0.9.1.
- `LayoutNodeKey` is a slotmap key (`new_key_type!`) with NO `Default` impl — cannot use `unwrap_or_default()`.
- `LayoutResult` is a struct with a required `node: LayoutNodeKey` field; the layouter discards the return value of `layout()` (`layouter.rs:139`), so the field is structurally required but semantically dead for filtering ROs.
- The pass-through migration is complete (commits `b0d7762`..`ea865f4`): `OpacityRenderObject`, `TransformRenderObject`, and `OffstageRenderObject` (onstage) are pass-through; `is_pass_through()` exists on the `RenderObject` trait; `RenderObjectRegistry::remove()` guards cleanup.
- `OffstageRenderObject` (offstage) owns a zero-size Taffy leaf and returns it via `layout_node()`. This leaf must NOT be linked into the IndexedStack's Taffy node.
- `OffstageElement::rebuild()` already calls `context.mark_parent_needs_layout()` when the `offstage` flag flips (`elements/offstage.rs:150`), so the IndexedStack's RO is marked dirty when any child's visibility changes. Commit `48e3ae8`.
- `ContainerElement::child_mounted()` calls `context.mark_needs_layout(parent_ro)` when a new child mounts (`elements/container.rs:266`), so the IndexedStack's RO is marked dirty when children are added/removed.
- Build command: `cargo build -p vexo`
- Test command: `cargo test -p vexo`
- No comments in code unless explaining a non-obvious invariant.

---

## Background: Why a Dedicated Render Object

`ContainerRenderObject` is shared by `Flex`, `Stack`, `IndexedStack`, `Grid`, `DecoratedContainer`, `SafeArea`, `WithLayout`, and `Container`. Its `layout()` unconditionally passes ALL `child_nodes` to `engine.set_children()`. Changing this behavior would affect every container widget.

Flutter has a separate `RenderIndexedStack` class that overrides `performLayout` to lay out only the visible child. Vexo should match this: a dedicated `IndexedStackRenderObject` that filters `child_nodes` to `[child_nodes[index]]` before calling `set_children()`.

The `Column + Stretch + 100%/100%` layout on IndexedStack already makes a single visible child fill the stack (Stretch fills the cross-axis; the child's main-axis size is its content height, but the stack's `height_percent(1.0)` fills the parent). This is the closest Taffy equivalent to Flutter's "hand the child the parent constraints." The change is NOT to the layout style — it's to WHICH children participate.

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `vexo/src/render_objects/indexed_stack.rs` | New `IndexedStackRenderObject` — filters Taffy children to `[child_nodes[index]]`. | Create |
| `vexo/src/render_objects/mod.rs` | Re-export `IndexedStackRenderObject`. | Modify |
| `vexo/src/widgets/indexed_stack.rs` | `IndexedStack` widget: use `IndexedStackRenderObject` instead of `ContainerRenderObject`; `update_render_object()` calls `set_index()`. Reuses `ContainerElement` (no new element needed — `ContainerElement::rebuild()` already calls `update_render_object()` and marks dirty on `LAYOUT`). | Modify |
| `vexo/src/passthrough_integration.rs` | New integration tests for Flutter-style `performLayout` behavior. | Modify |

---

## Task 1: Create `IndexedStackRenderObject`

**Files:**
- Create: `vexo/src/render_objects/indexed_stack.rs`
- Modify: `vexo/src/render_objects/mod.rs`

**Interfaces:**
- Consumes: `RenderObject` trait, `LayoutContext`, `LayoutResult`, `LayoutNodeKey`, `Layout`, `Style`, `RenderObjectKey`, `Bounds`, `Logical`, `Point`, `Size`, `RenderCommand`, `HitTestContext`, `PaintContext`, `ContainerRenderObject` (for paint/hit-test delegation or reuse of style painting).
- Produces: `IndexedStackRenderObject` struct with:
  - `pub fn new(index: usize) -> Self`
  - `pub fn new_with_style(index: usize, layout: Layout, style: Style) -> Self`
  - `pub fn set_index(&mut self, index: usize) -> bool` (returns true if changed)
  - `pub fn set_layout(&mut self, layout: Layout) -> bool`
  - `pub fn set_style(&mut self, style: Style) -> bool`
  - `pub fn index(&self) -> usize`
  - All `RenderObject` trait methods implemented.

- [ ] **Step 1: Write the failing tests**

Create `vexo/src/render_objects/indexed_stack.rs` with the test module first. The tests reference `IndexedStackRenderObject` which doesn't exist yet — they will fail to compile.

```rust
//! Render object for IndexedStack — lays out only the visible child.
//!
//! Matches Flutter's `RenderIndexedStack.performLayout`: only the child at
//! `index` participates in Taffy layout. Offstage children's zero-size leaf
//! nodes (owned by their `OffstageRenderObject`) are NOT linked into this
//! node's Taffy children list, so Taffy's `compute()` never visits them.

use std::any::Any;

use crate::core::{Bounds, Color, Logical, Point, Position, Absolute};
use crate::layout::{Layout, LayoutNodeKey};
use crate::render::RenderCommand;
use crate::style::Style;
use crate::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey,
};

pub struct IndexedStackRenderObject {
    children: Vec<RenderObjectKey>,
    index: usize,
    layout: Layout,
    style: Style,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl IndexedStackRenderObject {
    pub fn new(index: usize) -> Self {
        Self::new_with_style(index, indexed_stack_layout(), Style::default())
    }

    pub fn new_with_style(index: usize, layout: Layout, style: Style) -> Self {
        Self {
            children: Vec::new(),
            index,
            layout,
            style,
            computed_bounds: None,
            layout_node: None,
        }
    }

    pub fn set_index(&mut self, index: usize) -> bool {
        if self.index != index {
            self.index = index;
            true
        } else {
            false
        }
    }

    pub fn set_layout(&mut self, layout: Layout) -> bool {
        if self.layout != layout {
            self.layout = layout;
            true
        } else {
            false
        }
    }

    pub fn set_style(&mut self, style: Style) -> bool {
        if self.style != style {
            self.style = style;
            true
        } else {
            false
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }
}

fn indexed_stack_layout() -> Layout {
    use crate::layout::{AlignItems, FlexDirection};
    Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch)
        .width_percent(1.0)
        .height_percent(1.0)
}

impl RenderObject for IndexedStackRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let visible_nodes: Vec<LayoutNodeKey> = child_nodes
            .get(self.index)
            .map(|n| vec![*n])
            .unwrap_or_default();

        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &self.layout);
                ctx.engine().set_children(existing, &visible_nodes);
                LayoutResult {
                    node: existing,
                    size: crate::core::Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_container(&self.layout, &visible_nodes);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: crate::core::Size::zero(),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        let bounds = match &self.computed_bounds {
            Some(b) => b,
            None => return vec![],
        };

        let mut commands = Vec::new();
        let pos: Position<Logical, Absolute> = ctx.absolute_position();

        let absolute_bounds = Bounds::new(
            pos.x,
            pos.y,
            pos.x + bounds.width(),
            pos.y + bounds.height(),
        );

        if let Some(ref cr) = self.style.corner_radius {
            commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
        }

        if let Some(bg_color) = self.style.background {
            commands.push(RenderCommand::rect(absolute_bounds, bg_color));
        }

        if let Some(ref border) = self.style.border {
            commands.push(RenderCommand::rect_with_border(
                absolute_bounds,
                Color::TRANSPARENT,
                border.color,
                border.width,
            ));
        }

        if self.style.corner_radius.is_some() {
            commands.push(RenderCommand::PopCornerRadius);
        }

        commands
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        &self.children
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn add_child(&mut self, child: RenderObjectKey) {
        self.children.push(child);
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if let Some(pos) = self.children.iter().position(|&c| c == old) {
            self.children[pos] = new;
        } else {
            self.children.push(new);
        }
    }

    fn clear_children(&mut self) {
        self.children.clear();
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.children = vec![child];
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        if self.style.clip {
            self.computed_bounds
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Size;
    use crate::layout::{LayoutEngine, TaffyLayoutEngine};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_indexed_stack_ro_creation() {
        let ro = IndexedStackRenderObject::new(0);
        assert_eq!(ro.index(), 0);
        assert!(ro.layout_node().is_none());
    }

    #[test]
    fn test_indexed_stack_ro_set_index() {
        let mut ro = IndexedStackRenderObject::new(0);
        assert!(ro.set_index(2));
        assert_eq!(ro.index(), 2);
        assert!(!ro.set_index(2));
    }

    #[test]
    fn test_indexed_stack_ro_layout_filters_to_visible_child() {
        let mut ro = IndexedStackRenderObject::new(1);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child0 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine().create_leaf(&Layout::default().width(50.0).height(50.0))
        };
        let child1 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine().create_leaf(&Layout::default().width(80.0).height(60.0))
        };
        let child2 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine().create_leaf(&Layout::default().width(30.0).height(30.0))
        };

        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        ro.layout(&mut ctx, &[child0, child1, child2]);

        let stack_node = ro.layout_node().expect("should have a layout node");

        let linked_children = engine.children(stack_node);
        assert_eq!(
            linked_children.len(),
            1,
            "only the visible child (index 1) should be linked"
        );
        assert_eq!(
            linked_children[0], child1,
            "the linked child should be the one at index 1"
        );
    }

    #[test]
    fn test_indexed_stack_ro_layout_index_out_of_bounds_links_nothing() {
        let mut ro = IndexedStackRenderObject::new(5);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child0 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine().create_leaf(&Layout::default().width(50.0).height(50.0))
        };

        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
        ro.layout(&mut ctx, &[child0]);

        let stack_node = ro.layout_node().expect("should have a layout node");
        let linked_children = engine.children(stack_node);
        assert!(
            linked_children.is_empty(),
            "index out of bounds should link no children"
        );
    }

    #[test]
    fn test_indexed_stack_ro_index_change_relays_children() {
        let mut ro = IndexedStackRenderObject::new(0);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child0 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine().create_leaf(&Layout::default().width(50.0).height(50.0))
        };
        let child1 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine().create_leaf(&Layout::default().width(80.0).height(60.0))
        };

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[child0, child1]);
        }

        let stack_node = ro.layout_node().unwrap();
        assert_eq!(engine.children(stack_node), vec![child0]);

        ro.set_index(1);
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[child0, child1]);
        }

        assert_eq!(
            engine.children(stack_node),
            vec![child1],
            "after index flip, the visible child should be child1"
        );
    }

    #[test]
    fn test_indexed_stack_ro_apply_layout_reads_bounds() {
        let mut ro = IndexedStackRenderObject::new(0);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let child0 = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ctx.engine().create_leaf(&Layout::default().width(100.0).height(50.0))
        };

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[child0]);
        }

        let stack_node = ro.layout_node().unwrap();
        engine.compute(stack_node, Size::new(200.0, 200.0), &mut font_system);

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.apply_layout(&mut ctx);
        }

        let bounds = ro.computed_bounds().expect("should have bounds");
        assert_eq!(bounds.width(), 200.0);
        assert_eq!(bounds.height(), 200.0);
    }
}
```

- [ ] **Step 2: Add the module to `render_objects/mod.rs`**

In `vexo/src/render_objects/mod.rs`, add the module declaration and re-export. After the `container` module line (~line 12), add:

```rust
mod indexed_stack;
```

And in the re-export section (after `pub use container::ContainerRenderObject;` ~line 21), add:

```rust
pub use indexed_stack::IndexedStackRenderObject;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p vexo render_objects::indexed_stack`
Expected: FAIL — compilation errors because the module is new and references unresolved types. (After Step 2, the module is wired in; the tests should compile and the assertions verify the filtering behavior.)

- [ ] **Step 4: Build to verify it compiles**

Run: `cargo build -p vexo`
Expected: PASS — the new RO compiles. Fix any import warnings.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vexo render_objects::indexed_stack`
Expected: PASS — all 5 tests green.

- [ ] **Step 6: Run full test suite to verify no regressions**

Run: `cargo test -p vexo`
Expected: PASS — no existing tests reference `IndexedStackRenderObject` yet, so no regressions.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/render_objects/indexed_stack.rs vexo/src/render_objects/mod.rs
git commit -m "feat(render_objects): add IndexedStackRenderObject

Dedicated render object for IndexedStack that filters its Taffy
set_children() to include only the child at \`index\`. Offstage
children's zero-size leaf nodes are never linked into the stack's
Taffy node, so compute() never visits them — matching Flutter's
RenderIndexedStack.performLayout which lays out only the visible
child.

Currently unused; the IndexedStack widget is wired in the next task."
```

---

## Task 2: Wire `IndexedStack` widget to use `IndexedStackRenderObject`

**Files:**
- Modify: `vexo/src/widgets/indexed_stack.rs`

**Interfaces:**
- Consumes: `IndexedStackRenderObject` from Task 1, `ContainerElement` (existing), `Element` trait, `RenderObject` trait, `UpdateResult`, `Widget` trait.
- Produces: `IndexedStack` widget that creates `IndexedStackRenderObject` instead of `ContainerRenderObject`, and whose `update_render_object()` calls `set_index()` when the index changes.

**Design decision:** Reuse `ContainerElement` — it already handles multi-child reconciliation (mount/inflate/update/unmount) and child_mounted → mark_needs_layout. The only thing that needs to happen on index change is `update_render_object()` calling `set_index()`, which `ContainerElement::rebuild()` already invokes via `self.widget.update_render_object(ro)`. When `set_index()` returns true, `update_render_object()` returns `UpdateResult::LAYOUT`, and `ContainerElement::rebuild()` marks the RO dirty. The next layout pass re-runs `IndexedStackRenderObject::layout()` with the new index, re-filtering `set_children()`.

No new element type is needed.

- [ ] **Step 1: Write the failing test**

Add this test to the `tests` module in `vexo/src/widgets/indexed_stack.rs` (after `test_indexed_stack_clone_preserves_wrappers`):

```rust
    #[test]
    fn test_indexed_stack_creates_indexed_stack_render_object() {
        use crate::render_objects::IndexedStackRenderObject;

        let s = IndexedStack::new(1)
            .push(Text::new("A"))
            .push(Text::new("B"));

        let ro = s.create_render_object();
        let indexed_ro = ro
            .as_any()
            .downcast_ref::<IndexedStackRenderObject>()
            .expect("IndexedStack should create IndexedStackRenderObject");
        assert_eq!(indexed_ro.index(), 1);
    }

    #[test]
    fn test_indexed_stack_update_render_object_index_change() {
        use crate::render_objects::IndexedStackRenderObject;

        let s_old = IndexedStack::new(0).push(Text::new("A")).push(Text::new("B"));
        let s_new = IndexedStack::new(1).push(Text::new("A")).push(Text::new("B"));

        let mut ro = s_old.create_render_object();
        let result = s_new.update_render_object(ro.as_mut());

        assert!(
            result.contains(crate::UpdateResult::LAYOUT),
            "index change should signal LAYOUT"
        );
        let indexed_ro = ro
            .as_any()
            .downcast_ref::<IndexedStackRenderObject>()
            .unwrap();
        assert_eq!(indexed_ro.index(), 1);
    }

    #[test]
    fn test_indexed_stack_update_render_object_no_index_change() {
        use crate::render_objects::IndexedStackRenderObject;

        let s_old = IndexedStack::new(1).push(Text::new("A")).push(Text::new("B"));
        let s_new = IndexedStack::new(1).push(Text::new("A")).push(Text::new("B"));

        let mut ro = s_old.create_render_object();
        let result = s_new.update_render_object(ro.as_mut());

        assert_eq!(
            result,
            crate::UpdateResult::NONE,
            "no index/layout/style change should signal NONE"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo indexed_stack::tests`
Expected: FAIL — `create_render_object()` still returns `ContainerRenderObject`, so the downcast to `IndexedStackRenderObject` fails.

- [ ] **Step 3: Update imports in `vexo/src/widgets/indexed_stack.rs`**

In `vexo/src/widgets/indexed_stack.rs`, replace the import of `ContainerRenderObject`:

Replace:
```rust
use super::super::render_objects::ContainerRenderObject;
```

with:
```rust
use super::super::render_objects::IndexedStackRenderObject;
```

- [ ] **Step 4: Update `create_render_object()`**

In `vexo/src/widgets/indexed_stack.rs`, find the `impl Widget for IndexedStack` block and replace `create_render_object()`:

Replace:
```rust
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_with_style(
            self.layout.clone(),
            self.style.clone(),
        ))
    }
```

with:
```rust
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(IndexedStackRenderObject::new_with_style(
            self.index,
            self.layout.clone(),
            self.style.clone(),
        ))
    }
```

- [ ] **Step 5: Update `update_render_object()`**

In the same `impl Widget for IndexedStack` block, replace `update_render_object()`:

Replace:
```rust
    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(container_ro) = render_object
            .as_any_mut()
            .downcast_mut::<ContainerRenderObject>()
        {
            let layout_changed = container_ro.set_layout(self.layout.clone());
            let style_changed = container_ro.set_style(self.style.clone());
            if layout_changed {
                UpdateResult::LAYOUT
            } else if style_changed {
                UpdateResult::PAINT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }
```

with:
```rust
    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(ro) = render_object
            .as_any_mut()
            .downcast_mut::<IndexedStackRenderObject>()
        {
            let index_changed = ro.set_index(self.index);
            let layout_changed = ro.set_layout(self.layout.clone());
            let style_changed = ro.set_style(self.style.clone());
            if index_changed || layout_changed {
                UpdateResult::LAYOUT
            } else if style_changed {
                UpdateResult::PAINT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vexo indexed_stack::tests`
Expected: PASS — all widget tests (existing + 3 new) green.

- [ ] **Step 7: Run full test suite**

Run: `cargo test -p vexo`
Expected: Some failures in integration tests that assert `IndexedStack`'s child gets laid out — specifically `test_indexed_stack_flag_flip_updates_layout` and possibly others. These are expected and will be verified/fixed in Task 3. If only those fail, proceed. If other tests fail, investigate before proceeding.

- [ ] **Step 8: Commit**

```bash
git add vexo/src/widgets/indexed_stack.rs
git commit -m "feat(widgets): wire IndexedStack to IndexedStackRenderObject

IndexedStack now creates IndexedStackRenderObject instead of
ContainerRenderObject. update_render_object() calls set_index()
so that when the index changes, the RO is marked LAYOUT-dirty
and the next layout pass re-filters set_children() to link only
the newly-visible child."
```

---

## Task 3: Verify and fix integration tests

**Files:**
- Modify: `vexo/src/stateful_integration_test.rs` (if assertions need updating)
- Modify: `vexo/src/passthrough_integration.rs` (if the nav transition regression test references ContainerRenderObject for the stack)

**Interfaces:**
- Consumes: `IndexedStackRenderObject` from Task 1, wired `IndexedStack` widget from Task 2.
- Produces: All integration tests passing with the new Flutter-style `performLayout` behavior.

The key behavioral change: offstage children's Taffy nodes are no longer linked to the IndexedStack's node. Previously, they were linked (as zero-size leaves). Now they're orphaned in the Taffy tree (still exist as nodes owned by their `OffstageRenderObject`, but not children of any computed node).

This means:
1. `engine.get_layout(offstage_child_node)` may return `None` or stale results (the node was never computed because it's not reachable from the root). This is correct Flutter behavior.
2. The `apply_layout_recursive` walk uses `children()` on the RO, NOT the Taffy tree. `OffstageRenderObject::children()` returns `&[]` when offstage, so the layouter skips applying layout to offstage children's ROs. This is already correct.
3. The visible child's RO gets `apply_layout` called, reads its computed bounds. Correct.

- [ ] **Step 1: Run the full test suite and capture failures**

Run: `cargo test -p vexo 2>&1 | tee /tmp/test_output.txt`
Expected: Some tests may fail. Read the output to identify which.

- [ ] **Step 2: Read the test output**

Run: `cat /tmp/test_output.txt` (or use the Read tool)
Identify each failure. Likely candidates:
- `test_indexed_stack_flag_flip_updates_layout` — asserts bounds on offstage children. May need to relax assertions about offstage children (their bounds are now `None` instead of zero-size, which is more correct).
- `test_passthrough_nav_transition_text_does_not_wrap` (in `passthrough_integration.rs`) — this should still PASS (it's the regression test for the original bug; the new behavior is strictly better for this scenario).

- [ ] **Step 3: Fix failing tests**

For each failing test, determine if the assertion is testing old (incorrect) behavior or real behavior:

**If `test_indexed_stack_flag_flip_updates_layout` fails** on the offstage bounds assertion (around `stateful_integration_test.rs:1966`):

The current assertion is:
```rust
        assert!(
            bounds_b.is_none() || bounds_b.unwrap().width() == 0.0,
            "offstage Page B should have zero or no bounds"
        );
```

This should still pass — `OffstageRenderObject::children()` returns `&[]` when offstage, so `apply_layout_recursive` never calls `apply_layout` on the offstage child's RO, so its `computed_bounds` stays `None`. The assertion allows `None`. If it fails, it means something else changed. Investigate.

**If other tests fail** with assertion errors about offstage children having unexpected bounds:

The fix is to update the assertion to match the new (correct) behavior: offstage children have `None` bounds (not zero-size bounds). This is more correct — Flutter's offstage children don't have layout either.

**If tests fail with panics** (e.g., "child node not found"):

This indicates a real bug in the implementation. Do NOT weaken the test — investigate the RO's `layout()` or the element's `rebuild()`. Likely cause: the `set_children()` call with a filtered list is removing offstage children's nodes from the Taffy tree entirely (Taffy's `set_children` may detach previously-linked children). Check whether offstage children's zero-size leaf nodes survive after `set_children()` is called with only the visible child.

To verify offstage nodes survive, add a debug assertion in the test or use `engine.get_layout(offstage_node)` — if it returns `None` after `set_children()`, the node was removed. If so, the fix is: `OffstageRenderObject` must own its zero-size leaf independently of whether it's linked to a parent. The current `OffstageRenderObject` creates the leaf in `layout()` and stores it in `owned_node`. As long as `owned_node` is not `remove_node`'d, the node persists in the engine even if not linked to a parent. `set_children()` on the IndexedStack only changes the IndexedStack's child list — it does NOT remove the offstage leaf from the engine (Taffy's `set_children` detaches the old children from the parent but does not delete them).

If panics persist, add `log::debug!` to `IndexedStackRenderObject::layout()` logging `self.index`, `child_nodes.len()`, and `visible_nodes`, and have the user run with `RUST_LOG=debug` to trace.

- [ ] **Step 4: Re-run the full test suite**

Run: `cargo test -p vexo`
Expected: PASS — all tests green.

- [ ] **Step 5: Commit (if any test files were modified)**

```bash
git add -A
git commit -m "test(integration): update assertions for Flutter-style IndexedStack layout

Offstage children no longer get laid out (their Taffy nodes are not
linked to the IndexedStack's node), matching Flutter's
RenderIndexedStack. Update assertions that expected zero-size bounds
on offstage children to expect None bounds instead."
```

If no test files needed modification, skip this step.

---

## Task 4: Add integration test for Flutter-style `performLayout`

**Files:**
- Create: `vexo/src/tests/indexed_stack_perform_layout.rs` (or add to `passthrough_integration.rs` — see decision below)

**Decision:** Add to `vexo/src/passthrough_integration.rs` — it already has the harness for building RO trees and running `Layouter::layout()`. Creating a new file requires wiring it into `lib.rs` as a `#[cfg(test)]` module. Adding to the existing integration test file is simpler and co-locates related tests.

**Interfaces:**
- Consumes: `IndexedStackRenderObject`, `OffstageRenderObject`, `ContainerRenderObject`, `Layouter`, `RenderObjectRegistry`, `TaffyLayoutEngine`, `DirtyTracking`.
- Produces: Integration tests proving the visible child receives the IndexedStack's constraints and offstage children are not laid out.

- [ ] **Step 1: Write the failing tests**

Add these tests to the end of `vexo/src/passthrough_integration.rs` (before the closing of the module if there is one, or at the end of the file):

```rust
// ============================================================================
// IndexedStack Flutter-style performLayout integration tests
// ============================================================================

use crate::render_objects::IndexedStackRenderObject;

/// Build a tree: IndexedStack → [Offstage(onstage, child0), Offstage(offstage, child1)].
/// Returns (stack_key, offstage0_key, offstage1_key, child0_key, child1_key).
fn build_indexed_stack_tree(
    registry: &mut RenderObjectRegistry,
    index: usize,
    child0_ro: Box<dyn RenderObject>,
    child1_ro: Box<dyn RenderObject>,
    offstage0_flag: bool,
    offstage1_flag: bool,
) -> (
    RenderObjectKey,
    RenderObjectKey,
    RenderObjectKey,
    RenderObjectKey,
    RenderObjectKey,
) {
    let stack_elem = make_element_key();
    let offstage0_elem = make_element_key();
    let offstage1_elem = make_element_key();
    let child0_elem = make_element_key();
    let child1_elem = make_element_key();

    let child0_key = registry.create(child0_ro, child0_elem);
    let child1_key = registry.create(child1_ro, child1_elem);
    let offstage0_key = registry.create(OffstageRenderObject::new(offstage0_flag), offstage0_elem);
    let offstage1_key = registry.create(OffstageRenderObject::new(offstage1_flag), offstage1_elem);
    let stack_key = registry.create(IndexedStackRenderObject::new(index), stack_elem);

    registry.set_child(offstage0_key, child0_key);
    registry.set_child(offstage1_key, child1_key);
    registry.set_child(stack_key, offstage0_key);
    registry.set_child(stack_key, offstage1_key);
    registry.set_root(stack_key);

    (stack_key, offstage0_key, offstage1_key, child0_key, child1_key)
}

#[test]
fn test_indexed_stack_only_visible_child_is_laid_out() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let child0_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));
    let child1_ro = Box::new(ContainerRenderObject::new(Layout::default().height(60.0)));

    let (stack_key, offstage0_key, offstage1_key, child0_key, child1_key) =
        build_indexed_stack_tree(
            &mut registry,
            0,
            child0_ro,
            child1_ro,
            false,
            true,
        );

    dirty.mark_needs_layout(stack_key);
    dirty.mark_needs_layout(offstage0_key);
    dirty.mark_needs_layout(offstage1_key);
    dirty.mark_needs_layout(child0_key);
    dirty.mark_needs_layout(child1_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let child0_bounds = registry
        .get(child0_key)
        .unwrap()
        .computed_bounds()
        .expect("visible child0 should have bounds");
    assert_eq!(
        child0_bounds.width(),
        300.0,
        "visible child should fill the stack's width (grandparent constraints)"
    );

    let stack_bounds = registry
        .get(stack_key)
        .unwrap()
        .computed_bounds()
        .expect("stack should have bounds");
    assert_eq!(stack_bounds.width(), 300.0);
    assert_eq!(stack_bounds.height(), 200.0);
}

#[test]
fn test_indexed_stack_offstage_child_not_linked_to_taffy_node() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let child0_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));
    let child1_ro = Box::new(ContainerRenderObject::new(Layout::default().height(60.0)));

    let (stack_key, offstage0_key, offstage1_key, child0_key, child1_key) =
        build_indexed_stack_tree(
            &mut registry,
            0,
            child0_ro,
            child1_ro,
            false,
            true,
        );

    dirty.mark_needs_layout(stack_key);
    dirty.mark_needs_layout(offstage0_key);
    dirty.mark_needs_layout(offstage1_key);
    dirty.mark_needs_layout(child0_key);
    dirty.mark_needs_layout(child1_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let stack_node = registry
        .get(stack_key)
        .unwrap()
        .layout_node()
        .expect("stack should have a layout node");

    let linked_children = engine.children(stack_node);
    assert_eq!(
        linked_children.len(),
        1,
        "IndexedStack's Taffy node should have exactly 1 linked child (the visible one)"
    );

    let offstage1_node = registry
        .get(offstage1_key)
        .unwrap()
        .layout_node()
        .expect("offstage1 should still own its zero-size leaf node");
    assert!(
        !linked_children.contains(&offstage1_node),
        "offstage child's zero-size leaf must NOT be linked to the stack's Taffy node"
    );

    assert!(
        engine.get_layout(offstage1_node).is_none(),
        "offstage child's leaf node should not have a computed layout (not reachable from root)"
    );
}

#[test]
fn test_indexed_stack_index_flip_relays_visible_child() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let child0_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));
    let child1_ro = Box::new(ContainerRenderObject::new(Layout::default().height(60.0)));

    let (stack_key, offstage0_key, offstage1_key, child0_key, child1_key) =
        build_indexed_stack_tree(
            &mut registry,
            0,
            child0_ro,
            child1_ro,
            false,
            true,
        );

    dirty.mark_needs_layout(stack_key);
    dirty.mark_needs_layout(offstage0_key);
    dirty.mark_needs_layout(offstage1_key);
    dirty.mark_needs_layout(child0_key);
    dirty.mark_needs_layout(child1_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let child0_bounds_before = registry
        .get(child0_key)
        .unwrap()
        .computed_bounds()
        .expect("child0 visible initially");
    assert!(child0_bounds_before.width() > 0.0);

    let stack_ro = registry.get_mut(stack_key).unwrap();
    let downcast_ro = stack_ro
        .as_any_mut()
        .downcast_mut::<IndexedStackRenderObject>()
        .unwrap();
    downcast_ro.set_index(1);

    let offstage0_ro = registry.get_mut(offstage0_key).unwrap();
    let downcast_off0 = offstage0_ro
        .as_any_mut()
        .downcast_mut::<OffstageRenderObject>()
        .unwrap();
    downcast_off0.set_offstage(true);

    let offstage1_ro = registry.get_mut(offstage1_key).unwrap();
    let downcast_off1 = offstage1_ro
        .as_any_mut()
        .downcast_mut::<OffstageRenderObject>()
        .unwrap();
    downcast_off1.set_offstage(false);

    dirty.mark_needs_layout(stack_key);
    dirty.mark_needs_layout(offstage0_key);
    dirty.mark_needs_layout(offstage1_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let child1_bounds_after = registry
        .get(child1_key)
        .unwrap()
        .computed_bounds()
        .expect("child1 should have bounds after flip");
    assert_eq!(
        child1_bounds_after.width(),
        300.0,
        "newly-visible child1 should fill the stack's width"
    );

    let stack_node = registry
        .get(stack_key)
        .unwrap()
        .layout_node()
        .unwrap();
    let linked_children = engine.children(stack_node);
    assert_eq!(
        linked_children.len(),
        1,
        "after flip, still exactly 1 linked child"
    );
}

#[test]
fn test_indexed_stack_visible_child_receives_grandparent_width() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let parent_elem = make_element_key();
    let stack_elem = make_element_key();
    let offstage_elem = make_element_key();
    let child_elem = make_element_key();

    let child_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));
    let child_key = registry.create(child_ro, child_elem);
    let offstage_ro = Box::new(OffstageRenderObject::new(false));
    let offstage_key = registry.create(offstage_ro, offstage_elem);
    let stack_ro = Box::new(IndexedStackRenderObject::new(0));
    let stack_key = registry.create(stack_ro, stack_elem);
    let parent_ro = Box::new(ContainerRenderObject::new(column_layout()));
    let parent_key = registry.create(parent_ro, parent_elem);

    registry.set_child(offstage_key, child_key);
    registry.set_child(stack_key, offstage_key);
    registry.set_child(parent_key, stack_key);
    registry.set_root(parent_key);

    dirty.mark_needs_layout(parent_key);
    dirty.mark_needs_layout(stack_key);
    dirty.mark_needs_layout(offstage_key);
    dirty.mark_needs_layout(child_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(375.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let child_bounds = registry
        .get(child_key)
        .unwrap()
        .computed_bounds()
        .expect("child should have bounds");
    assert_eq!(
        child_bounds.width(),
        375.0,
        "child should receive grandparent's width directly through the IndexedStack"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo test_indexed_stack_only_visible_child_is_laid_out test_indexed_stack_offstage_child_not_linked_to_taffy_node test_indexed_stack_index_flip_relays_visible_child test_indexed_stack_visible_child_receives_grandparent_width`
Expected: PASS (the implementation from Tasks 1-2 should make these pass). If any fail, the implementation has a bug — investigate before proceeding.

If the tests PASS on first run, this confirms the implementation is correct. Change the expected: PASS.

- [ ] **Step 3: Run full test suite**

Run: `cargo test -p vexo`
Expected: PASS — all tests green.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/passthrough_integration.rs
git commit -m "test(indexed_stack): integration tests for Flutter-style performLayout

Verifies:
- Only the visible child is laid out (offstage children's Taffy nodes
  are not linked to the stack's node)
- The visible child receives the grandparent's width directly
- Index flip re-links the newly-visible child
- Offstage child's zero-size leaf persists in the engine but is not
  reachable from the root (no computed layout)"
```

---

## Task 5: Verify navigation transition regression test

**Files:**
- Verify: `vexo/src/passthrough_integration.rs` (`test_passthrough_nav_transition_text_does_not_wrap` or similar)

This task verifies the original bug (navigation transition text wrapping, fixed by `76bfc73`) does NOT regress. The `AlignItems::Stretch` workaround on IndexedStack remains in place (per the spec's non-goals), so the regression test should still pass. But the Flutter-style `performLayout` is a stricter correctness improvement — offstage children no longer even participate in the Taffy tree.

- [ ] **Step 1: Run the navigation transition regression test**

Run: `cargo test -p vexo test_passthrough_nav_transition_text_does_not_wrap`
Expected: PASS

If it FAILS, this is a regression. The Flutter-style `performLayout` should make the transition MORE correct, not less. Investigate:
- Is the `Stack` (used in the transition, not `IndexedStack`) still using `ContainerRenderObject`? Yes — only `IndexedStack` was changed. The `Stack` widget is untouched.
- Is the base `IndexedStack` in `vexo_uikit/src/navigation.rs:599` using the new RO? It should be, since `IndexedStack::create_render_object()` now returns `IndexedStackRenderObject`.

- [ ] **Step 2: Run the full desktop demo build**

Run: `cargo build -p desktop_demo`
Expected: PASS — the demo compiles with the new RO.

- [ ] **Step 3: Run the full shared_app build**

Run: `cargo build -p shared_app`
Expected: PASS

- [ ] **Step 4: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: PASS — all tests across all crates green.

- [ ] **Step 5: Commit (if any fixes were needed)**

If no fixes were needed, skip this step. If fixes were needed:

```bash
git add -A
git commit -m "fix(indexed_stack): regression fix for nav transition

<description of what was broken and how it was fixed>"
```

---

## Task 6: Update documentation

**Files:**
- Modify: `docs/vexo_vs_flutter_render_object_architecture.md` (mark the IndexedStack special case as resolved)
- Modify: `docs/superpowers/specs/2026-07-08-pass-through-render-objects-design.md` (mark the deferred item as done)

- [ ] **Step 1: Update the finding doc**

In `docs/vexo_vs_flutter_render_object_architecture.md`, find the "IndexedStack special case" section (around line 151) and the migration plan item 5 (around line 173). Update the status:

Replace (around line 151):
```
3. **IndexedStack special case.** Flutter's `IndexedStack` overrides `performLayout` to lay out only the visible child with parent constraints. Vexo's current `IndexedStack` lays out all children (offstage ones become 0-size leaves). Migration options:
   - **Option A:** Keep all children in the Taffy tree (state preservation) but only feed the visible child's node to Taffy's `compute`. This requires Taffy API support for "skip this node."
   - **Option B:** Detach offstage children's Taffy nodes from the parent (keep the render objects alive, just not linked). This matches Flutter's approach more closely.
```

with:
```
3. **IndexedStack special case.** RESOLVED. `IndexedStack` now uses a dedicated `IndexedStackRenderObject` that filters its Taffy `set_children()` to include only the child at `index` (Option B: offstage children's zero-size leaf nodes are not linked to the stack's Taffy node). This matches Flutter's `RenderIndexedStack.performLayout`. See `vexo/src/render_objects/indexed_stack.rs`.
```

Replace (around line 173):
```
5. **Consider IndexedStack** — Evaluate Option A or B for Flutter-style `performLayout` on `IndexedStack`. This is the highest-impact change but also the most complex.
```

with:
```
5. **IndexedStack Flutter-style performLayout** — DONE. Implemented via `IndexedStackRenderObject`. See plan `docs/superpowers/plans/2026-07-12-indexed-stack-flutter-style-perform-layout.md`.
```

- [ ] **Step 2: Update the pass-through spec doc**

In `docs/superpowers/specs/2026-07-08-pass-through-render-objects-design.md`, find the "Non-goals" section (around line 19). Update:

Replace (around line 19):
```
- **`IndexedStack` Flutter-style `performLayout`** (lay out only the visible child with parent constraints). The finding doc marks this as the most complex change; it is independent of the pass-through migration.
```

with:
```
- **`IndexedStack` Flutter-style `performLayout`** — DONE in a follow-up plan (`docs/superpowers/plans/2026-07-12-indexed-stack-flutter-style-perform-layout.md`). Implemented via a dedicated `IndexedStackRenderObject` that filters `set_children()` to the visible child only.
```

- [ ] **Step 3: Commit**

```bash
git add docs/vexo_vs_flutter_render_object_architecture.md docs/superpowers/specs/2026-07-08-pass-through-render-objects-design.md
git commit -m "docs: mark IndexedStack Flutter-style performLayout as resolved

The IndexedStack special case (deferred in the pass-through render
objects spec) is now implemented via IndexedStackRenderObject."
```

---

## Summary

| Task | Deliverable | Key Verification |
|---|---|---|
| 1 | `IndexedStackRenderObject` — filters `set_children()` to `[child_nodes[index]]` | 5 unit tests: creation, set_index, layout filters, out-of-bounds, index change relays, apply_layout |
| 2 | `IndexedStack` widget wired to new RO; `update_render_object()` calls `set_index()` | 3 widget tests: creates correct RO type, index change → LAYOUT, no change → NONE |
| 3 | Integration tests verified/fixed | Full `cargo test -p vexo` green |
| 4 | 4 integration tests: only visible child laid out, offstage not linked, index flip relays, grandparent width propagates | `cargo test -p vexo test_indexed_stack_*` green |
| 5 | Navigation transition regression verified | `cargo test --workspace` green, desktop_demo + shared_app build |
| 6 | Docs updated | Finding + spec mark IndexedStack as resolved |

## References

- Finding doc: `docs/vexo_vs_flutter_render_object_architecture.md` (section "IndexedStack special case")
- Pass-through spec (non-goals): `docs/superpowers/specs/2026-07-08-pass-through-render-objects-design.md:19`
- Pass-through plan (precedent for RO changes): `docs/superpowers/plans/2026-07-08-pass-through-render-objects.md`
- `vexo/src/widgets/indexed_stack.rs` — current widget (uses `ContainerRenderObject`)
- `vexo/src/render_objects/offstage.rs` — Offstage RO (offstage owns zero-size leaf, onstage is pass-through)
- `vexo/src/render_objects/container.rs` — `ContainerRenderObject` (shared, do NOT modify)
- `vexo/src/layouter.rs` — layouter (unchanged by this plan)
- `vexo/src/elements/container.rs` — `ContainerElement` (reused, handles child reconciliation + mark_needs_layout)
- `vexo/src/elements/offstage.rs:150` — `mark_parent_needs_layout()` on flag flip (already implemented)
- Flutter `RenderIndexedStack`: `packages/flutter/lib/src/widgets/basic.dart` (`performLayout` override)
- Workaround commit: `76bfc73` — `AlignItems::Stretch` on IndexedStack/Stack (kept as defense-in-depth)
- Flag-flip propagation commit: `48e3ae8` — `fix(offstage): propagate needs_layout to parent on flag flip`
