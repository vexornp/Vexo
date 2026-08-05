# Physics-Driven Animation System — Design Spec

**Date:** 2026-08-05
**Status:** Approved (pending implementation)
**Scope:** Add a configurable, user-facing physics animation system to `vexo`, modeled on Flutter's `package:physics` and iOS `UISpringTimingParameters`. Generalize the existing internal `SpringSimulation`/`MomentumSimulation` into a pure-math `Simulation` trait, integrate it with `AnimationController` via `animate_with()`, and refactor `ScrollViewElement` to consume the new types — fixing the hardcoded-physics gap (ROADMAP §9 "no `ScrollPhysics` abstraction") as a side effect.

---

## 1. Background & Motivation

Vexo already has the kernel of a physics animation system:

- **`SpringSimulation`** (`vexo/src/animation/spring.rs`) — critically-damped harmonic oscillator, semi-implicit Euler with 1/120s substepping, settle detection.
- **`MomentumSimulation`** (`vexo/src/animation/momentum.rs`) — exponential-decay fling, analytic closed-form.
- **`AnimationController`** (`animation/controller.rs`) — time-based tween driver, `forward()`/`reverse()`/`advance()`.
- **`AnimationTicker`** (`animation/ticker.rs`) — per-frame callback registry.

But the existing sims have two big limitations relative to the goal of "users can opt to add their own animation with the physics effect":

1. **Hardcoded params.** `STIFFNESS=340`, `DAMPING_RATIO=1.0` (critically-damped only — no overshoot control), `TAU=0.325`, `V_STOP=13.0`. Not configurable.
2. **Framework-internal, not user-facing.** Each sim *owns* the ticker plumbing (`TickHandle`, `dirty_sender`, `ElementKey`). Only `ScrollViewElement` uses them. A user building their own widget cannot say "spring this value to X" without hand-rolling a sim struct + ticker registration + manual `advance()` stepping in `rebuild_from_state`.

So this work is less "add a physics engine" and more **"generalize the existing sims into a user-facing, configurable physics API + unify it with `AnimationController`."**

### Reference designs investigated

- **Flutter `package:physics`** — the closest match. `Simulation` trait (`x(t)`, `dx(t)`, `isDone(t)`, `Tolerance`); `SpringSimulation(SpringDescription, from, end, velocity)` (damped harmonic oscillator, analytic); `SpringDescription { mass, stiffness, damping }` + `.withDampingRatio(r)`; `FrictionSimulation` (exponential decay, ≈ `MomentumSimulation`); `GravitySimulation` (constant accel). **Integration key:** `AnimationController.animateWith(simulation)` samples `sim.x(elapsed)` each frame instead of linear time, stops at `isDone`. One controller, time-based *or* physics-driven.
- **iOS** — `UISpringTimingParameters { mass, stiffness, damping, initialVelocity }` (configurable form) + the simpler `usingSpringWithDamping:dampingRatio:` (0..1, 1=no overshoot). UIKit Dynamics (gravity/collision/snap/attachment) is a separate, heavier system — out of scope here.
- **Rust ecosystem** — no off-the-shelf crate fits. `rapier2d`/`bevy_rapier` are full rigid-body engines (collisions, joints, broad-phase) — overkill for UI animation and heavy deps. `bevy_tweening`/`splines` are tweening, not physics. Nothing mirrors Flutter's lightweight `Simulation` trait. An in-house ~450-line module (pure math, no deps) matches Vexo's existing philosophy — the framework already rolls its own spring/momentum rather than pulling a crate.

### Out of scope (explicitly)

- Implicit physics widgets (`SpringContainer`, `AnimatedPositioned`, etc.) — tracked separately under ROADMAP §7 "implicit animations".
- `GravitySimulation` (Flutter has it; YAGNI for Vexo — no consumer identified; trait is extensible if needed later).
- UIKit-Dynamics-style behavior system (gravity/collision/snap/attachment behaviors) — much larger scope, ~2000+ lines, no current consumer.
- 2D / `Vector2` simulations — users compose two 1D controllers, matching Flutter (which has no 2D spring either).
- Animating non-numeric types via physics (e.g. color springs) — users compose via `Tween` + controller `value()`.
- `ScrollNotification` / `ScrollMetrics` (separate ROADMAP §9 gap).
- A demo screen — the physics primitive layer is the deliverable; a demo is a nice-to-have follow-up.

