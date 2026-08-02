# MouseRegion Pass-Through Render Object — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert `MouseRegionRenderObject` from owning a `Column + Stretch` Taffy container to a true pass-through proxy that mirrors `GestureDetectorRenderObject`, then verify cursor resolution still works end-to-end through the pass-through layer.

**Architecture:** Replace `MouseRegionRenderObject::layout()` so it returns the child's Taffy node directly (no intervening container), add `is_pass_through() -> true` so the registry/painter/hit-tester/reconciler take the pass-through branches, and drop the now-unused `FlexDirection`/`AlignItems` imports. The annotation pipeline (keyed on the RO, collected from the hit path) is layout-independent and needs no changes. An end-to-end integration test verifies the cursor/hover pipeline still resolves the declared cursor through the shared node.

**Tech Stack:** Rust, Taffy 0.11 (layout engine), Vexo three-tree architecture (widget → element → render object), `glyphon` FontSystem (test harness).

**Spec:** `docs/superpowers/specs/2026-08-02-mouse-region-pass-through-design.md`

## Global Constraints

- **Mirror `GestureDetectorRenderObject` (`vexo/src/widgets/gesture_detector.rs:437-544`) line-for-line** — the two are sibling invisible modifiers and must be structurally identical post-conversion.
- **No changes to `MouseRegion` widget, `MouseRegionElement`, or the annotation pipeline** (`register_annotation`, `set_cursor_annotation`, `remove_cursor_annotation`). The spec's §"Non-Goals" forbids it.
- **No call-site migration** — the spec's §"Call-site audit" confirmed none of the four call sites rely on the injected `Column + Stretch`.
- **Import line after conversion:** `use crate::layout::{Layout, LayoutNodeKey};` (matches `gesture_detector.rs:39`). `FlexDirection` and `AlignItems` drop; `Layout` stays (used in the no-child branch + tests); `LayoutNodeKey` stays (the `layout_node` field type).
- **Per `CLAUDE.md`:** Never run `cargo run -p desktop_demo` — the agent cannot interact with the GUI. GUI validation is a user-run step (documented in the spec, not automated in this plan).
- **Per `CLAUDE.md`:** Always run `cargo build` after editing Rust files, and `cargo test` after implementing features. Never assume tests pass without running them.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `vexo/src/widgets/mouse_region.rs` | MouseRegion widget, element, render object | Convert RO to pass-through; drop 2 imports; fix stale doc comment; add `#[cfg(test)] mod tests` block with 2 unit tests |
| `vexo/src/integration_tests.rs` | Cross-module integration tests (has pipeline + registry harness) | Add 1 integration test for cursor resolution through pass-through |

---

### Task 1: Convert MouseRegionRenderObject to pass-through (TDD)

**Files:**
- Modify: `vexo/src/widgets/mouse_region.rs` (lines 30, 405-455, 469-474 area)
- Test: `vexo/src/widgets/mouse_region.rs` (new `#[cfg(test)] mod tests` block at end of file)

**Interfaces:**
- Consumes: `crate::layout::{Layout, LayoutNodeKey}` (already imported; `FlexDirection`/`AlignItems` removed), `crate::core::{Bounds, Logical, Point, Size}` (already imported), the `RenderObject` trait (already implemented), `crate::layout::TaffyLayoutEngine` + `crate::widgets::create_test_font_system` pattern from `gesture_detector.rs:562` for the unit tests.
- Produces: a `MouseRegionRenderObject` where `is_pass_through() == true` and `layout()` returns the child's node. Downstream consumers (registry cleanup at `render_object.rs:493-499`, painter coordinate correction at `painter.rs:266-273`, hit-tester coordinate correction at `hit_test.rs:380-394`, reconciler walk at `reconciler.rs:970-990`) automatically take the pass-through branches.

**Reference:** The target `layout()` and `is_pass_through()` bodies already exist verbatim at `vexo/src/widgets/gesture_detector.rs:465-498`. Copy them exactly.

- [ ] **Step 1: Write the failing unit tests**

