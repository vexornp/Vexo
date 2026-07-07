# Navigation Animation Support — Design Spec

**Date:** 2026-07-07
**Status:** Approved (pending implementation)
**Scope:** Add Flutter-style navigation animation to `NavigationStackView`, building the necessary animation foundation (Curves, transition widgets) along the way.

---

## 1. Background & Motivation

Vexo's `NavigationStackView` (`vexo_uikit/src/navigation.rs`) currently hard-switches pages via `IndexedStack` + `Offstage`. Push/pop is instant — no visual transition. The framework already has `AnimationController`, `AnimationTicker`, and `Tween` primitives in `vexo/src/animation/`, but no `Curve`/easing, no animated widget abstractions, and no production usages of `AnimationController`.

Goal: add smooth page transitions to navigation, modeled on Flutter's `Navigator`/`PageRoute` mechanics, while building the reusable animation foundation that future features (implicit animations, hero transitions, etc.) can build on.

---

## 2. Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Foundation scope | Full foundation first (Curve trait + impls, CurvedAnimation, transition widgets) | Matches Flutter's architecture; reusable for future animated widgets |
| Transition style selection | Builder + platform defaults | Caller can customize per-`NavigationStackView`; sensible mobile/desktop defaults out of the box |
| AnimatedBuilder concept | Option 2a — no separate `AnimatedBuilder` widget; transitions are `Component`s that read `controller.value()` in `render()` | Avoids making `AnimationController` `Clone`/shared; matches how Flutter transitions actually work |
| Transition layer approach | Approach A — dual-mount transition overlay; `IndexedStack` for steady state | Preserves `IndexedStack`'s state preservation; matches Flutter's `Overlay`-based dual-mount semantics |
| Layout-aware slide offsets | Read `computed_bounds().size.width` from prior frame's render object; cache in state; pass via `TransitionCtx.page_width` | No magic constants; no clipping hacks; uses real layout |
| GlobalKey reparenting | **Deferred (Path B for v1).** Accept remount of outgoing page during transition. Path A (true reparenting) tracked as TODO in `ROADMAP.md`. | Path A requires reconciler changes touching the hottest framework path; defer to avoid scope explosion. Steady-state `IndexedStack` still preserves state correctly. |

---

## 3. Component Design

### 3.1 Curves — `vexo/src/animation/curve.rs` (new)

```rust
pub trait Curve: Send + Sync {
    fn transform(&self, t: f64) -> f64;
}
```

Initial implementations:
- `LinearCurve` — `t` (identity)
- `EaseInCurve` — `t * t`
- `EaseOutCurve` — `1 - (1 - t) * (1 - t)`
- `EaseInOutCurve` — piecewise: `2*t*t` for `t < 0.5`, `1 - (-2*t + 2)^2 / 2` for `t >= 0.5`

`CurvedAnimation<'a>` wraps `&'a AnimationController` + `Box<dyn Curve>`, exposing `value()` that applies the curve. No separate dirty callback — piggybacks on the controller's.

**Re-exports:** `Curve`, `LinearCurve`, `EaseInCurve`, `EaseOutCurve`, `EaseInOutCurve`, `CurvedAnimation` from `vexo::animation` and `vexo::*`.

### 3.2 Transition widgets — `vexo/src/widgets/transitions.rs` (new)

`SlideTransition` and `FadeTransition` are `Component`s whose `State` owns an `AnimationController`. Self-contained — the caller does not pass in a controller.

```rust
pub struct SlideTransition {
    direction: SlideDirection,   // Horizontal | Vertical
    begin: f32,                  // offset at t=0 (logical px)
    end: f32,                    // offset at t=1 (logical px)
    curve: Box<dyn Curve>,
    duration: Duration,
    child: Box<dyn Widget>,
}
```

Lifecycle:
- `State::on_mount`: `controller.set_ticker(ctx.animation_ticker().clone())` + `controller.set_dirty_callback(ctx.dirty_callback())` + `controller.forward()`.
- `State::on_tick`: `controller.advance(now)`.
- `State::render`: read `controller.value()`, apply `curve.transform(t)`, lerp `begin → end`, wrap child in `Transform::translate`.

`FadeTransition` is the same shape but wraps child in `Opacity::new(child, alpha)`.

**Curve default:** `EaseInOutCurve`. Override via `.curve(impl Curve)`.
**Duration default:** 300ms.

**Ownership:** Each transition owns its controller. Trade-off: callers can't coordinate one controller across multiple transitions. Acceptable for navigation's push/pop model where each transition is independent.

