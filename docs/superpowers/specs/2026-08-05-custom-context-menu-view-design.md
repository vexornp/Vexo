# Custom Context Menu View — Design

**Date:** 2026-08-05
**Scope:** Let callers supply a fully-custom widget as the context menu's
content, replacing the hardcoded `menu_view`. The builder runs at render time
(always sees the current theme) and is captured per-trigger (each bubble can
render a different menu style). Builds on the existing
`ContextMenu` host/trigger/controller trio shipped in
`2026-08-04-context-menu-message-bubble-design.md`.

## Goal

Replace the private `menu_view(items, controller, theme)` function with a
caller-supplied `MenuBuilder`. The builder produces the entire menu widget —
layout, chrome, items, behavior. `MenuItem` and the items plumbing are removed
entirely; every menu is fully custom.

## Non-goals (explicitly out of scope)

- **No `MenuItem` convenience helper.** The user chose to drop items entirely.
  A `simple_menu_builder(items: Vec<MenuItem>) -> MenuBuilder` helper can be
  added later if missed, without touching the host/controller.
- **No structural change** to the host/trigger/controller split, the dismiss
  barrier, hit-test order, or positioning (still raw click coordinates).
- **No change to right-click plumbing** (`on_secondary_press`, arena gating on
  `Primary`) — that work shipped with the prior spec.
- **No edge-aware positioning** (flip/clamp near window edges). Stays a future
  host-level enhancement.
- **No hover highlight** on items. Still gated on `MouseRegion` going public.
- **No keyboard `Escape` dismiss.** Still future work.
- **No position passed to the builder.** Builder receives only
  `&ContextMenuController` and `&ThemeData`. If a future menu needs the click
  position (e.g. to open upward based on Y), this can be revisited.

## Context: what exists vs. what changes

