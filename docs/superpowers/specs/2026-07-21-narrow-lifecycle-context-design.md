# Narrow LifecycleContext — Design

**Date:** 2026-07-21
**Status:** Proposed
**Scope:** `vexo` crate (`vexo/src/stateful_widget.rs` primarily; two docstring
touches in `vexo/src/pipeline.rs` and `vexo/src/reconciler.rs`)

## Motivation

`LifecycleContext` (`vexo/src/stateful_widget.rs:205-327`) is passed to every
`ComponentState` lifecycle hook (`on_mount`, `on_update`, `on_unmount`,
`on_rebuild`). It currently exposes:

- Fields (all private already): `element_id`, `build_owner`, `widget`,
  `dirty_callback`, `animation_ticker`.
- Mutating methods: `setState()`, `request_rebuild()`.
- Read methods: `element_id()`, `widget()`, `dirty_callback()`,
  `animation_ticker()`.
- Side-effect method: `clear_focus()`.

A grep across every lifecycle hook in the workspace shows that user code calls
only **four** of these:

| Method | Callsites |
|---|---|
| `widget()` | 11 — `navigation.rs`, `tab_bar.rs`, `text_edit.rs`, `transitions.rs`, `chat_screen.rs`, `stateful_integration_test.rs` |
| `dirty_callback()` | 9 — same set, wiring controllers to the rebuild trigger |
| `animation_ticker()` | 4 — `navigation.rs`, `transitions.rs` (×2) |
| `clear_focus()` | 1 — `navigation.rs:512` (the recently-migrated `on_rebuild` callsite) |

No user lifecycle hook calls `setState()`, `request_rebuild()`, or
`element_id()`. The three unused methods are pure API surface frozen into the
public shape.

The `element_id` field is consumed only by those three unused methods —
`setState` and `request_rebuild` both call
`self.build_owner.mark_needs_build(self.element_id)`, and `element_id()` is its
getter. With the methods gone, the field is dead too.

Meanwhile the actual rebuild-trigger path runs through `dirty_callback()` →
`dirty_sender` → `BuildOwner::mark_needs_build()`, completely bypassing
`setState`/`request_rebuild`. `Signal` (auto-wired by
`#[derive(ComponentState)]`) is the idiomatic state-mutation path. The current
`LifecycleContext` struct docstring (line 196) calls `setState()` "the key
method" — that is misleading; it has zero callers.

This mirrors the situation that motivated the `RenderContext` narrowing
(`docs/superpowers/specs/2026-07-21-narrow-render-context-design.md`): dead
mutators and unused reads frozen into a public context object. Same first
principles apply:

- **Declarative over imperative** — lifecycle hooks should wire subscriptions
  and trigger side-effects through well-scoped primitives (dirty callback,
  clear_focus), not through a generic `setState(state, |s| ...)` that
  duplicates what `Signal` already does.
- **Encapsulation** — dead methods on a public trait object confuse readers
  and invite callers to use the wrong path.
- **YAGNI** — `element_id` on `LifecycleContext` was speculative API; no
  hook uses it.

## Goals

- Remove `setState`, `request_rebuild`, `element_id()` from
  `LifecycleContext`'s public surface.
- Drop the now-dead `element_id` field.
- Drop `element_id` from the `LifecycleContext::new()` signature.
- Reword docstrings/comments that reference the removed `setState` so they
  describe the actual mechanism (`Signal::set` / dirty callback).
- Preserve all current behavior. No hook changes behavior; the removed
  methods were dead.

## Non-Goals

- No changes to `RenderContext`. Already narrowed.
- No changes to `EventContext` or `ElementContext`. Different context objects.
- No splitting of `BuildOwner`. Still a grab-bag (dirty elements, focus state,
  safe-area source, deferred unfocus); separate refactor.
- No removal of `BuildOwner` exposure from `LifecycleContext`. It stays as a
  private field powering `clear_focus`.
- No new abstraction types. Private fields enforce encapsulation fine.
- No changes to `ComponentState::on_event` signature (uses `EventContext`,
  not `LifecycleContext`).
- No typed wrapper around `widget()` to avoid the `&dyn Any` downcast dance.
  Separate concern.

