# Mobile Long-Press Context Menu — Design

**Date:** 2026-08-11
**Scope:** Show the message-bubble context menu on mobile by long-pressing
the bubble (500ms hold). Desktop keeps right-click. Adds a reusable,
framework-level `on_long_press` gesture to `GestureDetector`, modeled on the
existing `on_secondary_press` API, backed by a new `LongPressRecognizer` in
the gesture arena. Also fixes a pre-existing mobile wiring bug where the
mobile `ChatScreen` used a fresh `ContextMenuController` instead of the
shared root host's controller — without this fix the trigger fires but the
menu never renders.

Reuses the existing `ContextMenu` host, `ContextMenuController`, and
`message_menu::builder` unchanged. The menu content (reactions pill +
actions card) is identical to desktop's right-click menu.

## Goal

1. **Long-press opens the menu on mobile** — press and hold a message bubble
   for 500ms; the context menu appears at the press point.
2. **Reusable framework gesture** — `on_long_press` lives on `GestureDetector`
   (and as a `Widget` trait default method), available to any widget, not
   hardcoded to the message bubble.
3. **No desktop regression** — right-click continues to open the menu on
   desktop; no desktop behavior or test changes.
4. **Composes with scroll** — long-press and vertical drag (scroll)
   disambiguate correctly via the gesture arena: if the finger drifts past
   slop before 500ms, scroll wins and long-press cancels; if 500ms elapses
   with the finger still, long-press wins and scroll never starts.
5. **Fix mobile `ContextMenuController` wiring** — `MobileChatsPage` passes
   the shared `state.context_menu` into `ChatScreen`, mirroring desktop, so
   the root `ContextMenu` host renders the menu.

## Non-goals (explicitly out of scope)

- **No long-press visual feedback during the hold.** No bubble highlight,
  no haptic, no progress ring. The menu just appears at 500ms. (Per user
  choice; can be added later as a separate recognizer-side or
  widget-side concern.)
- **No long-press on desktop.** Desktop stays right-click-only. The
  `on_long_press` API is available on desktop (ungated), but
  `context_menu_trigger` doesn't call it there.
- **No haptic feedback.** The framework has no haptic API today; adding one
  is a separate concern.
- **No platform injection on `context_menu_trigger`.** The trigger branches
  on `Platform::current()` (compile-time). No `platform: Option<Platform>`
  parameter for test override — YAGNI; existing right-click tests cover the
  desktop branch, and the mobile branch is a one-liner verified by
  compilation. If testability is later needed, add the parameter then.
- **No change to menu content or positioning.** `message_menu::builder`,
  `MenuContent`, `MenuMetrics`, and the `ContextMenu` host's positioning
  logic are unchanged. The menu opens at the long-press position exactly as
  it opens at the right-click position on desktop.
- **No `on_double_tap`, no drag callbacks.** This spec adds long-press only.
  Other future gestures are out of scope.
- **No close animation.** Unchanged from the current instant-dismiss design.

## Architecture

Three independent layers, bottom-up. Each can be reviewed and merged
independently.

```
┌─────────────────────────────────────────────────────────────────┐
│  Layer 3: Wiring (shared_app + vexo_uikit)                      │
│  • context_menu_trigger branches on Platform::current()         │
│  • MobileChatsPage passes state.context_menu into ChatScreen    │
│    (fixes pre-existing bug where mobile used a fresh            │
│    ContextMenuController not mounted at the root host)          │
└─────────────────────────────────────────────────────────────────┘
                              ▲
┌─────────────────────────────────────────────────────────────────┐
│  Layer 2: Framework gesture API (vexo)                          │
│  • GestureDetector::on_long_press callback field + builder      │
│  • Widget::on_long_press default trait method                   │
│  • GestureDetectorElement registers LongPressRecognizer +      │
│    fires on_long_press on Tick-driven arena win                 │
└─────────────────────────────────────────────────────────────────┘
                              ▲
┌─────────────────────────────────────────────────────────────────┐
│  Layer 1: Arena time dimension (vexo/gestures)                  │
│  • ArenaEvent::Tick { now } new variant                         │
│  • LongPressRecognizer (Down→start timer, Tick→accept @500ms,   │
│    Move>slop→reject, Up→reject, Cancel→reject)                  │
│  • arena.handle_event runs try_resolve on Tick                  │
│  • Pipeline feeds Tick to active arena each frame               │
└─────────────────────────────────────────────────────────────────┘
```

