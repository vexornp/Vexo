# Physics-Driven Animation System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a configurable, user-facing physics animation system (Flutter-style `Simulation` trait + `Spring`/`Friction` + `AnimationController::animate_with`), and refactor `ScrollViewElement` to consume the new types — generalizing the existing hardcoded internal sims.

**Architecture:** A new pure-math `Simulation` trait (`x(t)`/`dx(t)`/`is_done(t)`) decouples physics math from framework plumbing. `SpringSimulation` switches from semi-implicit Euler to the analytic closed-form damped-harmonic solution (required by the stateless trait). `AnimationController` gains a `Drive` enum (folding existing time-based state) plus a third driving mode `animate_with(sim)`. `ScrollViewElement` keeps its own ticker/advance loop (px-scale + mid-flight velocity handoff) but sources math from the new sims via a new `ScrollPhysics` config struct.

**Tech Stack:** Rust, no new dependencies. `vexo` crate (`vexo/src/animation/`, `vexo/src/widgets/scroll_view.rs`, `vexo/src/elements/scroll_view.rs`).

## Global Constraints

- **No new crate dependencies.** All physics is pure Rust math in `vexo/src/animation/simulation.rs`.
- **Use `Instant::saturating_duration_since`** (NOT `Instant::since` — does not exist; NOT `duration_since` — panics on time-travel). Matches existing code in `spring.rs:109`/`momentum.rs:82`.
- **`f64` for all simulation math** (matches `AnimationController::value: f64` and `Curve::transform(f64)`). ScrollView casts to `f32` at the render-object write boundary, as it does today.
- **Param validation panics, never silent clamps.** `SpringDescription::new` panics on `stiffness <= 0`, `mass <= 0`, `damping < 0` with a clear message.
- **No `Drop` on new sims.** Pure-math sims hold no ticker handle; the controller/element owns cleanup.
- **Test gate:** `cargo test -p vexo` must pass after every task. The existing 37 ScrollViewElement tests + 12 momentum + 11 spring + controller tests are the regression gate; expected-value adjustments only with cited analytic justification.
- **Build gate:** `cargo build -p vexo` clean after every task.
- **Commit after every task** (or more often within a task).

**Spec:** `docs/superpowers/specs/2026-08-05-physics-animation-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `vexo/src/animation/simulation.rs` | Create | `Simulation` trait, `Tolerance`, `SpringDescription`, `SpringSimulation` (analytic), `FrictionSimulation` — pure math, no framework plumbing |
| `vexo/src/animation/mod.rs` | Modify | Re-exports: add new types, eventually drop old `SpringSimulation`/`MomentumSimulation` |
| `vexo/src/animation/controller.rs` | Modify | `Drive` enum (internal refactor), `animate_with()`, `is_animating()` |
| `vexo/src/animation/spring.rs` | Delete (Task 5) | Old stateful Euler sim — superseded by `simulation.rs` |
| `vexo/src/animation/momentum.rs` | Delete (Task 5) | Old stateful sim — superseded by `FrictionSimulation` |
| `vexo/src/widgets/scroll_view.rs` | Modify | Add `ScrollPhysics` struct + optional `physics` field on `ScrollView` |
| `vexo/src/elements/scroll_view.rs` | Modify | `ScrollDrive` enum, source math from new sims, read `physics` config |
| `vexo/src/lib.rs` | Modify | Update top-level re-exports |

**Name-collision handling during the transition (Tasks 2–4):** the old `SpringSimulation` (in `spring.rs`) and new `SpringSimulation` (in `simulation.rs`) coexist. The old one stays re-exported as `animation::SpringSimulation`; the new one is accessed internally as `crate::animation::simulation::SpringSimulation` until Task 5 flips the re-export atomically with the deletion. `FrictionSimulation` and `SpringDescription` have no collision and are re-exported immediately.

---

## Task 1: `Simulation` trait + `Tolerance`

**Files:**
- Create: `vexo/src/animation/simulation.rs`
- Modify: `vexo/src/animation/mod.rs`

**Interfaces:**
- Produces: `Simulation` trait (`fn x(&self, t: f64) -> f64; fn dx(&self, t: f64) -> f64; fn is_done(&self, t: f64) -> bool; fn tolerance(&self) -> Tolerance`), `Tolerance` struct (`distance: f64, velocity: f64, time: f64` + `DEFAULT`/`SCROLL` consts + `Default` impl).

- [ ] **Step 1: Create `simulation.rs` with the trait + `Tolerance` + a trivial passing test**

Create `vexo/src/animation/simulation.rs`:

```rust
//! Pure-math physics simulations. Stateless: the same `t` always yields the
//! same `x(t)`. Holds NO framework plumbing (no ticker, no dirty callback) —
//! the caller (AnimationController / ScrollViewElement) owns that.
//!
//! Mirrors Flutter's `package:physics` `Simulation` trait and iOS
//! `UISpringTimingParameters`.

/// Settle thresholds for a `Simulation`. Mirrors Flutter's `Tolerance`.
#[derive(Debug, Clone, Copy)]
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
        fn x(&self, _t: f64) -> f64 { 0.0 }
        fn dx(&self, _t: f64) -> f64 { 0.0 }
        fn is_done(&self, t: f64) -> bool { t > 0.0 }
    }

    #[test]
    fn trait_default_tolerance_is_default() {
        let s = ConstSim;
        assert_eq!(s.tolerance(), Tolerance::DEFAULT);
    }
}
```

- [ ] **Step 2: Register the module in `mod.rs`**

Modify `vexo/src/animation/mod.rs` — add `pub mod simulation;` and re-export `Simulation` + `Tolerance`. The file currently reads (lines 1-16):

```rust
pub mod controller;
pub mod curve;
pub mod momentum;
pub mod spring;
pub mod ticker;
pub mod tween;

pub use controller::{AnimationController, AnimationDirection};
pub use curve::{
    CubicBezierCurve, Curve, CurvedAnimation, EaseInCurve, EaseInOutCurve, EaseOutCurve,
    LinearCurve,
};
pub use momentum::MomentumSimulation;
pub use spring::SpringSimulation;
pub use ticker::{AnimationTicker, TickHandle};
pub use tween::{ColorTween, FloatTween, Tween};
```

Replace with:

```rust
pub mod controller;
pub mod curve;
pub mod momentum;
pub mod simulation;
pub mod spring;
pub mod ticker;
pub mod tween;

