# iOS-Style Swipe-Right-to-Pop for NavigationStackView

**Date:** 2026-08-13
**Status:** Approved (pending spec review)
**Related:**
- `2026-07-07-navigation-animation-design.md` (lists swipe-back as follow-up)
- `2026-07-17-gesture-arena-design.md` (arena architecture; horizontal drag explicitly out of scope)
- `2026-08-05-physics-animation-design.md` (gesture-velocity → spring handoff pattern)
- `2026-08-11-mobile-long-press-context-menu.md` (most recent recognizer addition — template)

## Problem

`NavigationStackView` animates push/pop transitions as fire-and-forget 350ms
animations triggered by `NavigationController::push`/`pop`. There is no
gesture-driven pop. iOS users expect to swipe right from the leading screen
edge to interactively pop the top page, with the page following the finger
and a velocity-aware commit/cancel decision on release.

This is the canonical "swipe back" gesture native to `UINavigationController`
via `interactivePopGestureRecognizer`.

## Decisions (from brainstorming)

| Decision | Choice | Rationale |
|---|---|---|
| Trigger area | Leading-edge-only (≈20pt) | iOS-faithful; avoids conflict with horizontal content |
| Release behavior | Threshold + velocity | iOS-faithful; spring carries gesture velocity |
| API surface | Always-on for mobile | Matches `UINavigationController` default; no opt-in needed |

## Architecture

Five changes across two crates:

```
┌─ vexo/ (framework) ─────────────────────────────────────────────┐
│                                                                  │
│  1. EdgePanRecognizer          (new, gestures/edge_pan.rs)       │
│     • leading-edge-gated horizontal drag recognizer              │
│     • mirrors VerticalDragRecognizer's slop/accept/reject model  │
│                                                                  │
│  2. EdgePanDetector widget     (new, widgets/edge_pan_detector.rs)│
│     • wraps child, registers recognizer, fires start/update/end  │
│     • mirrors GestureDetector's element pattern                  │
│                                                                  │
│  3. AnimationController::set_value(v)  (animation/controller.rs) │
│     • sets value 0..1, stops drive, fires dirty                  │
│     • lets the finger drive progress directly                    │
│                                                                  │
│  4. event_handler.rs is_drag_winner check                        │
│     • extended to include EdgePanRecognizer                      │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─ vexo_uikit/ (app layer) ────────────────────────────────────────┐
│                                                                  │
│  5. NavigationStackView interactive pop                          │
│     • NavigationStackViewState gains InteractivePop state        │
│     • renders dual-view pop transition driven by finger/spring   │
│     • NavigationController gains begin/commit/cancel interactive │
│       pop API                                                   │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

## Data Flow (single swipe)

```
Finger Down (within 20pt of left edge)
  → EdgePanRecognizer: Pending
Finger Move (|Δx| > slop, horizontal-dominant)
  → EdgePanRecognizer: Accepted → wins arena
  → EdgePanDetectorElement::on_arena_winner_update(Down, Move)
  → fires on_start, then on_update(total_delta_x)
  → NavigationStackViewState:
      • snapshots from_path = controller.path()
      • calls controller.begin_interactive_pop() → returns from_path
      • creates InteractivePop { controller.set_value(0), phase: Dragging }
      • on each update: progress = Δx / content_width → set_value(progress)
      • feeds VelocityTracker (Instant::now(), progress)
Finger Up
  → fires on_end(final_delta_x)
  → NavigationStackViewState:
      • velocity = tracker.velocity() (progress/sec)
      • if progress > 0.5 || velocity > FLICK_THRESHOLD:
          phase = Committing
          controller.animate_with(SpringSimulation::new(
              SpringDescription::ios(340.0, 1.0), progress, 1.0, velocity))
      • else:
          phase = Cancelling
          controller.animate_with(SpringSimulation::new(
              SpringDescription::ios(340.0, 1.0), progress, 0.0, velocity))
Spring settles (detected in render: phase != Dragging && !is_animating())
  → Committing: controller.commit_interactive_pop() (pops path, no pending op)
                → clear state
  → Cancelling: controller.cancel_interactive_pop() (no path mutation)
                → clear state