---

## 2. Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Physics scope | Configurable springs + friction only (Flutter-style `Simulation` trait) | Matches iOS `UISpringTimingParameters` + Flutter `package:physics`. No new deps. What the existing `SpringSimulation` generalizes into. UIKit Dynamics-lite rejected as ~5× scope with no consumer. |
| User consumption model | Explicit `AnimationController::animate_with(sim)` | Matches Flutter exactly. Works for any interpolatable property via `Tween`. Matches the user's "opt to add" phrasing. Implicit physics widgets deferred (builds the missing implicit-animation layer too — separate ROADMAP item). |
| Architecture approach | Flutter-faithful + refactor ScrollView | `Simulation` trait + `Spring`/`Friction` + `animate_with`, refactor `ScrollViewElement` to use new types. Unifies everything, fixes ROADMAP §9 "ScrollPhysics" gap, no duplication. Additive-only (Approach 2) rejected: leaves two spring impls coexisting. Minimal (Approach 3) rejected: no extensibility, doesn't unify `MomentumSimulation`. |
| Simulation trait shape | Stateless `x(t)` / `dx(t)` / `is_done(t)` (not stateful `advance(dt)`) | Decouples physics math from framework plumbing (ticker/dirty). Matches Flutter. Lets any consumer query arbitrary `t`. Required for `animate_with`'s sample-each-frame model. |
| Spring integrator | Analytic closed-form damped-harmonic solution (replaces semi-implicit Euler) | The stateless `x(t)` trait requires it. Frame-rate-independent, no substepping, no dt-clamp. Well-trodden math (Flutter `spring_simulation.dart`). Retires `MAX_FRAME_DT`/`DT`/substep constants. Main behavioral risk — see §6. |
| Tolerance model | Context-dependent: unitless-tight default (`0.001`), px-scale for scroll (`1.0`px / `13.0`px/s) | Resolves "spring works in px for scroll, unitless for controller" tension — same sim, different tolerance. |
| Overshoot policy | No clamping inside controller/sim; user picks `damping_ratio` or clamps in consumer | Clamping would defeat the point of under-damped springs. Documented; matches Flutter. |
| ScrollView integration | Keeps its own ticker/advance loop (not forced through `AnimationController`) | ScrollView operates in px (not 0..1), needs mid-flight velocity handoff (fling→spring on edge hit), writes directly to render object. Controller's `animate_with` (whole-sim replacement) doesn't model mid-flight swap. Mechanical translation of existing control flow; only sim construction + `advance→x(t)` call shape changes. |
| `MAX_DURATION` ceiling | Preserved as `Tolerance.time` default (10s) | Safety net against runaway sims, unchanged from today. |
| Param validation | `SpringDescription::new` panics on `stiffness<=0`/`mass<=0`/`damping<0` | Pre-condition validation, not silent clamping. Matches how Vexo handles other invalid widget configs. |
| Demo screen | Out of scope | Physics primitive layer is the deliverable. Demo is a follow-up. |

---

## 3. Component Design

### 3.1 `Simulation` trait + `Tolerance` — `vexo/src/animation/simulation.rs` (new)

The heart of the design: **decoupling physics math from framework plumbing.** Today `SpringSimulation` owns ticker handles and dirty senders. The new trait is pure math; the framework plumbing stays in `AnimationController` / `ScrollViewElement`.

