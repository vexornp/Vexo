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

/// A cubic Bézier easing curve defined by two control points
/// `(p1x, p1y)` and `(p2x, p2y)` between the endpoints `(0, 0)` and `(1, 1)`.
///
/// This is the same model used by CSS `cubic-bezier()` and Flutter's `Cubic`
/// class. The input `t` (linear time, `0..=1`) is interpreted as the *x*
/// coordinate of the Bézier; the returned value is the corresponding *y*
/// coordinate. Because the Bézier's x(t) is not generally linear in `t`, we
/// solve for the parametric `t` that yields the requested x via
/// Newton-Raphson (with a bisection fallback), then evaluate the y at that
/// parametric `t`.
///
/// # Common control points
///
/// | Points | Name | Notes |
/// |--------|------|-------|
/// | `(0.33, 1.0, 0.68, 1.0)` | ease-out-cubic | iOS `UINavigationController` push/pop feel; strong end-deceleration |
/// | `(0.25, 0.1, 0.25, 1.0)` | Apple `default` | `kCAMediaTimingFunctionDefault`, used across iOS |
/// | `(0.4, 0.0, 0.2, 1.0)` | Material standard | Material Design "standard" easing |
/// | `(0.16, 1.0, 0.3, 1.0)` | ease-out-expo-ish | Very aggressive end-deceleration |
///
/// # Constraints
///
/// `p1x` and `p2x` must be in `[0, 1]` to keep the curve a monotonic function
/// of x (so every input x has a unique y). `p1y` and `p2y` may exceed `[0, 1]`
/// (producing overshoot/undershoot), though navigation transitions typically
/// keep them in range.
#[derive(Debug, Clone, Copy)]
pub struct CubicBezierCurve {
    pub p1x: f64,
    pub p1y: f64,
    pub p2x: f64,
    pub p2y: f64,
}

impl CubicBezierCurve {
    /// Create a new cubic Bézier curve with the given control points.
    pub fn new(p1x: f64, p1y: f64, p2x: f64, p2y: f64) -> Self {
        Self { p1x, p1y, p2x, p2y }
    }

    /// Evaluate the Bézier at parametric `t` (0..=1) along the x axis.
    #[inline]
    fn bezier_x(&self, t: f64) -> f64 {
        // Cubic Bézier: (1-t)³·P0 + 3(1-t)²t·P1 + 3(1-t)t²·P2 + t³·P3
        // with P0 = (0,0) and P3 = (1,1).
        let one_minus_t = 1.0 - t;
        3.0 * one_minus_t * one_minus_t * t * self.p1x
            + 3.0 * one_minus_t * t * t * self.p2x
            + t * t * t
    }

    /// Evaluate the Bézier at parametric `t` (0..=1) along the y axis.
    #[inline]
    fn bezier_y(&self, t: f64) -> f64 {
        let one_minus_t = 1.0 - t;
        3.0 * one_minus_t * one_minus_t * t * self.p1y
            + 3.0 * one_minus_t * t * t * self.p2y
            + t * t * t
    }

    /// Derivative of the x component of the Bézier w.r.t. parametric `t`.
    #[inline]
    fn bezier_x_prime(&self, t: f64) -> f64 {
        let one_minus_t = 1.0 - t;
        3.0 * one_minus_t * one_minus_t * (self.p1x)
            + 6.0 * one_minus_t * t * (self.p2x - self.p1x)
            + 3.0 * t * t * (1.0 - self.p2x)
    }