Add a `#[cfg(test)] mod tests` block at the end of `vexo/src/widgets/mouse_region.rs` (the file currently has no test module). Include the `create_test_font_system` helper mirroring `gesture_detector.rs:562`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Size;
    use crate::layout::{Layout, TaffyLayoutEngine};
    use crate::widgets::Text;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_mouse_region_render_object_is_pass_through() {
        let widget = MouseRegion::new(Text::new("Hello"));
        let ro = widget.create_render_object();
        assert!(
            ro.is_pass_through(),
            "MouseRegion's render object must be pass-through"
        );
    }

    #[test]
    fn test_mouse_region_layout_returns_child_node() {
        let mut ro = MouseRegionRenderObject::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        // Create a child Taffy node the way the pipeline would: by calling
        // engine.create_leaf and passing the key as a child_nodes entry.
        let child_node = ctx
            .engine()
            .create_leaf(&Layout::default().width(50.0).height(50.0));
        let result = ro.layout(&mut ctx, &[child_node]);

        assert_eq!(
            result.node, child_node,
            "layout() must return the child's node (pass-through)"
        );
        assert_eq!(
            ro.layout_node(),
            Some(child_node),
            "layout_node() must return the child's node after layout()"
        );
    }

    #[test]
    fn test_mouse_region_layout_no_child_creates_throwaway_node() {
        let mut ro = MouseRegionRenderObject::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = ro.layout(&mut ctx, &[]);

        // Should not panic; should return some node and store it.
        assert!(ro.layout_node().is_some());
        assert_eq!(ro.layout_node(), Some(result.node));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vexo --lib widgets::mouse_region::tests`
Expected: FAIL. `test_mouse_region_render_object_is_pass_through` fails with an `assert!` panic ("MouseRegion's render object must be pass-through") because `is_pass_through()` currently defaults to `false`. The two `layout_returns_child_node` / `layout_no_child` tests will fail because `layout()` currently returns the RO's own container node, not the child's node.

If the tests fail to *compile* (e.g. `LayoutContext::new` signature drift, missing import), fix the compile error first — the test bodies mirror `gesture_detector.rs:727-766` exactly, so any compile error signals a drift between this file and GestureDetector that the implementer should reconcile before proceeding.

- [ ] **Step 3: Rewrite `layout()` to return the child's node**

In `vexo/src/widgets/mouse_region.rs`, replace the entire body of `MouseRegionRenderObject::layout()` (currently lines ~433-455) with the pass-through version. The replacement must match `gesture_detector.rs:465-486` exactly.

Old body (to replace):
```rust
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch);
        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &layout);
                ctx.engine().set_children(existing, child_nodes);
                LayoutResult {
                    node: existing,
                    size: Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_container(&layout, child_nodes);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::zero(),
                }
            }
        }
    }
```

New body:
```rust
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // Pass-through: return the child's node directly. No intervening
        // container — the grandparent links the grandchild's Taffy node.
        // Mirrors `GestureDetectorRenderObject::layout`.
        match child_nodes.first() {
            Some(&child_node) => {
                self.layout_node = Some(child_node);
                LayoutResult {
                    node: child_node,
                    size: Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_leaf(&Layout::default());
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::zero(),
                }
            }
        }
    }
```

- [ ] **Step 4: Add `is_pass_through() -> true`**

In the same `impl RenderObject for MouseRegionRenderObject` block, add the `is_pass_through` override. Place it immediately after `apply_layout` (matching `gesture_detector.rs:496-498`'s placement). Insert:

```rust
    fn is_pass_through(&self) -> bool {
        true
    }