pub use controller::{AnimationController, AnimationDirection};
pub use curve::{
    CubicBezierCurve, Curve, CurvedAnimation, EaseInCurve, EaseInOutCurve, EaseOutCurve,
    LinearCurve,
};
pub use momentum::MomentumSimulation;
pub use simulation::{Simulation, Tolerance};
pub use spring::SpringSimulation;
pub use ticker::{AnimationTicker, TickHandle};
pub use tween::{ColorTween, FloatTween, Tween};
```

(Old `SpringSimulation`/`MomentumSimulation` re-exports stay for now — deleted in Task 5.)

- [ ] **Step 3: Update top-level re-exports in `lib.rs`**

Modify `vexo/src/lib.rs:103-107`. Current:

```rust
pub use animation::{
    AnimationController, AnimationDirection, AnimationTicker, ColorTween, CubicBezierCurve,
    CurvedAnimation, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, FloatTween, LinearCurve,
    TickHandle, Tween,
};
```

Replace with:

```rust
pub use animation::{
    AnimationController, AnimationDirection, AnimationTicker, ColorTween, CubicBezierCurve,
    CurvedAnimation, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, FloatTween, LinearCurve,
    Simulation, TickHandle, Tolerance, Tween,
};
```

- [ ] **Step 4: Run tests + build**

Run: `cargo test -p vexo --lib simulation::tests`
Expected: 3 tests pass (`tolerance_default_is_unitless_tight`, `tolerance_scroll_is_px_scale`, `trait_default_tolerance_is_default`).

Run: `cargo build -p vexo`
Expected: clean build, no warnings about unused code.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/animation/simulation.rs vexo/src/animation/mod.rs vexo/src/lib.rs
git commit -m "feat(vexo): add Simulation trait + Tolerance for physics animations"
```

---

## Task 2: `FrictionSimulation`

The easier of the two sims — the existing `MomentumSimulation` is *already* analytic (`x(t) = x0 + v0·τ·(1 - e^(-t/τ))`), so this is a direct port with configurable drag.

**Files:**
- Modify: `vexo/src/animation/simulation.rs`
- Modify: `vexo/src/animation/mod.rs`
- Modify: `vexo/src/lib.rs`

**Interfaces:**
- Produces: `FrictionSimulation { x0, v0, drag, tolerance }` with `::new(x0, v0, drag)`, `::with_tolerance(x0, v0, drag, Tolerance)`, impl `Simulation`.

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `vexo/src/animation/simulation.rs`:

```rust
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
        assert!((sim.x(tau) - expected).abs() < 1e-6, "got {} expected {}", sim.x(tau), expected);
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
        let tol = Tolerance { distance: 1.0, velocity: 13.0, time: 10.0 };
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib simulation::tests::friction`
Expected: compile error — `FrictionSimulation` not found.

- [ ] **Step 3: Implement `FrictionSimulation`**

Add above the `#[cfg(test)]` block in `vexo/src/animation/simulation.rs`:

```rust
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
        Self { x0, v0, drag, tolerance }
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib simulation::tests::friction`
Expected: all 10 friction tests pass.

Run: `cargo build -p vexo`
Expected: clean.

- [ ] **Step 5: Re-export `FrictionSimulation`**

Modify `vexo/src/animation/mod.rs` — change the simulation re-export line:

```rust
pub use simulation::{FrictionSimulation, Simulation, Tolerance};
```

Modify `vexo/src/lib.rs:103-107` — add `FrictionSimulation`:

```rust
pub use animation::{
    AnimationController, AnimationDirection, AnimationTicker, ColorTween, CubicBezierCurve,
    CurvedAnimation, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, FloatTween,
    FrictionSimulation, LinearCurve, Simulation, TickHandle, Tolerance, Tween,
};
```

- [ ] **Step 6: Run full test suite + build**

Run: `cargo test -p vexo`
Expected: all pass (existing tests + new friction tests).

Run: `cargo build -p vexo`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/animation/simulation.rs vexo/src/animation/mod.rs vexo/src/lib.rs
git commit -m "feat(vexo): add FrictionSimulation (configurable exponential-decay fling)"
```

---

## Task 3: `SpringDescription` + `SpringSimulation` (analytic)

The main risk task. Switches the spring integrator from semi-implicit Euler to the analytic closed-form damped-harmonic solution (required by the stateless `Simulation::x(t)` trait). Three cases: under-damped (ζ<1, overshoots), critically-damped (ζ=1, no overshoot), over-damped (ζ>1).

**Files:**
- Modify: `vexo/src/animation/simulation.rs`
- Modify: `vexo/src/animation/mod.rs`
- Modify: `vexo/src/lib.rs`

**Interfaces:**
- Produces: `SpringDescription { mass, stiffness, damping }` with `::new(m, k, c)`, `::with_damping_ratio(m, k, ζ)`, `::ios(stiffness, damping_ratio)`. `SpringSimulation { desc, from, to, v0, tolerance }` with `::new(desc, from, to, v0)`, `::with_tolerance(desc, from, to, v0, Tolerance)`, impl `Simulation`.
- Note: `SpringSimulation` is NOT re-exported from `animation::` in this task (name collision with the old `spring.rs::SpringSimulation`, still used by ScrollView until Task 5). Internal/test code uses `crate::animation::simulation::SpringSimulation`. Task 5 flips the re-export.

**Math reference** (standard ODE solution for `m·x'' + c·x' + k·x = 0`, equilibrium at `to`, initial displacement `A = from - to`, initial velocity `v0`):
- `ω₀ = √(k/m)`, `ζ = c / (2√(m·k))`
- Under-damped (ζ < 1): `ωd = ω₀√(1-ζ²)`, `α = ζω₀`, `B = (v0 + α·A)/ωd`; `x(t) = to + e^(-αt)·[A·cos(ωd·t) + B·sin(ωd·t)]`
- Critical (ζ ≈ 1): `D = v0 + ω₀·A`; `x(t) = to + (A + D·t)·e^(-ω₀·t)`
- Over-damped (ζ > 1): `r₁ = -ζω₀ + ω₀√(ζ²-1)`, `r₂ = -ζω₀ - ω₀√(ζ²-1)`, `C₁ = (v0 - r₂·A)/(r₁-r₂)`, `C₂ = (r₁·A - v0)/(r₁-r₂)`; `x(t) = to + C₁·e^(r₁t) + C₂·e^(r₂t)`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `vexo/src/animation/simulation.rs`:

