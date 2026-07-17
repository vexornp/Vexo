# Scroll View Inertial Momentum Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add iOS-style inertial momentum to `ScrollView` so a touch fling coasts to a stop after the finger lifts.

**Architecture:** Two new pure-value types (`VelocityTracker`, `MomentumSimulation`) wired into `ScrollViewElement` at five modification sites (`mount`, three `on_arena_winner_update` arms, `rebuild_from_state`, `unmount`). The simulation owns ticker registration + dirty callback, mirroring `AnimationController`. The existing deferred-apply pipeline (`mpsc` → `drain_dirty_to_build_owner` → `rebuild_from_state`) drives per-frame offset writes via the existing `apply_scroll_offset` method.

**Tech Stack:** Rust, wgpu, taffy, glyphon. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-07-17-scroll-momentum-design.md`

## Global Constraints

- Behavior scope: momentum only. No rubber-band, no spring-back, no overscroll stretch. Hard clamp at `[0, max_scroll]` each frame.
- Input devices: touch drag only. Mouse wheel and keyboard stay instantaneous. Existing `test_mouse_wheel_still_works` (asserts `ctrl.current_offset() == 100.0`) MUST pass unchanged.
- Min fling velocity: `V_MIN_FLING = 50.0 px/s`. Below this, no momentum.
- Velocity decay terminate threshold: `V_STOP = 13.0 px/s`.
- Decay time constant: `τ = 0.325 s`.
- Max duration safety ceiling: `MAX_DURATION = 10.0 s`.
- Velocity window: `100ms`.
- Physics model: closed-form exponential decay `v(t) = v0·e^(-t/τ)`, `Δoffset(t) = v0·τ·(1 - e^(-t/τ))`. NOT Euler integration.
- Sign convention: pointer `y` increases downward. `VelocityTracker::velocity()` returns raw pointer-space `dy/dt`. Element negates before passing to `MomentumSimulation::start` (so positive `v0` = offset increases = scrolls toward bottom).
- No new widgets. No changes to `ScrollView` widget, `ScrollController` public API, `ScrollViewRenderObject`, `VerticalDragRecognizer`, or `pipeline.rs`/`window.rs`.
- After every Rust file edit: run `cargo build -p vexo`. After implementing a feature: run `cargo test -p vexo`.
- NEVER run `cargo run -p desktop_demo` (per CLAUDE.md — can't interact with GUI).

---

## File Structure

| File | Status | Responsibility |
|---|---|---|
| `vexo/src/gestures/velocity_tracker.rs` | NEW | Pure-value ring buffer of `(Instant, y)`. Windowed least-squares slope. No framework deps. |
| `vexo/src/gestures/mod.rs` | MODIFY | Add `pub mod velocity_tracker;` + re-export. |
| `vexo/src/animation/momentum.rs` | NEW | Time-driven exponential-decay simulation. Owns ticker registration + dirty callback (mirrors `AnimationController`). |
| `vexo/src/animation/mod.rs` | MODIFY | Add `pub mod momentum;` + re-export. |
| `vexo/src/elements/scroll_view.rs` | MODIFY | Add 3 fields, stash ticker in `mount`, wire 3 arena arms + `rebuild_from_state` + `unmount`. |

---

## Task 1: `VelocityTracker` — pure ring buffer + least-squares slope

**Files:**
- Create: `vexo/src/gestures/velocity_tracker.rs`
- Modify: `vexo/src/gestures/mod.rs`
- Test: `vexo/src/gestures/velocity_tracker.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `std::time::{Duration, Instant}`, `std::collections::VecDeque`.
- Produces: `pub struct VelocityTracker`, `VelocityTracker::new() -> Self`, `VelocityTracker::add(&mut self, t: Instant, y: f32)`, `VelocityTracker::velocity(&self) -> f32`, `VelocityTracker::clear(&mut self)`.

- [ ] **Step 1: Write the failing tests**

Create `vexo/src/gestures/velocity_tracker.rs` with tests only (no impl yet):

