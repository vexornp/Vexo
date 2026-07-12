# Pass-Through Render Objects

Status: Design — approved, pending implementation plan.
Related: `docs/vexo_vs_flutter_render_object_architecture.md` (finding), commit `76bfc73` (workaround).

## Problem & Goals

Vexo's single-child render objects — `OpacityRenderObject`, `TransformRenderObject`, and the onstage branch of `OffstageRenderObject` — each insert a `Column + AlignItems::Stretch` flex container into the Taffy tree. This is unlike Flutter, where the analogous render objects (`RenderOpacity`, `RenderTransform`, `RenderOffstage`) are *pass-through*: they forward the parent's `BoxConstraints` directly to the child, adopt the child's size, and add no layout node of their own.

This architectural difference is why the navigation transition text-wrapping bug (fixed in `76bfc73`) was possible in Vexo but cannot occur in Flutter. The current fix is a workaround — `AlignItems::Stretch` on `IndexedStack`/`Stack` — that papers over the symptom (the child gets a definite cross-axis width from the parent) without removing the root cause (extra flex-container layers participating in bottom-up max-content measurement, creating circular dependencies when a container's size is content-derived and its visible subtree has weak in-flow max-content).

### Goal

Convert `OpacityRenderObject`, `TransformRenderObject`, and `OffstageRenderObject` (onstage branch) to true pass-through render objects: they create **no Taffy node**, their child's Taffy node is linked directly to the grandparent, and they adopt the child's computed bounds. This removes the extra measurement layers that enabled the bug, matches Flutter's semantics, and reduces Taffy node count and measurement passes.

### Non-goals (deferred to follow-up specs)

- **`DecoratedContainer`** — actively uses `.padding()` in production (`shared_app/src/lib.rs:245` nav item, `vexo_uikit/src/button.rs:242` Button). Not pure pass-through. Its Flutter-faithful split into `DecoratedBox` (pass-through, paint-only) + `Padding` (layout container) is a separate design. Audit confirmed `DecoratedContainer` uses `layout_builder_methods!()` and real call sites chain `.padding()` / `.padding_each()`.
- **`IndexedStack` Flutter-style `performLayout`** — DONE in a follow-up plan (`docs/superpowers/plans/2026-07-12-indexed-stack-flutter-style-perform-layout.md`). Implemented via a dedicated `IndexedStackRenderObject` that filters `set_children()` to the visible child only.
- **Reverting the `AlignItems::Stretch` workaround** on `IndexedStack`/`Stack`. Kept as defense-in-depth — Stretch is a reasonable default and reverting risks reintroducing circular-dependency bugs in scenarios the pass-through migration does not cover.

### Success criteria

1. `Opacity`, `Transform`, `Offstage`-onstage create no Taffy node; their child's node links directly to the grandparent.
2. The navigation transition renders correctly (text does not wrap) with pass-through ROs in place. (The `Stretch` workaround remains; this criterion verifies the pass-through ROs do not break the currently-working transition.)
3. All existing tests pass.
4. New layout tests verify: (a) a pass-through RO's child receives the grandparent's definite width directly, (b) nested pass-through ROs (`Opacity(Transform(child))`) link correctly through to the grandchild, (c) `Offstage` flag-flip transitions its node state correctly.

---

## The `is_pass_through()` Trait Signal

The core design challenge: the layouter (`vexo/src/layouter.rs`) links parent → child via `child.layout_node()`. A naive pass-through RO returning `None` from `layout_node()` would orphan its grandchild — the grandparent would link nothing for this branch.

The solution has two parts, developed across the next two sections:

1. **`layout_node()` returns the child's node** (not `None`) for pass-through ROs. This keeps the layouter's existing linking logic working with **zero changes** (see Pipeline & Registry Integration). The pass-through RO "borrows" the child's node for layout participation.
2. **`is_pass_through()` trait method** — a single boolean that distinguishes "I own a node" from "I'm borrowing my child's node." This is needed in exactly **one place**: the registry's cleanup guard on `remove()` (see Pipeline & Registry Integration). Without it, removing a pass-through RO would push the borrowed (child's) node to the orphaned list, causing double-removal when the child is also removed.

