# Narrow RenderContext — Design

**Date:** 2026-07-21
**Status:** Proposed
**Scope:** `vexo`, `vexo_uikit`, `shared_app` crates

## Motivation

`RenderContext` (`vexo/src/stateful_widget.rs:309-400`) is passed to every
`Component::render()` call. It currently exposes:

- All fields as `pub`: `dirty`, `render_objects`, `build_owner`,
  `inherited_map`, `inherited_registry`, `element_id`.
- Mutating methods: `request_rebuild()`, `mark_needs_layout()`,
  `mark_needs_paint()`, `clear_focus()`.
- Read methods: `is_focused()`, `safe_area()`, `element_id()`.
- Dependency subscription: `depend_on_inherited_widget::<V>()`.

A grep across all `render()` callsites in the workspace shows that user
`render()` code calls only **four** of these:

| Method | Callsite |
|---|---|
| `is_focused()` | `vexo/src/widgets/text_edit.rs:588` |
| `clear_focus()` | `vexo_uikit/src/navigation.rs:516` |
| `depend_on_inherited_widget()` | `vexo/src/widgets/theme.rs:103` (`Theme::of`) |
| `safe_area()` | `SafeArea` widget |

No user `render()` code calls `request_rebuild()`, `mark_needs_layout()`,
`mark_needs_paint()`, or touches `dirty` / `render_objects` /
`build_owner` / `inherited_map` / `inherited_registry` directly. The
leakage is unused on the user side; it is pure API surface frozen into
the public shape.

This violates CLAUDE.md's first principles:

- **Declarative over imperative** — `render()` should be a pure
  function of `(state, environment) → widget tree`. Mutating methods
  during render invite ordering bugs and re-entrancy.
- **Scope over global** — framework registries (`DirtyTracking`,
  `RenderObjectRegistry`) are scoped to the element layer, not the
  widget layer.
- **Encapsulation** — `pub` fields freeze internal types
  (`BuildOwner`, `InheritedMap`) into the public API.

The one mutating method that *is* used from `render()` — `clear_focus()`
in `NavigationStackView::render()` — is a workaround. Its docstring
(`stateful_widget.rs:357-372`) admits it: `clear_focus()` exists on
`RenderContext` *because* render doesn't have `FocusManager`, so the
call has to be deferred via `BuildOwner`. Semantically it is a
state-driven side-effect (a navigation pop is starting), not a
render-time computation. It belongs in a lifecycle hook, not in
`render()`.

## Goals

- Make all `RenderContext` fields private.
- Delete the unused mutating methods (`request_rebuild`,
  `mark_needs_layout`, `mark_needs_paint`).
- Move `clear_focus()` off `RenderContext` and into a new
  `ComponentState::on_rebuild` lifecycle hook, with the implementation
  exposed on `LifecycleContext`.
- Drop `dirty` and `render_objects` from `RenderContext`'s field set
  entirely — they belong to the element layer (`ElementContext`), not
  the widget-facing context.
- Preserve all current behavior, including the deferred-unfocus
  semantics of `clear_focus()` (still stashed on `BuildOwner`, applied
  by the pipeline after `perform_rebuilds()` returns).

## Non-Goals

- No changes to `LifecycleContext` beyond adding `clear_focus()`. Its
  existing shape is fine.
- No changes to `EventContext`. The `ctx.clear_focus()` at
  `vexo/src/event_context.rs:399` is a different `ctx` (EventContext),
  unrelated to this refactor.
- No splitting of `BuildOwner`. It has become a grab-bag (dirty
  elements, focus state, safe-area source, deferred unfocus) but that
  is a separate refactor.
- No removal of `BuildOwner` exposure from `LifecycleContext`. Separate
  concern.
- No new abstraction types (no `Environment` sub-object, no
  `RenderCtxInner`). Private fields enforce encapsulation fine.

## Design

### Narrowed `RenderContext` public API

