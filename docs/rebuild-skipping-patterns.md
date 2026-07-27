# Rebuild Skipping Patterns

**Status:** Reference doc (not a feature spec)
**Scope:** `vexo/` framework + app/widget authors

This document explains when and how Vexo skips `render()` calls to keep
hot paths (keyboard animation, scroll, gestures) within the frame budget.
App authors should read this once; framework authors should keep it in
mind whenever they touch the rebuild pipeline.

---

## Background: two rebuild paths

Every `Component` can be re-rendered through one of two paths. They have
different triggers and different rules:

| Path | Triggered by | Checks `should_rebuild()`? |
|---|---|---|
| `update()` | Parent passes a new widget down the cascade | **Yes** — skips `render()` if false |
| `rebuild_from_state()` | `BuildOwner::mark_needs_build()` — fired by `Signal::set`, `InheritedWidget` invalidation, or pipeline dirtying on resize | **No** — always re-renders |

This split is intentional: parent-cascade rebuilds are *expected* to be
no-ops when the widget is identical (frequent case), but state-driven
rebuilds are *requested* by the component itself or by a dependency it
opted into (InheritedWidget) — the component is asking to re-render, so
the framework honors the request.

The implication: **`should_rebuild()` only gates the parent-cascade path.
InheritedWidget changes, Signal sets, and resize always re-render.**
This is why rotation, theme toggles, and explicit state changes work
correctly even when `should_rebuild()` returns `false`.

---

## The three-level ladder

Vexo offers three mechanisms for skipping unnecessary `render()` calls,
in increasing order of explicitness and decreasing order of generality.

### Level 1 — Default (do nothing)

```rust
impl Component for MyWidget {
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> { ... }
    // should_rebuild() defaults to true
}
```

**Always correct.** Use for every component that isn't in a hot path.
The framework still does its own internal optimizations (layout caching,
render-object diffing) — you don't pay the full cost of `render()` even
at level 1.

**When to use:** everywhere by default. Don't override `should_rebuild()`
unless profiling shows a problem.

### Level 2 — `Memo<T>` (framework caching)

When a frequently-rebuilding parent has a child subtree that doesn't
change, wrap the child build in `Memo::new(deps, || build_subtree())`.
The framework caches the built subtree and only re-invokes `build` when
`deps` changes. On parent cascades where `deps` is unchanged,
`Memo::should_rebuild()` returns `false` and the cascade stops — no
`render()`, no child reconciliation.

```rust
// Parent re-renders every keyboard frame, but the settings list only
// depends on `items`. Memo skips the list rebuild when items are unchanged.
fn render(&self, state, ctx) -> Box<dyn Widget> {
    let header = build_animated_header(state.scroll_offset.get());
    let items = state.items.clone();
    MultiChild::new(
        children![
            header,
            Memo::new(items, || build_settings_list(&items)),
        ],
        Layout::column(),
    ).boxed()
}
```

**Key points:**
- The optimization lives in the *framework*, not in the child. The child
  Component doesn't know or care that it's being memoized.
- `deps: T` must implement `PartialEq + Clone`. The comparison is the sole
  arbiter of whether to rebuild. Capture **everything** `build` reads that
  could change — if `build` reads a `Theme` or `MediaQuery` value, include
  it in `deps` or the cache will be stale across that dependency's
  invalidation.
- The `build` closure is invoked at most once per unique `deps` value.
- `Memo` caches the **widget configuration tree**, not the element or
  render-object trees. Descendants still respond to `Signal::set` and
  `InheritedWidget` invalidation via the state-driven rebuild path —
  those bypass `should_rebuild()` and re-render the relevant descendant
  regardless of `Memo`'s cache.
- Internally, `Memo` uses `Shared` (a `pub(crate)` proxy widget with
  `Rc` pointer comparison) to skip the child cascade. App authors never
  touch `Shared` directly.

**When to use:** when a parent re-renders frequently and has a child
subtree that depends on stable data. This is vexo's analog of React's
`useMemo` and Flutter's `const` widgets.

### Level 3 — Explicit `should_rebuild()` on the child

When `Memo` isn't feasible (e.g., the child is built deep in a closure you
don't own) and the child sits in a hot path, override `should_rebuild()`
directly:

```rust
impl Component for ChatScreen {
    fn should_rebuild(&self, old: &Self) -> bool {
        self.conv_id != old.conv_id
            || self.messages != old.messages
        // Ignore: on_send closure (fresh Rc each render, same behavior)
    }

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> { ... }
}
```

**Rules for writing a correct `should_rebuild()`:**
1. Compare **only fields that `render()` reads**. If you compare a field
   that `render()` ignores, you'll trigger spurious rebuilds. If you
   *don't* compare a field that `render()` reads, you'll get a stale UI
   bug.