**Key invariant:** long-press participates in the gesture arena like any
other recognizer. When scroll's `VerticalDragRecognizer` accepts first
(finger moves >18px), the arena rejects the `LongPressRecognizer` and
clears its timer. When long-press accepts first (500ms elapsed, finger
still), the arena rejects scroll — no scroll begins. This composition is
the entire reason for the arena-integrated approach (vs. an element-owned
timer bypassing the arena, which would have scroll-composition risks).

**Why a new `ArenaEvent::Tick` rather than a wall-clock check inside
`handle_event`:** the arena is pure and event-driven; recognizers only
advance when fed. Without `Tick`, the `LongPressRecognizer` would have no
way to "wake up" at 500ms — it only sees Down/Move/Up/Cancel, all of which
are user-initiated. `Tick` is the missing clock input. The pipeline already
ticks every frame for animations (`window.rs:644`
`self.animation_ticker.tick()`), so feeding it into the arena is one extra
call per frame.

## Layer 1 — Arena time dimension + `LongPressRecognizer`

### `ArenaEvent::Tick { now }`

New variant in `vexo/src/gestures/arena_event.rs`:

```rust
#[derive(Clone, Copy, Debug)]
pub enum ArenaEvent {
    Down { position: Point<Logical> },
    Move { position: Point<Logical> },
    Up { position: Point<Logical> },
    Cancel,
    Tick { now: std::time::Instant },  // NEW
}
```

`now: std::time::Instant` so recognizers compute elapsed time. Existing
recognizers (`TapRecognizer`, `VerticalDragRecognizer`) add an
`ArenaEvent::Tick { .. } => {}` no-op arm — their `handle_event` matches
are exhaustive today, so this is a required (trivial) edit.

### `arena.rs::handle_event` — resolve on `Tick`

Currently `handle_event` only calls `try_resolve()` on `Move`/`Up`. Add
`Tick` to that branch so a `Tick`-driven `Accepted` (long-press firing at
500ms) triggers resolution:

```rust
match event {
    ArenaEvent::Cancel => { /* unchanged */ }
    ArenaEvent::Move { .. } | ArenaEvent::Up { .. } | ArenaEvent::Tick { .. } => {
        self.try_resolve();
        match self.winner { /* unchanged */ }
    }
    ArenaEvent::Down { .. } => ArenaOutcome::Open,
}
```

The `current_position` extraction at the top of `handle_event` (line 96-101)
needs a `Tick` arm: `Tick` uses the last known position (the recognizer
stored `down_position` on `Down`, and `Move` updated `current_position`
since — but for `Tick`, the position is irrelevant; `LongPressRecognizer`
uses `down_position` for its slop check, not `current_position`). Use
`self.down_position` as the `Tick` arm's `current_position` (same as
`Cancel`).

### `LongPressRecognizer`

New file `vexo/src/gestures/long_press.rs`, modeled exactly on `tap.rs`:

```rust
pub struct LongPressRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
    down_time: Option<std::time::Instant>,
}

impl LongPressRecognizer {
    pub fn new() -> Self {
        Self {
            resolution: RecognizerResolution::Pending,
            down_position: Point::zero(),
            down_time: None,
        }
    }

    pub fn down_position(&self) -> Point<Logical> {
        self.down_position
    }
}
```

`down_time: Option<Instant>` (not bare `Instant`) so the recognizer is
safely default-constructible for the arena's `add()` path; `None` means
"never seen Down" → `Tick` is a no-op.

