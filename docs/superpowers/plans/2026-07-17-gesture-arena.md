# Gesture Arena: Tap vs. Scroll-Drag Disambiguation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Disambiguate tap vs. vertical-drag (scroll) using a per-pointer `GestureArena`, so a drag on a tappable row inside a `ScrollView` scrolls instead of triggering the row's tap action.

**Architecture:** A new `vexo/src/gestures/` module holds a pure `GestureArena` resolver and two `GestureRecognizer` structs (`TapRecognizer`, `VerticalDragRecognizer`). On pointer press, `EventHandler` creates an arena and offers every element in the hit-test path a chance to register a recognizer. Subsequent move/up events route through the arena, which resolves a winner via a slop-threshold rule (18px) and notifies the owning element to fire its callback. The arena is dropped on release.

**Tech Stack:** Rust, `vexo` workspace crate, `cargo test` for verification.

**Spec:** `docs/superpowers/specs/2026-07-17-gesture-arena-design.md`

## Global Constraints

- Slop constants: `TAP_SLOP = 18.0`, `VERTICAL_DRAG_SLOP = 18.0` (both `pub(crate)` in `vexo/src/gestures/mod.rs`).
- Single-pointer only: `InputEvent` carries no pointer id. One arena at a time, stored as `Option<GestureArena>` on the pipeline. Created on press, dropped on release.
- Recognizers are pure state machines: they expose `resolution()` and never call arena methods or hold user callbacks. Callbacks live on elements.
- `on_press` remains immediate press-down feedback (fires via the normal `on_event` bubble on press, regardless of arena outcome). `on_tap` is the arena-mediated action callback (fires on release-after-win).
- Mouse-wheel `Scroll` events and `Keyboard` events keep their current dispatch paths — they do NOT enter the arena.
- Run `cargo build -p vexo` after every Rust edit, `cargo test -p vexo` after every feature. Never assume tests pass without running them.
- Never run `cargo run -p desktop_demo` yourself — ask the user.

---

## File Structure

New files (pure logic, no element/pipeline deps):
- `vexo/src/gestures/mod.rs` — module root, re-exports, slop constants
- `vexo/src/gestures/arena_event.rs` — `ArenaEvent` enum
- `vexo/src/gestures/recognizer.rs` — `RecognizerResolution`, `ArenaContext`, `GestureRecognizer` trait
- `vexo/src/gestures/tap.rs` — `TapRecognizer`
- `vexo/src/gestures/vertical_drag.rs` — `VerticalDragRecognizer`
- `vexo/src/gestures/arena.rs` — `GestureArena`, `ArenaOutcome`

Modified files (wiring):
- `vexo/src/lib.rs` — declare `pub mod gestures;`
- `vexo/src/element.rs` — add `register_gestures` + `on_arena_winner_update` default methods
- `vexo/src/pipeline.rs` — `current_arena: Option<GestureArena>` field + `cancel_current_gesture()`
- `vexo/src/event_handler.rs` — arena creation on press, registration walk, move/up routing, cancel
- `vexo/src/widgets/gesture_detector.rs` — `on_tap` field + builder; `register_gestures`; `on_arena_winner_update`
- `vexo/src/widgets/mod.rs` — `WidgetExt::on_tap`
- `vexo/src/elements/scroll_view.rs` — `register_gestures`; `on_arena_winner_update`; remove drag branches from `on_event`; remove `drag_active`/`drag_last_y`
- `vexo/src/window.rs` — call `cancel_current_gesture()` on window unfocus
- `shared_app/src/chats/conversation_list.rs:75` — `.on_press` → `.on_tap`
- `vexo_uikit/src/button.rs` — rename action API `on_press` → `on_tap`; split visual feedback from action in `render`
- `shared_app/src/chats/chat_screen.rs:176` — `.on_press(on_send)` → `.on_tap(on_send)`
- `vexo_uikit/tests/button_tests.rs:32,45` — `.on_press` → `.on_tap`

---

### Task 1: gestures module scaffolding (ArenaEvent, RecognizerResolution, ArenaContext, GestureRecognizer trait)

**Files:**
- Create: `vexo/src/gestures/mod.rs`
- Create: `vexo/src/gestures/arena_event.rs`
- Create: `vexo/src/gestures/recognizer.rs`
- Modify: `vexo/src/lib.rs` (add `pub mod gestures;` near line 22, after `pub mod input;`)

**Interfaces:**
- Produces: `ArenaEvent`, `RecognizerResolution`, `ArenaContext`, `GestureRecognizer` trait — consumed by Tasks 2-4.

- [ ] **Step 1: Create `arena_event.rs`**

```rust
//! Arena events fed to gesture recognizers by the GestureArena.

use crate::core::Point;
use crate::core::Logical;

/// An event delivered to every recognizer in the arena.
#[derive(Clone, Copy, Debug)]
pub enum ArenaEvent {
    Down { position: Point<Logical> },
    Move { position: Point<Logical> },
    Up { position: Point<Logical> },
    Cancel,
}
```

- [ ] **Step 2: Create `recognizer.rs`**

```rust
//! GestureRecognizer trait and supporting types.

use std::any::Any;

use crate::core::{Logical, Point};

use super::arena_event::ArenaEvent;

/// Outcome of a recognizer's internal state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecognizerResolution {
    /// Still competing — no decision yet.
    Pending,
    /// This recognizer has claimed the gesture (won).
    Accepted,
    /// This recognizer has given up (lost or cancelled).
    Rejected,
}

/// Shared facts computed once by the arena and handed to each recognizer.
///
/// Recognizers track their own accumulated state (e.g. `total_delta_y`)
/// internally; this struct only carries the per-event shared facts.
#[derive(Clone, Copy, Debug)]
pub struct ArenaContext {
    pub down_position: Point<Logical>,
    pub current_position: Point<Logical>,
}

/// A self-contained gesture state machine.
///
/// Recognizers never call arena methods and never hold user callbacks.
/// The arena reads `resolution()` to decide a winner; the owning element
/// holds the callback and fires it when the arena resolves.
pub trait GestureRecognizer: Any {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &ArenaContext);
    fn resolution(&self) -> RecognizerResolution;

    fn accepted(&self) -> bool {
        matches!(self.resolution(), RecognizerResolution::Accepted)
    }
    fn rejected(&self) -> bool {
        matches!(self.resolution(), RecognizerResolution::Rejected)
    }
}
```

- [ ] **Step 3: Create `mod.rs`**

```rust
//! Gesture arena: disambiguates competing gesture recognizers for a pointer.
//!
//! Currently implements Tap vs. VerticalDrag (scroll) disambiguation via a
//! slop-threshold rule, matching Flutter's GestureArena behavior for this
//! recognizer pair.

pub mod arena;
pub mod arena_event;
pub mod recognizer;
pub mod tap;
pub mod vertical_drag;

pub use arena::{ArenaOutcome, GestureArena};
pub use arena_event::ArenaEvent;
pub use recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
pub use tap::TapRecognizer;
pub use vertical_drag::VerticalDragRecognizer;

/// Movement threshold (in logical pixels) beyond which a tap is rejected.
/// Matches Flutter's `kTouchSlop`.
pub(crate) const TAP_SLOP: f32 = 18.0;

/// Cumulative vertical movement threshold beyond which a vertical drag is
/// recognized. Matches Flutter's vertical drag slop.
pub(crate) const VERTICAL_DRAG_SLOP: f32 = 18.0;
```

- [ ] **Step 4: Add module declaration to `lib.rs`**

In `vexo/src/lib.rs`, after the line `pub mod input;` (line 22), add:

```rust
pub mod gestures;
```

- [ ] **Step 5: Create placeholder files so the module compiles**

Create `vexo/src/gestures/tap.rs`:
```rust
//! TapRecognizer — recognizes a tap (down + up without slop breach).
```

Create `vexo/src/gestures/vertical_drag.rs`:
```rust
//! VerticalDragRecognizer — recognizes a vertical drag (cumulative y past slop).
```

Create `vexo/src/gestures/arena.rs`:
```rust
//! GestureArena — per-pointer resolver for competing recognizers.
```

- [ ] **Step 6: Build to verify module wiring**

Run: `cargo build -p vexo`
Expected: compiles with no errors (the `pub use` of empty modules is fine since the types don't exist yet — actually this WILL fail because `mod.rs` re-exports types that don't exist).

Fix: temporarily comment out the `pub use` lines in `mod.rs` that reference not-yet-created types, keeping only the module declarations and constants:

```rust
pub mod arena;
pub mod arena_event;
pub mod recognizer;
pub mod tap;
pub mod vertical_drag;

pub use arena_event::ArenaEvent;
pub use recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};

pub(crate) const TAP_SLOP: f32 = 18.0;
pub(crate) const VERTICAL_DRAG_SLOP: f32 = 18.0;
```