```

- [ ] **Step 5: Drop unused imports**

In `vexo/src/widgets/mouse_region.rs` line 30, change:

```rust
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey};
```

to:

```rust
use crate::layout::{Layout, LayoutNodeKey};
```

`Layout` stays (used in the new `layout()` no-child branch as `Layout::default()` and in the new test as `Layout::default().width(..).height(..)`). `LayoutNodeKey` stays (the `layout_node` field type). `FlexDirection` and `AlignItems` were only used in the old `layout()` body and are now unused.

- [ ] **Step 6: Fix the stale doc comment**

In `vexo/src/widgets/mouse_region.rs`, the doc comment on `MouseRegionRenderObject` (lines ~405-409) currently reads:

```rust
/// Pass-through render object for MouseRegion - invisible.
///
/// Same as GestureDetectorRenderObject: delegates layout to child,
/// generates no paint commands, hit tests using computed bounds.
/// The annotation lives on the registry, not on this render object.
```

That comment went stale when GestureDetector was converted on 2026-07-31 and is only now becoming accurate again. Update the second sentence to reflect the now-true state and call out the annotation mechanism:

```rust
/// Pass-through render object for MouseRegion - invisible.
///
/// Mirrors `GestureDetectorRenderObject`: `layout()` returns the child's
/// node directly (no intervening Taffy container), `apply_layout` adopts
/// the shared node's computed bounds, `paint()` generates no commands,
/// `hit_test()` uses the adopted bounds. The `MouseTrackerAnnotation`
/// lives on the `RenderObjectRegistry` (keyed on this RO), not on the RO
/// itself — it is registered by `MouseRegionElement::register_annotation`
/// during mount and collected from the hit path during cursor resolution.
```

- [ ] **Step 7: Run the unit tests to verify they pass**

Run: `cargo test -p vexo --lib widgets::mouse_region::tests`
Expected: PASS — all three tests (`test_mouse_region_render_object_is_pass_through`, `test_mouse_region_layout_returns_child_node`, `test_mouse_region_layout_no_child_creates_throwaway_node`).

- [ ] **Step 8: Build the workspace and run the full vexo test suite**

Run: `cargo build -p vexo`
Expected: Builds with no warnings about unused imports (if `FlexDirection`/`AlignItems` survived, this would warn).

Run: `cargo test -p vexo`
Expected: All tests pass. Pay particular attention to:
- `widgets::gesture_detector::tests` (pass-through sibling — should be unaffected)
- `stateful_integration_test` (exercises nested `GestureDetector + MouseRegion` wrappers per `stateful_integration_test.rs:1198`)
- `hit_test` (the coordinate-correction branch is now active for MouseRegion)
- `integration_tests` (broad pipeline tests)

If any test fails, do NOT proceed — investigate the regression. The most likely failure mode is a hit-test miss caused by the pass-through coordinate correction interacting with a call site that previously relied on the old `Column + Stretch` container bounds. The spec's §"Call-site audit" confirmed this shouldn't happen, but a test failure here is the place to catch it.

- [ ] **Step 9: Commit**

```bash
git add vexo/src/widgets/mouse_region.rs
git commit -m "refactor(mouse_region): convert render object to pass-through

Mirror GestureDetectorRenderObject (converted 2026-07-31): layout()
returns the child's Taffy node directly instead of creating a
Column+Stretch container, and is_pass_through() returns true so the
registry/painter/hit-tester/reconciler take the pass-through branches.

Completes the symmetry: every single-child modifier RO in vexo is now
a true pass-through proxy. Finishes the deferred audit from the
2026-07-31 unify-layout spec.