```rust
/// Settle thresholds for a `Simulation`. Mirrors Flutter's `Tolerance`.
#[derive(Debug, Clone, Copy)]
pub struct Tolerance {
    pub distance: f64,   // |x - target| below this → considered at rest
    pub velocity: f64,   // |dx| below this → considered at rest
    pub time: f64,       // hard ceiling: past this elapsed time → done (safety)
}

impl Tolerance {
    /// Unitless-tight default for the controller path (0..1 progress).
    pub const DEFAULT: Tolerance = Tolerance { distance: 1e-3, velocity: 1e-3, time: 10.0 };
    /// Px-scale tolerance for scroll-view physics (matches today's X_SETTLE / V_SETTLE).
    pub const SCROLL: Tolerance = Tolerance { distance: 1.0, velocity: 13.0, time: 10.0 };
}

impl Default for Tolerance { fn default() -> Self { Self::DEFAULT } }

/// A pure-math physics simulation. Stateless: the same `t` always yields the
/// same `x(t)`. Holds NO framework plumbing (no ticker, no dirty callback) —
/// the caller (AnimationController / ScrollViewElement) owns that.
pub trait Simulation: Send + Sync {
    /// Value at elapsed seconds `t`.
    fn x(&self, t: f64) -> f64;
    /// Velocity (dx/dt) at elapsed seconds `t`.
    fn dx(&self, t: f64) -> f64;
    /// True once the simulation has settled within `tolerance()`.
    fn is_done(&self, t: f64) -> bool;
    /// Settle thresholds. Default = `Tolerance::DEFAULT`.
    fn tolerance(&self) -> Tolerance { Tolerance::DEFAULT }
}
```

`Send + Sync` + no generics → object-safe; `Box<dyn Simulation>` works. `dx(t)` is needed for `is_done` and for fling→spring velocity handoff (ScrollView already does this handoff today).

### 3.2 `SpringDescription` — `vexo/src/animation/simulation.rs`

Mirrors iOS `UISpringTimingParameters` + Flutter `SpringDescription`:

```rust
/// Physical description of a spring. Maps 1:1 to iOS `UISpringTimingParameters`
/// and Flutter `SpringDescription`.
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
    pub fn new(mass: f64, stiffness: f64, damping: f64) -> Self;

    /// Damping-ratio form: `damping = 2·ζ·√(m·k)`.
    /// `ratio < 1.0` → under-damped (overshoots); `= 1.0` → critically-damped
    /// (no overshoot); `> 1.0` → over-damped (sluggish, no overshoot).
    /// Matches iOS `usingSpringWithDamping:` and Flutter `withDampingRatio`.
    pub fn with_damping_ratio(mass: f64, stiffness: f64, ratio: f64) -> Self;

    /// iOS-style convenience: `mass = 1.0`. Equivalent to
    /// `with_damping_ratio(1.0, stiffness, damping_ratio)`.
    /// The existing hardcoded `STIFFNESS=340, DAMPING_RATIO=1.0` becomes
    /// `SpringDescription::ios(340.0, 1.0)`.
    pub fn ios(stiffness: f64, damping_ratio: f64) -> Self;
}
```

### 3.3 `SpringSimulation` — `vexo/src/animation/simulation.rs` (replaces `spring.rs`)

Pure-math, configurable, supports overshoot. Analytic closed-form damped-harmonic solution (three cases: under-damped `ζ<1`, critically-damped `ζ=1`, over-damped `ζ>1`). The math source is Flutter's `spring_simulation.dart`; the three-case split follows the standard ODE solution for `m·x'' + c·x' + k·x = 0`.

```rust
/// Damped harmonic oscillator from `from` to `to` with initial velocity `v0`.
/// Stateless: `x(t)` is the closed-form analytic solution.
pub struct SpringSimulation {
    desc: SpringDescription,
    from: f64,
    to: f64,
    v0: f64,
    tolerance: Tolerance,
}

impl SpringSimulation {
    pub fn new(desc: SpringDescription, from: f64, to: f64, v0: f64) -> Self;
    pub fn with_tolerance(desc: SpringDescription, from: f64, to: f64, v0: f64, tolerance: Tolerance) -> Self;
}

impl Simulation for SpringSimulation {
    fn x(&self, t: f64) -> f64 { /* analytic, three-case */ }
    fn dx(&self, t: f64) -> f64 { /* analytic derivative */ }
    fn is_done(&self, t: f64) -> bool {
        t >= self.tolerance.time
            || (self.x(t) - self.to).abs() < self.tolerance.distance
                && self.dx(t).abs() < self.tolerance.velocity
    }
    fn tolerance(&self) -> Tolerance { self.tolerance }
}
```

**No `Drop` impl.** The new pure-math sim holds no ticker handle (the controller/element owns it). `Box<dyn Simulation>` is just a math object.

### 3.4 `FrictionSimulation` — `vexo/src/animation/simulation.rs` (replaces `momentum.rs`)

Pure-math extraction of the existing `MomentumSimulation`, which is *already* analytic (`x(t) = x0 + v0·τ·(1 - e^(-t/τ))`). The only change is configurable drag (`τ`) instead of hardcoded `TAU=0.325`.

```rust
/// Exponential-decay fling. `x(t) = x0 + v0·τ·(1 - e^(-t/τ))`.
pub struct FrictionSimulation {
    x0: f64,
    v0: f64,
    drag: f64,          // τ — decay time constant
    tolerance: Tolerance,
}