### The signal

Add a default-implemented trait method:

```rust
// vexo/src/render_object.rs — RenderObject trait
fn is_pass_through(&self) -> bool { false }
```

The three pass-through ROs override:
- `OpacityRenderObject::is_pass_through() -> true` (always)
- `TransformRenderObject::is_pass_through() -> true` (always)
- `OffstageRenderObject::is_pass_through() -> !self.offstage` (onstage = transparent, borrows child's node; offstage = owns a zero-size leaf node)

### Why a trait method, not a magic `None` check

`layout_node()` returning `None` is already overloaded — it means "first frame, node not yet created" for normal ROs. Pass-through ROs do NOT return `None` from `layout_node()` (they return the child's node), so `None` retains its single meaning. `is_pass_through()` answers a different question: "do I *own* the node I'm reporting, or am I borrowing it?" — needed only for cleanup ownership.

### Why this is non-breaking

`is_pass_through()` has a default of `false`. All existing ROs (Text, Image, Container, TextEdit, ScrollView, GestureDetector, MouseRegion, SafeArea, Positioned, DecoratedContainer, the stateful widget RO) inherit the default and behave exactly as before. Only the three converted ROs override it.

---

## Pipeline & Registry Integration

A simplification emerged during design: if the pass-through RO's `layout_node()` returns its **child's** node (rather than `None`), most of the layouter needs no changes at all. `is_pass_through()` shrinks to a single use: guarding cleanup.

### The key insight

`layout_node()` already serves as "the node I participate in layout with." For a pass-through RO, that is the child's node. The only place that needs to know "do I *own* a node?" is orphan cleanup on removal — and `is_pass_through()` answers exactly that.

### Pass-through RO fields

Each pass-through RO gains one field:

```rust
child_layout_node: Option<LayoutNodeKey>,  // The child's Taffy node (stored from layout())
```

### `layout_node()` behavior

| RO state | `layout_node()` returns | `is_pass_through()` |
|---|---|---|
| Normal RO (Text, Flex, etc.) | `self.layout_node` (owned Taffy node) | `false` |
| Opacity / Transform | `self.child_layout_node` (child's node) | `true` |
| Offstage — offstage | `self.owned_node` (zero-size leaf) | `false` |
| Offstage — onstage | `self.child_layout_node` (child's node) | `true` |

### Why the layouter needs NO changes

Tracing each layouter touch point with `layout_node()` returning the child's node:

**1. First-frame check** (`layouter.rs:122`):
```rust
obj.layout_node().is_none()
```
- Pass-through, first frame: `child_layout_node` is `None` → `is_none()` = true → `needs_layout` = true → `layout()` called → stores child node.
- Pass-through, subsequent frame: `child_layout_node` is `Some` → `is_none()` = false → skipped.
- No change needed.

**2. Child-node collection** (`layouter.rs:135`):
```rust
c.layout_node()
```
- Grandparent collects child nodes. Its child is the pass-through RO. `PassThrough.layout_node()` returns `child_layout_node` = the grandchild's node. Grandparent links grandchild's node directly.
- No recursive skip needed. No change needed.

**3. Root lookup** (`layouter.rs:148`):
```rust
obj.layout_node()
```
- If root is pass-through (rare): returns child's node. `engine.compute()` runs on the real root.
- No change needed.

### The one registry change: cleanup guard

`RenderObjectRegistry::remove()` (`render_object.rs:466`) currently pushes `obj.layout_node()` to orphaned nodes for Taffy cleanup. For a pass-through RO, `layout_node()` returns the *child's* node — which the child's own `remove()` will also push. Double push → double `engine.remove_node()`.

Fix: guard with `is_pass_through()`:

```rust
pub fn remove(&mut self, key: RenderObjectKey) {
    if let Some(obj) = self.objects.get(key) {
        if !obj.is_pass_through() {           // new guard
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

When a pass-through RO is removed, cleanup is skipped — its child's RO handles its own node cleanup. For `Offstage` offstage (which owns a zero-size leaf), `is_pass_through()` is `false`, so the leaf IS cleaned up.

### `apply_layout` for pass-through ROs

The pass-through RO reads the child's computed bounds via its stored `child_layout_node`:

```rust
fn apply_layout(&mut self, ctx: &mut LayoutContext) {
    if let Some(child_node) = self.child_layout_node {
        if let Some(computed) = ctx.engine_ref().get_layout(child_node) {
            self.computed_bounds = Some(computed.bounds);
        }
    }
}
```

The pass-through RO's bounds equal the child's bounds — matching Flutter's "adopt child size" semantics.

### Offstage flag-flip transitions

When `Offstage` flips state, the old node must be cleaned up:

- **offstage → onstage**: `layout()` detects the transition, calls `engine.remove_node()` on the old zero-size leaf, stores `child_layout_node` from `child_nodes`.
- **onstage → offstage**: `layout()` clears `child_layout_node`, creates a new zero-size leaf node.

### Summary of changes

| File | Change |
|---|---|
| `render_object.rs` | Add `is_pass_through()` default method. Guard `remove()` cleanup. |
| `layouter.rs` | No changes. |
| `opacity.rs` | `layout()`: no Taffy node, store `child_layout_node`. `layout_node()`: return `child_layout_node`. `apply_layout()`: read child bounds. `is_pass_through()`: `true`. |
| `transform.rs` | Same as opacity. |
| `offstage.rs` | Onstage branch: same as opacity. Offstage branch: unchanged. `layout_node()`: return `owned_node` or `child_layout_node` depending on state. `is_pass_through()`: `!self.offstage`. Handle flag-flip node cleanup. |

---

## Render Object Changes

### `OpacityRenderObject` (`vexo/src/render_objects/opacity.rs`)

**Removed:** `layout_node: Option<LayoutNodeKey>` field (the owned Taffy node).

**Added:** `child_layout_node: Option<LayoutNodeKey>` field.

**`layout()`:** No longer calls `ctx.engine()`. Stores the child's node from `child_nodes`.

```rust
fn layout(&mut self, _ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
    // Pass-through: no Taffy node created. The child's node is linked
    // directly to the grandparent via layout_node().
    let child_node = child_nodes.first().copied().expect(
        "pass-through render object requires a child widget; \
         Opacity/Transform/Offstage always have a child per their constructors",
    );
    self.child_layout_node = Some(child_node);
    LayoutResult {
        node: child_node, // unused by layouter (layouter.rs:139 discards return),
                          // but required by the struct field.
        size: crate::core::Size::zero(),
    }
}
```

**`LayoutResult.node` note:** `LayoutResult` is a struct with a required `node: LayoutNodeKey`. The layouter does **not** read `LayoutResult.node` — it is discarded by `layout_dirty_recursive` (`layouter.rs:139`: `obj.layout(ctx, &child_nodes);` — return value unused). The RO's `layout_node()` method is the source of truth for all linking. The pass-through RO sets `node` to the child's node (the only valid key available) to satisfy the struct field. `LayoutNodeKey` is a slotmap key with no `Default` impl, so the no-child case cannot use `unwrap_or_default()`; instead, the RO enforces its invariant (child must exist) via `expect`. This matches the widget constructors, which all require a child widget — `Opacity::new(child, ...)`, `Transform::new(child, ...)`, `Offstage::new(child, ...)` all take a non-optional child.

**`layout_node()`:** Returns the child's node.
```rust
fn layout_node(&self) -> Option<LayoutNodeKey> {
    self.child_layout_node
}
```

**`apply_layout()`:** Reads child's computed bounds.
```rust
fn apply_layout(&mut self, ctx: &mut LayoutContext) {
    if let Some(child_node) = self.child_layout_node {
        if let Some(computed) = ctx.engine_ref().get_layout(child_node) {
            self.computed_bounds = Some(computed.bounds);
        }
    }
}
```

**`is_pass_through()`:** `true`.

**Unchanged methods:** `paint`, `hit_test`, `children`, `set_child_id`, `replace_child`, `opacity`, `as_any`, `as_any_mut`, `computed_bounds`. The opacity value is still exposed via `opacity()` for the painter's `PushOpacity`/`PopOpacity` wrapping. Paint and hit-test logic is independent of layout node ownership.

### `TransformRenderObject` (`vexo/src/widgets/transform.rs`)

Identical pattern to Opacity:
- Remove `layout_node` field, add `child_layout_node`.
- `layout()`: no Taffy node, store `child_nodes.first()`.
- `layout_node()`: return `child_layout_node`.
- `apply_layout()`: read child bounds.
- `is_pass_through()`: `true`.
- `paint_transform()`, `hit_test_transform()`, all other methods: unchanged. The transform is still exposed for the painter's `PushTransform`/`PopTransform` wrapping.

### `OffstageRenderObject` (`vexo/src/render_objects/offstage.rs`)

The most complex — it has two branches with different node ownership.

**Fields:**
```rust
pub struct OffstageRenderObject {
    offstage: bool,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    /// Owned Taffy node — only when offstage (zero-size leaf).
    /// None when onstage (pass-through).
    owned_node: Option<LayoutNodeKey>,
    /// Child's Taffy node — only when onstage (pass-through).
    /// None when offstage.
    child_layout_node: Option<LayoutNodeKey>,
}
```

**`is_pass_through()`:** `!self.offstage`.

**`layout()` — offstage branch (unchanged from current):**
Creates/updates a zero-size leaf node, stores in `owned_node`. `child_layout_node` cleared. `children()` returns `&[]` (child not linked into layout).

**`layout()` — onstage branch (new pass-through behavior):**
- If transitioning from offstage (was `owned_node`): call `ctx.engine().remove_node()` on the old `owned_node`, clear it.
- Store `child_nodes.first()` into `child_layout_node`.
- No Taffy node created.

**`layout_node()`:**
```rust
fn layout_node(&self) -> Option<LayoutNodeKey> {
    if self.offstage {
        self.owned_node
    } else {
        self.child_layout_node
    }
}
```

**`apply_layout()`:**
```rust
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
```

**Flag-flip node lifecycle** (handled in `layout()`):

| Transition | Old node | New state |
|---|---|---|
| offstage → onstage | `owned_node` (zero-size leaf) | `engine.remove_node(owned_node)`, clear `owned_node`, set `child_layout_node` from `child_nodes` |
| onstage → offstage | `child_layout_node` (borrowed) | Clear `child_layout_node` (no removal — the child owns it), create new zero-size leaf in `owned_node` |

The offstage→onstage path removes the owned zero-size leaf; the onstage→offstage path simply stops borrowing the child's node (the child still owns it, just is not linked into the parent's layout via `children()` returning `&[]`).

**Unchanged methods:** `children`, `set_child_id`, `replace_child`, `paint`, `hit_test`, `as_any`, `as_any_mut`, `computed_bounds`. When offstage, `children()` returns `&[]` so the painter/hit-tester/layouter skip the child. When onstage, `children()` returns `&[child]` so `apply_layout_recursive` recurses into the child (correct — the child's `apply_layout` runs, and the Offstage RO reads the child's bounds via `child_layout_node`).

### What does NOT change

- **`DecoratedContainer`** — stays a layout container (`ContainerRenderObject` with its Taffy node + style + flex-builder methods). Deferred to a follow-up spec.
- **`ContainerRenderObject`** (Flex/Stack/IndexedStack) — unchanged.
- **All other ROs** (Text, Image, TextEdit, ScrollView, GestureDetector, MouseRegion, SafeArea, Positioned) — unchanged. `is_pass_through()` defaults to `false`.
- **Widget layer** — `Opacity`, `Transform`, `Offstage` widgets' public API is unchanged. `create_render_object()`, `update_render_object()`, element mount/unmount/rebuild all work as before. The change is entirely within the render object layer.

---

## Data Flow Walkthrough

This section traces the complete layout/paint/hit-test/cleanup flows with pass-through ROs in place, using the navigation transition scenario (the bug's original reproducer) as the concrete example.

### The transition widget tree

```
Flex::column (NavStackView root)
├── nav_bar (Flex::row)
└── Stack
    ├── Positioned(left=0,right=0,top=0,bottom=0)  // outgoing
    │   └── Opacity          ← pass-through (was: Column+Stretch)
    │       └── Transform    ← pass-through (was: Column+Stretch)
    │           └── page Column → Text
    └── Positioned(left=0,right=0,top=0,bottom=0)  // incoming
        └── Opacity          ← pass-through
            └── Transform    ← pass-through
                └── page Column → Text
```

### Layout flow (bottom-up)

Starting from the root Flex::column, `layout_dirty_recursive` recurses children-first:

1. **Text (outgoing page)** — leaf, creates Taffy node `N_text_out`, returns it via `layout_node()`.
2. **page Column (outgoing)** — container, collects `child_nodes = [N_text_out]`, creates container node `N_col_out` with that child. `layout_node()` returns `N_col_out`.
3. **Transform (outgoing)** — pass-through. `layout()` is called with `child_nodes = [N_col_out]` (collected from the Column's `layout_node()`). Stores `child_layout_node = N_col_out`. Creates no Taffy node. `layout_node()` returns `N_col_out`.
4. **Opacity (outgoing)** — pass-through. `layout()` called with `child_nodes = [N_col_out]` (collected from Transform's `layout_node()`, which returned the borrowed `N_col_out`). Stores `child_layout_node = N_col_out`. `layout_node()` returns `N_col_out`.
5. **Positioned (outgoing)** — layout container (absolute positioning). Collects `child_nodes = [N_col_out]` (from Opacity's `layout_node()`). Creates its own node `N_pos_out` with `N_col_out` as child.
6. Steps 1–5 repeat for the incoming page, producing `N_pos_in`.
7. **Stack** — layout container. Collects `child_nodes = [N_pos_out, N_pos_in]`. Creates `N_stack`.
8. **nav_bar (Flex::row)** — creates `N_navbar`.
9. **root Flex::column** — collects `child_nodes = [N_navbar, N_stack]`. Creates `N_root`.

**Taffy tree shape (outgoing branch):**
```
N_root (Flex::column)
├── N_navbar (Flex::row)
└── N_stack (Stack)
    └── N_pos_out (Positioned)        ← Opacity/Transform are GONE from the tree
        └── N_col_out (page Column)   ← linked directly to Positioned
            └── N_text_out (Text)
```

Compare to the **current** tree (with flex-container ROs):
```
N_root
└── N_stack
    └── N_pos_out
        └── N_opacity_out (Column+Stretch)   ← extra layer
            └── N_transform_out (Column+Stretch)  ← extra layer
                └── N_col_out
                    └── N_text_out
```

The pass-through migration removes 2 Taffy nodes per transition branch (4 total during a transition). The page Column's Text now receives constraints through `Positioned` only — no intervening flex containers participating in max-content measurement.

### Compute flow

`engine.compute(N_root, available_size, ...)` runs Taffy on the cleaned tree. The page Column's Text receives definite constraints derived from `Positioned(left=0, right=0)` resolving against the Stack's width — which resolves against the root Flex's width — which is the window width. No circular max-content dependency through Opacity/Transform.

This is the correctness improvement: the extra `Column + Stretch` layers that participated in bottom-up max-content measurement are gone. The `AlignItems::Stretch` workaround on Stack/IndexedStack remains as defense-in-depth, but the root cause (pass-through ROs as flex containers) is removed.

### Apply-layout flow

`apply_layout_recursive` walks the RO tree (not the Taffy tree — it uses `children()`):

1. root Flex::column — `apply_layout()` reads `N_root`'s computed bounds.
2. nav_bar — reads `N_navbar`.
3. Stack — reads `N_stack`.
4. Positioned (outgoing) — reads `N_pos_out`.
5. **Opacity (outgoing)** — `apply_layout()` reads `child_layout_node = N_col_out`'s computed bounds. Stores them as its own `computed_bounds`. (The painter needs these bounds.)
6. **Transform (outgoing)** — reads `N_col_out`'s bounds. Stores as its own.
7. page Column — reads `N_col_out`.
8. Text — reads `N_text_out`.

Key point: Opacity and Transform each read the **same** `N_col_out` bounds. They adopt the child's size — matching Flutter's "adopt child size" semantics. Their `computed_bounds` is used by hit-testing and (for Transform) by the painter's transform application.

### Paint flow

The painter's `paint_recursive` walks the RO tree via `children()`:

1. root Flex — paints its decorations (none).
2. nav_bar — paints.
3. Stack — paints.
4. Positioned (outgoing) — the painter applies the Positioned's offset, recurses into children.
5. **Opacity (outgoing)** — `opacity()` returns `Some(0.5)`. Painter emits `PushOpacity(0.5)`, recurses into `children()` = `[Transform]`.
6. **Transform (outgoing)** — `paint_transform()` returns `Some(transform)`. Painter emits `PushTransform(transform)`, recurses into `children()` = `[page Column]`.
7. page Column — paints its decorations, recurses to Text.
8. Text — paints text.
9. Painter emits `PopTransform`, `PopOpacity` as it unwinds.

**Paint is unchanged.** The `PushOpacity`/`PushTransform` wrapping depends on `opacity()`/`paint_transform()` trait methods, not on layout node ownership. Pass-through ROs still wrap their child's paint commands exactly as before.

### Hit-test flow

`hit_test` on a pass-through RO uses `computed_bounds` (which equals the child's bounds, set in `apply_layout`). The hit-test traversal in the pipeline walks `children()`, so it descends into the child normally. For Transform, `hit_test_transform()` inverts the pointer position before testing children — unchanged.

**Hit-test is unchanged.**

### Cleanup flow (element unmount)

When a transition completes and the outgoing page is unmounted:

1. `Opacity` element unmounts → `RenderObjectRegistry::remove(opacity_ro_key)`.
   - `remove()` checks `is_pass_through()` → `true` → skips orphaned-node push. (The child's node is owned by the child RO.)
2. `Transform` element unmounts → `remove()` → `is_pass_through()` → `true` → skips.
3. page Column element unmounts → `remove()` → `is_pass_through()` → `false` → pushes `N_col_out` to orphaned nodes.
4. Text element unmounts → `remove()` → pushes `N_text_out`.
5. Layouter drains orphaned nodes, calls `engine.remove_node(N_col_out)` and `engine.remove_node(N_text_out)`.

**No double-removal.** Each Taffy node is removed exactly once, by its owner. The pass-through ROs contribute nothing to cleanup — they borrowed the child's node, they do not own it.

### Offstage flag-flip flow

When `IndexedStack` switches index (e.g., page push):

1. `Offstage` widget for the outgoing page gets `offstage: true` (was `false`).
2. Element `rebuild()` → `update_render_object()` → `set_offstage(true)` returns true → `UpdateResult::LAYOUT` → `mark_needs_layout`.
3. Layouter processes the Offstage RO (it is dirty).
4. `layout()` detects `offstage == true`:
   - Transition from onstage: `child_layout_node` was `Some(N_col)`. Clear it (the child still owns `N_col` — no removal). Create new zero-size leaf `N_leaf`, store in `owned_node`.
   - `children()` now returns `&[]` — child is unlinked from layout.
5. The IndexedStack's `ContainerRenderObject` collects `child_nodes` from each Offstage child. For the now-offstage one, `layout_node()` returns `owned_node = N_leaf` (zero-size). For the newly-onstage one, `layout_node()` returns its `child_layout_node`.

The flag-flip correctly transitions node ownership: onstage borrows the child's node, offstage owns a zero-size leaf.

---

## Testing Strategy

### Unit tests (per-RO, no pipeline)

These verify the pass-through behavior in isolation, using a real `TaffyLayoutEngine` + `LayoutContext` but no element/pipeline machinery.

**`OpacityRenderObject` tests** (`vexo/src/render_objects/opacity.rs`):
- `test_opacity_pass_through_creates_no_node` — Call `layout()` with a child node. Assert `child_layout_node == Some(child_node)`, assert no Taffy node was created (no `owned_node`).
- `test_opacity_layout_node_returns_child_node` — After `layout()`, assert `layout_node()` returns the child's node.
- `test_opacity_is_pass_through` — Assert `is_pass_through() == true`.
- `test_opacity_apply_layout_reads_child_bounds` — Create a child leaf with known size, run `engine.compute()`, call `apply_layout()`. Assert `computed_bounds` matches the child's computed bounds (i.e., adopts child size).
- `test_opacity_with_no_child_panics` — `layout()` with empty `child_nodes` should panic (enforces the invariant that pass-through ROs require a child). Uses `#[should_panic]`.

**`TransformRenderObject` tests** (`vexo/src/widgets/transform.rs`):
- Mirror of the Opacity tests (`test_transform_pass_through_creates_no_node`, `test_transform_layout_node_returns_child_node`, `test_transform_is_pass_through`, `test_transform_apply_layout_reads_child_bounds`).
- Existing transform tests (`test_transform_render_object_paint_transform`, `test_transform_render_object_hit_test_transform`, `test_transform_update_render_object`) remain unchanged — paint/transform behavior is unaffected.

**`OffstageRenderObject` tests** (`vexo/src/render_objects/offstage.rs`):
- `test_offstage_onstage_is_pass_through` — Onstage: `is_pass_through() == true`.
- `test_offstage_offstage_is_not_pass_through` — Offstage: `is_pass_through() == false`.
- `test_offstage_onstage_creates_no_owned_node` — Onstage `layout()`: `owned_node == None`, `child_layout_node == Some(child)`.
- `test_offstage_offstage_creates_zero_leaf` — Offstage `layout()`: `owned_node == Some(leaf)`, `child_layout_node == None`. (Existing test `test_offstage_layout_offstage_creates_zero_node` already covers this — keep it.)
- `test_offstage_flag_flip_onstage_to_offstage` — Start onstage (stores `child_layout_node`). Flip to offstage, call `layout()`. Assert `owned_node` is `Some` (new zero-size leaf), `child_layout_node` is `None`. Assert the child's node was NOT removed from the engine (child still owns it — verify via `engine.get_layout(child_node).is_some()`).
- `test_offstage_flag_flip_offstage_to_onstage` — Start offstage (has `owned_node` leaf). Flip to onstage, call `layout()`. Assert `owned_node` is `None` (old leaf removed from engine — verify via `engine.get_layout(old_leaf).is_none()`), `child_layout_node == Some(child)`.
- `test_offstage_layout_node_switches_with_flag` — Onstage: `layout_node()` returns `child_layout_node`. Offstage: returns `owned_node`. (Existing test `test_offstage_layout_onstage_passes_child` at `offstage.rs:258` should be updated to assert it returns the *child's* node, not a created container node.)

### Integration tests (pipeline-level, real tree)

These verify the layouter + registry + RO tree interact correctly. They use the real `Layouter::layout()` entry point with a hand-built `RenderObjectRegistry`.

- `test_passthrough_grandchild_receives_grandparent_width` — The core correctness test. Tree: `Flex::column (width=300) → Opacity → Text("Hello")`. Run layout with available width 300. Assert Text's computed width == the width it would receive if Opacity were not there (i.e., the grandparent's definite width propagates directly). Compare against a control tree without Opacity. This is the test that would have *failed* on the pre-migration Opacity if the grandparent's width was content-derived.
- `test_nested_passthrough_links_correctly` — Tree: `Flex::column → Opacity → Transform → Text`. Assert Text receives the grandparent's width. Verifies the `child_layout_node` chaining: Opacity returns Transform's `child_layout_node` = Text's node, so the Flex links Text's node directly (through both pass-throughs).
- `test_passthrough_adopts_child_size` — Tree: `Flex::row → Opacity(child=Text("Hi"))`. Assert Opacity's `computed_bounds` equals Text's `computed_bounds` (size adoption).
- `test_passthrough_removal_no_double_cleanup` — Build a tree with Opacity → Text. Remove the Opacity RO from the registry. Drain orphaned nodes. Assert only the Text's node is in the orphaned list (not zero, not two). Then remove Text, drain, assert Text's node is there. Call `engine.remove_node` on each — no panic.
- `test_offstage_flag_flip_in_pipeline` — Build `IndexedStack`-like tree: `Container → [Offstage(onstage, Text), Offstage(offstage, Text)]`. Run layout. Flip the first Offstage to offstage, second to onstage. Run layout again. Assert the now-onstage Text receives the container's width; the now-offstage RO has zero-size bounds.

### Regression: navigation transition

- `test_nav_transition_text_does_not_wrap` — Reproduce the original bug scenario at the RO-tree level. Tree: `Flex::column → [nav_bar (width=140), Stack → [Positioned(L=R=T=B=0) → Opacity → Transform → page Column → Text("long text")]]`. Run layout with available width 375 (the original bug's window width). Assert the Text does NOT wrap (i.e., its computed height == single-line height, or its width >= natural text width). This test must pass both WITH the pass-through ROs AND with the `AlignItems::Stretch` workaround still in place.

This is the end-to-end proof that the migration achieves its goal. The unit + integration tests above isolate the mechanism; this test verifies the actual bug scenario.

### Existing tests

All existing tests in `opacity.rs`, `transform.rs`, `offstage.rs`, `container.rs`, `layouter.rs`, `e2e_test.rs`, `stateful_integration_test.rs`, `window_integration_test.rs` must pass without modification — except the Offstage tests that assert the old flex-container behavior (`test_offstage_layout_onstage_passes_child` at `offstage.rs:258`, which currently asserts `layout_node.is_some()` — it will now assert `layout_node == child_node`).

### What we are NOT testing

- **`DecoratedContainer` pass-through** — out of scope (deferred).
- **`IndexedStack` Flutter-style `performLayout`** — out of scope (deferred).
- **Performance benchmarks** — node-count reduction (4 fewer Taffy nodes per transition branch) is a predictable structural improvement; correctness tests suffice. No benchmark harnesses added.
- **Paint/hit-test regression** — these flows are unchanged by the design (verified in Data Flow Walkthrough). Existing paint/hit-test tests cover them. No new tests needed.

---

## References

- Finding doc: `docs/vexo_vs_flutter_render_object_architecture.md`
- Workaround commit: `76bfc73` — `fix(widgets): stretch IndexedStack/Stack children to fix nav transition text wrapping`
- `vexo/src/render_objects/opacity.rs:59-61` — Opacity layout (current flex container)
- `vexo/src/render_objects/offstage.rs:96-100` — Offstage onstage layout (current flex container)
- `vexo/src/widgets/transform.rs:85-87` — Transform layout (current flex container)
- `vexo/src/layouter.rs:122` — first-frame needs-layout check
- `vexo/src/layouter.rs:135` — child-node collection
- `vexo/src/layouter.rs:148` — root node lookup
- `vexo/src/render_object.rs:466` — registry `remove()` orphaned-node cleanup
- `vexo/src/widgets/decorated_container.rs:339` — `layout_builder_methods!()` usage (reason DecoratedContainer is deferred)
- `shared_app/src/lib.rs:245` — `.padding(10.0)` on DecoratedContainer (production usage)
- `vexo_uikit/src/button.rs:242` — `.padding_each(...)` on DecoratedContainer (production usage)
- Flutter `RenderOpacity`: `packages/flutter/lib/src/rendering/proxy_box.dart`
- Flutter `RenderOffstage`: `packages/flutter/lib/src/widgets/basic.dart`