(Leave out `pub use arena::{...}`, `pub use tap::...`, `pub use vertical_drag::...` until those types exist — they'll be added in Tasks 2-4.)

Run: `cargo build -p vexo`
Expected: compiles clean.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/gestures/ vexo/src/lib.rs
git commit -m "feat(gestures): scaffold gestures module with ArenaEvent and GestureRecognizer trait"
```

---

### Task 2: TapRecognizer (TDD)

**Files:**
- Create: `vexo/src/gestures/tap.rs`
- Test: inline `#[cfg(test)] mod tests` in `tap.rs`

**Interfaces:**
- Consumes: `ArenaEvent`, `ArenaContext`, `GestureRecognizer`, `RecognizerResolution` (Task 1), `TAP_SLOP` (Task 1)
- Produces: `TapRecognizer` struct — consumed by Task 4 (arena) and Task 8 (GestureDetectorElement registration)

- [ ] **Step 1: Write the failing tests**

Replace the placeholder content of `vexo/src/gestures/tap.rs` with:

```rust
//! TapRecognizer — recognizes a tap (down + up without slop breach).
//!
//! State transitions on ArenaEvent:
//! - Down  → store position, stay Pending
//! - Move  → if |Δx| or |Δy| from down exceeds TAP_SLOP → Rejected, else Pending
//! - Up    → if still Pending → Accepted (tap wins)
//! - Cancel → Rejected

use crate::core::Logical;
use crate::core::Point;

use super::arena_event::ArenaEvent;
use super::recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
use super::TAP_SLOP;

pub struct TapRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
}

impl TapRecognizer {
    pub fn new() -> Self {
        Self {
            resolution: RecognizerResolution::Pending,
            down_position: Point::zero(),
        }
    }
}

impl Default for TapRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for TapRecognizer {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &ArenaContext) {
        if self.rejected() {
            return;
        }
        match event {
            ArenaEvent::Down { .. } => {
                self.down_position = ctx.down_position;
            }
            ArenaEvent::Move { .. } => {
                let dx = (ctx.current_position.x - self.down_position.x).abs();
                let dy = (ctx.current_position.y - self.down_position.y).abs();
                if dx > TAP_SLOP || dy > TAP_SLOP {
                    self.resolution = RecognizerResolution::Rejected;
                }
            }
            ArenaEvent::Up { .. } => {
                if self.resolution == RecognizerResolution::Pending {
                    self.resolution = RecognizerResolution::Accepted;
                }
            }
            ArenaEvent::Cancel => {
                self.resolution = RecognizerResolution::Rejected;
            }
        }
    }

    fn resolution(&self) -> RecognizerResolution {
        self.resolution
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
    fn tap_accepts_on_up_after_down_no_move() {
        let mut r = TapRecognizer::new();
        let p = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: p }, ctx(p, p));
        r.handle_event(&ArenaEvent::Up { position: p }, ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
    }

    #[test]
    fn tap_rejects_on_move_past_slop_vertical() {
        let mut r = TapRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(50.0, 80.0); // Δy = 30 > 18
        r.handle_event(&ArenaEvent::Down { position: down }, ctx(down, down));
        r.handle_event(&ArenaEvent::Move { position: moved }, ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn tap_rejects_on_move_past_slop_horizontal() {
        let mut r = TapRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(80.0, 50.0); // Δx = 30 > 18
        r.handle_event(&ArenaEvent::Down { position: down }, ctx(down, down));
        r.handle_event(&ArenaEvent::Move { position: moved }, ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn tap_stays_pending_on_move_within_slop() {
        let mut r = TapRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(55.0, 60.0); // Δx=5, Δy=10, both < 18
        r.handle_event(&ArenaEvent::Down { position: down }, ctx(down, down));
        r.handle_event(&ArenaEvent::Move { position: moved }, ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }

    #[test]
    fn tap_rejects_on_cancel() {
        let mut r = TapRecognizer::new();
        let p = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: p }, ctx(p, p));
        r.handle_event(&ArenaEvent::Cancel, ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn tap_rejects_on_up_after_slop_breach() {
        let mut r = TapRecognizer::new();
        let down = Point::new(50.0, 50.0);
        let moved = Point::new(50.0, 80.0);
        r.handle_event(&ArenaEvent::Down { position: down }, ctx(down, down));
        r.handle_event(&ArenaEvent::Move { position: moved }, ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
        r.handle_event(&ArenaEvent::Up { position: moved }, ctx(down, moved));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vexo gestures::tap`
Expected: all 6 tests PASS (the implementation is already in the file above).

- [ ] **Step 3: Add `pub use tap::TapRecognizer;` to `mod.rs`**

In `vexo/src/gestures/mod.rs`, add to the `pub use` block:
```rust
pub use tap::TapRecognizer;
```

- [ ] **Step 4: Build the full crate**

Run: `cargo build -p vexo`
Expected: compiles clean.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/gestures/tap.rs vexo/src/gestures/mod.rs
git commit -m "feat(gestures): add TapRecognizer with slop-based rejection"
```

---

### Task 3: VerticalDragRecognizer (TDD)

**Files:**
- Create: `vexo/src/gestures/vertical_drag.rs`
- Test: inline `#[cfg(test)] mod tests` in `vertical_drag.rs`

**Interfaces:**
- Consumes: `ArenaEvent`, `ArenaContext`, `GestureRecognizer`, `RecognizerResolution` (Task 1), `VERTICAL_DRAG_SLOP` (Task 1)
- Produces: `VerticalDragRecognizer` struct with `last_position()` and `total_delta_y()` accessors — consumed by Task 4 (arena) and Task 9 (ScrollViewElement reads position to apply scroll)

- [ ] **Step 1: Write the failing tests + implementation**

Replace the placeholder content of `vexo/src/gestures/vertical_drag.rs` with:

```rust
//! VerticalDragRecognizer — recognizes a vertical drag (cumulative Δy past slop).
//!
//! State transitions on ArenaEvent:
//! - Down  → store positions, total_delta_y = 0, stay Pending
//! - Move  → accumulate total_delta_y += delta.y; if |total| > VERTICAL_DRAG_SLOP → Accepted
//! - Up    → if Pending → Rejected (was a tap); if Accepted → stays Accepted
//! - Cancel → Rejected
//!
//! Uses CUMULATIVE delta (sum of per-move deltas), not net displacement, so
//! back-and-forth jitter still counts as drag intent. Matches Flutter's
//! VerticalDragGestureRecognizer.

use crate::core::{Logical, Point};

use super::arena_event::ArenaEvent;
use super::recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
use super::VERTICAL_DRAG_SLOP;

pub struct VerticalDragRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
    last_position: Point<Logical>,
    total_delta_y: f32,
}

impl VerticalDragRecognizer {
    pub fn new() -> Self {
        Self {
            resolution: RecognizerResolution::Pending,
            down_position: Point::zero(),
            last_position: Point::zero(),
            total_delta_y: 0.0,
        }
    }

    /// Last pointer position seen — read by ScrollViewElement to compute the
    /// per-move scroll delta.
    pub fn last_position(&self) -> Point<Logical> {
        self.last_position
    }

    /// Cumulative vertical movement since down. Read by the element for
    /// diagnostics; scroll deltas are computed from `last_position` deltas.
    pub fn total_delta_y(&self) -> f32 {
        self.total_delta_y
    }

    pub fn down_position(&self) -> Point<Logical> {
        self.down_position
    }
}

impl Default for VerticalDragRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for VerticalDragRecognizer {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &ArenaContext) {
        if self.rejected() {
            return;
        }
        match event {
            ArenaEvent::Down { .. } => {
                self.down_position = ctx.down_position;
                self.last_position = ctx.down_position;
                self.total_delta_y = 0.0;
            }
            ArenaEvent::Move { .. } => {
                let delta_y = ctx.current_position.y - self.last_position.y;
                self.last_position = ctx.current_position;
                self.total_delta_y += delta_y;
                if self.resolution == RecognizerResolution::Pending
                    && self.total_delta_y.abs() > VERTICAL_DRAG_SLOP
                {
                    self.resolution = RecognizerResolution::Accepted;
                }
            }
            ArenaEvent::Up { .. } => {
                if self.resolution == RecognizerResolution::Pending {
                    self.resolution = RecognizerResolution::Rejected;
                }
                // If already Accepted, stays Accepted (drag completed).
            }
            ArenaEvent::Cancel => {
                self.resolution = RecognizerResolution::Rejected;
            }
        }
    }

    fn resolution(&self) -> RecognizerResolution {
        self.resolution
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
    fn drag_accepts_on_cumulative_move_past_slop() {
        let mut r = VerticalDragRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 60.0),
            },
            ctx(down, Point::new(50.0, 60.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 70.0),
            },
            ctx(down, Point::new(50.0, 70.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
    }

    #[test]
    fn drag_stays_pending_on_single_small_move() {
        let mut r = VerticalDragRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 60.0),
            },
            ctx(down, Point::new(50.0, 60.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }

    #[test]
    fn drag_rejects_on_up_without_slop() {
        let mut r = VerticalDragRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, ctx(down, down));
        r.handle_event(&ArenaEvent::Up { position: down }, ctx(down, down));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn drag_stays_accepted_after_slop() {
        let mut r = VerticalDragRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 80.0),
            },
            ctx(down, Point::new(50.0, 80.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 75.0),
            },
            ctx(down, Point::new(50.0, 75.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
    }

    #[test]
    fn drag_rejects_on_cancel() {
        let mut r = VerticalDragRecognizer::new();
        let p = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: p }, ctx(p, p));
        r.handle_event(&ArenaEvent::Cancel, ctx(p, p));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn drag_cumulative_back_and_forth_still_breaches() {
        let mut r = VerticalDragRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, ctx(down, down));
        // +15
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 65.0),
            },
            ctx(down, Point::new(50.0, 65.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
        // -15 (back to start, net 0, but cumulative 30)
        r.handle_event(
            &ArenaEvent::Move {
                position: Point::new(50.0, 50.0),
            },
            ctx(down, Point::new(50.0, 50.0)),
        );
        assert_eq!(
            r.resolution(),
            RecognizerResolution::Accepted,
            "cumulative 30 > 18 slop, even though net displacement is 0"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vexo gestures::vertical_drag`
Expected: all 6 tests PASS.

- [ ] **Step 3: Add `pub use` to `mod.rs`**

In `vexo/src/gestures/mod.rs`, add:
```rust
pub use vertical_drag::VerticalDragRecognizer;
```

- [ ] **Step 4: Build the full crate**

Run: `cargo build -p vexo`
Expected: compiles clean.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/gestures/vertical_drag.rs vexo/src/gestures/mod.rs
git commit -m "feat(gestures): add VerticalDragRecognizer with cumulative-delta slop"
```

---

### Task 4: GestureArena (TDD)

**Files:**
- Create: `vexo/src/gestures/arena.rs`
- Test: inline `#[cfg(test)] mod tests` in `arena.rs`

**Interfaces:**
- Consumes: `ArenaEvent`, `ArenaContext`, `GestureRecognizer`, `TapRecognizer` (Task 2), `VerticalDragRecognizer` (Task 3), `ElementKey` (from `crate::id`)
- Produces: `GestureArena`, `ArenaOutcome` — consumed by Task 6 (pipeline) and Task 10 (event_handler)

- [ ] **Step 1: Write the failing tests + implementation**

Replace the placeholder content of `vexo/src/gestures/arena.rs` with:

```rust
//! GestureArena — per-pointer resolver for competing gesture recognizers.
//!
//! One arena per active pointer press. Elements register recognizers into it
//! on press; the arena feeds ArenaEvents to every recognizer and resolves a
//! single winner via the slop model:
//!   - Any recognizer that returns Accepted on Move → wins immediately,
//!     all others are rejected, arena closes.
//!   - On Up, if still open: any recognizer that accepts on Up wins;
//!     otherwise sweep to the first non-rejected recognizer (Flutter default).
//!   - On Cancel: arena closes with no winner.
//!
//! The arena is pure: it does NOT fire user callbacks. EventHandler reads the
//! winner and notifies the owning element.

use crate::core::{Logical, Point};
use crate::id::ElementKey;

use super::arena_event::ArenaEvent;
use super::recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};

/// Result of feeding an event to the arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaOutcome {
    /// A recognizer accepted; `winner_index` is its position in the arena.
    Resolved { winner_index: usize },
    /// Arena was cancelled (no winner).
    ClosedNoWinner,
    /// Still competing — no decision yet.
    Open,
}

struct ArenaEntry {
    recognizer: Box<dyn GestureRecognizer>,
    owner: ElementKey,
}

pub struct GestureArena {
    entries: Vec<ArenaEntry>,
    down_position: Point<Logical>,
    winner: Option<usize>,
    closed: bool,
}

impl GestureArena {
    pub fn new(down_position: Point<Logical>) -> Self {
        Self {
            entries: Vec::new(),
            down_position,
            winner: None,
            closed: false,
        }
    }

    /// Register a recognizer with its owning element. No-op if the arena is
    /// already closed (single-winner invariant).
    pub fn add(&mut self, recognizer: Box<dyn GestureRecognizer>, owner: ElementKey) {
        if self.closed {
            return;
        }
        self.entries.push(ArenaEntry { recognizer, owner });
    }

    /// Number of registered recognizers.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The winning recognizer, if resolved.
    pub fn winner_recognizer(&self) -> Option<&dyn GestureRecognizer> {
        self.winner.map(|i| self.entries[i].recognizer.as_ref())
    }

    /// The owning element of the winning recognizer, if resolved.
    pub fn winner_owner(&self) -> Option<ElementKey> {
        self.winner.map(|i| self.entries[i].owner)
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Feed an event to every recognizer, then resolve.
    pub fn handle_event(&mut self, event: ArenaEvent) -> ArenaOutcome {
        if self.closed {
            // Already resolved; a closed arena stays closed with its winner.
            return match self.winner {
                Some(i) => ArenaOutcome::Resolved { winner_index: i },
                None => ArenaOutcome::ClosedNoWinner,
            };
        }

        let current_position = match &event {
            ArenaEvent::Down { position } => *position,
            ArenaEvent::Move { position } => *position,
            ArenaEvent::Up { position } => *position,
            ArenaEvent::Cancel => self.down_position,
        };
        let ctx = ArenaContext {
            down_position: self.down_position,
            current_position,
        };

        // Feed event to every recognizer.
        for entry in &mut self.entries {
            entry.recognizer.handle_event(&event, &ctx);
        }

        match event {
            ArenaEvent::Cancel => {
                self.closed = true;
                self.winner = None;
                ArenaOutcome::ClosedNoWinner
            }
            ArenaEvent::Move { .. } | ArenaEvent::Up { .. } => {
                self.try_resolve();
                match self.winner {
                    Some(i) => ArenaOutcome::Resolved { winner_index: i },
                    None if self.closed => ArenaOutcome::ClosedNoWinner,
                    None => ArenaOutcome::Open,
                }
            }
            ArenaEvent::Down { .. } => ArenaOutcome::Open,
        }
    }

    /// Resolution sweep: if any recognizer accepted → it wins, reject others.
    /// If on Up and none accepted but some are pending → sweep to first
    /// non-rejected (Flutter default sweep).
    fn try_resolve(&mut self) {
        if self.closed {
            return;
        }
        // First pass: look for an Accepted recognizer.
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.recognizer.accepted() {
                self.declare_winner(i);
                return;
            }
        }
        // If we reach here on an Up event, sweep to first non-rejected.
        // (Called after feeding Up, so rejections are up-to-date.)
    }

    /// Called when the arena should attempt a sweep (e.g. on Up with no
    /// accepted recognizer). Public so EventHandler can trigger a sweep
    /// after feeding Up.
    pub fn sweep_on_up(&mut self) {
        if self.closed {
            return;
        }
        // First: any accepted?
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.recognizer.accepted() {
                self.declare_winner(i);
                return;
            }
        }
        // Sweep to first non-rejected (Pending). Rejected ones are skipped.
        for (i, entry) in self.entries.iter().enumerate() {
            if !entry.recognizer.rejected() {
                self.declare_winner(i);
                return;
            }
        }
        // All rejected — no winner.
        self.closed = true;
        self.winner = None;
    }

    fn declare_winner(&mut self, index: usize) {
        self.winner = Some(index);
        self.closed = true;
        // Reject all others (they've lost).
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if i != index && !entry.recognizer.rejected() {
                // Feed Cancel to losers so they clean up.
                let ctx = ArenaContext {
                    down_position: self.down_position,
                    current_position: self.down_position,
                };
                entry.recognizer.handle_event(&ArenaEvent::Cancel, &ctx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gestures::{TapRecognizer, VerticalDragRecognizer};

    fn dummy_element_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    fn arena_with_tap_and_drag() -> GestureArena {
        let mut arena = GestureArena::new(Point::new(50.0, 50.0));
        arena.add(Box::new(TapRecognizer::new()), dummy_element_key());
        arena.add(
            Box::new(VerticalDragRecognizer::new()),
            dummy_element_key(),
        );
        arena
    }

    #[test]
    fn arena_resolves_drag_winner_on_slop_breach() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        let outcome = arena.handle_event(ArenaEvent::Move {
            position: Point::new(50.0, 80.0), // Δy = 30 > 18
        });
        assert_eq!(outcome, ArenaOutcome::Resolved { winner_index: 1 });
        assert!(arena.winner_recognizer().unwrap().accepted());
    }

    #[test]
    fn arena_resolves_tap_winner_on_release_before_slop() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        arena.handle_event(ArenaEvent::Up { position: down });
        arena.sweep_on_up();
        assert!(arena.is_closed());
        assert_eq!(arena.winner, Some(0)); // tap at index 0
    }

    #[test]
    fn arena_open_during_small_move() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        let outcome = arena.handle_event(ArenaEvent::Move {
            position: Point::new(50.0, 55.0), // Δy = 5 < 18
        });
        assert_eq!(outcome, ArenaOutcome::Open);
    }

    #[test]
    fn arena_closed_no_winner_on_cancel() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        let outcome = arena.handle_event(ArenaEvent::Cancel);
        assert_eq!(outcome, ArenaOutcome::ClosedNoWinner);
        assert!(!arena.winner.is_some());
    }

    #[test]
    fn arena_single_recipient_sweeps_on_up() {
        let mut arena = GestureArena::new(Point::new(50.0, 50.0));
        arena.add(Box::new(TapRecognizer::new()), dummy_element_key());
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        arena.handle_event(ArenaEvent::Up { position: down });
        arena.sweep_on_up();
        assert_eq!(arena.winner, Some(0));
    }

    #[test]
    fn arena_deepest_wins_on_tie() {
        // Deepest (index 0) is the inner drag; outer (index 1) is the outer drag.
        let mut arena = GestureArena::new(Point::new(50.0, 50.0));
        arena.add(
            Box::new(VerticalDragRecognizer::new()),
            dummy_element_key(),
        );
        arena.add(
            Box::new(VerticalDragRecognizer::new()),
            dummy_element_key(),
        );
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        arena.handle_event(ArenaEvent::Move {
            position: Point::new(50.0, 80.0),
        });
        assert_eq!(arena.winner, Some(0), "deepest (index 0) wins the tie");
    }

    #[test]
    fn arena_add_noop_after_closed() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        arena.handle_event(ArenaEvent::Cancel);
        let len_before = arena.len();
        arena.add(Box::new(TapRecognizer::new()), dummy_element_key());
        assert_eq!(arena.len(), len_before, "add is no-op on closed arena");
    }

    #[test]
    fn arena_no_second_winner_after_closed() {
        let mut arena = arena_with_tap_and_drag();
        let down = Point::new(50.0, 50.0);
        arena.handle_event(ArenaEvent::Down { position: down });
        arena.handle_event(ArenaEvent::Move {
            position: Point::new(50.0, 80.0),
        });
        // Drag won (index 1). Feed Up — winner must stay index 1.
        let outcome = arena.handle_event(ArenaEvent::Up {
            position: Point::new(50.0, 80.0),
        });
        assert_eq!(outcome, ArenaOutcome::Resolved { winner_index: 1 });
        assert_eq!(arena.winner, Some(1));
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p vexo gestures::arena`
Expected: all 8 tests PASS.

- [ ] **Step 3: Add `pub use` to `mod.rs`**

In `vexo/src/gestures/mod.rs`, add:
```rust
pub use arena::{ArenaOutcome, GestureArena};
```

- [ ] **Step 4: Build the full crate**

Run: `cargo build -p vexo`
Expected: compiles clean.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/gestures/arena.rs vexo/src/gestures/mod.rs
git commit -m "feat(gestures): add GestureArena resolver with slop-based winner declaration"
```

---

### Task 5: Element trait extensions (register_gestures, on_arena_winner_update)

**Files:**
- Modify: `vexo/src/element.rs` (add two default methods to the `Element` trait)

**Interfaces:**
- Consumes: `GestureArena`, `ArenaEvent`, `GestureRecognizer` (Tasks 1-4), `ElementKey`, `EventContext`
- Produces: `Element::register_gestures()` and `Element::on_arena_winner_update()` default methods — consumed by Tasks 8, 9 (GestureDetectorElement, ScrollViewElement overrides)

- [ ] **Step 1: Add the two default methods to the `Element` trait**

In `vexo/src/element.rs`, add these imports near the top (after line 11):

```rust
use crate::gestures::{ArenaEvent, GestureArena, GestureRecognizer};
```

Then add these two methods to the `Element` trait, right before the closing `}` of the trait (after `fn focus_attachment_mut`, line 84):

```rust
    /// Register gesture recognizers into the arena for this pointer press.
    ///
    /// Called once on pointer press for every element in the hit-test path
    /// (deepest first). Default: no-op. Override to add recognizers.
    fn register_gestures(&mut self, _arena: &mut GestureArena, _self_id: ElementKey) {}

    /// Called on each subsequent Move/Up event **only for the winning element**.
    ///
    /// The element downcasts the recognizer to read its state and apply
    /// effects (e.g. ScrollView reads the drag recognizer's position delta).
    /// Default: no-op.
    fn on_arena_winner_update(
        &mut self,
        _recognizer: &dyn GestureRecognizer,
        _event: &ArenaEvent,
        _ctx: &mut super::EventContext,
    ) {
    }
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p vexo`
Expected: compiles clean (the new methods have default impls, so all existing implementors are unaffected).

- [ ] **Step 3: Commit**

```bash
git add vexo/src/element.rs
git commit -m "feat(element): add register_gestures and on_arena_winner_update default methods"
```

---

### Task 6: Pipeline current_arena field + cancel_current_gesture

**Files:**
- Modify: `vexo/src/pipeline.rs` (add field + method + thread through handle_event)

**Interfaces:**
- Consumes: `GestureArena` (Task 4)
- Produces: `ThreeTreePipeline::current_arena` field (pub(crate)), `ThreeTreePipeline::cancel_current_gesture()` — consumed by Task 10 (event_handler) and Task 13 (window.rs)

- [ ] **Step 1: Add the field to `ThreeTreePipeline`**

In `vexo/src/pipeline.rs`, add the import near the top imports (after line 60 area):

```rust
use crate::gestures::GestureArena;
```

Add a field to the `ThreeTreePipeline` struct (after `inherited_maps` field, line 155):

```rust
    /// Per-pointer gesture arena. Created on press, dropped on release.
    /// Single-pointer only (InputEvent has no pointer id).
    pub(crate) current_arena: Option<GestureArena>,
