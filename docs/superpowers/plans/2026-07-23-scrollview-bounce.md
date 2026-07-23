# ScrollView Bounce Effect Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add iOS-style rubber-band + spring-back bounce to ScrollView when dragging past edges, with fling-to-edge carryover.

**Architecture:** A new `SpringSimulation` (critically-damped harmonic oscillator) mirrors `MomentumSimulation`'s ticker/dirty-callback lifecycle. Drag past edge applies rubber-band resistance (canonical iOS curve). Release in overscroll starts a spring back to the edge. Fling hitting edge mid-flight hands off remaining velocity to the spring for one bounded overshoot. Two existing hard-clamp sites are removed so `scroll_offset` can leave `[0, max_scroll]` during bounce. The painter already handles arbitrary offsets (clip-before-offset order), so no rendering changes.

**Tech Stack:** Rust, wgpu, Taffy layout, glyphon text. No new dependencies. `AnimationTicker` infrastructure reused.

## Global Constraints

- Vertical-only bounce (matches current vertical-only ScrollView).
- Always-on, no opt-out, no new public API on `ScrollView`.
- `momentum` and `spring` simulations are mutually exclusive — starting one stops the other.
- Painter/clip/offset pipeline unchanged (already handles overscroll).
- Gesture arena, `VerticalDragRecognizer`, `VelocityTracker` unchanged.
- `ScrollController` externally unchanged.
- Spring constants are `const` in `spring.rs` for easy tuning.
- Per CLAUDE.md: never run `cargo run -p desktop_demo` — instrument with `log::debug!` and ask the user to run.

**Spec:** `docs/superpowers/specs/2026-07-23-scrollview-bounce-design.md`

---

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `vexo/src/animation/spring.rs` | **NEW** — `SpringSimulation` physics (critically-damped spring ODE, ticker-driven lifecycle) | T1 |
| `vexo/src/animation/mod.rs` | Re-export `SpringSimulation` | T1 |
| `vexo/src/animation/momentum.rs` | Add `velocity()` accessor for fling-to-edge handoff | T2 |
| `vexo/src/elements/scroll_view.rs` | Rubber-band helper, clamp loosening, drag resistance, release→spring, fling→spring handoff, spring lifecycle stops, `rebuild_from_state` spring branch | T3–T7 |
| `vexo/src/render_objects/scroll_view.rs` | Remove hard-clamp in `apply_layout` | T4 |

---

### Task 1: SpringSimulation Module

**Files:**
- Create: `vexo/src/animation/spring.rs`
- Modify: `vexo/src/animation/mod.rs:4,12` (add module + re-export)

**Interfaces:**
- Consumes: `AnimationTicker`, `TickHandle` from `vexo/src/animation/ticker.rs`; `ElementKey` from `vexo/src/id.rs`
- Produces: `SpringSimulation` struct with `new()`, `start(offset0, v0, rest, now, dirty_sender, element_id, ticker)`, `advance(now) -> Option<f32>`, `stop()`, `is_active()`, `velocity() -> f32`, `rest() -> f32`. Lifecycle mirrors `MomentumSimulation` exactly (same ticker-registration + dirty-callback + Drop guard pattern).

- [ ] **Step 1: Write the failing test for SpringSimulation construction and basic settle**

Create `vexo/src/animation/spring.rs` with only the test module and a minimal stub struct that won't compile (to verify tests fail first):