```rust
//! VelocityTracker — windowed least-squares velocity estimation from pointer samples.
//!
//! Pure value type. No framework dependencies.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Window over which samples contribute to velocity estimation. Matches iOS/Flutter.
const WINDOW: Duration = Duration::from_millis(100);

pub struct VelocityTracker {
    samples: VecDeque<(Instant, f32)>,
}

impl VelocityTracker {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::new(),
        }
    }

    pub fn add(&mut self, t: Instant, y: f32) {
        self.samples.push_back((t, y));
        let cutoff = t.checked_sub(WINDOW).unwrap_or(t);
        while let Some(&(front_t, _)) = self.samples.front() {
            if front_t < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn velocity(&self) -> f32 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        // Least-squares slope of y over t (seconds). Use an arbitrary epoch
        // (the first sample's timestamp) to keep the numbers small and avoid
        // precision loss from large Instant-as-secs_f64 values.
        let t0 = self.samples.front().unwrap().0;
        let n = self.samples.len() as f64;
        let mut sum_t = 0.0;
        let mut sum_y = 0.0;
        let mut sum_tt = 0.0;
        let mut sum_ty = 0.0;
        for &(t, y) in self.samples.iter() {
            let dt = (t.saturating_duration_since(t0)).as_secs_f64();
            sum_t += dt;
            sum_y += y as f64;
            sum_tt += dt * dt;
            sum_ty += dt * (y as f64);
        }
        let denom = n * sum_tt - sum_t * sum_t;
        if denom.abs() < 1e-12 {
            return 0.0;
        }
        let slope = (n * sum_ty - sum_t * sum_y) / denom;
        slope as f32
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

impl Default for VelocityTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(elapsed_ms: u64) -> Instant {
        Instant::now() + Duration::from_millis(elapsed_ms)
    }

    #[test]
    fn empty_tracker_returns_zero() {
        let vt = VelocityTracker::new();
        assert_eq!(vt.velocity(), 0.0);
    }

    #[test]
    fn single_sample_returns_zero() {
        let mut vt = VelocityTracker::new();
        vt.add(t(0), 100.0);
        assert_eq!(vt.velocity(), 0.0);
    }

    #[test]
    fn two_samples_50ms_100px_apart_gives_2000_px_per_s() {
        let mut vt = VelocityTracker::new();
        vt.add(t(0), 100.0);
        vt.add(t(50), 200.0); // 100px over 0.05s = 2000 px/s
        let v = vt.velocity();
        assert!((v - 2000.0).abs() < 1.0, "got {}", v);
    }

    #[test]
    fn three_collinear_samples_match_slope() {
        let mut vt = VelocityTracker::new();
        vt.add(t(0), 0.0);
        vt.add(t(100), 500.0); // 500 px over 0.1s = 5000 px/s
        vt.add(t(200), 1000.0);
        let v = vt.velocity();
        assert!((v - 5000.0).abs() < 1.0, "got {}", v);
    }

    #[test]
    fn noisy_samples_still_return_a_slope() {
        let mut vt = VelocityTracker::new();
        vt.add(t(0), 0.0);
        vt.add(t(33), 200.0);
        vt.add(t(66), 150.0); // jitter down
        vt.add(t(99), 450.0);
        let v = vt.velocity();
        // Slope should be positive (overall upward) and finite.
        assert!(v > 0.0, "got {}", v);
        assert!(v.is_finite());
    }

    #[test]
    fn samples_outside_window_are_dropped() {
        let mut vt = VelocityTracker::new();
        // 5 samples spanning 200ms. Only the last ~100ms should count.
        vt.add(t(0), 0.0);
        vt.add(t(50), 10.0);
        vt.add(t(100), 20.0);
        vt.add(t(150), 500.0);
        vt.add(t(200), 1000.0);
        let v = vt.velocity();
        // If all 5 samples counted, slope would be 5000 px/s (1000px/0.2s).
        // With only last ~100ms counting (the steep part), slope is ~10000 px/s.
        assert!(v > 5000.0, "old samples should be dropped; got {}", v);
    }

    #[test]
    fn clear_empties_buffer() {
        let mut vt = VelocityTracker::new();
        vt.add(t(0), 0.0);
        vt.add(t(50), 100.0);
        assert!(vt.velocity() != 0.0);
        vt.clear();
        assert_eq!(vt.velocity(), 0.0);
    }

    #[test]
    fn decreasing_y_returns_negative_velocity() {
        let mut vt = VelocityTracker::new();
        vt.add(t(0), 200.0);
        vt.add(t(50), 100.0); // y decreasing → negative slope
        let v = vt.velocity();
        assert!(v < 0.0, "got {}", v);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass (impl is inline above)**

Run: `cargo test -p vexo gestures::velocity_tracker`
Expected: 8 tests PASS.

If any fail, fix the impl (not the tests — the tests are the spec).

- [ ] **Step 3: Wire module into `gestures/mod.rs`**

Modify `vexo/src/gestures/mod.rs` — add the module declaration and re-export. After the existing `pub mod vertical_drag;` line:

```rust
pub mod velocity_tracker;
```

And in the re-exports block (after `pub use vertical_drag::VerticalDragRecognizer;`):

```rust
pub use velocity_tracker::VelocityTracker;
```

- [ ] **Step 4: Build to verify wiring**

Run: `cargo build -p vexo`
Expected: builds clean, no warnings about unused code (the type will be used in Task 3).

- [ ] **Step 5: Commit**

```bash
git add vexo/src/gestures/velocity_tracker.rs vexo/src/gestures/mod.rs
git commit -m "feat(gestures): add VelocityTracker for windowed pointer-velocity estimation"
```

---

## Task 2: `MomentumSimulation` — exponential-decay fling with ticker ownership

**Files:**
- Create: `vexo/src/animation/momentum.rs`
- Modify: `vexo/src/animation/mod.rs`
- Test: `vexo/src/animation/momentum.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `std::sync::{mpsc, Arc}`, `std::time::{Duration, Instant}`, `crate::animation::ticker::{AnimationTicker, TickHandle}`, `crate::id::ElementKey`.
- Produces:
  - `pub struct MomentumSimulation`
  - `MomentumSimulation::new() -> Self` (inactive)
  - `MomentumSimulation::start(&mut self, offset0: f32, v0: f32, now: Instant, dirty_sender: mpsc::Sender<ElementKey>, element_id: ElementKey, ticker: Arc<AnimationTicker>)`
  - `MomentumSimulation::advance(&mut self, now: Instant) -> Option<f32>` — returns `Some(offset)` while active and velocity above threshold, `None` when terminated.
  - `MomentumSimulation::stop(&mut self)`
  - `MomentumSimulation::is_active(&self) -> bool`

- [ ] **Step 1: Write the failing tests**

Create `vexo/src/animation/momentum.rs` with tests only:

```rust
//! MomentumSimulation — exponential-decay fling physics, ticker-driven.
//!
//! Mirrors `AnimationController`'s ownership pattern: holds a `TickHandle`
//! registered with `AnimationTicker`, plus a dirty callback that sends the
//! owning element's ID through the pipeline's mpsc channel. The element
//! drives each frame's offset write in `rebuild_from_state` via `advance`.

use std::sync::Arc;

use crate::animation::ticker::{AnimationTicker, TickHandle};
use crate::id::ElementKey;

/// Decay time constant. iOS UIKit / Flutter公开 deceleration constant.
const TAU: f32 = 0.325;
/// Below this velocity (px/s), the simulation terminates.
const V_STOP: f32 = 13.0;
/// Safety ceiling — normal flings never approach this.
const MAX_DURATION: f32 = 10.0;

pub struct MomentumSimulation {
    offset0: f32,
    v0: f32,
    start_time: Option<std::time::Instant>,
    active: bool,
    ticker: Option<Arc<AnimationTicker>>,
    tick_handle: Option<TickHandle>,
}

impl MomentumSimulation {
    pub fn new() -> Self {
        Self {
            offset0: 0.0,
            v0: 0.0,
            start_time: None,
            active: false,
            ticker: None,
            tick_handle: None,
        }
    }

    pub fn start(
        &mut self,
        offset0: f32,
        v0: f32,
        now: std::time::Instant,
        dirty_sender: std::sync::mpsc::Sender<ElementKey>,
        element_id: ElementKey,
        ticker: Arc<AnimationTicker>,
    ) {
        self.stop(); // drop any prior registration
        self.offset0 = offset0;
        self.v0 = v0;
        self.start_time = Some(now);
        self.active = true;
        self.ticker = Some(ticker.clone());
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = dirty_sender.send(element_id);
        });
        self.tick_handle = Some(ticker.register(cb));
        // Fire dirty callback immediately so the element is scheduled for
        // rebuild on this event-loop turn — mirrors AnimationController::forward
        // (controller.rs:49-51). Without this, the ticker only ticks inside
        // render_retain(), which only runs when a frame is already requested —
        // a deadlock.
        let _ = dirty_sender.send(element_id);
    }

    /// Advance the simulation. Returns `Some(offset)` while the fling is
    /// still alive and above `V_STOP`; `None` once terminated.
    pub fn advance(&mut self, now: std::time::Instant) -> Option<f32> {
        if !self.active {
            return None;
        }
        let start = match self.start_time {
            Some(t) => t,
            None => return None,
        };
        let dt = now.saturating_duration_since(start).as_secs_f32() as f32;
        if dt > MAX_DURATION {
            self.terminate();
            return None;
        }
        let v = self.v0 * (-dt / TAU).exp();
        if v.abs() < V_STOP {
            self.terminate();
            return None;
        }
        let delta = self.v0 * TAU * (1.0 - (-dt / TAU).exp());
        Some(self.offset0 + delta)
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.start_time = None;
        if let (Some(ticker), Some(handle)) = (self.ticker.clone(), self.tick_handle.take()) {
            ticker.unregister(handle);
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    fn terminate(&mut self) {
        self.active = false;
        self.start_time = None;
        if let (Some(ticker), Some(handle)) = (self.ticker.clone(), self.tick_handle.take()) {
            ticker.unregister(handle);
        }
    }
}

impl Default for MomentumSimulation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn start_sim(v0: f32) -> (MomentumSimulation, Instant, mpsc::Receiver<ElementKey>, Arc<AnimationTicker>) {
        let (tx, rx) = mpsc::channel();
        let ticker = Arc::new(AnimationTicker::new());
        let mut sim = MomentumSimulation::new();
        let now = Instant::now();
        sim.start(0.0, v0, now, tx, ElementKey::default(), ticker.clone());
        (sim, now, rx, ticker)
    }

    #[test]
    fn new_is_inactive() {
        let sim = MomentumSimulation::new();
        assert!(!sim.is_active());
    }

    #[test]
    fn start_activates_and_registers_with_ticker() {
        let (sim, _, _rx, ticker) = start_sim(1000.0);
        assert!(sim.is_active());
        assert!(ticker.has_active());
    }

    #[test]
    fn advance_at_t_zero_returns_offset0() {
        let (mut sim, now, _rx, _ticker) = start_sim(1000.0);
        let offset = sim.advance(now).unwrap();
        assert!((offset - 0.0).abs() < 1e-3, "got {}", offset);
    }

    #[test]
    fn advance_at_t_tau_matches_closed_form() {
        let (mut sim, now, _rx, _ticker) = start_sim(1000.0);
        let dt = Duration::from_secs_f32(TAU);
        let offset = sim.advance(now + dt).unwrap();
        // Δoffset = v0·τ·(1 - 1/e)
        let expected = 1000.0 * TAU * (1.0 - 1.0 / std::f32::consts::E);
        assert!((offset - expected).abs() < 1.0, "got {} expected {}", offset, expected);
    }

    #[test]
    fn positive_v0_increases_offset() {
        let (mut sim, now, _rx, _ticker) = start_sim(1000.0);
        let later = now + Duration::from_millis(100);
        let offset = sim.advance(later).unwrap();
        assert!(offset > 0.0, "got {}", offset);
    }

    #[test]
    fn negative_v0_decreases_offset() {
        let (mut sim, now, _rx, _ticker) = start_sim(-1000.0);
        let later = now + Duration::from_millis(100);
        let offset = sim.advance(later).unwrap();
        assert!(offset < 0.0, "got {}", offset);
    }

    #[test]
    fn stop_clears_active_and_unregisters() {
        let (mut sim, _, _rx, ticker) = start_sim(1000.0);
        assert!(ticker.has_active());
        sim.stop();
        assert!(!sim.is_active());
        assert!(!ticker.has_active());
        // advance after stop is a no-op
        assert!(sim.advance(Instant::now()).is_none());
    }

    #[test]
    fn advance_terminates_when_velocity_decays_below_v_stop() {
        // v0 small enough that decay crosses V_STOP quickly.
        // v(t) = v0·e^(-t/τ) < V_STOP  ⇒  t > τ·ln(v0/V_STOP)
        let v0 = 50.0; // just above V_MIN_FLING but close to V_STOP
        let (mut sim, now, _rx, _ticker) = start_sim(v0);
        let t_stop = TAU * (v0 / V_STOP).ln();
        let just_before = now + Duration::from_secs_f32(t_stop * 0.9);
        let just_after = now + Duration::from_secs_f32(t_stop * 1.2);
        assert!(sim.advance(just_before).is_some(), "still above V_STOP");
        assert!(sim.advance(just_after).is_none(), "below V_STOP → terminate");
        assert!(!sim.is_active());
    }

    #[test]
    fn advance_terminates_after_max_duration() {
        let (mut sim, now, _rx, _ticker) = start_sim(100_000.0); // huge v0, won't decay fast
        let way_later = now + Duration::from_secs_f32(MAX_DURATION + 1.0);
        assert!(sim.advance(way_later).is_none());
        assert!(!sim.is_active());
    }

    #[test]
    fn dirty_callback_fires_on_start() {
        let (tx, rx) = mpsc::channel();
        let ticker = Arc::new(AnimationTicker::new());
        let mut sim = MomentumSimulation::new();
        sim.start(0.0, 1000.0, Instant::now(), tx, ElementKey::default(), ticker);
        // start() fires the dirty callback immediately.
        assert!(rx.try_recv().is_ok(), "dirty callback should fire on start");
    }

    #[test]
    fn ticker_tick_fires_dirty_callback_after_start() {
        let (tx, rx) = mpsc::channel();
        let ticker = Arc::new(AnimationTicker::new());
        let mut sim = MomentumSimulation::new();
        sim.start(0.0, 1000.0, Instant::now(), tx, ElementKey::default(), ticker.clone());
        // Drain the immediate fire from start().
        let _ = rx.try_recv();
        ticker.tick();
        assert!(rx.try_recv().is_ok(), "ticker.tick() should fire the registered callback");
    }
}
```

