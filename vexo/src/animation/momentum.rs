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
        let dirty_sender_for_cb = dirty_sender.clone();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = dirty_sender_for_cb.send(element_id);
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

impl Drop for MomentumSimulation {
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

    fn start_sim(
        v0: f32,
    ) -> (
        MomentumSimulation,
        Instant,
        mpsc::Receiver<ElementKey>,
        Arc<AnimationTicker>,
    ) {
        let (tx, rx) = mpsc::channel();
        let ticker = Arc::new(AnimationTicker::new());
        let mut sim = MomentumSimulation::new();
        let now = Instant::now();
        sim.start(0.0, v0, now, tx, dummy_element_key(), ticker.clone());
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
        assert!(
            (offset - expected).abs() < 1.0,
            "got {} expected {}",
            offset,
            expected
        );
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
        assert!(
            sim.advance(just_after).is_none(),
            "below V_STOP → terminate"
        );
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
        sim.start(0.0, 1000.0, Instant::now(), tx, dummy_element_key(), ticker);
        // start() fires the dirty callback immediately.
        assert!(rx.try_recv().is_ok(), "dirty callback should fire on start");
    }

    #[test]
    fn ticker_tick_fires_dirty_callback_after_start() {
        let (tx, rx) = mpsc::channel();
        let ticker = Arc::new(AnimationTicker::new());
        let mut sim = MomentumSimulation::new();
        sim.start(
            0.0,
            1000.0,
            Instant::now(),
            tx,
            dummy_element_key(),
            ticker.clone(),
        );
        // Drain the immediate fire from start().
        let _ = rx.try_recv();
        ticker.tick();
        assert!(
            rx.try_recv().is_ok(),
            "ticker.tick() should fire the registered callback"
        );
    }
}
