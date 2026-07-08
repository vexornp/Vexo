# Per-Item Detail State Preservation Across Sidebar Toggles (Desktop)

**Date:** 2026-07-08
**Scope:** `shared_app/src/lib.rs` (demo app only — no framework changes)

## Problem

On desktop, toggling between sidebar items (Inbox, Starred, Sent, Drafts, Archive,
Trash) destroys the detail page's state:

- Text-edit content is reset (the `TextEditingController` is dropped and rebuilt).
- Scroll position is lost.
- Pushed "Page N" navigation is popped to root (`pop_to_root()` is called on every
  toggle).

## Root Cause

The desktop branch of `State::view()` (`shared_app/src/lib.rs:65-112`) builds a
**single** `DetailPage` per render and wraps it in one `NavigationStackView`
backed by one shared `NavigationController`. The sidebar callback does:

```rust
selected_for_cb.set(Some(id));
nav_for_cb.pop_to_root();   // ← destroys nav stack
```

When `selected` changes, `view()` rebuilds from scratch: `build_detail_content(id, …)`
constructs a fresh `DetailPage` widget. The framework reconciles it by type only
(`Component`-backed `can_update` is type-only at `vexo/src/stateful_widget.rs:694`),
so the same `DetailPage` element is reused — but `DetailPageState::on_update`
(`shared_app/src/lib.rs:288-307`) explicitly drops the `TextEditingController`
whenever the `id` changes:

```rust
if old_id != new_id {
    self.sync_controller(new_id);   // drops stale controller, builds fresh
}
```

State loss is by design under the current architecture: one element, repeatedly
re-synced to a new id.

## Flutter's Pattern

Flutter preserves sidebar+detail state via three primitives:

| Flutter             | Purpose                                  | Vexo equivalent                          | Status        |
| ------------------- | ---------------------------------------- | ---------------------------------------- | ------------- |
| `IndexedStack`      | Keep all children mounted, show one      | `IndexedStack` (`vexo/src/widgets/indexed_stack.rs`) | Exists        |
| `Offstage`          | Hide subtree but keep element/state alive | `Offstage` (`vexo/src/widgets/offstage.rs`) | Exists        |
| `AutomaticKeepAlive`| Lazy keep-alive in scrollable page lists | —                                        | Not needed    |

`IndexedStack` wraps each child in `Offstage` (`indexed_stack.rs:88`). Toggling
the index flips `offstage` flags; `OffstageElement::can_update` is type-only
(`offstage.rs:120`), so child elements are updated in place, never unmounted.
`NavigationStackView` already uses `IndexedStack` internally for push/pop state
preservation (`navigation.rs:610`). The sidebar↔detail relationship does not —
that's the gap.

## Solution

**Approach A (chosen):** Replace the desktop's single shared `NavigationController`
+ fresh `DetailPage` per toggle with **one `IndexedStack` of 6
`NavigationStackView`s**, each owning its own `NavigationController<Dest>` and its
own `DetailPage` for a fixed item id. Toggling the sidebar just flips the index —
no rebuild, no `pop_to_root`. State survives because all subtrees stay mounted.

### Alternatives Considered