```

Initialize it in `new()` (after `inherited_maps: SecondaryMap::new(),`, line 178):

```rust
            current_arena: None,
```

- [ ] **Step 2: Add `cancel_current_gesture` method**

Add this method to `impl ThreeTreePipeline` (after `handle_event`, around line 553):

```rust
    /// Cancel any active gesture arena (e.g. on window unfocus).
    ///
    /// Feeds Cancel to the arena (all recognizers reject, no winner fires),
    /// then drops it. Safe to call when no arena is active (no-op).
    pub fn cancel_current_gesture(&mut self) {
        if let Some(mut arena) = self.current_arena.take() {
            arena.handle_event(crate::gestures::ArenaEvent::Cancel);
        }
    }
```

- [ ] **Step 3: Build to verify**

Run: `cargo build -p vexo`
Expected: compiles clean.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/pipeline.rs
git commit -m "feat(pipeline): add current_arena field and cancel_current_gesture"
```

---

### Task 7: GestureDetector on_tap field + builder + WidgetExt::on_tap

**Files:**
- Modify: `vexo/src/widgets/gesture_detector.rs` (add `on_tap` field + builder)
- Modify: `vexo/src/widgets/mod.rs` (add `WidgetExt::on_tap`)

**Interfaces:**
- Consumes: nothing new
- Produces: `GestureDetector::on_tap()` builder, `GestureDetector.on_tap` field, `WidgetExt::on_tap()` — consumed by Task 8 (element fires on_tap) and Task 11 (conversation_list migration)

