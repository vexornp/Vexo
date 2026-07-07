//! Curves for easing animation progress.
//!
//! A `Curve` transforms a linear time value `t ∈ [0, 1]` into an eased
//! value `t' ∈ [0, 1]`. Curves are pure functions — they hold no state
//! and may be reused across animations.
//!
//! `CurvedAnimation` wraps a borrowed `AnimationController` + a `Curve`,
//! exposing `value()` that applies the curve to the controller's raw
//! linear value. It does not register its own dirty callback — it
//! piggybacks on the controller's.
//!
//! This matches Flutter's `Curve` class and `CurvedAnimation`.

use super::controller::AnimationController;

/// A curve transforms a linear progress value `t ∈ [0, 1]` into an eased
/// value `t' ∈ [0, 1]`.
///
/// Curves are pure functions: the same `t` always produces the same `t'`.
/// Implementations must be `Send + Sync` so they can be stored in
/// `Component`s and shared across threads if needed.
pub trait Curve: Send + Sync {
    fn transform(&self, t: f64) -> f64;
}

/// Identity curve: `t' = t`. Useful as a baseline and in tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinearCurve;

impl Curve for LinearCurve {
    fn transform(&self, t: f64) -> f64 {
        t
    }
}

/// Ease-in curve: `t' = t²`. Accelerates from rest.
#[derive(Debug, Clone, Copy, Default)]
pub struct EaseInCurve;

impl Curve for EaseInCurve {
    fn transform(&self, t: f64) -> f64 {
        t * t
    }
}

/// Ease-out curve: `t' = 1 - (1 - t)²`. Decelerates to rest.
/// iOS page transitions use this family.
#[derive(Debug, Clone, Copy, Default)]
pub struct EaseOutCurve;

impl Curve for EaseOutCurve {
    fn transform(&self, t: f64) -> f64 {
        1.0 - (1.0 - t) * (1.0 - t)
    }
}

/// Ease-in-out curve: piecewise blend of `EaseInCurve` (first half) and
/// `EaseOutCurve` (second half). Accelerates then decelerates.
#[derive(Debug, Clone, Copy, Default)]
pub struct EaseInOutCurve;

impl Curve for EaseInOutCurve {
    fn transform(&self, t: f64) -> f64 {
        if t < 0.5 {
            // ease-in half: 2 * t²
            2.0 * t * t
        } else {
            // ease-out half: 1 - (-2t + 2)² / 2
            1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
        }
    }
}

/// Wraps a borrowed `AnimationController` + a `Curve`, exposing `value()`
/// that applies the curve to the controller's raw linear value.
///
/// Does not register its own dirty callback — piggybacks on the
/// controller's. The controller is borrowed, not owned, so the
/// `CurvedAnimation` cannot outlive the controller.
pub struct CurvedAnimation<'a> {
    parent: &'a AnimationController,
    curve: Box<dyn Curve>,
}

impl<'a> CurvedAnimation<'a> {
    /// Create a new `CurvedAnimation` wrapping `parent` with the given `curve`.
    pub fn new(parent: &'a AnimationController, curve: Box<dyn Curve>) -> Self {
        Self { parent, curve }
    }

    /// The eased value: `curve.transform(parent.value())`.
    pub fn value(&self) -> f64 {
        self.curve.transform(self.parent.value())
    }

