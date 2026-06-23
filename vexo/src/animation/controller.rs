use std::sync::Arc;
use std::time::{Duration, Instant};

use super::ticker::{AnimationTicker, TickHandle};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationDirection {
    Forward,
    Reverse,
    Stopped,
}

pub struct AnimationController {
    duration: Duration,
    value: f64,
    direction: AnimationDirection,
    start_time: Option<Instant>,
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    ticker: Option<Arc<AnimationTicker>>,
    tick_handle: Option<TickHandle>,
}

impl AnimationController {
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            value: 0.0,
            direction: AnimationDirection::Stopped,
            start_time: None,
            dirty_callback: None,
            ticker: None,
            tick_handle: None,
        }
    }

    pub fn forward(&mut self) {
        self.unregister_from_ticker();
        self.value = 0.0;
        self.direction = AnimationDirection::Forward;
        self.start_time = Some(Instant::now());
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
        self.value = 1.0;
        self.unregister_from_ticker();
        self.direction = AnimationDirection::Reverse;
        self.start_time = Some(Instant::now());
        if let (Some(ticker), Some(cb)) = (&self.ticker, &self.dirty_callback) {
            self.tick_handle = Some(ticker.register(cb.clone()));
        }
        if let Some(cb) = &self.dirty_callback {
            cb();
        }
    }

    pub fn stop(&mut self) {
        self.direction = AnimationDirection::Stopped;
        self.start_time = None;
        self.unregister_from_ticker();
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn direction(&self) -> AnimationDirection {
        self.direction
    }

    pub fn start_time(&self) -> Option<Instant> {
        self.start_time
    }

    pub fn set_dirty_callback(&mut self, cb: Arc<dyn Fn() + Send + Sync>) {
        self.dirty_callback = Some(cb);
    }

    pub fn set_ticker(&mut self, ticker: Arc<AnimationTicker>) {
        self.ticker = Some(ticker);
    }

    pub fn advance(&mut self, now: Instant) {
        if self.direction == AnimationDirection::Stopped {
            return;
        }
        if self.duration.is_zero() {
            self.value = match self.direction {
                AnimationDirection::Forward => 1.0,
                AnimationDirection::Reverse => 0.0,
                AnimationDirection::Stopped => return,
            };
            self.direction = AnimationDirection::Stopped;
            self.start_time = None;
            self.unregister_from_ticker();
            if let Some(cb) = &self.dirty_callback {
                cb();
            }
            return;
        }
        let start = self.start_time.unwrap();
        let elapsed = now.checked_duration_since(start).unwrap_or(Duration::ZERO).as_secs_f64();
        let duration = self.duration.as_secs_f64();
        let raw = elapsed / duration;

        self.value = match self.direction {
            AnimationDirection::Forward => raw.min(1.0),
            AnimationDirection::Reverse => (1.0 - raw).max(0.0),
            AnimationDirection::Stopped => return,
        };

        if self.value >= 1.0 || self.value <= 0.0 {
            self.direction = AnimationDirection::Stopped;
            self.start_time = None;
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc;

    #[test]
    fn test_controller_new() {
        let ctrl = AnimationController::new(Duration::from_millis(300));
        assert_eq!(ctrl.value(), 0.0);
        assert_eq!(ctrl.direction, AnimationDirection::Stopped);
    }

    #[test]
    fn test_controller_forward_starts() {
        let mut ctrl = AnimationController::new(Duration::from_millis(300));
        ctrl.forward();
        assert_eq!(ctrl.direction, AnimationDirection::Forward);
        assert!(ctrl.start_time.is_some());
    }

    #[test]
    fn test_controller_advance_forward() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.forward();
        let start = ctrl.start_time.unwrap();
        let now = start + Duration::from_millis(500);
        ctrl.advance(now);
        assert!((ctrl.value() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_controller_advance_completes_forward() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.forward();
        let start = ctrl.start_time.unwrap();
        let now = start + Duration::from_millis(1001);
        ctrl.advance(now);
        assert!((ctrl.value() - 1.0).abs() < 0.01);
        assert_eq!(ctrl.direction, AnimationDirection::Stopped);
    }

    #[test]
    fn test_controller_advance_reverse() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.reverse();
        let start = ctrl.start_time.unwrap();
        let now = start + Duration::from_millis(500);
        ctrl.advance(now);
        assert!((ctrl.value() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_controller_advance_completes_reverse() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.reverse();
        let start = ctrl.start_time.unwrap();
        let now = start + Duration::from_millis(1001);
        ctrl.advance(now);
        assert!((ctrl.value() - 0.0).abs() < 0.01);
        assert_eq!(ctrl.direction, AnimationDirection::Stopped);
    }

    #[test]
    fn test_controller_stop() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.forward();
        ctrl.stop();
        assert_eq!(ctrl.direction, AnimationDirection::Stopped);
        assert!(ctrl.start_time.is_none());
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
        let start = ctrl.start_time.unwrap();
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
        let start = ctrl.start_time.unwrap();
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
        assert_eq!(ctrl.direction, AnimationDirection::Stopped);
    }

    #[test]
    fn test_controller_zero_duration_reverse() {
        let mut ctrl = AnimationController::new(Duration::ZERO);
        ctrl.reverse();
        ctrl.advance(Instant::now());
        assert!(ctrl.value().abs() < 0.001);
        assert_eq!(ctrl.direction, AnimationDirection::Stopped);
    }
}