- [ ] **Step 1: Add `on_tap` field to `GestureDetector` struct**

In `vexo/src/widgets/gesture_detector.rs`, add a field to the `GestureDetector` struct (after `on_release`, line 66):

```rust
    /// Callback invoked when a tap is recognized (pointer up, having won the
    /// arena). Arena-mediated — does NOT fire if a drag wins instead.
    on_tap: Option<Rc<RefCell<dyn FnMut()>>>,
```

Initialize it in `new()` (after `on_release: None,`, line 79):

```rust
            on_tap: None,
```

Add it to `Clone` impl (after `on_release: self.on_release.clone(),`, line 129):

```rust
            on_tap: self.on_tap.clone(),
```

- [ ] **Step 2: Add the `on_tap` builder method**

Add this method to `impl GestureDetector` (after `on_release`, around line 113):

```rust
    /// Set the callback for tap events (arena-mediated: fires on pointer-up
    /// after winning the arena). Use this for actions like navigation — it
    /// will NOT fire if a drag (scroll) wins the gesture instead.
    pub fn on_tap(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_tap = Some(Rc::new(RefCell::new(callback)));
        self
    }
```

- [ ] **Step 3: Add `on_tap` field to `GestureDetectorElement`**

Add field to `GestureDetectorElement` struct (after `on_release`, line 180):

