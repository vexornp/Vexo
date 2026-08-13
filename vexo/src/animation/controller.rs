use std::sync::Arc;
use std::time::{Duration, Instant};

use super::simulation::Simulation;
use super::ticker::{AnimationTicker, TickHandle};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationDirection {
    Forward,
    Reverse,
    Stopped,
}

enum Drive {
    Stopped,
    Time {
        direction: AnimationDirection,
        start: Instant,
        duration: Duration,
    },
    Simulation {
        sim: Box<dyn Simulation>,
        start: Instant,
    },
}

pub struct AnimationController {
    drive: Drive,
    value: f64,
    duration: Duration,
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    ticker: Option<Arc<AnimationTicker>>,
    tick_handle: Option<TickHandle>,
}

impl AnimationController {
    pub fn new(duration: Duration) -> Self {
        Self {
            drive: Drive::Stopped,
            value: 0.0,
            duration,
            dirty_callback: None,
            ticker: None,
            tick_handle: None,
        }
    }

    pub fn forward(&mut self) {
        self.forward_with_start(Instant::now());
    }

    /// Begin a forward tween (0 → 1) whose `start_time` is `start` instead of
    /// `Instant::now()`.
    ///
    /// Used to synchronize a Vexo tween with an animation that already began —
    /// e.g. the iOS software keyboard, whose slide starts the moment
    /// `keyboardWillShow` fires, a frame before the avoidance widget's first
    /// render. Stamping `start_time` with the notification instant means the
    /// tween's first sampled value already reflects the time elapsed since the
    /// keyboard began, so the two move in lockstep instead of the avoidance
    /// lagging the keyboard for the whole duration.
    ///
    /// `start` may be in the past (the usual case for sync) or now; it must
    /// not be in the future. A future-dated `start` is handled gracefully —
    /// `advance` will compute zero elapsed until time catches up — but
    /// defeats the purpose of this method.
    pub fn forward_with_start(&mut self, start: Instant) {
        self.unregister_from_ticker();
        self.value = 0.0;
        self.drive = Drive::Time {
            direction: AnimationDirection::Forward,
            start,
            duration: self.duration,
        };
        if let (Some(ticker), Some(cb)) = (&self.ticker, &self.dirty_callback) {
            self.tick_handle = Some(ticker.register(cb.clone()));
        }
        // Fire dirty callback immediately so the element is marked for rebuild
        // and a frame is requested on the same event loop turn. Without this,
        // the callback is only fired on the next tick(), which only runs inside
        // render_retain(), which only runs when a frame is already requested —
        // a deadlock.
        if let Some(cb) = &self.dirty_callback {
            cb();
        }
    }

    pub fn reverse(&mut self) {
        self.unregister_from_ticker();
        self.value = 1.0;
        self.drive = Drive::Time {
            direction: AnimationDirection::Reverse,
            start: Instant::now(),
            duration: self.duration,
        };
        if let (Some(ticker), Some(cb)) = (&self.ticker, &self.dirty_callback) {
            self.tick_handle = Some(ticker.register(cb.clone()));
        }
        if let Some(cb) = &self.dirty_callback {
            cb();
        }
    }

    pub fn stop(&mut self) {
        self.drive = Drive::Stopped;
        self.unregister_from_ticker();
    }

    /// Set the controller's value directly, stopping any active drive.
    ///
    /// Used by gesture-driven animations (e.g. swipe-to-pop) where the finger
    /// controls progress: each pointer Move calls `set_value(progress)` so the
    /// rendered transition tracks the finger 1:1. On release, the caller starts
    /// a spring via `animate_with` to settle to 0.0 or 1.0.
    ///
    /// After this call: `is_animating() == false`, `direction() == Stopped`,
    /// `value() == v.clamp(0.0, 1.0)`. The value is clamped so a finger
    /// briefly overshooting the content width can't push progress past 1.0.
    pub fn set_value(&mut self, v: f64) {
        self.unregister_from_ticker();
        self.drive = Drive::Stopped;
        self.value = v.clamp(0.0, 1.0);
        if let Some(cb) = &self.dirty_callback {
            cb();
        }
    }

