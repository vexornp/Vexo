# InheritedWidget Design

**Date:** 2026-07-12
**Status:** Approved (pending user spec review)
**Scope:** `vexo/` crate

## Motivation

Vexo needs a Flutter `InheritedWidget` equivalent: a widget that exposes an
immutable value to all descendants, where descendants can read the value
implicitly (no prop drilling) and auto-rebuild when the value changes. This is
the direct analog of SwiftUI's `@Environment`.

Today Vexo has two ad-hoc mechanisms that touch "environment" data:

- `ElementContext.parent_focus_node_id` — ancestor-scoped, but hardcoded to one
  type (focus). The reconciler pre-computes the nearest ancestor focus node and
  passes it down at mount.
- `BuildOwner.safe_area_source` / `RenderContext::safe_area()` /
  `LayoutContext::safe_area_source()` — ancestor-scoped in intent but
  implemented as a **global** side-channel via atomics on `BuildOwner`.

`InheritedWidget` generalizes the ancestor-scoping pattern so any widget can
expose/consume scoped environment data. This spec delivers the primitive plus a
`Theme` built-in that proves the ergonomic lookup pattern.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Primary scope | Primitive + theming layer (no migration of existing globals) | Smallest scope that proves the pattern; unblocks future migrations |
| Mutation model | Immutable + rebuild-driven (Flutter model) | Composes with existing rebuild/reconcile machinery; no new reactive plumbing |
| Aspect-based rebuilds | Whole-value only (no `InheritedModel`) | YAGNI; simpler dependency tracking (set per ancestor) |
| Built-ins shipped | `Theme` + `Theme::of(ctx)` lookup pattern only | Proves the ergonomic layer; app authors copy the pattern for their own env values |
| Ancestor lookup | Cached per-element map (Approach A) | Only design that cleanly supports render-time lookups given Vexo's `RenderContext` boundary; matches Flutter's cached `_inheritedWidgets` |
| Dependency registration | Render-time, via `RefCell` interior mutability on `InheritedRegistry` | Matches Flutter's "lookup-is-dependency" semantics; avoids fragile two-phase commit |

## Architecture

```
                    ┌─────────────────────────────────────┐
                    │  InheritedWidget<T>  (Widget layer) │
                    │  - holds immutable T                │
                    │  - create_element → InheritedElement│
                    └─────────────────────────────────────┘
                                    │ mount
                                    ▼
                    ┌─────────────────────────────────────┐
                    │  InheritedElement  (Element layer)  │
                    │  - registers self in InheritedRegistry
                    │    under TypeId::of::<T>()          │
                    │  - on update: if value changed,     │
                    │    mark_needs_build on all dependents│
                    └─────────────────────────────────────┘
                                    │
        ┌───────────────────────────┴───────────────────────────┐
        ▼                                                       ▼
┌────────────────────────────┐              ┌─────────────────────────────────┐
│  InheritedRegistry         │              │  ElementContext.inherited_map   │
│  (new, on pipeline)        │              │  (new field)                    │
│  - provider_for_type::<T>()│◄─── read ────│  HashMap<TypeId, ElementKey>    │
│  - dependents:             │              │  nearest ancestor providing T   │
│      HashMap<ElementKey,   │              │  (Arc-shared from parent, copy- │
│        HashMap<TypeId,     │              │   on-write at InheritedElement) │
│        HashSet<ElementKey>>>│             └─────────────────────────────────┘
│  - lookup walks this for   │
│    nearest ancestor        │
└────────────────────────────┘
```

### End-to-end flow

1. **Mount:** A new element inherits its parent's `inherited_map` (cheap `Arc`
   clone). If the element is an `InheritedElement` for type `T`, it inserts
   `T → self` into its own copy-on-write map and registers itself (and its
   value) in `InheritedRegistry`. Children built after this point see the new
   map.
2. **Lookup (render-time):** A `Component::render()` calls `Theme::of(ctx)`.
   `ctx` is a `RenderContext`, which holds a borrowed `&InheritedMap`. The map
   yields the provider's element key. The lookup then reads the value via
   `InheritedRegistry::value::<T>(key)`, and registers the caller as a
   dependent.
3. **Depend:** On first lookup, the caller is added to
   `InheritedRegistry::dependents[provider][T]`. Idempotent.
4. **Update:** An `InheritedElement`'s `update()` compares old vs new value via
   `update_should_notify`. If changed, it updates the stored value and iterates
   `dependents[self][T]`, calling `BuildOwner::mark_needs_build(dep)` on each.
   The next rebuild pass rebuilds dependents in depth order (existing
   machinery).