```

## Component 1: `EdgePanRecognizer`

**File:** `vexo/src/gestures/edge_pan.rs`
**Mirrors:** `vexo/src/gestures/vertical_drag.rs`

A horizontal-drag recognizer gated on the initial `Down` position lying within
`EDGE_WIDTH` of the leading (left) edge.

### States

`Idle` → `Pending` (down within edge zone) → `Accepted` (slop exceeded)

### Recognition logic

- **`Down`:** if `position.x <= EDGE_WIDTH` → `Pending`, store `down_position`.
  Else → reject immediately. A non-edge drag never competes in the arena, so
  a future horizontal-scroll recognizer won't be starved by an edge-pan that
  ignores non-edge drags after winning.
- **`Move` (Pending):** compute Δx, Δy from `down_position`.
  - If `|Δx| > HORIZONTAL_DRAG_SLOP` and `|Δx| > |Δy|` → `Accepted` (wins
    arena).
  - If `|Δy| > slop` and `|Δy| > |Δx|` → reject (let vertical scroll win).
  - This is the same mutual-exclusion that lets `VerticalDragRecognizer` and
    `TapRecognizer` coexist today.
- **`Move` (Accepted):** update `last_position`, accumulate `total_delta_x`.
- **`Up` (Accepted):** recognizer stays accepted; the element handles release
  via `on_arena_winner_update(Up)`.

### Accessors

Mirrors `VerticalDragRecognizer`'s read-only accessor pattern:

```rust
pub fn down_position(&self) -> Point<Logical>
pub fn last_position(&self) -> Point<Logical>
pub fn total_delta_x(&self) -> f32
```

### Constants

Added to `vexo/src/gestures/mod.rs`:

```rust
pub(crate) const EDGE_WIDTH: f32 = 20.0;              // logical pt
pub(crate) const HORIZONTAL_DRAG_SLOP: f32 = 18.0;    // matches VERTICAL_DRAG_SLOP
```

## Component 2: `EdgePanDetector` widget

**File:** `vexo/src/widgets/edge_pan_detector.rs`
**Mirrors:** `vexo/src/widgets/gesture_detector.rs`

```rust
pub struct EdgePanDetector {
    child: Box<dyn Widget>,
    enabled: bool,
    on_start:  Option<Rc<RefCell<dyn FnMut()>>>,
    on_update: Option<Rc<RefCell<dyn FnMut(f32)>>>,   // total_delta_x
    on_end:    Option<Rc<RefCell<dyn FnMut(f32)>>>,   // final total_delta_x
}
```

### Builder API

```rust
EdgePanDetector::new(child, enabled)
    .on_start(move || { ... })
    .on_update(move |delta_x| { ... })
    .on_end(move |final_delta_x| { ... })
```

### Element behavior

- `register_gestures()` registers `EdgePanRecognizer` only when `enabled == true`.
  When disabled, the element is a pure pass-through wrapper (stable widget type
  → no reconciler remount when toggling between root/non-root).
- `on_arena_winner_update()` downcasts to `EdgePanRecognizer`, reads
  `total_delta_x()`, fires the matching callback:
  - On `Down` + first `Move` (recognizer just accepted) → `on_start`, then
    `on_update`.
  - On subsequent `Move` → `on_update`.
  - On `Up` → `on_end`.

### Why a separate widget, not extending `GestureDetector`?

`GestureDetector` is tap/press-focused. Adding pan callbacks would bloat its
API surface for a single consumer. `EdgePanDetector` is a focused, composable
widget — consistent with the "everything is a widget" philosophy. If a second
pan consumer appears later, generalize then (YAGNI).

## Component 3: `AnimationController::set_value`

**File:** `vexo/src/animation/controller.rs`

```rust
pub fn set_value(&mut self, v: f64) {
    self.unregister_from_ticker();
    self.drive = Drive::Stopped;
    self.value = v.clamp(0.0, 1.0);
    if let Some(cb) = &self.dirty_callback {
        cb();
    }
}
```

Stops any active drive, sets the value directly, fires dirty so the element
rebuilt. The clamp protects against the finger briefly overshooting the content
width.

### Interaction with existing completion detection

After `set_value`, `is_animating()` is `false` and `direction()` is `Stopped`.
The nav stack's existing "transition completed" check
(`direction() == Stopped && value() >= 1.0`) **must not** fire on a
finger-driven value — the InteractivePop state machine handles
commit/cancel/complete detection separately (see Component 5).

### Why framework-level

`set_value` is a generally-useful primitive. Any finger-driven animation —
drawer, slider, sheet — needs it. Belongs in `vexo/`.

## Component 4: `event_handler.rs` drag-winner check

**File:** `vexo/src/event_handler.rs` (currently lines 345-352)

```rust
let is_drag_winner = arena
    .winner_recognizer()
    .map(|r| {
        r.as_any()
            .downcast_ref::<crate::gestures::VerticalDragRecognizer>()
            .is_some()
            || r.as_any()
                .downcast_ref::<crate::gestures::EdgePanRecognizer>()
                .is_some()
    })
    .unwrap_or(false);