Note: `ElementKey::default()` is used in tests — verify it derives `Default` (slotmap keys typically do). If not, the test will fail to compile; replace with `ElementKey::from(slotmap::KeyData::from(0u64))` or whatever the codebase uses. Check `vexo/src/id.rs` if compilation fails.

- [ ] **Step 2: Run tests to verify they pass (impl is inline above)**

Run: `cargo test -p vexo animation::momentum`
Expected: 11 tests PASS.

If `ElementKey::default()` doesn't exist, fix the test imports/usage to match `vexo/src/id.rs` — do NOT change the impl signatures.

- [ ] **Step 3: Wire module into `animation/mod.rs`**

Modify `vexo/src/animation/mod.rs` — add after `pub mod tween;`:

```rust
pub mod momentum;
```

And in the re-exports (after `pub use tween::{ColorTween, FloatTween, Tween};`):

```rust
pub use momentum::MomentumSimulation;
```

- [ ] **Step 4: Build to verify wiring**

Run: `cargo build -p vexo`
Expected: builds clean.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/animation/momentum.rs vexo/src/animation/mod.rs
git commit -m "feat(animation): add MomentumSimulation for iOS-style fling decay"
```

---

## Task 3: Wire `VelocityTracker` + `MomentumSimulation` into `ScrollViewElement`

**Files:**
- Modify: `vexo/src/elements/scroll_view.rs` (multiple sites, listed below)
- Test: `vexo/src/elements/scroll_view.rs` (inline `#[cfg(test)] mod tests` — add new tests)

**Interfaces:**
- Consumes:
  - From Task 1: `crate::gestures::VelocityTracker` (`new`, `add(t, y)`, `velocity() -> f32`, `clear`).
  - From Task 2: `crate::animation::MomentumSimulation` (`new`, `start(offset0, v0, now, dirty_sender, element_id, ticker)`, `advance(now) -> Option<f32>`, `stop`, `is_active`).
  - From existing code: `crate::animation::AnimationTicker`, `crate::id::ElementKey`, `std::sync::mpsc::Sender`, `std::time::Instant`, `crate::gestures::ArenaEvent`.
- Produces: `ScrollViewElement` with two new fields wired into five modification sites. No public API changes.

- [ ] **Step 1: Add the two new fields to `ScrollViewElement`**

Modify `vexo/src/elements/scroll_view.rs:35-48`. The current struct is:

```rust
pub struct ScrollViewElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
    scroll_offset: f32,
    content_height: f32,
    viewport_height: f32,
    controller: Option<ScrollController>,
    last_drag_y: f32,
}
```