5. **Unmount:** `InheritedElement` removes itself from `InheritedRegistry`.
   Dependents' entries are dropped. Vexo never reparents, so dependents always
   unmount with or before the provider — no dangling-dependent problem.

### Key invariants

- `inherited_map` is built top-down at mount, never mutated post-mount (only
  swapped wholesale on rebuild of an ancestor provider).
- Vexo has no reparenting, so the map is always consistent with tree position —
  no invalidation logic needed.
- `RenderContext` stays tree-free; it gets a borrowed `&InheritedMap` for
  lookups only.
- The registry's `RefCell` is never re-entered: `mount`/`update`/`unmount`
  don't invoke user code while holding the borrow.

## The `InheritedWidget` Primitive

### `InheritedWidget` trait

A new widget trait, **separate from `Component`**, because `InheritedWidget` is
a provider, not a stateful renderer. Requires `Clone` so the blanket `Widget`
impl can satisfy `clone_boxed()`:

```rust
/// A widget that exposes a value of type `T` to all descendants.
///
/// Immutable: to change the value, an ancestor rebuilds with a new
/// `InheritedWidget`. Dependents auto-rebuild via the `InheritedRegistry`.
pub trait InheritedWidget: Clone + 'static {
    /// The value type exposed to descendants.
    type Value: Clone + PartialEq + 'static;

    /// The current value exposed to descendants.
    fn value(&self) -> &Self::Value;

    /// The single child subtree that gets access to this value.
    ///
    /// Returned as `&dyn Widget` for the blanket `Widget` impl to forward
    /// to `Widget::child()`.
    fn child(&self) -> &dyn Widget;

    /// Optional key for identity across frames (default `None`).
    fn key(&self) -> Option<WidgetKey> {
        None
    }

    /// Whether updating `old_widget → new_widget` should rebuild dependents.
    ///
    /// Default: rebuild iff `value()` changed. Override for custom semantics
    /// (e.g. always-rebuild, or deep-compare a non-PartialEq type).
    fn update_should_notify(&self, old: &Self, new: &Self) -> bool {
        old.value() != new.value()
    }
}
```

A blanket `impl<T: InheritedWidget> Widget for T` provides:
- `create_element()` → `InheritedElement`
- `create_render_object()` → reuses the existing `ProxyRenderObject` from
  `stateful_widget.rs` (no paint, passes layout/hit-test through to child —
  identical role as for `Component`)
- `child()` → delegates to `InheritedWidget::child()`
- `key()` → delegates to `InheritedWidget::key()`
- `clone_boxed()` → `Box::new(self.clone())` (requires `Clone` supertrait)
- `as_any()` → `self`

### `InheritedElement`

Stores the widget + the `TypeId` key under which it registered. Lifecycle:

- **`mount`:**
  1. Register self in `InheritedRegistry` under
     `TypeId::of::<W::Value>()`, storing `widget.value().clone()`.
  2. Copy parent's `inherited_map`, insert
     `TypeId::of::<W::Value>() → self.id`. This becomes the map this element's
     subtree sees. Store the new `Arc<InheritedMap>` in the pipeline's
     `SecondaryMap` so children can read it.
  3. `mount_render_object()` (pass-through proxy).
  4. Inflate the child — child's mount copies *this* element's map.

- **`update(new_widget)`:**
  1. Compare `update_should_notify(old, new)`.
  2. If true:
     - `inherited_registry.update_value(self.id, new.value().clone())`
     - Iterate `inherited_registry.dependents_for(self.id)` and call
       `BuildOwner::mark_needs_build(dep)` for each.
  3. Store new widget, reconcile child as usual.

- **`unmount`:**
  1. `inherited_registry.remove_provider(self.id)` — drops the value and its
     dependents entry.
  2. Pipeline clears the `SecondaryMap` entry (drops the `Arc<InheritedMap>`).
  3. Standard child unmount + render-object removal + focus detach — same as
     `SafeAreaElement`.

### `InheritedRegistry`

Lives next to `ElementRegistry`, owned by the pipeline. Uses `RefCell`
internally so it can be borrowed as `&InheritedRegistry` from both
`ElementContext` and `RenderContext` while methods take `&self`:

```rust
pub struct InheritedRegistry {
    /// The value each provider exposes, keyed by provider element.
    values: RefCell<HashMap<ElementKey, Box<dyn Any + Send + Sync>>>,

    /// Dependents per (provider, type). Used by InheritedElement::update
    /// to mark dependents dirty when the value changes.
    dependents: RefCell<HashMap<ElementKey, HashMap<TypeId, HashSet<ElementKey>>>>,
}
```

