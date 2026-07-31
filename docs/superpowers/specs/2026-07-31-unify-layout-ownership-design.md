# Unify Layout Ownership — Remove GestureDetector's Layout Field

**Date:** 2026-07-31
**Status:** Proposed
**Scope:** `vexo`, `shared_app`, `vexo_uikit` crates
**Supersedes (partially):** Resolves the open-ended "GestureDetector is
safe, no migration needed" carve-out in
`docs/superpowers/specs/2026-07-20-remove-widgetext-layout-methods-design.md`
(lines 191, 211–216, 251–256).

## Motivation

Vexo's layout API was unified on 2026-07-20 around one rule: **layout is
always an explicit constructor parameter, never a silent wrapping trait
method.** After that change, the only ways to introduce a Taffy layout
node are:

- `MultiChild::new(children, layout)` / `MultiChild::empty(layout)` —
  multi-child container.
- `Stack::new()` (default layout) + `.with_layout(layout)` (replace) —
  multi-child container.
- `Grid::new()` + `.with_layout(layout)` — multi-child container.
- `WithLayout::new(child, layout)` — single-child wrapper.
- `GestureDetector::new(child)` + `.with_layout(layout)` — **the lone
  exception.** A single-child widget that owns a `layout: Layout` field
  and a Taffy container node.

`GestureDetector` breaks the rule. The 2026-07-20 spec acknowledged this
but declined to migrate it, citing the detector's inherent method as
"safe, no wrapping." That framing is incomplete: the detector doesn't
*wrap* in `WithLayout`, but it does *own* a layout node — which is the
very thing single-child widgets were not supposed to do under the unified
model. The detector also currently owns a Taffy *container* node
(`gesture_detector.rs:504`, `create_container`) for hit-testing, while
every other single-child modifier (`DecoratedBox`, `Opacity`, `Transform`,
`ClipRRect`, `FractionalTranslation`) is a true pass-through proxy.

This spec finishes the symmetry: **single-child widgets never own a
`Layout` — use `WithLayout` to add one.** GestureDetector becomes a
pass-through proxy (consistent with `DecoratedBox`), and `WithLayout`
gains an inherent `.with_layout()` so every layout-owning widget exposes
the same replace-layout API.

### Why the detector owned a layout node

The original design used the detector's own Taffy container bounds for
hit-testing. In the tab bar (`vexo_uikit/src/tab_bar.rs:184`), the
detector is the equal-width slot (`flex_grow(1.0)`), so its container
bounds = the full slot, and tap-anywhere-in-slot works. Converting to
pass-through means the detector must adopt the *child's* bounds instead.
The migration preserves full-slot hit-testing by moving `.with_layout(L)`
from the detector onto the content: the `WithLayout` wrapper now owns the
slot node, the detector adopts the wrapper's bounds, and the tap region is
unchanged. See §"Call-site migration" for the proof.

## Goals

- Remove the `layout` field, the inherent `.with_layout()`, and
  `new_with_layout(...)` from `GestureDetector`.
- Convert `GestureDetectorRenderObject` to a true pass-through proxy
  (the `DecoratedBoxRenderObject` model): `layout()` returns the child's
  node, `apply_layout()` adopts the child's computed bounds,
  `is_pass_through()` returns `true`.
- Migrate the 3 production + 2 test call sites that chain
  `.with_layout(L)` on `GestureDetector` to wrap the content in
  `WithLayout::new(content, L)` instead.
- Add an inherent `.with_layout(layout: Layout) -> Self` to `WithLayout`
  for API parity with `MultiChild`/`Stack`/`Grid` (all of which expose an
  inherent replace-layout method).

## Non-Goals

- **No trait-default `.with_layout()` on `Widget`.** The 2026-07-20 ADR
  removed this deliberately: `WithLayout::new` injects
  `FlexDirection::Column` + `AlignItems::Stretch` defaults the caller
  can't see, and a trait method that silently wraps would reintroduce
  that footgun. This spec honors the ADR.
- No migration of the ~83 existing `WithLayout::new(child, layout)` call
  sites. Additive-only for those.