    /// The raw (un-eased) value of the parent controller.
    pub fn parent_value(&self) -> f64 {
        self.parent.value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn test_linear_curve_endpoints() {
        let c = LinearCurve;
        assert_eq!(c.transform(0.0), 0.0);
        assert_eq!(c.transform(1.0), 1.0);
    }

    #[test]
    fn test_linear_curve_midpoint() {
        let c = LinearCurve;
        assert!((c.transform(0.5) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_ease_in_endpoints() {
        let c = EaseInCurve;
        assert_eq!(c.transform(0.0), 0.0);
        assert_eq!(c.transform(1.0), 1.0);
    }

    #[test]
    fn test_ease_in_midpoint() {
        let c = EaseInCurve;
        // t=0.5 → 0.25
        assert!((c.transform(0.5) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn test_ease_in_monotonic() {
        let c = EaseInCurve;
        let mut prev = 0.0;
        for i in 0..=100 {
            let t = i as f64 / 100.0;
            let v = c.transform(t);
            assert!(v >= prev, "not monotonic at t={}: {} < {}", t, v, prev);
            prev = v;
        }
    }

    #[test]
    fn test_ease_out_endpoints() {
        let c = EaseOutCurve;
        assert_eq!(c.transform(0.0), 0.0);
        assert_eq!(c.transform(1.0), 1.0);
    }

    #[test]
    fn test_ease_out_midpoint() {
        let c = EaseOutCurve;
        // t=0.5 → 1 - 0.25 = 0.75
        assert!((c.transform(0.5) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn test_ease_out_monotonic() {
        let c = EaseOutCurve;
        let mut prev = 0.0;
        for i in 0..=100 {
            let t = i as f64 / 100.0;
            let v = c.transform(t);
            assert!(v >= prev, "not monotonic at t={}: {} < {}", t, v, prev);
            prev = v;
        }
    }

    #[test]
    fn test_ease_in_out_endpoints() {
        let c = EaseInOutCurve;
        assert_eq!(c.transform(0.0), 0.0);
        assert_eq!(c.transform(1.0), 1.0);
    }

    #[test]
    fn test_ease_in_out_midpoint() {
        let c = EaseInOutCurve;
        // At t=0.5: ease-in half gives 2*0.25 = 0.5; ease-out half gives 1 - 1/2 = 0.5
        let v = c.transform(0.5);
        assert!((v - 0.5).abs() < 1e-9, "midpoint should be 0.5, got {}", v);
    }

    #[test]
    fn test_ease_in_out_continuous_at_half() {
        let c = EaseInOutCurve;
        // The two pieces must agree at t=0.5
        let ease_in_value: f64 = 2.0_f64 * 0.5_f64 * 0.5_f64;
        let ease_out_value: f64 = 1.0_f64 - (-2.0_f64 * 0.5_f64 + 2.0_f64).powi(2) / 2.0_f64;
        assert!((ease_in_value - ease_out_value).abs() < 1e-9);
        assert!((c.transform(0.5) - ease_in_value).abs() < 1e-9);
    }

    #[test]
    fn test_ease_in_out_monotonic() {
        let c = EaseInOutCurve;
        let mut prev = 0.0;
        for i in 0..=100 {
            let t = i as f64 / 100.0;
            let v = c.transform(t);
            assert!(v >= prev, "not monotonic at t={}: {} < {}", t, v, prev);
            prev = v;
        }
    }

    #[test]
    fn test_curved_animation_applies_curve() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.forward();
        let start = ctrl.start_time().unwrap();
        // Advance to t=0.5 raw
        ctrl.advance(start + Duration::from_millis(500));
        assert!((ctrl.value() - 0.5).abs() < 1e-3);

        let curved = CurvedAnimation::new(&ctrl, Box::new(EaseInCurve));
        // EaseInCurve.transform(0.5) = 0.25
        assert!((curved.value() - 0.25).abs() < 1e-3);
        // parent_value() returns raw
        assert!((curved.parent_value() - 0.5).abs() < 1e-3);
    }

    #[test]
    fn test_curved_animation_with_linear_is_identity() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.forward();
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start + Duration::from_millis(250));

        let curved = CurvedAnimation::new(&ctrl, Box::new(LinearCurve));
        assert!((curved.value() - ctrl.value()).abs() < 1e-9);
    }

    #[test]
    fn test_curved_animation_at_completion() {
        let mut ctrl = AnimationController::new(Duration::from_secs(1));
        ctrl.forward();
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start + Duration::from_millis(1001));

        let curved = CurvedAnimation::new(&ctrl, Box::new(EaseInOutCurve));
        // At t=1.0 every curve returns 1.0
        assert!((curved.value() - 1.0).abs() < 1e-9);
    }
}