## Design

### Narrowed `LifecycleContext` public API

```rust
pub struct LifecycleContext<'a> {
    // REMOVED: element_id: ElementKey  (no remaining method needs it)

    build_owner: &'a BuildOwner,                       // private; powers clear_focus
    widget: &'a dyn Any,                               // private; powers widget()
    dirty_callback: Arc<dyn Fn() + Send + Sync>,       // private; powers dirty_callback()
    animation_ticker: Arc<AnimationTicker>,            // private; powers animation_ticker()
}

impl<'a> LifecycleContext<'a> {
    /// Private constructor — unchanged visibility (module-private).
    /// Signature drops the `element_id` arg.
    fn new(
        build_owner: &'a BuildOwner,
        widget: &'a dyn Any,
        dirty_callback: Arc<dyn Fn() + Send + Sync>,
        animation_ticker: Arc<AnimationTicker>,
    ) -> Self { ... }

    // --- reads (used) ---
    pub fn widget(&self) -> &dyn Any
    pub fn dirty_callback(&self) -> Arc<dyn Fn() + Send + Sync>
    pub fn animation_ticker(&self) -> &Arc<AnimationTicker>

    // --- side-effect (used from on_rebuild) ---
    pub fn clear_focus(&self)

    // REMOVED:
    //   pub fn setState<S, F>(&mut self, state: &mut S, callback: F)
    //   pub fn request_rebuild(&self)
    //   pub fn element_id(&self) -> ElementKey
}
```

**What changes:**

- 3 methods removed: `setState`, `request_rebuild`, `element_id()`.
- 1 field removed: `element_id` (only consumed by the removed methods).
- Constructor signature drops `element_id` as its first arg.
- 4 internal callsites of `LifecycleContext::new` — `mount` (line 592),
  `update` (line 658), `unmount` (line 703), `rebuild_from_state` (line 769)
  — each drops the `element_id` first arg.
- 1 in-module test callsite at line 1516 drops the `element_id` arg.

**What stays:**

- All fields remain private.
- Constructor remains module-private (`fn new`, not `pub fn new`). No external
  code constructs `LifecycleContext`; exposing the constructor would freeze
  `BuildOwner`, `AnimationTicker`, and the dirty-callback type into the public
  API surface — a widening, not a narrowing.
- The four methods that user code actually calls are unchanged in behavior
  and signature.

### Docstring rewrites

Five places currently reference the removed `setState` and mislead readers.

**1. `vexo/src/stateful_widget.rs:196`** — LifecycleContext struct doc.

Current:
> The key method is `setState()`, which mutates state and marks the
> element dirty for rebuild.

Reword to:
> State mutations trigger rebuilds through `Signal` (auto-wired by
> `#[derive(ComponentState)]`) or by calling the `dirty_callback()` exposed
> here — both end up at `BuildOwner::mark_needs_build()`.

**2. `vexo/src/stateful_widget.rs:94`** — `on_rebuild` hook doc.

Current:
> Called once before each state-driven rebuild (setState / Signal::set),

Reword to:
> Called once before each state-driven rebuild (triggered by `Signal::set`
> or the dirty callback),

**3. `vexo/src/stateful_widget.rs:756`** — comment in `rebuild_from_state`.

Current:
> This is called by perform_rebuilds() when setState() or
> Signal::set() marked this element dirty.

Reword to:
> when a `Signal::set` or dirty-callback invocation marked this element
> dirty.

**4. `vexo/src/pipeline.rs:320`** — comment.

Current:
> Elements call this when their state changes (e.g., setState equivalent).

Reword to:
> Elements call this when their state changes (via `Signal::set` or the
> dirty callback).

**5. `vexo/src/reconciler.rs:191`** — comment.

Current:
> First, perform any pending state-driven rebuilds (from setState)

Reword to:
> from `Signal::set` / dirty-callback invocations.

**Kept as-is:**

- `vexo/src/build_owner.rs:30` — "In Flutter, when `setState()` is
  called..." — Flutter comparison, not a reference to Vexo's removed method.
- `vexo/src/pipeline.rs:692` — "This follows Flutter's model: focus change
  → setState() → ..." — same, describing Flutter's model.