impl FrictionSimulation {
    pub fn new(x0: f64, v0: f64, drag: f64) -> Self;                       // DEFAULT tolerance
    pub fn with_tolerance(x0: f64, v0: f64, drag: f64, t: Tolerance) -> Self;
}

impl Simulation for FrictionSimulation { /* x, dx, is_done — closed-form */ }
```

### 3.5 Retired code

- `vexo/src/animation/spring.rs` — **deleted.** The `SpringSimulation` struct (stateful, ticker-coupled) is superseded by §3.3. Its math moves into the analytic form. Tests migrate.
- `vexo/src/animation/momentum.rs` — **deleted.** The `MomentumSimulation` struct is superseded by §3.4. Tests migrate.
- Module-level `const STIFFNESS/DAMPING_RATIO/X_SETTLE/V_SETTLE/DT/MAX_FRAME_DT/MAX_DURATION` (spring.rs) and `TAU/V_STOP/MAX_DURATION` (momentum.rs) — moved into `Tolerance::DEFAULT`/`Tolerance::SCROLL` and `ScrollPhysics::default()` (§3.7). The Euler-only `DT`/`MAX_FRAME_DT` constants are **retired** (analytic form needs no substep/clamp).

### 3.6 `AnimationController` integration — `vexo/src/animation/controller.rs` (refactored)

The controller gains a third driving mode alongside `forward()`/`reverse()` (time-based). It becomes the single integration point — one place that owns the ticker handle, dirty callback, and the `advance` loop.

**New internal state** (folds existing `direction`/`start_time` fields into a private enum; public accessors `direction()`/`start_time()` preserved for back-compat):

```rust
enum Drive {
    Stopped,
    Time { direction: AnimationDirection, start: Instant, duration: Duration },
    Simulation { sim: Box<dyn Simulation>, start: Instant },
}