Methods:
- `register_provider(&self, key: ElementKey, type_id: TypeId, value: Box<dyn Any>)`
- `update_value(&self, key: ElementKey, value: Box<dyn Any>)`
- `remove_provider(&self, key: ElementKey)` — also drops its dependents
- `add_dependent(&self, provider: ElementKey, type_id: TypeId, dep: ElementKey)`
- `value::<V>(&self, provider: ElementKey) -> Option<std::cell::Ref<'_, V>>` — downcast + read. Returns a `Ref` guard because values live behind a `RefCell`; the guard keeps the borrow alive for the caller's read. Callers typically `.clone()` out of the guard (as `Theme::of` does).
- `dependents_for(&self, provider: ElementKey) -> Vec<ElementKey>` — returns a snapshot `Vec` (not an iterator over the RefCell borrow) so callers can iterate without holding the borrow while calling `mark_needs_build`.

### `InheritedMap` (the per-element cache)

```rust
pub struct InheritedMap {
    inner: HashMap<TypeId, ElementKey>,
}
```

Stored per-element via `Arc<InheritedMap>` (clone-on-write at
`InheritedElement`). On the context side, both `ElementContext` and
`RenderContext` hold `&InheritedMap` — render gets it as a borrow, no
allocation.

The lookup helper lives on `RenderContext`:

```rust
impl RenderContext<'_> {
    /// Read the nearest inherited value of type `V`. Establishes a
    /// dependency: caller rebuilds when the provider's value changes.
    ///
    /// Returns `None` if no ancestor provides `V`. The returned `Ref` guard
    /// keeps the registry's value borrow alive; callers typically `.clone()`
    /// out of it (values are `Clone + PartialEq` by trait requirement).
    pub fn depend_on_inherited_widget::<V: 'static>(&mut self) -> Option<std::cell::Ref<'_, V>> { ... }
}
```

`&mut self` because it mutates the registry's `dependents` (via `RefCell`).
This matches Flutter's "lookup-is-dependency" semantics.

## The `Theme` Built-in

A concrete `InheritedWidget` implementation proving the ergonomic pattern. Lives
in `vexo/src/widgets/theme.rs`.

### `ThemeData`

The immutable value exposed to descendants:

```rust
#[derive(Clone, PartialEq)]
pub struct ThemeData {
    pub primary: Color,
    pub on_primary: Color,
    pub background: Color,
    pub on_background: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub error: Color,
    pub on_error: Color,
}
```

Core Material-ish color roles only — easy to extend later without breaking
dependents (a new field is additive). Ships two presets:

```rust
impl ThemeData {
    pub fn light() -> Self { ... }
    pub fn dark() -> Self { ... }
}
```

### `Theme` widget

```rust
pub struct Theme {
    data: ThemeData,
    child: Box<dyn Widget>,
}

impl Theme {
    pub fn new(data: ThemeData, child: impl Widget + 'static) -> Self { ... }
}
```

`impl InheritedWidget for Theme` with `type Value = ThemeData`. The blanket
`impl<T: InheritedWidget> Widget for T` gives it `create_element →
InheritedElement` and a pass-through render object for free.

### `Theme::of(ctx)` lookup

```rust
impl Theme {
    /// Read the nearest ancestor `Theme`. Establishes a dependency:
    /// caller rebuilds when the theme data changes.
    ///
    /// Falls back to `ThemeData::light()` when no `Theme` ancestor exists,
    /// so tests and small demos that don't wrap a `Theme` get sensible colors.
    pub fn of(ctx: &mut RenderContext) -> ThemeData {
        match ctx.depend_on_inherited_widget::<ThemeData>() {
            Some(guard) => guard.clone(),
            None => ThemeData::light(),
        }
    }
}
```

### Why a separate `ThemeData` value type (not `Theme` itself)

The lookup is `depend_on_inherited_widget::<ThemeData>()`, keyed by
`TypeId::of::<ThemeData>()`. This means:
- The **widget type** (`Theme`) can be renamed/subclassed without breaking
  lookups.
- If a second `InheritedWidget` also exposes `ThemeData` (unlikely but legal),
  the nearest one wins — predictable, type-based dispatch.

This matches Flutter: `Theme.of(context)` finds the nearest `InheritedTheme`,
keyed by the *value* type, not the widget type.

### Usage pattern

```rust
// App root: provide a theme
fn render(&self, _state, ctx) -> Box<dyn Widget> {
    Box::new(Theme::new(ThemeData::dark(), MyScreen::new()))
}

// Descendant: consume it
impl Component for MyButton {
    fn render(&self, _state, ctx) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        Text::new("Press")
            .color(theme.on_primary)
            .background(theme.primary)
            .boxed()
    }
}
```