2. **Closures and controllers** are the usual reason parent cascades
   produce "different but equivalent" widgets. They typically allocate a
   fresh `Rc` per render, so `Rc::ptr_eq` is useless — but their
   *behavior* is stable. Skip them in the comparison.
3. **State-driven rebuilds bypass this hook entirely.** If your
   component's `render()` depends on state that's only observable via
   `Signal::set` or an `InheritedWidget` dependency, `should_rebuild()`
   cannot help — and shouldn't. Make sure that data flows through one of
   those channels.
4. **Never compare by pointer.** Widget instances are cloned by the
   reconciler; pointer equality is meaningless. Use value equality
   (`!=`) on data fields, or call controller methods that reflect
   observable state (e.g., `controller.path()`).

**When to use:** when `Memo` isn't feasible and profiling shows
`render()` is on a hot path. As of this writing, three components use
this hook: `ChatScreen`, `TabBarView`, `NavigationStackView`. All three
sit in the keyboard-animation path with heavy `render()` output and
stable props (closures/controllers differ, data doesn't).

---

## When *not* to optimize

- Don't override `should_rebuild()` for components that aren't in a
  measured hot path. The default is correct and the optimization is
  pointless if `render()` is already cheap.
- Don't wrap every child in `Memo`. `Memo` adds a `Component` element +
  state slot to the tree. Use it when the subtree is genuinely expensive
  and genuinely stable.
- Don't write `should_rebuild()` that returns `false` unconditionally
  unless your `render()` truly has no inputs (rare). Most components need
  to re-render when their widget props change.

---

## Why not a derive macro?

A `#[derive(Component)]` macro that auto-generates `should_rebuild()`
sounds attractive but doesn't work in practice:

- **Closures** are typically `Rc<dyn Fn>` allocated fresh per render.
  `Rc::ptr_eq` returns `false` → optimization never fires. To make it
  fire, app authors would have to memoize closures in state (React's
  `useCallback` pattern) — a notorious source of bugs.
- **Controllers and observables** don't implement `PartialEq` in any
  meaningful way. The relevant question is "has observable state
  changed?", which only the component author can answer.
- **Irrelevant fields** (debug IDs, fresh closures) would force spurious
  rebuilds unless annotated with `#[skip]` — collapsing back to manual
  annotation, just in attribute form.

The explicit `should_rebuild()` hook is honest about what it is: a
manual escape hatch for hot paths where the component author knows
which fields matter. It doesn't pretend to be automatic.

---

## Why not fine-grained reactivity (yet)?

The theoretically correct solution to "skip unnecessary rebuilds" is
fine-grained reactivity: track which `Signal`s each `render()` reads
and only re-render when those signals change. This is the SolidJS/Leptos
model. Vexo already has `Signal<T>`; instrumenting `RenderContext` to
track signal reads would let the framework skip rebuilds automatically
with zero annotation.

This is **not** the current design. It would require rethinking the
rebuild model (render runs reactively on signal write, not on parent
cascade). It's a future direction, not an evolution of
`should_rebuild()`. If vexo outgrows the three-level ladder, that's the
direction to invest in — not derive macros.

---

## Reference: where the mechanisms live

| Mechanism | File | Notes |
|---|---|---|
| `Component::should_rebuild()` (trait) | `vexo/src/stateful_widget.rs` | Default `true`. Override for hot paths (level 3). |
| `StatefulElement::update()` (gated by should_rebuild) | `vexo/src/stateful_widget.rs` | Parent-cascade path. |
| `StatefulElement::rebuild_from_state()` (NOT gated) | `vexo/src/stateful_widget.rs` | State-driven path. Always re-renders. |
| `Memo<T>` (public API, level 2) | `vexo/src/widgets/memo.rs` | Caches subtree by `deps: T: PartialEq + Clone`. |
| `Shared` (internal, used by `Memo`) | `vexo/src/widgets/shared.rs` | `pub(crate)` proxy with `Rc` pointer comparison. |
| `InheritedElement::update()` (Rc pointer check) | `vexo/src/inherited_widget.rs` | Skips child cascade when child `Rc` matches. |
| `MediaQuery` (uses `Rc<dyn Widget>` child) | `vexo/src/widgets/media_query.rs` | Built-in user of `Rc` sharing. |
| `RootMediaQuery` | `vexo/src/widgets/media_query.rs` | Framework-internal `Rc` sharing user. |
| `MediaQueryMutator` | `vexo/src/widgets/media_query.rs` | Built-in `Rc` sharing user. |