```rust
    on_tap: Option<Rc<RefCell<dyn FnMut()>>>,
```

Initialize in `new()` (after `on_release: None,`, line 193):

```rust
            on_tap: None,
```

Update `set_widget_from_widget` (after `self.on_release = widget.on_release.clone();`, line 202):

```rust
        self.on_tap = widget.on_tap.clone();
```

Update `set_widget` (in the `if let Some(gd)` block, after `self.on_release = gd.on_release.clone();`, line 229):

```rust
            self.on_tap = gd.on_tap.clone();
```

Update `rebuild` (in the `if let Some(gd)` block, after `self.on_release = gd.on_release.clone();`, line 348):

```rust
                self.on_tap = gd.on_tap.clone();
```

- [ ] **Step 4: Add `WidgetExt::on_tap`**

In `vexo/src/widgets/mod.rs`, add this method to the `WidgetExt` trait (after `on_release`, around line 310):

```rust
    fn on_tap(self, callback: impl FnMut() + 'static) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(GestureDetector::new(self).on_tap(callback))
    }
```

- [ ] **Step 5: Build to verify**

Run: `cargo build -p vexo`
Expected: compiles clean.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/gesture_detector.rs vexo/src/widgets/mod.rs
git commit -m "feat(gesture_detector): add on_tap builder and WidgetExt::on_tap"
```

---

### Task 8: GestureDetectorElement register_gestures + on_arena_winner_update

**Files:**
- Modify: `vexo/src/widgets/gesture_detector.rs` (override the two new Element methods)

**Interfaces:**
- Consumes: `GestureArena`, `TapRecognizer` (Task 2), `ElementKey`, `ArenaEvent`, `GestureRecognizer`, `EventContext`
- Produces: `GestureDetectorElement` registers a `TapRecognizer` on press and fires `on_tap` when the tap wins — consumed by Task 10 (event_handler calls these)

- [ ] **Step 1: Add imports to gesture_detector.rs**

At the top of `vexo/src/widgets/gesture_detector.rs`, add:

```rust
use crate::gestures::{ArenaEvent, GestureArena, GestureRecognizer, TapRecognizer};
```

- [ ] **Step 2: Override `register_gestures` on `GestureDetectorElement`**

Add this impl to `impl Element for GestureDetectorElement` (after `on_event`, before `rebuild`, around line 341):

```rust
    fn register_gestures(&mut self, arena: &mut GestureArena, self_id: ElementKey) {
        // Only register a tap recognizer if there's an on_tap callback.
        // (on_press/on_release fire immediately via on_event and don't need
        // the arena — they're press-down feedback, not actions.)
        if self.on_tap.is_some() {
            arena.add(Box::new(TapRecognizer::new()), self_id);
        }
    }

    fn on_arena_winner_update(
        &mut self,
        recognizer: &dyn GestureRecognizer,
        event: &ArenaEvent,
        _ctx: &mut EventContext,
    ) {
        // Fire on_tap when the tap recognizer wins (on Up).
        if let ArenaEvent::Up { .. } = event {
            if recognizer.accepted() {
                if let Some(callback) = &self.on_tap {
                    (callback.borrow_mut())();
                }
            }
        }
    }
```

- [ ] **Step 3: Build to verify**

Run: `cargo build -p vexo`
Expected: compiles clean.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/gesture_detector.rs
git commit -m "feat(gesture_detector): register TapRecognizer and fire on_tap on arena win"
```

---

### Task 9: ScrollViewElement register_gestures + on_arena_winner_update + remove old drag branches

**Files:**
- Modify: `vexo/src/elements/scroll_view.rs`

**Interfaces:**
- Consumes: `GestureArena`, `VerticalDragRecognizer` (Task 3), `ArenaEvent`, `GestureRecognizer`, `EventContext`, `ScrollViewRenderObject`
- Produces: `ScrollViewElement` registers a `VerticalDragRecognizer` and applies scroll deltas on drag win — consumed by Task 10

- [ ] **Step 1: Add imports**

At the top of `vexo/src/elements/scroll_view.rs`, add:

```rust
use crate::gestures::{ArenaEvent, GestureArena, GestureRecognizer, VerticalDragRecognizer};
```

- [ ] **Step 2: Remove the old drag fields from `ScrollViewElement`**

Remove these fields from the struct (lines 44-45):
```rust
    drag_active: bool,
    drag_last_y: f32,
```

Remove their initialization from `new()` (lines 60-61):
```rust
            drag_active: false,
            drag_last_y: 0.0,
```

- [ ] **Step 3: Add `last_drag_y` field to track position across move events**

Add this field to `ScrollViewElement` (where `drag_active` was):

```rust
    /// Tracks the last y position from the drag recognizer, to compute
    /// per-move scroll deltas. Set when the drag recognizer wins.
    last_drag_y: f32,
```

Initialize in `new()`:
```rust
            last_drag_y: 0.0,
```

- [ ] **Step 4: Remove drag branches from `on_event`**

Replace the entire `on_event` method body (lines 225-292) with this version that keeps only `Scroll` (wheel) and `Keyboard` branches:

```rust
    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
        _state: &mut StateStorage,
    ) -> Option<Box<dyn Any>> {
        match event {
            InputEvent::Scroll { delta, .. } => {
                let new_offset = self.scroll_offset - delta.y;
                self.apply_scroll_offset(new_offset, context);
                return Some(Box::new(()));
            }

            InputEvent::Keyboard {
                key,
                state: ButtonState::Pressed,
                ..
            } => {
                let delta = match key {
                    Key::Named(NamedKey::ArrowUp) => Some(-LINE_HEIGHT),
                    Key::Named(NamedKey::ArrowDown) => Some(LINE_HEIGHT),
                    Key::Named(NamedKey::PageUp) => Some(-self.viewport_height),
                    Key::Named(NamedKey::PageDown) => Some(self.viewport_height),
                    Key::Named(NamedKey::Home) => Some(-self.scroll_offset),
                    Key::Named(NamedKey::End) => Some(self.max_scroll() - self.scroll_offset),
                    _ => None,
                };
                if let Some(d) = delta {
                    self.apply_scroll_offset(self.scroll_offset + d, context);
                    return Some(Box::new(()));
                }
            }

            _ => {}
        }
        None
    }
```

Note: the `PointerButton` and `PointerMoved` branches are GONE — the arena now handles drag. `on_press`/`on_release` no longer fire from scroll view (they were never used here anyway).

- [ ] **Step 5: Override `register_gestures`**

Add to `impl Element for ScrollViewElement` (after `on_event`):

```rust
    fn register_gestures(&mut self, arena: &mut GestureArena, self_id: ElementKey) {
        arena.add(Box::new(VerticalDragRecognizer::new()), self_id);
    }
```

- [ ] **Step 6: Override `on_arena_winner_update`**

Add to `impl Element for ScrollViewElement` (after `register_gestures`):

```rust
    fn on_arena_winner_update(
        &mut self,
        recognizer: &dyn GestureRecognizer,
        event: &ArenaEvent,
        ctx: &mut EventContext,
    ) {
        // Downcast to read the drag recognizer's position.
        let Some(drag) = recognizer
            .as_any()
            .downcast_ref::<VerticalDragRecognizer>()
        else {
            return;
        };

        match event {
            ArenaEvent::Move { .. } => {
                // Compute scroll delta from recognizer's last position.
                let delta = self.last_drag_y - drag.last_position().y;
                self.last_drag_y = drag.last_position().y;
                let new_offset = self.scroll_offset + delta;
                self.apply_scroll_offset(new_offset, ctx);
            }
            ArenaEvent::Down { .. } => {
                // Drag just won (on the move that crossed slop). Initialize
                // last_drag_y from the recognizer's current position so the
                // first delta is measured from here, not from the press-down.
                self.last_drag_y = drag.last_position().y;
            }
            ArenaEvent::Up { .. } => {
                // Drag ended. No scroll applied on up (no momentum in v1).
            }
            ArenaEvent::Cancel => {
                // Drag cancelled. No cleanup needed.
            }
        }
    }
```

Wait — there's a subtlety. `GestureRecognizer` has `as_any()` via the `Any` supertrait. But we need to expose `as_any()` on the trait. Let me check: the trait is `pub trait GestureRecognizer: Any`. To downcast, we need `fn as_any(&self) -> &dyn Any`. Add it to the trait.