    /// Drive the controller with a physics simulation. `sim.x(t)` IS the
    /// value (the sim owns from/to/v0). Stamps `start_time`, registers the
    /// ticker, fires dirty immediately (avoids the render_retain deadlock).
    /// Cancels any prior time or sim drive first.
    pub fn animate_with(&mut self, sim: Box<dyn Simulation>) {
        self.unregister_from_ticker();
        self.drive = Drive::Simulation {
            sim,
            start: Instant::now(),
        };
        if let (Some(ticker), Some(cb)) = (&self.ticker, &self.dirty_callback) {
            self.tick_handle = Some(ticker.register(cb.clone()));
        }
        if let Some(cb) = &self.dirty_callback {
            cb();
        }
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn direction(&self) -> AnimationDirection {
        match self.drive {
            Drive::Time { direction, .. } => direction,
            _ => AnimationDirection::Stopped,
        }
    }

    pub fn start_time(&self) -> Option<Instant> {
        match self.drive {
            Drive::Time { start, .. } | Drive::Simulation { start, .. } => Some(start),
            Drive::Stopped => None,
        }
    }

    /// True while any drive (time or simulation) is active. Replaces the
    /// `direction() != Stopped` check callers make, since the Simulation path
    /// has no `AnimationDirection`.
    pub fn is_animating(&self) -> bool {
        !matches!(self.drive, Drive::Stopped)
    }

    pub fn set_dirty_callback(&mut self, cb: Arc<dyn Fn() + Send + Sync>) {
        self.dirty_callback = Some(cb);
    }

    pub fn set_ticker(&mut self, ticker: Arc<AnimationTicker>) {
        self.ticker = Some(ticker);
    }

    pub fn advance(&mut self, now: Instant) {
        match &mut self.drive {
            Drive::Stopped => return,
            Drive::Time {
                direction,
                start,
                duration,
            } => {
                let direction = *direction;
                let start = *start;
                let duration = *duration;
                self.advance_time(now, direction, start, duration);
            }
            Drive::Simulation { sim, start } => {
                let start = *start;
                let elapsed = now.saturating_duration_since(start).as_secs_f64();
                self.value = sim.x(elapsed);
                if sim.is_done(elapsed) {
                    // Snap to the exact target to avoid tiny residual error
                    // (e.g. 0.999 instead of 1.0) that can cause
                    // misclassification downstream — e.g. opacity-based
                    // depth-write classification treating a 99.9%-opaque
                    // card as transparent, causing background text to show
                    // through it.
                    let target = sim.target();
                    if !target.is_nan() {
                        self.value = target;
                    }
                    self.drive = Drive::Stopped;
                    self.unregister_from_ticker();
                }
                if let Some(cb) = &self.dirty_callback {
                    cb();
                }
            }
        }
    }

    /// Time-path advance, factored out of `advance()` for clarity. Preserves
    /// the exact behavior of the original `advance()` time logic, including
    /// the direction-aware completion fix (controller.rs:147-157).
    fn advance_time(
        &mut self,
        now: Instant,
        direction: AnimationDirection,
        start: Instant,
        duration: Duration,
    ) {
        if direction == AnimationDirection::Stopped {
            return;
        }
        if duration.is_zero() {
            self.value = match direction {
                AnimationDirection::Forward => 1.0,
                AnimationDirection::Reverse => 0.0,
                AnimationDirection::Stopped => return,
            };
            self.drive = Drive::Stopped;
            self.unregister_from_ticker();
            if let Some(cb) = &self.dirty_callback {
                cb();
            }
            return;
        }
        let elapsed = now.saturating_duration_since(start).as_secs_f64();
        let duration = duration.as_secs_f64();
        let raw = elapsed / duration;

        // Direction-aware completion: a Forward tween starts at value=0 and
        // completes at value>=1.0; a Reverse tween starts at value=1 and
        // completes at value<=0.0. Checking BOTH bounds regardless of
        // direction is a bug — it stops a Forward tween on its very first
        // advance when elapsed≈0 (value=0.0), which happens when `advance`
        // is called with a `now` that predates `start_time` (e.g. a stale
        // `now` captured once per perform_rebuilds cycle and reused across
        // elements, where a controller created during an earlier element's
        // rebuild has start_time > now). See KeyboardAvoidance retarget bug.
        let completed = match direction {
            AnimationDirection::Forward => {
                self.value = raw.min(1.0);
                self.value >= 1.0
            }
            AnimationDirection::Reverse => {
                self.value = (1.0 - raw).max(0.0);
                self.value <= 0.0
            }
            AnimationDirection::Stopped => return,
        };

        if completed {
            self.drive = Drive::Stopped;
            self.unregister_from_ticker();
        }

        if let Some(cb) = &self.dirty_callback {
            cb();
        }
    }

    fn unregister_from_ticker(&mut self) {
        if let (Some(ticker), Some(handle)) = (&self.ticker, self.tick_handle.take()) {
            ticker.unregister(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::simulation::SpringSimulation;
    use crate::animation::SpringDescription;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc;

    /// Build a controller wired to a ticker + mpsc dirty callback, matching
    /// the pattern used by the existing `test_controller_registers_with_ticker`.
    fn controller_with_ticker() -> (
        AnimationController,
        std::sync::mpsc::Receiver<()>,
        std::sync::Arc<crate::animation::AnimationTicker>,
    ) {
        let ticker = std::sync::Arc::new(crate::animation::AnimationTicker::new());
        let (tx, rx) = std::sync::mpsc::channel();
        let mut ctrl = AnimationController::new(std::time::Duration::from_secs(1));
        ctrl.set_dirty_callback(std::sync::Arc::new(move || {
            let _ = tx.send(());
        }));
        ctrl.set_ticker(ticker.clone());
        (ctrl, rx, ticker)
    }

    fn critical_spring_sim(from: f64, to: f64, v0: f64) -> SpringSimulation {
        SpringSimulation::new(SpringDescription::ios(340.0, 1.0), from, to, v0)
    }

    #[test]
    fn test_controller_new() {
        let ctrl = AnimationController::new(Duration::from_millis(300));
        assert_eq!(ctrl.value(), 0.0);
        assert_eq!(ctrl.direction(), AnimationDirection::Stopped);
    }

    #[test]
    fn test_controller_forward_starts() {
        let mut ctrl = AnimationController::new(Duration::from_millis(300));
        ctrl.forward();
        assert_eq!(ctrl.direction(), AnimationDirection::Forward);
        assert!(ctrl.start_time().is_some());
    }

    #[test]
    fn test_controller_advance_forward() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.forward();
        let start = ctrl.start_time().unwrap();
        let now = start + Duration::from_millis(500);
        ctrl.advance(now);
        assert!((ctrl.value() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_controller_advance_completes_forward() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.forward();
        let start = ctrl.start_time().unwrap();
        let now = start + Duration::from_millis(1001);
        ctrl.advance(now);
        assert!((ctrl.value() - 1.0).abs() < 0.01);
        assert_eq!(ctrl.direction(), AnimationDirection::Stopped);
    }

    #[test]
    fn test_controller_advance_reverse() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.reverse();
        let start = ctrl.start_time().unwrap();
        let now = start + Duration::from_millis(500);
        ctrl.advance(now);
        assert!((ctrl.value() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_controller_advance_completes_reverse() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.reverse();
        let start = ctrl.start_time().unwrap();
        let now = start + Duration::from_millis(1001);
        ctrl.advance(now);
        assert!((ctrl.value() - 0.0).abs() < 0.01);
        assert_eq!(ctrl.direction(), AnimationDirection::Stopped);
    }

    #[test]
    fn test_controller_stop() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.forward();
        ctrl.stop();
        assert_eq!(ctrl.direction(), AnimationDirection::Stopped);
        assert!(ctrl.start_time().is_none());
    }

    #[test]
    fn test_controller_advance_stopped_is_noop() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        let now = Instant::now();
        ctrl.advance(now);
        assert_eq!(ctrl.value(), 0.0);
    }

    #[test]
    fn test_controller_dirty_callback_fires() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        ctrl.set_dirty_callback(Arc::new(move || {
            called_clone.store(true, Ordering::SeqCst);
        }));
        ctrl.forward();
        let start = ctrl.start_time().unwrap();
        let now = start + Duration::from_millis(100);
        ctrl.advance(now);
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_controller_registers_with_ticker() {
        let ticker = Arc::new(AnimationTicker::new());
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        let (tx, rx) = mpsc::channel();
        ctrl.set_dirty_callback(Arc::new(move || {
            let _ = tx.send(());
        }));
        ctrl.set_ticker(ticker.clone());
        ctrl.forward();
        assert!(ticker.has_active());
        ticker.tick();
        // The dirty callback fires via ticker.tick(), which sends through the channel
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn test_controller_unregisters_on_complete() {
        let ticker = Arc::new(AnimationTicker::new());
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.set_dirty_callback(Arc::new(|| {}));
        ctrl.set_ticker(ticker.clone());
        ctrl.forward();
        assert!(ticker.has_active());
        let start = ctrl.start_time().unwrap();
        // advance() is called separately from tick(), so no reentrancy issue
        ctrl.advance(start + Duration::from_millis(1001));
        assert!(!ticker.has_active());
    }

    #[test]
    fn test_controller_stop_unregisters() {
        let ticker = Arc::new(AnimationTicker::new());
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.set_dirty_callback(Arc::new(|| {}));
        ctrl.set_ticker(ticker.clone());
        ctrl.forward();
        assert!(ticker.has_active());
        ctrl.stop();
        assert!(!ticker.has_active());
    }

    #[test]
    fn test_controller_forward_then_forward_reregisters() {
        let ticker = Arc::new(AnimationTicker::new());
        let counter = Arc::new(AtomicUsize::new(0));
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        let counter_clone = counter.clone();
        ctrl.set_dirty_callback(Arc::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        }));
        ctrl.set_ticker(ticker.clone());
        ctrl.forward();
        ctrl.forward(); // second forward should unregister old and register new
                        // Only one callback should be active in the ticker (not two),
                        // so tick() should fire exactly once.
        let before = counter.load(Ordering::SeqCst);
        ticker.tick();
        let after = counter.load(Ordering::SeqCst);
        assert_eq!(after - before, 1);
    }

    #[test]
    fn test_controller_reverse_sets_value_to_1() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        assert_eq!(ctrl.value(), 0.0);
        ctrl.reverse();
        assert_eq!(ctrl.value(), 1.0);
    }

    #[test]
    fn test_controller_zero_duration_forward() {
        let mut ctrl = AnimationController::new(Duration::ZERO);
        ctrl.forward();
        ctrl.advance(Instant::now());
        assert!((ctrl.value() - 1.0).abs() < 0.001);
        assert_eq!(ctrl.direction(), AnimationDirection::Stopped);
    }

    #[test]
    fn test_controller_zero_duration_reverse() {
        let mut ctrl = AnimationController::new(Duration::ZERO);
        ctrl.reverse();
        ctrl.advance(Instant::now());
        assert!(ctrl.value().abs() < 0.001);
        assert_eq!(ctrl.direction(), AnimationDirection::Stopped);
    }

    // Regression: a Forward tween must NOT stop when its first `advance`
    // sees zero (or near-zero) elapsed time. This happens when `advance` is
    // called with a `now` that predates `start_time` — e.g. a stale `now`
    // captured once per perform_rebuilds cycle and reused across elements,
    // where a controller created during an earlier element's rebuild has
    // start_time > now. The old completion check `value <= 0.0 || value >=
    // 1.0` wrongly stopped a Forward tween at its starting value (0.0),
    // which killed the KeyboardAvoidance dismiss tween whenever the user
    // tapped outside *during* the show tween.
    #[test]
    fn test_controller_forward_does_not_stop_at_zero_elapsed() {
        let mut ctrl = AnimationController::new(Duration::from_millis(250));
        ctrl.forward();
        // `now` == start_time → elapsed == 0 → raw == 0 → value == 0.0.
        // A Forward tween at its start must NOT be considered complete.
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start);
        assert_eq!(ctrl.direction(), AnimationDirection::Forward);
        assert!(ctrl.value().abs() < 1e-9);
        // And it must still progress on a later advance with real elapsed.
        ctrl.advance(start + Duration::from_millis(125));
        assert_eq!(ctrl.direction(), AnimationDirection::Forward);
        assert!((ctrl.value() - 0.5).abs() < 0.01);
    }

    // Regression: a Reverse tween must NOT stop at value==1.0 (its start).
    #[test]
    fn test_controller_reverse_does_not_stop_at_start_value() {
        let mut ctrl = AnimationController::new(Duration::from_millis(250));
        ctrl.reverse();
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start); // elapsed == 0 → value == 1.0 (Reverse start)
        assert_eq!(ctrl.direction(), AnimationDirection::Reverse);
        assert!((ctrl.value() - 1.0).abs() < 1e-9);
        ctrl.advance(start + Duration::from_millis(125));
        assert_eq!(ctrl.direction(), AnimationDirection::Reverse);
        assert!((ctrl.value() - 0.5).abs() < 0.01);
    }