pub struct AnimationController {
    drive: Drive,
    value: f64,                                    // may overshoot [0,1] on the Simulation path
    duration: Duration,                            // used by forward()/reverse() to populate Drive::Time
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    ticker: Option<Arc<AnimationTicker>>,
    tick_handle: Option<TickHandle>,
}
```

**New public API:**

```rust
impl AnimationController {
    /// Drive the controller with a physics simulation. `sim.x(t)` IS the
    /// value (the sim owns from/to/v0). Stamps `start_time`, registers the
    /// ticker, fires dirty immediately (same immediate-fire pattern as
    /// `forward()` at controller.rs:63-70 — avoids the render_retain deadlock
    /// documented there). Cancels any prior time or sim drive first.
    pub fn animate_with(&mut self, sim: Box<dyn Simulation>);
}
```

The sim owns `from`/`to`/`v0` (Flutter-faithful: `SpringSimulation::new(desc, from, to, v0)`); `animate_with` just stamps time and drives the ticker. `value = sim.x(elapsed)`. Avoids double-encoding from/to between sim and controller.

**`advance(now)` gains a `Simulation` arm:**

```rust
match self.drive {
    Drive::Simulation { ref sim, start } => {
        let elapsed = now.since(start).as_secs_f64();
        self.value = sim.x(elapsed);
        if sim.is_done(elapsed) {
            self.drive = Drive::Stopped;
            self.unregister_from_ticker();
        }
        if let Some(cb) = &self.dirty_callback { cb(); }
    }
    Drive::Time { .. } => { /* existing logic, refactored into the enum */ }
    Drive::Stopped => return,
}
```

**Existing time-based API unchanged.** `forward()` / `reverse()` / `forward_with_start(start)` (the keyboard-sync method, controller.rs:55) populate `Drive::Time` — behavior identical to today. Callers like `KeyboardAvoidance` and `NavigationStackView` keep working untouched. The `Drive::Time` refactor is purely internal.

**New `is_animating()` accessor** replaces the `direction != Stopped` check callers make today (since `Simulation` has no `AnimationDirection`). Existing `direction()` stays for the `Time` path and returns `Stopped` on the `Simulation` path.

**Retargeting / cancellation.** Calling `animate_with` while one is running stops the old one first (same `self.unregister_from_ticker()` pattern as `forward()` at controller.rs:56). Calling `forward()` while a `Simulation` is running cancels the sim. Matches today's `forward`-then-`forward` reregistration (test at controller.rs:315) and the regression comment at controller.rs:360. **`animate_with` does not accept a gesture velocity** — for gesture-driven springs (fling→spring handoff, swipe-back), the *sim* carries `v0` in its constructor and the caller builds the sim with the gesture velocity before calling `animate_with`. Keeps the controller gesture-agnostic.

### 3.7 `ScrollPhysics` — `vexo/src/widgets/scroll_view.rs` (or new `scroll_physics.rs`)

A small config struct (not a trait — YAGNI: only one policy today, iOS-style). Fixes ROADMAP §9 "no `ScrollPhysics` abstraction":

```rust
pub struct ScrollPhysics {
    pub spring: SpringDescription,        // bounce-back / overscroll return
    pub friction: f64,                    // τ for FrictionSimulation (fling decay)
    pub fling_min_velocity: f32,          // gate (was hardcoded V_STOP = 13.0)
    pub settle: Tolerance,                // px-scale tolerance for scroll sims
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        Self {
            spring: SpringDescription::ios(340.0, 1.0),  // today's STIFFNESS/DAMPING_RATIO
            friction: 0.325,                              // today's TAU
            fling_min_velocity: 13.0,                     // today's V_STOP
            settle: Tolerance::SCROLL,                    // today's X_SETTLE/V_SETTLE/MAX_DURATION
        }
    }
}
```

`ScrollView` widget gains an optional `physics: ScrollPhysics` field (default = today's behavior). `ScrollViewElement` reads it instead of the module-level `const`s.

### 3.8 `ScrollViewElement` refactor — `vexo/src/elements/scroll_view.rs`

ScrollView keeps its **own ticker-registration + `advance`-in-`rebuild_from_state` loop** (it can't use `AnimationController::animate_with` because: it operates in px not 0..1; it needs mid-flight velocity handoff on edge-hit which the controller's whole-sim-replacement model doesn't support; it writes directly to the render object). But the *math* comes from the new pure-math sims:

```rust
enum ScrollDrive {
    Idle,
    Fling { sim: FrictionSimulation, start: Instant },
    Bounce { sim: SpringSimulation, start: Instant },
}

// In ScrollViewElement (replaces self.spring / self.momentum fields):
drive: ScrollDrive,
ticker_handle: Option<TickHandle>,   // one handle, whichever sim is active
physics: ScrollPhysics,
```

**`rebuild_from_state` arms** (mechanical translation of today's scroll_view.rs:601-699 — control flow preserved 1:1; only sim construction and `advance(now)→x(t)` call shape changes):

- **`Fling` arm:** `let t = now.since(start); let x = sim.x(t);` clamp to bounds; on edge-hit, capture `v = sim.dx(t)`, swap to `Bounce { sim: SpringSimulation::new(physics.spring, clamped, rest, v).with_tolerance(physics.settle), start: now }`.
- **`Bounce` arm:** `sim.x(t)` → scroll offset; `sim.is_done(t)` → snap to `rest`, unregister ticker.
- The ticker callback is a single closure sending `element_id` through the dirty channel (same as today).

---

## 4. User-Facing API

What a `shared_app`/`vexo_uikit` developer touches:

```rust
// 1. Build a spring (iOS-style damping-ratio form)
let spring = SpringDescription::ios(stiffness = 340.0, damping_ratio = 0.8); // 0.8 → slight overshoot

// 2. Build a simulation: spring from 0 → 1, starting at rest
let sim = SpringSimulation::new(spring, /*from*/ 0.0, /*to*/ 1.0, /*v0*/ 0.0);

// 3. Drive it with a controller
let mut ctrl = AnimationController::new(Duration::ZERO); // duration ignored on sim path
ctrl.set_ticker(ticker);
ctrl.set_dirty_callback(cb);
ctrl.animate_with(Box::new(sim));