- [ ] **Step 7: Add `as_any` to `GestureRecognizer` trait**

In `vexo/src/gestures/recognizer.rs`, add this method to the `GestureRecognizer` trait:

```rust
    fn as_any(&self) -> &dyn Any where Self: Sized {
        self
    }
```

Actually, this won't work for `&dyn GestureRecognizer` because `Sized` bound prevents object-safety. Use the standard pattern instead:

```rust
    fn as_any(&self) -> &dyn Any;
```

And implement it in each recognizer. Add to `TapRecognizer` impl:
```rust
    fn as_any(&self) -> &dyn Any {
        self
    }
```

Add to `VerticalDragRecognizer` impl:
```rust
    fn as_any(&self) -> &dyn Any {
        self
    }
```

- [ ] **Step 8: Build to verify**

Run: `cargo build -p vexo`
Expected: compiles clean. Fix any errors (the `drag_active`/`drag_last_y` removal may have leftover references in tests — update them).

- [ ] **Step 9: Update existing tests that reference removed fields**

In `vexo/src/elements/scroll_view.rs`, the tests at lines 461 (`test_touch_drag_scrolls_via_pipeline`) and 591 (`test_touch_drag_clamps_at_top`) use the OLD drag path (pointer press → pointer moved). These will now fail because the arena intermediates. They will be REPLACED by Task 11's integration tests. For now, comment them out (they'll be replaced):

Add `#[ignore]` to `test_touch_drag_scrolls_via_pipeline` and `test_touch_drag_clamps_at_top`, with a comment:

```rust
    #[ignore = "replaced by arena-based tests in Task 11 — old direct-drag path removed"]
    #[test]
    fn test_touch_drag_scrolls_via_pipeline() { ... }
```

Do the same for `test_touch_drag_clamps_at_top`.

Keep `test_mouse_wheel_still_works` UN-ignored (wheel path is unchanged).

Run: `cargo test -p vexo --lib scroll_view`
Expected: `test_mouse_wheel_still_works` passes; the two drag tests are ignored.

- [ ] **Step 10: Commit**

```bash
git add vexo/src/elements/scroll_view.rs vexo/src/gestures/recognizer.rs vexo/src/gestures/tap.rs vexo/src/gestures/vertical_drag.rs
git commit -m "feat(scroll_view): register VerticalDragRecognizer, apply scroll on arena win, remove old drag branches"
```

---

### Task 10: EventHandler wiring — arena creation, registration walk, move/up routing

**Files:**
- Modify: `vexo/src/event_handler.rs`
- Modify: `vexo/src/pipeline.rs` (thread `current_arena` into `handle_event`)

**Interfaces:**
- Consumes: `GestureArena`, `ArenaEvent`, `ArenaOutcome` (Task 4), `Element::register_gestures`/`on_arena_winner_update` (Task 5), `ThreeTreePipeline::current_arena` (Task 6)
- Produces: the complete event flow that makes the arena work end-to-end

This is the central wiring task. It modifies `handle_pointer_event` to:
1. On press: create arena, walk hit path deepest→shallowest calling `register_gestures`, feed `Down`.
2. On move: feed `Move` to arena; if drag won → call winner's `on_arena_winner_update` (no bubble); if open → bubble (for MouseRegion hover).
3. On release: feed `Up` + `sweep_on_up`; if tap won → call winner's `on_arena_winner_update` + bubble release (for `on_release`); if drag won → call winner's update (no bubble); drop arena.

- [ ] **Step 1: Change `EventHandler::handle_event` signature to accept the arena**

In `vexo/src/event_handler.rs`, add the import:

```rust
use crate::gestures::{ArenaEvent, ArenaOutcome, GestureArena};
```

Change `handle_event` to accept `current_arena: &mut Option<GestureArena>` and pass it through to `handle_pointer_event`. Update the signature (line 39):

```rust
    pub fn handle_event(
        element_registry: &mut ElementRegistry,
        render_objects: &RenderObjectRegistry,
        state: &mut StateStorage,
        font_system: &mut glyphon::FontSystem,
        build_owner: &BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        current_arena: &mut Option<GestureArena>,
        _position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
        scale_source: &ScaleSource,
        clipboard: &Arc<dyn Clipboard>,
    ) -> Option<Box<dyn Any>> {
```

Pass `current_arena` to `handle_pointer_event` in both `PointerMoved` and `PointerButton` arms (lines 54-81):

```rust
            InputEvent::PointerMoved { position } => Self::handle_pointer_event(
                element_registry,
                render_objects,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focus_manager,
                current_arena,
                *position,
                event,
                modifiers,
                scale_source,
                clipboard,
            ),
            InputEvent::PointerButton { position, .. } => Self::handle_pointer_event(
                element_registry,
                render_objects,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focus_manager,
                current_arena,
                *position,
                event,
                modifiers,
                scale_source,
                clipboard,
            ),
```

The `Scroll` and `Keyboard` arms do NOT receive `current_arena` (unchanged).

- [ ] **Step 2: Rewrite `handle_pointer_event` with arena logic**

Replace the entire `handle_pointer_event` method (lines 123-222) with:

```rust
    pub(crate) fn handle_pointer_event(
        element_registry: &mut ElementRegistry,
        render_objects: &RenderObjectRegistry,
        state: &mut StateStorage,
        font_system: &mut glyphon::FontSystem,
        build_owner: &BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        current_arena: &mut Option<GestureArena>,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
        scale_source: &ScaleSource,
        clipboard: &Arc<dyn Clipboard>,
    ) -> Option<Box<dyn Any>> {
        let absolute_position = Position::<Logical, Absolute>::new(position.x, position.y);
        let hit_result = render_objects.hit_test(absolute_position);

        if !hit_result.is_hit() {
            if let InputEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } = event
            {
                focus_manager.unfocus();
            }
            return None;
        }

        let local_position = hit_result
            .inner_bounds()
            .map(|b| Point::new(position.x - b.position().x, position.y - b.position().y))
            .unwrap_or(position);

        let element_path = hit_result.element_path();

        // Determine if this is a press, move, or release.
        let is_press = matches!(
            event,
            InputEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            }
        );
        let is_release = matches!(
            event,
            InputEvent::PointerButton {
                state: ButtonState::Released,
                ..
            }
        );
        let is_move = matches!(event, InputEvent::PointerMoved { .. });

        // === PRESS: create arena, register gestures, feed Down, then bubble press ===
        if is_press {
            // Defensive: if a stale arena exists (e.g. window blurred mid-press),
            // drop it and start fresh.
            *current_arena = Some(GestureArena::new(position));

            if let Some(arena) = current_arena.as_mut() {
                // Walk deepest→shallowest so deepest recognizer is at index 0.
                for &element_id in element_path.iter().rev() {
                    if let Some(element) = element_registry.get_mut(element_id) {
                        element.register_gestures(arena, element_id);
                    }
                }
                // Feed Down.
                arena.handle_event(ArenaEvent::Down { position });
            }
        }

        // === MOVE: feed Move to arena; if drag won, call winner (no bubble);
        //     if still open, bubble for MouseRegion hover ===
        if is_move {
            if let Some(arena) = current_arena.as_mut() {
                let outcome = arena.handle_event(ArenaEvent::Move { position });
                if let ArenaOutcome::Resolved { winner_index: _ } = outcome {
                    if let Some(winner_id) = arena.winner_owner() {
                        let bounds = hit_result
                            .bounds_for_element(winner_id)
                            .unwrap_or_default();
                        if let Some(element) = element_registry.get_mut(winner_id) {
                            let mut ctx = EventContext::with_build_owner(
                                winner_id,
                                position,
                                local_position,
                                focus_manager.primary_focus_element(),
                                bounds,
                                modifiers,
                                scale_source.clone(),
                                font_system,
                                build_owner,
                                dirty_sender,
                                Some(render_objects),
                                clipboard.clone(),
                            );
                            let winner_recognizer = arena.winner_recognizer().unwrap();
                            element.on_arena_winner_update(
                                winner_recognizer,
                                &ArenaEvent::Move { position },
                                &mut ctx,
                            );
                        }
                        // Drag owns the pointer — do NOT bubble.
                        return Some(Box::new(()));
                    }
                }
                // Arena still open — fall through to bubble (MouseRegion hover).
            }
        }

        // === RELEASE: feed Up + sweep; if tap won, call winner + bubble release;
        //     if drag won, call winner (no bubble); drop arena ===
        if is_release {
            let mut drag_won = false;
            if let Some(arena) = current_arena.as_mut() {
                arena.handle_event(ArenaEvent::Up { position });
                arena.sweep_on_up();
                if let Some(winner_id) = arena.winner_owner() {
                    let bounds = hit_result
                        .bounds_for_element(winner_id)
                        .unwrap_or_default();
                    // Check if the winner is a drag (not a tap) — drag consumes release.
                    let is_drag_winner = arena
                        .winner_recognizer()
                        .map(|r| {
                            r.as_any()
                                .downcast_ref::<crate::gestures::VerticalDragRecognizer>()
                                .is_some()
                        })
                        .unwrap_or(false);
                    drag_won = is_drag_winner;

                    if let Some(element) = element_registry.get_mut(winner_id) {
                        let mut ctx = EventContext::with_build_owner(
                            winner_id,
                            position,
                            local_position,
                            focus_manager.primary_focus_element(),
                            bounds,
                            modifiers,
                            scale_source.clone(),
                            font_system,
                            build_owner,
                            dirty_sender,
                            Some(render_objects),
                            clipboard.clone(),
                        );
                        let winner_recognizer = arena.winner_recognizer().unwrap();
                        element.on_arena_winner_update(
                            winner_recognizer,
                            &ArenaEvent::Up { position },
                            &mut ctx,
                        );
                    }
                }
            }
            // Drop the arena — gesture sequence complete.
            *current_arena = None;

            if drag_won {
                // Drag consumed the release — do NOT bubble (on_release won't fire).
                return Some(Box::new(()));
            }
            // Tap won (or no arena) — fall through to bubble release so
            // on_release fires (release feedback).
        }

        // === BUBBLE: deepest→shallowest, first handler stops propagation ===
        let mut any_message: Option<Box<dyn Any>> = None;
        for &element_id in element_path.iter().rev() {
            if let Some(element) = element_registry.get_mut(element_id) {
                let bounds = hit_result
                    .bounds_for_element(element_id)
                    .unwrap_or_default();
                let mut ctx = EventContext::with_build_owner(
                    element_id,
                    position,
                    local_position,
                    focus_manager.primary_focus_element(),
                    bounds,
                    modifiers,
                    scale_source.clone(),
                    font_system,
                    build_owner,
                    dirty_sender,
                    Some(render_objects),
                    clipboard.clone(),
                );
                let message = element.on_event(event, &mut ctx, state);
                if let Some(focus_element) = ctx.focus_request() {
                    let node_id = focus_manager
                        .node_for_element(focus_element)
                        .expect("Focus node must exist");
                    focus_manager.request_focus(node_id);
                } else if ctx.should_clear_focus() {
                    focus_manager.unfocus();
                }
                if message.is_some() {
                    any_message = message;
                    break;
                }
            }
        }

        if any_message.is_none() {
            if is_press {
                focus_manager.unfocus();
            }
        }

        any_message
    }
```