```rust
pub struct RenderContext<'a> {
    element_id: ElementKey,
    build_owner: &'a BuildOwner,                // private
    inherited_map: &'a InheritedMap,            // private
    inherited_registry: &'a InheritedRegistry,  // private
    // REMOVED: dirty, render_objects
}

impl<'a> RenderContext<'a> {
    /// Public constructor. Takes only the fields user render() needs.
    pub fn new(
        element_id: ElementKey,
        build_owner: &'a BuildOwner,
        inherited_map: &'a InheritedMap,
        inherited_registry: &'a InheritedRegistry,
    ) -> Self { ... }

    // --- reads ---
    pub fn element_id(&self) -> ElementKey
    pub fn is_focused(&self) -> bool
    pub fn safe_area(&self) -> EdgeInsets

    // --- dependency subscription (the only &mut self method) ---
    pub fn depend_on_inherited_widget<V: Clone + 'static>(&mut self) -> Option<V>
}
```

Removed from `RenderContext`:
- All `pub` fields → private.
- `request_rebuild()`, `mark_needs_layout()`, `mark_needs_paint()` →
  deleted (zero callers in user `render()`).
- `clear_focus()` → moved to `LifecycleContext` (see below).
- `dirty` and `render_objects` fields → removed entirely. The element
  layer continues to own them via `ElementContext`; user `render()` does
  not need them.

### New `on_rebuild` lifecycle hook

```rust
pub trait ComponentState {
    // ...existing hooks (on_mount, on_update, on_unmount, on_tick, on_event)...

    /// Called once before each state-driven rebuild (setState / Signal::set),
    /// NOT before parent-widget updates (that's `on_update`).
    ///
    /// Use for side-effects that must happen at the start of a rebuild pass:
    /// clearing focus when a navigation transition begins, dismissing a
    /// pending modal, etc. `render()` itself must stay pure.
    ///
    /// Default: no-op.
    fn on_rebuild(&mut self, _ctx: &mut LifecycleContext) {}
}
```