// 4. Read value in render() — may overshoot [0,1] when under-damped
let t = ctrl.value();                                  // 0.0 .. ~1.05 .. 1.0
let offset = FloatTween::new(0.0, 200.0).lerp(t);      // spring a panel 200px down
// → Transform::translate(0.0, offset) etc.
```

One controller, one sim, any property via `Tween`/`CurvedAnimation`. This is the "user can opt to add their own animation with the physics effect" path from the original request.

**Velocity from a gesture** (e.g. swipe-to-dismiss carrying finger velocity into the spring):

```rust
let v0 = velocity_tracker.velocity().y;   // px/s from the existing VelocityTracker
let sim = SpringSimulation::new(spring, current_offset, target_offset, v0);
ctrl.animate_with(Box::new(sim));
```

**Composing for 2D / multi-property:** one controller drives `value` 0..1; the user maps that to multiple tweens. No need for a 2D spring (Flutter doesn't have one either — you compose). For independent per-axis physics, use two controllers.

---

## 5. Edge Cases & Decisions

1. **Overshoot past `to`.** Under-damped springs overshoot. `value()` reflects this. If a user wraps in `Tween::lerp` for a *position*, the panel overshoots its target and settles — desired. If they're driving something that mustn't overshoot (e.g. opacity), they either pick `damping_ratio = 1.0` (critical, no overshoot) or clamp in their consumer. We document this; we do **not** clamp inside the controller/sim (would defeat the point).

2. **`from == to` with `v0 == 0`.** `is_done` returns true on frame 0 (no motion). `animate_with` registers, fires dirty, next `advance` sees `is_done`, unregisters, done. One frame of work. Matches today's zero-duration controller behavior (test at controller.rs:343).

3. **NaN / degenerate params.** `stiffness = 0` (no restoring force) → spring never returns; `mass = 0` → div-by-zero. `SpringDescription::new` validates: `stiffness > 0`, `mass > 0`, `damping >= 0`, panicking with a clear message otherwise. Pre-condition validation, not silent clamping. Matches how Vexo handles other invalid widget configs.

4. **`MAX_DURATION` ceiling.** Today both sims cap at 10s (`spring.rs:26`, `momentum.rs:18`). The new sims keep this as `Tolerance.time` default (10s); `is_done` returns true past it. Safety net against a runaway sim, unchanged behavior.

5. **Frame `dt` clamping — RETIRED.** Today `spring.rs:24` clamps frame dt to 1/30s after a pause, needed for *Euler* stability. The **analytic** form is closed-form `x(t)` — no dt, no substepping, no clamp needed. The Euler `MAX_FRAME_DT` / `DT` constants are retired. (Genuine simplification the analytic form buys us; called out as a behavioral change in §6.)

6. **`Drop` cleanup.** Today `SpringSimulation`/`MomentumSimulation` implement `Drop` to unregister the ticker (spring.rs:183). The new pure-math sims hold **no ticker handle** (the controller/element owns it), so they need no `Drop`. The controller's existing ticker-unregister-on-`stop`/on-complete path already covers this.

7. **`Simulation` trait object safety.** `Send + Sync` + no generics → object-safe; `Box<dyn Simulation>` works. Verified against the trait shape in §3.1.

---

## 6. Behavioral Risks & Mitigations

### 6.1 Spring integrator change (Euler → analytic) — MAIN RISK

The existing `spring.rs` uses semi-implicit Euler with 1/120s substepping. The new `SpringSimulation` uses the analytic closed-form solution. The stateless `Simulation::x(t)` trait *requires* the analytic form (Euler is stateful `advance(dt)`).

**Risk:** the existing spring tests assert settle-time < 1s and no-overshoot-for-critical-damping. These *should* hold for the analytic form with the same params (the analytic solution is exact, Euler approximates it), but settle time may differ by a few ms.

**Mitigation:** the spec requires all existing spring tests pass unchanged, with expected-value adjustments *only* where the analytic spring's settle time legitimately differs from Euler — and only with a cited analytic justification (not a "make the test pass" tweak). If any test fails for a non-math reason, that's a regression to fix, not a test to relax. The 37 ScrollViewElement tests + 12 momentum + 11 spring tests are the regression gate.

**Math source:** Flutter's `spring_simulation.dart` (BSD-licensed, well-audited). Three-case split (under/critical/over-damped) follows the standard ODE solution for `m·x'' + c·x' + k·x = 0`.