- [ ] **Step 3: Update `pipeline.rs` `handle_event` to pass `current_arena`**

In `vexo/src/pipeline.rs`, update `handle_event` (line 525) to pass `&mut self.current_arena`:

Change the `EventHandler::handle_event(...)` call (line 534) to add the new parameter after `&mut self.focus_manager,`:

```rust
        let result = EventHandler::handle_event(
            &mut self.element_registry,
            &self.render_objects,
            &mut self.state,
            font_system,
            &self.build_owner,
            &self.dirty_sender,
            &mut self.focus_manager,
            &mut self.current_arena,
            position,
            event,
            modifiers,
            scale_source,
            clipboard,
        );
```

- [ ] **Step 4: Build to verify**

Run: `cargo build -p vexo`
Expected: compiles clean. Fix any signature mismatches.

- [ ] **Step 5: Run existing tests to check for regressions**

Run: `cargo test -p vexo --lib`
Expected: recognizer + arena tests pass; scroll tests that were `#[ignore]`d are skipped; `test_mouse_wheel_still_works` passes. Some tests that rely on the old press-bubble behavior may fail — note them and fix in Task 11.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/event_handler.rs vexo/src/pipeline.rs
git commit -m "feat(event_handler): route pointer events through gesture arena (press/move/up)"
```

---

### Task 11: Migrate conversation_list to on_tap + write the bug-fix integration tests

**Files:**
- Modify: `shared_app/src/chats/conversation_list.rs:75` (`.on_press` → `.on_tap`)
- Test: `vexo/src/elements/scroll_view.rs` (add arena integration tests, replace the `#[ignore]`d ones)

**Interfaces:**
- Consumes: `WidgetExt::on_tap` (Task 7), arena event flow (Task 10)
- Produces: the bug fix (drag scrolls, not navigates) + validation tests

- [ ] **Step 1: Migrate conversation_list call site**

In `shared_app/src/chats/conversation_list.rs`, line 75, change:

```rust
        .on_press(on_press)
```

to:

```rust
        .on_tap(on_press)
```

(The parameter is still named `on_press` from the function signature — that's fine, it's just the local variable name. The method call changes to `.on_tap`.)

- [ ] **Step 2: Build shared_app**

Run: `cargo build -p shared_app`
Expected: compiles clean.

- [ ] **Step 3: Write the bug-fix integration tests**