### Test impact

**In-module test — `vexo/src/stateful_widget.rs:1503`
(`test_lifecycle_context_clear_focus_requests_unfocus`)**

Currently:
```rust
let ctx = LifecycleContext::new(
    element_id,
    &build_owner,
    &widget as &dyn Any,
    dirty_callback,
    animation_ticker,
);
```

Update to drop the `element_id` arg and its now-unused `make_element_key()`
binding:
```rust
let ctx = LifecycleContext::new(
    &build_owner,
    &widget as &dyn Any,
    dirty_callback,
    animation_ticker,
);
```

The `let element_id = make_element_key();` line above is removed (no other
reference in this test).

**Tests for `setState` / `request_rebuild` / `element_id()`:**

None exist. Grep confirms zero test functions reference them. Nothing to
delete.

**External tests:**

None construct `LifecycleContext` directly (constructor is module-private;
tests outside the module go through `ThreeTreePipeline`). Zero changes.

**New tests:**

None. This is pure surface narrowing with no new behavior to lock in. The
existing `test_lifecycle_context_clear_focus_requests_unfocus` already
exercises the constructor and `clear_focus` after the change.

## Migration Plan

Single-phase — one commit, no behavior change. Phasing was justified for
`RenderContext` because it migrated a live callsite (`clear_focus` from
`render` to `on_rebuild`) and added a new hook. Here, the removed methods are
dead — there is nothing to migrate.

**Steps:**

1. In `vexo/src/stateful_widget.rs`:
   - Remove `element_id: ElementKey` field from `LifecycleContext`.
   - Remove `setState`, `request_rebuild`, `element_id()` methods (including
     their docstrings).
   - Drop `element_id` from `LifecycleContext::new()` signature.
   - Update the 4 internal callsites: `mount` (line 592), `update`
     (line 658), `unmount` (line 703), `rebuild_from_state` (line 769) —
     each drops the `element_id` first arg.
   - Update the in-module test callsite at line 1516 — drop the `element_id`
     arg and the now-unused `let element_id = make_element_key();` line.
   - Reword docstrings at lines 94, 196, 756 per the list above.
2. Reword comments at `vexo/src/pipeline.rs:320` and
   `vexo/src/reconciler.rs:191`.
3. Build: `cargo build -p vexo`, `cargo build -p vexo_uikit`,
   `cargo build -p shared_app`.
4. Test: `cargo test --workspace`.

No manual GUI run needed — pure API surface narrowing with zero behavior
change. No callsite changes behavior; the removed methods were dead.

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Hidden caller of `setState`/`request_rebuild`/`element_id()` in code not grepped | Very low | Compile error | `cargo build --workspace` catches it. Two independent greps (`ctx.setState`, `setState(`, `request_rebuild(`, `ctx.element_id()` filtered to `LifecycleContext`) found zero user callsites. |
| Constructor signature change breaks a future test that gets merged in parallel | Very low | Compile error | Constructor is module-private; only `stateful_widget.rs` can call it. |
| Docstring rewrite introduces inaccuracy | Low | Misleading docs | Each rewrite substitutes the actual mechanism (`Signal::set` / dirty callback) for the removed `setState`. |
| `element_id` field removal breaks a future feature that needs element identity in a lifecycle hook | Low | Re-add the field when needed | YAGNI — no current or planned hook uses it. `widget()`, `dirty_callback()`, `animation_ticker()`, `clear_focus()` cover every real use case today. |

## Out of Scope

- `RenderContext` (already narrowed).
- `EventContext` or `ElementContext` changes.
- Splitting `BuildOwner` (still a grab-bag of dirty tracking, focus state,
  safe-area source, deferred unfocus).
- Removing `BuildOwner` exposure from `LifecycleContext` (it stays as a
  private field powering `clear_focus`).
- Adding a typed wrapper around `widget()` to avoid the `&dyn Any` downcast
  dance.
- Splitting `LifecycleContext` into public + internal types.
- Adding an `Environment` sub-object for reads.

These are legitimate future refactors but unrelated to the current goal:
removing dead mutators and unused reads from `LifecycleContext`.