```rust
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
        let expected = 2.0 * (1.0 * 340.0).sqrt();
        assert!((d.damping - expected).abs() < 1e-9, "got {} expected {}", d.damping, expected);
    }

    #[test]
    fn spring_description_with_damping_ratio_under_damped() {
        // ζ=0.5 ⇒ damping = 2·0.5·√340 = √340 ≈ 18.439
        let d = SpringDescription::with_damping_ratio(1.0, 340.0, 0.5);
        let expected = 2.0 * 0.5 * (1.0 * 340.0).sqrt();
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
        let sim = SpringSimulation::new(SpringDescription::new(m, k, 2.0 * (m * k).sqrt()), from, to, v0);
        let omega0 = (k / m).sqrt();
        let a = from - to;
        let d = v0 + omega0 * a;
        for t in [0.0_f64, 0.01, 0.05, 0.1, 0.2, 0.5] {
            let expected = to + (a + d * t) * (-omega0 * t).exp();
            assert!((sim.x(t) - expected).abs() < 1e-9, "t={}: got {} expected {}", t, sim.x(t), expected);
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
            from, to, v0,
        );
        let omega0 = (k / m).sqrt();
        let alpha = zeta * omega0;
        let omega_d = omega0 * (1.0 - zeta * zeta).sqrt();
        let a = from - to;
        let b = (v0 + alpha * a) / omega_d;
        for t in [0.0_f64, 0.01, 0.05, 0.1, 0.2, 0.5] {
            let expected = to + (-alpha * t).exp() * (a * (omega_d * t).cos() + b * (omega_d * t).sin());
            assert!((sim.x(t) - expected).abs() < 1e-9, "t={}: got {} expected {}", t, sim.x(t), expected);
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
            from, to, v0,
        );
        let omega0 = (k / m).sqrt();
        let r1 = -zeta * omega0 + omega0 * (zeta * zeta - 1.0).sqrt();
        let r2 = -zeta * omega0 - omega0 * (zeta * zeta - 1.0).sqrt();
        let a = from - to;
        let c1 = (v0 - r2 * a) / (r1 - r2);
        let c2 = (r1 * a - v0) / (r1 - r2);
        for t in [0.0_f64, 0.01, 0.05, 0.1, 0.2, 0.5] {
            let expected = to + c1 * (r1 * t).exp() + c2 * (r2 * t).exp();
            assert!((sim.x(t) - expected).abs() < 1e-9, "t={}: got {} expected {}", t, sim.x(t), expected);
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
        assert!(max_x <= 0.0 + 1e-3, "critical spring should not overshoot past to; max was {}", max_x);
    }

    #[test]
    fn spring_under_damped_overshoots_past_to() {
        // ζ=0.5 ⇒ overshoots. From -100 toward 0, should cross past 0
        // into positive territory at least once.
        let sim = SpringSimulation::new(
            SpringDescription::with_damping_ratio(1.0, 340.0, 0.5),
            -100.0, 0.0, 0.0,
        );
        let mut max_x = -100.0_f64;
        for i in 0..=500 {
            let t = i as f64 / 120.0;
            max_x = max_x.max(sim.x(t));
        }
        assert!(max_x > 0.0, "under-damped spring should overshoot past to; max was {}", max_x);
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
        assert!(settle_time < 1.0, "spring should settle in under 1s; took {}s", settle_time);
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
            SpringDescription::ios(340.0, 1.0), -100.0, 0.0, 0.0, tol,
        );
        assert_eq!(sim.tolerance(), tol);
    }

    #[test]
    fn spring_zero_motion_is_done_at_t_zero() {
        // from == to, v0 == 0 ⇒ no motion ⇒ is_done immediately.
        let sim = critical_spring(50.0, 50.0, 0.0);
        assert!(sim.is_done(0.0));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib simulation::tests::spring`
Expected: compile error — `SpringDescription`/`SpringSimulation` not found.

- [ ] **Step 3: Implement `SpringDescription` + `SpringSimulation`**

Add above the `#[cfg(test)]` block in `vexo/src/animation/simulation.rs`:

```rust
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
        Self { mass, stiffness, damping }
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
        Self { desc, from, to, v0, tolerance }
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
        if (self.from - self.to).abs() < self.tolerance.distance && self.v0.abs() < self.tolerance.velocity {
            return true;
        }
        (self.x(t) - self.to).abs() < self.tolerance.distance
            && self.dx(t).abs() < self.tolerance.velocity
    }

    fn tolerance(&self) -> Tolerance {
        self.tolerance
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib simulation::tests`
Expected: all spring + friction + tolerance tests pass.

If `spring_settles_within_one_second` fails: the analytic critical spring with k=340, m=1 has `ω₀≈18.44`. Settle time for the analytic form should be *faster* than the Euler approximation (Euler over-damps slightly with substepping). If the test fails, log the actual settle time — if it's legitimately different (e.g. 0.8s vs 0.95s), the test still asserts `< 1.0` so it should pass. If it fails with a value > 1.0, that's a real bug — investigate the math before adjusting.

Run: `cargo build -p vexo`
Expected: clean.

- [ ] **Step 5: Re-export `SpringDescription` (NOT `SpringSimulation` — name collision)**

Modify `vexo/src/animation/mod.rs` — change the simulation re-export:

```rust
pub use simulation::{FrictionSimulation, Simulation, SpringDescription, Tolerance};
```

(`SpringSimulation` is intentionally NOT re-exported here — it collides with the old `spring.rs::SpringSimulation` still used by ScrollView until Task 5. Internal code references it as `crate::animation::simulation::SpringSimulation`.)

Modify `vexo/src/lib.rs:103-107` — add `SpringDescription`:

```rust
pub use animation::{
    AnimationController, AnimationDirection, AnimationTicker, ColorTween, CubicBezierCurve,
    CurvedAnimation, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, FloatTween,
    FrictionSimulation, LinearCurve, Simulation, SpringDescription, TickHandle, Tolerance, Tween,
};
```

- [ ] **Step 6: Run full test suite + build**

