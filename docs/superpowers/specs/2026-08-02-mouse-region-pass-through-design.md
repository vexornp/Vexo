# MouseRegion Pass-Through Render Object

**Date:** 2026-08-02
**Status:** Proposed
**Scope:** `vexo` crate (single file: `vexo/src/widgets/mouse_region.rs`)
**Follows:** `docs/superpowers/specs/2026-07-31-unify-layout-ownership-design.md`
(lines 85–89, 280–284, 392–395 — MouseRegion was deliberately left out of
that refactor, with "a future spec may convert it for consistency.")

## Motivation

`MouseRegionRenderObject` is the last single-child modifier render object
in `vexo` that owns a Taffy container node. Every other invisible
single-child modifier is a true pass-through proxy:

| Render object | File | `is_pass_through()` |
|---|---|---|
| `ProxyRenderObject` | `render_objects/proxy.rs:97` | `true` |
| `DecoratedBoxRenderObject` | `render_objects/decorated_box.rs:101` | `true` |
| `GestureDetectorRenderObject` | `widgets/gesture_detector.rs:496` | `true` |
| `OpacityRenderObject` | `render_objects/opacity.rs:76` | `true` |
| `TransformRenderObject` | `widgets/transform.rs:93` | `true` |
| `ClipRRectRenderObject` | `render_objects/clip_rrect.rs:68` | `true` |
| `FractionalTranslationRenderObject` | `widgets/fractional_translation.rs:118` | `true` |
| **`MouseRegionRenderObject`** | **`widgets/mouse_region.rs:432`** | **`false` (default)** |

The MouseRegion RO currently creates a `FlexDirection::Column +
AlignItems::Stretch` Taffy container (`mouse_region.rs:434-454`) between
its parent and child. This:

- Adds a real layout layer the caller never asked for. Unlike
  `MultiChild`/`Stack`/`Grid`/`WithLayout`, `MouseRegion` exposes no
  `layout` field and no `.with_layout()` — there is no way for the caller
  to see or control the injected `Column + Stretch`. (This is the exact
  footgun the 2026-07-20 ADR removed from the unified layout API.)
- Diverges from its own doc comment (`mouse_region.rs:405-409`), which
  claims "Same as `GestureDetectorRenderObject`." That comment went stale
  when GestureDetector was converted to pass-through on 2026-07-31.
- Forces the painter and hit-tester to take the non-pass-through
  coordinate branch (`painter.rs:266`, `hit_test.rs:387`), adding an
  `position_in_parent` offset that, for an invisible modifier, is the
  only behavioral difference from a pass-through proxy.

The 2026-07-31 spec acknowledged this and deferred: "Converting it is
out of scope (would require a separate audit of `MouseRegion` callers).
Left as-is." This spec performs that audit (see §"Call-site audit") and
finishes the conversion.

## Goals

- Convert `MouseRegionRenderObject` to a true pass-through proxy: `layout()`
  returns the child's node, `apply_layout()` reads the shared node's
  computed bounds, `is_pass_through()` returns `true`.
- Mirror `GestureDetectorRenderObject` (`gesture_detector.rs:465-498`)
  line-for-line. The two are sibling invisible modifiers; they should be
  structurally identical.
- Add a unit test asserting `is_pass_through() == true` (regression guard
  mirroring `test_gesture_detector_render_object_is_pass_through`,
  `gesture_detector.rs:716-724`).
- Add an integration test that verifies cursor resolution works
  end-to-end through the pass-through layer — coverage that has never
  existed for MouseRegion.
- Fix the stale doc comment at `mouse_region.rs:405-409`.

## Non-Goals

- No change to the `MouseRegion` widget API or `MouseRegionElement`.
- No change to the annotation registration pipeline
  (`register_annotation`, `set_cursor_annotation`,
  `remove_cursor_annotation`). Annotation registration is keyed on the RO
  and layout-independent; it works unchanged.
- No change to the `opaque` field or `MouseTrackerAnnotation`
  construction. `opaque` is currently stored but unread by
  `resolve_cursor` / `dispatch_hover_changes` (`input/cursor.rs:37, 53,
  66-69`). That is a pre-existing condition outside this spec's scope.
- No migration of call sites — the audit (§"Call-site audit") confirms
  none rely on the injected `Column + Stretch`.
- No deprecation period. Internal codebase, no external consumers.

## Architecture

### The conversion

`MouseRegionRenderObject` currently (`mouse_region.rs:432-455`):