```

Without this, a winning `EdgePanRecognizer`'s `Up` event would bubble past the
element instead of being consumed as a drag release. This is the exact bug
class the existing check exists to prevent for vertical drags.

## Component 5: `NavigationController` interactive-pop API

Three new methods on the existing `NavigationController<Dest>`. They do **not**
touch the `pending` field — interactive pop bypasses the pending-op mechanism
entirely, because the path isn't mutated until commit.

```rust
impl<Dest: Hash + Eq + Clone + 'static> NavigationController<Dest> {
    /// Begin an interactive pop. Does NOT mutate the path. Returns the
    /// from_path snapshot the view should render against. Returns None if
    /// the path is empty (at root) or if a pending (non-interactive)
    /// transition is already in flight.
    pub fn begin_interactive_pop(&self) -> Option<Vec<Dest>>;

    /// Commit an interactive pop that has animated to completion. Removes
    /// the top of the path, does NOT set a pending op (the interactive
    /// animation already played the visual transition). Fires dirty so the
    /// view re-renders in steady state against the new (shorter) path.
    /// Returns the popped value, or None if the path was empty.
    pub fn commit_interactive_pop(&self) -> Option<Dest>;

    /// Cancel an interactive pop. No path mutation, no dirty fire — the
    /// view simply clears its interactive state and re-renders steady-state
    /// against the unchanged path.
    pub fn cancel_interactive_pop(&self);
}
```

### Why split begin/commit/cancel

The gesture starts before we know whether it will commit. `begin_interactive_pop`
snapshots the from_path once (the view needs this to render the outgoing
overlay). On release, the view decides commit vs. cancel based on
progress+velocity, then calls the matching method. The controller stays a dumb
state-holder; the view owns the decision logic.

### Invariant

Only one interactive pop at a time. `begin_interactive_pop` returns `None` if
`pending().is_some()` (a button-triggered push/pop is mid-flight). This prevents
the user from starting a swipe while a push animation is still settling —
matching iOS, where `interactivePopGestureRecognizer` is disabled during push.

## Component 6: `NavigationStackViewState` interactive-pop state machine

### New state field

```rust
struct InteractivePop<Dest: Hash + Eq + Clone + 'static> {
    controller: AnimationController,   // finger (set_value) or spring (animate_with)
    from_path: Vec<Dest>,              // snapshot at begin — page sliding away
    to_path: Vec<Dest>,                // path.minus_top() — destination revealed
    phase: InteractivePopPhase,
    velocity_tracker: VelocityTracker, // fed (Instant, progress) on each Move
    content_width: f32,                // for progress = delta_x / content_width
}

