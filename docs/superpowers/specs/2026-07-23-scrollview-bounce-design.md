# ScrollView Bounce Effect — Design

**Date:** 2026-07-23
**Status:** Approved (pending spec review)
**Scope:** Vertical-only bounce for `ScrollView`, always-on, iOS-accurate rubber-band + spring-back.

## Problem

`ScrollView` currently hard-clamps `scroll_offset` to `[0, max_scroll]` at two sites
(`elements/scroll_view.rs:93-95` and `render_objects/scroll_view.rs:112-115`). Dragging or
flinging past an edge stops dead. There is no overscroll physics. The goal is iOS-style
behavior: drag past an edge moves content with decreasing resistance (rubber-band), release
springs back to the edge, and a fling that hits an edge mid-flight carries its remaining
momentum into a brief spring overshoot and settle.

## Requirements

1. **Full iOS rubber-band drag** — dragging past an edge moves content with resistance that
   grows as overscroll deepens; content asymptotically approaches ~one viewport past the edge.
2. **Spring-back on release** — releasing in overscroll starts a spring that returns content
   to the edge. The release velocity carries into the spring (a hard flick back feels
   different from a gentle release).
3. **Fling carries into bounce** — a fling that hits an edge mid-flight hands off its
   remaining velocity to a spring, producing one bounded overshoot and settle (no dead stop).
4. **Always-on, no opt-out** — every `ScrollView` bounces. No new public API.
5. **Vertical only** — matches the current vertical-only `ScrollView`.
6. **No regressions** — programmatic `jump_to`, wheel, keyboard, and tap-through to children
   all continue to work; they stop any active bounce.

## Non-Goals

- Horizontal bounce (would require horizontal scroll support first).
- Configurable physics per-instance (always-on; constants are module-level `const`s).
- Pixel-precise match to iOS — feel is tuned by eye.

## Architecture

### Files touched

| File | Change |
|---|---|
| `vexo/src/animation/spring.rs` | **NEW** — `SpringSimulation`, mirrors `MomentumSimulation`'s lifecycle |
| `vexo/src/animation/mod.rs` | Re-export `SpringSimulation` |
| `vexo/src/animation/momentum.rs` | Add `pub fn velocity(&self) -> f32` (read-only accessor) |
| `vexo/src/elements/scroll_view.rs` | Loosen clamps, add rubber-band drag resistance, release→spring, fling→spring handoff, `rebuild_from_state` spring branch, lifecycle stops, extract `apply_rubber_band` helper |
| `vexo/src/render_objects/scroll_view.rs` | Remove the `apply_layout` hard-clamp at L112-115 |
| `vexo/src/widgets/scroll_view.rs` | No change (always-on; no new API) |

### What does NOT change

- **Painter / clip / offset pipeline** — already handles arbitrary (including negative/overscroll) offsets correctly. Clip is pushed before offset (`painter.rs:218-295`), so content is clipped to the viewport while the offset shifts it — exactly the rubber-band visual.
- **Gesture arena, `VerticalDragRecognizer`, `VelocityTracker`** — drag detection and velocity sampling work as-is.
- **`ScrollController`** — externally unchanged; internally cancels any active spring (just as it cancels momentum today at L467).
- **Hit-testing** — `hit_test.rs:356-375` already subtracts `scroll_offset` from the pointer, so children shift correctly during overscroll.
- **Public `ScrollView` API** — no new methods.

### State model

`ScrollViewElement` gains one field. `scroll_offset` is now **allowed to leave `[0, max_scroll]`**
during a bounce — it is the single source of truth, and the two hard clamps are removed.

```rust
struct ScrollViewElement {
    // existing
    scroll_offset: f32,           // now ALLOWED outside [0, max_scroll] during bounce
    momentum: MomentumSimulation,
    // ...
    // new
    spring: SpringSimulation,     // bounce-back; mutually exclusive with momentum
}
```

**Invariant:** `momentum` and `spring` are mutually exclusive. Starting one stops the other.
Enforced at every `start()` call site. A `debug_assert` guards this during development.

### Data flow during a bounce