**`CompositeTransition` deferred.** Callers stack two wrapper Components for now (YAGNI for v1).

### 3.3 Navigation integration — `vexo_uikit/src/navigation.rs` (modified)

#### 3.3.1 Two-phase push/pop via `pending` op

`NavigationController` gains a `pending: Rc<RefCell<Option<PendingOp>>>` field. On `push(dest)`:
1. Snapshot `from = self.path.clone()`.
2. Build `to = from + [dest]`.
3. Store `pending = Some(PendingOp { from, to, kind: Push })` *without* mutating `path`.
4. Fire dirty callback.

The view's state observes `pending` in render, clones it into `state.transition`, calls `controller.clear_pending()`, and starts `state.transition.controller.forward()`. The actual path mutation is deferred until the transition completes.

`PendingOp`:
```rust
struct PendingOp<Dest> {
    from: Vec<Dest>,
    to: Vec<Dest>,
    kind: TransitionDir,  // Push | Pop | PopToRoot
}
```

#### 3.3.2 `NavigationStackViewState` transition state

```rust
pub struct NavigationStackViewState<Dest> {
    _marker: PhantomData<Dest>,
    transition: Option<NavTransition<Dest>>,
}

struct NavTransition<Dest> {
    direction: TransitionDir,
    controller: AnimationController,
    from_path: Vec<Dest>,
    to_path: Vec<Dest>,
    page_width: f32,   // cached from render object bounds
}
```

#### 3.3.3 Three render paths

```rust
fn render(...) -> Box<dyn Widget> {
    if let Some(t) = &state.transition {
        // TRANSITION: render a Stack with both pages, animated
        let eased = t.curve.transform(t.controller.value());
        let outgoing = build_page(t.from_path.last());
        let incoming = build_page(t.to_path.last());
        return Stack::new()
            .push((transition_builder)(&TransitionCtx { t: eased, is_incoming: false, ... }, outgoing))
            .push((transition_builder)(&TransitionCtx { t: eased, is_incoming: true, ... }, incoming))
            .boxed();
    }
    // STEADY: existing IndexedStack path with current path
    IndexedStack::new(path.len())...
}
```

#### 3.3.4 Transition completion

`state.on_tick(now)` advances `state.transition.as_mut().unwrap().controller.advance(now)`. When `controller.direction() == Stopped`:
1. Apply `pending.to` to `controller.path` (the deferred mutation).
2. Set `state.transition = None`.
3. Next `render()` runs the steady-state `IndexedStack` path.

#### 3.3.5 `page_width` cache

During transition, the state looks up the nav content area's render object (via the `RenderObjectKey` stored on the `StatefulElement`'s `ProxyRenderObject`, or by tagging the content `Stack`/`IndexedStack` with a `Local` `Key` and querying the registry), reads `computed_bounds().size.width`, and caches it in `NavTransition.page_width`. Used in the transition builder via `TransitionCtx.page_width`.

Note: this is a read-only registry lookup, not `GlobalKey`-based reparenting. The reparenting deferral in §5 does not block `page_width` lookup.

On the first frame of a transition (`computed_bounds() == None`), use a sentinel default (e.g., 375.0 logical px). The controller starts at `t ≈ 0` so the offset is `≈ full_width` regardless; one-frame imprecision is invisible.

### 3.4 Default transitions — `vexo_uikit/src/transitions.rs` (new)

```rust
pub struct TransitionCtx {
    pub t: f64,                  // eased 0..1
    pub is_incoming: bool,       // true = new page, false = outgoing
    pub direction: TransitionDir,  // Push | Pop | PopToRoot
    pub platform: Platform,
    pub page_width: f32,         // layout-derived width
}

pub enum TransitionDir { Push, Pop, PopToRoot }

pub fn default_mobile_transition(ctx: &TransitionCtx, child: Box<dyn Widget>) -> Box<dyn Widget>
pub fn default_desktop_transition(ctx: &TransitionCtx, child: Box<dyn Widget>) -> Box<dyn Widget>
```

**Mobile (iOS-style horizontal slide):**
- Push, incoming: slide from right (`x: page_width → 0`), opacity 1.0
- Push, outgoing: slide slightly left (`x: 0 → -page_width * 0.3`), opacity `1.0 → 0.7`
- Pop, incoming: reverse of Push.outgoing
- Pop, outgoing: reverse of Push.incoming