```rust
//! SpringSimulation — critically-damped harmonic oscillator, ticker-driven.
//!
//! Mirrors `MomentumSimulation`'s ownership pattern: holds a `TickHandle`
//! registered with `AnimationTicker`, plus a dirty callback that sends the
//! owning element's ID through the pipeline's mpsc channel. The element
//! drives each frame's offset write in `rebuild_from_state` via `advance`.

use std::sync::Arc;

use crate::animation::ticker::{AnimationTicker, TickHandle};
use crate::id::ElementKey;

/// Spring stiffness (k). Higher = snappier pull-back. iOS ~300-400.
const STIFFNESS: f32 = 340.0;
/// 1.0 = critically damped (no overshoot on release). <1.0 wobbly, >1.0 sluggish.
const DAMPING_RATIO: f32 = 1.0;
/// Below this distance from rest (px), the simulation considers itself settled.
const X_SETTLE: f32 = 1.0;
/// Below this velocity (px/s), the simulation considers itself settled.
const V_SETTLE: f32 = 13.0;
/// Fixed physics substep. Frame-rate independent integration.
const DT: f32 = 1.0 / 120.0;
/// Clamp dt after a window pause to prevent integrator explosion.
const MAX_FRAME_DT: f32 = 1.0 / 30.0;
/// Safety ceiling — normal springs never approach this.
const MAX_DURATION: f32 = 10.0;

pub struct SpringSimulation; // stub — will fail to compile tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn dummy_element_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    fn start_spring(
        offset0: f32,
        v0: f32,
        rest: f32,
    ) -> (
        SpringSimulation,
        Instant,
        mpsc::Receiver<ElementKey>,
        Arc<AnimationTicker>,
    ) {
        let (tx, rx) = mpsc::channel();
        let ticker = Arc::new(AnimationTicker::new());
        let mut sim = SpringSimulation::new();
        let now = Instant::now();
        sim.start(offset0, v0, rest, now, tx, dummy_element_key(), ticker.clone());
        (sim, now, rx, ticker)
    }

    /// Advance the simulation by `dt` seconds (simulated), returning the
    /// final offset. Pumps the spring in small substeps internally.
    fn advance_by(sim: &mut SpringSimulation, start: Instant, dt_secs: f32) -> Option<f32> {
        sim.advance(start + Duration::from_secs_f32(dt_secs))
    }

    #[test]
    fn new_is_inactive() {
        let sim = SpringSimulation::new();
        assert!(!sim.is_active());
    }

    #[test]
    fn start_activates_and_registers_with_ticker() {
        let (sim, _, _rx, ticker) = start_spring(-40.0, 0.0, 0.0);
        assert!(sim.is_active());
        assert!(ticker.has_active());
    }

    #[test]
    fn spring_settles_to_rest_from_offset() {
        let (mut sim, start, _rx, _ticker) = start_spring(-40.0, 0.0, 0.0);
        // Advance ~1 second of simulated time in chunks.
        let mut last = Some(0.0);
        for i in 1..=120 {
            last = advance_by(&mut sim, start, i as f32 * DT);
            if last.is_none() {
                break;
            }
        }
        assert!(last.is_none(), "spring should have settled within 1s");
        assert!(!sim.is_active(), "spring should be inactive after settling");
    }

    #[test]
    fn spring_final_offset_near_rest() {
        let (mut sim, start, _rx, _ticker) = start_spring(-100.0, 0.0, 0.0);
        let mut final_offset = 0.0;
        for i in 1..=500 {
            match advance_by(&mut sim, start, i as f32 * DT) {
                Some(o) => final_offset = o,
                None => break,
            }
        }
        assert!(
            (final_offset - 0.0).abs() < X_SETTLE,
            "final offset {} should be within {} of rest 0.0",
            final_offset,
            X_SETTLE
        );
    }

    #[test]
    fn spring_settles_to_rest_with_initial_velocity() {
        let (mut sim, start, _rx, _ticker) = start_spring(-40.0, 500.0, 0.0);
        for i in 1..=500 {
            if advance_by(&mut sim, start, i as f32 * DT).is_none() {
                break;
            }
        }
        assert!(!sim.is_active(), "should settle even with initial velocity");
    }

    #[test]
    fn spring_does_not_overshoot_when_released_from_overscroll() {
        // Release from overscroll (offset=-40, v0=0, rest=0).
        // Critical damping: should NOT cross past rest into positive territory.
        let (mut sim, start, _rx, _ticker) = start_spring(-40.0, 0.0, 0.0);
        let mut max_offset = -40.0;
        for i in 1..=500 {
            match advance_by(&mut sim, start, i as f32 * DT) {
                Some(o) => max_offset = max_offset.max(o),
                None => break,
            }
        }
        assert!(
            max_offset <= 0.0 + X_SETTLE,
            "critically-damped spring should not overshoot past rest; max was {}",
            max_offset
        );
    }

    #[test]
    fn spring_overshoots_once_when_fling_hits_edge() {
        // Fling handoff: spring starts AT edge (offset=0) with velocity AWAY
        // from rest (negative = past top edge). Should overshoot into negative,
        // then return to 0.
        let (mut sim, start, _rx, _ticker) = start_spring(0.0, -800.0, 0.0);
        let mut offsets = Vec::new();
        for i in 1..=500 {
            match advance_by(&mut sim, start, i as f32 * DT) {
                Some(o) => offsets.push(o),
                None => break,
            }
        }
        // Should have gone negative (overshoot).
        assert!(
            offsets.iter().any(|&o| o < -0.5),
            "spring should overshoot into negative; min was {}",
            offsets.iter().cloned().fold(0.0f32, f32::min)
        );
        // Should have settled at 0.
        assert!(!sim.is_active(), "spring should have settled");
    }

    #[test]
    fn spring_settle_time_under_one_second() {
        let (mut sim, start, _rx, _ticker) = start_spring(-100.0, 0.0, 0.0);
        let mut settle_time = 0.0;
        for i in 1..=1000 {
            if advance_by(&mut sim, start, i as f32 * DT).is_none() {
                settle_time = i as f32 * DT;
                break;
            }
        }
        assert!(
            settle_time < 1.0,
            "spring should settle in under 1s; took {}s",
            settle_time
        );
    }

    #[test]
    fn spring_stops_immediately_after_stop_call() {
        let (mut sim, _, _rx, ticker) = start_spring(-40.0, 0.0, 0.0);
        assert!(sim.is_active());
        sim.stop();
        assert!(!sim.is_active());
        assert!(!ticker.has_active());
        assert!(sim.advance(Instant::now()).is_none());
    }

    #[test]
    fn spring_handles_max_frame_dt() {
        // Simulate a window pause: advance by 2.0 seconds in one call.
        let (mut sim, start, _rx, _ticker) = start_spring(-40.0, 0.0, 0.0);
        let result = sim.advance(start + Duration::from_secs_f32(2.0));
        assert!(result.is_some(), "spring should survive large dt without NaN");
        // Continue and verify it still settles.
        for i in 1..=500 {
            if advance_by(&mut sim, start, 2.0 + i as f32 * DT).is_none() {
                break;
            }
        }
        assert!(!sim.is_active(), "spring should still settle after large dt");
    }

    #[test]
    fn dirty_callback_fires_on_start() {
        let (tx, rx) = mpsc::channel();
        let ticker = Arc::new(AnimationTicker::new());
        let mut sim = SpringSimulation::new();
        sim.start(-40.0, 0.0, 0.0, Instant::now(), tx, dummy_element_key(), ticker);
        assert!(rx.try_recv().is_ok(), "dirty callback should fire on start");
    }

    #[test]
    fn ticker_tick_fires_dirty_callback_after_start() {
        let (tx, rx) = mpsc::channel();
        let ticker = Arc::new(AnimationTicker::new());
        let mut sim = SpringSimulation::new();
        sim.start(
            -40.0,
            0.0,
            0.0,
            Instant::now(),
            tx,
            dummy_element_key(),
            ticker.clone(),
        );
        let _ = rx.try_recv(); // drain immediate fire from start()
        ticker.tick();
        assert!(
            rx.try_recv().is_ok(),
            "ticker.tick() should fire the registered callback"
        );
    }

    #[test]
    fn velocity_and_rest_accessors_work() {
        let (sim, _, _rx, _ticker) = start_spring(-40.0, 300.0, 10.0);
        assert!((sim.rest() - 10.0).abs() < 1e-3, "rest should be 10.0");
        // velocity right after start should be ~v0 (before any advance).
        assert!(
            sim.velocity().abs() > 0.0,
            "velocity should be non-zero right after start"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib animation::spring 2>&1 | head -30`
Expected: Compilation error — `SpringSimulation` is a unit struct without `new`, `start`, `advance`, etc.

- [ ] **Step 3: Implement SpringSimulation**

Replace the stub `pub struct SpringSimulation;` with the full implementation:

```rust
pub struct SpringSimulation {
    // Physics state
    offset: f32,
    velocity: f32,
    rest: f32,
    // Timing
    start_time: Option<std::time::Instant>,
    last_step: Option<std::time::Instant>,
    active: bool,
    // Framework plumbing (same pattern as MomentumSimulation)
    ticker: Option<Arc<AnimationTicker>>,
    tick_handle: Option<TickHandle>,
}

impl SpringSimulation {
    pub fn new() -> Self {
        Self {
            offset: 0.0,
            velocity: 0.0,
            rest: 0.0,
            start_time: None,
            last_step: None,
            active: false,
            ticker: None,
            tick_handle: None,
        }
    }

    /// Start the spring toward `rest` from `offset0` with initial velocity `v0`.
    /// Mirrors `MomentumSimulation::start` — registers a ticker callback and
    /// fires the dirty callback immediately so the element is scheduled for
    /// rebuild on this event-loop turn.
    pub fn start(
        &mut self,
        offset0: f32,
        v0: f32,
        rest: f32,
        now: std::time::Instant,
        dirty_sender: std::sync::mpsc::Sender<ElementKey>,
        element_id: ElementKey,
        ticker: Arc<AnimationTicker>,
    ) {
        self.stop(); // drop any prior registration
        self.offset = offset0;
        self.velocity = v0;
        self.rest = rest;
        self.start_time = Some(now);
        self.last_step = Some(now);
        self.active = true;
        self.ticker = Some(ticker.clone());
        let dirty_sender_for_cb = dirty_sender.clone();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = dirty_sender_for_cb.send(element_id);
        });
        self.tick_handle = Some(ticker.register(cb));
        // Fire dirty callback immediately — mirrors MomentumSimulation::start.
        let _ = dirty_sender.send(element_id);
    }

    /// Advance the simulation. Returns `Some(offset)` while the spring is
    /// still settling; `None` once it has settled (within X_SETTLE / V_SETTLE).
    pub fn advance(&mut self, now: std::time::Instant) -> Option<f32> {
        if !self.active {
            return None;
        }
        let start = match self.start_time {
            Some(t) => t,
            None => return None,
        };
        let last = match self.last_step {
            Some(t) => t,
            None => return None,
        };

        // MAX_DURATION safety check.
        let total_dt = now.saturating_duration_since(start).as_secs_f32() as f32;
        if total_dt > MAX_DURATION {
            self.terminate();
            return None;
        }

        // Clamp frame dt to prevent integrator explosion after a pause.
        let mut frame_dt = now.saturating_duration_since(last).as_secs_f32() as f32;
        if frame_dt > MAX_FRAME_DT {
            frame_dt = MAX_FRAME_DT;
        }

        // Substep integration (semi-implicit / symplectic Euler).
        let damping = 2.0 * (STIFFNESS * DAMPING_RATIO).sqrt();
        let mut remaining = frame_dt;
        while remaining > 0.0 {
            let step = remaining.min(DT);
            let a = -STIFFNESS * (self.offset - self.rest) - damping * self.velocity;
            self.velocity += a * step;
            self.offset += self.velocity * step;
            remaining -= step;
        }
        self.last_step = Some(now);

        // Settle check.
        if (self.offset - self.rest).abs() < X_SETTLE && self.velocity.abs() < V_SETTLE {
            self.terminate();
            return None;
        }
        Some(self.offset)
    }

    pub fn stop(&mut self) {
        self.cleanup();
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    pub fn rest(&self) -> f32 {
        self.rest
    }

    fn terminate(&mut self) {
        self.cleanup();
    }

    fn cleanup(&mut self) {
        self.active = false;
        self.start_time = None;
        self.last_step = None;
        if let (Some(ticker), Some(handle)) = (self.ticker.clone(), self.tick_handle.take()) {
            ticker.unregister(handle);
        }
    }
}

impl Default for SpringSimulation {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SpringSimulation {
    fn drop(&mut self) {
        if self.tick_handle.is_some() {
            self.stop();
        }
    }
}
```