| Capability | Status after this change | Location |
|---|---|---|
| `ContextMenuController` (position Signal + payload cell) | **modified** — payload changes from `Vec<MenuItem>` to `Option<MenuBuilder>` | `vexo_uikit/src/context_menu.rs:55` |
| `ContextMenu` host (Stack + barrier + Positioned menu) | **modified** — calls builder instead of `menu_view` | `vexo_uikit/src/context_menu.rs:111` |
| `context_menu_trigger` sugar | **modified** — takes `MenuBuilder` instead of `Vec<MenuItem>` | `vexo_uikit/src/context_menu.rs:240` |
| `MenuItem` struct + `MenuItem::new` | **removed** | `vexo_uikit/src/context_menu.rs:18-37` |
| Private `menu_view` fn | **removed** (recipe migrates into ChatScreen's builder) | `vexo_uikit/src/context_menu.rs:192-227` |
| `MenuBuilder` newtype | **added** | `vexo_uikit/src/context_menu.rs` (new) |
| `vexo_uikit/src/lib.rs:23` re-export | **modified** — drops `MenuItem`, adds `MenuBuilder` | `vexo_uikit/src/lib.rs` |
| ChatScreen placeholder menu | **modified** — `placeholder_items()` → `placeholder_menu_builder()` | `shared_app/src/chats/chat_screen.rs` |
| `data.rs` / `desktop.rs` / `chats/mod.rs` / `app.rs` | **unchanged** — only thread the controller, never `MenuItem` | `shared_app/src/...` |
| Right-click detection (`on_secondary_press`, arena gating) | unchanged | `vexo/src/widgets/...`, `vexo/src/event_handler.rs` |

**Key seam observation:** the controller flows through `data.rs`, `desktop.rs`,
`chats/mod.rs`, and `app.rs` *without* ever exposing `MenuItem`. Only
`context_menu.rs` (the trio + tests), `lib.rs` (re-export), and
`chat_screen.rs` (the one caller that builds items) are touched. This confirms
the controller is the right boundary — the breaking change is contained.

## Chosen approach: builder closure stored on controller

A `MenuBuilder` newtype wraps
`Rc<dyn Fn(&ContextMenuController, &ThemeData) -> Box<dyn Widget>>`. The
controller stores the latest builder (set at `show()` time) in an
`Rc<RefCell<Option<MenuBuilder>>>`. The host invokes the builder **inside
`render`** (after `signal_value` reports the menu open), so the builder always
reads the current theme — theme toggles re-render the menu correctly. The
builder receives `&controller` so its item rows can call `controller.close()`
on tap, exactly as the old `menu_view` did.

Per-trigger style is supported: each `context_menu_trigger` captures its own
`MenuBuilder`; different triggers can pass different builders.

### Rejected alternatives

- **Pre-built widget stored on controller** (`show(pos, widget: Box<dyn Widget>)`):
  conceptually simpler, but breaks theming. The trigger's
  `on_secondary_press(|pos| …)` callback only receives `pos`, not `ctx`/theme,
  so the trigger can't build a themed widget without capturing stale theme at
  `context_menu_trigger` call time. Worse fit for a themeable framework.
- **Keep `MenuItem` as an optional convenience** (`simple_menu_builder(items) -> MenuBuilder`):
  contradicts the user's "drop items entirely" choice. Can be layered on later
  without touching the host/controller if missed.

## Architecture

### `MenuBuilder` (new public type)

```rust
pub struct MenuBuilder(Rc<dyn Fn(&ContextMenuController, &ThemeData) -> Box<dyn Widget>>);

impl MenuBuilder {
    pub fn new(f: impl Fn(&ContextMenuController, &ThemeData) -> Box<dyn Widget> + 'static) -> Self {
        Self(Rc::new(f))
    }
}

impl Clone for MenuBuilder {
    fn clone(&self) -> Self { Self(Rc::clone(&self.0)) }
}

impl std::ops::Deref for MenuBuilder {
    type Target = dyn Fn(&ContextMenuController, &ThemeData) -> Box<dyn Widget>;
    fn deref(&self) -> &Self::Target { &*self.0 }
}
```

`Rc<dyn Fn>` (not `FnMut`): the builder is cloned into the controller's cell
and re-invoked on every rebuild; `Rc` keeps clones cheap and matches the
existing `MenuItem.on_select: Rc<dyn Fn()>` pattern (single-threaded, no
`Send + Sync` bounds that `Arc` would impose).

### `ContextMenuController` (modified)

```rust
#[derive(Clone)]
pub struct ContextMenuController {
    position: Signal<Option<Point<Logical>>>,
    builder: Rc<RefCell<Option<MenuBuilder>>>,   // was: items: Rc<RefCell<Vec<MenuItem>>>
}

impl ContextMenuController {
    pub fn new() -> Self;
    pub fn show(&self, position: Point<Logical>, builder: MenuBuilder);  // was: items: Vec<MenuItem>
    pub fn close(&self);
    pub fn position_signal(&self) -> &Signal<Option<Point<Logical>>>;
    pub fn builder_snapshot(&self) -> Option<MenuBuilder>;  // was: items_snapshot() -> Vec<MenuItem>
}
```

`builder_snapshot()` returns `Option<MenuBuilder>` — `Some` only while open
(the host calls it after seeing `position.is_some()`). Cloned out of the
`RefCell` so the borrow releases immediately, mirroring the old
`items_snapshot`. The `Signal` still carries only `Option<Point<Logical>>`
(the builder is `!Send + !Sync` via `Rc`, same constraint that kept items out
of the `Signal` in the prior design).

`MenuItem` is removed entirely. No more `items` cell, no `items_snapshot`.

### `ContextMenu` host (modified render)

`render` is unchanged except the open branch:

```rust
if let Some(pos) = position {
    let builder = self.controller.builder_snapshot();   // Option<MenuBuilder>
    if let Some(builder) = builder {
        // barrier (unchanged — full-size dismiss target, on_press -> close)
        stack = stack.push(barrier);

        // menu: call the builder instead of menu_view(&items, …)
        let menu = builder(&self.controller, &theme);
        let positioned_menu = vexo::Positioned::new(menu).left(pos.x).top(pos.y);
        stack = stack.push(positioned_menu);
    }
}
```

The `builder(&controller, &theme)` call happens inside `render`, so it runs at
rebuild time and always reads the current `ThemeData`. The builder receives
`&controller` so its item rows can call `controller.close()` on tap.

### `context_menu_trigger` (modified)

```rust
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    builder: MenuBuilder,                // was: items: Vec<MenuItem>
) -> Box<dyn Widget> {
    let ctrl = controller.clone();
    child.on_secondary_press(move |pos| {
        ctrl.show(pos, builder.clone());
    })
}
```

`builder.clone()` is `Rc::clone` — cheap. Each trigger captures one builder;
different triggers can pass different builders, so per-bubble style is
supported.

### `menu_view` removed

The private `fn menu_view(items, controller, theme)` is deleted. Its visual
recipe (DecoratedBox with `theme.surface` / `theme.outline` / 8px radius /
shadow wrapping a `column!` of padded `Text` rows) migrates into ChatScreen's
builder closure as the demo.

### Responsibility split (unchanged from prior spec)

The controller owns open/close + state; the host owns rendering + dismiss; the
trigger owns detection. Each unit has one job and can be tested independently.
The builder owns *what the menu looks like* — pushed out of the framework and
into the caller.

## ChatScreen wiring (`shared_app/src/chats/chat_screen.rs`)

The placeholder menu moves from `MenuItem`s into a builder closure. The visual
recipe is preserved verbatim — only the wrapping shape changes.

**Before:**
```rust
fn placeholder_items() -> Vec<MenuItem> {
    vec![
        MenuItem::new("Copy", Rc::new(|| log::debug!("context menu: Copy"))),
        MenuItem::new("Reply", Rc::new(|| log::debug!("context menu: Reply"))),
        MenuItem::new("Delete", Rc::new(|| log::debug!("context menu: Delete"))),
    ]
}
// …
context_menu_trigger(build_message_bubble(...), self.context_menu.clone(), placeholder_items())
```

**After:**
```rust
fn placeholder_menu_builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| {
        let labels = [("Copy", "context menu: Copy"),
                      ("Reply", "context menu: Reply"),
                      ("Delete", "context menu: Delete")];
        let column = vexo::column! {
            for (label, msg) in labels {
                let ctrl = ctrl.clone();
                vexo::GestureDetector::new(
                    vexo::WithLayout::new(
                        vexo::Text::new(label).with_color(theme.on_surface),
                        vexo::Layout::default().padding(8.0).width(160.0),
                    ),
                )
                .on_tap(move || { log::debug!("{}", msg); ctrl.close(); })
            }
        };
        vexo::DecoratedBox::with_style(
            vexo::WithLayout::new(column, vexo::Layout::default().min_width(160.0)),
            vexo::Style::default()
                .corner_radius(8.0)
                .background(theme.surface)
                .border(theme.outline, 1.0)
                .shadow(
                    vexo::BoxShadow::new(vexo::Color::BLACK.with_alpha(0.25))
                        .blur(6.0).offset(0.0, 2.0),
                ),
        ).boxed()
    })
}
// …
context_menu_trigger(
    build_message_bubble(...),
    self.context_menu.clone(),
    placeholder_menu_builder(),
)
```

`use vexo_uikit::{ContextMenuController, MenuBuilder, context_menu_trigger, …}`
replaces the `MenuItem` import. `ChatScreen`'s `context_menu:
ContextMenuController` field, its `Clone` impl, and the app-root
`ContextMenu::new(...)` wrap are **unchanged**.

## Data flow

```text
right-click bubble
  → ContextMenuTrigger.on_secondary_press(global_pos)
  → controller.show(pos, builder)        [builder cell ← builder; Signal::set(Some(pos))]
  → host's signal_value dependency marked dirty
  → host rebuilds: barrier + Positioned(builder(&controller, &theme), pos) mount
  → user clicks item   →  item.on_tap()  →  controller.close()
  OR user clicks outside → barrier.on_press → controller.close()
  → host rebuilds: barrier + menu unmount
```

The only difference from the existing spec: `builder` flows where `items`
used to. The builder is invoked **inside `render`** (not at right-click time),
so it sees the live theme.

## Dismiss behavior (unchanged from prior spec)

- **Outside click (any button)** on the barrier → `controller.close()`. The
  barrier uses `on_press`, which fires for `Primary` (immediate) and
  `Secondary` (fall-through, since the barrier has no `on_secondary_press`). So
  left- *or* right-clicking outside the menu closes it.
- **Item left-click** → arena-mediated `on_tap` (`Primary` creates the arena) →
  user closure + `controller.close()`.
- **Item right-click** → no arena (`Secondary` gated) → `on_tap` doesn't fire →
  no-op. (Same v1 limitation as before.)
- **Right-click another bubble while open** → barrier catches it first →
  closes; the right-click doesn't reach the other bubble. v1 limitation:
  close-then-right-click-again. (Same as prior spec.)
- **Escape key** → not handled in v1. (Same as prior spec.)
- **Scroll while open** → a press starts (barrier catches it) → closes.

## Position assumption (unchanged)

Global click coordinates == Stack-local coordinates, because the host `Stack`
fills the window (the `ContextMenu` host is lifted to the app root — see commit
`465938c`). The `InputEvent`'s `position` is already in window-logical
coordinates, so no transform is needed. The builder does **not** receive the
position; if a future menu needs it (e.g. open upward near the bottom edge),
this can be revisited.

## Migration impact

**Breaking changes (public API of `vexo_uikit::context_menu`):**
- `MenuItem` struct + `MenuItem::new` — **removed**.
- `ContextMenuController::show(position, items: Vec<MenuItem>)` →
  `show(position, builder: MenuBuilder)`.
- `ContextMenuController::items_snapshot() -> Vec<MenuItem>` →
  `builder_snapshot() -> Option<MenuBuilder>`.
- `context_menu_trigger(child, controller, items: Vec<MenuItem>)` →
  `context_menu_trigger(child, controller, builder: MenuBuilder)`.
- `MenuBuilder` — **added** (newtype, `Clone`, `Deref` to the `Fn`).

**Callers affected (full list):**
- `vexo_uikit/src/context_menu.rs` — the trio itself + tests (rewritten).
- `vexo_uikit/src/lib.rs:23` — `pub use` drops `MenuItem`, adds `MenuBuilder`.
- `shared_app/src/chats/chat_screen.rs:13` — import swap;
  `placeholder_items()` → `placeholder_menu_builder()`; the 4 test
  constructors unchanged.
- `shared_app/src/data.rs`, `shared_app/src/chats/desktop.rs`,
  `shared_app/src/chats/mod.rs`, `shared_app/src/app.rs` — **no change**
  (only thread `ContextMenuController`, never `MenuItem`).

Code churn is confined to `context_menu.rs` (rewrite) + `chat_screen.rs`
(placeholder swap + imports) + `lib.rs` (re-export).

## Testing

Tests split by crate, mirroring the existing test layout. The existing
`vexo_uikit/src/context_menu.rs` tests are rewritten from `Vec<MenuItem>` to
`MenuBuilder`; the assertions (position state, render-tree presence, item-tap
fires, barrier dismisses) stay structurally identical.

### `vexo_uikit` unit tests — controller state (`context_menu.rs #[cfg(test)]`)

- `test_controller_show_close`: `show(pos, builder)` →
  `position_signal().get()` is `Some(pos)` and `builder_snapshot()` is `Some`.
  `close()` → position `None`, `builder_snapshot()` is `None`.
- `test_controller_clone_shares_state`: `controller.show(...)` on one clone →
  the other clone observes the same position and builder (shared via
  `Signal`'s `Arc` + `Rc<RefCell>`).

### `vexo_uikit` integration tests — host rendering (using `ThreeTreePipeline`)

A shared test builder helper:
```rust
fn test_builder(label: &'static str) -> MenuBuilder {
    MenuBuilder::new(move |_ctrl, _theme| vexo::Text::new(label).boxed())
}
```

- `test_host_closed_has_only_content`: mount host closed → render tree contains
  `"content"`, NOT `"Copy"`. (Unchanged assertion; builder just isn't invoked.)
- `test_host_open_renders_menu_at_position`:
  `controller.show(pos, test_builder("Copy"))`; `perform_rebuilds`; assert
  `"Copy"` appears in the render tree. (Builder was invoked at render time.)
- `test_item_tap_fires_on_select_and_closes`: builder renders a
  `GestureDetector` row whose `on_tap` flips a `Cell<bool>` and calls
  `ctrl.close()`. Open, `handle_event` Primary press+release at the row, assert
  the cell flipped AND position is `None` after `perform_rebuilds`.
- `test_barrier_dismiss_on_outside_click`: open, click far away → position
  `None`. (Unchanged.)
- **New** `test_builder_reads_current_theme`: open with a builder that reads
  `theme.surface` into the rendered text (e.g. encode a color channel into the
  label); toggle theme; `perform_rebuilds`; assert the new theme's value
  appears. Locks in the "builder runs at render time" guarantee.

### `shared_app` ChatScreen tests (`chat_screen.rs #[cfg(test)]`)

- 4 existing constructors: unchanged (`context_menu: ContextMenuController::new()`).
- `test_right_click_bubble_opens_context_menu`: rewrite the items assertion —
  right-click a bubble, `perform_rebuilds`, assert `"Copy"` (now produced by
  `placeholder_menu_builder`) appears in the render tree. Assertion identical;
  the path that produces the text differs.
- `test_left_click_bubble_does_not_open_context_menu`: unchanged (left-click →
  no `"Copy"`).

### Verification gates (per CLAUDE.md)

- `cargo build -p vexo_uikit` after the type changes.
- `cargo build -p shared_app` after ChatScreen wiring.
- `cargo test -p vexo_uikit` — controller + host + tap + barrier + theme tests.
- `cargo test -p shared_app` — ChatScreen regression net.
- Existing ChatScreen tests are the regression guard for the wrapper change; if
  the render-tree walks break, the builder's output shape diverged from the old
  `menu_view` (investigate before "fixing" the test).

## Open questions / future work

- **`MenuItem` as a future convenience:** if a future caller wants the old
  items-driven menu, a `simple_menu_builder(items: Vec<MenuItem>) -> MenuBuilder`
  helper can be added without touching the host/controller. Out of scope here.
- **Edge-aware positioning:** still raw click coords. Builder could in principle
  receive the position and do its own clamping, but the builder does not
  receive the position in this design — so this stays a future host-level
  enhancement.
- **Hover highlight:** still gated on `MouseRegion` going public. Unaffected by
  this change.
- **Keyboard dismiss (`Escape`):** still future work (needs focus on the menu
  or a global key hook). Unaffected by this change.
- **Real actions** (`Copy` via `Clipboard`, `Delete` via message mutation,
  `Reply` UI): unaffected by this change — the builder just makes it easier to
  wire rich item UI when those land.
