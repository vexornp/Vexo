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

/// Exponential-decay fling. `x(t) = x0 + v0·τ·(1 - e^(-t/τ))`,
/// `dx(t) = v0·e^(-t/τ)`. Mirrors the retired `MomentumSimulation` math
/// (which was already analytic), with configurable drag `τ` instead of the
/// hardcoded `TAU = 0.325`.
pub struct FrictionSimulation {
    x0: f64,
    v0: f64,
    drag: f64,
    tolerance: Tolerance,
}

impl FrictionSimulation {
    /// New fling from `x0` with initial velocity `v0` and drag time-constant
    /// `drag` (τ). Default tolerance (`Tolerance::DEFAULT`).
    pub fn new(x0: f64, v0: f64, drag: f64) -> Self {
        Self::with_tolerance(x0, v0, drag, Tolerance::DEFAULT)
    }

    /// New fling with an explicit tolerance (e.g. `Tolerance::SCROLL` for
    /// scroll-view physics).
    pub fn with_tolerance(x0: f64, v0: f64, drag: f64, tolerance: Tolerance) -> Self {
        Self {
            x0,
            v0,
            drag,
            tolerance,
        }
    }
}

impl Simulation for FrictionSimulation {
    fn x(&self, t: f64) -> f64 {
        // x0 + v0·τ·(1 - e^(-t/τ))
        self.x0 + self.v0 * self.drag * (1.0 - (-t / self.drag).exp())
    }

    fn dx(&self, t: f64) -> f64 {
        // v0·e^(-t/τ)
        self.v0 * (-t / self.drag).exp()
    }

    fn is_done(&self, t: f64) -> bool {
        if t >= self.tolerance.time {
            return true;
        }
        self.dx(t).abs() < self.tolerance.velocity
    }

    fn tolerance(&self) -> Tolerance {
        self.tolerance
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

    #[test]
    fn friction_new_is_not_done_at_t_zero() {
        let sim = FrictionSimulation::new(0.0, 1000.0, 0.325);
        assert!(!sim.is_done(0.0));
    }

    #[test]
    fn friction_x_at_t_zero_returns_x0() {
        let sim = FrictionSimulation::new(50.0, 1000.0, 0.325);
        assert!((sim.x(0.0) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn friction_x_at_t_tau_matches_closed_form() {
        // x(τ) = x0 + v0·τ·(1 - 1/e)
        let x0 = 0.0;
        let v0 = 1000.0;
        let tau = 0.325;
        let sim = FrictionSimulation::new(x0, v0, tau);
        let expected = x0 + v0 * tau * (1.0 - 1.0 / std::f64::consts::E);
        assert!(
            (sim.x(tau) - expected).abs() < 1e-6,
            "got {} expected {}",
            sim.x(tau),
            expected
        );
    }

    #[test]
    fn friction_positive_v0_increases_x() {
        let sim = FrictionSimulation::new(0.0, 1000.0, 0.325);
        assert!(sim.x(0.1) > 0.0);
    }

    #[test]
    fn friction_negative_v0_decreases_x() {
        let sim = FrictionSimulation::new(0.0, -1000.0, 0.325);
        assert!(sim.x(0.1) < 0.0);
    }

    #[test]
    fn friction_dx_decays_exponentially() {
        let sim = FrictionSimulation::new(0.0, 1000.0, 0.325);
        let v0 = sim.dx(0.0);
        let v_later = sim.dx(0.1);
        assert!(v_later < v0);
        // dx(t) = v0·e^(-t/τ); at t=0 dx=v0.
        assert!((v0 - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn friction_is_done_when_velocity_decays_below_tolerance() {
        // v(t) = v0·e^(-t/τ) < tolerance.velocity ⇒ done.
        let v0 = 1000.0;
        let tau = 0.325;
        let tol = Tolerance {
            distance: 1.0,
            velocity: 13.0,
            time: 10.0,
        };
        let sim = FrictionSimulation::with_tolerance(0.0, v0, tau, tol);
        // t where v drops below 13: t = τ·ln(v0/13)
        let t_stop = tau * (v0 / 13.0).ln();
        assert!(!sim.is_done(t_stop * 0.9));
        assert!(sim.is_done(t_stop * 1.2));
    }

    #[test]
    fn friction_is_done_past_time_ceiling() {
        let sim = FrictionSimulation::new(0.0, 1000.0, 0.325);
        assert!(sim.is_done(11.0)); // > tolerance.time (10.0)
    }

    #[test]
    fn friction_tolerance_returns_configured() {
        let tol = Tolerance::SCROLL;
        let sim = FrictionSimulation::with_tolerance(0.0, 1000.0, 0.325, tol);
        assert_eq!(sim.tolerance(), tol);
    }

    #[test]
    fn friction_default_tolerance_is_default() {
        let sim = FrictionSimulation::new(0.0, 1000.0, 0.325);
        assert_eq!(sim.tolerance(), Tolerance::DEFAULT);
    }
}