```
User drags past top edge
  → on_arena_winner_update (Move) applies rubber-band resistance to the over-edge delta
  → scroll_offset goes negative (e.g. -40)
  → ScrollViewRenderObject.scroll_offset Cell is set to -40
  → painter PushOffset shifts content down by 40px (rubber-band visual) ✓

User releases
  → on_arena_winner_update (Up): if scroll_offset < 0 (or > max),
    spring.start(offset=-40, v0=tracked_velocity, rest=0)
  → spring registers a ticker handle + fires dirty callback
  → next frame: rebuild_from_state advances spring → scroll_offset moves toward 0
  → spring settles (|offset-rest| < X_SETTLE && |v| < V_SETTLE) → spring.stop() unregisters ticker
```

## SpringSimulation Physics

A new module `vexo/src/animation/spring.rs`, structurally mirroring `MomentumSimulation`
(`animation/momentum.rs`): same ticker-registration + dirty-callback + `advance(now) ->
Option<f32>` + `stop()` + `is_active()` lifecycle.

### The ODE

A **critically-damped harmonic oscillator** toward a `rest` position. Critical damping means
the spring returns to rest **as fast as possible without oscillating** — iOS's settle feel
(no wobble, just a smooth deceleration to the edge).

```
x'' = -k·(x - rest) - c·x'
```

For critical damping, `c = 2·√k`. `DAMPING_RATIO` multiplies this coefficient
(`c = 2·√k · ratio`), so `ratio = 1.0` is critical, `< 1.0` underdamped (wobbly),
`> 1.0` overdamped (sluggish). We ship at `1.0`.

### Integration

Semi-implicit (symplectic) Euler with a fixed substep — stable for springs and cheap. Each
`advance(now)` call:

1. Compute `dt = now - last_step_time`, clamped to `MAX_FRAME_DT` to avoid instability after
   a pause (mirrors `momentum.rs`).
2. Substep `dt` into fixed `DT = 1/120s` chunks (frame-rate independent; stable regardless
   of refresh rate).
3. For each substep:
   ```
   a = -k·(x - rest) - c·v
   v += a · DT
   x += v · DT
   ```
4. Update internal `x`, `v`, `last_step_time`.
5. Return `Some(x)` if not settled, `None` if settled.

### Settle detection