**State machine** (mirrors Flutter's `LongPressGestureRecognizer`):

| Event | Transition |
|---|---|
| `Down { .. }` | store `down_position = ctx.down_position`, `down_time = Some(now)`, stay `Pending` |
| `Move { .. }` | if `\|Δx\|` or `\|Δy\|` from `down_position` > `LONG_PRESS_SLOP` (18px) → `Rejected`; else stay `Pending` (timer continues) |
| `Tick { now }` | if `Pending` and `Some(down_time)` and `now - down_time >= LONG_PRESS_DURATION` → `Accepted`; else stay `Pending` |
| `Up { .. }` | `Rejected` (finger lifted before 500ms — it was a tap, not a long-press) |
| `Cancel` | `Rejected` |

Slop check uses **net displacement** from `down_position` (like `TapRecognizer`),
not cumulative delta (like `VerticalDragRecognizer`). Rationale: a finger
that drifts back and forth within 18px is still "essentially still" — a
long-press. A finger that cumulatively traveled 30px but ended 5px from
start has been moving, not holding.

**Critical composition details:**
- On `Tick`-driven `Accepted`, `arena.try_resolve()` runs, calls
  `declare_winner(long_press_index)`, which feeds `Cancel` to all other
  recognizers (tap, vertical drag) — clearing their state. The arena closes.
- The `on_arena_winner_update` call to the element uses the triggering
  `Tick` event (not a synthesized `Up`), so the element's dispatch path
  must handle `Tick` as a "fire long-press" signal — see Layer 2.

### Constants (in `gestures/mod.rs` next to `TAP_SLOP`)

```rust
/// Duration the pointer must remain pressed (without exceeding slop)
/// before a long-press is recognized. Matches iOS
/// `UILongPressGestureRecognizer`'s default `minimumPressDuration`.
pub(crate) const LONG_PRESS_DURATION: std::time::Duration =
    std::time::Duration::from_millis(500);

/// Movement threshold (in logical pixels) beyond which a long-press is
/// rejected. Same value as TAP_SLOP and VERTICAL_DRAG_SLOP — one slop
/// for all three keeps the feel consistent and avoids surprising
/// "I moved 17px and got a long-press instead of a scroll" edge cases.
pub(crate) const LONG_PRESS_SLOP: f32 = 18.0;
```

### Pipeline wiring

The arena is stored on `ThreeTreePipeline` as
`current_arena: Option<GestureArena>` (`pipeline.rs:160`). New method
`pipeline.tick_arena(now: Instant)`:

- If `current_arena` is `Some` and not closed: feed
  `ArenaEvent::Tick { now }`, and if the feed resolves the arena
  (`ArenaOutcome::Resolved`), call `on_arena_winner_update` on the winner
  element with the `Tick` event — mirroring the existing Move/Up
  winner-dispatch in `event_handler.rs:172-194` and `:296-329`.
- If closed or `None`: no-op.

Called from `window.rs` right after `self.animation_ticker.tick()` (line
644), before `perform_rebuilds()`. The `now` is `Instant::now()`
(matching what `AnimationTicker::tick` uses internally; if the window
already captures a frame-start `Instant`, reuse it — verify during
implementation).

**Why before rebuilds:** the long-press firing may `set` a Signal (e.g.
open the menu via `controller.show`), which marks elements dirty;
`perform_rebuilds()` (called later in the frame) needs to see that dirty
mark to re-render and show the menu this frame.

### Edge cases (Layer 1)

- **Arena dropped before 500ms:** user lifts finger at 300ms →
  `event_handler.rs` feeds `Up`, recognizer goes `Rejected`,
  `sweep_on_up` runs (tap wins if registered). Arena is dropped
  (`*current_arena = None` at line 222). No `Tick` is ever fed to a dead
  arena. Clean.
- **Scroll wins mid-hold:** finger down, at 200ms finger drifts 25px →
  `Move` feeds both recognizers: `VerticalDragRecognizer` accepts
  (cumulative Δy > 18), `LongPressRecognizer` rejects (movement > slop).
  Arena resolves to drag, long-press is cancelled via `declare_winner`'s
  Cancel feed. No long-press fires.
- **Tap and long-press both registered:** tap accepts on `Up`, long-press
  accepts on `Tick` (always strictly before `Up`, since 500ms < finger-
  hold-time). They're mutually exclusive by construction. The arena's
  deepest-wins-on-tie rule (`arena.rs:262`) is irrelevant — there's no tie.
- **Tick arrives after Down with no Down seen:** `down_time` is `None` →
  `Tick` arm is a no-op, recognizer stays `Pending`. Defensive against
  misordered events.

## Layer 2 — Framework gesture API

### `GestureDetector` widget

Add a fifth callback field to `vexo/src/widgets/gesture_detector.rs`,
mirroring `on_secondary_press`'s shape (callback receives global position +
element bounds — needed because `context_menu_trigger` calls
`controller.show(pos, builder)`):