Change to (add three fields — the third stashes the ticker for the `Up` wiring, since `EventContext` doesn't expose it):

```rust
pub struct ScrollViewElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
    scroll_offset: f32,
    content_height: f32,
    viewport_height: f32,
    controller: Option<ScrollController>,
    last_drag_y: f32,
    velocity_tracker: VelocityTracker,
    momentum: MomentumSimulation,
    animation_ticker: Option<Arc<AnimationTicker>>,
}
```

Update `ScrollViewElement::new()` (`scroll_view.rs:51-64`) to initialize the new fields:

```rust
pub fn new() -> Self {
    Self {
        id: None,
        key: None,
        render_object: None,
        widget: None,
        focus_attachment: None,
        scroll_offset: 0.0,
        content_height: 0.0,
        viewport_height: 0.0,
        controller: None,
        last_drag_y: 0.0,
        velocity_tracker: VelocityTracker::new(),
        momentum: MomentumSimulation::new(),
        animation_ticker: None,
    }
}
```

Add the new imports at the top of the file (after the existing `use` statements around `scroll_view.rs:1-19`):

```rust
use std::time::Instant;

use crate::animation::{AnimationTicker, MomentumSimulation};
use crate::gestures::VelocityTracker;
```

(If `Arc` isn't already imported, add `use std::sync::Arc;` — check the existing imports; `wire_dirty_callback` already uses `Arc`, so it's likely imported.)

- [ ] **Step 2: Stash the ticker in `mount`**

Modify `ScrollViewElement::mount` (`scroll_view.rs:158-176`). Add the stash as the first line of the method body:

```rust
fn mount(&mut self, context: &mut ElementContext) {
    self.animation_ticker = Some(context.animation_ticker.clone());
    let element_key = context.element_id;
    // ... rest of existing mount unchanged ...
}
```

- [ ] **Step 3: Wire `Down` — clear tracker, stop momentum**

Modify `ScrollViewElement::on_arena_winner_update` (`scroll_view.rs:289-316`). The current `Down` arm:

```rust
ArenaEvent::Down { .. } => {
    // Drag just won (on the move that crossed slop). Initialize
    // last_drag_y from the recognizer's DOWN position ...
    self.last_drag_y = drag.down_position().y;
}
```

Change to:

```rust
ArenaEvent::Down { .. } => {
    // Stop any in-flight fling BEFORE clearing the tracker, so a new drag's
    // samples can't race with an old fling's dirty callback.
    self.momentum.stop();
    self.velocity_tracker.clear();
    self.last_drag_y = drag.down_position().y;
}
```

- [ ] **Step 4: Wire `Move` — sample into tracker (existing delta logic unchanged)**

In the same method, the current `Move` arm (`scroll_view.rs:290-300`):

```rust
ArenaEvent::Move { position } => {
    let delta = self.last_drag_y - position.y;
    self.last_drag_y = position.y;
    let new_offset = self.scroll_offset + delta;
    self.apply_scroll_offset(new_offset, ctx);
}
```

Change to (add the tracker sample FIRST, so the timestamp reflects when the pointer was here, not after delta math):

```rust
ArenaEvent::Move { position } => {
    self.velocity_tracker.add(Instant::now(), position.y);
    let delta = self.last_drag_y - position.y;
    self.last_drag_y = position.y;
    let new_offset = self.scroll_offset + delta;
    self.apply_scroll_offset(new_offset, ctx);
}
```

- [ ] **Step 5: Wire `Up` — start momentum if velocity above threshold**

In the same method, the current `Up` arm (`scroll_view.rs:310-312`):

```rust
ArenaEvent::Up { .. } => {
    // Drag ended. No scroll applied on up (no momentum in v1).
}
```

Change to (note the sign-flip on velocity per the spec's Sign Convention):

```rust
ArenaEvent::Up { .. } => {
    // Sign-flip: tracker returns pointer-space dy/dt (y-down). The existing
    // Move handler does `delta = last_drag_y - position.y` (negates pointer
    // delta) before applying to scroll_offset, so an upward finger motion
    // (dy/dt < 0) produces positive offset delta. To scroll the same direction
    // after release, negate the tracker velocity so positive v0 = offset
    // increases = scrolls toward bottom.
    let v = -self.velocity_tracker.velocity();
    const V_MIN_FLING: f32 = 50.0;
    if v.abs() < V_MIN_FLING {
        return;
    }
    let Some(element_id) = self.id else { return; };
    let Some(tx) = ctx.dirty_sender.cloned() else { return; };
    let Some(ticker) = self.animation_ticker.clone() else { return; };
    self.momentum.start(
        self.scroll_offset,
        v,
        Instant::now(),
        tx,
        element_id,
        ticker,
    );
}
```

Note: `ctx.dirty_sender` is `Option<&'a mpsc::Sender<ElementKey>>` (see `event_context.rs:66`). `.cloned()` lifts it to `Option<Sender>` — cloning a `Sender` is cheap and `Send + Sync`.

- [ ] **Step 6: Wire `rebuild_from_state` — step the simulation each rebuild**

Modify `ScrollViewElement::rebuild_from_state` (`scroll_view.rs:366-405`). The current method consumes `pending` (programmatic jump), then calls `mark_needs_paint`. Insert a momentum step between the pending block and the mark_needs_paint block.

Find the existing pending block:
```rust
if let Some(target) = pending {
    if let Some(ro_key) = self.render_object {
        if let Some(svro) = context
            .render_objects
            .get(ro_key)
            .and_then(|ro| ro.as_any().downcast_ref::<ScrollViewRenderObject>())
        {
            // ... existing target-apply ...
        }
    }
}
```

Add `self.momentum.stop();` as the FIRST line inside `if let Some(target) = pending {` — so a programmatic jump cancels any in-flight fling:

```rust
if let Some(target) = pending {
    self.momentum.stop();  // programmatic jump cancels in-flight fling
    if let Some(ro_key) = self.render_object {
        // ... existing target-apply unchanged ...
    }
}
```

Then AFTER the pending block and BEFORE the existing `if let Some(ro_key) = self.render_object { context.mark_needs_paint(ro_key); }`, add:

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

`apply_scroll_offset` already calls `mark_needs_build` (via the `BuildOwner` in `EventContext` — wait, `rebuild_from_state` receives `ElementContext`, not `EventContext`). Verify `apply_scroll_offset`'s signature. The existing method (`scroll_view.rs:74`) takes `ctx: &EventContext`. In `rebuild_from_state` we have `context: &mut ElementContext`. Check how the existing pending block applies its offset — it calls `svro.set_scroll_offset(clamped)` directly and `ctrl.set_current_offset(clamped)` directly, NOT via `apply_scroll_offset`. So the momentum block must do the same: write directly to the render object + controller, and call `mark_needs_build` via the build owner.

**IMPORTANT — adjust the momentum block to match the pending block's pattern:**

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
            if let Some(ro_key) = self.render_object {
                if let Some(svro) = context
                    .render_objects
                    .get(ro_key)
                    .and_then(|ro| ro.as_any().downcast_ref::<ScrollViewRenderObject>())
                {
                    self.viewport_height = svro.viewport_size().height;
                    self.content_height = svro.content_size().height;
                    svro.set_scroll_offset(clamped);
                }
            }
            self.scroll_offset = clamped;
            if let Some(ctrl) = self.controller.as_ref() {
                ctrl.set_current_offset(clamped);
            }
            context.build_owner.mark_needs_build(self.id.unwrap());
        }
        None => {
            self.momentum.stop();
        }
    }
}
```

This mirrors the pending block's write pattern exactly. Verify `context.build_owner.mark_needs_build` is the correct call by comparing to how the existing pending block schedules its next rebuild — it does NOT explicitly call `mark_needs_build` (the dirty callback from the ticker does that on the next tick). So actually, the momentum block should NOT call `mark_needs_build` directly either — the ticker fires the dirty callback → mpsc → `drain_dirty_to_build_owner` → next `rebuild_from_state`. Remove the `mark_needs_build` line:

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
            if let Some(ro_key) = self.render_object {
                if let Some(svro) = context
                    .render_objects
                    .get(ro_key)
                    .and_then(|ro| ro.as_any().downcast_ref::<ScrollViewRenderObject>())
                {
                    self.viewport_height = svro.viewport_size().height;
                    self.content_height = svro.content_size().height;
                    svro.set_scroll_offset(clamped);
                }
            }
            self.scroll_offset = clamped;
            if let Some(ctrl) = self.controller.as_ref() {
                ctrl.set_current_offset(clamped);
            }
            // The next frame's tick fires the dirty callback (registered in
            // momentum.start), which sends element_id through the mpsc channel,
            // which drain_dirty_to_build_owner picks up to schedule the next
            // rebuild_from_state. No explicit mark_needs_build here.
        }
        None => {
            self.momentum.stop();
        }
    }
}
```

But wait — `apply_scroll_offset` (the existing method used during interactive drag) DOES call `bo.mark_needs_build(ctx.element_id())` at `scroll_view.rs:106-108`. That's because interactive events come through `EventContext` which has `build_owner: Option<&BuildOwner>`. The deferred-apply path (`rebuild_from_state`) doesn't need it because the ticker drives the next rebuild. Match the pending block — no `mark_needs_build` call.

