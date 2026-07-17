# Scroll View Inertial Momentum — Design Spec

**Date:** 2026-07-17
**Status:** Draft

## Problem

`ScrollView` currently tracks the finger 1:1 during a drag, then stops dead on
release. The comment at `vexo/src/elements/scroll_view.rs:311` says it plainly:

```rust
ArenaEvent::Up { .. } => {
    // Drag ended. No scroll applied on up (no momentum in v1).
}
```

iOS scroll views coast after the finger lifts: the content keeps moving, the
velocity decays exponentially, and the scroll settles to a stop over roughly
2–3 seconds for a brisk flick. Adding this behavior is the goal of this spec.

## Scope (Decisions Locked During Brainstorming)

| # | Decision | Choice |
|---|---|---|
| 1 | Behavior scope | **Momentum only.** No rubber-band overscroll, no spring-back. Hard clamp at boundaries. |
| 2 | Input devices | **Touch drag only.** Mouse wheel and keyboard remain instantaneous per-event. Existing `test_mouse_wheel_still_works` (asserts `offset == 100.0`) stays passing unchanged. |
| 3 | Edge behavior | **Hard clamp at `[0, max_scroll]`** each frame. If a fling reaches the edge, scroll stops there. |
| 4 | Velocity source | **Last-N time-weighted samples** (~100ms window). Matches iOS/Flutter. |
| 5 | Min fling velocity | **50 px/s.** Below this, no momentum — the scroll stops where the finger left it. Matches Flutter's `minFlingVelocity`. |

## Non-Goals

- Rubber-band / overscroll stretch (deferred).
- Bounce-back at slow drag past edge (deferred).
- Mouse-wheel momentum (out of scope per Q2).
- Keyboard momentum (out of scope per Q2).
- Horizontal scroll (out of scope).
- Public API changes to `ScrollController` (its `jump_to_*` methods stay
  programmatic-instant; momentum is internal to the touch path).

## Approach

Two new pure-value types, plus four small wiring points on the existing
`ScrollViewElement`. No new widgets, no changes to the `ScrollView` widget, no
changes to `ScrollController`'s public API, no changes to the gesture
recognizer or render object.