```rust
pub struct GestureDetector {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    on_press: Option<Rc<RefCell<dyn FnMut()>>>,
    on_release: Option<Rc<RefCell<dyn FnMut()>>>,
    on_tap: Option<Rc<RefCell<dyn FnMut()>>>,
    on_secondary_press:
        Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
    on_long_press:
        Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,  // NEW
}
```

Builder method `on_long_press(mut self, f)` mirrors `on_secondary_press`.
Updated in `rebuild()` (line 414-419) alongside the other callbacks:

```rust
self.on_long_press = gd.on_long_press.clone();
```

### `Widget` trait default method (`vexo/src/widgets/mod.rs:199-231`)

Add `on_long_press` alongside `on_secondary_press` — wraps `self` in a
`GestureDetector`:

```rust
fn on_long_press(
    self,
    f: impl FnMut(Point<Logical>, Bounds<Logical>) + 'static,
) -> Box<dyn Widget>
where
    Self: Sized + 'static,
{
    GestureDetector::new(self).on_long_press(f).boxed()
}
```

### `GestureDetectorElement` changes

**`register_gestures` (line 385-392):** register a `LongPressRecognizer`
when `on_long_press` is set:

```rust
fn register_gestures(&mut self, arena: &mut GestureArena, self_id: ElementKey) {
    if self.on_tap.is_some() {
        arena.add(Box::new(TapRecognizer::new()), self_id);
    }
    if self.on_long_press.is_some() {
        arena.add(Box::new(LongPressRecognizer::new()), self_id);
    }
}
```

Registration order: tap first, then long-press. Order is cosmetic — tap
and long-press can't tie (see Layer 1 edge case above).

**`on_arena_winner_update` (line 394-408):** currently only handles
`Up` → tap. Add a `Tick` arm. The `_ctx` parameter becomes used, so rename
to `ctx`:

```rust
fn on_arena_winner_update(
    &mut self,
    recognizer: &dyn GestureRecognizer,
    event: &ArenaEvent,
    ctx: &mut EventContext,
) {
    match event {
        ArenaEvent::Up { .. } => {
            if recognizer.accepted() {
                if let Some(callback) = &self.on_tap {
                    (callback.borrow_mut())();
                }
            }
        }
        ArenaEvent::Tick { .. } => {
            // Long-press fires at 500ms while the finger is still down.
            // Position comes from the recognizer's `down_position()`
            // (the press location — semantically the long-press happened
            // *at* where the finger went down, not where it drifted to by
            // 500ms). Bounds come from the EventContext, which the
            // pipeline's tick_arena dispatch builds from
            // `render_objects.bounds_for_element(winner_id)` (same lookup
            // as the Move-winner path at event_handler.rs:298).
            if recognizer.accepted() {
                if let Some(callback) = &self.on_long_press {
                    if let Some(lp) = recognizer
                        .as_any()
                        .downcast_ref::<crate::gestures::LongPressRecognizer>()
                    {
                        (callback.borrow_mut())(lp.down_position(), ctx.bounds());
                    }
                }
            }
        }
        _ => {}
    }
}
```

**Position/bounds sourcing for the long-press callback:** `on_secondary_press`
gets `(position, bounds)` from `EventContext` inside `on_event` (line 361).
But `on_arena_winner_update` is called from the pipeline's tick path, not
from `on_event`. The `EventContext` passed to `on_arena_winner_update` in
the new `pipeline.tick_arena` dispatch must be built with:
- `position`: the recognizer's `down_position()` (the press location —
  semantically correct: the long-press happened *at* where the finger went
  down, not where it drifted to by 500ms). Exposed via a getter on
  `LongPressRecognizer` (like `VerticalDragRecognizer::down_position()` at
  `vertical_drag.rs:50`). The `on_arena_winner_update` code downcasts the
  `&dyn GestureRecognizer` to `&LongPressRecognizer` to read it.
- `bounds`: looked up from `render_objects.bounds_for_element(winner_id)`
  (same lookup as the Move-winner path at `event_handler.rs:298`). Passed
  through the `EventContext`, read via `ctx.bounds()`.