```rust
fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
    let layout = Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch);
    match self.layout_node {
        Some(existing) => {
            ctx.engine().set_style(existing, &layout);
            ctx.engine().set_children(existing, child_nodes);
            LayoutResult { node: existing, size: Size::zero() }
        }
        None => {
            let node = ctx.engine().create_container(&layout, child_nodes);
            self.layout_node = Some(node);
            LayoutResult { node, size: Size::zero() }
        }
    }
}
// is_pass_through() not overridden → defaults to false
```

After conversion (mirroring `gesture_detector.rs:465-498`):

```rust
fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
    // Pass-through: return the child's node directly. No intervening
    // container — the grandparent links the grandchild's Taffy node.
    match child_nodes.first() {
        Some(&child_node) => {
            self.layout_node = Some(child_node);
            LayoutResult { node: child_node, size: Size::zero() }
        }
        None => {
            let node = ctx.engine().create_leaf(&Layout::default());
            self.layout_node = Some(node);
            LayoutResult { node, size: Size::zero() }
        }
    }
}

fn is_pass_through(&self) -> bool { true }
```

Everything else in the `RenderObject` impl is unchanged in body:

- **`apply_layout()`** (`mouse_region.rs:457-463`) — reads
  `self.layout_node` from the engine. Pre-conversion that node was the
  RO's own container; post-conversion it's the child's node. Either way
  `self.computed_bounds` is populated. The body is identical.
- **`hit_test()`** (`mouse_region.rs:469-474`) — bounds-based check
  against `self.computed_bounds`. Unchanged. Post-conversion,
  `computed_bounds` equals the child's bounds (same Taffy node), so the
  MouseRegion is hit exactly when its child is hit — which is precisely
  what keeps its annotation in the hit path.
- **`paint()`** (`mouse_region.rs:465-467`) — returns `vec![]`.
  Unchanged.
- **`children()` / `set_child_id()` / `replace_child()` / `layout_node()`
  / `computed_bounds()`** — all unchanged.

### Why cursor/hover still works

The cursor/hover pipeline depends on two things, neither of which
involves layout ownership:

1. **The RO is in the hit path.** Hit testing (`hit_test.rs:309-323`)
   adds an RO to the path when its `computed_bounds` contains the
   pointer. A pass-through MouseRegion shares the child's
   `computed_bounds` (same Taffy node, read in `apply_layout`), so it is
   in the path exactly when the child is.
2. **The annotation is registered on the RO.** Registration happens in
   `MouseRegionElement::register_annotation`
   (`mouse_region.rs:212-230`) via
   `RenderObjectRegistry::set_cursor_annotation(ro_key, annotation)`,
   keyed on the RO — completely layout-independent.

Annotation *collection* walks the hit path
(`hit_test.rs:230-241`):

```rust
let annotations: Vec<(ElementKey, MouseTrackerAnnotation)> = result
    .path()
    .iter()
    .filter_map(|&ro_key| {
        let annotation = self.cursor_annotation(ro_key).cloned()?;
        let element_key = self.element_for(ro_key)?;
        Some((element_key, annotation))
    })
    .collect();
```

Cursor *resolution* (`mouse_tracker.rs:49-60`) walks the annotations
deepest-first and applies Flutter's `firstNonDeferred()` semantics.
Hover enter/exit (`mouse_tracker.rs:75-104`) compares the new annotation
set against the previous frame's set and fires `on_enter`/`on_exit` by
element-key set membership.

None of these stages read `computed_bounds` or care whether the RO owns
its Taffy node. This is the identical mechanism `GestureDetector` (which
carries no annotation but participates in the same hit path) and
`DecoratedBox` (which carries paint, not annotations) already use
successfully as pass-through proxies.

### Coordinate correction

The pass-through conversion activates the coordinate-correction branch in
two places:

- **Painter** (`painter.rs:266-273`) — when recursing into a
  pass-through RO's child, `position_in_parent` is *subtracted* from the
  accumulated absolute position so the child's own equal
  `position_in_parent` cancels out. Without this, the shared origin would
  be double-counted.
- **Hit-tester** (`hit_test.rs:380-394`) — the same correction, so a
  pointer inside the child is correctly localized for both ROs in the
  shared-node case.

Both corrections are gated on `is_pass_through() == true` and already
exist for `GestureDetector`/`DecoratedBox`/etc. MouseRegion simply joins
the set of ROs that take this branch.

### Reconciler interaction

When a child is replaced beneath a pass-through RO, the reconciler
(`reconciler.rs:970-990`) walks up through the chain of pass-through ROs,
marking each plus the first non-pass-through ancestor as needing layout.
This is necessary because the pass-through RO doesn't own a node to
re-link — the nearest owning ancestor does. A pass-through MouseRegion
participates in this walk correctly by virtue of `is_pass_through() ==
true`; no additional change is needed.

