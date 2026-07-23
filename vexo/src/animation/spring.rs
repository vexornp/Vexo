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
        sim.start(
            offset0,
            v0,
            rest,
            now,
            tx,
            dummy_element_key(),
            ticker.clone(),
        );
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
        let mut max_offset: f32 = -40.0;
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
        assert!(
            result.is_some(),
            "spring should survive large dt without NaN"
        );
        // Continue and verify it still settles.
        for i in 1..=500 {
            if advance_by(&mut sim, start, 2.0 + i as f32 * DT).is_none() {
                break;
            }
        }
        assert!(
            !sim.is_active(),
            "spring should still settle after large dt"
        );
    }

    #[test]
    fn dirty_callback_fires_on_start() {
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
            ticker,
        );
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