**`on_event` (line 336-383):** no change. Long-press is arena-mediated, not
immediate. `on_event` only handles the immediate `on_press`/`on_release`/
`on_secondary_press` paths. A Primary press with `on_long_press` set but no
`on_press`/`on_tap` falls through `on_event` returning `None` (bubbles),
while `register_gestures` still registers the `LongPressRecognizer` in the
arena. This is the correct split — the arena owns long-press resolution,
not `on_event`.

### Why `on_long_press` takes `(Point, Bounds)` like `on_secondary_press`

So `context_menu_trigger` can call `controller.show(pos, builder)` uniformly
for both triggers. If `on_long_press` took `()` like `on_tap`, the trigger
would have no position and the menu would open at a default location.
Keeping the signature aligned with `on_secondary_press` makes Layer 3 a
clean branch.

## Layer 3 — Wiring (`shared_app` + `vexo_uikit`)

### `context_menu_trigger` branches on platform

`vexo_uikit/src/context_menu.rs:631-640` today:

```rust
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    builder: MenuBuilder,
) -> Box<dyn Widget> {
    let ctrl = controller.clone();
    child.on_secondary_press(move |pos, _bounds| {
        ctrl.show(pos, builder.clone());
    })
}
```

Change to branch on `Platform::current()`:

```rust
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    builder: MenuBuilder,
) -> Box<dyn Widget> {
    let ctrl = controller.clone();
    match Platform::current() {
        Platform::Desktop => child.on_secondary_press(move |pos, _bounds| {
            ctrl.show(pos, builder.clone());
        }),
        Platform::Mobile => child.on_long_press(move |pos, _bounds| {
            ctrl.show(pos, builder.clone());
        }),
    }
}
```

**Why `Platform::current()` (compile-time) not a runtime parameter:** the
existing `effective_platform()` override pattern (`conversation_list.rs:129`)
exists for *testability* — letting tests inject a platform. But
`context_menu_trigger` is called from `ChatScreen::render`, which has dozens
of existing tests asserting right-click behavior (`chat_screen.rs:942-1210`).
Those tests would break en masse if the trigger switched to long-press
under an injected `Platform::Mobile`.

Instead, the existing right-click tests continue to work as-is (they
compile and run on desktop, where `Platform::current() == Desktop` →
`on_secondary_press`). New long-press tests (see Testing) synthesize the
gesture via `ArenaEvent::Tick` directly against the arena, or against a
`GestureDetectorElement` in isolation — they don't need
`context_menu_trigger` to branch under a fake platform.

If we later want to test the mobile branch of `context_menu_trigger` itself,
add an optional `platform: Platform` parameter then. YAGNI for now.

### Mobile wiring fix (`shared_app/src/chats/mod.rs:106`)

Today (broken — pre-existing bug, not introduced by this feature):

```rust
// Inside MobileChatsPage's destination builder for ChatsRoute::Chat(id):
ChatScreen {
    // ...
    context_menu: ContextMenuController::new(),  // fresh, not the root host's
}
```

The root `ContextMenu` host at `app.rs:163` wraps `state.context_menu`. A
trigger calling `show()` on a *different* controller instance never reaches
the mounted host — the menu's `Stack` overlay never renders. On desktop this
works because `desktop.rs:118` already passes `self.context_menu.clone()`
(the shared controller). Mobile has the bug.

Fix: pass the shared controller through, exactly as desktop does. Required
plumbing:

1. **`MobileChatsPage`** gains a `context_menu: ContextMenuController`
   field.
2. **`build_chats_tab` (mobile variant, `mod.rs:117`)** receives the
   controller from `app.rs` and passes it into `MobileChatsPage`.
3. **`app.rs` mobile path** calls `build_chats_tab(state.context_menu.clone())`
   (mirroring the desktop call at `app.rs:46`/`:121`).
4. **`MobileChatsPage::render` destination builder** constructs `ChatScreen`
   with `context_menu: self.context_menu.clone()` instead of
   `ContextMenuController::new()`.

The fix shape:

```rust
struct MobileChatsPage {
    context_menu: ContextMenuController,  // NEW
    // ... existing fields
}

// In render() destination builder:
ChatScreen {
    // ...
    context_menu: self.context_menu.clone(),  // shared, not fresh
}
```

This mirrors the desktop path exactly. It's a pre-existing bug that
surfaces as "menu trigger fires but menu never renders" — fixing it is
required for long-press to work on mobile at all.