- [ ] **Step 7: Wire `unmount` — stop momentum to drop ticker registration**

Modify `ScrollViewElement::unmount` (`scroll_view.rs:200-208`). Add `self.momentum.stop();` as the FIRST line:

```rust
fn unmount(&mut self, context: &mut ElementContext) {
    self.momentum.stop();
    if let Some(ctrl) = self.controller.as_ref() {
        ctrl.clear_dirty_callback();
    }
    // ... rest of existing unmount unchanged ...
}
```

- [ ] **Step 8: Build to verify compilation**

Run: `cargo build -p vexo`
Expected: builds clean. If `Arc` or `Instant` imports are missing, add them. If `ctx.dirty_sender.cloned()` doesn't compile because `Sender` isn't `Clone`... it IS `Clone` (std guarantees it) — re-check the field type if this fails.

- [ ] **Step 9: Run existing scroll_view tests to verify no regressions**

Run: `cargo test -p vexo elements::scroll_view`
Expected: all existing tests PASS, especially `test_mouse_wheel_still_works` (asserts `ctrl.current_offset() == 100.0` — momentum must NOT engage for mouse wheel).

If `test_mouse_wheel_still_works` fails, the bug is that momentum is being triggered by the scroll event — but the wiring only touches `on_arena_winner_update`, which mouse wheel doesn't go through. The failure would indicate an unrelated regression.

- [ ] **Step 10: Write the failing integration test — fast upward fling scrolls further after release**

Append to the `#[cfg(test)] mod tests` block in `vexo/src/elements/scroll_view.rs`:

```rust
#[test]
fn test_fling_scrolls_after_release() {
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
    let ticker = Arc::new(AnimationTicker::new());
    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.reconcile(Box::new(sv));
    let mut engine = crate::layout::TaffyLayoutEngine::new();
    let mut font_system = crate::resource::new_font_system();
    pipeline.layout(
        crate::core::Size::new(400.0, 600.0),
        &mut engine,
        &mut font_system,
    );

    // Press at (200, 300).
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
    // Three fast upward moves over ~60ms (synthetic — real time will vary,
    // but VelocityTracker uses Instant::now() so we can't fake timestamps here).
    // We rely on the moves being fast enough in wall-clock to exceed V_MIN_FLING.
    for &y in &[290.0, 270.0, 240.0] {
        let mv = InputEvent::PointerMoved {
            position: Point::new(200.0, y),
        };
        pipeline.handle_event(
            Point::new(200.0, y),
            &mv,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
    }
    let offset_at_release = ctrl.current_offset();
    assert!(
        offset_at_release > 0.0,
        "drag should have scrolled; got {}",
        offset_at_release
    );

    // Release.
    let release = InputEvent::PointerButton {
        position: Point::new(200.0, 240.0),
        button: PointerButton::Primary,
        state: ButtonState::Released,
    };
    pipeline.handle_event(
        Point::new(200.0, 240.0),
        &release,
        Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &test_clipboard(),
    );

    // Pump the ticker + pipeline to let momentum run.
    // Each tick fires the dirty callback → mpsc → drain_dirty_to_build_owner
    // → rebuild_from_state → advance + apply.
    for _ in 0..30 {
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
    }

    assert!(
        ctrl.current_offset() > offset_at_release,
        "momentum should have scrolled further after release; got {} after release, {} after pump",
        offset_at_release,
        ctrl.current_offset()
    );
}
```

- [ ] **Step 11: Run the new test to verify it passes**

Run: `cargo test -p vexo elements::scroll_view::tests::test_fling_scrolls_after_release`
Expected: PASS.

If it FAILS with "momentum should have scrolled further", the likely causes are:
1. The synthetic moves weren't fast enough in wall-clock to exceed `V_MIN_FLING = 50 px/s`. Check by adding a `dbg!(v)` in the `Up` arm. If `v` is below 50, the test needs faster moves (smaller y deltas won't help — the test depends on wall-clock speed). Consider making the moves larger (e.g. `[200.0, 100.0, 0.0]`) so even a slow-ish execution crosses 50 px/s.
2. The ticker isn't being pumped correctly. Verify `pipeline.drain_dirty_to_build_owner()` and `pipeline.perform_rebuilds()` are the right method names (check `pipeline.rs` for the actual names — the existing `test_scroll_controller_wired_on_mount_via_pipeline` at `scroll_view.rs:476-477` uses them).
3. The momentum block in `rebuild_from_state` isn't being reached. Add `dbg!(self.momentum.is_active())` at the top of the block.

- [ ] **Step 12: Write integration test — fling clamps at bottom edge**

Append:

```rust
#[test]
fn test_fling_clamps_at_bottom_edge() {
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
    let ticker = Arc::new(AnimationTicker::new());
    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.reconcile(Box::new(sv));
    let mut engine = crate::layout::TaffyLayoutEngine::new();
    let mut font_system = crate::resource::new_font_system();
    pipeline.layout(
        crate::core::Size::new(400.0, 600.0),
        &mut engine,
        &mut font_system,
    );

    // Hard upward fling from the middle.
    let press = InputEvent::PointerButton {
        position: Point::new(200.0, 500.0),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    pipeline.handle_event(
        Point::new(200.0, 500.0),
        &press,
        Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &test_clipboard(),
    );
    // Violent upward motion.
    for &y in &[400.0, 200.0, 0.0] {
        let mv = InputEvent::PointerMoved {
            position: Point::new(200.0, y),
        };
        pipeline.handle_event(
            Point::new(200.0, y),
            &mv,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
    }
    let release = InputEvent::PointerButton {
        position: Point::new(200.0, 0.0),
        button: PointerButton::Primary,
        state: ButtonState::Released,
    };
    pipeline.handle_event(
        Point::new(200.0, 0.0),
        &release,
        Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &test_clipboard(),
    );

    // Pump long enough for the fling to fully decay or hit the edge.
    for _ in 0..120 {
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
    }

    // Compute max_scroll the same way the element does.
    // We can't read it directly, but we know content > viewport, so just
    // assert the offset is bounded and stable.
    let final_offset = ctrl.current_offset();
    assert!(
        final_offset.is_finite(),
        "offset should be finite; got {}",
        final_offset
    );
    assert!(
        final_offset >= 0.0,
        "offset should be >= 0; got {}",
        final_offset
    );
    // It should have scrolled significantly (the fling was violent).
    assert!(
        final_offset > 100.0,
        "fling should have scrolled a lot; got {}",
        final_offset
    );
}
```