    /// Solve for the parametric `t` that yields `bezier_x(t) == x`.
    ///
    /// Uses Newton-Raphson (fast convergence for typical curves) with a
    /// bisection fallback when Newton fails or the derivative is too small.
    /// Returns a parametric `t` in `[0, 1]`.
    fn solve_parametric_for_x(&self, x: f64) -> f64 {
        // Clamp input to valid range.
        let x = x.clamp(0.0, 1.0);

        // Newton-Raphson first: typically converges in <8 iterations.
        let mut t = x; // Good initial guess for monotonic-in-x curves.
        for _ in 0..8 {
            let x_at_t = self.bezier_x(t);
            let dx = x_at_t - x;
            if dx.abs() < 1e-7 {
                return t;
            }
            let d = self.bezier_x_prime(t);
            if d.abs() < 1e-7 {
                break; // Derivative too small; fall back to bisection.
            }
            t -= dx / d;
        }

        // Bisection fallback: guaranteed to converge for monotonic-in-x curves.
        let mut lo = 0.0_f64;
        let mut hi = 1.0_f64;
        let mut t = x;
        for _ in 0..60 {
            let x_at_t = self.bezier_x(t);
            if x_at_t < x {
                lo = t;
            } else {
                hi = t;
            }
            let next = (lo + hi) * 0.5;
            if (next - t).abs() < 1e-9 {
                return next;
            }
            t = next;
        }
        t
    }
}

impl Curve for CubicBezierCurve {
    fn transform(&self, t: f64) -> f64 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        let parametric = self.solve_parametric_for_x(t);
        self.bezier_y(parametric)
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

    // ---- CubicBezierCurve ----

    #[test]
    fn test_cubic_bezier_endpoints() {
        // ease-out-cubic
        let c = CubicBezierCurve::new(0.33, 1.0, 0.68, 1.0);
        assert!((c.transform(0.0) - 0.0).abs() < 1e-9);
        assert!((c.transform(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_cubic_bezier_linear_identity() {
        // Control points on the diagonal produce identity (linear) curve.
        let c = CubicBezierCurve::new(1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0);
        for i in 0..=20 {
            let t = i as f64 / 20.0;
            assert!(
                (c.transform(t) - t).abs() < 1e-6,
                "linear identity failed at t={}: got {}",
                t,
                c.transform(t)
            );
        }
    }

    #[test]
    fn test_cubic_bezier_ease_out_cubic_midpoint() {
        // ease-out-cubic (0.33, 1, 0.68, 1): strong end-deceleration.
        // transform(0.5) should be well above 0.5 (around 0.87).
        let c = CubicBezierCurve::new(0.33, 1.0, 0.68, 1.0);
        let v = c.transform(0.5);
        assert!(
            v > 0.8,
            "ease-out-cubic at t=0.5 should be > 0.8, got {}",
            v
        );
        assert!(
            v < 0.95,
            "ease-out-cubic at t=0.5 should be < 0.95, got {}",
            v
        );
    }

    #[test]
    fn test_cubic_bezier_monotonic() {
        // A valid easing curve (with p1x, p2x in [0, 1]) must be monotonic in t.
        let c = CubicBezierCurve::new(0.33, 1.0, 0.68, 1.0);
        let mut prev = 0.0;
        for i in 0..=200 {
            let t = i as f64 / 200.0;
            let v = c.transform(t);
            assert!(
                v >= prev - 1e-9,
                "not monotonic at t={}: {} < {}",
                t,
                v,
                prev
            );
            prev = v;
        }
    }

    #[test]
    fn test_cubic_bezier_matches_known_ease_out_cubic_polynomial() {
        // The polynomial form of "ease-out-cubic" is 1 - (1 - t)³.
        // The cubic-bezier (0.33, 1, 0.68, 1) is a *close approximation* of this
        // (not exact — control points differ from the polynomial's exact
        // Bézier representation). Verify they're within ~0.05 of each other
        // across the domain, which confirms our solver is producing sensible
        // values for this canonical control point set.
        let c = CubicBezierCurve::new(0.33, 1.0, 0.68, 1.0);
        for i in 0..=20 {
            let t = i as f64 / 20.0;
            let poly = 1.0 - (1.0 - t).powi(3);
            let bezier = c.transform(t);
            assert!(
                (bezier - poly).abs() < 0.05,
                "bezier {} diverges from polynomial {} at t={}",
                bezier,
                poly,
                t
            );
        }
    }

    #[test]
    fn test_cubic_bezier_clamps_out_of_range_input() {
        let c = CubicBezierCurve::new(0.33, 1.0, 0.68, 1.0);
        assert_eq!(c.transform(-0.5), 0.0);
        assert_eq!(c.transform(1.5), 1.0);
    }
}