In `vexo/src/elements/scroll_view.rs`, replace the two `#[ignore]`d tests (`test_touch_drag_scrolls_via_pipeline` and `test_touch_drag_clamps_at_top`) with these arena-based tests. Add them to the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn test_drag_in_tappable_row_scrolls_not_navigates() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::Flex;
        use crate::ThreeTreePipeline;
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;

        // Build a scroll view of tappable rows (GestureDetector.on_tap).
        let tap_count = Rc::new(Cell::new(0u32));
        let ctrl = ScrollController::new();
        let mut col = Flex::column();
        for _ in 0..200 {
            let tc = tap_count.clone();
            col = col.push(
                crate::Text::new("row")
                    .boxed()
                    .on_tap(move || tc.set(tc.get() + 1)),
            );
        }
        let sv = ScrollView::new(col.boxed()).controller(ctrl.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(sv));
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Press at (200, 300) inside the viewport.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        // Drag UP 50px (past slop) → should scroll toward bottom, NOT tap.
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 250.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 250.0),
            &move_evt,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 250.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(200.0, 250.0),
            &release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(
            ctrl.current_offset() > 0.0,
            "drag should scroll; got offset={}",
            ctrl.current_offset()
        );
        assert_eq!(
            tap_count.get(),
            0,
            "drag should NOT fire on_tap (navigate)"
        );
    }

    #[test]
    fn test_tap_in_tappable_row_navigates_not_scrolls() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::Flex;
        use crate::ThreeTreePipeline;
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;

        let tap_count = Rc::new(Cell::new(0u32));
        let ctrl = ScrollController::new();
        let mut col = Flex::column();
        for _ in 0..200 {
            let tc = tap_count.clone();
            col = col.push(
                crate::Text::new("row")
                    .boxed()
                    .on_tap(move || tc.set(tc.get() + 1)),
            );
        }
        let sv = ScrollView::new(col.boxed()).controller(ctrl.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(sv));
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Press + Release with no move past slop → tap fires, no scroll.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert_eq!(tap_count.get(), 1, "tap should fire on_tap once");
        assert_eq!(
            ctrl.current_offset(),
            0.0,
            "tap should NOT scroll"
        );
    }

    #[test]
    fn test_drag_clamps_at_top_with_arena() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::Flex;
        use crate::ThreeTreePipeline;
        use std::sync::Arc;

        let ctrl = ScrollController::new();
        let mut col = Flex::column();
        for _ in 0..200 {
            col = col.push(crate::Text::new("row"));
        }
        let sv = ScrollView::new(col.boxed()).controller(ctrl.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(sv));
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        // Drag DOWN 1000px from offset 0 → clamp at 0.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 1300.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 1300.0),
            &move_evt,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(ctrl.current_offset(), 0.0, "clamped at top");
    }

    #[test]
    fn test_on_press_fires_on_down_regardless_of_drag_win() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::Flex;
        use crate::ThreeTreePipeline;
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;

        let press_count = Rc::new(Cell::new(0u32));
        let tap_count = Rc::new(Cell::new(0u32));
        let ctrl = ScrollController::new();
        let mut col = Flex::column();
        for _ in 0..200 {
            let pc = press_count.clone();
            let tc = tap_count.clone();
            col = col.push(
                crate::Text::new("row")
                    .boxed()
                    .on_press(move || pc.set(pc.get() + 1))
                    .on_tap(move || tc.set(tc.get() + 1)),
            );
        }
        let sv = ScrollView::new(col.boxed()).controller(ctrl.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(sv));
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Press → on_press fires immediately.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(press_count.get(), 1, "on_press fires on press-down");

        // Drag past slop → drag wins, tap rejected.
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 250.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 250.0),
            &move_evt,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 250.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(200.0, 250.0),
            &release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(press_count.get(), 1, "on_press stays at 1 (no double-fire)");
        assert_eq!(tap_count.get(), 0, "on_tap does NOT fire (drag won)");
        assert!(ctrl.current_offset() > 0.0, "drag scrolled");
    }

    #[test]
    fn test_tap_outside_scroll_view_unchanged() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::ThreeTreePipeline;
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;

        let tap_count = Rc::new(Cell::new(0u32));
        let tc = tap_count.clone();
        let widget = crate::Text::new("tap me")
            .boxed()
            .on_tap(move || tc.set(tc.get() + 1));

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(widget));
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let press = InputEvent::PointerButton {
            position: Point::new(50.0, 20.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(50.0, 20.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let release = InputEvent::PointerButton {
            position: Point::new(50.0, 20.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(50.0, 20.0),
            &release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(tap_count.get(), 1, "tap fires outside scroll view");
    }
```

Also remove the `#[ignore]` attributes from the old tests and DELETE those two old test functions entirely (they're replaced by the new ones above).

- [ ] **Step 4: Run the integration tests**

Run: `cargo test -p vexo --lib scroll_view`
Expected: all tests pass, including the 5 new arena integration tests and `test_mouse_wheel_still_works`.

If `test_drag_in_tappable_row_scrolls_not_navigates` fails, debug using `RUST_LOG=debug` logs — do NOT reason without evidence (per CLAUDE.md GUI debugging rule, but these are headless tests so they can run directly).

- [ ] **Step 5: Run the full test suite**

Run: `cargo test -p vexo`
Expected: all pass.

Run: `cargo test -p shared_app`
Expected: `test_conversation_list_renders_in_pipeline` passes.

- [ ] **Step 6: Commit**

```bash
git add shared_app/src/chats/conversation_list.rs vexo/src/elements/scroll_view.rs
git commit -m "fix(chats): drag on conversation list scrolls instead of navigating (gesture arena)"
```

---

### Task 12: Migrate Button action API to on_tap + chat_screen send button

**Files:**
- Modify: `vexo_uikit/src/button.rs` (rename `on_press` → `on_tap`, split visual feedback from action in `render`)
- Modify: `shared_app/src/chats/chat_screen.rs:176` (`.on_press` → `.on_tap`)
- Modify: `vexo_uikit/tests/button_tests.rs:32,45` (`.on_press` → `.on_tap`)

**Interfaces:**
- Consumes: `WidgetExt::on_tap` (Task 7), `WidgetExt::on_press` (visual feedback)
- Produces: Button actions fire on tap-recognized (release-after-win), consistent with the arena model

- [ ] **Step 1: Rename Button's `on_press` field and builder to `on_tap`**

In `vexo_uikit/src/button.rs`:

Rename the field (line 55):
```rust
    on_tap: Rc<RefCell<dyn FnMut()>>,
```

Rename in `new()` (line 66):
```rust
            on_tap: Rc::new(RefCell::new(|| {})),
```

Rename the builder method (line 80):
```rust
    /// Set the tap action callback. Fires when the tap is recognized
    /// (pointer up, having won the gesture arena) — does NOT fire if a
    /// drag wins instead.
    pub fn on_tap(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_tap = Rc::new(RefCell::new(callback));
        self
    }
```

Update `press()` (line 112) to use the renamed field:
```rust
    pub fn press(&self) {
        if !self.disabled {
            (self.on_tap.borrow_mut())();
        }
    }
```

Update the doc comment example (line 49):
```rust
///     .on_tap(|| submit())
```

- [ ] **Step 2: Split visual feedback from action in `render`**

In `button.rs`, find the `render` method (around line 205). The current wiring (lines 250-257) is:

```rust
            .on_press(move || {
                if !disabled {
                    is_pressed_signal.set(true);
                    (on_press_cb.borrow_mut())();
                }
            })
```

Replace it with separate visual-feedback and action callbacks:

```rust
            .on_press(move || {
                if !disabled {
                    is_pressed_signal.set(true);
                }
            })
            .on_tap(move || {
                if !disabled {
                    (on_tap_cb.borrow_mut())();
                }
            })
```

And update the variable that captures the callback. Find the line (around 222):
```rust
        let on_press_cb = self.on_press.clone();
```
Change to:
```rust
        let on_tap_cb = self.on_tap.clone();
```

Keep `.on_release(...)`, `.on_enter(...)`, `.on_exit(...)` unchanged.

- [ ] **Step 3: Update chat_screen.rs call site**

In `shared_app/src/chats/chat_screen.rs`, line 176, change:

```rust
                .on_press(on_send),
```

to:

```rust
                .on_tap(on_send),
```

- [ ] **Step 4: Update button_tests.rs call sites**

In `vexo_uikit/tests/button_tests.rs`, lines 32 and 45, change `.on_press(` to `.on_tap(`.

- [ ] **Step 5: Build and test**

Run: `cargo build -p vexo_uikit && cargo build -p shared_app`
Expected: compiles clean.

Run: `cargo test -p vexo_uikit`
Expected: all button tests pass.

Run: `cargo test -p shared_app`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/button.rs vexo_uikit/tests/button_tests.rs shared_app/src/chats/chat_screen.rs
git commit -m "refactor(button): split visual feedback (on_press) from action (on_tap)"
```

---

### Task 13: Cancel-on-blur in window.rs

**Files:**
- Modify: `vexo/src/window.rs` (call `cancel_current_gesture()` on window unfocus)

**Interfaces:**
- Consumes: `ThreeTreePipeline::cancel_current_gesture()` (Task 6)
- Produces: arena is cancelled (not leaked) when the window loses focus mid-press

- [ ] **Step 1: Add cancel handling to the window event loop**

In `vexo/src/window.rs`, find the `handle_window_event` method (line 150). It matches on `WindowEvent` variants. Add a new arm for `WindowEvent::Focused` (placed before the `_ =>` catch-all, around line 226):

```rust
            WindowEvent::Focused(focused) => {
                if !focused {
                    // Window lost focus — cancel any in-flight gesture so the
                    // arena doesn't leak (no release event will arrive).
                    self.three_tree_pipeline.cancel_current_gesture();
                }
            }
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p vexo`
Expected: compiles clean. (If `WindowEvent::Focused` is not a variant in this winit version, check the winit version in Cargo.toml — winit 0.30 uses `WindowEvent::Focused(bool)`.)

- [ ] **Step 3: Commit**

```bash
git add vexo/src/window.rs
git commit -m "feat(window): cancel gesture arena on window unfocus"
```

---

### Task 14: Full workspace verification

- [ ] **Step 1: Build the entire workspace**

Run: `cargo build`
Expected: all crates compile clean.

- [ ] **Step 2: Run the entire test suite**

Run: `cargo test`
Expected: all tests pass across all crates.

- [ ] **Step 3: Ask the user to run the desktop demo**

Per CLAUDE.md, never run `cargo run -p desktop_demo` yourself. Ask the user:

> "The gesture arena is implemented. Please run `cargo run -p desktop_demo` and verify: (1) dragging on the conversation list scrolls instead of opening a chat, (2) tapping a conversation still opens the chat, (3) the send button still works, (4) mouse wheel scroll still works."

- [ ] **Step 4: Final commit (if any fixes were needed from user testing)**

If the user reports issues, fix them with the `debugging-gui-with-logs` skill (form hypothesis → add logs → ask user to run → read evidence → fix root cause). Commit fixes.

---

## Self-Review Notes

**Spec coverage:**
- Arena + recognizers (spec §"Arena and Recognizer Core") → Tasks 1-4 ✓
- Element registration (spec §"Element Registration") → Tasks 5, 8, 9 ✓
- Pipeline wiring (spec §"Pipeline Wiring") → Tasks 6, 10 ✓
- on_tap API (spec §"GestureDetector widget API") → Task 7 ✓
- Call-site migration (spec §"Call-site migration") → Tasks 11, 12 ✓
- Edge case 3 (cancel-on-blur) → Task 13 ✓
- Edge cases 1, 2, 4, 5, 6, 7 → covered by Task 11 tests ✓
- Testing strategy (spec §"Testing Strategy") → Tasks 2, 3, 4 (Layers 1-2) + Task 11 (Layer 3) ✓

**Placeholder scan:** No TBD/TODO. All steps contain complete code.

**Type consistency:** `TapRecognizer::new()`, `VerticalDragRecognizer::new()`, `GestureArena::new(down_position)`, `arena.add(recognizer, owner)`, `arena.handle_event(event) -> ArenaOutcome`, `arena.winner_owner() -> Option<ElementKey>`, `arena.winner_recognizer() -> Option<&dyn GestureRecognizer>`, `arena.sweep_on_up()`, `recognizer.as_any()` — all consistent across tasks.