Run: `cargo test -p vexo`
Expected: all pass (existing + new). The old `spring.rs`/`momentum.rs` tests still pass (untouched).

Run: `cargo build -p vexo`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/animation/simulation.rs vexo/src/animation/mod.rs vexo/src/lib.rs
git commit -m "feat(vexo): add analytic SpringSimulation + SpringDescription"
```

---

## Task 4: `AnimationController::animate_with` + `Drive` enum

Refactor the controller's internal state into a `Drive` enum (folding existing `direction`/`start_time`/`duration` fields), preserving the public time-based API (`forward`/`reverse`/`forward_with_start`/`stop`/`value`/`direction`/`start_time`), and add the new physics driving mode `animate_with(sim)`.

**Files:**
- Modify: `vexo/src/animation/controller.rs`

**Interfaces:**
- Consumes: `Simulation` trait (from Task 1), `crate::animation::simulation::SpringSimulation` (from Task 3, used in tests).
- Produces: `AnimationController::animate_with(Box<dyn Simulation>)`, `AnimationController::is_animating() -> bool`. Public time-based API unchanged.

- [ ] **Step 1: Read the current controller to confirm exact field/impl layout**

Run: `cargo test -p vexo --lib controller` (baseline — must pass before changes)
Expected: all existing controller tests pass (this is the regression baseline for the refactor).

- [ ] **Step 2: Write the failing tests for `animate_with`**

Append to the `tests` module in `vexo/src/animation/controller.rs`. First add the import at the top of the `tests` module (after `use super::*;`):

```rust
    use crate::animation::simulation::SpringSimulation;
    use crate::animation::SpringDescription;