### Registry cleanup

`RenderObjectRegistry::remove` (`render_object.rs:493-499`) skips
orphan-node cleanup for pass-through ROs, because the node is borrowed
from the child (the child owns it and will clean it up). Without
`is_pass_through() == true`, removing a pass-through MouseRegion would
incorrectly orphan the child's node. The flag flip is mandatory.

## Call-site audit

Every `MouseRegion::new` and trait-extension (`.cursor()` / `.on_enter()`
/ `.on_exit()`) call site was inspected for reliance on the injected
`Column + Stretch`. None depend on it.

| Call site | Construction | Audit |
|---|---|---|
| `vexo/src/widgets/text_edit.rs:605` | `MouseRegion::new(DecoratedBox(WithLayout(content, padding(8.0))))` | The child is an already-self-sizing `DecoratedBox` wrapping a `WithLayout`-padded `TextEditContent`. The padding and content sizing are explicit; MouseRegion's `Column + Stretch` is inert. **No regression.** |
| `vexo_uikit/src/button.rs:291-296` | `MultiChild(...).on_press().on_tap().on_release().on_enter().on_exit().opacity(...)`, then outer `WithLayout(..., Layout::default().align_self(Start))` | The `MultiChild` owns its sizing; the outer `WithLayout` controls the button's slot sizing. MouseRegion wraps an already-sized subtree. **No regression.** |
| `vexo/src/widgets/mod.rs:497` | `Text::new("Hover").cursor(Pointer)` (test) | `Text` self-sizes. Test asserts type only. **No regression.** |
| `vexo/src/stateful_integration_test.rs:1259, 1521` | `.on_enter()/.on_exit()` chained on already-sized subtrees (test) | Test code, no layout assertions. **No regression.** |

The pattern is uniform: MouseRegion is always the outermost wrapper on an
already-sized subtree. No call site uses MouseRegion as the *primary*
layout container for an intrinsic-size child. The conversion is safe.

## Testing

### New unit tests

**`vexo/src/widgets/mouse_region.rs`** — add a `#[cfg(test)] mod tests`
block (the file currently has none), mirroring `gesture_detector.rs:716-744`:

- `test_mouse_region_render_object_is_pass_through` —
  `MouseRegion::new(Text::new("x")).create_render_object().is_pass_through()
  == true`. Regression guard.
- `test_mouse_region_layout_returns_child_node` — construct a
  `MouseRegionRenderObject`, call `layout()` with a single child node,
  assert `result.node == child_node` and `ro.layout_node() == Some(child_node)`.
  Mirrors `test_gesture_detector_layout_returns_child_node`
  (`gesture_detector.rs:727-744`).

These tests construct the RO directly and call methods on it — no
registry access needed.

### New integration test

**`vexo/src/integration_tests.rs`** — this file already constructs
`ElementRegistry` + `RenderObjectRegistry` manually (`integration_tests.rs:13-16`)
and has `use super::*`; it is the natural home for a test that needs the
full hit-test → annotation-collection → cursor-resolution path.

- `test_mouse_region_cursor_resolution_through_pass_through` —
  1. Build a `MouseRegion::new(child).cursor(System(Pointer))` subtree
     where `child` has known, finite bounds (e.g. a `WithLayout` with
     explicit width/height, or a `Text` with known metrics).
  2. Mount the subtree into a manually-constructed `ElementRegistry` +
     `RenderObjectRegistry` (following the `integration_tests.rs:13-16`
     harness pattern).
  3. Run one layout pass.
  4. Hit-test a point inside the child's bounds via
     `RenderObjectRegistry::hit_test`.
  5. Assert: the MouseRegion RO is in `result.path()`.
  6. Assert: `result.annotations()` contains the MouseRegion's
     annotation, with `cursor == System(Pointer)`.
  7. Assert: `MouseTracker::resolve_cursor(&result.annotations()) ==
     SystemCursorKind::Pointer`.

This test directly verifies MouseRegion's actual purpose — cursor
declaration — end-to-end through the pass-through layer. It is the
strongest guard against a regression that the unit test would miss (e.g.
the annotation being silently dropped because the RO fell out of the hit
path). No equivalent test exists today for any pass-through RO; this
establishes the precedent.

### Tests that must still pass (regression guards)

- All existing `mouse_region.rs` tests — they exercise the widget, element,
  annotation registration, and unmount paths, none of which change.
