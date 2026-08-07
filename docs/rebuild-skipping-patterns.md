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

### Level 2 — `Memo<T>` and `Shared` (framework caching)

When a frequently-rebuilding parent has a child subtree that doesn't change,
level 2 caches the subtree so the cascade stops early. Vexo offers two
primitives; they differ in *what* they compare to decide "skip or reconcile":

#### `Memo<T>` — compare declared `deps`

Wrap the child build in `Memo::new(deps, || build_subtree())`. The framework
caches the built subtree and only re-invokes `build` when `deps` changes.

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
- **`Memo<()>` is almost never correct.** Since `()` is always equal,
  `should_rebuild()` returns `false` on *every* parent cascade — not just
  the ones you wanted to skip. This blocks genuine data updates (e.g.
  switching conversations in a chat screen) even when the parent passes
  fresh content. If the subtree has *any* input that can change, capture it
  in `deps` or use `Shared` instead.
- `Memo` caches the **widget configuration tree**, not the element or
  render-object trees. Descendants still respond to `Signal::set` and
  `InheritedWidget` invalidation via the state-driven rebuild path —
  those bypass `should_rebuild()` and re-render the relevant descendant
  regardless of `Memo`'s cache.

**When to use:** when the subtree is built lazily and depends on known,
comparable data. This is vexo's analog of React's `useMemo`.

#### `Shared` — compare the `Rc` pointer of an already-built child

`Shared::new(rc)` wraps an `Rc<dyn Widget>`. `SharedElement` compares the
`Rc` pointer on `update()`/`rebuild()` and skips `update_child()` when the
pointer is unchanged.

The idiomatic pattern is a **wrapper `Component`** that stores
`child: Rc<dyn Widget>` as a field and reuses it across `render()` calls:

```rust
// KeyboardAvoider is the only MediaQuery dependent in its subtree, so it
// re-renders on every keyboard frame — but the child content only changes
// when the PARENT builds a fresh KeyboardAvoider. Storing the child as Rc
// and wrapping it in Shared gives: same Rc on keyboard frames (skip),
// different Rc on parent rebuild (reconcile).
#[derive(Clone)]
pub struct KeyboardAvoider {
    child: Rc<dyn Widget>,  // ← the cache lives in the widget struct
}

impl Component for KeyboardAvoider {
    fn render(&self, _state, ctx) -> Box<dyn Widget> {
        let bottom = MediaQuery::of(ctx).view_insets.bottom;
        let child = Rc::clone(&self.child);   // O(1), same pointer
        WithLayout::new(
            Shared::new(child),
            Layout::default().padding_each(0., 0., 0., bottom),
        ).boxed()
    }
}
```

The widget struct's lifetime *is* the cache: a fresh `Rc::new()` only runs
in the constructor (which the parent only calls when building genuinely new
content), while `render()` reuses the same `Rc` via `Rc::clone`. This is
why `Shared` is safer than `Memo` when the child is opaque (built by the
caller, not by this component) — there's no `deps` to enumerate and get
wrong.

**Key points:**
- **Footgun:** a fresh `Rc::new()` inside `render()` defeats the
  optimization (the pointer is always new → always reconciles). Always
  cache the `Rc` in the widget struct and use `Rc::clone` in `render()`.
- Like `Memo`, descendants of `Shared` still respond to `Signal::set` and
  `InheritedWidget` invalidation via the state-driven rebuild path.

**When to use:** wrapper components whose child is built by the caller
(not by this component) and whose `render()` is on a hot path. `Memo`'s
`deps` would have to enumerate every field that could affect the opaque
child — `Shared` sidesteps that by comparing the child itself.

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

## Signal field rule: root Signals only, derive in State

**This rule is load-bearing for any component using Level 3
(`should_rebuild() == false`). Violating it causes silent state-driven-rebuild
failures.**

### The rule

**Signal widget fields must be root Signals (identity-stable for the element's
lifetime). Never pass `Signal::derive(...)` created in parent `render()` as a
child widget field.**

### Why

When `should_rebuild` returns `false`, the framework still replaces widget
fields (`StatefulElement::update`, `stateful_widget.rs`) but skips `render()`.
`signal_value` — which registers the dirty_callback as a weak subscriber on
the Signal — is only called during `render()`. If a parent passes a fresh
`Signal::derive(...)` each cascade, the new derived Signal never gets a
subscriber, and state-driven rebuilds silently break.

Root Signals don't have this problem: they're `Arc`-cloned (same identity), so
the subscription registered on mount persists across render-skips.

### The derived-in-State pattern

When a child needs a derived view of a root Signal (e.g. filtering a
`HashMap<ConvId, Vec<Message>>` to one conversation's slice), the derived
Signal must live in **State**, not the Widget struct:

```rust
struct ChatScreen {
    conv_id: ConvId,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,  // root, not derived
}

struct ChatScreenState {
    derived_messages: Option<Signal<Vec<Message>>>,
}

impl ComponentState for ChatScreenState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        let widget = ctx.widget().downcast_ref::<ChatScreen>().unwrap();
        let conv_id = widget.conv_id.clone();
        let root = widget.messages.clone();
        self.derived_messages = Some(Signal::derive(root, move |map| {
            map.get(&conv_id).cloned().unwrap_or_default()
        }));
    }
}

impl Component for ChatScreen {
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let messages = ctx.signal_value(state.derived_messages.as_ref().unwrap());
        // ...
    }
}
```

The derived's Arc identity is stable (created once in `on_mount`, owned by
State which persists across widget replacements), so the weak subscription
survives `should_rebuild == false`.

### Why no `on_update` re-derivation

Root Signals are created once and live for the app's lifetime. No existing
code path swaps the root source for an already-mounted element. If a future
feature needs that, `on_update` re-derivation with `Signal::ptr_eq` checks
is the trigger — not pre-building it now.

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
| `Shared` (public API, level 2) | `vexo/src/widgets/shared.rs` | Proxy widget with `Rc` pointer comparison. Used by `Memo` internally and directly by wrapper components like `KeyboardAvoider`. |
| `InheritedElement::update()` (Rc pointer check) | `vexo/src/inherited_widget.rs` | Skips child cascade when child `Rc` matches. |
| `MediaQuery` (uses `Rc<dyn Widget>` child) | `vexo/src/widgets/media_query.rs` | Built-in user of `Rc` sharing. |
| `RootMediaQuery` | `vexo/src/widgets/media_query.rs` | Framework-internal `Rc` sharing user. |
| `MediaQueryMutator` | `vexo/src/widgets/media_query.rs` | Built-in `Rc` sharing user. |
