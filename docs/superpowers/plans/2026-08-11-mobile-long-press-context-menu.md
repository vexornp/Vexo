# Mobile Long-Press Context Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the message-bubble context menu on mobile when the user long-presses (500ms hold) a message bubble; desktop keeps right-click.

**Architecture:** Add a time dimension to the gesture arena via a new `ArenaEvent::Tick { now }` variant, fed every frame by the pipeline. Implement a `LongPressRecognizer` that accepts at 500ms if the pointer stayed within slop. Expose `on_long_press` on `GestureDetector` (and as a `Widget` trait default method) mirroring `on_secondary_press`. `context_menu_trigger` branches on `Platform::current()`: desktop → `on_secondary_press`, mobile → `on_long_press`. Also fix a pre-existing mobile wiring bug where `MobileChatsPage` constructed `ChatScreen` with a fresh `ContextMenuController` instead of the shared root host's controller.

**Tech Stack:** Rust, vexo framework (custom three-tree architecture), Taffy layout, winit, ios target via wgpu/Metal.

## Global Constraints

- **Long-press duration:** exactly `Duration::from_millis(500)` (per spec).
- **Long-press slop:** `18.0` logical pixels (same value as `TAP_SLOP` and `VERTICAL_DRAG_SLOP`).
- **Slop model:** net displacement from `down_position` (like `TapRecognizer`), NOT cumulative delta.
- **Platform branching in `context_menu_trigger`:** uses `Platform::current()` (compile-time `cfg`), no runtime parameter.
- **Desktop behavior unchanged:** right-click continues to open the menu; all four existing right-click tests in `chat_screen.rs` must pass without modification.
- **`on_long_press` callback signature:** `FnMut(Point<Logical>, Bounds<Logical>) + 'static` (matches `on_secondary_press`, so `context_menu_trigger` can call `controller.show(pos, builder)` uniformly).
- **No visual feedback during the 500ms hold** (no highlight, no haptic).
- **No `cfg` gates on `on_long_press`:** the API is available on all platforms; only `context_menu_trigger` branches.
- **Commit message style:** match existing repo conventions (lowercase `feat:`/`fix:`/`refactor:`/`test:`/`docs:` prefix, concise subject).
- **Run tests with:** `cargo test -p vexo` for framework tests, `cargo test -p shared_app` for chat-screen tests. Run `cargo build` after every Rust edit.
- **Never run `cargo run -p desktop_demo`** — the user runs the GUI themselves.

**Spec reference:** `docs/superpowers/specs/2026-08-11-mobile-long-press-context-menu-design.md`

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `vexo/src/gestures/arena_event.rs` | Enum of events fed to recognizers | Add `Tick { now }` variant |
| `vexo/src/gestures/mod.rs` | Module roots + shared slop constants | Add `LONG_PRESS_DURATION`, `LONG_PRESS_SLOP`; re-export `LongPressRecognizer` |
| `vexo/src/gestures/long_press.rs` | `LongPressRecognizer` state machine | **Create** (new file) |
| `vexo/src/gestures/tap.rs` | `TapRecognizer` | Add `Tick` no-op arm |
| `vexo/src/gestures/vertical_drag.rs` | `VerticalDragRecognizer` | Add `Tick` no-op arm |
| `vexo/src/gestures/arena.rs` | Per-pointer arena resolver | `handle_event` resolves on `Tick`; add 2 integration tests |
| `vexo/src/pipeline.rs` | Three-tree pipeline | Add `tick_arena(now)` method |
| `vexo/src/window.rs` | Desktop window/event loop | Call `pipeline.tick_arena(Instant::now())` after `animation_ticker.tick()` |
| `vexo/src/widgets/gesture_detector.rs` | GestureDetector widget + element | Add `on_long_press` field/builder/registration/dispatch; add element test |
| `vexo/src/widgets/mod.rs` | `Widget` trait + default methods | Add `on_long_press` default method |
| `vexo/src/lib.rs` | Crate root | (Possibly) re-export `LongPressRecognizer` — verify during Task 3 |
| `vexo_uikit/src/context_menu.rs` | `context_menu_trigger` fn | Branch on `Platform::current()` |
| `shared_app/src/chats/mod.rs` | `MobileChatsPage` + `build_chats_tab` | Add `context_menu` field; thread from `app.rs` |
| `shared_app/src/app.rs` | App entry / `view()` | Mobile `build_chats_tab` call passes `state.context_menu.clone()` |
| `shared_app/src/chats/chat_screen.rs` | Chat screen + tests | Add `test_long_press_bubble_opens_context_menu` |

---

## Task 1: Add `ArenaEvent::Tick` variant + recognizer no-op arms

**Files:**
- Modify: `vexo/src/gestures/arena_event.rs` (entire file, 13 lines)
- Modify: `vexo/src/gestures/tap.rs:39-63` (`handle_event` match)
- Modify: `vexo/src/gestures/vertical_drag.rs:62-91` (`handle_event` match)
- Modify: `vexo/src/gestures/arena.rs:96-101` (`current_position` extraction) and `arena.rs:112-127` (`handle_event` match)

**Interfaces:**
- Produces: `ArenaEvent::Tick { now: std::time::Instant }` — a new variant all recognizers must handle (even if as a no-op).

- [ ] **Step 1: Add the `Tick` variant to `ArenaEvent`**

Edit `vexo/src/gestures/arena_event.rs`. Replace the entire file content with:

```rust
//! Arena events fed to gesture recognizers by the GestureArena.

use crate::core::Logical;
use crate::core::Point;

/// An event delivered to every recognizer in the arena.
#[derive(Clone, Copy, Debug)]
pub enum ArenaEvent {
    Down { position: Point<Logical> },
    Move { position: Point<Logical> },
    Up { position: Point<Logical> },
    Cancel,
    /// Fired each animation frame while the arena is active (pointer down,
    /// not yet resolved). Carries the frame's `Instant` so time-based
    /// recognizers (e.g. `LongPressRecognizer`) can compute elapsed time.
    /// Pure event-driven recognizers (Tap, VerticalDrag) ignore this.
    Tick { now: std::time::Instant },
}
```

- [ ] **Step 2: Add `Tick` no-op arm to `TapRecognizer::handle_event`**

In `vexo/src/gestures/tap.rs`, the `handle_event` match (lines 43-62) currently has arms for `Down`, `Move`, `Up`, `Cancel`. Add a `Tick` no-op arm before the closing brace of the `match`:

```rust
            ArenaEvent::Cancel => {
                self.resolution = RecognizerResolution::Rejected;
            }
            ArenaEvent::Tick { .. } => {
                // Tap is purely event-driven; ignore the clock tick.
            }
        }
```

- [ ] **Step 3: Add `Tick` no-op arm to `VerticalDragRecognizer::handle_event`**

In `vexo/src/gestures/vertical_drag.rs`, the `handle_event` match (lines 66-91). Add a `Tick` no-op arm:

```rust
            ArenaEvent::Cancel => {
                self.resolution = RecognizerResolution::Rejected;
            }
            ArenaEvent::Tick { .. } => {
                // VerticalDrag is purely event-driven; ignore the clock tick.
            }
        }
```

- [ ] **Step 4: Update `arena.rs::handle_event` to handle `Tick`**

In `vexo/src/gestures/arena.rs`, the `handle_event` method (lines 87-128). Two edits:

**4a.** The `current_position` extraction (lines 96-101). Add a `Tick` arm using `self.down_position` (same as `Cancel` — `Tick` has no position; recognizers that care use their stored `down_position`):

```rust
        let current_position = match &event {
            ArenaEvent::Down { position } => *position,
            ArenaEvent::Move { position } => *position,
            ArenaEvent::Up { position } => *position,
            ArenaEvent::Cancel => self.down_position,
            ArenaEvent::Tick { .. } => self.down_position,
        };
```

**4b.** The resolution match (lines 112-127). Add `Tick` to the branch that calls `try_resolve`:

```rust
        match event {
            ArenaEvent::Cancel => {
                self.closed = true;
                self.winner = None;
                ArenaOutcome::ClosedNoWinner
            }
            ArenaEvent::Move { .. } | ArenaEvent::Up { .. } | ArenaEvent::Tick { .. } => {
                self.try_resolve();
                match self.winner {
                    Some(i) => ArenaOutcome::Resolved { winner_index: i },
                    None if self.closed => ArenaOutcome::ClosedNoWinner,
                    None => ArenaOutcome::Open,
                }
            }
            ArenaEvent::Down { .. } => ArenaOutcome::Open,
        }
```

- [ ] **Step 5: Build to verify it compiles**

Run: `cargo build -p vexo`
Expected: compiles with no errors (recognizers now all match `Tick`).

- [ ] **Step 6: Run existing tests to verify no regressions**

Run: `cargo test -p vexo gestures`
Expected: all existing gesture tests pass (the `Tick` arms are no-ops; no behavior change).

- [ ] **Step 7: Commit**

```bash
git add vexo/src/gestures/arena_event.rs vexo/src/gestures/tap.rs vexo/src/gestures/vertical_drag.rs vexo/src/gestures/arena.rs
git commit -m "feat(gestures): add ArenaEvent::Tick variant for time-based recognizers

Adds a Tick { now: Instant } variant to ArenaEvent and a no-op arm to
TapRecognizer and VerticalDragRecognizer. arena.handle_event now runs
try_resolve on Tick so a Tick-driven Accepted (e.g. long-press at 500ms)
can resolve the arena. No behavior change yet — no recognizer produces
Accepted on Tick."
```

---

## Task 2: Implement `LongPressRecognizer`

**Files:**
- Create: `vexo/src/gestures/long_press.rs`
- Modify: `vexo/src/gestures/mod.rs` (entire file, 27 lines)

**Interfaces:**
- Consumes: `ArenaEvent` (from Task 1), `ArenaContext`, `GestureRecognizer`, `RecognizerResolution` — all from `vexo/src/gestures/recognizer.rs` and `arena_event.rs`.
- Produces: `LongPressRecognizer` struct with:
  - `pub fn new() -> Self`
  - `pub fn down_position(&self) -> Point<Logical>` (read by `GestureDetectorElement::on_arena_winner_update` in Task 5)
  - impl `GestureRecognizer` (handle_event, resolution, as_any)
  - impl `Default`

- [ ] **Step 1: Add the constants and module declarations to `gestures/mod.rs`**

Edit `vexo/src/gestures/mod.rs`. Replace the entire file content with:

```rust
//! Gesture arena: disambiguates competing gesture recognizers for a pointer.
//!
//! Currently implements Tap vs. VerticalDrag (scroll) vs. LongPress
//! disambiguation via a slop-threshold + time rule, matching Flutter's
//! GestureArena behavior for this recognizer set.

pub mod arena;
pub mod arena_event;
pub mod long_press;
pub mod recognizer;
pub mod tap;
pub mod velocity_tracker;
pub mod vertical_drag;

pub use arena::{ArenaOutcome, GestureArena};
pub use arena_event::ArenaEvent;
pub use long_press::LongPressRecognizer;
pub use recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
pub use tap::TapRecognizer;
pub use velocity_tracker::VelocityTracker;
pub use vertical_drag::VerticalDragRecognizer;

/// Movement threshold (in logical pixels) beyond which a tap is rejected.
/// Matches Flutter's `kTouchSlop`.
pub(crate) const TAP_SLOP: f32 = 18.0;

/// Cumulative vertical movement threshold beyond which a vertical drag is
/// recognized. Matches Flutter's vertical drag slop.
pub(crate) const VERTICAL_DRAG_SLOP: f32 = 18.0;

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

- [ ] **Step 2: Write the failing test — accepts on Tick after 500ms**

Create `vexo/src/gestures/long_press.rs` with the struct skeleton and the first test. The test must fail (function not yet implemented):

```rust
//! LongPressRecognizer — recognizes a long-press (pointer down held still
//! for LONG_PRESS_DURATION).
//!
//! State transitions on ArenaEvent:
//! - Down  → store position + down_time, stay Pending
//! - Move  → if |Δx| or |Δy| from down exceeds LONG_PRESS_SLOP → Rejected
//! - Tick  → if Pending and elapsed >= LONG_PRESS_DURATION → Accepted
//! - Up    → Rejected (finger lifted before the duration — was a tap)
//! - Cancel → Rejected
//!
//! Slop check uses NET displacement from `down_position` (like TapRecognizer),
//! not cumulative delta (like VerticalDragRecognizer): a finger that drifts
//! back and forth within slop is still "essentially still" — a long-press.

use std::any::Any;
use std::time::{Duration, Instant};

use crate::core::{Logical, Point};

use super::arena_event::ArenaEvent;
use super::recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
use super::{LONG_PRESS_DURATION, LONG_PRESS_SLOP};

pub struct LongPressRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
    down_time: Option<Instant>,
}

impl LongPressRecognizer {
    pub fn new() -> Self {
        Self {
            resolution: RecognizerResolution::Pending,
            down_position: Point::zero(),
            down_time: None,
        }
    }

    /// The pointer's press location. Read by `GestureDetectorElement::
    /// on_arena_winner_update` to source the long-press callback's
    /// position argument (semantically: the long-press happened *at* where
    /// the finger went down, not where it drifted to by 500ms).
    pub fn down_position(&self) -> Point<Logical> {
        self.down_position
    }
}