enum InteractivePopPhase {
    Dragging,      // finger down, controller driven by set_value
    Committing,    // released past threshold, spring → 1.0
    Cancelling,    // released before threshold, spring → 0.0
}
```

`NavigationStackViewState` also gains a `content_width: Option<f32>` field,
cached each `render()` from `MediaQuery::of(ctx).size.width`. This mirrors the
existing `ticker`/`dirty_callback` caching pattern: gesture callbacks fire
outside `render()` and need the last-known content width to compute
`progress = delta_x / content_width`. Updated on every render (cheap), read by
`on_start`/`on_update`.

### VelocityTracker reuse

The existing `VelocityTracker` (1D, field named `y`) is fed `(Instant, progress)`
— a scalar — so no new tracker is needed. `velocity()` returns progress/sec,
which is exactly the unit the `SpringSimulation`'s `v0` parameter expects.

The state's `transition` field (existing `NavTransition`) and `interactive_pop`
are mutually exclusive — the view never has both in flight.
`begin_interactive_pop` returns `None` if `pending().is_some()`, and the view
refuses to start an interactive pop if `state.transition.is_some()`.

### Lifecycle hooks

- **`on_tick`** — advances `interactive_pop.controller` (same as it does for
  `transition` today).
- **`on_rebuild`** — the existing `clear_focus()` guard checks
  `self.transition.is_none() && controller.pending().is_some()`. Extended: also
  `&& self.interactive_pop.is_none()`. Focus should already be clear by the time
  the drag starts, but the guard stays conservative.

### Render flow

`render()` gains a branch alongside the existing pending-op check:

```
if state.interactive_pop is Some(ip):
    progress = ip.controller.value()   // 0..1, finger-driven or spring-driven
    eased    = transition_curve.transform(progress)

    // Base (underneath) = destination page = to_path top.
    //   Exactly the pop case: base_fx = -0.3 * (1 - eased),
    //   base_alpha = 0.85 + 0.15 * eased
    //   (reuses base_fx_alpha(TransitionDir::Pop, Mobile, eased) verbatim)

    // Overlay (outgoing) = from_path top, driven by the SAME
    //   default_mobile_transition used for a button pop, with
    //   ctx.t = eased, is_incoming = false, direction = Pop.
    //   Produces: fx = eased (slides right 0→1), alpha = 1.0.

    // Nav bar: title from to_path top, can_pop = true.

    // Same Stack[ base IndexedStack(to_path.len) ; Positioned(overlay) ]
    //   structure as the existing transition rendering. The base IndexedStack
    //   shows the destination page with preserved state — the same
    //   state-preservation invariant the existing transition relies on.

else if controller.pending().is_some():
    ... existing transition rendering unchanged ...

else:
    ... existing steady-state rendering unchanged ...
```

### Key reuse

`base_fx_alpha()` and `default_mobile_transition()` are already pure functions
parameterized by `eased` — the interactive path calls them with the
finger-driven `eased` instead of the time-driven one. No new transform math.

### EdgePanDetector wiring

The view's `render()` output is wrapped:

```rust
let can_pop = path.len() > 0
    && state.transition.is_none()
    && state.interactive_pop.is_none()
    && effective_platform == Mobile;
let view = EdgePanDetector::new(view, can_pop)
    .on_start(clone controller, state_ctx => move || {
        // begin_interactive_pop snapshots from_path; store InteractivePop in state.
    })
    .on_update(move |delta_x| {
        // progress = (delta_x / ip.content_width).clamp(0.0, 1.0)
        // ip.controller.set_value(progress)
        // ip.velocity_tracker.add(Instant::now(), progress)
    })
    .on_end(move |final_delta_x| {
        // velocity = ip.velocity_tracker.velocity() (progress/sec)
        // if progress > 0.5 || velocity > FLICK_THRESHOLD:
        //     ip.phase = Committing
        //     ip.controller.animate_with(SpringSimulation::new(
        //         SpringDescription::ios(340.0, 1.0), progress, 1.0, velocity))
        // else:
        //     ip.phase = Cancelling
        //     ip.controller.animate_with(SpringSimulation::new(
        //         SpringDescription::ios(340.0, 1.0), progress, 0.0, velocity))
    });