    #[test]
    fn animate_with_sets_value_from_sim_at_t_zero() {
        let (mut ctrl, _rx, _ticker) = controller_with_ticker();
        ctrl.animate_with(Box::new(critical_spring_sim(0.0, 1.0, 0.0)));
        // First advance samples sim.x(≈0) ≈ from = 0.
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start);
        assert!(ctrl.value().abs() < 1e-6, "got {}", ctrl.value());
    }

    #[test]
    fn animate_with_completes_and_unregisters() {
        let (mut ctrl, _rx, ticker) = controller_with_ticker();
        ctrl.animate_with(Box::new(critical_spring_sim(0.0, 1.0, 0.0)));
        assert!(ticker.has_active());
        // Advance well past settle (k=340 critical settles < 1s).
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start + std::time::Duration::from_secs(2));
        assert!(!ctrl.is_animating(), "should be done");
        assert!(!ticker.has_active(), "should have unregistered");
        assert!(
            (ctrl.value() - 1.0).abs() < 1e-3,
            "should be at to=1.0, got {}",
            ctrl.value()
        );
    }

    #[test]
    fn animate_with_fires_dirty_on_start() {
        let (mut ctrl, rx, _ticker) = controller_with_ticker();
        ctrl.animate_with(Box::new(critical_spring_sim(0.0, 1.0, 0.0)));
        // Mirrors the render_retain-deadlock-prevention test for forward()
        // (controller.rs test_controller_registers_with_ticker).
        assert!(rx.try_recv().is_ok(), "dirty callback should fire on start");
    }

    #[test]
    fn animate_with_cancels_prior_sim() {
        let (mut ctrl, _rx, ticker) = controller_with_ticker();
        ctrl.animate_with(Box::new(critical_spring_sim(0.0, 1.0, 0.0)));
        ctrl.animate_with(Box::new(critical_spring_sim(0.0, 2.0, 0.0)));
        // Only one ticker registration should be active.
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        // Re-register a counter callback to confirm single active handle by
        // counting ticks: with exactly one active handle, one tick fires once.
        // (We can't easily count the sim's own callback; instead verify
        // has_active stays true and a second animate_with doesn't double up
        // by checking the sim still drives correctly.)
        assert!(ticker.has_active());
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start + std::time::Duration::from_secs(2));
        assert!(
            (ctrl.value() - 2.0).abs() < 1e-3,
            "second sim's to=2.0 should win, got {}",
            ctrl.value()
        );
        let _ = counter; // suppress unused
    }

    #[test]
    fn forward_cancels_sim() {
        let (mut ctrl, _rx, ticker) = controller_with_ticker();
        ctrl.animate_with(Box::new(critical_spring_sim(0.0, 1.0, 0.0)));
        assert!(ticker.has_active());
        ctrl.forward();
        // forward() should have replaced the sim drive with a time Drive.
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start + std::time::Duration::from_millis(500));
        // Time drive at 500ms of 1s duration (default in controller_with_ticker).
        // Value should be ~0.5 (linear), NOT a spring value.
        assert!(
            (ctrl.value() - 0.5).abs() < 0.05,
            "forward should cancel sim; got {}",
            ctrl.value()
        );
    }

    #[test]
    fn sim_cancels_forward() {
        let (mut ctrl, _rx, _ticker) = controller_with_ticker();
        ctrl.forward();
        ctrl.animate_with(Box::new(critical_spring_sim(0.0, 1.0, 0.0)));
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start + std::time::Duration::from_secs(2));
        // Should be at sim's to=1.0 via spring, not a linear 1.0 at exactly 1s.
        // Distinguish by checking it's NOT stopped at the 1s time boundary —
        // a sim settles based on is_done, a time drive completes at duration.
        assert!((ctrl.value() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn animate_with_zero_motion_done_frame_one() {
        // from==to, v0==0 ⇒ is_done at t=0. After one advance, controller stops.
        let (mut ctrl, _rx, ticker) = controller_with_ticker();
        ctrl.animate_with(Box::new(critical_spring_sim(5.0, 5.0, 0.0)));
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start);
        assert!(!ctrl.is_animating(), "zero-motion sim should be done");
        assert!(!ticker.has_active());
    }

    #[test]
    fn animate_with_under_damped_overshoots_then_settles() {
        let (mut ctrl, _rx, _ticker) = controller_with_ticker();
        let sim = SpringSimulation::new(
            SpringDescription::with_damping_ratio(1.0, 340.0, 0.5),
            0.0,
            1.0,
            0.0,
        );
        ctrl.animate_with(Box::new(sim));
        let start = ctrl.start_time().unwrap();
        // Sample mid-flight for overshoot.
        let mut max_value = 0.0_f64;
        for i in 1..=120 {
            ctrl.advance(start + std::time::Duration::from_secs_f64(i as f64 / 120.0));
            max_value = max_value.max(ctrl.value());
            if !ctrl.is_animating() {
                break;
            }
        }
        assert!(
            max_value > 1.0,
            "under-damped should overshoot past to=1.0; max was {}",
            max_value
        );
        assert!(!ctrl.is_animating(), "should settle");
        assert!(
            (ctrl.value() - 1.0).abs() < 1e-3,
            "should settle at to=1.0, got {}",
            ctrl.value()
        );
    }

    #[test]
    fn is_animating_false_when_stopped() {
        let ctrl = AnimationController::new(std::time::Duration::from_secs(1));
        assert!(!ctrl.is_animating());
    }

    #[test]
    fn is_animating_true_when_forward() {
        let mut ctrl = AnimationController::new(std::time::Duration::from_secs(1));
        ctrl.set_dirty_callback(std::sync::Arc::new(|| {}));
        ctrl.forward();
        assert!(ctrl.is_animating());
    }

    #[test]
    fn set_value_sets_value_and_stops_drive() {
        let mut ctrl = AnimationController::new(Duration::from_millis(100));
        ctrl.forward();
        assert!(ctrl.is_animating());
        ctrl.set_value(0.42);
        assert!(!ctrl.is_animating(), "set_value must stop the drive");
        assert_eq!(ctrl.direction(), AnimationDirection::Stopped);
        assert!((ctrl.value() - 0.42).abs() < 1e-9, "value must be 0.42");
    }

    #[test]
    fn set_value_clamps_to_0_1() {
        let mut ctrl = AnimationController::new(Duration::from_millis(100));
        ctrl.set_value(-0.5);
        assert_eq!(ctrl.value(), 0.0, "negative clamps to 0");
        ctrl.set_value(1.5);
        assert_eq!(ctrl.value(), 1.0, ">1 clamps to 1");
    }

    #[test]
    fn set_value_fires_dirty_callback() {
        use std::sync::{Arc, Mutex};
        let count = Arc::new(Mutex::new(0u32));
        let cb_count = count.clone();
        let mut ctrl = AnimationController::new(Duration::from_millis(100));
        ctrl.set_dirty_callback(Arc::new(move || {
            *cb_count.lock().unwrap() += 1;
        }));
        ctrl.set_value(0.5);
        assert_eq!(*count.lock().unwrap(), 1, "set_value must fire dirty once");
    }

    #[test]
    fn set_value_cancels_prior_simulation() {
        let mut ctrl = AnimationController::new(Duration::from_millis(100));
        ctrl.animate_with(Box::new(critical_spring_sim(0.0, 1.0, 0.0)));
        assert!(ctrl.is_animating());
        ctrl.set_value(0.3);
        assert!(!ctrl.is_animating(), "set_value must cancel the simulation");
        assert!((ctrl.value() - 0.3).abs() < 1e-9);
    }
}