- `vexo_uikit/tests/button_render_tests.rs:149` — explicitly mentions
  "pass-through wrappers (GestureDetector, MouseRegion) between the
  `DecoratedBox` and the root." Pre-conversion this comment was
  aspirational for MouseRegion; post-conversion it becomes accurate.
  **Still passes** (the test counts ROs by type, not by pass-through
  flag).
- `vexo/src/stateful_integration_test.rs:1198` — exercises nested
  `GestureDetector + MouseRegion` wrappers. **Still passes**; both are
  now pass-through, the test exercises element-tree shape, not layout
  ownership.

### Compile-test

The strongest structural guard is `cargo build -p vexo`. The conversion
drops `FlexDirection` and `AlignItems` from the
`use crate::layout::{...}` import (they were only used in the old
`layout()` body); the import becomes
`use crate::layout::{Layout, LayoutNodeKey};`, matching
`gesture_detector.rs:39` exactly. `Layout` stays — it's used in the
no-child branch (`Layout::default()`) and in tests. `LayoutNodeKey` stays
as the `layout_node` field type.

### GUI validation (required, user-run)

Per `CLAUDE.md`, the agent does not run `cargo run -p desktop_demo`.
Cursor resolution is the one behavioral risk. The user must run:

- **TextEdit cursor**: hover over the text field in the demo → cursor
  becomes `Text` (I-beam). Exercises `text_edit.rs:605`.
- **Button hover**: hover over a button → `on_enter` fires (button
  highlights), hover out → `on_exit` fires (button resets). Exercises
  `button.rs:291-296`.
- **Pointer cursor**: hover over any `.cursor(Pointer)` widget → cursor
  becomes a hand. Exercises `mod.rs:497`-style call sites.

If any regress, the symptom is "cursor doesn't change on hover" or
"hover callbacks don't fire." Per the `debugging-gui-with-logs` workflow,
the fix path is: add `log::debug!` in
`MouseRegionRenderObject::apply_layout` printing `computed_bounds`, run
with `RUST_LOG=debug | grep mouse_region`, and inspect the bounds. The
expected bounds match the child's bounds (same Taffy node).

## File-Level Change Summary

| File | Change |
|---|---|
| `vexo/src/widgets/mouse_region.rs` | **Convert** `MouseRegionRenderObject` to pass-through: rewrite `layout()` to return child's node (mirror `gesture_detector.rs:465-486`), add `is_pass_through() == true` (mirror `gesture_detector.rs:496-498`). **Drop** unused imports `FlexDirection`, `AlignItems` (import becomes `use crate::layout::{Layout, LayoutNodeKey};`, matching `gesture_detector.rs:39`). **Fix** stale doc comment at `:405-409`. **Add** `#[cfg(test)] mod tests` block with `test_mouse_region_render_object_is_pass_through` + `test_mouse_region_layout_returns_child_node` unit tests. |
| `vexo/src/integration_tests.rs` | **Add** `test_mouse_region_cursor_resolution_through_pass_through` — mounts a `MouseRegion`-wrapped subtree into a manually-constructed registry, hit-tests, and asserts cursor resolution returns the declared cursor. Uses the existing `integration_tests.rs:13-16` harness pattern. |

Total: two files changed, ~25 lines of production code modified, ~100
lines of test code added.

## Resolved Decisions

1. **Mirror GestureDetector exactly** → The two ROs are sibling invisible
   modifiers; structural identity makes them easiest to reason about.
   Approach B (rename `layout_node` → `child_layout_node` to match
   `ProxyRenderObject`) was rejected because it would introduce a naming
   split from GestureDetector without behavioral benefit.
2. **No shared pass-through helper/trait** → Over-engineering for a
   single-field struct. YAGNI.
3. **Keep `opaque` field unchanged** → It is currently unread by the
   cursor-resolution pipeline. Making it functional is a separate
   concern (Flutter's `MouseRegion.opaque` controls whether the region
   blocks annotations from ROs *behind* it, which requires a more
   sophisticated hit-test traversal than this codebase currently does).
4. **No call-site migration** → The audit (§"Call-site audit") confirms
   none of the four call sites rely on the injected `Column + Stretch`.
   This is the audit the 2026-07-31 spec deferred; it is now complete.
5. **Integration test for cursor resolution** → The unit test alone
   (mirroring GestureDetector) would not catch a regression where the
   annotation is silently dropped from the hit path. Since MouseRegion's
   entire purpose is cursor/hover — unlike GestureDetector, whose purpose
   is gesture routing — an end-to-end cursor-resolution test is the
   correct verification scope.
6. **No deprecation period** → Internal codebase, no external consumers.