### 6.2 ScrollView refactor

The `rebuild_from_state` arms (scroll_view.rs:601-699) are a mechanical translation: control flow (edge-hit handoff, snap-to-rest on settle) preserved 1:1; only sim construction and `advance(now)→x(t)` call shape changes. The 37 existing element tests pin the behavior.

### 6.3 `Drive` enum refactor in `AnimationController`

Folds existing `direction`/`start_time` fields into a private enum. Public accessors `direction()`/`start_time()` preserved. The existing time-path tests (controller.rs:183-397) must pass unchanged — they are the gate that the refactor is behavior-preserving.

---

## 7. File Layout

```
vexo/src/animation/
├── mod.rs              (re-exports updated: add Simulation, Tolerance,
│                       SpringDescription, SpringSimulation, FrictionSimulation;
│                       drop old SpringSimulation/MomentumSimulation)
├── simulation.rs       (NEW) Simulation trait, Tolerance, SpringDescription,
│                       SpringSimulation (analytic), FrictionSimulation
├── controller.rs       (refactored) Drive enum, animate_with(), is_animating()
├── spring.rs           (DELETED) — superseded by simulation.rs
├── momentum.rs         (DELETED) — superseded by simulation.rs
├── ticker.rs           (unchanged)
├── curve.rs            (unchanged)
└── tween.rs            (unchanged)

vexo/src/widgets/scroll_view.rs   (add ScrollPhysics + optional physics field)
vexo/src/elements/scroll_view.rs  (refactor: ScrollDrive enum, new sim types)
vexo/src/lib.rs                   (re-exports updated)
```

---

## 8. Testing Strategy

### 8.1 Unit — math (`simulation.rs`)

Port the existing 12 momentum + 11 spring tests to the new types, asserting the same physical properties:
- Decay-to-rest, no-overshoot-for-critical-damping, overshoot-when-under-damped, settle-time, velocity-decay.
- Add: under-damped overshoot magnitude, `with_damping_ratio` produces correct ζ, `FrictionSimulation` matches the closed-form `x0 + v0·τ·(1-e^(-t/τ))` (already asserted today), `Tolerance` settle boundaries.
- **Golden curve test:** sample `SpringSimulation::x(t)` at 20 points for a known `(mass, k, c)` and assert against Flutter's reference values (computed from the analytic formula, source cited). This is the "matches iOS/Flutter feel" guardrail.

### 8.2 Unit — controller (`controller.rs`)

New tests for `animate_with`:
- Completes-and-unregisters.
- Fires-dirty-on-start (avoids the render_retain deadlock — mirrors controller.rs:258).
- Cancels-prior-sim-when-called-twice.
- `forward`-cancels-sim, sim-cancels-`forward`.
- Zero-motion (`from==to`, `v0==0`) is-done-frame-0.
- Overshoots-then-settles for under-damped.
- Keep all existing time-path tests unchanged (the `Drive::Time` refactor must be invisible to them).

### 8.3 Integration — scroll (`scroll_view.rs`)

- The 37 existing element tests run unchanged. If the analytic spring's settle time differs from Euler's for the same params, adjust expected values *only* with a cited analytic justification — otherwise the test is the regression gate.
- Add one new test: custom `ScrollPhysics` (stiffer spring) produces faster settle than default — proves the config surface works.

### 8.4 Out of scope

- No new demo screen (see §1).
- No golden/image tests (no widget-test framework yet — ROADMAP §12).

---

## 9. Re-export Surface

`vexo::animation` and `vexo::*` re-export:
- `Simulation` (trait), `Tolerance`
- `SpringDescription`, `SpringSimulation`, `FrictionSimulation`
- `AnimationController` (with new `animate_with`), `AnimationDirection`, `AnimationTicker`, `TickHandle`
- `Curve`, `CurvedAnimation`, `LinearCurve`, `EaseInCurve`, `EaseOutCurve`, `EaseInOutCurve`, `CubicBezierCurve`
- `Tween`, `FloatTween`, `ColorTween`
- (Retired: old `SpringSimulation`, old `MomentumSimulation`.)

`vexo::widgets::scroll_view` re-exports `ScrollPhysics`.