```

### State-mutation mechanism

Gesture callbacks fire from the event handler, outside `render()`. The existing
`ScrollViewElement` mutates its own element state directly in
`on_arena_winner_update`. For `NavigationStackView`, the state lives in
`NavigationStackViewState` (a `ComponentState`), not on an element the gesture
callback can reach.

**Approach:** Stash a mutable handle in the element. `EdgePanDetectorElement`
owns `on_start/on_update/on_end` as `Rc<RefCell<dyn FnMut()>>` (matching
`GestureDetector`'s pattern). The `NavigationStackView` element captures a
`Rc<RefCell<Option<InteractivePop>>>` shared with its `ComponentState`, and the
callbacks mutate it through that cell. The dirty callback (already cached in
state) fires to trigger a rebuild.

This mirrors how `GestureDetector` already passes callbacks, and the shared
`Rc<RefCell<Option<...>>>` between state and element is the same pattern
`TextEditingController` uses for its shared value cell.

### Completion detection

In `render()`, when `ip.phase != Dragging && !ip.controller.is_animating()`:

- `Committing` → `controller.commit_interactive_pop()` (mutates path, fires
  dirty), `state.interactive_pop = None`.
- `Cancelling` → `controller.cancel_interactive_pop()` (no-op on path),
  `state.interactive_pop = None`.

The existing "transition completed" check (`direction() == Stopped &&
value() >= 1.0`) is **not** used for interactive pop — `set_value` leaves the
controller `Stopped`, so that check would falsely fire. The interactive branch
has its own completion check.

## Constants

| Constant | Value | Location | Source |
|---|---|---|---|
| `EDGE_WIDTH` | 20.0 logical pt | `vexo/src/gestures/mod.rs` | iOS `UIScreenEdgePanGestureRecognizer` default edge zone |
| `HORIZONTAL_DRAG_SLOP` | 18.0 pt | `vexo/src/gestures/mod.rs` | Matches existing `VERTICAL_DRAG_SLOP` |
| `FLICK_THRESHOLD` | 0.5 progress/sec | `vexo_uikit/src/navigation.rs` | "Flicked at least half the page-width in one second" — low bar that still distinguishes a deliberate flick from a slow drag-and-release below threshold |
| `SPRING_VEL_SCALE` | 1.0 | `vexo_uikit/src/navigation.rs` | Tracker reports progress/sec; spring wants progress/sec. Named constant for easy tuning |

## Edge Cases

1. **Swipe at root (empty path):** `can_pop = false` → `EdgePanDetector.enabled
   = false` → no recognizer registered → drag passes through to content. No-op,
   no jank.
2. **Swipe during a push/pop animation:** `can_pop` is false while
   `state.transition.is_some()` or `controller.pending().is_some()` → recognizer
   not registered. Matches iOS (interactive pop disabled during push).
3. **Swipe during an already-active swipe:** `can_pop` is false while
   `state.interactive_pop.is_some()` → second swipe can't start. First swipe
   must complete or cancel.
4. **Finger lifts below threshold with high leftward velocity (flick-left to
   cancel):** velocity is negative (leftward) → fails `velocity >
   FLICK_THRESHOLD` (which checks rightward) → cancels. A leftward flick
   shouldn't commit.
5. **Content width unknown at first Move:** `progress = delta_x /
   content_width`. Content width comes from `MediaQuery::of(ctx).size.width`
   (the nav stack fills the width), cached in `NavigationStackViewState` on
   each render and read by `on_start`/`on_update`. Safe because the nav stack
   is always full-width.
6. **`pop_to_root` interaction:** out of scope. `pop_to_root` stays
   button-triggered (its multi-page animation is a different visual). Interactive
   pop only ever pops one level. Documented as a non-goal.
7. **Desktop:** `can_pop` requires `platform == Mobile` (the `EdgePanDetector`
   is always present but `enabled = false` on desktop). No edge-pan on desktop —
   desktop has no stack metaphor, matching the existing desktop fade transition.
8. **Uninterrupted spring to completion:** if the spring overshoots 1.0
   (under-damped), `advance()` snaps to `target = 1.0` when `is_done()`. The
   completion check sees `!is_animating()` and commits. No visual glitch.
9. **State preservation across cancel:** cancel doesn't touch the path, so the
   `IndexedStack` keeps showing `path.len()` (the current top) with all its
   preserved state. The overlay (outgoing page) unmounts cleanly. No state loss.
10. **`on_unmount`:** if an interactive pop is in flight when the nav stack
    unmounts, `ip.controller.stop()` (unregisters from ticker) and clear state.
    Mirrors the existing `transition.controller.stop()` cleanup.

## Testing

Three layers, mirroring the existing navigation test structure:

### 1. Unit tests (in-crate, `#[cfg(test)]`)