### Desktop: zero changes

`ChatScreen` is shared, and `context_menu_trigger` now branches on platform.
Desktop compiles to `Platform::Desktop` → `on_secondary_press` → existing
behavior. No desktop code path changes, no desktop test changes.

### Why no `cfg` gates

`on_long_press` is a framework-level API available on all platforms. On
desktop, `context_menu_trigger` simply doesn't call it. If a desktop widget
*wants* long-press (e.g. a future trackpad scenario), the API is there.
Keeping it ungated matches how `on_secondary_press` is available on mobile
too (just never triggered because touch maps to Primary).

## Testing

Three layers of tests, each isolating its layer.

### Layer 1 tests — `vexo/src/gestures/long_press.rs` (unit tests in-file)

Mirror the structure of `tap.rs`'s tests. Pure recognizer state machine, no
pipeline.

- `long_press_accepts_on_tick_after_500ms` — Down, Tick at 499ms (Pending),
  Tick at 500ms (Accepted).
- `long_press_rejects_on_up_before_500ms` — Down, Up at 300ms (Rejected).
  Verifies a quick tap doesn't fire long-press.
- `long_press_rejects_on_move_past_slop` — Down, Move 25px (Rejected).
- `long_press_rejects_on_cancel` — Down, Cancel (Rejected).
- `long_press_tick_is_noop_before_down` — Tick without prior Down (Pending,
  `down_time` is `None`).
- `long_press_stays_pending_on_tick_before_500ms` — Down, Tick at 250ms
  (Pending).
- `long_press_stays_pending_on_move_within_slop` — Down, Move 10px
  (Pending, timer continues).

### Layer 1 tests — `vexo/src/gestures/arena.rs` (integration with arena)

Two tests, extending the existing `mod tests`:

- `arena_resolves_long_press_winner_on_tick` — arena with Tap +
  LongPress, Down, Tick at 499ms (Open), Tick at 500ms (Resolved,
  long-press at index 1 wins, tap fed Cancel → Rejected).