- No changes to `MultiChild`/`Stack`/`Grid` — their inherent
  `.with_layout()` already sets their own field and is the model
  `WithLayout` is being brought into parity with.
- No changes to `Positioned` (its insets are Stack-specific, not general
  layout).
- No changes to `MouseRegionRenderObject`, even though it has the same
  "owns a container node for hit-testing" shape as the current
  GestureDetector. It has no `layout` field and no `.with_layout()`
  inherent method, so it doesn't violate the unified model. A future spec
  may convert it for consistency; out of scope here.
- No deprecation period. Internal codebase, no external consumers.

## Architecture

### GestureDetector widget (`vexo/src/widgets/gesture_detector.rs`)

**Remove:**

- The `layout: Layout` field (`:65`).
- The `Layout::default().flex_direction(Column).align(Stretch)` default
  set in `GestureDetector::new` (`:81-83`) — no longer needed.
- The inherent `pub fn with_layout(mut self, layout: Layout) -> Self`
  method (`:105-108`).
- The `layout: self.layout.clone()` line in `impl Clone for
  GestureDetector` (`:141`).
- The `Layout` and `FlexDirection`/`AlignItems` imports if they become
  unused.

**Keep unchanged:** `key`, `child`, `on_press`, `on_release`, `on_tap`
fields; `new`, `with_key`, `on_press`, `on_release`, `on_tap`,
`child()` methods; `key()`, `create_element()`, `as_any()`, `child()`,
`clone_boxed()` on the `Widget` impl.

After the change, `GestureDetector::new(content).on_press(cb)` is the
complete API — no layout knobs.

### GestureDetectorRenderObject (`vexo/src/widgets/gesture_detector.rs:458-566`)

Convert to the pass-through model defined by
`DecoratedBoxRenderObject` (`vexo/src/render_objects/decorated_box.rs:66-103`).

**Struct fields:** drop `layout: Layout`. Keep `child`,
`computed_bounds`, `layout_node` — but `layout_node` now stores the
*child's* node (adopted), not a self-owned container.

**Constructors:** drop `new_with_layout(layout)`. Keep `new()` —
returns a pass-through RO with no layout opinion. (Mirrors
`DecoratedBoxRenderObject::new(style)` minus the style.)

**`RenderObject` impl:**

- `layout(&mut self, ctx, child_nodes)` — pass-through. Return the
  child's node directly; defensive zero-size leaf if no child. Identical
  structure to `decorated_box.rs:67-91`.
- `apply_layout(&mut self, ctx)` — read the child's computed bounds via
  the adopted `layout_node`; store in `computed_bounds`. Identical to
  `decorated_box.rs:93-99`.
- `is_pass_through(&self) -> bool` — **add**, returns `true`. Tells the
  registry to skip orphan-node cleanup on removal (the child owns the
  node). See `render_object.rs:385-394` for the contract.
- `paint`, `hit_test`, `children`, `as_any`, `as_any_mut`,
  `set_child_id`, `replace_child`, `layout_node`, `computed_bounds` —
  unchanged. `hit_test` already checks `computed_bounds`, which now
  holds the child's bounds (which, post-migration, is the `WithLayout`
  wrapper's bounds = the slot).

**External downcast callers (tests only):**
- `vexo_uikit/src/tab_bar.rs:428,467,561,572` — downcast to
  `GestureDetectorRenderObject` to inspect `computed_bounds()`. The type
  still exists; `computed_bounds()` still exists. **No change needed.**
- `shared_app/src/me/profile_screen.rs:519` — downcast to count
  gesture ROs. Type unchanged. **No change needed.**

### WithLayout inherent `.with_layout()` (`vexo/src/widgets/with_layout.rs`)

Add an inherent method mirroring `MultiChild::with_layout`
(`multi_child.rs:64-67`):

```rust
impl WithLayout {
    /// Replace the layout.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
}
```