The annotation pipeline (keyed on the RO, collected from the hit path)
is layout-independent and unchanged."
```

---

### Task 2: Integration test for cursor resolution through the pass-through layer

**Files:**
- Test: `vexo/src/integration_tests.rs` (add to the existing `mod tests` at the end of the file, around line 537)

**Interfaces:**
- Consumes: `crate::ThreeTreePipeline`, `crate::ThreeTreePipeline::new`, `crate::ThreeTreePipeline::reconcile`, `crate::ThreeTreePipeline::layout`, `crate::ThreeTreePipeline::hit_test`, `crate::MouseTracker::resolve_cursor`, `crate::MouseCursor`, `crate::SystemCursorKind`, `crate::widgets::MouseRegion` (pub(crate)), `crate::widgets::Text`, `crate::layout::Layout`, `crate::WithLayout`, `crate::core::{Absolute, Logical, Position, Size}`. All of these are already in scope in `integration_tests.rs` via `use super::*;` and the explicit `use` statements at the top of the test module (see `integration_tests.rs:450-456`).
- Produces: a regression guard proving the cursor/hover pipeline resolves the declared cursor through a pass-through MouseRegion. This is the first end-to-end test of MouseRegion's actual purpose; it establishes the precedent for testing pass-through ROs that carry annotations.

**Reference patterns:**
- Pipeline mount + layout + hit-test: `integration_tests.rs:117-144` (`test_hit_test_through_pipeline`)
- Downcast-by-type assertion on the hit path: `integration_tests.rs:526-534` (`test_scroll_view_cross_axis_stretching`)
- `MouseTracker::resolve_cursor` signature: `mouse_tracker.rs:49` — takes `&[(ElementKey, MouseTrackerAnnotation)]`, returns `SystemCursorKind`
- `HitTestResult::annotations()`: `hit_test.rs:164` — returns `&[(ElementKey, MouseTrackerAnnotation)]`

- [ ] **Step 1: Write the failing integration test**

Append a new test to the `mod tests` block at the end of `vexo/src/integration_tests.rs`. The test mounts a `MouseRegion`-wrapped sized child, hit-tests a point inside the child, and asserts cursor resolution returns `Pointer`.

```rust
    #[test]
    fn test_mouse_region_cursor_resolution_through_pass_through() {
        use crate::MouseTracker;
        use crate::input::{MouseCursor, SystemCursorKind};
        use crate::widgets::MouseRegion;

        let mut pipeline: ThreeTreePipeline =
            ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // MouseRegion wrapping a sized WithLayout(Text) child. The child
        // is 100x50 at the origin; MouseRegion is pass-through and shares
        // those bounds (same Taffy node), so it's in the hit path for any
        // point inside (50, 25).
        let child = WithLayout::new(
            Text::new("Hover me"),
            Layout::default().width(100.0).height(50.0),
        );
        let widget = MouseRegion::new(child)
            .cursor(MouseCursor::System(SystemCursorKind::Pointer));

        pipeline.reconcile(Box::new(widget));
        pipeline.layout(
            CoreSize::new(800.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Hit-test the center of the child's bounds.
        let hit_pos = Position::<Logical, Absolute>::new(50.0, 25.0);
        let result = pipeline.hit_test(hit_pos);

        // 1. The hit must land — sanity check the test setup.
        assert!(
            result.is_hit(),
            "hit test at (50, 25) must hit the MouseRegion-wrapped child"
        );

        // 2. The MouseRegion's annotation must be in the collected
        //    annotations (the pass-through layer must not have dropped it
        //    from the hit path).
        let has_pointer_annotation = result
            .annotations()
            .iter()
            .any(|(_, annotation)| {
                annotation.cursor == MouseCursor::System(SystemCursorKind::Pointer)
            });
        assert!(
            has_pointer_annotation,
            "MouseRegion's Pointer cursor annotation must be in the hit-test \
             annotations; got {} annotations: {:?}",
            result.annotations().len(),
            result
                .annotations()
                .iter()
                .map(|(_, a)| a.cursor)
                .collect::<Vec<_>>(),
        );

        // 3. Cursor resolution must return Pointer (the declared cursor),
        //    proving the pass-through layer preserves the cursor pipeline.
        let resolved = MouseTracker::resolve_cursor(result.annotations());
        assert_eq!(
            resolved,
            SystemCursorKind::Pointer,
            "resolve_cursor must return Pointer through the pass-through MouseRegion"
        );
    }
```

- [ ] **Step 2: Run the integration test to verify it passes**

Run: `cargo test -p vexo --lib integration_tests::tests::test_mouse_region_cursor_resolution_through_pass_through`
Expected: PASS.

This test is written *after* Task 1's conversion (the test asserts the pass-through behavior is correct, not that the old behavior fails), so it should pass on the first run. If it fails, the most likely cause is that the hit point `(50, 25)` is not actually inside the child's laid-out bounds — the test should print the actual annotations (the `assert!` message already does) and the implementer should adjust the hit point or the `WithLayout` sizing. A failure of assertion #2 (annotation missing from path) would indicate the pass-through RO fell out of the hit path — that would be a real regression in Task 1 and requires going back to investigate `MouseRegionRenderObject::apply_layout` and the hit-tester's pass-through coordinate correction (`hit_test.rs:380-394`).

- [ ] **Step 3: Run the full vexo test suite to verify no regressions**

Run: `cargo test -p vexo`
Expected: All tests pass, including the new integration test and all pre-existing tests.

- [ ] **Step 4: Build the full workspace**

Run: `cargo build --workspace`
Expected: Builds cleanly. This catches any downstream breakage in `shared_app` / `vexo_uikit` / `desktop_demo` from the MouseRegion RO change (the spec's audit says there should be none, but this is the final guard).

- [ ] **Step 5: Commit**

```bash
git add vexo/src/integration_tests.rs
git commit -m "test(mouse_region): add cursor-resolution integration test

End-to-end test that mounts a MouseRegion-wrapped sized child, hit-tests
a point inside the child, and asserts the MouseRegion's Pointer cursor
annotation is collected on the hit path and that resolve_cursor returns
Pointer.

First integration test of MouseRegion's actual purpose (cursor/hover)
through the pass-through layer. Establishes the precedent for testing
pass-through ROs that carry annotations — GestureDetector has gesture
routing, MouseRegion has cursor resolution, both now have end-to-end
coverage."
```

---

## Self-Review

**1. Spec coverage:**
- §"Goals" — "Convert `MouseRegionRenderObject` to a true pass-through proxy" → Task 1, Steps 3-4. ✓
- §"Goals" — "Mirror `GestureDetectorRenderObject` line-for-line" → Task 1 copies `gesture_detector.rs:465-486` and `:496-498` verbatim. ✓
- §"Goals" — "Add a unit test asserting `is_pass_through() == true`" → Task 1, Step 1 (`test_mouse_region_render_object_is_pass_through`). ✓
- §"Goals" — "Add an integration test that verifies cursor resolution works end-to-end" → Task 2. ✓
- §"Goals" — "Fix the stale doc comment at `mouse_region.rs:405-409`" → Task 1, Step 6. ✓
- §"Non-Goals" — no widget/element/annotation-pipeline changes → Task 1 only touches the RO impl, imports, and doc comment; Task 2 only adds a test. ✓
- §"Non-Goals" — no `opaque` field changes → not touched. ✓
- §"Non-Goals" — no call-site migration → no call sites touched. ✓
- §"Call-site audit" — verified by the workspace build in Task 2 Step 4. ✓
- §"Testing" — "New unit tests" (`test_mouse_region_render_object_is_pass_through`, `test_mouse_region_layout_returns_child_node`) → Task 1, Step 1. The spec mentions two unit tests; the plan adds a third (`test_mouse_region_layout_no_child_creates_throwaway_node`) mirroring `gesture_detector.rs:752-765` — this is consistent with "mirror GestureDetector exactly" and adds a no-regression guard for the no-child branch. ✓
- §"Testing" — "New integration test" (`test_mouse_region_cursor_resolution_through_pass_through`) → Task 2. ✓
- §"Testing" — "Tests that must still pass" → Task 1 Step 8 and Task 2 Step 3 run the full `cargo test -p vexo`. ✓
- §"Testing" — "Compile-test as the primary guard" → Task 1 Step 8 (`cargo build -p vexo`) and Task 2 Step 4 (`cargo build --workspace`). ✓
- §"Testing" — "GUI validation (required, user-run)" → Not automated in this plan (per `CLAUDE.md`, the agent cannot run `cargo run -p desktop_demo`). The spec documents the user-run steps; the implementer should flag this to the user at handoff. ✓

**2. Placeholder scan:** No "TBD", "TODO", "implement later", "add error handling", "similar to Task N", or undescribed steps. Every code step contains the full code to write. Every command step contains the exact command and expected output. ✓

**3. Type consistency:**
- `MouseRegionRenderObject::new()` — used in Task 1 test, defined at `mouse_region.rs:416`. ✓
- `MouseRegion::new(child)` — used in Task 1 test and Task 2 test, defined at `mouse_region.rs:61`. ✓
- `MouseRegion::cursor(MouseCursor)` — used in Task 2 test, defined at `mouse_region.rs:79`. ✓
- `Widget::create_render_object()` — used in Task 1 test, trait method, returns `Box<dyn RenderObject>`. ✓
- `RenderObject::is_pass_through()` — asserted in Task 1 test, added in Task 1 Step 4. ✓
- `RenderObject::layout()` — called in Task 1 test, rewritten in Task 1 Step 3. Signature matches `gesture_detector.rs:465`. ✓
- `RenderObject::layout_node()` — asserted in Task 1 test, returns `Option<LayoutNodeKey>`, unchanged. ✓
- `LayoutContext::new(&mut engine, &mut font_system)` — used in Task 1 test, mirrors `gesture_detector.rs:732`. ✓
- `TaffyLayoutEngine::new()` — used in Task 1 test, mirrors `gesture_detector.rs:730`. ✓
- `pipeline.reconcile(Box::new(widget))` — used in Task 2 test, mirrors `integration_tests.rs:110,126,153,486`. ✓
- `pipeline.layout(CoreSize::new(..), &mut engine, &mut font_system)` — used in Task 2 test, mirrors `integration_tests.rs:487`. (`CoreSize` is the alias imported at `integration_tests.rs:451`.) ✓
- `pipeline.hit_test(Position::<Logical, Absolute>::new(..))` — used in Task 2 test, mirrors `integration_tests.rs:520`. ✓
- `result.is_hit()`, `result.annotations()` — used in Task 2 test, defined at `hit_test.rs:164` and elsewhere. ✓
- `MouseTracker::resolve_cursor(&[(ElementKey, MouseTrackerAnnotation)])` — used in Task 2 test, signature at `mouse_tracker.rs:49`. ✓
- `MouseCursor::System(SystemCursorKind::Pointer)` — used in Task 2 test, defined at `input/cursor.rs:27` and `:8`. ✓
- `WithLayout::new(child, Layout::default().width(..).height(..))` — used in Task 2 test, mirrors `integration_tests.rs:476,481-484`. ✓

No type drift detected. ✓

**4. Scope check:** The plan covers exactly the spec's scope — one production file change (the RO conversion + cleanup) and one test file change (integration test). No sub-project decomposition needed. The two tasks are independently testable: Task 1 produces a converted RO with unit-test coverage; Task 2 adds end-to-end coverage that builds on Task 1. ✓

---