- `EdgePanRecognizer` — down-in-zone accepts, down-outside rejects, slop
  exceeded horizontally accepts, vertical-dominant move rejects (lets scroll
  win), `total_delta_x` accumulates correctly. Pattern: `vertical_drag.rs` tests.
- `AnimationController::set_value` — sets value, stops drive,
  `is_animating() == false`, fires dirty, clamps to 0..1.
- `NavigationController::begin/commit/cancel_interactive_pop` — begin returns
  from_path and doesn't mutate path; commit pops and fires dirty; cancel
  doesn't mutate or fire; begin returns None when pending is Some or path
  empty.

### 2. Integration tests (`vexo_uikit/tests/`)

New file: `navigation_interactive_pop_tests.rs`. Uses the existing test harness
pattern from `navigation_animation_tests.rs`:

- Push a page, then simulate `begin_interactive_pop` + `set_value(0.5)` →
  assert render produces both pages (overlay + base), base shows destination.
- `set_value(0.5)` then commit spring to 1.0 → assert path shortened,
  steady-state rendering restored, no pending op.
- `set_value(0.5)` then cancel spring to 0.0 → assert path unchanged,
  steady-state restored.
- `begin_interactive_pop` while pending → returns None.
- `begin_interactive_pop` at root → returns None.

Reuse the existing mock-render / harness utilities already in
`navigation_animation_tests.rs`.

### 3. Manual GUI verification (per CLAUDE.md, user-run)

- Push a page, swipe from left edge → page follows finger, destination dimmed
  underneath.
- Release past halfway → springs to completion, path pops.
- Release before halfway → springs back, path unchanged.
- Flick quickly from edge → commits even if progress < 0.5.
- Swipe at root → no-op, root scroll still works.
- Swipe during push animation → no-op.
- State preservation: edit text on a pushed page, swipe-pop-cancel,
  swipe-pop-commit-then-repush → edits intact (cancel) / fresh page (commit).

## Scope & Non-Goals

### In scope

- `EdgePanRecognizer` + `EdgePanDetector` (new gesture type + widget)
- `AnimationController::set_value`
- `event_handler.rs` drag-winner check extension
- `NavigationController` begin/commit/cancel interactive-pop API
- `NavigationStackViewState` interactive-pop state machine + rendering
- Always-on for mobile, disabled at root / during transitions / on desktop
- Unit + integration tests

### Non-goals

- Interactive `pop_to_root` (multi-page) — button-only, different visual.
- Generic `on_pan_*` callbacks on `GestureDetector` — YAGNI; `EdgePanDetector`
  is focused. Generalize when a second consumer appears.
- Desktop edge-swipe — desktop has no stack metaphor.
- `pop_to_root` swipe (swipe from edge with two fingers, etc.) — iOS doesn't
  have this either.
- Customizable edge width / threshold via builder — constants are tuned; expose
  later if needed.

## File Inventory

| File | Change |
|---|---|
| `vexo/src/gestures/edge_pan.rs` | **New** — `EdgePanRecognizer` |
| `vexo/src/gestures/mod.rs` | Add `edge_pan` module; `EDGE_WIDTH`, `HORIZONTAL_DRAG_SLOP` constants |
| `vexo/src/widgets/edge_pan_detector.rs` | **New** — `EdgePanDetector` widget + element |
| `vexo/src/widgets/mod.rs` | Re-export `EdgePanDetector` |
| `vexo/src/event_handler.rs` | Extend `is_drag_winner` check (lines ~345-352) |
| `vexo/src/animation/controller.rs` | Add `set_value` method + unit tests |
| `vexo_uikit/src/navigation.rs` | `InteractivePop` state, render branch, `EdgePanDetector` wiring, `on_tick`/`on_rebuild`/`on_unmount` updates, `FLICK_THRESHOLD`/`SPRING_VEL_SCALE` constants |
| `vexo_uikit/tests/navigation_interactive_pop_tests.rs` | **New** — integration tests |

## Open Questions

None. All decisions resolved during brainstorming.