- `arena_long_press_rejected_when_drag_wins_first` — arena with LongPress
  + VerticalDrag, Down, Move 25px at 200ms (drag accepts, long-press
  rejected via `declare_winner`'s Cancel feed). Verifies the
  scroll-composition guarantee.

### Layer 2 test — `vexo/src/widgets/gesture_detector.rs` (element fires callback)

One test in the existing test module. Mount a `GestureDetector` with
`on_long_press` set, feed Down via `handle_event`, tick the pipeline past
500ms via `pipeline.tick_arena(Instant)` (or the window tick path), assert
the callback fired with the down position + element bounds. Mirror the
existing tap tests' shape in this file.

This is the test that proves the `Tick`-winner dispatch path (Layer 2's
`on_arena_winner_update` `Tick` arm) works end-to-end through the pipeline,
not just inside the arena.

### Layer 3 test — `shared_app/src/chats/chat_screen.rs` (mobile menu opens on long-press)

New test alongside the existing `test_right_click_bubble_opens_context_menu`
(line 942). Shape:

- Build a `ChatScreen` wrapped in `ContextMenu` (same as existing tests).
- Instead of synthesizing a `PointerButton::Secondary` press, synthesize:
  Primary press at bubble position → `pipeline.tick_arena(now + 500ms)` →
  assert menu content ("Copy"/"Reply"/"Delete") appears in the render tree.
- Assert `controller.phase() == Open` (or the render-tree presence,
  matching the existing test's style).

**This test runs on desktop** (where `Platform::current() == Desktop`) but
exercises the long-press recognizer path directly — it doesn't go through
`context_menu_trigger`'s platform branch. It tests the recognizer +
element + menu wiring, not the trigger's platform selection. The trigger's
platform branch is compile-time-verified (the mobile branch simply doesn't
compile on desktop if broken).

### Test we explicitly DON'T write

A test that forces `context_menu_trigger` into the mobile branch on
desktop. This would require adding the optional `platform: Platform`
parameter we deferred in Layer 3. Per YAGNI: the mobile branch is
`child.on_long_press(...)` — a one-liner that either compiles or doesn't.
If we later add platform injection for testability, the test comes then.

### Existing tests — regression check

- All four right-click tests in `chat_screen.rs` (lines 942-1209) must pass
  unchanged. They compile and run on desktop → `Platform::Desktop` →
  `on_secondary_press` branch. No changes needed.
- All `chat_screen.rs` tests that construct `ChatScreen` with
  `context_menu: ContextMenuController::new()` (lines 432, 446, etc.) —
  these use a fresh controller, not the shared one. They work today because
  they don't test the menu. After the Layer-3 wiring fix, `MobileChatsPage`
  will use the shared controller, but these unit tests construct
  `ChatScreen` directly and remain valid. No changes.
- The mobile wiring fix (Layer 3) changes `MobileChatsPage` — verify no
  existing `MobileChatsPage` test breaks. Need to check during
  implementation; if there's a `MobileChatsPage` test, it may need its
  construction updated to pass a controller.

## File-by-file change summary

| File | Layer | Change |
|---|---|---|
| `vexo/src/gestures/arena_event.rs` | 1 | Add `Tick { now: Instant }` variant. |
| `vexo/src/gestures/mod.rs` | 1 | Add `LONG_PRESS_DURATION`, `LONG_PRESS_SLOP` constants; `pub use long_press::LongPressRecognizer`; `pub mod long_press`. |
| `vexo/src/gestures/long_press.rs` | 1 | New file. `LongPressRecognizer` state machine + unit tests. |
| `vexo/src/gestures/tap.rs` | 1 | Add `Tick { .. } => {}` no-op arm to `handle_event`. |
| `vexo/src/gestures/vertical_drag.rs` | 1 | Add `Tick { .. } => {}` no-op arm to `handle_event`. |
| `vexo/src/gestures/arena.rs` | 1 | `handle_event`: add `Tick` to the `try_resolve` branch; add `Tick` arm to `current_position` extraction; add 2 arena integration tests. |
| `vexo/src/event_handler.rs` | 1 | (Possibly) extract a shared `dispatch_arena_winner` helper used by Move/Up/`Tick` paths — verify during implementation. |
| `vexo/src/pipeline.rs` | 1 | Add `tick_arena(now: Instant)` method that feeds `ArenaEvent::Tick` to `current_arena` and dispatches the winner. |
| `vexo/src/window.rs` | 1 | Call `pipeline.tick_arena(Instant::now())` right after `self.animation_ticker.tick()` (line 644). |
| `vexo/src/widgets/gesture_detector.rs` | 2 | Add `on_long_press` field + builder method; register `LongPressRecognizer` in `register_gestures`; add `Tick` arm to `on_arena_winner_update`; clone in `rebuild`; add element test. |
| `vexo/src/widgets/mod.rs` | 2 | Add `on_long_press` default trait method on `Widget`. |
| `vexo_uikit/src/context_menu.rs` | 3 | `context_menu_trigger` branches on `Platform::current()`. |
| `shared_app/src/chats/mod.rs` | 3 | `MobileChatsPage` gains `context_menu` field; `build_chats_tab` threads it from `app.rs`; destination builder uses `self.context_menu.clone()`. |
| `shared_app/src/app.rs` | 3 | Mobile `build_chats_tab` call passes `state.context_menu.clone()` (mirror desktop). |
| `shared_app/src/chats/chat_screen.rs` | 3 | Add `test_long_press_bubble_opens_context_menu` test. |

## Open questions for implementation

- **`build_chats_tab` mobile signature:** does the mobile `build_chats_tab`
  (`mod.rs:117`) currently take a `ContextMenuController`, or only the
  desktop variant (`desktop.rs:181`)? Verify during implementation; thread
  the controller through accordingly.
- **Frame-start `Instant`:** does `window.rs` already capture a frame-start
  `Instant` that `tick_arena` should reuse, or is `Instant::now()` inside
  `tick_arena` fine? Verify during implementation; `Instant::now()` is
  correct either way, reuse is just a micro-optimization.
- **`dispatch_arena_winner` extraction:** the Move-winner and Up-winner
  dispatch in `event_handler.rs` (lines 172-194, 296-329) duplicates
  ~20 lines. The new Tick-winner dispatch would add a third copy.
  Extract a shared helper during implementation if the duplication is
  bothersome — but keep the extraction mechanical, no behavior change.