Note: this *replaces* the layout wholesale (same semantics as
`MultiChild::with_layout`), it does not re-apply the `Column + Stretch`
default injection that `WithLayout::new` performs (`with_layout.rs:261-265`).
Callers who want the defaults should use `WithLayout::new`. Callers who
want to replace an existing `WithLayout`'s layout with a fully-specified
one use `.with_layout()`. This matches `MultiChild::with_layout`, which
also replaces without re-applying any defaults.

This is a pure additive change with no migration. It brings `WithLayout`
into API parity with the other three layout-owning widgets
(`MultiChild`/`Stack`/`Grid`), so the unified rule becomes: **every
widget that owns a `layout` field exposes `.with_layout()` to replace
it; no widget that doesn't own a layout field exposes one.**

## Call-site migration

Three production sites and two test sites chain `.with_layout(L)` on
`GestureDetector`. The migration is mechanical: move `.with_layout(L)`
from the detector onto the content.

```rust
// Before (detector owns the box)
GestureDetector::new(content)
    .on_press(cb)
    .with_layout(L)
    .boxed()

// After (WithLayout inside owns the box; detector adopts its bounds)
GestureDetector::new(WithLayout::new(content, L))
    .on_press(cb)
    .boxed()
```

### Production sites

| File:line | Layout | Hit-test region |
|---|---|---|
| `vexo_uikit/src/tab_bar.rs:184-193` | `flex_grow(1.0).flex_direction(Column).align(Stretch).justify(Center)` | Full equal-width slot. The `WithLayout` wrapper now owns the `flex_grow(1.0)` slot node; detector adopts its bounds. **Unchanged.** |
| `shared_app/src/desktop_shell.rs:133-143` | `width_percent(1.0).height(48.0).flex_shrink(0.0).align(Center).justify(Center)` | Full-width 48px row. `WithLayout` owns the fixed-size node; detector adopts. **Unchanged.** |
| `shared_app/src/me/profile_screen.rs:376-381` | `flex_shrink(0.0)` | Content-sized. `flex_shrink(0.0)` only prevents shrinking; the content already sizes itself. Moving onto content is behavior-equivalent. **Unchanged.** |

### Test sites

| File:line | Migration |
|---|---|
| `vexo/src/widgets/gesture_detector.rs:744` | `GestureDetector::new(Text::new("Slot")).with_layout(layout)` → `GestureDetector::new(WithLayout::new(Text::new("Slot"), layout))` |
| `vexo/src/widgets/gesture_detector.rs:765` | `GestureDetector::new(Text::new("Clone Me")).with_layout(layout)` → `GestureDetector::new(WithLayout::new(Text::new("Clone Me"), layout))` |

### Why hit-testing is preserved

The pass-through detector's `hit_test` checks `computed_bounds`
(`gesture_detector.rs:527-532`), which post-migration holds the child's
bounds. The child is now `WithLayout::new(content, L)`, and `L` is
exactly the layout previously applied to the detector. So:

- The `WithLayout` owns the Taffy node with layout `L`.
- The `WithLayout`'s computed bounds = what the detector's bounds were
  before (same `L`, same parent constraints).
- The detector adopts those bounds (pass-through `apply_layout`).
- `hit_test` checks the same region.

The only behavioral difference is *which render object owns the node* —
the `WithLayout`'s RO instead of the detector's. The geometry and
hit-test result are identical.

### Reconciliation stability

`GestureDetector` widgets migrate from `GestureDetector::new(content)
.with_layout(L)` (one widget, layout field set) to
`GestureDetector::new(WithLayout::new(content, L))` (two widgets: a
`WithLayout` child of a `GestureDetector`). This is a structural tree
change at the migration commit: the reconciler will unmount the old
single-element subtree and mount the new two-element subtree on first
run after the code change. This is fine — it's a source edit, not a
runtime tree mutation. No state is lost that matters (GestureDetector
holds no `ComponentState`; its callbacks are re-cloned from the new
widget on mount).

## Error Handling & Edge Cases

1. **Missed call site** → compile error. Removing the inherent
   `.with_layout()` from `GestureDetector` makes every remaining
   `gd.with_layout(...)` call fail to compile. The compiler catches all
   of them. (Verified: only 5 sites exist — 3 production, 2 test — per
   `rg "GestureDetector.*\.with_layout"`.)