- [ ] **Step 4: Add module + re-export in `vexo/src/animation/mod.rs`**

In `vexo/src/animation/mod.rs`, add `pub mod spring;` (after `pub mod momentum;` on line 4) and add `pub use spring::SpringSimulation;` (after `pub use momentum::MomentumSimulation;` on line 12).

The file should look like:

```rust
pub mod controller;
pub mod curve;
pub mod momentum;
pub mod spring;
pub mod ticker;
pub mod tween;

pub use controller::{AnimationController, AnimationDirection};
pub use curve::{
    CubicBezierCurve, Curve, CurvedAnimation, EaseInCurve, EaseInOutCurve, EaseOutCurve,
    LinearCurve,
};
pub use momentum::MomentumSimulation;
pub use spring::SpringSimulation;
pub use ticker::{AnimationTicker, TickHandle};
pub use tween::{ColorTween, FloatTween, Tween};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vexo --lib animation::spring 2>&1 | tail -20`
Expected: All 14 spring tests PASS.

- [ ] **Step 6: Run full crate build + test to verify no regressions**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: BUILD SUCCEEDS.

Run: `cargo test -p vexo --lib 2>&1 | tail -10`
Expected: All existing tests still PASS (spring module is additive, nothing else changed).

- [ ] **Step 7: Commit**

```bash
git add vexo/src/animation/spring.rs vexo/src/animation/mod.rs
git commit -m "feat: add SpringSimulation physics module

Critically-damped harmonic oscillator mirroring MomentumSimulation's
ticker/dirty-callback lifecycle. Integrates via semi-implicit Euler with
fixed 1/120s substeps. Settles within X_SETTLE/V_SETTLE thresholds.
Constants tuned for iOS-like feel (STIFFNESS=340, DAMPING_RATIO=1.0)."
```

---

### Task 2: MomentumSimulation Velocity Accessor

**Files:**
- Modify: `vexo/src/animation/momentum.rs:20-27,101-103` (add field tracking + accessor)

**Interfaces:**
- Consumes: existing `MomentumSimulation` internals
- Produces: `pub fn velocity(&self) -> f32` on `MomentumSimulation` — returns the current velocity of the fling at the last `advance` call. Needed by Task 7 for fling-to-edge handoff.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `vexo/src/animation/momentum.rs`, right before the closing `}` of the test module (after `ticker_tick_fires_dirty_callback_after_start`):

```rust
    #[test]
    fn velocity_accessor_returns_current_velocity() {
        let (mut sim, now, _rx, _ticker) = start_sim(1000.0);
        // Before advance, velocity should be v0 (start sets offset0=0, v0=1000).
        let v_before = sim.velocity();
        assert!(
            (v_before - 1000.0).abs() < 1.0,
            "velocity before advance should be ~v0 (1000); got {}",
            v_before
        );
        // After advancing, velocity decays.
        let later = now + Duration::from_millis(100);
        let _ = sim.advance(later);
        let v_after = sim.velocity();
        assert!(
            v_after < v_before,
            "velocity should decay after advancing; got {} -> {}",
            v_before,
            v_after
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib animation::momentum::tests::velocity_accessor_returns_current_velocity 2>&1 | tail -10`
Expected: FAIL with "no method named `velocity` found for struct `MomentumSimulation`".

- [ ] **Step 3: Implement the velocity tracking + accessor**

In `vexo/src/animation/momentum.rs`, add a `current_velocity` field to the struct (after `v0` on line 22):

```rust
pub struct MomentumSimulation {
    offset0: f32,
    v0: f32,
    current_velocity: f32,
    start_time: Option<std::time::Instant>,
    active: bool,
    ticker: Option<Arc<AnimationTicker>>,
    tick_handle: Option<TickHandle>,
}
```

Initialize it in `new()` (after `v0: 0.0,`):

```rust
            v0: 0.0,
            current_velocity: 0.0,
```

Set it in `start()` (after `self.v0 = v0;`):

```rust
        self.v0 = v0;
        self.current_velocity = v0;
```

Update it in `advance()` — replace the existing velocity computation block. Find these lines in `advance`:

```rust
        let v = self.v0 * (-dt / TAU).exp();
        if v.abs() < V_STOP {
            self.terminate();
            return None;
        }
```

Replace with:

```rust
        let v = self.v0 * (-dt / TAU).exp();
        self.current_velocity = v;
        if v.abs() < V_STOP {
            self.terminate();
            return None;
        }
```

Add the accessor method (after `is_active`, around line 103):

```rust
    pub fn velocity(&self) -> f32 {
        self.current_velocity
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo --lib animation::momentum::tests::velocity_accessor_returns_current_velocity 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 5: Run full momentum test suite to verify no regressions**

Run: `cargo test -p vexo --lib animation::momentum 2>&1 | tail -10`
Expected: All momentum tests PASS (11 existing + 1 new = 12).

- [ ] **Step 6: Commit**

```bash
git add vexo/src/animation/momentum.rs
git commit -m "feat: add velocity() accessor to MomentumSimulation