Note: this test does NOT assert an exact `max_scroll` value because that depends on Taffy's layout of 200 "row" texts, which varies. The assertion is behavioral: offset is finite, non-negative, and significantly scrolled. A stricter version would query the render object for `content_size().height` and assert `final_offset == max_scroll` — but that requires reaching into the render object registry from the test, which the existing tests don't do. Keep it behavioral.

- [ ] **Step 13: Run the edge-clamp test**

Run: `cargo test -p vexo elements::scroll_view::tests::test_fling_clamps_at_bottom_edge`
Expected: PASS.

- [ ] **Step 14: Write integration test — touch Down stops in-flight momentum**

Append:

```rust
#[test]
fn test_touch_down_stops_in_flight_momentum() {
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
    let ticker = Arc::new(AnimationTicker::new());
    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.reconcile(Box::new(sv));
    let mut engine = crate::layout::TaffyLayoutEngine::new();
    let mut font_system = crate::resource::new_font_system();
    pipeline.layout(
        crate::core::Size::new(400.0, 600.0),
        &mut engine,
        &mut font_system,
    );

    // Start a fling.
    let press = InputEvent::PointerButton {
        position: Point::new(200.0, 400.0),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    pipeline.handle_event(
        Point::new(200.0, 400.0),
        &press,
        Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &test_clipboard(),
    );
    for &y in &[350.0, 250.0, 100.0] {
        let mv = InputEvent::PointerMoved {
            position: Point::new(200.0, y),
        };
        pipeline.handle_event(
            Point::new(200.0, y),
            &mv,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
    }
    let release = InputEvent::PointerButton {
        position: Point::new(200.0, 100.0),
        button: PointerButton::Primary,
        state: ButtonState::Released,
    };
    pipeline.handle_event(
        Point::new(200.0, 100.0),
        &release,
        Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &test_clipboard(),
    );

    // Pump once to let momentum start.
    ticker.tick();
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    let offset_mid_fling = ctrl.current_offset();

    // New touch Down — should stop momentum.
    let press2 = InputEvent::PointerButton {
        position: Point::new(200.0, 300.0),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    pipeline.handle_event(
        Point::new(200.0, 300.0),
        &press2,
        Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &test_clipboard(),
    );

    // Pump several more times — offset should NOT change (momentum stopped).
    for _ in 0..10 {
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
    }
    assert_eq!(
        ctrl.current_offset(),
        offset_mid_fling,
        "touch Down should have stopped momentum; offset should be frozen"
    );
}
```

- [ ] **Step 15: Run the stop-on-touch test**

Run: `cargo test -p vexo elements::scroll_view::tests::test_touch_down_stops_in_flight_momentum`
Expected: PASS.

- [ ] **Step 16: Write integration test — programmatic jump cancels momentum**

Append:

```rust
#[test]
fn test_jump_to_cancels_momentum() {
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
    let ticker = Arc::new(AnimationTicker::new());
    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.reconcile(Box::new(sv));
    let mut engine = crate::layout::TaffyLayoutEngine::new();
    let mut font_system = crate::resource::new_font_system();
    pipeline.layout(
        crate::core::Size::new(400.0, 600.0),
        &mut engine,
        &mut font_system,
    );

    // Start a fling.
    let press = InputEvent::PointerButton {
        position: Point::new(200.0, 400.0),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    pipeline.handle_event(
        Point::new(200.0, 400.0),
        &press,
        Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &test_clipboard(),
    );
    for &y in &[350.0, 250.0, 100.0] {
        let mv = InputEvent::PointerMoved {
            position: Point::new(200.0, y),
        };
        pipeline.handle_event(
            Point::new(200.0, y),
            &mv,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
    }
    let release = InputEvent::PointerButton {
        position: Point::new(200.0, 100.0),
        button: PointerButton::Primary,
        state: ButtonState::Released,
    };
    pipeline.handle_event(
        Point::new(200.0, 100.0),
        &release,
        Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &test_clipboard(),
    );

    // Immediately jump to a specific offset.
    ctrl.jump_to(50.0);

    // Pump — the jump should win, momentum should be cancelled.
    for _ in 0..10 {
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
    }

    assert_eq!(
        ctrl.current_offset(),
        50.0,
        "jump_to should have cancelled momentum and applied; got {}",
        ctrl.current_offset()
    );
}
```

- [ ] **Step 17: Run the jump-cancels-momentum test**

Run: `cargo test -p vexo elements::scroll_view::tests::test_jump_to_cancels_momentum`
Expected: PASS.

- [ ] **Step 18: Write integration test — slow drag produces no momentum**

Append:

```rust
#[test]
fn test_slow_drag_no_momentum() {
    use crate::animation::AnimationTicker;
    use crate::core::Point;
    use crate::core::ScaleSource;
    use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
    use crate::widgets::{ScrollController, ScrollView};
    use crate::Flex;
    use crate::ThreeTreePipeline;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    let ctrl = ScrollController::new();
    let mut col = Flex::column();
    for _ in 0..200 {
        col = col.push(crate::Text::new("row"));
    }
    let sv = ScrollView::new(col.boxed()).controller(ctrl.clone());
    let ticker = Arc::new(AnimationTicker::new());
    let mut pipeline = ThreeTreePipeline::new(ticker.clone());
    pipeline.reconcile(Box::new(sv));
    let mut engine = crate::layout::TaffyLayoutEngine::new();
    let mut font_system = crate::resource::new_font_system();
    pipeline.layout(
        crate::core::Size::new(400.0, 600.0),
        &mut engine,
        &mut font_system,
    );

    // Press.
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
    // Slow moves: 10px each, 200ms apart → 50 px/s. Right at the threshold;
    // we want BELOW threshold, so make it 250ms apart → 40 px/s.
    for &y in &[290.0, 280.0, 270.0] {
        thread::sleep(Duration::from_millis(250));
        let mv = InputEvent::PointerMoved {
            position: Point::new(200.0, y),
        };
        pipeline.handle_event(
            Point::new(200.0, y),
            &mv,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
    }
    let offset_at_release = ctrl.current_offset();
    let release = InputEvent::PointerButton {
        position: Point::new(200.0, 270.0),
        button: PointerButton::Primary,
        state: ButtonState::Released,
    };
    pipeline.handle_event(
        Point::new(200.0, 270.0),
        &release,
        Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &test_clipboard(),
    );

    // Pump — no momentum should engage.
    for _ in 0..10 {
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
    }

    assert_eq!(
        ctrl.current_offset(),
        offset_at_release,
        "slow drag (below V_MIN_FLING) should not engage momentum; got {} before release, {} after pump",
        offset_at_release,
        ctrl.current_offset()
    );
}
```

Note: this test uses `thread::sleep` to control wall-clock timing, which makes it slower than the others (~1s) and slightly flaky under CI load. If it flakes, widen the sleep to 300ms or reduce the move count. The behavioral assertion is what matters.

- [ ] **Step 19: Run the slow-drag test**

Run: `cargo test -p vexo elements::scroll_view::tests::test_slow_drag_no_momentum`
Expected: PASS.

- [ ] **Step 20: Run the full scroll_view test suite**

Run: `cargo test -p vexo elements::scroll_view`
Expected: ALL tests PASS — both the existing ones (especially `test_mouse_wheel_still_works`, `test_drag_in_tappable_row_scrolls_not_navigates`, `test_tap_in_tappable_row_navigates_not_scrolls`, `test_multi_move_drag_accumulates_scroll`) and the 5 new ones.

- [ ] **Step 21: Run the full vexo test suite**

Run: `cargo test -p vexo`
Expected: ALL tests PASS. No regressions in any other module.

- [ ] **Step 22: Commit**

```bash
git add vexo/src/elements/scroll_view.rs
git commit -m "feat(scroll): add iOS-style inertial momentum on touch fling"
```

---

## Self-Review Checklist (run after writing the plan, before handoff)

**Spec coverage:**
- Decision 1 (momentum only, hard clamp): Task 2 physics (`MAX_DURATION` terminate, no spring), Task 3 Step 6 (`clamp_offset`, `hit_edge` stop). ✓
- Decision 2 (touch only): Task 3 wiring is entirely in `on_arena_winner_update`, which mouse wheel/keyboard don't go through. Existing `test_mouse_wheel_still_works` preserved (Task 3 Step 9). ✓
- Decision 3 (hard clamp at edge): Task 3 Step 6 `hit_edge` detection + `momentum.stop()`. ✓
- Decision 4 (last-N time-weighted, ~100ms): Task 1 `WINDOW = 100ms`, least-squares slope. ✓
- Decision 5 (50 px/s min fling): Task 3 Step 5 `V_MIN_FLING = 50.0`. ✓
- Physics constants (τ=0.325, V_STOP=13, MAX_DURATION=10): Task 2 `TAU`, `V_STOP`, `MAX_DURATION`. ✓
- Sign convention (negate tracker velocity): Task 3 Step 5 `let v = -self.velocity_tracker.velocity();` with comment. ✓
- Six termination conditions: Task 2 (`advance` returns None for decay + MAX_DURATION), Task 3 Step 3 (Down stops), Step 6 (edge stop), Step 6 (None case), Step 6 (jump_to stops via `self.momentum.stop()` in pending block), Step 7 (unmount stops). ✓
- Frame loop continuity: Task 2 `start` fires dirty callback immediately + registers with ticker; Task 3 Step 6 momentum block applies offset each rebuild. ✓
- Files touched match spec table: Tasks 1-3 cover all 5 files. ✓
- Testing (unit + integration): Task 1 (8 unit), Task 2 (11 unit), Task 3 (5 integration + existing regression run). ✓

**Placeholder scan:** No "TBD", "TODO", "implement later", "add appropriate error handling". Every code step has complete code. ✓

**Type consistency:**
- `VelocityTracker::add(&mut self, t: Instant, y: f32)` — Task 1 defines, Task 3 Step 4 uses `self.velocity_tracker.add(Instant::now(), position.y)`. ✓
- `VelocityTracker::velocity(&self) -> f32` — Task 1 defines, Task 3 Step 5 uses `self.velocity_tracker.velocity()`. ✓
- `VelocityTracker::clear(&mut self)` — Task 1 defines, Task 3 Step 3 uses `self.velocity_tracker.clear()`. ✓
- `MomentumSimulation::new() -> Self` — Task 2 defines, Task 3 Step 1 uses `MomentumSimulation::new()`. ✓
- `MomentumSimulation::start(&mut self, offset0, v0, now, dirty_sender, element_id, ticker)` — Task 2 defines with `(f32, f32, Instant, mpsc::Sender<ElementKey>, ElementKey, Arc<AnimationTicker>)`, Task 3 Step 5 calls with `(self.scroll_offset, v, Instant::now(), tx, element_id, ticker)`. Types match: `self.scroll_offset: f32`, `v: f32`, `Instant::now(): Instant`, `tx: mpsc::Sender<ElementKey>` (from `ctx.dirty_sender.cloned()`), `element_id: ElementKey` (from `self.id`), `ticker: Arc<AnimationTicker>` (from `self.animation_ticker.clone()`). ✓
- `MomentumSimulation::advance(&mut self, now: Instant) -> Option<f32>` — Task 2 defines, Task 3 Step 6 uses `self.momentum.advance(now)`. ✓
- `MomentumSimulation::stop(&mut self)` — Task 2 defines, Task 3 Steps 3/5(via start)/6/7 use `self.momentum.stop()`. ✓
- `MomentumSimulation::is_active(&self) -> bool` — Task 2 defines, Task 3 Step 6 uses `self.momentum.is_active()`. ✓

No issues found. Plan is ready.