2. **`GestureDetector` with no child** — the pass-through `layout()`
   defensive branch (`child_nodes.first() == None`) creates a
   throwaway zero-size leaf, mirroring `DecoratedBoxRenderObject`. No
   panic. This case doesn't arise in practice (the widget always has a
   child) but the defensive branch is kept for framework robustness.

3. **`GestureDetectorRenderObject::new_with_layout` removal** — any
   external caller of this constructor breaks at compile time. Verified
   via `rg "new_with_layout"`: only one caller, `GestureDetector::create_render_object`
   (`gesture_detector.rs:161`), which is being rewritten in this change.
   No external callers.

4. **`GestureDetector::new` default layout removal** — the
   `Layout::default().flex_direction(Column).align(Stretch)` default
   (`:81-83`) is currently applied to the detector's own container node.
   Post-migration, the detector has no container node, so this default is
   gone. The migration wraps content in `WithLayout::new(content, L)`,
   and `WithLayout::new` applies its own `Column + Stretch` defaults
   (`with_layout.rs:261-265`). For the tab_bar/desktop_shell sites, `L`
   explicitly sets `flex_direction(Column).align(Stretch)` anyway, so
   the result is identical. For the profile_screen site, `L` is
   `flex_shrink(0.0)` only; `WithLayout::new` injects `Column + Stretch`
   on top — but the content is a self-sizing cell that already stretches
   to its natural width, so the injection is behaviorally inert.
   **No regression.**

5. **`MouseRegionRenderObject` not converted** — it has the same
   "owns a container node for hit-testing" shape, but no `layout` field
   and no `.with_layout()` inherent, so it doesn't violate the unified
   model. Converting it is out of scope (would require a separate
   audit of `MouseRegion` callers). Left as-is.

## Testing

### New unit tests

**`vexo/src/widgets/gesture_detector.rs`** (extend existing tests):

- `test_gesture_detector_render_object_is_pass_through` —
  `GestureDetectorRenderObject::new()` returns an RO where
  `is_pass_through() == true`. Regression guard mirroring
  `test_decorated_box_render_object_is_pass_through`
  (`decorated_box.rs:480-488`). (The absence of a `layout` field and
  inherent `.with_layout()` is enforced by the compile-test — if either
  survived, `cargo build` would fail at the migration call sites.)
- Update the two `with_layout` call-site tests (lines 744, 765) to the
  `WithLayout::new(content, layout)` form. Same bounds assertions, new
  construction.

**`vexo/src/widgets/with_layout.rs`**:

- `test_with_layout_inherent_replace` —
  `WithLayout::new(Text::new("x"), Layout::default().padding(8.0))
  .with_layout(Layout::default().padding(16.0))` produces a widget whose
  `layout_ref().padding == 16.0`. Mirrors
  `test_multi_child_with_layout_replaces` (`multi_child.rs:178-182`).

### Tests that must still pass (regression guards)

- All `gesture_detector.rs` tests except the two migrated call-site
  tests — they exercise event handling, focus, child mounting, none of
  which change.
- `vexo_uikit/src/tab_bar.rs:425` (`test_tab_bar_items_are_equal_width_full_height_slots`)
  — downcasts to `GestureDetectorRenderObject` and inspects
  `computed_bounds`. Post-migration, the detector is pass-through and
  adopts the `WithLayout` child's bounds, which are the equal-width
  slots. **Bounds assertions still hold.** This is the key hit-test
  regression guard.
- `shared_app/src/me/profile_screen.rs:507`
  (`test_appearance_picker_renders_two_tappable_cells`) — counts
  `GestureDetectorRenderObject` instances (expects 2). Type unchanged.
  **Still passes.**
- All `with_layout.rs` existing tests — `WithLayout::new` behavior
  unchanged; only an additive inherent method is added.

### Compile-test as the primary guard

The strongest test for the GD cleanup is `cargo build --workspace`.
Every missed migration site is a compile error (inherent method gone).
Every external caller of `new_with_layout` is a compile error. Once the
workspace builds, the structural migration is complete by construction.

### GUI validation (required, user-run)