The shape mirrors `AnimationController`: a time-driven simulation that owns
ticker registration + dirty callback. The existing deferred-apply pipeline
(mpsc → `drain_dirty_to_build_owner` → `rebuild_from_state`) drives the
per-frame offset writes. No new pipeline paths.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  vexo/src/gestures/velocity_tracker.rs   (NEW, ~80 lines)           │
│    VelocityTracker                                                  │
│      - ring buffer of (Instant, y) — last ~100ms                    │
│      - add(t, y) / velocity() -> f32  (least-squares slope)         │
│      - clear()                                                      │
│      Pure value type, no framework deps. Unit-testable in isolation.│
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│  vexo/src/animation/momentum.rs   (NEW, ~110 lines)                 │
│    MomentumSimulation                                               │
│      - offset, velocity (px/s), τ (decay constant), start Instant   │
│      - start(offset, velocity, now, dirty_cb, elem_id, ticker)      │
│      - advance(now) -> Option<f32>   // Some(new_offset) while live │
│      - stop()                                                       │
│      - is_active() -> bool                                          │
│      Owns ticker registration + dirty callback (mirrors             │
│      AnimationController's pattern in animation/controller.rs).     │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│  vexo/src/elements/scroll_view.rs   (MODIFIED, 5 modification sites) │
│    ScrollViewElement                                                │
│      + velocity_tracker: VelocityTracker                            │
│      + momentum: MomentumSimulation                                 │
│      + animation_ticker: Option<Arc<AnimationTicker>>               │
│        (stashed in mount(); EventContext doesn't expose the ticker) │
│      mount                       → stash ctx.animation_ticker.clone()│
│      on_arena_winner_update Down  → tracker.clear(); momentum.stop()│
│      on_arena_winner_update Move  → tracker.add(now, y)             │
│      (existing scroll-delta apply unchanged)                        │
│      on_arena_winner_update Up    → v = -tracker.velocity();         │
│                                     if |v| >= 50: momentum.start()  │
│      rebuild_from_state          → if momentum.is_active():          │
│                                     advance(now); apply + edge-stop │
│      unmount                      → momentum.stop()                 │
└─────────────────────────────────────────────────────────────────────┘
```

**Module placement rationale:**
- `velocity_tracker.rs` lives under `gestures/` because it consumes
  gesture-stage data (pointer positions during a drag) — same conceptual
  layer as `VerticalDragRecognizer`.
- `momentum.rs` lives under `animation/` because it's a time-driven
  simulation that registers with `AnimationTicker`, exactly like
  `AnimationController`.

### Data Flow for a Touch Fling

```
finger moves   →  VerticalDragRecognizer (unchanged)
                →  ScrollViewElement.on_arena_winner_update(Move)
                   ├─ tracker.add(now, position.y)         [NEW]
                   └─ apply delta to scroll_offset          [existing, unchanged]

finger lifts   →  on_arena_winner_update(Up)
                →  v = -tracker.velocity()                  [NEW]
                →  if |v| >= 50: momentum.start(offset, v)  [NEW]
                   └─ momentum registers with ticker, fires dirty callback
                      (mirrors AnimationController::forward at controller.rs:36-52)

per frame      →  AnimationTicker.tick()  (window.rs:435, unchanged)
                →  dirty callback sends element_id through mpsc
                →  pipeline.drain_dirty_to_build_owner()
                →  ScrollViewElement.rebuild_from_state()
                   ├─ offset = momentum.advance(now)        [NEW]
                   ├─ apply_scroll_offset(clamped)          [existing, unchanged]
                   └─ if hit edge / velocity decayed: momentum.stop()  [NEW]

finger down    →  on_arena_winner_update(Down)
again             ├─ momentum.stop()                        [NEW]
                   └─ tracker.clear()                       [NEW]
```

### Key Invariants

1. **Exclusive offset ownership.** The simulation is the only thing writing
   offset during momentum. Touch tracking writes during drag; momentum writes
   after release. They never overlap because `Down` stops the simulation
   before tracking begins.
2. **`apply_scroll_offset` is reused unchanged.** It already does
   clamp + render-object write + controller writeback + `mark_needs_build`
   (`scroll_view.rs:74-110`). Momentum just calls it with a new value each
   frame.
3. **Coexistence with deferred-apply.** The existing `ScrollController` jump
   path and the new momentum path both flow through `rebuild_from_state`.
   They can't conflict because a touch `Down` stops momentum, and a
   programmatic `jump_to_*` calls `momentum.stop()` before applying its target
   (the programmatic target wins by being applied last).

## Physics

Exponential decay, matching iOS UIKit's `UIScrollView`.

**Model (closed-form, not Euler — immune to frame-rate variance):**
```
v(t)     = v0 · e^(-t/τ)
Δoffset(t) = v0 · τ · (1 - e^(-t/τ))
offset(t) = offset0 + Δoffset(t)
```
where `v0` is release velocity in px/s (positive = scrolling toward bottom,
see Sign Convention below), `t` is seconds since release, `τ` is the decay
time constant.

**Constants:**

| Constant | Value | Source / Rationale |
|---|---|---|
| `τ` (decay time constant) | 0.325 s | iOS UIKit / Flutter公开 deceleration constant. At 60fps, brisk flings settle over ~2.5–3s. |
| `V_STOP` (terminate below) | 13.0 px/s | iOS UIKit `minimumVelocity`. |
| `V_MIN_FLING` (skip momentum below) | 50.0 px/s | Flutter `minFlingVelocity`. |
| `MAX_DURATION` (safety ceiling) | 10.0 s | Defensive; normal flings never approach this. |

### `MomentumSimulation::advance(now) -> Option<f32>`

1. `dt = (now - start_time).as_secs_f32()`.
2. If `dt > MAX_DURATION`: return `None` (terminate).
3. `v = v0 · e^(-dt/τ)`.
4. If `|v| < V_STOP`: return `None` (terminate).
5. `offset = offset0 + v0 · τ · (1 - e^(-dt/τ))`.
6. Return `Some(offset)`.

`advance` is a pure function of `(now, start_time, v0, offset0)` — no
internal state that depends on call frequency. This makes it trivially
unit-testable by feeding synthetic `Instant`s.

### Edge Clamp

The simulation itself does NOT clamp — it returns the raw physics offset,
which may overshoot `[0, max_scroll]`. The element's existing
`apply_scroll_offset` clamps before writing to the render object.

When the physics offset crosses an edge, the element stops the simulation to
avoid burning ticker ticks computing motion that gets thrown away. Detection
in `rebuild_from_state`:

```rust
if self.momentum.is_active() {
    let now = Instant::now();
    match self.momentum.advance(now) {
        Some(physics_offset) => {
            let clamped = self.clamp_offset(physics_offset);
            let hit_edge = (clamped - physics_offset).abs() > f32::EPSILON;
            if hit_edge {
                self.momentum.stop();
            }
            self.apply_scroll_offset(clamped, context);
        }
        None => {
            self.momentum.stop();
        }
    }
}
```

## Velocity Tracking

Pure-value ring buffer. No framework dependencies.

```rust
pub struct VelocityTracker {
    samples: VecDeque<(Instant, f32)>,  // (timestamp, y)
    window: Duration,                    // = 100ms
}
```

### `add(t, y)`
1. Push `(t, y)` onto the back.
2. Drop samples from the front older than `t - window`.

The windowing is what makes this robust to a slow start + fast finish: old
slow samples fall out and don't drag down the average.

### `velocity() -> f32`
Least-squares linear regression over the samples in the window (same
algorithm Flutter's `VelocityTracker` uses for this case):

```
Given N samples (t_i, y_i) with t_i in seconds (relative to any epoch):
  Σt = Σ t_i,  Σy = Σ y_i,  Σtt = Σ t_i²,  Σty = Σ t_i·y_i
  slope = (N·Σty - Σt·Σy) / (N·Σtt - Σt²)   // px per second
```

Edge cases:
- `< 2 samples` → return `0.0` (no fling).
- Denominator `≈ 0` (all samples at identical timestamp — defensive) →
  return `0.0`.

**Why least-squares over a 2-point secant:** A secant over the window
endpoints (`(y_last - y_first) / (t_last - t_first)`) throws away interior
signal and is strictly noisier than regression on the same samples. Least
squares uses every sample.

### Sign Convention

Pointer `y` increases downward (screen coordinates — matches
`VerticalDragRecognizer`).

Worked example:
- Finger moves **up** on screen: pointer `y` **decreases**.
- Existing drag handler (`scroll_view.rs:296`):
  `delta = last_drag_y - position.y` → **positive** delta → `scroll_offset`
  **increases** → content scrolls toward bottom. This is the user's
  expectation: drag up = see content below.
- `VelocityTracker::velocity()` returns raw pointer-space `dy/dt`. For the
  same upward finger motion: `dy/dt < 0`.
- For momentum to scroll toward bottom (offset **increases**) after an
  upward fling, we need `v0 > 0`. So the element **negates** the tracker
  result before passing to `momentum.start`:
  ```rust
  let v = -self.velocity_tracker.velocity();  // pointer-y-down → offset-up
  ```
- Symmetric: a downward finger motion → `dy/dt > 0` → negated `v0 < 0` →
  offset decreases → scrolls toward top. Correct.

### Sampling Point

`on_arena_winner_update(Move)` calls `tracker.add(Instant::now(), position.y)`
*before* the existing delta logic runs, so the sample's timestamp reflects
when the pointer was actually here, not after delta math.

On `Down`, `tracker.clear()` runs first.

**What doesn't get sampled:**
- Mouse wheel (`InputEvent::Scroll`) — no momentum per Decision 2, so no
  tracking.
- Keyboard — same.
- Pre-slop moves — yes, sampled. They're inside the 100ms window, so they
  contribute if the fling happens fast. Matches iOS: the recognizer accepts
  at slop, but the tracker has been recording since `Down`.

## Integration with `ScrollViewElement`

Five modification sites: `mount` (stash ticker), three `on_arena_winner_update`
arms (`Down`, `Move`, `Up`), `rebuild_from_state`, and `unmount`. No changes
to the widget, render object, controller's public API, or gesture recognizer.

### `EventContext` ticker access (resolved)

`EventContext` (`vexo/src/event_context.rs:24-88`) does **not** expose the
animation ticker — its fields are pointer/focus/font/clipboard/build_owner/
dirty_sender/render_objects only. So the element stashes the
`Arc<AnimationTicker>` during `mount`, when it has an `ElementContext`
(which does expose `pub animation_ticker` at `element_context.rs:41`). The
`Up` wiring then reads `self.animation_ticker.as_ref().unwrap().clone()`.
This is the same field-plumbing pattern `StatefulElement` uses
(`stateful_widget.rs:210`).

### New Fields (`scroll_view.rs:35-48`)

```rust
pub struct ScrollViewElement {
    // ... existing fields unchanged ...
    velocity_tracker: VelocityTracker,
    momentum: MomentumSimulation,
    animation_ticker: Option<Arc<AnimationTicker>>,  // stashed in mount()
}
```
Constructed in `ScrollViewElement::new()` with default state (empty tracker,
inactive simulation, `None` ticker — populated in `mount`).

### `mount` (`scroll_view.rs:158-176`)

One new line, stashes the ticker for later use in the `Up` wiring:

```rust
fn mount(&mut self, context: &mut ElementContext) {
    self.animation_ticker = Some(context.animation_ticker.clone());  // NEW
    // ... existing mount logic unchanged ...
}
```

### Wiring 1 — `on_arena_winner_update(Down)` (`scroll_view.rs:301-309`)

```rust
ArenaEvent::Down { .. } => {
    self.momentum.stop();           // NEW: kill any in-flight fling
    self.velocity_tracker.clear();  // NEW: fresh window for this drag
    self.last_drag_y = drag.down_position().y;  // existing, unchanged
}
```
Order matters: stop momentum *before* clearing the tracker, so an in-flight
fling can't race with a new drag's samples.

### Wiring 2 — `on_arena_winner_update(Move)` (`scroll_view.rs:290-300`)

```rust
ArenaEvent::Move { position } => {
    // NEW: sample for velocity. Done first so the sample's timestamp
    // reflects when the pointer was actually here, not after delta math.
    self.velocity_tracker.add(Instant::now(), position.y);

    // Existing delta logic, unchanged.
    let delta = self.last_drag_y - position.y;
    self.last_drag_y = position.y;
    let new_offset = self.scroll_offset + delta;
    self.apply_scroll_offset(new_offset, ctx);
}
```

### Wiring 3 — `on_arena_winner_update(Up)` (`scroll_view.rs:310-312`)

```rust
ArenaEvent::Up { .. } => {
    // NEW: maybe start momentum.
    let v = -self.velocity_tracker.velocity();  // sign-flip per Sign Convention
    if v.abs() < V_MIN_FLING {
        return;
    }
    // All three are Some by the time Up fires (post-mount, real pipeline).
    // `let-else` keeps the early-out readable.
    let Some(element_id) = self.id else { return; };
    let Some(tx) = ctx.dirty_sender.cloned() else { return; };
    let Some(ticker) = self.animation_ticker.clone() else { return; };

    self.momentum.start(
        self.scroll_offset,
        v,
        Instant::now(),
        tx,            // mpsc::Sender<ElementKey> for dirty callback
        element_id,    // mpsc payload
        ticker,        // Arc<AnimationTicker> for per-frame ticks
    );
}
```

**Field access notes:**
- `self.id: Option<ElementKey>` — set during `mount` (existing field at
  `scroll_view.rs:36`). Read via the field directly, not the
  `element_id()` accessor (which also returns `Option`).
- `ctx.dirty_sender: Option<&'a mpsc::Sender<ElementKey>>` — public field on
  `EventContext` (`event_context.rs:66`). `.cloned()` lifts
  `Option<&Sender>` to `Option<Sender>` (clones the `Sender`, which is cheap
  and `Send + Sync`).
- `self.animation_ticker` — the new stash field (see `mount` wiring above).

The dirty-callback shape mirrors `wire_dirty_callback` at
`scroll_view.rs:27-33`:
```rust
let tx = /* cloned Sender */;
let element_id = /* ElementKey */;
let cb = Arc::new(move || { let _ = tx.send(element_id); });
```

### Wiring 4 — `rebuild_from_state` (`scroll_view.rs:366-405`)

```rust
fn rebuild_from_state(&mut self, context: &mut ElementContext) {
    // Existing deferred-apply from ScrollController (jump_to_*).
    let pending = self.controller.as_ref().and_then(|c| c.take_target_offset());
    if let Some(target) = pending {
        self.momentum.stop();  // NEW: programmatic jump cancels in-flight fling
        // ... existing target-apply logic unchanged ...
    }

    // NEW: momentum step.
    if self.momentum.is_active() {
        let now = Instant::now();
        match self.momentum.advance(now) {
            Some(physics_offset) => {
                let clamped = self.clamp_offset(physics_offset);
                let hit_edge = (clamped - physics_offset).abs() > f32::EPSILON;
                if hit_edge {
                    self.momentum.stop();
                }
                self.apply_scroll_offset(clamped, context);
            }
            None => {
                self.momentum.stop();
            }
        }
    }

    // Existing mark_needs_paint — unchanged.
    if let Some(ro_key) = self.render_object {
        context.mark_needs_paint(ro_key);
    }
}
```

### `unmount` (`scroll_view.rs:200-208`)

One new line, prevents orphaned ticker registration:

```rust
fn unmount(&mut self, context: &mut ElementContext) {
    self.momentum.stop();  // NEW
    if let Some(ctrl) = self.controller.as_ref() { ctrl.clear_dirty_callback(); }
    // ... existing unmount logic unchanged ...
}
```

### Termination Conditions

All routes to `momentum.stop()`:

1. Velocity decayed below `V_STOP` → `advance` returns `None`.
2. Physics offset crossed an edge → element calls `stop()` after applying
   the clamped value (so we land exactly on the edge, not before it).
3. New touch `Down` → `stop()` (user grabs the scroll mid-fling).
4. Programmatic `jump_to_*` → `stop()` (external command wins).
5. `MAX_DURATION` exceeded → `advance` returns `None` (defensive).
6. Element `unmount` → `stop()` (drops ticker registration).

### Frame Loop Continuity

`momentum.start` fires the dirty callback immediately (same trick
`AnimationController::forward` uses at `controller.rs:49-51`), which sends
`element_id` through the mpsc → `pipeline.drain_dirty_to_build_owner()` →
`rebuild_from_state` runs → applies offset → `apply_scroll_offset` calls
`mark_needs_build` → next frame requested → ticker ticks the simulation's
callback → repeat. The loop is self-sustaining until `stop()`.

`window.rs:598-599` already requests frames while
`animation_ticker.has_active()`, so no window-layer changes are needed.

## Testing

### `VelocityTracker` unit tests (`vexo/src/gestures/velocity_tracker.rs`)

Pure, no framework:
- Empty tracker → `velocity() == 0.0`.
- Single sample → `velocity() == 0.0` (need ≥2).
- Two samples 50ms apart, 100px apart → `velocity() ≈ 2000 px/s`.
- Three samples forming a line → slope matches.
- Noisy samples (non-monotonic y) → regression still returns a slope.
- Old samples dropped from window: add 5 samples spanning 200ms, only last
  100ms contribute.
- `clear()` empties the buffer.

### `MomentumSimulation` unit tests (`vexo/src/animation/momentum.rs`)

Pure, feed synthetic `Instant`s:
- `advance` at `t=0` returns `Some(offset0)` (no movement yet).
- `advance` at `t=τ` returns offset with `Δoffset = v0·τ·(1 - 1/e)` to within
  `1e-3`.
- Velocity below `V_STOP` → `advance` returns `None`.
- `dt > MAX_DURATION` → `advance` returns `None`.
- `stop()` clears active state; subsequent `advance` is a no-op.
- `is_active()` is true after `start`, false after `stop` or after `advance`
  returns `None`.
- Sign propagation: positive `v0` → offset increases; negative `v0` → offset
  decreases.

### `ScrollViewElement` integration tests (`vexo/src/elements/scroll_view.rs`)

Build on the existing test harness (which already pumps the pipeline):
- **Fast upward drag → release → offset increases further after release.**
  Synthesize: press, several moves upward over ~80ms, release. Pump ticker
  + pipeline. Assert `ctrl.current_offset()` is strictly greater than the
  offset at release time.
- **Slow drag → release → no momentum.** Synthesize a drag with final
  velocity below 50 px/s. Assert offset at release equals offset after pump.
- **Fling toward bottom edge → clamps at max_scroll.** Build a 200-row
  scroll view, fling hard. Assert final offset `== max_scroll` exactly.
- **Fling toward top from middle → clamps at 0.** Start offset at 200px,
  fling upward. Assert final offset `== 0.0` exactly.
- **Tap mid-fling stops momentum.** Start a fling, then synthesize a new
  `Down` event partway through. Assert `momentum.is_active() == false` and
  the offset doesn't change on subsequent pumps.
- **Mouse wheel unchanged.** Existing `test_mouse_wheel_still_works` passes
  unchanged (asserts `offset == 100.0` after a single wheel tick — no
  momentum animation should engage).
- **Keyboard unchanged.** A new test asserting arrow-down produces exactly
  `LINE_HEIGHT` offset (40.0) per press, no coasting.
- **Programmatic `jump_to` cancels momentum.** Start a fling, immediately
  call `ctrl.jump_to(50.0)`, pump. Assert final offset `== 50.0`, not the
  fling's projected landing point.
- **VelocityTracker integration in Move.** A drag with a known
  position-time series produces a fling with predictable direction (sign
  of post-release offset delta matches sign of finger motion).

## Files Touched

| File | Change |
|---|---|
| `vexo/src/gestures/mod.rs` | Add `pub mod velocity_tracker;` + re-export `VelocityTracker`. |
| `vexo/src/gestures/velocity_tracker.rs` | NEW. |
| `vexo/src/animation/mod.rs` | Add `pub mod momentum;` + re-export `MomentumSimulation`. |
| `vexo/src/animation/momentum.rs` | NEW. |
| `vexo/src/elements/scroll_view.rs` | Add 3 fields (`velocity_tracker`, `momentum`, `animation_ticker`) + `mount` stash + 3 arena wirings + `rebuild_from_state` momentum step + `unmount` stop line. Update `new()`. |
| `vexo/src/lib.rs` | Re-export `VelocityTracker`, `MomentumSimulation` if public API surface needs them (likely no — internal use only). |

No changes to: `widgets/scroll_view.rs`, `widgets/scroll_controller.rs`,
`render_objects/scroll_view.rs`, `gestures/vertical_drag.rs`,
`gestures/arena.rs`, `pipeline.rs`, `window.rs`.

## Risks

- **Sign-flip correctness.** The `v = -tracker.velocity()` line is the one
  place a sign bug silently produces "fling goes the wrong way." Mitigated
  by the worked example in Sign Convention and an explicit integration test
  ("fling direction matches finger direction").
- **Mid-fling unmount.** If the element is unmounted mid-fling,
  `momentum.stop()` in `unmount` drops the ticker registration before the
  dirty callback can fire again. Safe — no use-after-free.
- **Ticker stash lifetime.** `animation_ticker: Option<Arc<AnimationTicker>>`
  is `None` between `new()` and `mount()`. The `Up` wiring handles this with
  `let Some(ticker) = ... else { return; }`. The `Option` matches the
  convention of neighboring fields like `controller` (also `Option`).
- **Test determinism.** Velocity-tracker tests use synthetic `Instant`s, so
  they're deterministic. Momentum-sim tests use synthetic `Instant`s, also
  deterministic. Integration tests synthesize input events with controlled
  timing; no real wall-clock dependency.