To switch themes at runtime, an ancestor `Component` holds a
`Signal<ThemeData>` (or uses `setState`), and rebuilds with
`Theme::new(new_data, child)`. Dependents rebuild automatically.

## Integration with Existing Pipeline

### `ElementContext` — new fields

```rust
pub struct ElementContext<'a> {
    // ... existing fields ...
    pub inherited_map: &'a InheritedMap,           // nearest-ancestor cache (read-only here)
    pub inherited_registry: &'a InheritedRegistry, // for register/remove at mount/unmount
}
```

Both borrowed. Plumbed through `ElementContext::new(...)` as two additional
args.

### `RenderContext` — new fields

```rust
pub struct RenderContext<'a> {
    // ... existing fields ...
    pub inherited_map: &'a InheritedMap,
    pub inherited_registry: &'a InheritedRegistry, // RefCell inside for depend() side-effect
}
```

`RenderContext` is constructed by `StatefulElement` during `mount`/`update`/
`rebuild_from_state` (see `build_child_widget` at `stateful_widget.rs:462`).
That method currently takes `dirty`, `render_objects`, `build_owner`. It gains
`inherited_map: &InheritedMap` and `inherited_registry: &InheritedRegistry` —
threaded from the `ElementContext` the `StatefulElement` received.

### `InheritedMap` propagation

A new `SecondaryMap<ElementKey, Arc<InheritedMap>>` on the pipeline stores the
per-element map, so children can look up their parent's map. Cleared on
unmount.

The pipeline computes the map when constructing `ElementContext`:

```rust
let inherited_map = match parent {
    None => Arc::new(InheritedMap::empty()),                       // root
    Some(p) => inherited_maps.get(p).cloned().unwrap_or_default(), // Arc clone
};
```

`InheritedElement::mount` does copy-on-write:

```rust
let mut map = (*parent_map).clone();      // HashMap clone, cheap (few entries)
map.insert(TypeId::of::<W::Value>(), self.id);
let arc = Arc::new(map);
context.store_inherited_map(arc.clone()); // for children to read
```

### Who registers as a provider

Only `InheritedElement::mount` calls
`inherited_registry.register_provider(self.id, TypeId::of::<W::Value>(),
Box::new(widget.value().clone()))`. Storing the **value** (not the widget)
means `value::<V>()` lookups don't need to touch the element tree — just the
registry.

### Rebuild ordering — why existing machinery suffices

`BuildOwner::sort_dirty_by_depth` already ensures parents rebuild before
children. When a `Theme` value changes:

1. Provider marks all dependents dirty (insertion order into `dirty_elements`).
2. `sort_dirty_by_depth` reorders by tree depth before drain.
3. Each dependent rebuilds in depth order, calling `Component::render()` →
   `Theme::of(ctx)` reads the *already-updated* registry value.

No new scheduling logic needed. The requirement — `update_value` happens
**before** the dirty dependents are drained — is guaranteed because `update()`
runs during reconciliation, before the next rebuild pass.

### Unmount

`InheritedElement::unmount`:
1. `inherited_registry.remove_provider(self.id)` — drops value + dependents
   entry.
2. Pipeline clears the `SecondaryMap` entry (drops the `Arc<InheritedMap>`).
3. Standard child unmount + render-object removal + focus detach.

Vexo never reparents, so a dependent is always unmounted before or alongside
its provider. No dangling-dependent problem.

### Files touched

Minimal footprint:

| File | Change |
|---|---|
| `element_context.rs` | +2 fields, +2 args on `new()` |
| `stateful_widget.rs` (`build_child_widget`, `RenderContext`) | +2 fields on `RenderContext`, thread through |
| `pipeline.rs` | Own `InheritedRegistry`, own `SecondaryMap` for maps, pass to `ElementContext::new` |
| `reconciler.rs` (or wherever `ElementContext` is constructed) | Compute `inherited_map` from parent |
| `lib.rs` | Re-export `InheritedWidget`, `Theme`, `ThemeData` |
| `widgets/mod.rs` | New `theme` submodule, re-export |
| `widgets/theme.rs` | New file — `Theme`, `ThemeData` |
| `inherited_widget.rs` (new) | `InheritedWidget` trait, `InheritedElement`, blanket `impl Widget` |
| `inherited_registry.rs` (new) | `InheritedRegistry`, `InheritedMap` |

No changes to the `Element` trait, `Widget` trait, `RenderObject` trait,
`BuildOwner`, `ElementRegistry`, or any existing widget/element.