Per `CLAUDE.md`, the agent does not run `cargo run -p desktop_demo`.
Hit-testing is the one behavioral risk. The user must run:

- Tab bar: tap anywhere in an equal-width tab slot → tab switches.
  (Exercises `tab_bar.rs:184` migration.)
- Desktop sidebar: tap anywhere in a 48px sidebar row → switches view.
  (Exercises `desktop_shell.rs:133` migration.)
- Profile appearance picker: tap either cell → toggles theme.
  (Exercises `profile_screen.rs:376` migration.)

If any of these regress, the symptom is "tap only registers on the
centered content, not the full slot/row." Per the
`debugging-gui-with-logs` workflow, the fix path is: add `log::debug!`
in `GestureDetectorRenderObject::hit_test` printing `computed_bounds`,
run with `RUST_LOG=debug | grep hit_test`, and inspect the bounds. The
expected bounds match the `WithLayout` child's bounds (full slot/row).

## File-Level Change Summary

| File | Change |
|---|---|
| `vexo/src/widgets/gesture_detector.rs` | **Remove** `layout` field, default-layout set in `new`, inherent `.with_layout()`, `new_with_layout()` constructor, `layout` line in `Clone` impl, unused `Layout`/`FlexDirection`/`AlignItems` imports. **Convert** `GestureDetectorRenderObject` to pass-through: drop `layout` field, rewrite `layout()` to return child's node, add `is_pass_through() == true`. **Migrate** 2 test call sites (lines 744, 765) to `WithLayout::new(content, layout)` form. **Add** `test_gesture_detector_render_object_is_pass_through` test. |
| `vexo/src/widgets/with_layout.rs` | **Add** inherent `pub fn with_layout(layout: Layout) -> Self` method (mirrors `MultiChild::with_layout`). **Add** `test_with_layout_inherent_replace` test. |
| `vexo_uikit/src/tab_bar.rs` | **Migrate** line 184 — `GestureDetector::new(content).on_press(cb).with_layout(L).boxed()` → `GestureDetector::new(WithLayout::new(content, L)).on_press(cb).boxed()`. (`WithLayout` already imported at line 18.) |
| `shared_app/src/desktop_shell.rs` | **Migrate** line 133 — same pattern. (`WithLayout` already imported at line 19.) |
| `shared_app/src/me/profile_screen.rs` | **Migrate** line 376 — same pattern. (`WithLayout` already imported — used at lines 159, 168, 205, etc.) |

**Total: 5 files edited, 5 call sites migrated, 1 widget struct slimmed,
1 render object converted to pass-through, 1 inherent method + 2 tests
added.**

## Resolved Decisions

1. **Honor the 2026-07-20 ADR** → No trait-default `.with_layout()` on
   `Widget`. The ADR removed this deliberately to eliminate the silent
   `Column + Stretch` injection footgun. This spec does not reintroduce
   it. The unification is achieved structurally: every layout-owning
   widget exposes an inherent `.with_layout()`, and GestureDetector
   stops owning a layout.
2. **Convert GestureDetector to pass-through** → yes. This is the
   structural cleanup. The detector becomes consistent with
   `DecoratedBox`/`Opacity`/`Transform`/`ClipRRect`/
   `FractionalTranslation` — all true pass-through single-child
   modifiers.
3. **Add `WithLayout::with_layout()` inherent** → yes. Brings
   `WithLayout` into parity with `MultiChild`/`Stack`/`Grid`. Pure
   additive, no migration.
4. **Migrate call sites by wrapping content** → yes. Moving
   `.with_layout(L)` from the detector onto the content preserves
   hit-testing because the pass-through detector adopts the
   `WithLayout` child's bounds, which equal the old detector bounds
   (same `L`, same constraints).
5. **No deprecation period** → Remove `GestureDetector::with_layout`
   outright. Internal codebase, no external consumers.
6. **`MouseRegionRenderObject` left as-is** → It has the same shape
   but doesn't violate the unified model (no `layout` field, no
   `.with_layout()`). Conversion is out of scope; a future spec can
   address it if consistency is desired.
