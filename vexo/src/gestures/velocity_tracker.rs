//! VelocityTracker — windowed least-squares velocity estimation from pointer samples.
//!
//! Pure value type. No framework dependencies.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Window over which samples contribute to velocity estimation. Matches iOS/Flutter.
const WINDOW: Duration = Duration::from_millis(100);
/// Epsilon for the least-squares denominator guard. Samples sharing an
/// identical timestamp produce `denom == 0`; we treat anything below this
/// as degenerate and return `0.0` rather than dividing by ~zero.
const DENOM_EPSILON: f64 = 1e-12;

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
        // Evict samples older than the window, but always retain at least two
        // so velocity() can produce an estimate. Without this guard, a sample
        // intended to sit exactly on the window edge can be dropped due to
        // sub-microsecond timestamp jitter, collapsing the buffer to one sample.
        while self.samples.len() > 2 {
            match self.samples.front() {
                Some(&(front_t, _)) if front_t < cutoff => {
                    self.samples.pop_front();
                }
                _ => break,
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
        if denom.abs() < DENOM_EPSILON {
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

    #[test]
    fn degenerate_denominator_returns_zero() {
        // Two samples at the same timestamp → denom == 0 → guard fires.
        let now = Instant::now();
        let mut vt = VelocityTracker::new();
        vt.add(now, 100.0);
        vt.add(now, 200.0);
        assert_eq!(vt.velocity(), 0.0);
    }

    #[test]
    fn default_produces_empty_tracker() {
        let vt = VelocityTracker::default();
        assert_eq!(vt.velocity(), 0.0);
    }
}