`on_rebuild` fires **only** on state-driven rebuilds. Parent-widget
updates go through `on_update`, not `on_rebuild`. This is the right
semantic: `clear_focus` on a navigation pop is a state-driven
side-effect (the controller's `pending()` op flipped), not a
parent-config response.

### `clear_focus` migration to `LifecycleContext`

```rust
impl LifecycleContext<'_> {
    /// Same deferred-unfocus semantics as the old RenderContext::clear_focus():
    /// stashed on BuildOwner, applied by the pipeline after
    /// perform_rebuilds() returns.
    ///
    /// Safe to call from `on_rebuild`, `on_mount`, `on_update`. No-op when
    /// nothing is focused.
    pub fn clear_focus(&self) {
        self.build_owner.request_unfocus();
    }
}
```

The `BuildOwner::request_unfocus()` / `take_unfocus_request()` machinery
stays as-is. Only the *caller path* changes: instead of
`RenderContext::clear_focus()` called from `render()`, it's
`LifecycleContext::clear_focus()` called from `on_rebuild`.

### `StatefulElement::rebuild_from_state` wiring

```rust
fn rebuild_from_state(&mut self, context: &mut ElementContext) {
    let element_id = self.id.unwrap_or(context.element_id);

    // NEW: fire on_rebuild before building the child widget.
    // This is the only place state-driven side-effects run.
    {
        let tx = context.dirty_sender.clone();
        let dirty_callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = tx.send(element_id);
        });
        let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
        let mut lifecycle_ctx = LifecycleContext::new(
            element_id,
            context.build_owner,
            &self.widget as &dyn Any,
            dirty_callback,
            context.animation_ticker.clone(),
        );
        state_ref.on_rebuild(&mut lifecycle_ctx);
    }

    // Then build the child widget with the narrowed RenderContext.
    let child_widget = {
        let state_ref = context.state.get_mut::<W::State>(element_id).unwrap();
        self.build_child_widget(
            element_id,
            state_ref,
            context.build_owner,
            context.inherited_map,
            context.inherited_registry,
        )
    };

    // ...existing reconcile logic unchanged...
}
```

`on_rebuild` is **not** called from `mount()` or `update()` — only from
`rebuild_from_state()`. The first render of an element goes through
`mount()` → `build_child_widget()` directly; parent-widget changes go
through `update()` → `on_update()` → `build_child_widget()`.

### `build_child_widget` signature

Drops the `dirty` and `render_objects` arguments:

```rust
fn build_child_widget(
    &self,
    element_id: ElementKey,
    state: &mut W::State,
    build_owner: &BuildOwner,
    inherited_map: &InheritedMap,
    inherited_registry: &InheritedRegistry,
) -> Box<dyn Widget> {
    let mut render_ctx = RenderContext::new(
        element_id,
        build_owner,
        inherited_map,
        inherited_registry,
    );
    self.widget.render(state, &mut render_ctx)
}
```

Three callsites in `StatefulElement` (mount, update, rebuild_from_state)
each drop the two args.

### `NavigationStackView` migration

The `ctx.clear_focus()` call at `vexo_uikit/src/navigation.rs:516`
moves from `render()` into `on_rebuild()`:

```rust
impl<Dest: Hash + Eq + Clone + 'static> ComponentState
    for NavigationStackViewState<Dest>
{
    fn on_rebuild(&mut self, ctx: &mut LifecycleContext) {
        // Was: ctx.clear_focus() inside render() when a pending op
        // was observed. Now: same check, same call, but in the
        // lifecycle hook — render() stays pure.
        if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
            if self.transition.is_none() && nav.controller.pending().is_some() {
                ctx.clear_focus();
            }
        }
    }

    // ...on_mount, on_update, on_unmount, on_tick unchanged...
}
```

The transition-start logic that follows in `render()` (the
`if state.transition.is_none() { ... start transition ... }` block at
navigation.rs:498) is unaffected — `on_rebuild` fires, then `render()`
builds the transition tree.

The `state.ticker` / `state.dirty_callback` cached in `on_mount` /
`on_update` remain valid: `on_rebuild` runs after those hooks have
already wired them.

## Test Impact

### In-crate unit tests (`vexo/src/stateful_widget.rs:1333-1494`)

- `test_render_context_request_rebuild` (line 1333) — **delete**.
  Tests the removed `request_rebuild()` method.
- `test_render_context_is_focused` (line 1364) — **update** to use
  `RenderContext::new(...)` instead of struct literal.
- `depend_on_inherited_widget_returns_value_when_provider_present`
  (line 1417) — **update** to use `RenderContext::new(...)`.
- `depend_on_inherited_widget_returns_none_when_no_provider`
  (line 1445) — **update** to use `RenderContext::new(...)`.
- `depend_on_inherited_widget_registers_dependent` (line 1468) —
  **update** to use `RenderContext::new(...)`.

### External test helpers

Three external test files have a `create_render_context` helper that
constructs `RenderContext` via struct literal:

- `vexo_uikit/tests/navigation_stack_tests.rs:208-224`
- `vexo_uikit/tests/navigation_animation_tests.rs:43-...`
- `vexo_uikit/tests/button_render_tests.rs:13-29`

All three simplify to:

```rust
fn create_render_context<'a>(
    element_id: ElementKey,
    build_owner: &'a BuildOwner,
    inherited_map: &'a InheritedMap,
    inherited_registry: &'a InheritedRegistry,
) -> RenderContext<'a> {
    RenderContext::new(element_id, build_owner, inherited_map, inherited_registry)
}
```

The `dirty: &mut DirtyTracking` and `render_objects: &mut RenderObjectRegistry`
arguments and locals drop out at every callsite. Callers that still
instantiate `DirtyTracking::new()` / `RenderObjectRegistry::new()` for
other test purposes keep them; those that only created them to satisfy
the old constructor drop them.

### Integration tests

- `vexo/src/stateful_integration_test.rs` — goes through
  `ThreeTreePipeline`, never constructs `RenderContext` directly. Zero
  changes.
- `vexo/src/inherited_integration_test.rs:55` — passes
  `ctx: &mut RenderContext` as a function parameter but only calls
  `depend_on_inherited_widget`. Zero changes.
- `vexo/src/focus/integration_tests.rs:710` — uses `_ctx:
  &mut crate::RenderContext` as an unused parameter. Zero changes.

### New test

Add `test_on_rebuild_fires_before_rebuild` to `stateful_widget.rs`
tests: a `Component` whose `on_rebuild` sets a flag, then assert the
flag is `true` after `rebuild_from_state` runs and that `render()` was
called afterwards. Locks in the ordering guarantee.

## Migration Plan

### Phase 1: Additive (compiles, no behavior change)

1. Make `RenderContext` fields private.
2. Add `pub fn new(element_id, build_owner, inherited_map,
   inherited_registry)` constructor.
3. Add `ComponentState::on_rebuild` hook (default no-op).
4. Add `LifecycleContext::clear_focus()` (identical body to the old
   `RenderContext::clear_focus()`).
5. Update all `RenderContext` struct-literal constructors (in-crate
   tests + external test helpers) to use `RenderContext::new(...)`.
6. Update `build_child_widget` signature to drop `dirty` and
   `render_objects` args. Update its three callsites in
   `StatefulElement`.

At this point the codebase compiles with both the old
`RenderContext::clear_focus()` and the new
`LifecycleContext::clear_focus()` coexisting.

### Phase 2: Migrate the one callsite

7. Move `navigation.rs:516` `ctx.clear_focus()` into
   `NavigationStackViewState::on_rebuild`. Body of the
   `if state.transition.is_none() && self.controller.pending().is_some()`
   block moves verbatim.
8. Wire `StatefulElement::rebuild_from_state` to call
   `state.on_rebuild(&mut lifecycle_ctx)` before `build_child_widget`.

### Phase 3: Remove old surface

9. Delete `RenderContext::clear_focus()`, `request_rebuild()`,
   `mark_needs_layout()`, `mark_needs_paint()`.
10. Delete `RenderContext`'s `dirty` and `render_objects` fields.
11. Delete `test_render_context_request_rebuild`.
12. Add `test_on_rebuild_fires_before_rebuild`.

### Phase 4: Verify

13. `cargo build -p vexo`
14. `cargo build -p vexo_uikit`
15. `cargo build -p shared_app`
16. `cargo test --workspace`
17. Manual: ask user to run `cargo run -p desktop_demo` and exercise
    the navigation pop case (the only behavioral change). Verify focus
    clears correctly. The automated tests cannot catch a regression in
    the deferred-unfocus timing on iOS.

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `on_rebuild` fires at wrong time (e.g., on parent-widget update) | Low | High (clear_focus fires spuriously) | Hook called *only* from `rebuild_from_state`, not from `update`. Explicit in the docstring. Unit test `test_on_rebuild_fires_before_rebuild` locks the ordering. |
| Navigation pop behavior changes (keyboard stays up on iOS) | Low | High (visible regression) | Manual test on iOS after migration. The deferred-unfocus path through `BuildOwner` is unchanged. |
| External test helper signatures break | High | Low (compile errors only) | Phase 1's `pub fn new(...)` makes the fix mechanical. |
| Hidden caller of removed methods in a crate not grepped | Low | Medium | `cargo build --workspace` in Phase 4 catches all of them. |
| `on_rebuild` ordering vs `on_tick` (which advances transition controller) | Low | Medium | `on_tick` fires per-frame from `animate()`; `on_rebuild` fires once per state-driven rebuild. They run in distinct phases; no interleaving. |

## Out of Scope

- `LifecycleContext` narrowing beyond adding `clear_focus()`.
- `EventContext` changes.
- `BuildOwner` splitting (deferred unfocus, focus state, safe-area
  source, dirty elements all coexist today).
- Removing `BuildOwner` exposure from `LifecycleContext`.
- Splitting `RenderContext` into public + internal types.
- Adding an `Environment` sub-object for reads.

These are legitimate future refactors but unrelated to the current
goal: making `render()` pure and encapsulating framework internals.