**Desktop (fade):**
- Incoming: opacity `0 → 1`
- Outgoing: opacity `1 → 0`
- No slide (desktop windows don't have the physical stack metaphor)

### 3.5 `NavigationStackView` API additions

```rust
impl<Dest> NavigationStackView<Dest> {
    pub fn transition<F: Fn(&TransitionCtx, Box<dyn Widget>) -> Box<dyn Widget> + 'static>(
        mut self, f: F
    ) -> Self;

    pub fn transition_duration(mut self, duration: Duration) -> Self;
    pub fn transition_curve(mut self, curve: Box<dyn Curve>) -> Self;
}
```

When `transition` is `None`, the view uses `default_mobile_transition` on `Platform::Mobile`, `default_desktop_transition` otherwise.

---

## 4. Lifecycle & Edge Cases

| Case | Behavior |
|---|---|
| Push while push-transition active (same direction) | Coalesce: replace `to` with new dest, restart controller at current `value` (don't reset to 0). |
| Pop while push-transition active | `controller.reverse()`, swap `from`/`to`. |
| `pop_to_root` while transitioning | Cancel current transition, start fresh pop to empty path. |
| Rapid push+pop (cancel) | Push's `to` never commits; pop discards `pending`; path unchanged. No visible transition if both happen before a frame renders. |
| Zero-duration transition | Controller completes on first `advance`; render runs transition path once at `t=1.0`, then steady-state next frame. Useful for tests. |
| Unmount during transition | `on_unmount` calls `controller.stop()` to unregister from ticker. Pending op dropped without committing. |
| Page widget rebuilds mid-transition | Transition `Stack` rebuilt from `from_path`/`to_path` snapshots. Page's own `ComponentState` updates flow through normally — but the page is remounted (not reparented) when entering the transition overlay, so its state is freshly initialized. **Known limitation — see §6.** |

**Default duration:** 300ms (mobile), 200ms (desktop). Configurable via `transition_duration`.
**Default curve:** `EaseInOutCurve`. Configurable via `transition_curve`.

---

## 5. The Outgoing Page Remount Problem (Path B limitation)

### The problem

During a push transition, the outgoing page moves from `IndexedStack`-mounted to transition-`Stack`-mounted. Reconciliation sees a widget-type change at that slot (different parent element types) and **remounts** the page element. Its `ComponentState` is freshly initialized — a `TextEdit` on the outgoing page loses its cursor position mid-transition.

### Why not fixed in v1

The correct Flutter-style fix is `GlobalKey`-based reparenting: tag each page with a deterministic `GlobalKey` derived from its `Dest`, and have the reconciler detect "this GlobalKey's element is being moved between parents" and reparent instead of remount. **Vexo's reconciler does not currently implement reparenting** — `GlobalKey` is registered on mount, unregistered on unmount, with no move detection in `reconcile_element`.

Implementing reparenting touches the hottest path in the framework (`Reconciler::reconcile_element`) and requires careful test coverage: reparent across siblings, reparent across tree depth, reparent with focus, reparent with active animations, reparent with unmount of old parent. Scope: ~200-400 LOC of reconciler changes plus tests.

### v1 acceptance

Ship without reparenting. The 300ms transition's state loss is acceptable for most pages (display-only). For pages with `TextEdit` state, the cursor resets mid-pop. Documented as a known limitation. Steady-state `IndexedStack` still preserves state correctly across completed push/pop cycles.

### Path A (deferred — TODO in `ROADMAP.md`)

1. Add `GlobalKey::from_hashable<H: Hash + Eq + 'static>(value: &H) -> GlobalKey` — deterministic key constructor.
2. Reconciler change: in `reconcile_element`, before inflating a new element for a widget with a `GlobalKey`, check `GlobalKeyRegistry::get_element(&key)`. If it returns `Some(existing)` and `existing.can_update(new_widget)`, move the existing element to the new parent slot instead of mount+unmount:
   - Detach from old parent's children list.
   - Attach to new parent's children list.
   - Update the element's parent pointer in `ElementRegistry`.
   - Reparent the focus node (existing `FocusAttachment::reparent_to` supports this).
   - Reparent the render object under the new parent's render object.

---

## 6. Testing Strategy

### 6.1 Foundation unit tests (no GPU)

**`vexo/src/animation/curve.rs`:**
- Each curve: `transform(0.0) == 0.0`, `transform(1.0) == 1.0`, monotonicity, midpoint value matches expected formula.
- `CurvedAnimation::value()` applies curve to parent's raw value.

**`vexo/src/widgets/transitions.rs`:**
- `SlideTransition` / `FadeTransition` States: `on_mount` wires controller + ticker, calls `forward()`. `on_tick` advances controller.
- `render` at `t=0` produces child wrapped in `Transform`/`Opacity` with expected begin values; at `t=1.0` produces end values.
- Use `MockBackend` (`vexo/src/render/mock_backend.rs`) to inspect emitted `RenderCommand`s.

### 6.2 Navigation transition tests

**`vexo_uikit/tests/navigation_animation_tests.rs` (new):**
- Push triggers transition state with `Push` direction, controller `forward()`.
- Pop during push-transition reverses controller.
- Transition completion commits `to_path` to `controller.path()`.
- Rapid push+pop (cancel) leaves path unchanged.
- `pop_to_root` mid-transition starts fresh pop to empty path.
- Default mobile transition produces slide+offset render commands; default desktop produces opacity-only.
- `page_width` is read from cached render object bounds.

### 6.3 Integration test (manual)

Add a sample to `shared_app` (or a new demo route) that pushes/pops pages so the user can visually verify the animation. **The assistant will not run `cargo run -p desktop_demo`** — per `CLAUDE.md`, ask the user to run it.

### 6.4 Out of scope

- Gesture-driven swipe-back (no gesture infrastructure tied to transitions yet).
- Deep linking / URL routing.
- Hero animations.

---

## 7. File Changes Summary

### New files

| Path | Purpose |
|---|---|
| `vexo/src/animation/curve.rs` | `Curve` trait, `LinearCurve`, `EaseInCurve`, `EaseOutCurve`, `EaseInOutCurve`, `CurvedAnimation` |
| `vexo/src/widgets/transitions.rs` | `SlideTransition`, `FadeTransition` (`Component`s) |
| `vexo_uikit/src/transitions.rs` | `TransitionCtx`, `TransitionDir`, `default_mobile_transition`, `default_desktop_transition` |
| `vexo_uikit/tests/navigation_animation_tests.rs` | Navigation transition tests |

### Modified files

| Path | Change |
|---|---|
| `vexo/src/animation/mod.rs` | Re-export curve types |
| `vexo/src/lib.rs` | Re-export `Curve`, `CurvedAnimation`, etc. |
| `vexo/src/widgets/mod.rs` | `mod transitions;` + re-exports |
| `vexo_uikit/src/navigation.rs` | `transition` field + builder, two-phase push/pop with `pending`, `NavigationStackViewState.transition` + `on_tick` + 3-path render, `page_width` cache |
| `vexo_uikit/src/lib.rs` | Re-export `transitions` module |
| `ROADMAP.md` | TODO entry for Path A (GlobalKey reparenting) |
| `shared_app/src/lib.rs` | Optional: demo pushing pages with visible transition |

### No changes to

- `vexo/src/animation/controller.rs` — existing `AnimationController` is sufficient.
- `vexo/src/animation/ticker.rs` — existing ticker is sufficient.
- `vexo/src/animation/tween.rs` — `FloatTween`/`ColorTween` already exist.
- `vexo/src/widgets/opacity.rs`, `vexo/src/widgets/transform.rs` — reused as-is.
- `vexo/src/widgets/indexed_stack.rs` — steady-state path unchanged.
- `vexo/src/stateful_widget.rs` — `on_tick` + `animation_ticker()` already exist.
- `vexo/src/reconciler.rs` — no GlobalKey reparenting in Path B.

---

## 8. Build & Verification

- `cargo build -p vexo` — verify curve + transition compiles standalone.
- `cargo build -p vexo_uikit` — verify navigation integration compiles.
- `cargo test -p vexo` — curve + transition unit tests.
- `cargo test -p vexo_uikit` — navigation animation tests.
- `cargo build -p desktop_demo` — verify shared_app still compiles.
- Ask user to run `cargo run -p desktop_demo` for visual verification.

---

## 9. Deferred Items (recorded in `ROADMAP.md`)

1. **Path A: GlobalKey reparenting.** Required for state-preserving page transitions. Without it, outgoing pages remount in the transition overlay, losing `ComponentState` for the 300ms duration.
2. **Gesture-driven swipe-back.** Needs gesture infrastructure tied to transition progress.
3. **`CompositeTransition`.** Callers stack wrapper Components for now.
4. **Queued multi-push transitions.** Rapid pushes coalesce to the latest; intermediate destinations are skipped.