Exposes the current fling velocity for the fling-to-edge spring handoff
(Task 7). Updates current_velocity in advance() alongside the existing
decay computation."
```

---

### Task 3: Rubber-Band Helper Function

**Files:**
- Modify: `vexo/src/elements/scroll_view.rs:24-37` (add helper function after `LINE_HEIGHT` const)

**Interfaces:**
- Consumes: nothing (pure function)
- Produces: `fn apply_rubber_band(raw_new: f32, viewport: f32, max: f32) -> f32` — private free function. Applies iOS rubber-band resistance to the out-of-bounds portion of `raw_new`. Used by Task 4 in the Move arm.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `vexo/src/elements/scroll_view.rs`, right after the `test_clipboard` helper (after line 539, before `setup_scroll_view`):

```rust
    #[test]
    fn test_rubber_band_no_resistance_in_bounds() {
        assert_eq!(apply_rubber_band(50.0, 400.0, 1000.0), 50.0);
    }

    #[test]
    fn test_rubber_band_no_resistance_at_exact_edge() {
        assert_eq!(apply_rubber_band(0.0, 400.0, 1000.0), 0.0);
        assert_eq!(apply_rubber_band(1000.0, 400.0, 1000.0), 1000.0);
    }

    #[test]
    fn test_rubber_band_shrinks_past_top() {
        let result = apply_rubber_band(-100.0, 400.0, 1000.0);
        assert!(result < 0.0, "should be past top (negative); got {}", result);
        assert!(result > -100.0, "should be resisted (less negative than raw); got {}", result);
        assert!(result > -400.0, "should not exceed viewport past edge; got {}", result);
    }

    #[test]
    fn test_rubber_band_shrinks_past_bottom() {
        let result = apply_rubber_band(1100.0, 400.0, 1000.0);
        assert!(result > 1000.0, "should be past bottom; got {}", result);
        assert!(result < 1100.0, "should be resisted (less than raw); got {}", result);
        assert!(result < 1400.0, "should not exceed viewport past edge; got {}", result);
    }

    #[test]
    fn test_rubber_band_asymptotic_at_viewport() {
        let result = apply_rubber_band(-10000.0, 400.0, 1000.0);
        assert!(
            result > -400.0,
            "content can never be dragged more than ~viewport past edge; got {}",
            result
        );
    }

    #[test]
    fn test_rubber_band_symmetric_top_bottom() {
        let top_result = apply_rubber_band(-100.0, 400.0, 1000.0);
        let bottom_result = apply_rubber_band(1100.0, 400.0, 1000.0);
        let top_excess = top_result.abs();
        let bottom_excess = (bottom_result - 1000.0).abs();
        assert!(
            (top_excess - bottom_excess).abs() < 0.01,
            "top and bottom excess should be symmetric; got top={} bottom={}",
            top_excess,
            bottom_excess
        );
    }

    #[test]
    fn test_rubber_band_zero_viewport_guarded() {
        // Should not panic on div-by-zero.
        let result = apply_rubber_band(-100.0, 0.0, 1000.0);
        assert!(result < 0.0, "should still be past top; got {}", result);
        assert!(result >= -100.0, "should not move more than raw; got {}", result);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_rubber_band 2>&1 | tail -10`
Expected: FAIL with "cannot find function `apply_rubber_band`".

- [ ] **Step 3: Implement the helper function**

In `vexo/src/elements/scroll_view.rs`, add the function after the `LINE_HEIGHT` constant (line 24), before the `wire_dirty_callback` function:

```rust
/// Apply iOS-style rubber-band resistance to a scroll offset.
///
/// When `raw_new` is within `[0, max]`, it passes through unchanged.
/// When past an edge, the over-edge portion is scaled by decreasing
/// resistance: `resistance = 1 - overscroll / (overscroll + viewport)`.
/// Content asymptotically approaches one viewport past the edge but
/// can never exceed it.
fn apply_rubber_band(raw_new: f32, viewport: f32, max: f32) -> f32 {
    let (base, excess) = if raw_new < 0.0 {
        (0.0, raw_new)
    } else if raw_new > max {
        (max, raw_new - max)
    } else {
        (raw_new, 0.0)
    };

    let overscroll = excess.abs();
    let resistance = 1.0 - overscroll / (overscroll + viewport.max(1.0));
    let resisted_excess = excess.signum() * overscroll * resistance;

    base + resisted_excess
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_rubber_band 2>&1 | tail -10`
Expected: All 7 rubber-band tests PASS.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/elements/scroll_view.rs
git commit -m "feat: add apply_rubber_band helper for overscroll resistance

Pure function implementing the canonical iOS rubber-band curve:
applied_delta = raw_delta * (1 - overscroll / (overscroll + viewport)).
Content asymptotically approaches one viewport past the edge. Unit-tested
in isolation; not yet wired into the drag handler."
```

---

### Task 4: Loosen Clamps + Wire Rubber-Band into Drag

**Files:**
- Modify: `vexo/src/elements/scroll_view.rs:93-95` (remove `clamp_offset`), `97-133` (remove clamp call in `apply_scroll_offset`), `320-337` (wire rubber-band into Move arm), `622-643` (remove clamp tests)
- Modify: `vexo/src/render_objects/scroll_view.rs:112-115` (remove hard-clamp in `apply_layout`)
- Modify: `vexo/src/elements/scroll_view.rs:842-894` (update `test_drag_clamps_at_top_with_arena` to expect overscroll)

**Interfaces:**
- Consumes: `apply_rubber_band` from Task 3
- Produces: `scroll_offset` can now leave `[0, max_scroll]`. `apply_scroll_offset` no longer clamps. `ScrollViewRenderObject::apply_layout` no longer hard-clamps. The Move arm applies rubber-band resistance.

- [ ] **Step 1: Write the failing integration test for drag past top going negative**

Add to the `#[cfg(test)] mod tests` block in `vexo/src/elements/scroll_view.rs`, after the `test_rubber_band_zero_viewport_guarded` test:

```rust
    #[test]
    fn test_drag_past_top_goes_negative() {
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::sync::Arc;

        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
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
        // Drag DOWN 200px (past slop, past top edge). Finger moves down →
        // scroll toward top → offset goes negative (overscroll).
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 500.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 500.0),
            &move_evt,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert!(
            ctrl.current_offset() < 0.0,
            "drag past top should produce negative offset (overscroll); got {}",
            ctrl.current_offset()
        );
    }

    #[test]
    fn test_drag_past_top_resists() {
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::sync::Arc;

        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
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
        // Drag DOWN 2000px (way past top). Without resistance, offset would
        // be -2000. With rubber-band, it should be much less (asymptote at
        // ~viewport=600).
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 2300.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 2300.0),
            &move_evt,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let offset = ctrl.current_offset();
        assert!(offset < 0.0, "should be past top; got {}", offset);
        assert!(
            offset > -600.0,
            "should not exceed ~viewport past edge (rubber-band); got {}",
            offset
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_drag_past_top 2>&1 | tail -10`
Expected: FAIL — `ctrl.current_offset()` is 0.0 (hard-clamped), not negative.

- [ ] **Step 3: Remove `clamp_offset` method and update `apply_scroll_offset`**

In `vexo/src/elements/scroll_view.rs`, remove the `clamp_offset` method (lines 93-95):

```rust
    fn clamp_offset(&self, offset: f32) -> f32 {
        offset.clamp(0.0, self.max_scroll())
    }
```

In `apply_scroll_offset` (around line 109), replace:

```rust
        let clamped = self.clamp_offset(new_offset);
        if (clamped - self.scroll_offset).abs() < f32::EPSILON {
            return false;
        }
        self.scroll_offset = clamped;
```

with:

```rust
        if (new_offset - self.scroll_offset).abs() < f32::EPSILON {
            return false;
        }
        self.scroll_offset = new_offset;
```

Then update the remaining references to `clamped` in the same method. Replace:

```rust
        if let Some(ctrl) = self.controller.as_ref() {
            ctrl.set_current_offset(clamped);
        }
```

with:

```rust
        if let Some(ctrl) = self.controller.as_ref() {
            ctrl.set_current_offset(new_offset);
        }
```

And replace:

```rust
                        svro.set_scroll_offset(clamped);
```

with:

```rust
                        svro.set_scroll_offset(new_offset);
```

- [ ] **Step 4: Remove hard-clamp in `ScrollViewRenderObject::apply_layout`**

In `vexo/src/render_objects/scroll_view.rs`, remove lines 112-115 in `apply_layout`:

```rust
        let max = self.max_scroll();
        if self.scroll_offset.get() > max {
            self.scroll_offset.set(max);
        }
```

The `apply_layout` method should end after the content_size assignment (the `if let Some(child_node)` block), with no clamp.

- [ ] **Step 5: Wire `apply_rubber_band` into the Move arm**

In `vexo/src/elements/scroll_view.rs`, in `on_arena_winner_update`, Move arm (around line 333-336), replace:

```rust
                let delta = self.last_drag_y - position.y;
                self.last_drag_y = position.y;
                let new_offset = self.scroll_offset + delta;
                self.apply_scroll_offset(new_offset, ctx);
```

with:

```rust
                let delta = self.last_drag_y - position.y;
                self.last_drag_y = position.y;
                let raw_new = self.scroll_offset + delta;
                let new_offset =
                    apply_rubber_band(raw_new, self.viewport_height, self.max_scroll());
                self.apply_scroll_offset(new_offset, ctx);
```

- [ ] **Step 6: Remove the three `clamp_offset` unit tests and update `test_drag_clamps_at_top_with_arena`**

In `vexo/src/elements/scroll_view.rs`, remove these three tests entirely (they test the removed `clamp_offset` method):

- `test_clamp_offset_at_zero` (lines 622-626)
- `test_clamp_offset_at_max` (lines 628-634)
- `test_no_scroll_when_content_fits` (lines 636-643)

Then update `test_drag_clamps_at_top_with_arena` (around line 842). Replace its assertion:

```rust
        assert_eq!(ctrl.current_offset(), 0.0, "clamped at top");
```

with:

```rust
        // With bounce enabled, dragging past top produces overscroll (negative
        // offset) rather than clamping at 0. The rubber-band resistance keeps
        // it bounded (~viewport past edge).
        let offset = ctrl.current_offset();
        assert!(offset <= 0.0, "should be at or past top; got {}", offset);
        assert!(
            offset > -600.0,
            "should not exceed ~viewport past edge; got {}",
            offset
        );
```

- [ ] **Step 7: Run the new + updated tests to verify they pass**

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_drag_past_top 2>&1 | tail -10`
Expected: Both `test_drag_past_top_goes_negative` and `test_drag_past_top_resists` PASS.

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_drag_clamps_at_top_with_arena 2>&1 | tail -5`
Expected: PASS (updated assertion).

- [ ] **Step 8: Run full scroll view test suite to verify no regressions**

Run: `cargo test -p vexo --lib elements::scroll_view 2>&1 | tail -15`
Expected: All tests PASS. (Existing tests like `test_mouse_wheel_still_works`, `test_multi_move_drag_accumulates_scroll`, `test_fling_scrolls_after_release` still pass — wheel and keyboard still clamp via the resistance function when past edges, and in-bounds deltas pass through unchanged.)

- [ ] **Step 9: Commit**

```bash
git add vexo/src/elements/scroll_view.rs vexo/src/render_objects/scroll_view.rs
git commit -m "feat: loosen scroll clamps + wire rubber-band into drag

- Remove clamp_offset method and its 3 unit tests
- Remove clamp call in apply_scroll_offset (offset can now leave [0, max])
- Remove hard-clamp in ScrollViewRenderObject::apply_layout
- Wire apply_rubber_band into Move arm for drag-past-edge resistance
- Update test_drag_clamps_at_top_with_arena to expect overscroll
- Add integration tests for drag past top going negative + resistance"
```

---

### Task 5: Spring Field + Lifecycle Stops + Rebuild Branch

**Files:**
- Modify: `vexo/src/elements/scroll_view.rs:7,39-67,83-85` (add import, field, init), `227-240,261-271,273-277,279-297,467` (add `spring.stop()` calls), `491-530` (add spring branch in `rebuild_from_state`)

**Interfaces:**
- Consumes: `SpringSimulation` from Task 1
- Produces: `ScrollViewElement` has a `spring: SpringSimulation` field. All lifecycle stops (`press`, `wheel`, `keyboard`, `jump_to`, `unmount`) also stop the spring. `rebuild_from_state` has a spring branch that advances the spring each frame. But nothing starts the spring yet (that's Tasks 6-7).

- [ ] **Step 1: Add the `spring` field and import**

In `vexo/src/elements/scroll_view.rs`, update the import on line 7:

```rust
use crate::animation::{AnimationTicker, MomentumSimulation, SpringSimulation};
```

Add the field to `ScrollViewElement` (after `momentum: MomentumSimulation,` on line 57):

```rust
    momentum: MomentumSimulation,
    /// Critically-damped spring for bounce-back. Mutually exclusive with
    /// `momentum` — starting one stops the other. Stepped in
    /// `rebuild_from_state` while `is_active()`.
    spring: SpringSimulation,
```

Initialize it in `new()` (after `momentum: MomentumSimulation::new(),` on line 83):

```rust
            momentum: MomentumSimulation::new(),
            spring: SpringSimulation::new(),
```

- [ ] **Step 2: Add `spring.stop()` to all lifecycle stops**

In `on_event`, PointerButton Pressed arm (around line 267), add after `self.momentum.stop();`:

```rust
                self.momentum.stop();
                self.spring.stop();
```

In `on_event`, Scroll arm (around line 273), add `self.spring.stop()` before `let new_offset`:

```rust
            InputEvent::Scroll { delta, .. } => {
                self.momentum.stop();
                self.spring.stop();
                let new_offset = self.scroll_offset - delta.y;
                self.apply_scroll_offset(new_offset, context);
                return Some(Box::new(()));
            }
```

In `on_event`, Keyboard arm (around line 279), add `self.spring.stop()` at the top of the match arm:

```rust
            InputEvent::Keyboard {
                key,
                state: ButtonState::Pressed,
                ..
            } => {
                self.momentum.stop();
                self.spring.stop();
                let delta = match key {
```

In `rebuild_from_state`, programmatic jump path (around line 467), add after `self.momentum.stop();`:

```rust
            self.momentum.stop();
            self.spring.stop();
```

In `unmount` (around line 232), add after `self.momentum.stop();`:

```rust
        self.momentum.stop();
        self.spring.stop();
```

- [ ] **Step 3: Add spring branch in `rebuild_from_state`**

In `vexo/src/elements/scroll_view.rs`, in `rebuild_from_state`, after the momentum `if` block (which ends around line 525) and before `if let Some(ro_key)` (line 527), add:

```rust
        if self.spring.is_active() {
            let now = Instant::now();
            match self.spring.advance(now) {
                Some(physics_offset) => {
                    if let Some(ro_key) = self.render_object {
                        if let Some(svro) = context
                            .render_objects
                            .get(ro_key)
                            .and_then(|ro| ro.as_any().downcast_ref::<ScrollViewRenderObject>())
                        {
                            self.viewport_height = svro.viewport_size().height;
                            self.content_height = svro.content_size().height;
                            svro.set_scroll_offset(physics_offset);
                        }
                    }
                    self.scroll_offset = physics_offset;
                    if let Some(ctrl) = self.controller.as_ref() {
                        ctrl.set_current_offset(physics_offset);
                    }
                }
                None => {
                    // Settled — snap exactly to rest and stop.
                    let rest = self.spring.rest();
                    self.spring.stop();
                    if let Some(ro_key) = self.render_object {
                        if let Some(svro) = context
                            .render_objects
                            .get(ro_key)
                            .and_then(|ro| ro.as_any().downcast_ref::<ScrollViewRenderObject>())
                        {
                            self.viewport_height = svro.viewport_size().height;
                            self.content_height = svro.content_size().height;
                            svro.set_scroll_offset(rest);
                        }
                    }
                    self.scroll_offset = rest;
                    if let Some(ctrl) = self.controller.as_ref() {
                        ctrl.set_current_offset(rest);
                    }
                }
            }
        }
```

- [ ] **Step 4: Add a debug_assert for mutual exclusivity**

In `rebuild_from_state`, at the very top of the method (after the opening `{` on line 452), add:

```rust
        debug_assert!(
            !(self.momentum.is_active() && self.spring.is_active()),
            "momentum and spring must not be active simultaneously"
        );
```

- [ ] **Step 5: Build and run full test suite to verify no regressions**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: BUILD SUCCEEDS.

Run: `cargo test -p vexo --lib elements::scroll_view 2>&1 | tail -15`
Expected: All tests PASS. (The spring is never started, so the spring branch in `rebuild_from_state` never executes. The `spring.stop()` calls are no-ops when spring is inactive. The `debug_assert` passes because both are inactive.)

- [ ] **Step 6: Commit**

```bash
git add vexo/src/elements/scroll_view.rs
git commit -m "feat: add spring field, lifecycle stops, and rebuild_from_state branch

Plumbing for bounce-back: adds SpringSimulation field to ScrollViewElement,
stops the spring alongside momentum in all lifecycle events (press, wheel,
keyboard, jump_to, unmount), and adds a spring branch in rebuild_from_state
that advances the spring each frame. Nothing starts the spring yet — that
comes in Tasks 6-7. Includes debug_assert for mutual exclusivity."
```

---

### Task 6: Release in Overscroll → Spring

**Files:**
- Modify: `vexo/src/elements/scroll_view.rs:354-398` (restructure Up arm)

**Interfaces:**
- Consumes: `SpringSimulation` from Task 1, `spring` field from Task 5, loosened clamps from Task 4
- Produces: When the user releases in overscroll (offset < 0 or offset > max), a spring starts toward the nearest edge. The staleness guard is moved into the in-bounds `else` branch.

- [ ] **Step 1: Write the failing integration tests**

Add to the `#[cfg(test)] mod tests` block in `vexo/src/elements/scroll_view.rs`, after the `test_drag_past_top_resists` test:

```rust
    #[test]
    fn test_release_past_top_starts_spring() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Press + drag down past top (overscroll).
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 300.0), &press);
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 500.0),
        };
        dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 500.0), &move_evt);
        assert!(ctrl.current_offset() < 0.0, "should be in overscroll");

        // Release.
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 500.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 500.0), &release);

        // Spring should be active (ticker has registrations).
        pump(&ticker, &mut pipeline);
        assert!(
            ticker.has_active(),
            "spring should be active after releasing in overscroll"
        );
    }

    #[test]
    fn test_release_in_bounds_starts_momentum_not_spring() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Press + fast drag up (in-bounds, builds velocity).
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 400.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 400.0), &press);
        for &y in &[350.0, 250.0, 150.0] {
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, y),
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, y), &mv);
        }
        assert!(ctrl.current_offset() > 0.0, "should have scrolled");

        // Release in-bounds.
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 150.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 150.0), &release);

        // Momentum should be active (not spring — this is the existing fling path).
        pump(&ticker, &mut pipeline);
        assert!(
            ticker.has_active(),
            "momentum should be active after in-bounds release with velocity"
        );
    }

    #[test]
    fn test_spring_settles_to_top_edge() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Press + drag down past top.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 300.0), &press);
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 500.0),
        };
        dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 500.0), &move_evt);

        // Release.
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 500.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 500.0), &release);

        // Pump until spring settles (ticker goes quiet).
        for _ in 0..2000 {
            pump(&ticker, &mut pipeline);
            if !ticker.has_active() {
                break;
            }
        }
        assert!(
            !ticker.has_active(),
            "spring should have settled"
        );
        assert_eq!(
            ctrl.current_offset(),
            0.0,
            "spring should settle exactly at top edge (0.0); got {}",
            ctrl.current_offset()
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_release_past_top_starts_spring 2>&1 | tail -10`
Expected: FAIL — spring is never started (ticker.has_active() is false after release in overscroll).

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_release_in_bounds_starts_momentum_not_spring 2>&1 | tail -5`
Expected: PASS (existing behavior — this is a regression guard).

- [ ] **Step 3: Restructure the Up arm to start spring on overscroll release**

In `vexo/src/elements/scroll_view.rs`, in `on_arena_winner_update`, replace the entire `ArenaEvent::Up { .. }` arm (lines 354-398) with:

```rust
            ArenaEvent::Up { .. } => {
                // Sign-flip: the tracker returns pointer-space dy/dt (y-down).
                // The Move handler does `delta = last_drag_y - position.y`
                // (negates pointer delta), so negate tracker velocity so
                // positive v0 = offset increases = scrolls toward bottom.
                let v = -self.velocity_tracker.velocity();
                let max = self.max_scroll();

                if self.scroll_offset < 0.0 {
                    // Released past top → bounce back to 0. Always start the
                    // spring, even with zero velocity — a critically-damped
                    // spring still pulls content back to the edge.
                    let now = Instant::now();
                    let Some(element_id) = self.id else {
                        return;
                    };
                    let Some(tx) = ctx.dirty_sender().cloned() else {
                        return;
                    };
                    let Some(ticker) = self.animation_ticker.clone() else {
                        return;
                    };
                    self.momentum.stop();
                    self.spring.start(
                        self.scroll_offset,
                        v,
                        0.0,
                        now,
                        tx,
                        element_id,
                        ticker,
                    );
                } else if self.scroll_offset > max {
                    // Released past bottom → bounce back to max.
                    let now = Instant::now();
                    let Some(element_id) = self.id else {
                        return;
                    };
                    let Some(tx) = ctx.dirty_sender().cloned() else {
                        return;
                    };
                    let Some(ticker) = self.animation_ticker.clone() else {
                        return;
                    };
                    self.momentum.stop();
                    self.spring.start(
                        self.scroll_offset,
                        v,
                        max,
                        now,
                        tx,
                        element_id,
                        ticker,
                    );
                } else {
                    // Released in-bounds — existing fling behavior, gated by
                    // staleness + minimum velocity. The staleness guard lives
                    // HERE (not at the top of the Up arm) because releasing
                    // in overscroll should always start the spring, even if
                    // the last move was stale.
                    let is_stale = self
                        .last_move_time
                        .map(|t| Instant::now().duration_since(t) > Duration::from_millis(100))
                        .unwrap_or(true);
                    if is_stale {
                        return;
                    }
                    const V_MIN_FLING: f32 = 50.0;
                    if v.abs() < V_MIN_FLING {
                        return;
                    }
                    let Some(element_id) = self.id else {
                        return;
                    };
                    let Some(tx) = ctx.dirty_sender().cloned() else {
                        return;
                    };
                    let Some(ticker) = self.animation_ticker.clone() else {
                        return;
                    };
                    self.momentum.start(
                        self.scroll_offset,
                        v,
                        Instant::now(),
                        tx,
                        element_id,
                        ticker,
                    );
                }
            }
```

- [ ] **Step 4: Run the new tests to verify they pass**

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_release_past_top_starts_spring 2>&1 | tail -5`
Expected: PASS.

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_release_in_bounds_starts_momentum_not_spring 2>&1 | tail -5`
Expected: PASS.

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_spring_settles_to_top_edge 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 5: Run full test suite to verify no regressions**

Run: `cargo test -p vexo --lib elements::scroll_view 2>&1 | tail -15`
Expected: All tests PASS. Key existing tests to verify:
- `test_fling_scrolls_after_release` — still works (in-bounds release → momentum)
- `test_pause_then_lift_no_momentum` — staleness guard now in the else branch, still prevents fling on stale release
- `test_slow_drag_no_momentum` — slow drag in-bounds, staleness/velocity guard still prevents fling

- [ ] **Step 6: Commit**

```bash
git add vexo/src/elements/scroll_view.rs
git commit -m "feat: release in overscroll starts spring-back

Restructure Up arm: if released past top/bottom edge, start a spring
toward the edge with the tracked velocity as v0. Staleness guard moved
into the in-bounds else branch (overscroll release always starts spring,
even with stale/zero velocity). Integration tests: release past top
starts spring, release in-bounds starts momentum, spring settles to edge."
```

---

### Task 7: Fling-to-Edge Handoff + Spring Branch Integration Tests

**Files:**
- Modify: `vexo/src/elements/scroll_view.rs:491-525` (momentum hit_edge → spring handoff)
- Modify: `vexo/src/elements/scroll_view.rs:1417-1554` (update fling-clamp tests for bounce settle)

**Interfaces:**
- Consumes: `MomentumSimulation::velocity()` from Task 2, `spring` field + branch from Task 5, loosened clamps from Task 4
- Produces: When a fling hits an edge mid-flight, the remaining velocity hands off to a spring (one bounded overshoot + settle). The existing fling-clamp tests are updated to reflect bounce settle behavior.

- [ ] **Step 1: Write the failing integration test for fling-to-edge spring handoff**

Add to the `#[cfg(test)] mod tests` block in `vexo/src/elements/scroll_view.rs`, after `test_spring_settles_to_top_edge`:

```rust
    #[test]
    fn test_fling_into_bottom_edge_starts_spring() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);
        let max_scroll = max_scroll_of(&pipeline);

        // Pre-scroll near the bottom so the fling hits the edge quickly.
        let target = (max_scroll - 500.0).max(0.0);
        ctrl.jump_to(target);
        for _ in 0..5 {
            pump(&ticker, &mut pipeline);
        }

        // Fling upward (toward bottom edge).
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 400.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 400.0), &press);
        for &y in &[300.0, 200.0, 100.0] {
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, y),
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, y), &mv);
        }
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 100.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 100.0), &release);

        // Pump enough for the fling to hit the edge and hand off to spring.
        for _ in 0..10 {
            pump(&ticker, &mut pipeline);
        }
        // After hitting the edge, momentum stops and spring starts.
        // The spring is active (ticker.has_active() is true).
        assert!(
            ticker.has_active(),
            "spring should be active after fling hits bottom edge"
        );

        // Pump until spring settles.
        for _ in 0..2000 {
            pump(&ticker, &mut pipeline);
            if !ticker.has_active() {
                break;
            }
        }
        assert!(!ticker.has_active(), "spring should have settled");
        assert_eq!(
            ctrl.current_offset(),
            max_scroll,
            "spring should settle exactly at bottom edge; got {}",
            ctrl.current_offset()
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_fling_into_bottom_edge_starts_spring 2>&1 | tail -10`
Expected: FAIL — currently fling hits edge and stops dead (momentum.stop() with no spring handoff). `ticker.has_active()` is false after 10 pumps because momentum terminated and no spring started.

- [ ] **Step 3: Implement fling-to-edge handoff in `rebuild_from_state`**

In `vexo/src/elements/scroll_view.rs`, in `rebuild_from_state`, find the momentum `hit_edge` block (around line 496-499). Replace:

```rust
                    let hit_edge = (clamped - physics_offset).abs() > f32::EPSILON;
                    if hit_edge {
                        self.momentum.stop();
                    }
```

with:

```rust
                    let hit_edge = (clamped - physics_offset).abs() > f32::EPSILON;
                    if hit_edge {
                        // Fling hit an edge — hand off remaining velocity to
                        // a spring for one bounded overshoot + settle.
                        let v = self.momentum.velocity();
                        let rest = if physics_offset < 0.0 { 0.0 } else { self.max_scroll() };
                        self.momentum.stop();
                        if let (Some(element_id), Some(ticker)) =
                            (self.id, self.animation_ticker.clone())
                        {
                            let now = Instant::now();
                            let tx = context.dirty_sender.clone();
                            self.spring.start(
                                clamped,
                                v,
                                rest,
                                now,
                                tx,
                                element_id,
                                ticker,
                            );
                        }
                    }
```

Note: `context.dirty_sender` is a `&mpsc::Sender<ElementKey>` field on `ElementContext` (not a method like `EventContext::dirty_sender()`), so we call `.clone()` directly. We use `if let` instead of `let Some(...) else { return; }` to avoid early returns that would skip `mark_needs_paint` at the end of `rebuild_from_state`. The existing code after the `if hit_edge` block already sets `svro.set_scroll_offset(clamped)` and `self.scroll_offset = clamped` (lines 500-514). This correctly applies the clamped (edge) offset for this frame. The spring will then take over on the next pump, starting from `clamped` (the edge) with velocity `v`.

- [ ] **Step 4: Update the existing fling-clamp tests for bounce settle**

The existing `test_fling_clamps_at_bottom_edge` and `test_fling_clamps_at_top_edge` tests assert exact edge values after 120 pumps. With the spring handoff, the fling hits the edge, spring starts, overshoots, then settles back to the edge. 120 rapid pumps may not give enough wall-clock time for the spring to settle. Update both tests to pump until the spring settles.

In `test_fling_clamps_at_bottom_edge` (around line 1473-1484), replace:

```rust
        // Pump long enough for the fling to hit the edge and clamp.
        for _ in 0..120 {
            pump(&ticker, &mut pipeline);
        }

        // Exact assertion: the fling should have clamped at max_scroll.
        assert_eq!(
            ctrl.current_offset(),
            max_scroll,
            "fling should clamp exactly at max_scroll ({})",
            max_scroll
        );
```

with:

```rust
        // Pump for the fling to hit the edge and the spring to settle.
        // The fling hands off to a spring on edge-hit; the spring overshoots
        // once then settles back to the edge. Pump until the ticker goes quiet.
        for _ in 0..5000 {
            pump(&ticker, &mut pipeline);
            if !ticker.has_active() {
                break;
            }
        }

        // After spring settle, offset snaps exactly to max_scroll (the rest).
        assert_eq!(
            ctrl.current_offset(),
            max_scroll,
            "fling should settle at max_scroll ({}) after bounce; got {}",
            max_scroll,
            ctrl.current_offset()
        );
```

In `test_fling_clamps_at_top_edge` (around line 1543-1553), replace:

```rust
        // Pump long enough for the fling to hit the top edge and clamp.
        for _ in 0..120 {
            pump(&ticker, &mut pipeline);
        }

        // Exact assertion: the fling should have clamped at 0.0 (top edge).
        assert_eq!(
            ctrl.current_offset(),
            0.0,
            "downward fling should clamp exactly at top edge (0.0)"
        );
```

with:

```rust
        // Pump for the fling to hit the top edge and the spring to settle.
        for _ in 0..5000 {
            pump(&ticker, &mut pipeline);
            if !ticker.has_active() {
                break;
            }
        }

        // After spring settle, offset snaps exactly to 0.0 (the rest).
        assert_eq!(
            ctrl.current_offset(),
            0.0,
            "downward fling should settle at top edge (0.0) after bounce; got {}",
            ctrl.current_offset()
        );
```

- [ ] **Step 5: Run the new + updated tests to verify they pass**

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_fling_into_bottom_edge_starts_spring 2>&1 | tail -5`
Expected: PASS.

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_fling_clamps_at_bottom_edge 2>&1 | tail -5`
Expected: PASS.

Run: `cargo test -p vexo --lib elements::scroll_view::tests::test_fling_clamps_at_top_edge 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 6: Run full test suite to verify no regressions**

Run: `cargo test -p vexo --lib 2>&1 | tail -15`
Expected: All tests PASS.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/elements/scroll_view.rs
git commit -m "feat: fling-to-edge hands off to spring for bounce

When a fling hits an edge mid-flight, the remaining velocity carries into
a spring (one bounded overshoot + settle) instead of stopping dead. Uses
MomentumSimulation::velocity() to read the current fling velocity at the
handoff point. Updates fling-clamp tests to pump until spring settles."
```

---

### Task 8: Manual Tuning Instrumentation

**Files:**
- Modify: `vexo/src/animation/spring.rs:128` (add log::debug! in `start`)
- Modify: `vexo/src/animation/spring.rs:155` (add log::debug! in `advance` settle)
- Modify: `vexo/src/elements/scroll_view.rs:354` (add log::debug! in Up arm spring start)
- Modify: `vexo/src/elements/scroll_view.rs:497` (add log::debug! in fling handoff)

**Interfaces:**
- Consumes: all previous tasks
- Produces: `log::debug!` instrumentation for manual visual tuning. No behavior change. No tests (logging is not testable).

- [ ] **Step 1: Add logging to SpringSimulation**

In `vexo/src/animation/spring.rs`, add at the top of the file (after the existing `use` statements):

```rust
use log::debug;
```

In `SpringSimulation::start`, add after `self.active = true;`:

```rust
        self.active = true;
        debug!(
            "[spring] start: offset0={}, v0={}, rest={}",
            offset0, v0, rest
        );
```

In `SpringSimulation::advance`, in the settle check block (where `terminate()` is called), add before `self.terminate()`:

```rust
        if (self.offset - self.rest).abs() < X_SETTLE && self.velocity.abs() < V_SETTLE {
            debug!(
                "[spring] settled: offset={}, rest={}, velocity={}",
                self.offset, self.rest, self.velocity
            );
            self.terminate();
            return None;
        }
```

- [ ] **Step 2: Add logging to ScrollViewElement spring start sites**

In `vexo/src/elements/scroll_view.rs`, add at the top of the file (after the existing `use` statements, around line 22):

```rust
use log::debug;
```

In the Up arm overscroll-release branch (the `if self.scroll_offset < 0.0` block), add after `self.spring.start(...)`:

```rust
                    self.spring.start(
                        self.scroll_offset,
                        v,
                        0.0,
                        now,
                        tx,
                        element_id,
                        ticker,
                    );
                    debug!(
                        "[scroll] release past top → spring: offset={}, v={}",
                        self.scroll_offset, v
                    );
```

Do the same for the `else if self.scroll_offset > max` block, after `self.spring.start(...)`:

```rust
                    self.spring.start(
                        self.scroll_offset,
                        v,
                        max,
                        now,
                        tx,
                        element_id,
                        ticker,
                    );
                    debug!(
                        "[scroll] release past bottom → spring: offset={}, v={}, max={}",
                        self.scroll_offset, v, max
                    );
```

In `rebuild_from_state`, in the `hit_edge` block, add after `self.spring.start(...)`:

```rust
                        self.spring.start(
                            clamped,
                            v,
                            rest,
                            now,
                            tx,
                            element_id,
                            ticker,
                        );
                        debug!(
                            "[scroll] fling hit edge → spring: clamped={}, v={}, rest={}",
                            clamped, v, rest
                        );
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: BUILD SUCCEEDS.

Run: `cargo test -p vexo --lib 2>&1 | tail -5`
Expected: All tests still PASS (logging is invisible in tests).

- [ ] **Step 4: Commit**

```bash
git add vexo/src/animation/spring.rs vexo/src/elements/scroll_view.rs
git commit -m "feat: add log::debug instrumentation for bounce tuning

Logs spring start (offset/v0/rest), spring settle (offset/rest/velocity),
and all three spring-trigger sites in ScrollViewElement (release past top,
release past bottom, fling hit edge). For manual visual tuning via:
RUST_LOG=vexo::animation::spring=debug,vexo::elements::scroll_view=debug"
```

- [ ] **Step 5: Provide the user with the manual tuning command and checklist**

Present the following to the user (do NOT run the demo yourself):

```
Bounce effect is implemented and all tests pass. To manually verify and tune the feel, run:

RUST_LOG=vexo::animation::spring=debug,vexo::elements::scroll_view=debug cargo run -p desktop_demo 2>&1 | grep -E "spring|scroll" | tee /tmp/bounce.log

Tuning checklist:
1. Drag chat list down past top → content rubber-bands, resistance increases with depth
2. Release → content springs back to top, no overshoot, smooth settle
3. Fling up hard from middle → content hits bottom, bounces once, settles at bottom
4. Fling down hard from middle → content hits top, bounces once, settles at top
5. Drag past top, then drag back down without releasing → content follows finger, no jerk at edge crossing
6. Press mid-bounce → bounce stops, drag resumes from current position
7. Wheel during bounce → bounce stops, wheel scroll takes over
8. Short list (content < viewport) → dragging still bounces against top
9. jump_to_bottom() during a bounce → bounce stops, jumps to bottom
10. Leave window for 5s, return mid-bounce → no explosion (verify in log: dt clamped)

Tuning knobs (in vexo/src/animation/spring.rs):
- STIFFNESS (higher = snappier return, currently 340)
- DAMPING_RATIO (lower = wobblier, currently 1.0 = critically damped)
- Velocity bleed at fling-to-edge handoff (if overshoot too aggressive, multiply v by 0.5 before passing to spring.start in rebuild_from_state)
```