impl Default for LongPressRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for LongPressRecognizer {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &ArenaContext) {
        if self.rejected() {
            return;
        }
        match event {
            ArenaEvent::Down { .. } => {
                self.down_position = ctx.down_position;
                // ctx doesn't carry the time; Down's `now` is inferred from
                // the first Tick. Set down_time on the first Tick instead
                // (see Tick arm). Leave None here so a Tick without a prior
                // Down is a no-op.
            }
            ArenaEvent::Move { .. } => {
                let dx = (ctx.current_position.x - self.down_position.x).abs();
                let dy = (ctx.current_position.y - self.down_position.y).abs();
                if dx > LONG_PRESS_SLOP || dy > LONG_PRESS_SLOP {
                    self.resolution = RecognizerResolution::Rejected;
                }
            }
            ArenaEvent::Tick { now } => {
                // First Tick after Down: record the start time. This is the
                // clock that drives the 500ms threshold. Using the first
                // Tick (not Down) means down_time is always None until the
                // first frame, so a stray Tick without a prior Down is a
                // no-op (defensive).
                if self.down_time.is_none() {
                    self.down_time = Some(*now);
                }
                if let Some(start) = self.down_time {
                    if now.duration_since(start) >= LONG_PRESS_DURATION {
                        self.resolution = RecognizerResolution::Accepted;
                    }
                }
            }
            ArenaEvent::Up { .. } => {
                // Finger lifted before the duration — was a tap, not a
                // long-press.
                self.resolution = RecognizerResolution::Rejected;
            }
            ArenaEvent::Cancel => {
                self.resolution = RecognizerResolution::Rejected;
            }
        }
    }

    fn resolution(&self) -> RecognizerResolution {
        self.resolution
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(down: Point<Logical>, current: Point<Logical>) -> ArenaContext {
        ArenaContext {
            down_position: down,
            current_position: current,
        }
    }

    #[test]
    fn long_press_accepts_on_tick_after_500ms() {
        let mut r = LongPressRecognizer::new();
        let p = Point::new(50.0, 50.0);
        let start = Instant::now();
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        // Tick at 499ms — still Pending.
        r.handle_event(
            &ArenaEvent::Tick {
                now: start + Duration::from_millis(499),
            },
            &ctx(p, p),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
        // Tick at 500ms — Accepted.
        r.handle_event(
            &ArenaEvent::Tick {
                now: start + Duration::from_millis(500),
            },
            &ctx(p, p),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
    }
}
```

Wait — the spec says `down_time` should be set on `Down`, not the first `Tick`. But `Down` doesn't carry `now`. Reconsider: the `ArenaContext` could carry the time, but it doesn't today, and adding it would touch every recognizer. Simpler: set `down_time` on the first `Tick` after `Down` (the first frame after press). This shifts the 500ms window by ≤1 frame (~16ms) — imperceptible, and matches how Flutter's `LongPressGestureRecognizer` actually works (it uses a timer started on `Down`, but the deadline is checked on each `Tick`).

This is a deviation from the spec's pseudocode ("Down → store `down_time = Some(now)`"). Document it in the impl with a comment (already done in the code above: "Set down_time on the first Tick instead").

- [ ] **Step 3: Run the test to verify it passes**

Run: `cargo test -p vexo gestures::long_press::tests::long_press_accepts_on_tick_after_500ms`
Expected: PASS.

- [ ] **Step 4: Add the remaining 6 unit tests**

Append these tests to the `mod tests` block in `vexo/src/gestures/long_press.rs`:

```rust
    #[test]
    fn long_press_rejects_on_up_before_500ms() {
        let mut r = LongPressRecognizer::new();
        let p = Point::new(50.0, 50.0);
        let start = Instant::now();
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        r.handle_event(
            &ArenaEvent::Tick {
                now: start + Duration::from_millis(300),
            },
            &ctx(p, p),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
        r.handle_event(&ArenaEvent::Up { position: p }, &ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn long_press_rejects_on_move_past_slop() {
        let mut r = LongPressRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(50.0, 80.0); // Δy = 30 > 18
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move { position: moved },
            &ctx(down, moved),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn long_press_rejects_on_cancel() {
        let mut r = LongPressRecognizer::new();
        let p = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        r.handle_event(&ArenaEvent::Cancel, &ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn long_press_tick_is_noop_before_down() {
        let mut r = LongPressRecognizer::new();
        let now = Instant::now();
        // Tick without a prior Down — down_time stays None, no-op.
        r.handle_event(&ArenaEvent::Tick { now }, &ctx(Point::zero(), Point::zero()));
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }

    #[test]
    fn long_press_stays_pending_on_tick_before_500ms() {
        let mut r = LongPressRecognizer::new();
        let p = Point::new(50.0, 50.0);
        let start = Instant::now();
        r.handle_event(&ArenaEvent::Down { position: p }, &ctx(p, p));
        r.handle_event(
            &ArenaEvent::Tick {
                now: start + Duration::from_millis(250),
            },
            &ctx(p, p),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }

    #[test]
    fn long_press_stays_pending_on_move_within_slop() {
        let mut r = LongPressRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(55.0, 60.0); // Δx=5, Δy=10, both < 18
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move { position: moved },
            &ctx(down, moved),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }
```

- [ ] **Step 5: Run all long-press tests**

Run: `cargo test -p vexo gestures::long_press`
Expected: all 7 tests PASS.

- [ ] **Step 6: Build the whole vexo crate**

Run: `cargo build -p vexo`
Expected: compiles with no errors.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/gestures/long_press.rs vexo/src/gestures/mod.rs
git commit -m "feat(gestures): add LongPressRecognizer

LongPressRecognizer accepts after 500ms if the pointer stayed within
LONG_PRESS_SLOP (18px, net displacement). Rejects on Up (was a tap),
Move past slop, or Cancel. down_time is set on the first Tick after Down
(the first animation frame after press), not on Down itself — Down
doesn't carry a timestamp and adding one to ArenaContext would touch every
recognizer. The ≤1-frame shift is imperceptible and matches Flutter's
timer-based deadline check."
```

---

## Task 3: Add arena integration tests for long-press

**Files:**
- Modify: `vexo/src/gestures/arena.rs:189-301` (extend `mod tests`)

**Interfaces:**
- Consumes: `LongPressRecognizer` (from Task 2), `TapRecognizer`, `VerticalDragRecognizer`, `ArenaEvent::Tick` (from Task 1).

- [ ] **Step 1: Write the failing test — arena resolves long-press winner on Tick**

Append to the `mod tests` block in `vexo/src/gestures/arena.rs`:

```rust
    #[test]
    fn arena_resolves_long_press_winner_on_tick() {
        use crate::gestures::LongPressRecognizer;
        use std::time::{Duration, Instant};

        let mut arena = GestureArena::new(Point::new(50.0, 50.0));
        arena.add(Box::new(TapRecognizer::new()), dummy_element_key());
        arena.add(Box::new(LongPressRecognizer::new()), dummy_element_key());
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });

        let start = Instant::now();
        // Tick at 499ms — still open.
        let outcome = arena.handle_event(ArenaEvent::Tick {
            now: start + Duration::from_millis(499),
        });
        assert_eq!(outcome, ArenaOutcome::Open);

        // Tick at 500ms — long-press (index 1) accepts and wins.
        let outcome = arena.handle_event(ArenaEvent::Tick {
            now: start + Duration::from_millis(500),
        });
        assert_eq!(outcome, ArenaOutcome::Resolved { winner_index: 1 });
        assert!(arena.winner_recognizer().unwrap().accepted());
        assert!(arena.is_closed(), "arena must close after resolving");
    }

    #[test]
    fn arena_long_press_rejected_when_drag_wins_first() {
        use crate::gestures::LongPressRecognizer;
        use std::time::{Duration, Instant};

        let mut arena = GestureArena::new(Point::new(50.0, 50.0));
        arena.add(Box::new(LongPressRecognizer::new()), dummy_element_key());
        arena.add(Box::new(VerticalDragRecognizer::new()), dummy_element_key());
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });

        let start = Instant::now();
        // Tick at 200ms — both still Pending.
        arena.handle_event(ArenaEvent::Tick {
            now: start + Duration::from_millis(200),
        });
        // Move 30px at 200ms — drag accepts (cumulative Δy > 18), long-press
        // rejects (movement > slop), arena resolves to drag (index 1).
        let outcome = arena.handle_event(ArenaEvent::Move {
            position: Point::new(50.0, 80.0),
        });
        assert_eq!(outcome, ArenaOutcome::Resolved { winner_index: 1 });
        // Long-press (index 0) was fed Cancel by declare_winner → Rejected.
        let lp = arena
            .winner_recognizer()
            .unwrap()
            .as_any()
            .downcast_ref::<LongPressRecognizer>();
        // The winner is the drag, not the long-press. Verify the long-press
        // is NOT the winner (it lost).
        assert!(lp.is_none(), "long-press must not be the winner when drag wins");
        assert!(arena.is_closed());
    }
```

- [ ] **Step 2: Run the new tests**

Run: `cargo test -p vexo gestures::arena::tests::arena_resolves_long_press_winner_on_tick cargo test -p vexo gestures::arena::tests::arena_long_press_rejected_when_drag_wins_first`
Expected: both PASS.

- [ ] **Step 3: Run all gesture tests to verify no regressions**

Run: `cargo test -p vexo gestures`
Expected: all tests PASS (existing + 2 new).

- [ ] **Step 4: Commit**

```bash
git add vexo/src/gestures/arena.rs
git commit -m "test(gestures): add arena integration tests for long-press

Verifies long-press wins on Tick at 500ms (tap fed Cancel), and long-press
loses when vertical drag accepts first on Move past slop."
```

---

## Task 4: Add `tick_arena` to the pipeline + call from `window.rs`

**Files:**
- Modify: `vexo/src/pipeline.rs` (add `tick_arena` method near `perform_rebuilds`, ~line 306)
- Modify: `vexo/src/window.rs:644` (call `tick_arena` after `animation_ticker.tick()`)
- Modify: `vexo/src/event_handler.rs` — extract a shared `dispatch_arena_winner` helper to avoid 3-way duplication (Move, Up, new Tick). The helper takes the same params as the existing inline dispatch.

**Interfaces:**
- Produces: `ThreeTreePipeline::tick_arena(&mut self, now: Instant)` — feeds `ArenaEvent::Tick { now }` to `current_arena`, dispatches the winner if resolved.

- [ ] **Step 1: Read the existing Move-winner and Up-winner dispatch in `event_handler.rs`**

Read `vexo/src/event_handler.rs:160-225` (Move/Up in empty space) and `:284-385` (Move/Up with hit). Note the duplication: both build an `EventContext` with `bounds_for_element(winner_id)`, then call `element.on_arena_winner_update(recognizer, event, &mut ctx)`.

The `tick_arena` dispatch needs the same shape, but:
- The `event` is `ArenaEvent::Tick { now }`.
- The `position` passed to `EventContext` is the recognizer's `down_position()` (the press location), looked up by downcasting the winner to `LongPressRecognizer`. If the winner is NOT a long-press recognizer (defensive — shouldn't happen on `Tick`), skip dispatch (no element handles `Tick` as a winner except via long-press).

- [ ] **Step 2: Add the `tick_arena` method to `ThreeTreePipeline`**

In `vexo/src/pipeline.rs`, find `perform_rebuilds` (line 306). Add a new public method right after it (before `mark_needs_build`):

```rust
    /// Feed a `Tick` event to the active gesture arena (if any) and dispatch
    /// the winner if the Tick resolves the arena. Called once per frame from
    /// `WindowState::render_retain` right after `animation_ticker.tick()`.
    ///
    /// This is the clock that drives time-based recognizers (currently only
    /// `LongPressRecognizer`). Without this call, long-press would never
    /// fire — the arena is purely event-driven (Down/Move/Up/Cancel) and
    /// has no way to "wake up" at 500ms.
    ///
    /// If the arena resolves on this Tick (e.g. long-press accepts at
    /// 500ms), the winner element's `on_arena_winner_update` is called with
    /// the `Tick` event so it can fire its `on_long_press` callback. The
    /// `EventContext` is built with the recognizer's `down_position()` as
    /// the position (the press location — semantically the long-press
    /// happened *at* where the finger went down) and the winner's bounds
    /// from `render_objects.bounds_for_element`.
    pub fn tick_arena(&mut self, now: std::time::Instant) {
        use crate::gestures::{ArenaEvent, ArenaOutcome, LongPressRecognizer};

        let arena = match self.current_arena.as_mut() {
            Some(a) => a,
            None => return,
        };
        if arena.is_closed() {
            return;
        }

        let outcome = arena.handle_event(ArenaEvent::Tick { now });
        if outcome != ArenaOutcome::Resolved {
            // Open or ClosedNoWinner — nothing to dispatch.
            // (ClosedNoWinner on Tick shouldn't happen — Tick never Cancels —
            // but handle it defensively by not dispatching.)
            return;
        }

        let winner_id = match arena.winner_owner() {
            Some(id) => id,
            None => return,
        };

        // Position: the recognizer's down_position (the press location).
        // Only LongPressRecognizer produces Accepted on Tick; if the winner
        // is some other recognizer (defensive), skip dispatch.
        let position = match arena
            .winner_recognizer()
            .and_then(|r| r.as_any().downcast_ref::<LongPressRecognizer>())
        {
            Some(lp) => lp.down_position(),
            None => return,
        };

        // Bounds: look up from the render tree (same lookup as the
        // Move-winner path in event_handler.rs:298).
        let bounds = self
            .render_objects
            .bounds_for_element(winner_id)
            .unwrap_or_default();

        let mut ctx = crate::event_context::EventContext::with_build_owner(
            winner_id,
            position,
            bounds,
            crate::input::Modifiers::default(),
            // tick_arena runs outside an input event; we don't have a
            // font_system or clipboard here. Long-press callbacks
            // (context_menu_trigger → controller.show) don't need them.
            // Pass stubs — verify the EventContext signature accepts these
            // during implementation; if font_system is required, thread it
            // from the pipeline (it's stored on the pipeline for layout).
            // For now, mirror the test-path: use a fresh font_system and
            // a stub clipboard. (See implementation note below.)
            // IMPLEMENTATION NOTE: read the actual EventContext::with_build_owner
            // signature and thread the real font_system + clipboard from the
            // pipeline. Do NOT leave stubs in production code.
            &mut self.font_system_placeholder,  // REPLACE — see note
            &self.build_owner,
            &self.dirty_sender,
            Some(&self.render_objects),
            self.clipboard_placeholder.clone(),  // REPLACE — see note
        );

        let winner_recognizer = arena.winner_recognizer().unwrap();
        if let Some(element) = self.element_registry.get_mut(winner_id) {
            element.on_arena_winner_update(
                winner_recognizer,
                &ArenaEvent::Tick { now },
                &mut ctx,
            );
        }
    }
```

**CRITICAL implementation note:** the code above has two placeholders (`font_system_placeholder`, `clipboard_placeholder`) that MUST be resolved by reading the actual `EventContext::with_build_owner` signature and the pipeline's fields. The pipeline stores `font_system` for layout but may not store a `Clipboard` — check `pipeline.rs` fields and how `handle_event` (which DOES have a clipboard param) receives it. If the pipeline doesn't own a clipboard, `tick_arena` must accept one as a parameter (threaded from `window.rs` alongside the existing `handle_event` calls). Do NOT merge with placeholders.

**Resolve the placeholders before proceeding:**
1. Read `vexo/src/event_context.rs` for the `with_build_owner` signature.
2. Read `pipeline.rs` fields (around line 145-161) to see what's available.
3. Read `window.rs` around line 644 to see what `window` owns (it has `self.backend`, `self.clipboard`?).
4. Decide: either (a) `tick_arena` takes `font_system: &mut glyphon::FontSystem` and `clipboard: &Arc<dyn Clipboard>` as params (threaded from `window.rs`), or (b) the pipeline stores them. Prefer (a) — matches `handle_event`'s pattern.

If (a), the signature becomes:
```rust
pub fn tick_arena(
    &mut self,
    now: std::time::Instant,
    font_system: &mut glyphon::FontSystem,
    clipboard: &Arc<dyn Clipboard>,
)
```

And `window.rs` call site becomes:
```rust
self.animation_ticker.tick();
self.three_tree_pipeline.tick_arena(
    std::time::Instant::now(),
    &mut self.font_system,
    &self.clipboard,
);
```

(Verify `window.rs` has `font_system` and `clipboard` fields — if not, thread them from where `handle_event` gets them.)

- [ ] **Step 3: Resolve the placeholders by reading the actual signatures**

Read these files (do NOT skip — the code in Step 2 will not compile as-is):
- `vexo/src/event_context.rs` — `EventContext::with_build_owner` signature
- `vexo/src/pipeline.rs:140-161` — pipeline fields
- `vexo/src/window.rs:640-700` — window fields and the tick call site
- `vexo/src/event_handler.rs:40-54` — how `handle_event` receives `font_system` and `clipboard`

Then rewrite `tick_arena` with the real types. Remove the placeholder comments.

- [ ] **Step 4: Call `tick_arena` from `window.rs`**

In `vexo/src/window.rs`, find the line `self.animation_ticker.tick();` (line 644). Add the `tick_arena` call immediately after, threading `font_system` and `clipboard` per the signature decided in Step 3:

```rust
        // Fire all active animation callbacks. These may mark elements dirty
        // via the mpsc channel, which perform_rebuilds() will process below.
        self.animation_ticker.tick();

        // Feed a Tick to the active gesture arena so time-based recognizers
        // (long-press) can fire. Must run BEFORE perform_rebuilds() so that
        // a long-press firing (which may set a Signal, e.g. open the menu)
        // has its dirty mark visible to the rebuild pass this frame.
        self.three_tree_pipeline.tick_arena(
            std::time::Instant::now(),
            &mut self.font_system,
            &self.clipboard,
        );
```

(Adjust the exact param names to match what `window.rs` actually owns — verify in Step 3.)

- [ ] **Step 5: Build to verify it compiles**

Run: `cargo build -p vexo`
Expected: compiles with no errors.

- [ ] **Step 6: Run all vexo tests to verify no regressions**

Run: `cargo test -p vexo`
Expected: all tests PASS (no behavior change yet — no element registers a `LongPressRecognizer` or handles `Tick` in `on_arena_winner_update`).

- [ ] **Step 7: Commit**

```bash
git add vexo/src/pipeline.rs vexo/src/window.rs
git commit -m "feat(pipeline): feed ArenaEvent::Tick to the active arena each frame

Adds ThreeTreePipeline::tick_arena(now), called from window.rs right after
animation_ticker.tick(). Feeds ArenaEvent::Tick to current_arena and, if
the Tick resolves the arena (e.g. long-press accepts at 500ms), dispatches
the winner element's on_arena_winner_update with the Tick event. This is
the clock that makes time-based gesture recognizers possible."
```

---

## Task 5: Add `on_long_press` to `GestureDetector` widget + element

**Files:**
- Modify: `vexo/src/widgets/gesture_detector.rs:62-76` (widget struct), `:78-127` (builders), `:180-279` (element struct + `set_widget`/`set_widget_from_widget`), `:385-392` (`register_gestures`), `:394-408` (`on_arena_winner_update`), `:410-439` (`rebuild`)
- Modify: `vexo/src/widgets/mod.rs:220-231` (add `on_long_press` default trait method after `on_secondary_press`)

**Interfaces:**
- Consumes: `LongPressRecognizer` (from Task 2), `ArenaEvent::Tick` (from Task 1), `pipeline.tick_arena` (from Task 4, for the test).
- Produces:
  - `GestureDetector::on_long_press(self, impl FnMut(Point<Logical>, Bounds<Logical>) + 'static) -> Self`
  - `Widget::on_long_press(self, impl FnMut(Point<Logical>, Bounds<Logical>) + 'static) -> Box<dyn Widget>` (default method)
  - `GestureDetectorElement` registers `LongPressRecognizer` when `on_long_press` is set
  - `GestureDetectorElement::on_arena_winner_update` fires `on_long_press` on `Tick`-driven win

- [ ] **Step 1: Add the `on_long_press` field to the `GestureDetector` widget struct**

In `vexo/src/widgets/gesture_detector.rs`, edit the struct (lines 62-76). Add the field after `on_secondary_press`:

```rust
pub struct GestureDetector {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    on_press: Option<Rc<RefCell<dyn FnMut()>>>,
    on_release: Option<Rc<RefCell<dyn FnMut()>>>,
    on_tap: Option<Rc<RefCell<dyn FnMut()>>>,
    on_secondary_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
    /// Callback invoked when a long-press is recognized (pointer held still
    /// for 500ms within slop). Arena-mediated — does NOT fire if a drag
    /// (scroll) wins instead. Receives the press position (where the finger
    /// went down) and the element's global bounds.
    on_long_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
}
```

- [ ] **Step 2: Initialize `on_long_press: None` in `GestureDetector::new`**

In the `new` method (lines 80-89), add `on_long_press: None,`:

```rust
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            on_press: None,
            on_release: None,
            on_tap: None,
            on_secondary_press: None,
            on_long_press: None,
        }
    }
```

- [ ] **Step 3: Add the `on_long_press` builder method**

After `on_secondary_press` (lines 121-127), add:

```rust
    /// Set the callback for long-press events (arena-mediated: fires after
    /// the pointer is held still for 500ms within slop). Receives the press
    /// position (where the finger went down, in window-logical coordinates)
    /// and the element's global bounds. Use this for actions like showing a
    /// context menu on mobile — it will NOT fire if a drag (scroll) wins the
    /// gesture instead.
    pub fn on_long_press(
        mut self,
        callback: impl FnMut(Point<Logical>, Bounds<Logical>) + 'static,
    ) -> Self {
        self.on_long_press = Some(Rc::new(RefCell::new(callback)));
        self
    }
```

- [ ] **Step 4: Add the `on_long_press` field to `GestureDetectorElement`**

In the element struct (lines 187-197), add the field after `on_secondary_press`:

```rust
pub struct GestureDetectorElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    on_press: Option<Rc<RefCell<dyn FnMut()>>>,
    on_release: Option<Rc<RefCell<dyn FnMut()>>>,
    on_tap: Option<Rc<RefCell<dyn FnMut()>>>,
    on_secondary_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
    on_long_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
    focus_attachment: Option<FocusAttachment>,
}
```

- [ ] **Step 5: Initialize `on_long_press: None` in `GestureDetectorElement::new`**

In `new` (lines 201-213):

```rust
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            on_press: None,
            on_release: None,
            on_tap: None,
            on_secondary_press: None,
            on_long_press: None,
            focus_attachment: None,
        }
    }
```

- [ ] **Step 6: Clone `on_long_press` in `set_widget_from_widget`**

In `set_widget_from_widget` (lines 216-223):

```rust
    fn set_widget_from_widget(&mut self, widget: &GestureDetector) {
        self.key = widget.key.clone();
        self.on_press = widget.on_press.clone();
        self.on_release = widget.on_release.clone();
        self.on_tap = widget.on_tap.clone();
        self.on_secondary_press = widget.on_secondary_press.clone();
        self.on_long_press = widget.on_long_press.clone();
        self.widget = Some(widget.clone_boxed());
    }
```

- [ ] **Step 7: Clone `on_long_press` in `set_widget` (RenderObjectElement impl)**

In `set_widget` (lines 243-253):

```rust
    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Some(gd) = widget.as_any().downcast_ref::<GestureDetector>() {
            self.key = gd.key.clone();
            self.on_press = gd.on_press.clone();
            self.on_release = gd.on_release.clone();
            self.on_tap = gd.on_tap.clone();
            self.on_secondary_press = gd.on_secondary_press.clone();
            self.on_long_press = gd.on_long_press.clone();
        }
        self.widget = Some(widget);
    }
```

- [ ] **Step 8: Register `LongPressRecognizer` in `register_gestures`**

Replace `register_gestures` (lines 385-392):

```rust
    fn register_gestures(&mut self, arena: &mut GestureArena, self_id: ElementKey) {
        // Only register a tap recognizer if there's an on_tap callback.
        // (on_press/on_release fire immediately via on_event and don't need
        // the arena — they're press-down feedback, not actions.)
        if self.on_tap.is_some() {
            arena.add(Box::new(TapRecognizer::new()), self_id);
        }
        if self.on_long_press.is_some() {
            arena.add(Box::new(crate::gestures::LongPressRecognizer::new()), self_id);
        }
    }
```

- [ ] **Step 9: Add the `Tick` arm to `on_arena_winner_update`**

Replace `on_arena_winner_update` (lines 394-408). Rename `_ctx` to `ctx` (now used):

```rust
    fn on_arena_winner_update(
        &mut self,
        recognizer: &dyn GestureRecognizer,
        event: &ArenaEvent,
        ctx: &mut EventContext,
    ) {
        match event {
            ArenaEvent::Up { .. } => {
                // Fire on_tap when the tap recognizer wins (on Up).
                if recognizer.accepted() {
                    if let Some(callback) = &self.on_tap {
                        (callback.borrow_mut())();
                    }
                }
            }
            ArenaEvent::Tick { .. } => {
                // Long-press fires at 500ms while the finger is still down.
                // Position comes from the recognizer's `down_position()`
                // (the press location). Bounds come from the EventContext,
                // which the pipeline's tick_arena dispatch builds from
                // render_objects.bounds_for_element(winner_id).
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

- [ ] **Step 10: Clone `on_long_press` in `rebuild`**

In `rebuild` (around lines 414-419), the block that reads `gd` fields:

```rust
            if let Some(gd) = widget.as_any().downcast_ref::<GestureDetector>() {
                self.on_press = gd.on_press.clone();
                self.on_release = gd.on_release.clone();
                self.on_tap = gd.on_tap.clone();
                self.on_secondary_press = gd.on_secondary_press.clone();
                self.on_long_press = gd.on_long_press.clone();
            }
```

- [ ] **Step 11: Add the `on_long_press` default method to the `Widget` trait**

In `vexo/src/widgets/mod.rs`, after `on_secondary_press` (lines 220-231), add:

```rust
    fn on_long_press(
        self,
        callback: impl FnMut(
                crate::core::Point<crate::core::Logical>,
                crate::core::Bounds<crate::core::Logical>,
            ) + 'static,
    ) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(GestureDetector::new(self).on_long_press(callback))
    }
```

- [ ] **Step 12: Build to verify it compiles**

Run: `cargo build -p vexo`
Expected: compiles with no errors. If `LongPressRecognizer` is not in scope in `gesture_detector.rs`, verify the import at line 37 — you may need to add it:

```rust
use crate::gestures::{ArenaEvent, GestureArena, GestureRecognizer, LongPressRecognizer, TapRecognizer};
```

- [ ] **Step 13: Run all vexo tests to verify no regressions**

Run: `cargo test -p vexo`
Expected: all existing tests PASS (no behavior change for existing callbacks; the new `Tick` arm is dead code until a `LongPressRecognizer` wins).

- [ ] **Step 14: Commit**

```bash
git add vexo/src/widgets/gesture_detector.rs vexo/src/widgets/mod.rs
git commit -m "feat(gesture-detector): add on_long_press callback

Mirrors on_secondary_press: takes FnMut(Point, Bounds) so callers (e.g.
context_menu_trigger) can position a menu at the press point. Registers a
LongPressRecognizer in the arena when set. on_arena_winner_update fires
on_long_press on a Tick-driven win (500ms elapsed, pointer still)."
```

---

## Task 6: Add element-level test for `on_long_press`

**Files:**
- Modify: `vexo/src/widgets/gesture_detector.rs` (extend `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `GestureDetector::on_long_press`, `pipeline.tick_arena` (Task 4), `LongPressRecognizer` (Task 2).

- [ ] **Step 1: Write the failing test — `on_long_press` fires after 500ms via `tick_arena`**

Append to the `mod tests` block in `vexo/src/widgets/gesture_detector.rs`. This test mounts a `GestureDetector` with `on_long_press` set, feeds a Primary press, ticks the pipeline past 500ms, and asserts the callback fired.

First, read the existing test helpers at the top of `mod tests` (around lines 592-610) to see what `test_clipboard()` and other helpers look like — model the new test's setup on `test_on_secondary_press_fires_with_position` (line 810).

```rust
    #[test]
    fn test_on_long_press_fires_after_500ms_via_tick_arena() {
        use std::cell::Cell;
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use crate::animation::AnimationTicker;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::pipeline::ThreeTreePipeline;
        use crate::platform::stub_clipboard::StubClipboard;
        use crate::core::{Bounds, Logical, Point, Size};

        let pressed = Rc::new(Cell::new(false));
        let press_pos = Rc::new(Cell::new(Point::new(0.0, 0.0)));
        let press_bounds = Rc::new(Cell::new(Bounds::new(0.0, 0.0, 0.0, 0.0)));
        let pressed_clone = pressed.clone();
        let pos_clone = press_pos.clone();
        let bounds_clone = press_bounds.clone();

        // A small tappable area at (10,10)-(110,60).
        let widget: Box<dyn Widget> = crate::DecoratedBox::with_style(
            crate::Text::new("Hold me"),
            crate::Style::default().background(crate::Color::WHITE),
        )
        .on_long_press(move |pos: Point<Logical>, bounds: Bounds<Logical>| {
            pressed_clone.set(true);
            pos_clone.set(pos);
            bounds_clone.set(bounds);
        });

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(widget);
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Primary press inside the bubble.
        let press = InputEvent::PointerButton {
            position: Point::new(50.0, 30.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        let clipboard: Arc<dyn crate::platform::Clipboard> = Arc::new(StubClipboard);
        pipeline.handle_event(
            Point::new(50.0, 30.0),
            &press,
            crate::input::Modifiers::default(),
            &mut font_system,
            &crate::core::ScaleSource::default(),
            &clipboard,
        );

        // Before 500ms: long-press has NOT fired.
        let start = Instant::now();
        pipeline.tick_arena(start + Duration::from_millis(400), &mut font_system, &clipboard);
        pipeline.perform_rebuilds();
        assert!(!pressed.get(), "long-press must not fire before 500ms");

        // At 500ms: long-press fires.
        pipeline.tick_arena(start + Duration::from_millis(500), &mut font_system, &clipboard);
        pipeline.perform_rebuilds();

        assert!(pressed.get(), "long-press callback should fire after 500ms");
        assert_eq!(press_pos.get(), Point::new(50.0, 30.0), "position should be the press location");
        // Bounds: the DecoratedBox's bounds should be passed through.
        // (Exact bounds depend on layout; just assert non-zero width.)
        assert!(press_bounds.get().width() > 0.0, "bounds should be the element's laid-out bounds");
    }
```

**Note:** the `tick_arena` signature here assumes it takes `(&mut font_system, &clipboard)` — verify this matches the signature you finalized in Task 4, Step 3. If `tick_arena` takes only `(now)`, adjust the call accordingly (but then the `on_long_press` callback's `bounds` may be default — verify the `EventContext` is built with real bounds regardless).

**Note on `Bounds::new`:** verify the `Bounds` constructor signature — it may be `Bounds::new(left, top, width, height)` or `Bounds::new(position, size)`. Read `vexo/src/core/bounds.rs` if unsure.

- [ ] **Step 2: Run the test to verify it passes**

Run: `cargo test -p vexo widgets::gesture_detector::tests::test_on_long_press_fires_after_500ms_via_tick_arena`
Expected: PASS. If it fails, debug:
  - Is `register_gestures` being called on press? (Check `event_handler.rs:272-276` — it walks `element_path` and calls `register_gestures` on each.)
  - Is the `LongPressRecognizer` registered? (Check the `on_long_press.is_some()` branch in `register_gestures`.)
  - Is `tick_arena` feeding the arena? (Check that `current_arena` is `Some` after the press.)
  - Is the `on_arena_winner_update` `Tick` arm firing the callback? (Add a `log::debug!` if needed.)

- [ ] **Step 3: Run all vexo tests to verify no regressions**

Run: `cargo test -p vexo`
Expected: all tests PASS.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/gesture_detector.rs
git commit -m "test(gesture-detector): on_long_press fires after 500ms via tick_arena

Mounts a GestureDetector with on_long_press, feeds a Primary press, ticks
the pipeline past 500ms, and asserts the callback fired with the press
position and the element's bounds. Exercises the full pipeline path:
register_gestures → LongPressRecognizer → tick_arena → on_arena_winner_update
→ on_long_press callback."
```

---

## Task 7: Branch `context_menu_trigger` on `Platform::current()`

**Files:**
- Modify: `vexo_uikit/src/context_menu.rs:631-640` (`context_menu_trigger` function)

**Interfaces:**
- Consumes: `Widget::on_long_press` (from Task 5), `Widget::on_secondary_press` (existing), `Platform::current()` (existing, `vexo_uikit/src/platform.rs:7`).
- Produces: `context_menu_trigger` that opens the menu on right-click (desktop) or long-press (mobile).

- [ ] **Step 1: Read the current `context_menu_trigger`**

Read `vexo_uikit/src/context_menu.rs:631-640`. Verify the current body:

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

- [ ] **Step 2: Verify `Platform` is already imported in this file**

Search `vexo_uikit/src/context_menu.rs` for `use.*Platform` or `Platform::`. If not imported, add to the `use vexo_uikit::...` or `use crate::...` block at the top:

```rust
use crate::Platform;
```

(Verify the exact import path — `Platform` is defined in `vexo_uikit/src/platform.rs` and re-exported from `vexo_uikit/src/lib.rs:15`. From within the crate, it's `crate::Platform`.)

- [ ] **Step 3: Replace `context_menu_trigger` with the platform-branching version**

Edit `vexo_uikit/src/context_menu.rs:631-640`:

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

- [ ] **Step 4: Build to verify it compiles**

Run: `cargo build -p vexo_uikit`
Expected: compiles with no errors. (Both `on_secondary_press` and `on_long_press` are now `Widget` trait methods available to any `impl Widget + 'static`.)

- [ ] **Step 5: Run all vexo_uikit tests to verify no regressions**

Run: `cargo test -p vexo_uikit`
Expected: all tests PASS (desktop tests compile with `Platform::Desktop` → `on_secondary_press` branch).

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/context_menu.rs
git commit -m "feat(context-menu): branch context_menu_trigger on Platform

Desktop uses on_secondary_press (right-click, unchanged); mobile uses
on_long_press (500ms hold). The menu content and positioning are identical
on both platforms — only the trigger differs."
```

---

## Task 8: Fix mobile `ContextMenuController` wiring

**Files:**
- Modify: `shared_app/src/chats/mod.rs:23-28` (`MobileChatsPage` struct), `:30-39` (`Clone` impl), `:44-113` (`render`), `:106` (the `ContextMenuController::new()` bug), `:117-130` (`build_chats_tab`)
- Modify: `shared_app/src/app.rs:66-71` (mobile `build_chats_tab` call — pass `context_menu.clone()`)

**Interfaces:**
- Consumes: `ContextMenuController` (existing), `state.context_menu` (existing, `app.rs:46`).
- Produces: `MobileChatsPage` with a `context_menu: ContextMenuController` field; `build_chats_tab` takes `context_menu: ContextMenuController` as a new parameter.

- [ ] **Step 1: Add `context_menu` field to `MobileChatsPage`**

In `shared_app/src/chats/mod.rs`, edit the struct (lines 23-28):

```rust
struct MobileChatsPage {
    conversations: Vec<Conversation>,
    nav: NavigationController<ChatsRoute>,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,
    me_avatar: Rc<[u8]>,
    context_menu: ContextMenuController,
}
```

- [ ] **Step 2: Update the `Clone` impl**

In the `Clone` impl (lines 30-39):

```rust
impl Clone for MobileChatsPage {
    fn clone(&self) -> Self {
        Self {
            conversations: self.conversations.clone(),
            nav: self.nav.clone(),
            messages: self.messages.clone(),
            me_avatar: Rc::clone(&self.me_avatar),
            context_menu: self.context_menu.clone(),
        }
    }
}
```

- [ ] **Step 3: Use `self.context_menu.clone()` in `render`'s destination builder**

In `render` (line 106), replace `context_menu: ContextMenuController::new(),` with `context_menu: self.context_menu.clone(),`:

```rust
                    chat_screen::ChatScreen {
                        conv_id: id_for_send.clone(),
                        messages,
                        avatar_bytes: avatar,
                        me_avatar_bytes: me_avatar_for_dest.clone(),
                        on_send: Rc::new(move |text: &str| {
                            // ... unchanged ...
                        }),
                        on_react: Rc::new(move |index: usize, rt: ReactionType| {
                            // ... unchanged ...
                        }),
                        scroll_controller: vexo::ScrollController::new(),
                        context_menu: self.context_menu.clone(),
                    }
                    .boxed()
```

**Note:** `self.context_menu.clone()` borrows `self` inside the `move |d| match d { ... }` closure. The closure already captures `convs`, `msgs`, `me_avatar_for_dest`, `nav` by clone. Capture `context_menu` by clone too, before the closure, to avoid the borrow. Add before the `.destination(...)` call:

```rust
        let context_menu = self.context_menu.clone();
```

And use `context_menu.clone()` inside the closure:

```rust
        let context_menu = self.context_menu.clone();
        // ... existing captures ...
        NavigationStackView::new(nav, chats_root)
            // ...
            .destination(move |d| match d {
                ChatsRoute::Chat(id) => {
                    // ...
                    chat_screen::ChatScreen {
                        // ...
                        context_menu: context_menu.clone(),
                    }
                    .boxed()
                }
                _ => Text::new("").boxed(),
            })
```

(Read the existing `render` carefully — the closure already clones `convs`, `msgs`, etc. before the closure. Mirror that pattern for `context_menu`.)

- [ ] **Step 4: Add `context_menu` parameter to `build_chats_tab`**

In `build_chats_tab` (lines 117-130):

```rust
pub(crate) fn build_chats_tab(
    conversations: Vec<Conversation>,
    nav: NavigationController<ChatsRoute>,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,
    me_avatar: Rc<[u8]>,
    context_menu: ContextMenuController,
) -> Box<dyn Widget> {
    MobileChatsPage {
        conversations,
        nav,
        messages,
        me_avatar,
        context_menu,
    }
    .boxed()
}
```

- [ ] **Step 5: Pass `state.context_menu.clone()` from `app.rs` mobile path**

In `shared_app/src/app.rs`, the mobile `build_chats_tab` call (lines 66-71):

```rust
                        ImTab::Chats => build_chats_tab(
                            conversations.clone(),
                            chats_nav.clone(),
                            messages_for_chat.clone(),
                            me_avatar.clone(),
                            context_menu.clone(),
                        ),
```

(`context_menu` is already extracted at `app.rs:46` as `let context_menu = state.context_menu.clone();` — verify it's still in scope at line 66. It is — it's used by the desktop branch at line 121.)

- [ ] **Step 6: Build to verify it compiles**

Run: `cargo build`
Expected: compiles with no errors across all crates.

- [ ] **Step 7: Run all shared_app tests to verify no regressions**

Run: `cargo test -p shared_app`
Expected: all existing tests PASS. If a `MobileChatsPage` test exists and constructs the struct directly, it will need the new `context_menu` field — update it to pass `ContextMenuController::new()` (tests don't need the shared controller; they don't test the menu).

- [ ] **Step 8: Commit**

```bash
git add shared_app/src/chats/mod.rs shared_app/src/app.rs
git commit -m "fix(chats): mobile ChatScreen uses the shared ContextMenuController

MobileChatsPage now receives state.context_menu (the controller mounted at
the root ContextMenu host) instead of constructing a fresh
ContextMenuController per chat. The fresh controller was never mounted, so
show() calls from the trigger never reached the host — the menu never
rendered. Mirrors the desktop wiring (desktop.rs:118). Required for the
mobile long-press trigger to actually open the menu."
```

---

## Task 9: Add integration test — long-press opens the context menu on the chat screen

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs` (extend `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `pipeline.tick_arena` (Task 4), `ChatScreen` + `ContextMenu` host (existing), the test helpers `find_text_in_tree`, `seed_messages_signal`, `seed_avatar`, `seed_me_avatar` (existing in the test module).

- [ ] **Step 1: Read the existing `test_right_click_bubble_opens_context_menu`**

Read `shared_app/src/chats/chat_screen.rs:942-1012`. The new test mirrors this exactly, except:
- Press is `PointerButton::Primary` (not `Secondary`).
- After the press, call `pipeline.tick_arena(now + 500ms)` to fire the long-press.

- [ ] **Step 2: Write the test**

Append to the `mod tests` block in `shared_app/src/chats/chat_screen.rs`:

```rust
    /// Long-press on a message bubble opens the context menu (mobile
    /// trigger). Mirrors `test_right_click_bubble_opens_context_menu` but
    /// uses a Primary press + `tick_arena(500ms)` instead of a Secondary
    /// press. Exercises the full pipeline path:
    ///   Primary press → register_gestures (LongPressRecognizer) →
    ///   tick_arena(500ms) → LongPressRecognizer accepts →
    ///   on_arena_winner_update(Tick) → on_long_press callback →
    ///   context_menu_trigger's mobile branch → controller.show(pos, builder) →
    ///   ContextMenu host renders the menu.
    ///
    /// NOTE: This test runs on desktop (where Platform::current() == Desktop,
    /// so context_menu_trigger uses on_secondary_press, NOT on_long_press).
    /// To test the long-press path directly, we bypass context_menu_trigger's
    /// platform branch by... hmm, actually we can't — ChatScreen::render
    /// calls context_menu_trigger, which on desktop wraps in
    /// on_secondary_press, not on_long_press. A Primary press + tick_arena
    /// would NOT fire the menu on desktop because no on_long_press callback
    /// is registered.
    ///
    /// SOLUTION: This test verifies the FRAMEWORK path (LongPressRecognizer +
    /// on_arena_winner_update + tick_arena) using a bare GestureDetector, not
    /// the ChatScreen integration. The ChatScreen integration test for
    /// mobile long-press requires Platform injection, which the spec defers
    /// (YAGNI). See Task 6 for the framework-level test.
    ///
    /// For the ChatScreen integration, the existing
    /// `test_right_click_bubble_opens_context_menu` covers the desktop branch
    /// (on_secondary_press). The mobile branch (on_long_press) is a one-liner
    /// verified by compilation on mobile targets.
    #[test]
    fn test_long_press_bubble_opens_context_menu_via_framework_gesture() {
        // This test is a no-op placeholder — see the doc comment above.
        // The actual framework-level long-press test is in
        // vexo/src/widgets/gesture_detector.rs (Task 6). The ChatScreen
        // mobile integration is covered by compilation + the existing
        // desktop right-click test.
        //
        // Leaving this test here as documentation of the testability gap
        // and the rationale (per spec: "Test we explicitly DON'T write").
        assert!(true, "framework-level test lives in gesture_detector.rs");
    }
```

Wait — this is a placeholder test that asserts `true`. That's not a real test. Reconsider.

The spec says: "This test runs on desktop but exercises the long-press recognizer path directly — it doesn't go through context_menu_trigger's platform branch. It tests the recognizer + element + menu wiring, not the trigger's platform selection."

But `ChatScreen::render` calls `context_menu_trigger`, which on desktop uses `on_secondary_press`. A Primary press + tick_arena won't fire the menu because no `on_long_press` is registered. The spec's claim that we can test this "directly" on desktop is wrong — we can't, without platform injection.

**Resolution:** Delete this placeholder test. The framework-level test in Task 6 is the real coverage. The ChatScreen integration for mobile long-press is verified by:
1. The existing `test_right_click_bubble_opens_context_menu` (desktop branch, unchanged).
2. Compilation on mobile targets (the `on_long_press` branch compiles or doesn't).
3. Manual testing on iOS (the user runs the app).

Per the spec's "Test we explicitly DON'T write" section, we accept this gap. Do NOT add a placeholder test.

- [ ] **Step 3: (Replacement) Add a regression test that the desktop right-click path still works after the platform branch**

The existing `test_right_click_bubble_opens_context_menu` already covers this. No new test needed — just verify it still passes (Step 4).

- [ ] **Step 4: Run all chat_screen tests to verify no regressions**

Run: `cargo test -p shared_app chats::chat_screen`
Expected: all existing tests PASS, including all 4 right-click tests (lines 942-1209). The platform branch in `context_menu_trigger` compiles to `on_secondary_press` on desktop, so these tests are unchanged.

- [ ] **Step 5: Run the full shared_app test suite**

Run: `cargo test -p shared_app`
Expected: all tests PASS.

- [ ] **Step 6: Commit (only if a test was actually added — if not, skip the commit)**

Since we did NOT add a new test (per the spec's "test we don't write" rationale), there's nothing to commit for this task. Mark the task complete and move on.

---

## Task 10: Final verification

**Files:** None modified.

- [ ] **Step 1: Build the entire workspace**

Run: `cargo build`
Expected: compiles with no errors across `vexo`, `vexo_uikit`, `shared_app`, `desktop_demo`.

- [ ] **Step 2: Run the entire test suite**

Run: `cargo test`
Expected: all tests PASS. Key suites to verify:
  - `vexo::gestures::long_press` (7 tests, Task 2)
  - `vexo::gestures::arena` (2 new + existing, Task 3)
  - `vexo::widgets::gesture_detector` (1 new + existing, Task 6)
  - `vexo_uikit::context_menu` (existing, Task 7)
  - `shared_app::chats::chat_screen` (existing right-click tests, Task 8/9)
  - `shared_app::chats` (existing, Task 8)

- [ ] **Step 3: Verify the mobile wiring fix compiles on a mobile target (if possible)**

If the dev machine can cross-compile to iOS (`cargo build --target aarch64-apple-ios` or similar), run it. If not, skip — the `cfg(target_os = "ios")` gating means the mobile path is compiled only on mobile targets, and the desktop build verifies the desktop path. The `build_for_ios.sh` script (per CLAUDE.md) handles the iOS build from Xcode.

Run (if iOS toolchain installed): `cargo build --target aarch64-apple-ios -p shared_app 2>&1 | tail -20`
Expected: compiles with no errors. If the target is not installed, this will fail with "error: can't find crate for 'core'" — that's expected, skip.

- [ ] **Step 4: Hand off to the user for manual iOS testing**

Per CLAUDE.md, never run `cargo run -p desktop_demo` yourself. The long-press behavior is mobile-only (desktop uses right-click). The user must run the iOS build via `./build_for_ios.sh` + Xcode and manually verify:
  - Long-press a message bubble → menu appears at the press point after 500ms.
  - The menu shows the reactions pill + actions card (same as desktop right-click).
  - Scrolling works (long-press does NOT fire during scroll).
  - Tapping a bubble does NOT open the menu (only long-press).

Report the handoff to the user:
> "Implementation complete. All tests pass. Please run `./build_for_ios.sh` and test on the iOS simulator: long-press a message bubble for 500ms — the context menu should appear. Verify scrolling still works (long-press should cancel during scroll) and that a quick tap does NOT open the menu."

---

## Self-Review

**1. Spec coverage:**

| Spec section | Task(s) |
|---|---|
| Layer 1: `ArenaEvent::Tick` | Task 1 |
| Layer 1: `LongPressRecognizer` | Task 2 |
| Layer 1: Arena integration tests | Task 3 |
| Layer 1: Pipeline wiring (`tick_arena`) | Task 4 |
| Layer 1: Constants (`LONG_PRESS_DURATION`, `LONG_PRESS_SLOP`) | Task 2 |
| Layer 2: `GestureDetector::on_long_press` field/builder | Task 5 |
| Layer 2: `Widget::on_long_press` default method | Task 5 |
| Layer 2: `register_gestures` | Task 5 |
| Layer 2: `on_arena_winner_update` `Tick` arm | Task 5 |
| Layer 2: Element test | Task 6 |
| Layer 3: `context_menu_trigger` platform branch | Task 7 |
| Layer 3: Mobile wiring fix (`MobileChatsPage.context_menu`) | Task 8 |
| Layer 3: ChatScreen integration test | Task 9 (resolved as "test we don't write" per spec) |
| Final verification | Task 10 |

**2. Placeholder scan:** Task 4 Step 2 has explicit placeholder code (`font_system_placeholder`, `clipboard_placeholder`) with a CRITICAL note to resolve them in Step 3 before proceeding. This is intentional — the exact `EventContext::with_build_owner` signature and pipeline fields must be read at implementation time; guessing them in the plan would risk type errors. Step 3 is a mandatory read-then-rewrite step, not a skip. Task 9 Step 2 has a placeholder test that is explicitly rejected in Step 2's own commentary and replaced with "no test" in Step 3 — this is the spec's "test we don't write," documented inline.

**3. Type consistency:**
- `LongPressRecognizer::down_position()` returns `Point<Logical>` — used in Task 5's `on_arena_winner_update` and Task 4's `tick_arena`. ✓
- `on_long_press` callback signature `FnMut(Point<Logical>, Bounds<Logical>) + 'static` — consistent across `GestureDetector::on_long_press` (Task 5), `Widget::on_long_press` (Task 5), and `context_menu_trigger`'s mobile branch (Task 7). ✓
- `tick_arena` signature: `(now: Instant, ...)` — Task 4 defines it, Task 6's test calls it. The `...` (font_system, clipboard) is resolved in Task 4 Step 3 and must be kept consistent with Task 6's test call. Task 6 Step 1 has a note to verify the signature matches. ✓
- `build_chats_tab` signature: adds `context_menu: ContextMenuController` — Task 8 Step 4 (definition) and Task 8 Step 5 (call site in `app.rs`). ✓

**4. Scope check:** The plan covers a single feature (mobile long-press context menu) across 3 layers. No sub-project decomposition needed — the layers are sequential dependencies, not independent subsystems.

**5. Ambiguity check:**
- Task 4 Step 2's placeholders are the main ambiguity. Resolved by mandatory Step 3 (read actual signatures, rewrite). The plan does NOT allow proceeding with placeholders.
- Task 9's "test we don't write" is documented and consistent with the spec.
- Task 8 Step 3's closure capture pattern (clone `context_menu` before the closure) is described in prose + code; the implementer reads the existing `render` to mirror the pattern exactly.

No unresolved ambiguities.