Settled when **both**:
- `|x - rest| < X_SETTLE` (1.0 px)
- `|v| < V_SETTLE` (13.0 px/s — same as `momentum.rs`'s `V_STOP`)

On settle, return `None`; the element calls `spring.stop()`.

### Constants (tuned, iOS-like)

Starting values, to be tuned by eye on the desktop demo. Defined as `const` in `spring.rs`
so they're trivial to retune:

```rust
const STIFFNESS: f32 = 340.0;        // k — how strongly it pulls back. iOS ~300-400.
const DAMPING_RATIO: f32 = 1.0;       // 1.0 = critically damped (no overshoot on release).
                                      // <1.0 underdamped (wobbly), >1.0 overdamped (sluggish).
const X_SETTLE: f32 = 1.0;            // px from rest to consider settled
const V_SETTLE: f32 = 13.0;           // px/s velocity to consider settled
const DT: f32 = 1.0 / 120.0;          // fixed substep
const MAX_FRAME_DT: f32 = 1.0 / 30.0; // clamp dt after a pause (mirrors momentum.rs)
const MAX_DURATION: f32 = 10.0;       // hard stop after 10s (safety, mirrors momentum.rs)
```

### Why critical damping (not underdamped)?

iOS's *release* bounce-back is critically damped — it doesn't overshoot the edge on the way
back. The *elasticity* comes from the drag resistance and the velocity carryover on release,
not from spring oscillation. An underdamped spring would make content wobble past the edge on
release, which is *not* how iOS scroll bounce-back behaves. (`DAMPING_RATIO` is exposed so a
future bouncier feel can be dialed in, but we ship at `1.0`.)

**Subtlety — fling-into-edge:** A critically-damped spring with `v0` pointing *away* from
rest will overshoot once (the velocity carries it past rest before the damping stops it),
then return. This is the correct fling-into-edge behavior: one bounded overshoot and settle,
not a dead stop.

### Lifecycle (identical to MomentumSimulation)

```rust
pub struct SpringSimulation {
    // physics state
    offset: f32,           // current x
    velocity: f32,         // current v
    rest: f32,             // target edge offset (0 or max_scroll)
    start_time: f32,       // for MAX_DURATION safety
    last_step: f32,        // for dt computation
    active: bool,
    // framework plumbing (same as momentum.rs)
    tick_handle: Option<TickHandle>,
    // ... (tx, element_id captured at start)
}

impl SpringSimulation {
    pub fn is_active(&self) -> bool;
    pub fn start(&mut self, offset0: f32, v0: f32, rest: f32, now: f32, tx, element_id, &ticker);
    pub fn advance(&mut self, now: f32) -> Option<f32>;  // None = settled
    pub fn velocity(&self) -> f32;                       // read-only, mirrors momentum addition
    pub fn rest(&self) -> f32;                           // read-only, for settle snap
    pub fn stop(&mut self);
}
```

## Drag Resistance (Rubber-Band)

When dragging past an edge, content moves with decreasing resistance. The further you pull,
the harder it gets.

### The resistance function

iOS uses a curve where the **effective delta** applied to `scroll_offset` shrinks as
overscroll grows. Canonical form (matches Flutter's `BouncingScrollPhysics` and iOS):

```
applied_delta = raw_delta * (1 - overscroll / (overscroll + viewport_dimension))
```

Properties:
- At `overscroll = 0` (just touching edge): `applied_delta = raw_delta` — no resistance yet,
  smooth handoff from normal scrolling.
- As `overscroll → ∞`: `applied_delta → 0` — content asymptotically approaches
  `viewport_dimension` past the edge but can never exceed it. Content can only be pulled
  ~one viewport past the edge, no matter how hard you drag.
- Symmetric: same resistance at top and bottom edges.

### Implementation (pure helper)

Extracted as a private free function so it's unit-testable:

```rust
fn apply_rubber_band(raw_new: f32, viewport: f32, max: f32) -> f32 {
    // Split into in-bounds base + out-of-bounds excess.
    let (base, excess) = if raw_new < 0.0 {
        (0.0, raw_new)              // excess is negative (past top)
    } else if raw_new > max {
        (max, raw_new - max)        // excess is positive (past bottom)
    } else {
        (raw_new, 0.0)              // no excess
    };

    let overscroll = excess.abs();
    let resistance = 1.0 - overscroll / (overscroll + viewport.max(1.0));
    let resisted_excess = excess.signum() * overscroll * resistance;

    base + resisted_excess
}
```

Pure function of `(raw_new, viewport, max)`. Handles both edges symmetrically. The
`viewport.max(1.0)` guard prevents div-by-zero pre-layout.

### Where it's applied

In `on_arena_winner_update`, **Move arm** (`elements/scroll_view.rs:320-337`):

```rust
let delta = self.last_drag_y - position.y;
self.last_drag_y = position.y;

let raw_new = self.scroll_offset + delta;
let new_offset = apply_rubber_band(raw_new, self.viewport_height, self.max_scroll());
self.apply_scroll_offset(new_offset, /* ... */);
```

### Loosening the clamps

The resistance only works if `scroll_offset` is **allowed** to leave `[0, max]`. Two clamp
sites change:

1. **`ScrollViewElement::clamp_offset`** (`elements/scroll_view.rs:93-95`) — currently
   hard-clamps. **Removed.** `apply_scroll_offset` no longer calls it. Clamping to
   `[0, max]` happens only at settle points (programmatic `jump_to`, and the spring's `rest`
   target). The drag path applies its own resistance and produces the final offset directly.

2. **`ScrollViewRenderObject::apply_layout`** (`render_objects/scroll_view.rs:112-115`) —
   currently snaps `scroll_offset` to `max` on every layout. **Removed.** Layout passes
   during a bounce no longer fight the spring. `max_scroll` is still respected logically
   (via the spring's `rest = max`), just not as a layout side-effect.

### `apply_scroll_offset` changes

The method (`elements/scroll_view.rs:97-133`) currently refreshes viewport/content height,
clamps, returns false on no-change, stores, syncs, marks. New behavior:
1. Refresh viewport/content height (unchanged).
2. **No clamp** — store `scroll_offset` directly (may be out of bounds).
3. Return false if no change (unchanged).
4. Store, sync, mark (unchanged).

Signature unchanged. The only behavioral change: the offset can now be out of `[0, max]`.

### Edge case: viewport_height not yet known

On the very first drag before layout completes, `viewport_height` may be 0. The
`.max(1.0)` guard prevents division by zero — resistance is ~0 and content barely moves
past the edge. Acceptable (and matches pre-bounce behavior where a 0 viewport means
`max_scroll = 0`).

## Release & Fling-to-Edge Handoff

### Release in overscroll

In `on_arena_winner_update`, **Up arm** (`elements/scroll_view.rs:354-398`).

**Restructure note:** The current staleness guard (L362-368) is an early `return;` at the top
of the Up arm, *before* velocity is computed. This must be moved **into the in-bounds `else`
branch** below, so that releasing in overscroll always starts the spring regardless of
staleness. (A stale release in overscroll still needs to bounce back — the spring starts with
`v0 = 0` if velocity is stale/zero, and still pulls content to the edge.)

```rust
// NOTE: staleness guard moved INTO the in-bounds branch below (was an early return at L362-368).
let v = -self.velocity_tracker.velocity();   // tracker returns f32 directly (no .y)
let max = self.max_scroll();

if self.scroll_offset < 0.0 {
    // released past top → bounce back to 0 (always, even if stale)
    self.momentum.stop();
    self.spring.start(self.scroll_offset, v, /* rest */ 0.0, now, ...);
} else if self.scroll_offset > max {
    // released past bottom → bounce back to max (always, even if stale)
    self.momentum.stop();
    self.spring.start(self.scroll_offset, v, /* rest */ max, now, ...);
} else {
    // released in-bounds — existing fling behavior, gated by staleness + min velocity
    let is_stale = self.last_move_time
        .map(|t| Instant::now().duration_since(t) > Duration::from_millis(100))
        .unwrap_or(true);
    if !is_stale && v.abs() >= V_MIN_FLING {
        self.momentum.start(self.scroll_offset, v, now, ...);
    }
}
```

**Velocity carryover:** `v` (tracked flick velocity) is passed to `spring.start` as `v0`. A
hard flick back feels different from a gentle release: the spring's initial velocity is the
flick velocity. A critically-damped spring with high `v0` will reach the edge and stop
cleanly (no overshoot, by design). This is iOS behavior.

**Staleness guard:** The 100ms-pause-means-no-fling guard (L362-368) currently sits as an
early `return;` at the top of the Up arm. It must be **moved into the in-bounds `else`
branch** so it only gates the momentum fling path. For the overscroll path, we **always**
start the spring — even with `v0 ≈ 0` (stale/zero velocity), a critically-damped spring still
pulls content back to the edge. A stale release in overscroll produces a gentle spring-back
rather than leaving content stranded past the edge.

### Fling hits edge mid-flight

In `rebuild_from_state` (`elements/scroll_view.rs:491-499`):

```rust
if self.momentum.is_active() {
    let physics_offset = self.momentum.advance(now);
    if let Some(offset) = physics_offset {
        let max = self.max_scroll();
        let clamped = offset.clamp(0.0, max);
        let hit_edge = (clamped - offset).abs() > EPSILON;
        if hit_edge {
            // Momentum would cross the edge. Hand off to spring.
            let v = self.momentum.velocity();           // NEW accessor
            let rest = if offset < 0.0 { 0.0 } else { max };
            self.momentum.stop();
            self.spring.start(clamped, v, rest, now, ...);  // start FROM the edge with remaining v
        } else {
            // in-bounds — unchanged
            self.apply_scroll_offset(offset, ...);
        }
    }
}
```

**Two details:**

1. **`MomentumSimulation::velocity()` accessor** — today `advance` only returns offset. We
   add `pub fn velocity(&self) -> f32` (one-line read-only accessor; `v` is already tracked
   internally at `momentum.rs:20`).

2. **Spring starts at `clamped` (the edge), not at `offset` (past the edge).** Deliberate:
   momentum carried the content *to* the edge; the spring takes over with the remaining
   velocity and produces the *overshoot* (the bounce). The spring's `rest` is the edge, so
   it will overshoot once past the edge (due to `v0`) and come back — the correct
   fling-into-edge behavior. Overshoot magnitude is bounded by `v0` and the spring constants.

   **Tuning knob:** If the overshoot feels too aggressive during manual tuning, bleed some
   velocity at the handoff (`v *= 0.5` before passing to the spring). Start with full
   velocity carryover.

### Spring branch in `rebuild_from_state`

New parallel branch mirroring the momentum block, placed right after it:

```rust
if self.spring.is_active() {
    let physics_offset = self.spring.advance(now);
    match physics_offset {
        Some(offset) => {
            // spring still running — apply its offset directly (no clamp; spring handles rest)
            self.apply_scroll_offset(offset, ...);
        }
        None => {
            // settled — snap exactly to rest and stop
            let rest = self.spring.rest();
            self.spring.stop();
            self.apply_scroll_offset(rest, ...);
        }
    }
}
```

Since `momentum` and `spring` are mutually exclusive, only one of these two `if` blocks ever
runs per frame.

### Lifecycle stops

The spring is stopped (unregistering its ticker handle, mirroring `momentum.stop()`) in all
the same places momentum is stopped:

| Location | Event | Action |
|---|---|---|
| `on_event` PointerButton Pressed (L261-271) | User presses during bounce | `self.spring.stop()` (user grabs content mid-bounce) |
| `on_event` wheel/scroll (L273-277) | Wheel during bounce | `self.spring.stop()` (wheel takes over) |
| `on_event` keyboard scroll (L279-297) | Arrow/Page during bounce | `self.spring.stop()` |
| `rebuild_from_state` programmatic `jump_to` (L467) | `ScrollController::jump_to` | `self.spring.stop()` (programmatic jump overrides) |
| `unmount` (L227-240) | Element destroyed | `self.spring.stop()` (free ticker handle) |

One-line additions next to existing `self.momentum.stop()` calls.

## Error Handling & Edge Cases

### Degenerate geometries

| Case | Behavior |
|---|---|
| `content_height ≤ viewport_height` (`max_scroll = 0`) | Content fits, nothing to scroll. Drag applies resistance from offset 0 immediately. Spring `rest = 0`. Content rubber-bands and returns. Correct iOS behavior — a non-scrollable list still bounces. |
| `viewport_height = 0` (pre-layout, first frame) | Resistance `.max(1.0)` guard prevents div-by-zero. Content barely moves past edge. Spring `rest = 0`; if started, settles immediately. No crash. |
| `content_height = 0` (empty child) | `max_scroll = 0`. Same as first case. Drag bounces against offset 0. |
| Layout changes mid-bounce (content grows/shrinks) | `apply_scroll_offset` refreshes `max_scroll` each call. If the spring's `rest` is now out of `[0, max]`, the spring still settles toward the *old* rest, and `apply_layout` no longer hard-clamps, so the offset lands at the old rest and stays. Next user interaction snaps to the new bounds naturally. Acceptable; matches iOS which can also briefly show stale bounds after a content change. |

### Physics stability guards

| Guard | Location | Purpose |
|---|---|---|
| `MAX_FRAME_DT = 1/30s` clamp on `dt` | `spring.rs` `advance()` | After a window pause/tab switch, `dt` could be seconds. Clamping prevents integrator explosion. Mirrors `momentum.rs`. |
| `MAX_DURATION = 10s` hard stop | `spring.rs` `advance()` | If the spring never settles (shouldn't happen with critical damping, but defensive), force-stop after 10s. Returns `None`. Mirrors `momentum.rs`. |
| Substep `DT = 1/120s` | `spring.rs` `advance()` | Splits a 60fps frame (16.7ms) into two 8.3ms substeps. Keeps symplectic Euler stable for `STIFFNESS = 340`. At 120fps: one substep. At 30fps (clamped dt): four substeps. Frame-rate independent. |
| `X_SETTLE = 1.0px` + `V_SETTLE = 13px/s` | `spring.rs` settle check | Guarantees termination. A critically-damped spring asymptotically approaches rest; without a settle threshold it would run forever. |
| `viewport_height.max(1.0)` in resistance | drag Move arm | Div-by-zero guard. |

### Mutually-exclusive simulations

`momentum` and `spring` must never both be active. The invariant is enforced at every start
site:

- `spring.start(...)` → first call `self.momentum.stop()` (and vice versa where applicable —
  though momentum is only ever started from the in-bounds release path, where spring is
  already inactive).
- `momentum.start(...)` (in-bounds release) → spring is already inactive (in-bounds, and any
  prior spring was stopped on press/wheel/keyboard/jump).

A `debug_assert` catches violations during development:

```rust
debug_assert!(!(self.momentum.is_active() && self.spring.is_active()));
```

(At most one active; both inactive is fine.)

### Interaction edge cases

| Interaction | Expected behavior | How |
|---|---|---|
| Press during bounce | Stop spring immediately, begin drag from current offset. | `on_event` Pressed arm calls `spring.stop()`. `on_arena_winner_update` Down arm (L338-353) initializes `last_drag_y` from recognizer's down position, so drag continues smoothly from wherever the bounce was. |
| Wheel during bounce | Stop spring, apply wheel delta. | `on_event` wheel arm calls `spring.stop()` before `apply_scroll_offset`. |
| `jump_to_bottom()` during bounce | Stop spring, jump to max. | `rebuild_from_state` programmatic-jump path (L467) calls `spring.stop()` alongside existing `momentum.stop()`. |
| Fling hits edge, bounces, user catches it mid-bounce | Press stops spring, drag resumes. | Same as "press during bounce." Spring's current offset is the drag start point. |
| Very fast fling into edge | Spring overshoots once (bounded by `v0`), settles. | See "Fling hits edge mid-flight" above. Tuning knob: bleed velocity at handoff if too aggressive. |
| Tiny drag past edge (1px) then release | Spring starts with tiny offset + tiny v, settles in <100ms. | Spring math handles it; settle thresholds terminate quickly. |
| Drag past top, then drag back in-bounds without releasing | Content follows finger back through the edge with no resistance on the in-bounds portion. | Resistance is a pure function of `raw_new` each Move; once `raw_new ≥ 0`, `excess = 0`, resistance = 1.0, full delta applies. Smooth. |

### Float precision

All comparisons use the existing `EPSILON` constants from the scroll view module. The
`hit_edge` check (`(clamped - offset).abs() > EPSILON`) already handles float drift. Spring
settle uses its own `X_SETTLE`/`V_SETTLE` (generous, 1px/13px/s) so float noise doesn't
prevent termination.

## Testing Strategy

Testing is split into physics unit tests (pure, fast), element integration tests (mock
backend, verify state transitions), and manual visual verification (the only way to judge
"feel").

### Layer 1: SpringSimulation unit tests

New `#[cfg(test)] mod tests` in `spring.rs`. Tests the physics in isolation — no element, no
ticker, no RO. `start` is called with a mock `now`; `advance` is called with incremented
`now` values.

| Test | Verifies |
|---|---|
| `spring_settles_to_rest_from_offset` | Start `offset=-40, v0=0, rest=0`. Advance ~1s. Assert returns `None` and final offset ≈ 0 (within `X_SETTLE`). |
| `spring_settles_to_rest_with_initial_velocity` | Start `offset=-40, v0=500, rest=0`. Assert settles at 0. Velocity doesn't prevent settling. |
| `spring_does_not_overshoot_when_released_from_overscroll` | Start `offset=-40, v0=0, rest=0`. Sample all offsets during settle. Assert none go past 0 into positive (critical damping: no overshoot on release-from-overscroll). |
| `spring_overshoots_once_when_fling_hits_edge` | Start `offset=0 (edge), v0=800, rest=0` (fling handoff: spring starts AT edge with velocity AWAY from rest). Sample offsets. Assert exactly one sign change (overshoots into negative, returns to 0). |
| `spring_settle_time_under_one_second` | Start `offset=-100, v0=0, rest=0`. Count frames until `None`. Assert total time < 1s. |
| `spring_stops_immediately_after_stop_call` | Start, call `stop()`, assert `is_active() == false`. |
| `spring_handles_max_frame_dt` | Start, call `advance` with `dt = 2.0` (simulating a window pause). Assert no NaN, no explosion, still settles eventually. Verifies the `MAX_FRAME_DT` clamp. |
| `spring_terminates_at_max_duration` | (Defensive) Force `MAX_DURATION` hit. Assert returns `None`. Hard to trigger naturally; may skip if fragile. |

### Layer 2: Drag resistance unit tests

`apply_rubber_band` is a pure function, unit-testable directly.

| Test | Verifies |
|---|---|
| `resistance_no_resistance_in_bounds` | `apply_rubber_band(50, 400, 1000) == 50`. In-bounds delta passes through unchanged. |
| `resistance_no_resistance_at_exact_edge` | `apply_rubber_band(0, 400, 1000) == 0` and `apply_rubber_band(1000, 400, 1000) == 1000`. Touching the edge is free. |
| `resistance_shrinks_past_top` | `apply_rubber_band(-100, 400, 1000)`. Assert result in `(-100, 0)` — resisted, not raw. Specifically `result > -400` (can't exceed viewport past edge). |
| `resistance_shrinks_past_bottom` | `apply_rubber_band(1100, 400, 1000)`. Assert result in `(1000, 1100)` and `< 1400` (viewport bound). |
| `resistance_asymptotic_at_viewport` | `apply_rubber_band(-10000, 400, 1000)`. Assert `result > -400` (asymptote). Content can never be dragged more than ~viewport past the edge. |
| `resistance_symmetric_top_bottom` | `apply_rubber_band(-100, 400, 1000)` and `apply_rubber_band(1100, 400, 1000)` produce mirrored excess (same `|excess|`). |
| `resistance_zero_viewport_guarded` | `apply_rubber_band(-100, 0, 1000)` doesn't panic; result in `(-100, 0]`. |

### Layer 3: Element integration tests

Use the existing `MockBackend` (`render/mock_backend.rs`). Construct a `ScrollViewElement`
with a tall child, mount it, drive input events, assert state.

| Test | Verifies |
|---|---|
| `drag_past_top_goes_negative` | Mount, send pointer-down + drag-up past top. Assert `scroll_offset < 0`. |
| `drag_past_top_resists` | Same drag, assert resulting `scroll_offset` is *less negative* than raw delta would produce. |
| `release_past_top_starts_spring` | Drag past top, release. Assert `spring.is_active()` and `!momentum.is_active()`. |
| `release_in_bounds_starts_momentum_not_spring` | Drag within bounds, release with velocity. Assert `momentum.is_active()` and `!spring.is_active()`. |
| `spring_settles_to_edge` | Start in overscroll, release, pump frames. Assert `scroll_offset` returns to 0 (or max) and `!spring.is_active()` at end. |
| `fling_into_edge_starts_spring` | Build up velocity, fling toward edge, pump frames. When momentum hits edge, assert `spring.is_active()` and `!momentum.is_active()`. |
| `press_during_bounce_stops_spring` | Start a bounce, send pointer-down mid-bounce. Assert `!spring.is_active()`. |
| `wheel_during_bounce_stops_spring` | Start a bounce, send wheel event. Assert `!spring.is_active()`. |
| `jump_to_during_bounce_stops_spring` | Start a bounce, call `ScrollController::jump_to(0.0)`, pump frame. Assert `!spring.is_active()` and offset == 0. |
| `momentum_spring_mutually_exclusive` | After any operation, assert at most one of `momentum.is_active()` / `spring.is_active()`. |
| `unmount_stops_spring` | Start a bounce, unmount element. Assert no panic, no leaked ticker handle (ticker's `has_active()` returns false). |

### Layer 4: Manual visual verification

Physics "feel" cannot be tested automatically. Per CLAUDE.md, the desktop demo is **never
run by the agent** — instrument with `log::debug!` and give the user the run command.

```bash
# User runs:
RUST_LOG=vexo::animation::spring=debug,vexo::elements::scroll_view=debug cargo run -p desktop_demo 2>&1 | grep -E "spring|scroll" | tee /tmp/bounce.log
```

Tuning checklist for the user:

1. Drag chat list down past top → content rubber-bands, resistance increases with depth.
2. Release → content springs back to top, no overshoot, smooth settle.
3. Fling up hard from middle → content hits bottom, bounces once, settles at bottom.
4. Fling down hard from middle → content hits top, bounces once, settles at top.
5. Drag past top, then drag back down without releasing → content follows finger, no jerk at edge crossing.
6. Press mid-bounce → bounce stops, drag resumes from current position.
7. Wheel during bounce → bounce stops, wheel scroll takes over.
8. Short list (content < viewport) → dragging still bounces against top.
9. `jump_to_bottom()` during a bounce → bounce stops, jumps to bottom.
10. Leave window for 5s, return mid-bounce → no explosion (verify in log: `dt` clamped).

Tuning knobs logged for adjustment:
- `STIFFNESS` (higher = snappier return)
- `DAMPING_RATIO` (lower = wobblier, e.g. 0.8 for a bouncier feel)
- Velocity bleed at fling-to-edge handoff (if overshoot too aggressive)

### What's NOT tested

- Horizontal bounce (out of scope).
- Multi-touch / two-finger interactions (not in current scroll model).
- Precise pixel-match to iOS (feel is subjective; tuned by eye).