## Error Handling & Edge Cases

### Error handling

Fail-soft by design — no `Result` returns on the hot path:

| Case | Behavior |
|---|---|
| `Theme::of(ctx)` with no ancestor provider | Returns fallback (`ThemeData::light()`). Documented. |
| `depend_on_inherited_widget::<V>()` with no provider | Returns `None`. Caller decides fallback. |
| Provider unmounted while dependents still live | Cannot happen — Vexo never reparents; dependents (descendants) unmount first or simultaneously. |
| Registry `RefCell` borrow panic | Only if a callback re-enters the registry mid-mutation. `InheritedElement::mount`/`update`/`unmount` don't invoke user code while holding the borrow, so structurally prevented. |
| Duplicate provider for same `TypeId` in ancestor chain | Nearest wins (map insert overwrites). Intentional, matches Flutter. |

No panics on the lookup path.

### Edge cases

- **Nested providers of the same type:** `Theme(dark) → ... → Theme(light) →
  descendant`. Descendant sees `light` (nearest ancestor). When the outer
  `Theme` updates, descendants *between* the two providers rebuild (they depend
  on the outer); the inner descendant does not (it depends on the inner).
  Tracked correctly because `dependents` is keyed by provider element key, not
  type.

- **Dependent rebuilds, reads a *different* provider:** Can happen if an
  ancestor `Component` rebuilds and swaps which `InheritedWidget` is in its
  subtree. The dependent's rebuild re-runs `Theme::of(ctx)` → re-reads the map
  → gets the new nearest provider → registers dependency on the **new**
  provider. The stale dependency on the old provider is dropped when the old
  provider unmounts (clearing its dependents entry). Correct, no leak.

  Note: stale dependency on the old provider persists until it unmounts. If the
  old provider stays mounted elsewhere in the tree, the dependent is still in
  its `dependents` set, but the dependent's *next* rebuild re-registers on the
  new provider, and the old provider's `update` would still mark the dependent
  dirty (a spurious rebuild). Flutter has the same behavior. We accept it; it's
  rare and self-correcting.

- **Provider value changes during a dependent's own rebuild:** The dependent
  reads the new value (registry updated before rebuild pass). No cycle.

- **`InheritedWidget` with no child:** Valid — `child: Box<dyn Widget>` is
  required, but the child can be a no-op. Provider still registers.

- **`InheritedWidget` used as the root:** `parent_map` is `None` → empty
  `InheritedMap` → provider inserts into empty map. Works.

## Testing Strategy

Three layers, bottom-up:

### Unit tests (in-module, `#[cfg(test)]`)

1. `InheritedRegistry` — register/remove/value/dependents, in isolation.
2. `InheritedMap` — empty, insert, clone-and-insert (COW semantics).
3. `ThemeData` — `light()`/`dark()` differ; `PartialEq` correctness.
4. `Theme` widget — `InheritedWidget::value()`, `update_should_notify()` true/false.

### Element-level integration tests (new `inherited_integration_test.rs`)

5. Mount a `Theme` with a child `Component` that calls `Theme::of(ctx)`. Assert
   the child read the provided value.
6. Update the `Theme` with new data. Assert the child element is marked dirty
   (`BuildOwner::is_dirty(child)`).
7. Drain dirty, rebuild child. Assert child read the *new* value.
8. No provider present → `Theme::of(ctx)` returns `light()` fallback.
9. Nested `Theme`s: descendant reads nearest; outer update rebuilds middle, not
   descendant.
10. Provider unmount → its `dependents` entry is gone (registry query).

These mirror the existing `stateful_integration_test.rs` /
`passthrough_integration.rs` harness — construct `ElementContext` manually,
drive `mount`/`update`/`unmount` directly, assert on observed state.

### End-to-end test (optional, in `integration_tests.rs`)

11. A minimal app: `Theme::new(dark, Counter)`. `Counter::render` reads
    `Theme::of(ctx)` and shows a label with `theme.on_background`. Toggle theme
    via a `Signal<ThemeData>` at the root. Re-render and assert the label color
    changed. Validates full pipeline integration; secondary to tests 5–10.

## Out of Scope

- **Aspect-based dependencies** (`InheritedModel`) — ruled out; whole-value
  dependency only.
- **Migrating `safe_area_source` / `focused_element` onto `InheritedWidget`** —
  ruled out; these stay on `BuildOwner` as global side-channels for now.
- **`MediaQuery`, `Directionality`, other built-ins** — ruled out; only
  `Theme` ships.
- **Observable/`Signal`-based inherited values** — ruled out; immutable +
  rebuild-driven only.
