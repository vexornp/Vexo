# Context Menu on Message Bubbles — Design

**Date:** 2026-08-04
**Scope:** Show a system-style context menu when the user right-clicks a message
bubble in the chat screen. Desktop only. Placeholder menu items (no real
actions) — the focus is the reusable menu infrastructure.

## Goal

Right-clicking a message bubble in `shared_app/src/chats/chat_screen.rs` opens a
floating context menu at the cursor position. The menu is rendered by a reusable
`ContextMenu` widget triple in `vexo_uikit`, so other screens can adopt the same
pattern later. Menu items are placeholders (`Copy` / `Reply` / `Delete`) that log
on select — enough to exercise the full open → select → close lifecycle.

## Non-goals (explicitly out of scope for v1)

- Touch long-press to open the menu (desktop right-click only).
- Real menu actions (clipboard, message mutation, reply UI). Items are
  placeholders.
- Hover highlight on menu items (`MouseRegion` is `pub(crate)`; revisit when it
  goes public).
- Keyboard `Escape` to dismiss (menu has no focus; keyboard routing is separate
  work).
- Dividers / submenus / icons (`MenuItem::separator()` is a future addition).
- Edge-aware menu positioning (flip/clamp near window edges). v1 places the menu
  at the raw click coordinates.
- Right-clicking a *different* bubble while the menu is already open
  re-opens it at the new bubble. v1: the dismiss barrier catches the right-click
  and closes; the user right-clicks again. (See §Dismiss behavior.)

## Context: what exists vs. what's missing

| Capability | Status | Location |
|---|---|---|
| `PointerButton::Secondary` enum variant | exists | `vexo/src/input/event.rs:84` |
| winit → `PointerButton::Secondary` mapping | **missing** (hardcoded to `Primary`) | `vexo/src/input/event.rs:281-289` |
| `GestureDetector::on_secondary_press` / `Widget::on_secondary_press` | **missing** | `vexo/src/widgets/gesture_detector.rs`, `vexo/src/widgets/mod.rs:197-218` |
| `Stack`, `Positioned` (overlay primitives) | exist, public | `vexo/src/widgets/stack.rs`, `vexo/src/widgets/positioned.rs` |
| `Stack::push(Option<Box<dyn Widget>>)` (conditional children) | exists | `vexo/src/widgets/stack.rs:84-88` |
| `Overlay` / `Popup` / `ContextMenu` widget | **does not exist** | — |
| Clipping | not clipped by default (full-viewport scissor); **`ScrollView` sets `overflow_x(Hidden)`** | `vexo/src/widgets/scroll_view.rs:72` |
| `EventContext::bounds()` (absolute window coords) | exists | `vexo/src/event_context.rs:146` |
| `InputEvent::PointerButton.position` (already global) | exists | `vexo/src/input/event.rs:32-39` |
| `Signal<T>`-driven rebuild pattern | exists | `shared_app/src/chats/chat_screen.rs:113` |
| `ScrollController` (the analog pattern for the new controller) | exists | `shared_app/src/chats/chat_screen.rs:23` |
| `pipeline.handle_event(...)` (test event dispatch) | exists, public | `vexo/src/pipeline.rs:555` |

**Key constraint from the clipping row:** the message list lives inside a
`ScrollView` (`overflow_x(Hidden)`). A menu rendered *inside* the scroll viewport
would be clipped at the scroll edges. Therefore the menu host must live
**outside** the ScrollView — at the ChatScreen root.

## Chosen approach: controller-based reusable host

Mirrors the existing `ScrollController` pattern. A `ContextMenuController` holds
a `Signal<Option<OpenMenu>>`. A `ContextMenu` host widget wraps the screen
content in a `Stack` and reads the controller's signal. A `context_menu_trigger`
helper wraps each bubble with a right-click detector that calls
`controller.show(position, items)`.

Rejected alternatives:
- **InheritedWidget overlay (Flutter `Overlay` pattern):** zero threading for
  callers, but it's a first-of-its-kind pattern in this codebase, needs
  InheritedWidget plumbing + context lookup, and is over-engineered for one
  consumer. Can be layered on top of the controller design later if threading
  ever becomes painful.