- **Approach B — `Offstage`-keyed cache keyed by item id.** Maintain a
  `Map<id, Offstage<NavigationStackView>>`, render only the selected one onstage.
  Lazy-mount on first visit. Rejected: needs a new caching widget (doesn't exist),
  map-based reconciliation is more complex than positional, and `IndexedStack`
  already does this better.

- **Approach C — Hoist detail state into `State`.** Lift `TextEditingController`,
  scroll position, nav stack, etc. into the top-level `State`, keyed by item id.
  `DetailPage` becomes stateless. Rejected: violates the framework's
  component-local state model; every detail-page concern must be hoisted (exactly
  what `ComponentState` is designed to avoid); removes locality that makes
  `DetailPage` understandable in isolation.

## Architecture

```
SafeArea
└─ Flex::row
   ├─ Sidebar (callback: just `state.selected.set(Some(id))`)
   └─ IndexedStack(selected_index)            ← index derived from state.selected
      ├─ [0] NavigationStackView(inbox_ctrl,    DetailPage("inbox"))
      ├─ [1] NavigationStackView(starred_ctrl,  DetailPage("starred"))
      ├─ [2] NavigationStackView(sent_ctrl,     DetailPage("sent"))
      ├─ [3] NavigationStackView(drafts_ctrl,   DetailPage("drafts"))
      ├─ [4] NavigationStackView(archive_ctrl,  DetailPage("archive"))
      └─ [5] NavigationStackView(trash_ctrl,    DetailPage("trash"))
```

### Why State Survives

1. `IndexedStack` wraps each child in `Offstage` (`indexed_stack.rs:88`). Toggling
   the index flips `offstage` flags; `OffstageElement::can_update` is type-only
   (`offstage.rs:120`), so child elements are updated in place, never unmounted.
2. Each `NavigationStackView` element persists → its `NavigationStackViewState`
   persists. Inside each, the internal `IndexedStack` (used for push/pop,
   `navigation.rs:610`) keeps that item's pushed pages mounted.
3. Each `DetailPage` element persists → `DetailPageState` (and its
   `TextEditingController`) persists.

### Sidebar Callback

Simplifies from `selected.set(Some(id)); nav.pop_to_root();` to just
`selected.set(Some(id))`. The index flip is the only effect. Each item's nav stack
is untouched on toggle.

### `selected_index` Derivation

```rust
fn selected_index(selected: Option<&'static str>) -> usize {
    selected
        .and_then(|id| ITEMS.iter().position(|(i, _)| *i == id))
        .unwrap_or(0)
}
```

Falls back to `0` if `None` (unreachable on desktop since `new()` sets
`Some("inbox")`, but defensive).

## State Shape & Initialization

**Current:**

```rust
selection_log: Signal<u32>,
selected: Signal<Option<&'static str>>,
nav_controller: NavigationController<Dest>,   // single shared controller
```

**New:**

```rust
selection_log: Signal<u32>,
selected: Signal<Option<&'static str>>,
nav_controllers: Vec<NavigationController<Dest>>,       // desktop: one per item, indexed by ITEMS position
mobile_nav_controller: NavigationController<Dest>,      // mobile: single shared stack
```

`nav_controllers` has length `ITEMS.len()` (= 6), fixed. Index `i` corresponds to
`ITEMS[i]`. `mobile_nav_controller` is the single controller mobile uses for its
push/pop flow — it must persist in `State` (not be created per `view()` call)
because `NavigationStackView`'s `on_mount` wires its dirty callback and its path
must survive across rebuilds.

**Initialization:**

```rust
fn new() -> Self::State {
    let mut state = Self::State::default();
    state.selected.set(Some("inbox"));
    while state.nav_controllers.len() < ITEMS.len() {
        state.nav_controllers.push(NavigationController::new());
    }
    // mobile_nav_controller is initialized by Default (NavigationController::default),
    // no explicit setup needed.
    state
}
```

`NavigationController<Dest>: Default` (`navigation.rs:238`), so
`Vec<NavigationController<Dest>>: Default` gives an empty vec (backfilled in
`new()`), and `NavigationController<Dest>: Default` gives a fresh empty controller
for `mobile_nav_controller` automatically.

**Derive-macro compatibility:** `#[derive(ComponentState, Default)]` auto-wires
`Signal` fields. `Vec<NavigationController<Dest>>` is not a `Signal` — it's plain
owned data, mutated only through `NavigationController`'s `Rc<RefCell<…>>`
internals (clones are cheap, sharing the underlying cells). The derive should
treat it as a plain field. If the derive rejects non-`Signal` fields during
implementation, drop the derive and implement `ComponentState` manually for
`State` (trivial — `State` is the top-level `Application`, no
`on_mount`/`on_update`/`on_unmount` behavior needed).

## Desktop `view()` Rebuild

```rust
Platform::Desktop => {
    let current = selected_signal.get_cloned();
    let index = selected_index(current);

    // Sidebar: callback now just sets selection — no nav mutation.
    let selected_for_cb = selected_signal.clone();
    let sidebar = build_sidebar(
        current,
        Rc::new(move |id| {
            selected_for_cb.set(Some(id));
        }),
        false,
    );

    // IndexedStack: one child per sidebar item, each with its own nav stack.
    let mut stack = IndexedStack::new(index);
    for (i, (id, label)) in ITEMS.iter().enumerate() {
        let ctrl = state.nav_controllers[i].clone();
        let detail = build_detail_content(id, selection_count.clone(), ctrl.clone());
        let nav_for_dest = ctrl.clone();
        stack = stack.push(
            NavigationStackView::new(ctrl, detail)
                .root_title(label.to_string())
                .title(|d| match d {
                    Dest::Page(n) => format!("Page: {}", n),
                    _ => String::new(),
                })
                .destination(move |d| match d {
                    Dest::Page(n) => build_page_content(*n, nav_for_dest.clone()),
                    _ => Text::new("").boxed(),
                })
                .boxed(),
        );
    }

    SafeArea::new(
        Flex::row()
            .background(Color::WHITE)
            .push(sidebar)
            .push(stack.flex_grow(1.0)),
    )
    .boxed()
}
```

**Key differences from current:**

1. `nav_controller.pop_to_root()` removed from sidebar callback.
2. `IndexedStack` replaces the single `NavigationStackView` — 6 children, one per
   item, all mounted once and kept alive by `Offstage`.
3. `state.nav_controllers[i]` cloned per item — clones share the underlying
   `Rc<RefCell<…>>` path/pending cells, so push/pop on item `i`'s controller
   mutates the same shared path that `NavigationStackView` reads.
4. `build_detail_content(id, …)` is called once per item per `view()` invocation
   (same as before — it constructs a `DetailPage` widget; the *element* persists
   via `IndexedStack`'s `Offstage` reconciliation, only `DetailPage::id` is
   updated in place via type-only `can_update`).

### Reconciliation Correctness

On the first `view()` call, `IndexedStack` mounts all 6 children. On subsequent
calls (e.g. sidebar toggle), `IndexedStack::can_update` is type-only (inherited
from `Widget::can_update` default — same type, no keys), so all 6 child elements
survive, only `index` flips and `Offstage` flags update. `NavigationStackView`'s
`can_update` is also type-only (`Component`-backed, `stateful_widget.rs:694`),
so each nav view element persists. Same for `DetailPage`.

## Mobile Path & Shared Helpers

**Mobile branch** (`lib.rs:113-143`) — **unchanged in behavior**. It uses a single
`NavigationStackView(state.mobile_nav_controller.clone(), sidebar)` with
destinations built on demand. The sidebar's `push(Dest::Item(id))` flow stays
as-is. No `IndexedStack`, no per-item controllers.

**Why a separate `mobile_nav_controller` field:** mobile's single shared nav stack
is semantically distinct from desktop's per-item stacks. Reusing one of desktop's
`nav_controllers[i]` for mobile would conflate two unrelated navigation contexts
and break the moment mobile code mutates the shared path while a desktop item
expects it untouched. Keeping them as separate `State` fields makes the
platform split explicit and avoids index-picking arbitrariness.

**Implication:** `state.nav_controllers: Vec<NavigationController<Dest>>` is
desktop-only data; `state.mobile_nav_controller` is mobile-only data. Both live in
the shared `State`. On either platform, the other platform's controllers are
allocated but unused (each controller is cheap — three `Rc<RefCell<…>>` cells,
empty path). Acceptable: ~7 empty controllers ≈ negligible memory, and keeping
`State` shape uniform across platforms avoids branching the struct.

**`build_detail_content`** — stays shared, no signature change. Called from both
desktop (now in a loop) and mobile (in the destination closure). Still constructs
a fresh `DetailPage` widget; the difference is *where* that widget lands:
- Desktop: child of a persistent `NavigationStackView` inside `IndexedStack` →
  element preserved.
- Mobile: built fresh per push (correct — mobile nav semantics are push/pop, not
  sidebar toggle).

**`build_page_content`** — unchanged.

**`DetailPage` / `DetailPageState`** — unchanged. The `on_update` controller
re-sync logic (`lib.rs:288-307`) remains correct: on desktop, `DetailPage` for a
given item id is mounted once and never has its `id` changed (each item has its
own element in the `IndexedStack`), so `on_update`'s `old_id != new_id` branch
never fires on desktop. It still fires correctly on mobile when a `DetailPage`
element is reused across different `Dest::Item` pushes within the single nav
stack. The "drop controller on id change" behavior is preserved for that case.

**`selection_log` / `selection_count` Signal** — unchanged. Still shared across
all detail pages (desktop and mobile). The "Bump counter" button increments it;
all detail pages re-render to show the new value because they read
`self.selection_count.get()` in `render`. This is pre-existing shared-counter
behavior, not affected by the change.

## Edge Cases

1. **`selected` is `None` on desktop** — `selected_index` falls back to `0`
   (Inbox). Unreachable in practice, but defensive. `IndexedStack::new(0)` is
   valid.

2. **`IndexedStack` index out of bounds** — can't happen: `selected_index`
   returns a value in `0..ITEMS.len()`, and the stack always has exactly
   `ITEMS.len()` children. No runtime check needed.

3. **Nav controller clone identity** — `NavigationController::clone` shares the
   underlying `Rc<RefCell<Vec<Dest>>>` (`navigation.rs:228-236`). Pushing on the
   clone captured in `destination` mutates the same path the `NavigationStackView`
   reads. Verified in existing mobile code which already relies on this.

4. **First-frame mount cost** — all 6 `DetailPage`s mount on the first desktop
   `view()`. For "inbox", `DetailPageState::on_mount` constructs a
   `TextEditingController` (which builds a throwaway `FontSystem` — the documented
   workaround at `lib.rs:272`). This cost was previously paid once per inbox
   selection; now paid once at startup. Acceptable: one-time ~tens-of-ms cost,
   and only "inbox" does it (other items have `None` controller).

5. **`pop_to_root` removal** — no code path now calls it on desktop. The
   `NavigationController::pop_to_root` method itself stays (public API; mobile
   might use it; tests cover it). We just stop *calling* it from the sidebar
   callback.

6. **Sidebar highlight** — `build_sidebar(current, …)` still receives
   `current: Option<&str>` for highlighting. Unchanged.

## Testing

Manual verification (the app is a GUI demo; existing tests are framework-level,
not app-level). The user runs:

```bash
cargo run -p desktop_demo
```

Checklist:

1. Select Inbox, type in the text edit, scroll down.
2. Click Starred, push "Next page" twice (now on Page 2).
3. Click Inbox — text edit content preserved, scroll position preserved.
4. Click Starred — still on Page 2 (per-item nav stack preserved).
5. Click Drafts, push "Next page" once.
6. Cycle Inbox → Starred → Drafts → Inbox → Starred → Drafts — each item's state
   and nav depth intact.
7. "Bump counter" on any item still updates the shared counter display on all
   items.

**No new automated tests** — this is a wiring change in the demo app, exercising
existing framework primitives that already have test coverage
(`stateful_integration_test.rs:1689` covers `IndexedStack`/`Offstage` state
preservation; `navigation_stack_tests.rs` covers `NavigationStackView`). Adding
app-level tests would duplicate framework coverage without testing the platform
GUI.

## Summary of Changes

**Single file modified:** `shared_app/src/lib.rs`

1. **`State` struct** — replace `nav_controller: NavigationController<Dest>` with
   two fields: `nav_controllers: Vec<NavigationController<Dest>>` (desktop,
   per-item) and `mobile_nav_controller: NavigationController<Dest>` (mobile,
   single shared).

2. **`State::new()`** — backfill `nav_controllers` to `ITEMS.len()` with
   `NavigationController::new()`. `mobile_nav_controller` is initialized by
   `Default` automatically.

3. **Add helper** `fn selected_index(selected: Option<&'static str>) -> usize`.

4. **Desktop `view()` branch** — restructure to build an `IndexedStack` of 6
   `NavigationStackView`s (one per item, each with its own controller from
   `state.nav_controllers[i]`); remove `pop_to_root()` from the sidebar callback.

5. **Mobile `view()` branch** — replace `state.nav_controller` references with
   `state.mobile_nav_controller`. Behavior otherwise unchanged.

6. **`DetailPage`, `DetailPageState`, `build_detail_content`,
   `build_page_content`, `build_sidebar`, `build_item_row`** — unchanged.

**No framework changes.** No new crates, widgets, or elements. The fix uses
`IndexedStack` and `Offstage` which already exist and are already tested.

**Risk:** Low. The change is confined to the demo app's view construction.
Framework primitives are proven by existing tests. The main risk is a derive-macro
snag on the `Vec<NavigationController<Dest>>` field — if
`#[derive(ComponentState)]` rejects it, drop the derive and implement
`ComponentState` manually for `State` (trivial — `State` is the top-level
`Application`, no `on_mount`/`on_update`/`on_unmount` behavior needed).
