//! Pure-math physics simulations. Stateless: the same `t` always yields the
//! same `x(t)`. Holds NO framework plumbing (no ticker, no dirty callback) —
//! the caller (AnimationController / ScrollViewElement) owns that.
//!
//! Mirrors Flutter's `package:physics` `Simulation` trait and iOS
//! `UISpringTimingParameters`.

/// Settle thresholds for a `Simulation`. Mirrors Flutter's `Tolerance`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerance {
    /// `|x - target|` below this → considered at rest.
    pub distance: f64,
    /// `|dx|` below this → considered at rest.
    pub velocity: f64,
    /// Hard ceiling: past this elapsed time → done (safety net against
    /// runaway sims). Matches the `MAX_DURATION = 10.0` constant in the
    /// retired `spring.rs`/`momentum.rs`.
    pub time: f64,
}

impl Tolerance {
    /// Unitless-tight default for the controller path (0..1 progress).
    pub const DEFAULT: Tolerance = Tolerance {
        distance: 1e-3,
        velocity: 1e-3,
        time: 10.0,
    };
    /// Px-scale tolerance for scroll-view physics. Matches today's
    /// `X_SETTLE = 1.0` / `V_SETTLE = 13.0` / `MAX_DURATION = 10.0`.
    pub const SCROLL: Tolerance = Tolerance {
        distance: 1.0,
        velocity: 13.0,
        time: 10.0,
    };
}

impl Default for Tolerance {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A pure-math physics simulation. Stateless: the same `t` always yields the
/// same `x(t)`. Holds NO framework plumbing (no ticker, no dirty callback) —
/// the caller (`AnimationController` / `ScrollViewElement`) owns that.
///
/// `Send + Sync` + no generics → object-safe; `Box<dyn Simulation>` works.
pub trait Simulation: Send + Sync {
    /// Value at elapsed seconds `t`.
    fn x(&self, t: f64) -> f64;
    /// Velocity (dx/dt) at elapsed seconds `t`.
    fn dx(&self, t: f64) -> f64;
    /// True once the simulation has settled within `tolerance()`.
    fn is_done(&self, t: f64) -> bool;
    /// Settle thresholds. Default = `Tolerance::DEFAULT`.
    fn tolerance(&self) -> Tolerance {
        Tolerance::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerance_default_is_unitless_tight() {
        let t = Tolerance::default();
        assert_eq!(t.distance, 1e-3);
        assert_eq!(t.velocity, 1e-3);
        assert_eq!(t.time, 10.0);
    }

    #[test]
    fn tolerance_scroll_is_px_scale() {
        let t = Tolerance::SCROLL;
        assert_eq!(t.distance, 1.0);
        assert_eq!(t.velocity, 13.0);
        assert_eq!(t.time, 10.0);
    }

    /// A trivial sim used only to exercise the trait's default `tolerance()`.
    struct ConstSim;
    impl Simulation for ConstSim {
        fn x(&self, _t: f64) -> f64 {
            0.0
        }
        fn dx(&self, _t: f64) -> f64 {
            0.0
        }
        fn is_done(&self, t: f64) -> bool {
            t > 0.0
        }
    }

    #[test]
    fn trait_default_tolerance_is_default() {
        let s = ConstSim;
        assert_eq!(s.tolerance(), Tolerance::DEFAULT);
    }
}