- **Inline in ChatScreen only:** smallest diff, but contradicts the "reusable
  widget" requirement.

## Architecture

Four pieces, in two crates:

### `vexo` core — right-click plumbing

**1. winit button mapping** (`vexo/src/input/event.rs`, `from_winit`,
`WindowEvent::PointerButton` arm at line 272-292)

Replace the hardcoded `PointerButton::Primary` with a match on winit's button
source:

- `Mouse(Left)` → `Primary`
- `Mouse(Right)` → `Secondary`
- `Mouse(Middle)` → `Tertiary`
- `Touch` / `Pen` / `Eraser` → `Primary` (unchanged — touch tap stays primary)

The exact winit 0.31 `ButtonSource` / `MouseButton` API is to be confirmed at
implementation time; the behavior is as above.

**2. `GestureDetector::on_secondary_press`**
(`vexo/src/widgets/gesture_detector.rs`)

- New field: `on_secondary_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>)>>>`
  — fires on `Secondary` + `Pressed`, passing the event's global position. The
  position is required because the menu must appear at the cursor; existing
  `on_press` / `on_tap` stay position-less (no breaking change to them).
- `on_event` (line 314-342) revised on `PointerButton` + `Pressed`:
  - `button == Secondary` and `on_secondary_press` is set → fire it with
    `position`, return `Some` (claim the event, skip `on_press`).
  - `button == Secondary` and `on_secondary_press` is **not** set → fall through
    to `on_press` (preserves current behavior for widgets that don't opt into
    secondary; this is what lets the dismiss barrier close on right-click).
  - `button == Primary` → existing `on_press` behavior.
- `register_gestures` (line 344-351): unchanged. The trigger sets no `on_tap`, so
  it registers no tap recognizer.

**3. Arena gating on Primary** (`vexo/src/event_handler.rs`,
`handle_pointer_event`)

Only create/feed the gesture arena for `Primary`-button presses. `Secondary`
presses skip the arena entirely (no `ArenaEvent::Down`/`Up` fed). Consequences:

- Right-click never triggers `on_tap` (arena-mediated) or drag/scroll
  recognizers. This is correct (right-click is not a tap or a drag) and fixes a
  latent bug: today every button is hardcoded to `Primary`, so right-click
  currently fires `on_tap` on tappable widgets (e.g. the Send button would send
  on right-click). After the winit fix, without this gating, that latent bug
  would persist. This is a targeted fix per the CLAUDE.md guidance ("where
  existing code has problems that affect the work, include targeted
  improvements").
- `on_press` (immediate, non-arena) still fires for `Secondary` in the
  fall-through case, so the dismiss barrier still closes on right-click.
- Touch / Pen remain `Primary` → arena works as today. No regression to
  scrolling or tapping.

**4. `Widget` trait fluent API** (`vexo/src/widgets/mod.rs:197-218`)

Add `on_secondary_press(self, impl FnMut(Point<Logical>) + 'static) -> Box<dyn
Widget>`, mirroring `on_press` / `on_tap` — wraps in
`GestureDetector::new(self).on_secondary_press(callback)`.

### Behavior matrix after these changes

| Widget config | Left-click | Right-click |
|---|---|---|
| `on_tap` only (Send button) | fires `on_tap` ✓ | nothing ✓ (latent bug fixed) |
| `on_press` only (barrier) | fires `on_press` ✓ | fires `on_press` ✓ |
| `on_secondary_press` only (trigger) | nothing ✓ | fires `on_secondary_press(pos)` ✓ |
| `on_press` + `on_secondary_press` | `on_press` | `on_secondary_press` (`on_press` skipped) |

### `vexo_uikit` — `ContextMenu` trio (new file `vexo_uikit/src/context_menu.rs`)

**`MenuItem`** (public, `Clone`):
```rust
pub struct MenuItem {
    pub label: String,
    pub on_select: Rc<dyn Fn()>,
}
```
`Rc<dyn Fn()>` is `Clone`. Items are built at trigger-construction time (e.g.
per message in the render loop), closing over whatever context the action needs.

**`OpenMenu`** (private to the module, `Clone`):
```rust
struct OpenMenu {
    position: Point<Logical>,
    items: Vec<MenuItem>,
}
```
Stored inside the controller; never escapes the module. `Clone` is required
because `Signal<T>` requires `T: Clone`.

**`ContextMenuController`** (public, `Clone`):
```rust
pub struct ContextMenuController { state: Signal<Option<OpenMenu>> }
impl ContextMenuController {
    pub fn new() -> Self;
    pub fn show(&self, position: Point<Logical>, items: Vec<MenuItem>); // state.set(Some(...))
    pub fn close(&self);                                              // state.set(None)
    fn state_signal(&self) -> &Signal<Option<OpenMenu>>;              // host reads this
}
```
Mirrors `ScrollController` — created by the screen's caller, held as a field,
`.clone()`d into triggers and the host. The `Signal` shares underlying state
across clones, so widget-struct recreation on rebuild doesn't lose menu state.

**`ContextMenu` host** (public) — a `Component`:
```rust
pub struct ContextMenu { controller: ContextMenuController, child: Box<dyn Widget> }
```
`type State = <trivial empty state struct, default ComponentState impl>` (the
host has no state of its own; it only reads the controller's signal).

`render(&self, _state, ctx)`:
1. `let open = ctx.signal_value(self.controller.state_signal());` — registers
   the rebuild dependency (same pattern as `ChatScreen::render` reading
   `self.messages`).
2. Build the `Stack`:
   - child 0: `self.child` (the screen content) — the only **non-positioned**
     child; it fills the Stack (it already carries `flex_grow(1.0)` from
     `chat_screen.rs`).
   - child 1 (conditional, `Some` when open): the **dismiss barrier** —
     `Positioned::new(GestureDetector::new(fill).on_press(move || controller.close()))`
     with all four insets set to `0` (`.left(0).top(0).right(0).bottom(0)`). It
     must be a `Positioned` child so it is taken out of flow and *overlaps* the
     content — a non-positioned child would flow below the content in the
     Stack's column flexbox, not overlap it. The all-insets-zero sizing makes the
     barrier fill the Stack so its hit-test covers the whole overlay. (`fill` is
     a minimal widget that accepts tight constraints — e.g. `Text::new("")` —
     whose only purpose is to give the pass-through `GestureDetector` full-size
     `computed_bounds` for hit-testing.)
   - child 2 (conditional, `Some` when open):
     `Positioned::new(menu_view(&open, controller, theme)).left(open.position.x).top(open.position.y)`
     — only `left`+`top` set, so the menu takes its intrinsic size at the cursor.
3. Return the `Stack`.

**Hit-test order:** vexo hit-tests Stack children in reverse (last child first =
topmost first — `vexo/src/hit_test.rs:377-395`, `for child in
obj.children().iter().rev()`), so the menu (child 2) is hit first, then the
barrier (child 1), then the content (child 0). A click on the menu hits the
menu; a click anywhere else hits the barrier (which closes); when closed, only
the content is present.

Conditional children via `Stack::push(Option<Box<dyn Widget>>)` — when closed,
no barrier/menu mount; when open, both mount. This avoids `Offstage` + 
`Positioned` interaction concerns and the mount/unmount is cheap (stateless
menu).

**`context_menu_trigger`** (public free function — sugar):
```rust
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    items: Vec<MenuItem>,
) -> Box<dyn Widget>
```
Returns `child.on_secondary_press(move |pos| controller.show(pos, items))`.
Callers may also use `on_secondary_press` directly.

**Responsibility split:** the controller owns open/close + state; the host owns
rendering + dismiss; the trigger owns detection. Each unit has one job and can
be tested independently.

### Menu view

`menu_view(open: &OpenMenu, controller, theme) -> Box<dyn Widget>`: a single
`DecoratedBox` (background `theme.surface`, border `theme.outline` 1.0,
`corner_radius` 8.0, drop shadow) wrapping a `column!` of item rows. `min_width`
~160px. Each item row:

```text
GestureDetector::new(
    WithLayout::new(
        Text::new(&item.label).with_color(theme.on_surface),
        Layout::default().padding(8.0).width(160.0),
    ),
)
.on_tap(move || { (item.on_select)(); controller.close(); })
```

Uses `theme.surface` / `theme.on_surface` / `theme.outline` (all already used in
`chat_screen.rs`). No hover, no dividers in v1 (see Non-goals).

## ChatScreen wiring (`shared_app/src/chats/chat_screen.rs`)

- New field `pub(crate) context_menu: ContextMenuController` on `ChatScreen` and
  its `Clone` impl. Created by the caller (the chat-list screen) alongside
  `ScrollController::new()`; tests pass `ContextMenuController::new()`.
- `ChatScreenState` unchanged (the controller lives on the widget).
- In `render`:
  - Build `content` exactly as today (the column with `ScrollView` + input bar).
  - Outermost widget becomes
    `ContextMenu::new(DecoratedBox::with_style(KeyboardAvoider::new(content), bg), self.context_menu.clone())`.
    The host `Stack` fills the window; the menu paints above the input bar,
    keyboard avoider, and background.
  - Per message: replace `build_message_bubble(...)` with
    `context_menu_trigger(build_message_bubble(...), self.context_menu.clone(), placeholder_items())`.
  - `placeholder_items()` →
    `vec![ MenuItem::new("Copy", Rc::new(|| log::debug!("copy"))), MenuItem::new("Reply", …), MenuItem::new("Delete", …) ]`
    — static no-op/logging items, enough to exercise open/select/close.

## Data flow

```text
right-click bubble
  → ContextMenuTrigger.on_secondary_press(global_pos)
  → controller.show(pos, items)            [Signal::set]
  → host's signal_value dependency marked dirty
  → host rebuilds: barrier + Positioned(menu, pos) mount
  → user clicks item   →  item.on_select()  →  controller.close()
  OR user clicks outside → barrier.on_press → controller.close()
  → host rebuilds: barrier + menu unmount
```

## Dismiss behavior

- **Outside click (any button)** on the barrier → `controller.close()`. The
  barrier uses `on_press`, which fires for Primary (immediate) and Secondary
  (fall-through, since the barrier has no `on_secondary_press`). So left- *or*
  right-clicking outside the menu closes it.
- **Item left-click** → arena-mediated `on_tap` (Primary creates the arena) →
  `on_select()` + `controller.close()`.
- **Item right-click** → no arena (Secondary gated) → `on_tap` doesn't fire →
  no-op. (Acceptable v1 behavior.)
- **Right-click another bubble while open** → the barrier catches it first →
  closes; the right-click doesn't reach the other bubble. v1 limitation:
  close-then-right-click-again. (See Non-goals.)
- **Escape key** → not handled in v1. (See Non-goals.)
- **Scroll while open** → a press starts (barrier catches it) → closes.

## Position assumption

Global click coordinates == Stack-local coordinates, because the host `Stack`
fills the window (ChatScreen is the full-screen route). The `InputEvent`'s
`position` is already in window-logical coordinates, so no transform is needed.

If ChatScreen is ever inset by navigation chrome, the position math must
subtract the Stack's origin. This is an explicit assumption; if it's violated
the menu will be offset by the inset.

## Testing

### `vexo` unit tests — right-click detection
(`vexo/src/widgets/gesture_detector.rs` `#[cfg(test)]`, mirroring the existing
`test_gesture_detector_element_event_press` pattern: construct element +
`EventContext`, fire `InputEvent`, assert)

- `on_secondary_press` fires with the correct `Point<Logical>` on `Secondary` +
  `Pressed` and returns `Some` (claims).
- `Primary` + `Pressed` with `on_secondary_press` set → callback does NOT fire.
- `Secondary` + `Pressed` with both `on_secondary_press` and `on_press` set →
  only `on_secondary_press` fires (`on_press` skipped).
- `Secondary` + `Pressed` with only `on_press` set → `on_press` fires
  (backward-compat fall-through).

`InputEvent::from_winit` mapping: add a test mapping `Right → Secondary`,
`Middle → Tertiary`, `Left → Primary` **if** winit 0.31's
`WindowEvent::PointerButton` is constructable in a `#[cfg(test)]` context;
otherwise verify manually and note it. (The GestureDetector behavior tests
above use synthetic `InputEvent`s directly and don't depend on `from_winit`.)

### `vexo` integration tests — arena gating
(`vexo/src/integration_tests.rs` or a new `context_menu_tests.rs`, using
`pipeline.handle_event`)

- Tree with a tappable widget (`on_tap` counter). Send `Primary` press+release
  at its position → counter increments. Send `Secondary` press+release → counter
  does NOT increment (arena gated on Primary; the latent right-click-triggers-
  on_tap bug is fixed).
- `ScrollView` scroll: `Primary` drag scrolls; `Secondary` press does not start a
  scroll drag. (If easy to assert; else skip.)

### `vexo_uikit` tests — the trio
(new `vexo_uikit/src/context_menu.rs` `#[cfg(test)]` module, using
`ThreeTreePipeline`)

- `ContextMenuController::show` / `close`: assert signal state transitions
  `None → Some → None`.
- `ContextMenu` host, closed: mount with a trivial child; assert the `Stack` has
  exactly 1 child (content only) — no barrier/menu in the render tree.
- `ContextMenu` host, open: `controller.show(pos, items)`; `perform_rebuilds`;
  assert the render tree now contains a `Positioned` render object at
  `(pos.x, pos.y)` and the item label text appears in a `TextRenderObject`.
  (Render-tree walk helpers mirror `chat_screen.rs`'s `find_text_in_tree`.)
- Item tap: with menu open, `handle_event` a `Primary` press+release at the
  item's position → assert `on_select` fired and the menu closed (signal `None`
  after `perform_rebuilds`).
- Barrier dismiss: with menu open, `handle_event` a `Primary` press *outside*
  the menu → menu closed.

### `shared_app` ChatScreen tests
(`shared_app/src/chats/chat_screen.rs`)

- Update the 4 existing test constructors with `context_menu:
  ContextMenuController::new()`; confirm they still pass (layout is identical
  when closed — the `Stack` has only child 0).
- New: right-click a bubble → menu appears. Mount ChatScreen, `handle_event` a
  `Secondary` press at a bubble's position, `perform_rebuilds`, assert an item
  label (`"Copy"`) appears in the render tree.
- New: left-click a bubble → no menu (regression guard for button gating).

### Verification gates (per CLAUDE.md)

Run `cargo build` after each crate's edits and `cargo test` after each feature
slice. Existing ChatScreen tests must stay green (they're the regression net for
the wrapper change). The existing tests' render-tree walks are depth-agnostic
(loop until a node with ≥2 children), so the extra proxy layers from the
`ContextMenu` wrapper should not break them — but confirm by running the tests.

## Open questions / future work

- **`from_winit` testability:** confirm whether winit 0.31's
  `WindowEvent::PointerButton` can be constructed in `#[cfg(test)]`. If not, the
  button-mapping is verified manually.
- **Hover highlight:** revisit once `MouseRegion` is public (or expose a
  `cursor`-style public wrapper).
- **Edge-aware positioning:** flip/clamp the menu near window edges.
- **Right-click-during-open:** let a right-click on another bubble close-and-
  reopen in one gesture (requires the barrier to forward secondary presses to
  the bubble beneath, or a different dismiss model).
- **Keyboard dismiss:** `Escape` closes (needs focus on the menu or a global key
  hook).
- **Real actions:** `Copy` (clipboard — Vexo has a `Clipboard` trait already),
  `Delete` (message mutation path into the `messages` Signal), `Reply` (input-bar
  UI + reply-target Signal).
