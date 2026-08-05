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

/// Physical description of a spring. Maps 1:1 to iOS
/// `UISpringTimingParameters` and Flutter `SpringDescription`.
#[derive(Debug, Clone, Copy)]
pub struct SpringDescription {
    pub mass: f64,
    pub stiffness: f64,
    pub damping: f64,
}

impl SpringDescription {
    /// Explicit (mass, stiffness, damping). Panics if `stiffness <= 0`,
    /// `mass <= 0`, or `damping < 0` — these produce non-physical springs
    /// (no restoring force / infinite acceleration).
    pub fn new(mass: f64, stiffness: f64, damping: f64) -> Self {
        assert!(stiffness > 0.0, "stiffness must be > 0, got {}", stiffness);
        assert!(mass > 0.0, "mass must be > 0, got {}", mass);
        assert!(damping >= 0.0, "damping must be >= 0, got {}", damping);
        Self {
            mass,
            stiffness,
            damping,
        }
    }

    /// Damping-ratio form: `damping = 2·ζ·√(m·k)`.
    /// `ratio < 1.0` → under-damped (overshoots); `= 1.0` → critically-damped
    /// (no overshoot); `> 1.0` → over-damped (sluggish, no overshoot).
    /// Matches iOS `usingSpringWithDamping:` and Flutter `withDampingRatio`.
    pub fn with_damping_ratio(mass: f64, stiffness: f64, ratio: f64) -> Self {
        let damping = 2.0 * ratio * (mass * stiffness).sqrt();
        Self::new(mass, stiffness, damping)
    }

    /// iOS-style convenience: `mass = 1.0`. Equivalent to
    /// `with_damping_ratio(1.0, stiffness, damping_ratio)`. The existing
    /// hardcoded `STIFFNESS=340, DAMPING_RATIO=1.0` becomes
    /// `SpringDescription::ios(340.0, 1.0)`.
    pub fn ios(stiffness: f64, damping_ratio: f64) -> Self {
        Self::with_damping_ratio(1.0, stiffness, damping_ratio)
    }
}

/// Damped harmonic oscillator from `from` to `to` with initial velocity `v0`.
/// Stateless: `x(t)` is the closed-form analytic solution of
/// `m·x'' + c·x' + k·x = 0` (three cases: under/critical/over-damped).
///
/// Math source: standard ODE solution, matching Flutter's
/// `spring_simulation.dart`.
pub struct SpringSimulation {
    desc: SpringDescription,
    from: f64,
    to: f64,
    v0: f64,
    tolerance: Tolerance,
}

impl SpringSimulation {
    pub fn new(desc: SpringDescription, from: f64, to: f64, v0: f64) -> Self {
        Self::with_tolerance(desc, from, to, v0, Tolerance::DEFAULT)
    }

    pub fn with_tolerance(
        desc: SpringDescription,
        from: f64,
        to: f64,
        v0: f64,
        tolerance: Tolerance,
    ) -> Self {
        Self {
            desc,
            from,
            to,
            v0,
            tolerance,
        }
    }

    /// Undamped natural frequency `ω₀ = √(k/m)`.
    #[inline]
    fn omega0(&self) -> f64 {
        (self.desc.stiffness / self.desc.mass).sqrt()
    }

    /// Damping ratio `ζ = c / (2·√(m·k))`.
    #[inline]
    fn zeta(&self) -> f64 {
        self.desc.damping / (2.0 * (self.desc.mass * self.desc.stiffness).sqrt())
    }
}