```

Then append these tests:

```rust
    fn critical_spring_sim(from: f64, to: f64, v0: f64) -> SpringSimulation {
        SpringSimulation::new(SpringDescription::ios(340.0, 1.0), from, to, v0)
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
        assert!((ctrl.value() - 1.0).abs() < 1e-3, "should be at to=1.0, got {}", ctrl.value());
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
        assert!((ctrl.value() - 2.0).abs() < 1e-3, "second sim's to=2.0 should win, got {}", ctrl.value());
        let _ = counter; // suppress unused
    }

    #[test]
    fn forward_cancels_sim() {
        let (mut ctrl, _rx, ticker) = controller_with_ticker();
        ctrl.animate_with(Box::new(critical_spring_sim(0.0, 1.0, 0.0)));
        assert!(ticker.has_active());
        ctrl.forward();
        // forward() should have replaced the sim drive with a time drive.
        let start = ctrl.start_time().unwrap();
        ctrl.advance(start + std::time::Duration::from_millis(500));
        // Time drive at 500ms of 1s duration (default in controller_with_ticker).
        // Value should be ~0.5 (linear), NOT a spring value.
        assert!((ctrl.value() - 0.5).abs() < 0.05, "forward should cancel sim; got {}", ctrl.value());
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
            0.0, 1.0, 0.0,
        );
        ctrl.animate_with(Box::new(sim));
        let start = ctrl.start_time().unwrap();
        // Sample mid-flight for overshoot.
        let mut max_value = 0.0_f64;
        for i in 1..=120 {
            ctrl.advance(start + std::time::Duration::from_secs_f64(i as f64 / 120.0));
            max_value = max_value.max(ctrl.value());
            if !ctrl.is_animating() { break; }
        }
        assert!(max_value > 1.0, "under-damped should overshoot past to=1.0; max was {}", max_value);
        assert!(!ctrl.is_animating(), "should settle");
        assert!((ctrl.value() - 1.0).abs() < 1e-3, "should settle at to=1.0, got {}", ctrl.value());
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
```

Add the `controller_with_ticker` helper at the top of the `tests` module (after the imports from Step 2):

```rust
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
        ctrl.set_dirty_callback(std::sync::Arc::new(move || { let _ = tx.send(()); }));
        ctrl.set_ticker(ticker.clone());
        (ctrl, rx, ticker)
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p vexo --lib controller::tests::animate_with` (and the other new test names)
Expected: compile error — `animate_with` / `is_animating` not found.

- [ ] **Step 4: Refactor `AnimationController` into `Drive` enum + add `animate_with`**

Replace the entire `AnimationController` struct + impl block in `vexo/src/animation/controller.rs`. Keep the existing `AnimationDirection` enum and the existing tests. The new struct + impl:

```rust
use super::simulation::Simulation;

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
    /// `Instant::now()`. Used to synchronize a Vexo tween with an animation
    /// that already began (e.g. the iOS software keyboard). See the original
    /// doc comment at controller.rs:40-54 — preserved verbatim.
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

    /// Drive the controller with a physics simulation. `sim.x(t)` IS the
    /// value (the sim owns from/to/v0). Stamps `start_time`, registers the
    /// ticker, fires dirty immediately (avoids the render_retain deadlock).
    /// Cancels any prior time or sim drive first.
    pub fn animate_with(&mut self, sim: Box<dyn Simulation>) {
        self.unregister_from_ticker();
        self.drive = Drive::Simulation { sim, start: Instant::now() };
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
            Drive::Time { direction, start, duration } => {
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
        let elapsed = now
            .saturating_duration_since(start)
            .as_secs_f64();
        let duration = duration.as_secs_f64();
        let raw = elapsed / duration;

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
```

- [ ] **Step 5: Run ALL controller tests — existing + new**

Run: `cargo test -p vexo --lib controller`
Expected: ALL tests pass — the existing time-path tests (unchanged behavior) AND the new `animate_with`/`is_animating` tests.

If any existing test fails: the `Drive::Time` refactor broke behavior. Re-read the original `advance()` (git diff) and find the divergence. Do NOT adjust the existing tests — they are the regression gate for the refactor being behavior-preserving.

- [ ] **Step 6: Run full test suite + build**

Run: `cargo test -p vexo`
Expected: all pass. (Note: `KeyboardAvoidance`, `NavigationStackView`, etc. use `forward()`/`reverse()`/`forward_with_start()` — their behavior is preserved by the `Drive::Time` path.)

Run: `cargo build -p vexo`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/animation/controller.rs
git commit -m "feat(vexo): add AnimationController::animate_with + Drive enum refactor"
```

---

## Task 5: `ScrollPhysics` + `ScrollViewElement` refactor + delete old sims

The swap task. Deletes `spring.rs`/`momentum.rs`, flips the `SpringSimulation` re-export from old→new, adds the `ScrollPhysics` config struct, and refactors `ScrollViewElement` to source math from the new sims. This is the task where the name collision resolves.

**Files:**
- Modify: `vexo/src/widgets/scroll_view.rs` (add `ScrollPhysics`)
- Modify: `vexo/src/elements/scroll_view.rs` (refactor `ScrollDrive`)
- Delete: `vexo/src/animation/spring.rs`
- Delete: `vexo/src/animation/momentum.rs`
- Modify: `vexo/src/animation/mod.rs` (drop old modules/re-exports, add new `SpringSimulation`)
- Modify: `vexo/src/lib.rs` (update re-exports)

**Interfaces:**
- Consumes: `FrictionSimulation`, `SpringSimulation`, `SpringDescription`, `Tolerance`, `Simulation` (from Tasks 1-3).
- Produces: `ScrollPhysics` struct (re-exported from `vexo::widgets::scroll_view`). ScrollView now physics-configurable.

- [ ] **Step 1: Read the current `rebuild_from_state` momentum/spring arms to confirm the exact control flow being translated**

Read `vexo/src/elements/scroll_view.rs` lines 595-710 (the `momentum.is_active()` and `spring.is_active()` arms). This is the control flow being preserved 1:1 — only sim construction and `advance(now)→x(t)` call shape changes.

Run: `cargo test -p vexo --lib elements::scroll_view` (baseline — 37 tests must pass before changes).

- [ ] **Step 2: Add `ScrollPhysics` to the widget file**

Modify `vexo/src/widgets/scroll_view.rs`. Add after the `use` block (after line 13), before `pub struct ScrollView`:

```rust
use crate::animation::{SpringDescription, Tolerance};

/// Physics configuration for a `ScrollView`. Fixes ROADMAP §9
/// "no ScrollPhysics abstraction" — physics was previously hardcoded
/// inline in `ScrollViewElement` (`STIFFNESS=340`, `TAU=0.325`, etc.).
#[derive(Debug, Clone, Copy)]
pub struct ScrollPhysics {
    /// Spring for bounce-back / overscroll return.
    pub spring: SpringDescription,
    /// Drag time-constant `τ` for `FrictionSimulation` (fling decay).
    pub friction: f64,
    /// Minimum fling velocity (px/s) — below this, a pointer-up does not fling.
    pub fling_min_velocity: f32,
    /// Px-scale settle tolerance for scroll sims.
    pub settle: Tolerance,
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        Self {
            spring: SpringDescription::ios(340.0, 1.0), // today's STIFFNESS/DAMPING_RATIO
            friction: 0.325,                            // today's TAU
            fling_min_velocity: 13.0,                   // today's V_STOP
            settle: Tolerance::SCROLL,                  // today's X_SETTLE/V_SETTLE/MAX_DURATION
        }
    }
}
```

Add a `physics` field to `ScrollView` (modify the struct at line 15):

```rust
pub struct ScrollView {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    controller: Option<ScrollController>,
    physics: ScrollPhysics,
}
```

Update `ScrollView::new` (line 22) to default `physics`:

```rust
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            controller: None,
            physics: ScrollPhysics::default(),
        }
    }
```

Add a builder method after `controller` (after line 38):

```rust
    pub fn physics(mut self, physics: ScrollPhysics) -> Self {
        self.physics = physics;
        self
    }

    pub fn physics_ref(&self) -> ScrollPhysics {
        self.physics
    }
```

Update `Clone for ScrollView` (line 46) to clone `physics` (it's `Copy`, so just add the field):

```rust
impl Clone for ScrollView {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            controller: self.controller.clone(),
            physics: self.physics,
        }
    }
}
```

- [ ] **Step 3: Refactor `ScrollViewElement` to use new sims**

Modify `vexo/src/elements/scroll_view.rs`. This is the core refactor. First, update the imports (line 7):

Current:
```rust
use crate::animation::{AnimationTicker, MomentumSimulation, SpringSimulation};
```

Replace with:
```rust
use crate::animation::{
    AnimationTicker, FrictionSimulation, ScrollPhysics, SpringDescription,
    SpringSimulation as SpringMath, Tolerance,
};
use crate::animation::simulation::Simulation as _;
```

Wait — `ScrollPhysics` is defined in `widgets::scroll_view`, not `animation`. Fix the import. Replace the import block with:

```rust
use crate::animation::{
    AnimationTicker, FrictionSimulation, SpringDescription, Tolerance,
};
use crate::animation::simulation::{Simulation, SpringSimulation as SpringMath};
use crate::widgets::scroll_view::ScrollPhysics;
```

(`SpringMath` aliases the new `SpringSimulation` to avoid confusion with the now-deleted old one during the transition — the alias makes the refactor self-documenting. `Simulation` is imported as a trait for calling `sim.x()`/`sim.dx()`/`sim.is_done()` methods.)

Now replace the `momentum`/`spring` fields in `ScrollViewElement` (lines 80-84) with a `ScrollDrive` enum + `physics` field. Current:

```rust
    momentum: MomentumSimulation,
    /// Critically-damped spring for bounce-back. Mutually exclusive with
    /// `momentum` — starting one stops the other. Stepped in
    /// `rebuild_from_state` while `is_active()`.
    spring: SpringSimulation,
```

Replace with:

```rust
    /// Active scroll physics drive. `Idle` when at rest. `Fling`/`Bounce`
    /// source math from the new pure-math sims; the ticker/dirty plumbing
    /// stays here (ScrollView can't use AnimationController::animate_with
    /// because it operates in px and needs mid-flight velocity handoff).
    drive: ScrollDrive,
    /// Stashed physics config (from the widget). Replaces the old module-level
    /// `const STIFFNESS`/`TAU`/etc.
    physics: ScrollPhysics,
```

Add the `ScrollDrive` enum above `ScrollViewElement` (after the imports, before `pub struct ScrollViewElement`):

```rust
/// Active physics drive for scroll. One sim active at a time; starting one
/// stops the other (preserves the old momentum/spring mutual-exclusion).
enum ScrollDrive {
    Idle,
    Fling {
        sim: FrictionSimulation,
        start: Instant,
    },
    Bounce {
        sim: SpringMath,
        start: Instant,
    },
}

impl ScrollDrive {
    fn is_active(&self) -> bool {
        !matches!(self, ScrollDrive::Idle)
    }
}
```

Update `ScrollViewElement::new` (lines 96-115) — replace the `momentum`/`spring` field inits with `drive`/`physics`:

```rust
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
            scroll_offset: 0.0,
            content_height: 0.0,
            viewport_height: 0.0,
            controller: None,
            last_drag_y: 0.0,
            velocity_tracker: VelocityTracker::new(),
            drive: ScrollDrive::Idle,
            physics: ScrollPhysics::default(),
            animation_ticker: None,
            last_move_time: None,
        }
    }
```

In `set_widget` (around line 182-188), capture the widget's `physics` alongside the controller:

```rust
    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Some(sv) = widget
            .as_any()
            .downcast_ref::<crate::widgets::scroll_view::ScrollView>()
        {
            self.key = sv.key().clone();
            self.controller = sv.controller_ref().cloned();
            self.physics = sv.physics_ref();
        }
        self.widget = Some(widget);
    }
```

Now the `rebuild_from_state` arms. This is the mechanical translation. The old code at lines 601-699 had two `if self.momentum.is_active()` / `if self.spring.is_active()` blocks. Replace them with a single `match &mut self.drive` block. The exact replacement depends on the surrounding code, so read lines 595-710 first, then apply this translation:

- Where the old code called `self.momentum.start(offset0, v0, now, tx, element_id, ticker)`, the new code builds a `FrictionSimulation::with_tolerance(offset0, v0, physics.friction, physics.settle)`, stores it in `self.drive = ScrollDrive::Fling { sim, start: now }`, registers the ticker callback (the same `tx.send(element_id)` closure), and fires `tx.send(element_id)` immediately.
- Where the old code called `self.spring.start(clamped, v, rest, now, tx, element_id, ticker)`, the new code builds `SpringMath::with_tolerance(physics.spring, clamped, rest, v, physics.settle)`, stores `self.drive = ScrollDrive::Bounce { sim, start: now }`, registers + fires.
- In the `rebuild_from_state` fling arm: `let t = now.saturating_duration_since(start).as_secs_f64(); let physics_offset = sim.x(t) as f32;` then the existing clamp/edge-hit logic. On edge-hit, capture `let v = sim.dx(t) as f32;` and swap to `Bounce`.
- In the bounce arm: `let t = ...; let physics_offset = sim.x(t) as f32;` write to scroll offset. `if sim.is_done(t)` → snap to `rest`, set `self.drive = ScrollDrive::Idle`, unregister ticker.
- `self.momentum.stop()` / `self.spring.stop()` → `self.drive = ScrollDrive::Idle` + unregister ticker.

Because the exact line-by-line edit is large and depends on the surrounding context (the arms reference `context.dirty_sender`, `self.id`, `self.animation_ticker`, `self.render_object`, `self.controller`), the implementer should: (a) read the current `rebuild_from_state` in full, (b) apply the translation above mechanically, preserving every clamp, edge-hit, snap-to-rest, and controller-write exactly. The 37 existing element tests are the gate that the translation is behavior-preserving.

Add a helper for the ticker register/unregister to keep the arms clean (place it among the other `ScrollViewElement` helper methods, e.g. near `apply_scroll_offset`):

```rust
    /// Register a ticker callback that sends this element's id through the
    /// dirty channel. Returns the handle. Shared by the Fling/Bounce start
    /// paths — replaces the old sims' built-in `start(...tx, element_id, ticker)`.
    fn register_ticker(
        &mut self,
        tx: std::sync::mpsc::Sender<crate::id::ElementKey>,
        ticker: std::sync::Arc<AnimationTicker>,
    ) -> Option<TickHandle> {
        let element_id = self.id?;
        let cb: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
            let _ = tx.send(element_id);
        });
        Some(ticker.register(cb))
    }

    fn unregister_ticker(&mut self) {
        if let (Some(ticker), Some(handle)) = (self.animation_ticker.clone(), self.tick_handle.take()) {
            ticker.unregister(handle);
        }
    }
```

(This assumes a `tick_handle` field on `ScrollViewElement` — add it next to `animation_ticker`. The old `momentum`/`spring` structs each held their own handle; now one shared handle lives on the element.)

Add `tick_handle: Option<TickHandle>` to the struct + `None` init in `new`. Add `use crate::animation::TickHandle;` to the imports.

- [ ] **Step 4: Delete the old sim files + flip re-exports**

Delete: `vexo/src/animation/spring.rs`
Delete: `vexo/src/animation/momentum.rs`

Modify `vexo/src/animation/mod.rs` — current:

```rust
pub mod controller;
pub mod curve;
pub mod momentum;
pub mod simulation;
pub mod spring;
pub mod ticker;
pub mod tween;

pub use controller::{AnimationController, AnimationDirection};
pub use curve::{
    CubicBezierCurve, Curve, CurvedAnimation, EaseInCurve, EaseInOutCurve, EaseOutCurve,
    LinearCurve,
};
pub use momentum::MomentumSimulation;
pub use simulation::{FrictionSimulation, Simulation, SpringDescription, Tolerance};
pub use spring::SpringSimulation;
pub use ticker::{AnimationTicker, TickHandle};
pub use tween::{ColorTween, FloatTween, Tween};
```

Replace with:

```rust
pub mod controller;
pub mod curve;
pub mod simulation;
pub mod ticker;
pub mod tween;

pub use controller::{AnimationController, AnimationDirection};
pub use curve::{
    CubicBezierCurve, Curve, CurvedAnimation, EaseInCurve, EaseInOutCurve, EaseOutCurve,
    LinearCurve,
};
pub use simulation::{
    FrictionSimulation, Simulation, SpringDescription, SpringSimulation, Tolerance,
};
pub use ticker::{AnimationTicker, TickHandle};
pub use tween::{ColorTween, FloatTween, Tween};
```

Modify `vexo/src/lib.rs:103-107` — add `SpringSimulation` (drop nothing — `SpringSimulation` was never in the top-level re-export before since the old one was being phased out; `MomentumSimulation` was never top-level re-exported either). Current:

```rust
pub use animation::{
    AnimationController, AnimationDirection, AnimationTicker, ColorTween, CubicBezierCurve,
    CurvedAnimation, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, FloatTween,
    FrictionSimulation, LinearCurve, Simulation, SpringDescription, TickHandle, Tolerance, Tween,
};
```

Replace with:

```rust
pub use animation::{
    AnimationController, AnimationDirection, AnimationTicker, ColorTween, CubicBezierCurve,
    CurvedAnimation, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, FloatTween,
    FrictionSimulation, LinearCurve, Simulation, SpringDescription, SpringSimulation, TickHandle,
    Tolerance, Tween,
};
```

- [ ] **Step 5: Build + fix any remaining references to deleted types**

Run: `cargo build -p vexo`
Expected: possibly compile errors elsewhere in the workspace (e.g. `shared_app` or `vexo_uikit` if they referenced `MomentumSimulation` or the old `SpringSimulation` directly). Search and fix:

Run: `rg "MomentumSimulation|animation::SpringSimulation\b" --type rust` (excluding `vexo/src/animation/simulation.rs`)

Any hit outside `simulation.rs` is a reference to a deleted type. Replace per the new API:
- `MomentumSimulation::new()` + `.start(offset0, v0, now, tx, id, ticker)` + `.advance(now)` → consumer builds a `FrictionSimulation::with_tolerance(...)` and drives it (only ScrollView did this; should be fully handled by Step 3).
- Old `SpringSimulation::new()` + `.start(...)` + `.advance(...)` → new `SpringSimulation::with_tolerance(...)` + `sim.x(t)` (only ScrollView did this).

- [ ] **Step 6: Run the full test suite — the regression gate**

Run: `cargo test --workspace`
Expected: ALL tests pass. This is the critical gate:
- 37 ScrollViewElement tests (the refactor must be behavior-preserving).
- 11 spring tests migrated to `simulation::tests::spring` (Task 3).
- 12 momentum tests migrated to `simulation::tests::friction` (Task 2).
- All controller tests (Task 4).
- All `shared_app`/`vexo_uikit` tests (they use `AnimationController::forward`/`reverse`, unaffected).

If a ScrollViewElement test fails: investigate whether it's (a) a real regression in the refactor — fix the refactor, OR (b) a legitimate settle-time difference from the analytic spring vs the old Euler spring. For (b) only: log the actual vs expected, cite the analytic justification (the closed-form is exact; Euler over-damps slightly with 1/120s substepping), and adjust the expected value with a comment citing the math. Do NOT relax assertions to make tests pass.

- [ ] **Step 7: Add a test proving the `ScrollPhysics` config surface works**

The existing `setup_scroll_view(&ctrl)` helper (scroll_view.rs:1267) builds a default-physics `ScrollView`. For this test, add a variant that accepts physics, then compare pump-count-to-settle between default and stiff physics. The existing `test_spring_settles_to_top_edge` test (scroll_view.rs:1025) is the model: drag past top edge, release, pump until `!ticker.has_active()`, count pumps.

Append to the `#[cfg(test)]` module in `vexo/src/elements/scroll_view.rs`. First, add a `setup_scroll_view_with_physics` helper near the existing `setup_scroll_view` (scroll_view.rs:1267). It's identical except it takes a `ScrollPhysics` and calls `.physics(...)`:

```rust
    /// Like `setup_scroll_view` but injects custom `ScrollPhysics`. Used by
    /// `stiffer_physics_settles_faster_than_default` to prove the config
    /// surface drives the bounce-back sim (ROADMAP §9 ScrollPhysics gap).
    fn setup_scroll_view_with_physics(
        ctrl: &crate::widgets::ScrollController,
        physics: crate::widgets::scroll_view::ScrollPhysics,
    ) -> (
        std::sync::Arc<crate::animation::AnimationTicker>,
        crate::ThreeTreePipeline,
        glyphon::FontSystem,
    ) {
        use crate::animation::AnimationTicker;
        use crate::widgets::ScrollView;
        use crate::{Layout, MultiChild};
        use std::sync::Arc;

        let mut col = MultiChild::empty(Layout::column());
        for _ in 0..200 {
            col = col.push(crate::Text::new("row"));
        }
        let sv = ScrollView::new(col.boxed()).controller(ctrl.clone()).physics(physics);
        let ticker = Arc::new(AnimationTicker::new());
        let mut pipeline = crate::ThreeTreePipeline::new(ticker.clone());
        pipeline.reconcile(Box::new(sv));
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        (ticker, pipeline, font_system)
    }
```

Then append the test (model the drag-release-pump loop on `test_spring_settles_to_top_edge` at scroll_view.rs:1025):

```rust
    #[test]
    fn stiffer_physics_settles_faster_than_default() {
        // A stiffer spring (k=2000 vs default 340) should settle in fewer
        // pumps. Proves the ScrollPhysics config surface actually drives the
        // bounce-back sim (ROADMAP §9 ScrollPhysics gap).
        use crate::animation::SpringDescription;
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::scroll_view::ScrollPhysics;
        use crate::widgets::ScrollController;

        // Drag past top edge + release, then count pumps until settled.
        // Returns the pump count. Modeled on test_spring_settles_to_top_edge.
        fn settle_pump_count(physics: ScrollPhysics) -> usize {
            let ctrl = ScrollController::new();
            let (ticker, mut pipeline, mut font_system) =
                setup_scroll_view_with_physics(&ctrl, physics);

            // Press + drag down past top (overscroll).
            let press = InputEvent::PointerButton {
                position: Point::new(200.0, 300.0),
                button: PointerButton::Primary,
                state: ButtonState::Pressed,
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 300.0), &press);
            let mv = InputEvent::PointerMoved { position: Point::new(200.0, 500.0) };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 500.0), &mv);
            let release = InputEvent::PointerButton {
                position: Point::new(200.0, 500.0),
                button: PointerButton::Primary,
                state: ButtonState::Released,
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, 500.0), &release);

            // Pump until spring settles. The 2ms sleep lets each pump's
            // advance see ~2ms of elapsed wall-clock time (the sim uses
            // Instant::now()), so the spring settles after ~200 pumps for
            // the default k=340.
            let mut pumps = 0;
            for _ in 0..5000 {
                pump(&ticker, &mut pipeline);
                pumps += 1;
                if !ticker.has_active() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert!(!ticker.has_active(), "spring should have settled");
            pumps
        }

        let default_pumps = settle_pump_count(ScrollPhysics::default());
        let stiff_pumps = settle_pump_count(ScrollPhysics {
            spring: SpringDescription::ios(2000.0, 1.0), // ~6× stiffer
            ..ScrollPhysics::default()
        });
        assert!(
            stiff_pumps < default_pumps,
            "stiffer spring (k=2000) should settle faster than default (k=340); \
             got stiff={} pumps, default={} pumps",
            stiff_pumps,
            default_pumps
        );
    }
```

- [ ] **Step 8: Run the new test + full suite + build**

Run: `cargo test -p vexo --lib elements::scroll_view::tests::stiffer_physics_settles_faster_than_default`
Expected: pass (after the implementer replaces the `todo!()` with a real assertion).

Run: `cargo test --workspace`
Expected: all pass.

Run: `cargo build -p vexo`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add vexo/src/widgets/scroll_view.rs vexo/src/elements/scroll_view.rs \
        vexo/src/animation/mod.rs vexo/src/lib.rs
git rm vexo/src/animation/spring.rs vexo/src/animation/momentum.rs
git commit -m "feat(vexo): ScrollPhysics config + refactor ScrollView to new sims

Delete the old hardcoded SpringSimulation/MomentumSimulation (stateful,
ticker-coupled, semi-implicit Euler). ScrollView now sources math from
the new pure-math Simulation trait (analytic spring, configurable
friction) via a ScrollPhysics config struct. Fixes ROADMAP §9 ScrollPhysics gap."
```

---

## Task 6: Update ROADMAP + final verification

**Files:**
- Modify: `ROADMAP.md`

- [ ] **Step 1: Update ROADMAP §7 (Animation) and §9 (Scrolling) gap status**

In `ROADMAP.md`, §7 "Animation" — the "Missing" list mentions "implicit animations" etc. but the existing list under "Exists" should now mention the new physics API. Find the §7 "Exists" paragraph (around line 229-242) and append:

```markdown
  **`Simulation` trait + `SpringSimulation` (analytic) + `FrictionSimulation`**
  (`animation/simulation.rs` — pure-math, stateless, configurable mass/stiffness/damping-ratio,
  iOS `UISpringTimingParameters` + Flutter `package:physics` model),
  **`AnimationController::animate_with(sim)`** (physics-driven third driving mode)
```

In §9 "Scrolling" — the "Missing" list mentions "ScrollPhysics abstraction ... physics is hardcoded inline". Remove that bullet (it's now fixed) and move `ScrollPhysics` to the "Exists" list. Find (around line 318-319):

```markdown
- `ScrollPhysics` abstraction (`Bouncing`/`Clamping`/`AlwaysScrollable`/`NeverScrollable`)
  — physics is hardcoded inline in `ScrollViewElement` (touch → rubber-band, wheel/kbd → clamp)
  with no caller-selectable policy
```

Replace with (in the "Exists" list above):

```markdown
- **`ScrollPhysics` config struct** (`widgets/scroll_view.rs` — configurable spring/friction/
  settle-tolerance; default = today's hardcoded values; `ScrollView::physics(p)` builder)
```

Note: the full `Bouncing`/`Clamping`/`AlwaysScrollable`/`NeverScrollable` *policy* abstraction is still missing — only the config-struct layer shipped. Keep a reduced bullet in "Missing":

```markdown
- `ScrollPhysics` *policy* abstraction (`Bouncing`/`Clamping`/`AlwaysScrollable`/`NeverScrollable`
  as selectable behaviors) — only the config-struct layer (`ScrollPhysics { spring, friction, ... }`)
  ships; touch→rubber-band / wheel→clamp is still hardcoded inline
```

- [ ] **Step 2: Final full verification**

Run: `cargo test --workspace`
Expected: ALL pass (~1164+ tests, plus the new ones from Tasks 1-5).

Run: `cargo build -p vexo --release`
Expected: clean.

Run: `cargo build --workspace`
Expected: clean (no broken references in `shared_app`/`vexo_uikit`/`desktop_demo`).

- [ ] **Step 3: Commit**

```bash
git add ROADMAP.md
git commit -m "docs(roadmap): mark physics animation + ScrollPhysics config as shipped"
```

---

## Self-Review Notes

**Spec coverage check:**
- §3.1 Simulation trait + Tolerance → Task 1 ✓
- §3.2 SpringDescription → Task 3 ✓
- §3.3 SpringSimulation (analytic) → Task 3 ✓
- §3.4 FrictionSimulation → Task 2 ✓
- §3.5 Retired code (delete spring.rs/momentum.rs) → Task 5 ✓
- §3.6 AnimationController Drive enum + animate_with → Task 4 ✓
- §3.7 ScrollPhysics → Task 5 ✓
- §3.8 ScrollViewElement refactor → Task 5 ✓
- §4 User-facing API → exercised by Task 4 tests ✓
- §5 Edge cases (overshoot, zero-motion, NaN, MAX_DURATION, no-Drop, object safety) → covered by Tasks 2-4 tests ✓
- §6 Behavioral risks (Euler→analytic) → Task 3 golden tests + Task 5 regression gate ✓
- §7 File layout → matches ✓
- §8 Testing strategy → Tasks 1-5 tests ✓
- §9 Re-export surface → Tasks 1-5 re-exports ✓

**Placeholder scan:** No `todo!()`/TBD/TODO left in any step. Task 5 Step 7 originally had a `todo!()` for the scroll-physics test; resolved by reading the existing `test_spring_settles_to_top_edge` (scroll_view.rs:1025) and `setup_scroll_view` (scroll_view.rs:1267) helpers and writing a complete `stiffer_physics_settles_faster_than_default` test with a `setup_scroll_view_with_physics` helper, both fully specified inline.

**Type consistency:** `SpringSimulation` aliased as `SpringMath` in scroll_view.rs element to avoid confusion during the transition; the alias resolves to the same type after Task 5's re-export flip. `Simulation` trait imported as a trait in scroll_view.rs for method dispatch. `TickHandle` added to the element struct. `Drive` enum's `Simulation` variant holds `Box<dyn Simulation>`. `animate_with` takes `Box<dyn Simulation>`. All consistent.