impl Simulation for SpringSimulation {
    fn x(&self, t: f64) -> f64 {
        let omega0 = self.omega0();
        let zeta = self.zeta();
        let a = self.from - self.to; // displacement from equilibrium at t=0

        if zeta < 1.0 {
            // Under-damped.
            let alpha = zeta * omega0;
            let omega_d = omega0 * (1.0 - zeta * zeta).sqrt();
            let b = (self.v0 + alpha * a) / omega_d;
            self.to + (-alpha * t).exp() * (a * (omega_d * t).cos() + b * (omega_d * t).sin())
        } else if zeta > 1.0 {
            // Over-damped.
            let r1 = -zeta * omega0 + omega0 * (zeta * zeta - 1.0).sqrt();
            let r2 = -zeta * omega0 - omega0 * (zeta * zeta - 1.0).sqrt();
            let c1 = (self.v0 - r2 * a) / (r1 - r2);
            let c2 = (r1 * a - self.v0) / (r1 - r2);
            self.to + c1 * (r1 * t).exp() + c2 * (r2 * t).exp()
        } else {
            // Critically-damped (zeta == 1.0).
            let d = self.v0 + omega0 * a;
            self.to + (a + d * t) * (-omega0 * t).exp()
        }
    }

    fn dx(&self, t: f64) -> f64 {
        let omega0 = self.omega0();
        let zeta = self.zeta();
        let a = self.from - self.to;

        if zeta < 1.0 {
            // dx = e^(-αt)·[v0·cos(ωd·t) - (α·v0 + ω₀²·A)/ωd · sin(ωd·t)]
            let alpha = zeta * omega0;
            let omega_d = omega0 * (1.0 - zeta * zeta).sqrt();
            let coeff = (alpha * self.v0 + omega0 * omega0 * a) / omega_d;
            (-alpha * t).exp() * (self.v0 * (omega_d * t).cos() - coeff * (omega_d * t).sin())
        } else if zeta > 1.0 {
            // dx = C1·r1·e^(r1·t) + C2·r2·e^(r2·t)
            let r1 = -zeta * omega0 + omega0 * (zeta * zeta - 1.0).sqrt();
            let r2 = -zeta * omega0 - omega0 * (zeta * zeta - 1.0).sqrt();
            let c1 = (self.v0 - r2 * a) / (r1 - r2);
            let c2 = (r1 * a - self.v0) / (r1 - r2);
            c1 * r1 * (r1 * t).exp() + c2 * r2 * (r2 * t).exp()
        } else {
            // Critical: dx = e^(-ω₀t)·(v0 - ω₀·D·t), D = v0 + ω₀·A
            let d = self.v0 + omega0 * a;
            (-omega0 * t).exp() * (self.v0 - omega0 * d * t)
        }
    }

    fn is_done(&self, t: f64) -> bool {
        if t >= self.tolerance.time {
            return true;
        }
        // from==to && v0==0 ⇒ no motion ⇒ done immediately.
        if (self.from - self.to).abs() < self.tolerance.distance
            && self.v0.abs() < self.tolerance.velocity
        {
            return true;
        }
        (self.x(t) - self.to).abs() < self.tolerance.distance
            && self.dx(t).abs() < self.tolerance.velocity
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

    // ---- SpringDescription ----

    #[test]
    fn spring_description_new_stores_params() {
        let d = SpringDescription::new(1.0, 340.0, 20.0);
        assert_eq!(d.mass, 1.0);
        assert_eq!(d.stiffness, 340.0);
        assert_eq!(d.damping, 20.0);
    }

    #[test]
    fn spring_description_with_damping_ratio_critical() {
        // ζ=1 ⇒ damping = 2·√(m·k) = 2·√340 ≈ 36.878
        let d = SpringDescription::with_damping_ratio(1.0, 340.0, 1.0);
        let expected: f64 = 2.0 * (1.0_f64 * 340.0).sqrt();
        assert!(
            (d.damping - expected).abs() < 1e-9,
            "got {} expected {}",
            d.damping,
            expected
        );
    }

    #[test]
    fn spring_description_with_damping_ratio_under_damped() {
        // ζ=0.5 ⇒ damping = 2·0.5·√340 = √340 ≈ 18.439
        let d = SpringDescription::with_damping_ratio(1.0, 340.0, 0.5);
        let expected: f64 = 2.0 * 0.5 * (1.0_f64 * 340.0).sqrt();
        assert!((d.damping - expected).abs() < 1e-9);
    }

    #[test]
    fn spring_description_ios_sets_mass_one() {
        let d = SpringDescription::ios(340.0, 1.0);
        assert_eq!(d.mass, 1.0);
        assert_eq!(d.stiffness, 340.0);
        // damping = 2·1·√340
        assert!((d.damping - 2.0 * 340.0_f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    #[should_panic(expected = "stiffness must be > 0")]
    fn spring_description_new_rejects_zero_stiffness() {
        let _ = SpringDescription::new(1.0, 0.0, 1.0);
    }

    #[test]
    #[should_panic(expected = "mass must be > 0")]
    fn spring_description_new_rejects_zero_mass() {
        let _ = SpringDescription::new(0.0, 340.0, 1.0);
    }

    #[test]
    #[should_panic(expected = "damping must be >= 0")]
    fn spring_description_new_rejects_negative_damping() {
        let _ = SpringDescription::new(1.0, 340.0, -1.0);
    }

    // ---- SpringSimulation: endpoints & basic shape ----

    fn critical_spring(from: f64, to: f64, v0: f64) -> SpringSimulation {
        // mass=1, stiffness=340, ζ=1 (critically-damped) — matches the
        // retired SpringSimulation's hardcoded STIFFNESS/DAMPING_RATIO.
        SpringSimulation::new(SpringDescription::ios(340.0, 1.0), from, to, v0)
    }

    #[test]
    fn spring_x_at_t_zero_is_from() {
        let sim = critical_spring(-100.0, 0.0, 0.0);
        assert!((sim.x(0.0) - (-100.0)).abs() < 1e-9);
    }

    #[test]
    fn spring_x_settles_to_to() {
        let sim = critical_spring(-100.0, 0.0, 0.0);
        // After 1s of a k=340 critical spring, well past settled.
        assert!((sim.x(1.0) - 0.0).abs() < 1e-3, "got {}", sim.x(1.0));
    }

    #[test]
    fn spring_dx_at_t_zero_is_v0() {
        let sim = critical_spring(-100.0, 0.0, 50.0);
        assert!((sim.dx(0.0) - 50.0).abs() < 1e-6, "got {}", sim.dx(0.0));
    }

    // ---- Closed-form spot-checks (self-verifying: expected computed from
    // the analytic formula inline, so the test doesn't depend on hand-math) ----

    #[test]
    fn spring_critical_matches_closed_form() {
        // Critical: x(t) = to + (A + D·t)·e^(-ω₀·t), A=from-to, D=v0+ω₀·A
        let m = 1.0_f64;
        let k = 340.0_f64;
        let from = -100.0_f64;
        let to = 0.0_f64;
        let v0 = 0.0_f64;
        let sim = SpringSimulation::new(
            SpringDescription::new(m, k, 2.0 * (m * k).sqrt()),
            from,
            to,
            v0,
        );
        let omega0 = (k / m).sqrt();
        let a = from - to;
        let d = v0 + omega0 * a;
        for t in [0.0_f64, 0.01, 0.05, 0.1, 0.2, 0.5] {
            let expected = to + (a + d * t) * (-omega0 * t).exp();
            assert!(
                (sim.x(t) - expected).abs() < 1e-9,
                "t={}: got {} expected {}",
                t,
                sim.x(t),
                expected
            );
        }
    }

    #[test]
    fn spring_under_damped_matches_closed_form() {
        // Under: x(t) = to + e^(-αt)·[A·cos(ωd·t) + B·sin(ωd·t)]
        let m = 1.0_f64;
        let k = 340.0_f64;
        let zeta = 0.5;
        let from = -100.0_f64;
        let to = 0.0_f64;
        let v0 = 0.0_f64;
        let sim = SpringSimulation::new(
            SpringDescription::with_damping_ratio(m, k, zeta),
            from,
            to,
            v0,
        );
        let omega0 = (k / m).sqrt();
        let alpha = zeta * omega0;
        let omega_d = omega0 * (1.0 - zeta * zeta).sqrt();
        let a = from - to;
        let b = (v0 + alpha * a) / omega_d;
        for t in [0.0_f64, 0.01, 0.05, 0.1, 0.2, 0.5] {
            let expected =
                to + (-alpha * t).exp() * (a * (omega_d * t).cos() + b * (omega_d * t).sin());
            assert!(
                (sim.x(t) - expected).abs() < 1e-9,
                "t={}: got {} expected {}",
                t,
                sim.x(t),
                expected
            );
        }
    }

    #[test]
    fn spring_over_damped_matches_closed_form() {
        // Over: x(t) = to + C1·e^(r1·t) + C2·e^(r2·t)
        let m = 1.0_f64;
        let k = 340.0_f64;
        let zeta = 2.0; // over-damped
        let from = -100.0_f64;
        let to = 0.0_f64;
        let v0 = 0.0_f64;
        let sim = SpringSimulation::new(
            SpringDescription::with_damping_ratio(m, k, zeta),
            from,
            to,
            v0,
        );
        let omega0 = (k / m).sqrt();
        let r1 = -zeta * omega0 + omega0 * (zeta * zeta - 1.0).sqrt();
        let r2 = -zeta * omega0 - omega0 * (zeta * zeta - 1.0).sqrt();
        let a = from - to;
        let c1 = (v0 - r2 * a) / (r1 - r2);
        let c2 = (r1 * a - v0) / (r1 - r2);
        for t in [0.0_f64, 0.01, 0.05, 0.1, 0.2, 0.5] {
            let expected = to + c1 * (r1 * t).exp() + c2 * (r2 * t).exp();
            assert!(
                (sim.x(t) - expected).abs() < 1e-9,
                "t={}: got {} expected {}",
                t,
                sim.x(t),
                expected
            );
        }
    }

    // ---- Behavioral properties (the regression gate) ----

    #[test]
    fn spring_critical_does_not_overshoot_past_to() {
        // ζ=1 ⇒ no overshoot. Release from -100 toward 0; should not cross
        // past 0 into positive territory. (Ported from the retired
        // spring.rs `spring_does_not_overshoot_when_released_from_overscroll`.)
        let sim = critical_spring(-100.0, 0.0, 0.0);
        let mut max_x = -100.0_f64;
        for i in 0..=500 {
            let t = i as f64 / 120.0;
            max_x = max_x.max(sim.x(t));
        }
        assert!(
            max_x <= 0.0 + 1e-3,
            "critical spring should not overshoot past to; max was {}",
            max_x
        );
    }

    #[test]
    fn spring_under_damped_overshoots_past_to() {
        // ζ=0.5 ⇒ overshoots. From -100 toward 0, should cross past 0
        // into positive territory at least once.
        let sim = SpringSimulation::new(
            SpringDescription::with_damping_ratio(1.0, 340.0, 0.5),
            -100.0,
            0.0,
            0.0,
        );
        let mut max_x = -100.0_f64;
        for i in 0..=500 {
            let t = i as f64 / 120.0;
            max_x = max_x.max(sim.x(t));
        }
        assert!(
            max_x > 0.0,
            "under-damped spring should overshoot past to; max was {}",
            max_x
        );
    }

    #[test]
    fn spring_settles_within_one_second() {
        // Ported from retired spring.rs `spring_settle_time_under_one_second`.
        let sim = critical_spring(-100.0, 0.0, 0.0);
        let mut settle_time = f64::MAX;
        for i in 0..=1200 {
            let t = i as f64 / 120.0;
            if sim.is_done(t) {
                settle_time = t;
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
    fn spring_is_done_past_time_ceiling() {
        let sim = critical_spring(-100.0, 0.0, 0.0);
        assert!(sim.is_done(11.0));
    }

    #[test]
    fn spring_with_explicit_tolerance_uses_it() {
        let tol = Tolerance::SCROLL;
        let sim = SpringSimulation::with_tolerance(
            SpringDescription::ios(340.0, 1.0),
            -100.0,
            0.0,
            0.0,
            tol,
        );
        assert_eq!(sim.tolerance(), tol);
    }

    #[test]
    fn spring_zero_motion_is_done_at_t_zero() {
        // from == to, v0 == 0 ⇒ no motion ⇒ is_done immediately.
        let sim = critical_spring(50.0, 50.0, 0.0);
        assert!(sim.is_done(0.0));
    }
}
