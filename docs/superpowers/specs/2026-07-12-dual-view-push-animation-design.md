# Dual-View Push Animation (SwiftUI-Style) — Design Spec

**Date:** 2026-07-12
**Status:** Approved (pending implementation)
**Scope:** Add an offset animation to the underneath (pushing) view during mobile push/pop transitions, so both the pushing and pushed views translate in concert — matching SwiftUI's native `UINavigationController` push animation.

---

## 1. Background & Motivation

Vexo's `NavigationStackView` (`vexo_uikit/src/navigation.rs`) already implements a two-page push/pop transition: the overlay (moving page) slides in/out via `FractionalTranslation`, while the base (underneath page) gets only an `Opacity` fade.

In SwiftUI's native push animation (UIKit's `UINavigationController`), **both** views translate:
- The **pushed** view slides in from the right at full opacity.
- The **pushing** view slides left (~30% of page width) and dims slightly, staying visible peeking from the left edge.

Currently in Vexo, the pushing view fades to alpha 0.0 in place — no offset. The `default_mobile_transition` function (`vexo_uikit/src/transitions.rs:66-77`) already defines an outgoing offset curve (`-0.3 * t`), but that branch is dead code: the base page bypasses `transition_fn` entirely and is wrapped only in `Opacity` at `navigation.rs:642`.

**Goal:** Add a visible offset animation to the underneath view on mobile, and change its opacity from fade-to-0 to slight-dim (to 0.6), so the underneath view stays visible as it slides — matching SwiftUI.

**Non-goals:** Desktop transition changes (stays fade-only). Custom `transition_fn` integration for the base page. Gesture-driven swipe-back.

---

## 2. Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Underneath view opacity | Dim to 0.6 (not fade to 0) | SwiftUI-style: underneath view stays visible peeking from left. Dim (not full opacity) mitigates text bleed-through with transparent page backgrounds. |
| Offset magnitude | 30% of page width (`-0.3`) | Matches iOS/UINavigationController and the existing dead-code curve in `default_mobile_transition`. |
| Implementation approach | Approach A — inline offset computation | Compute `base_fx` alongside `base_alpha` in `navigation.rs`. Consistent with existing `base_alpha` pattern. Type-stable by construction. `transition_fn` continues to control only the overlay. |
| Platform scope | Mobile only | Slide metaphor is inherently mobile (iOS/UINavigationController). Desktop keeps fade-only. |
| Custom `transition_fn` integration | Base does NOT go through `transition_fn` | The base animation is an internal navigator concern (state preservation), not a user-customizable visual — same as `base_alpha` already is. Avoids type-stability risk with arbitrary custom widget trees. |

---

## 3. Visual Behavior

### 3.1 Push (mobile)

`t: 0 → 1`, `eased = transition_curve.transform(t)`

| `t` | Base (old top, underneath) | Overlay (new top, incoming) |
|---|---|---|
| 0.0 | alpha=1.0, fx=0.0 (in place) | fx=1.0 (off-screen right), alpha=1.0 |
| 0.5 | alpha=0.8, fx=-0.15 (slid 15% left) | fx=0.5 (halfway), alpha=1.0 |
| 1.0 | alpha=0.6, fx=-0.3 (slid 30% left) | fx=0.0 (in place), alpha=1.0 |

- `base_fx = -0.3 * eased` — slides left 30% of page width
- `base_alpha = 1.0 - 0.4 * eased` — dims from 1.0 to 0.6
- Overlay (unchanged): `fx = 1.0 - eased`, `alpha = 1.0`

### 3.2 Pop (mobile)

`t: 0 → 1`, `eased = transition_curve.transform(t)`

| `t` | Base (destination, underneath) | Overlay (outgoing page) |
|---|---|---|
| 0.0 | alpha=0.6, fx=-0.3 (slid left) | fx=0.0 (in place), alpha=1.0 |
| 0.5 | alpha=0.8, fx=-0.15 | fx=0.5 (sliding right), alpha=1.0 |
| 1.0 | alpha=1.0, fx=0.0 (in place) | fx=1.0 (off-screen right), alpha=1.0 |

- `base_fx = -0.3 * (1.0 - eased)` — slides back from -0.3 to 0
- `base_alpha = 0.6 + 0.4 * eased` — un-dims from 0.6 to 1.0
- Overlay: `fx = eased`, `alpha = 1.0`

### 3.3 PopToRoot

Same curves as Pop.

### 3.4 Steady state

`base_fx = 0.0`, `base_alpha = 1.0`, no overlay.

### 3.5 Desktop

Unchanged — `base_fx = 0.0` always, `base_alpha` fades 1↔0 as before (fade-only, no slide).

### 3.6 Seamless endpoints

**Push at t=1:** The overlay (new top, full opacity, in place at fx=0.0) fully covers the base (old top at alpha 0.6, slid left). When the transition ends and the base index switches to the new top at alpha 1.0, there is no visible jump — the overlay was already showing the new top at full opacity. The old top's disappearance is invisible (it was fully covered by the overlay).

**Pop at t=0:** The overlay (outgoing page, which was the old top) appears at fx=0.0, alpha=1.0, covering the full screen. The base (destination) switches underneath at alpha 0.6, fx=-0.3 — invisible because the overlay covers it. The transition from steady state (old top visible) to pop-t=0 (overlay = old top at full opacity) is seamless.

**Pop at t=1:** The overlay (outgoing) is at fx=1.0 (off-screen right). The base (destination) is at fx=0.0, alpha=1.0. Transition ends, overlay removed. No jump.

---

## 4. Code Changes

### 4.1 `vexo_uikit/src/navigation.rs` (lines 631-642)

Replace the `base_alpha`-only block with a combined `base_fx` + `base_alpha` computation, and wrap the base in `FractionalTranslation`:

```rust
// base_fx and base_alpha are both f32 (FractionalTranslation and Opacity take f32).
// `eased` is f64 (from curve.transform), so each expression is cast to f32.
let (base_fx, base_alpha): (f32, f32) = match state.transition.as_ref() {
    None => (0.0, 1.0),
    Some(t) => {
        let raw_t = t.controller.value();
        let eased = self.transition_curve.transform(raw_t);
        let platform = self.effective_platform();
        match (t.direction, platform) {
            (TransitionDir::Push, Platform::Mobile) => {
                ((-0.3 * eased) as f32, (1.0 - 0.4 * eased) as f32)
            }
            (TransitionDir::Pop | TransitionDir::PopToRoot, Platform::Mobile) => {
                ((-0.3 * (1.0 - eased)) as f32, (0.6 + 0.4 * eased) as f32)
            }
            (TransitionDir::Push, Platform::Desktop) => (0.0, (1.0 - eased) as f32),
            (TransitionDir::Pop | TransitionDir::PopToRoot, Platform::Desktop) => {
                (0.0, eased as f32)
            }
        }
    }
};
let base_widget: Box<dyn Widget> =
    Opacity::new(FractionalTranslation::new(base_stack, base_fx, 0.0), base_alpha).boxed();
```

**Key points:**
- `FractionalTranslation` is **always** present (steady + transition, all platforms). At `fx=0.0` it is a paint-time no-op (`paint_transform()` returns `None`) and layout pass-through — zero cost at steady state.
- Desktop gets `base_fx = 0.0` always, preserving its current fade-only behavior.
- The overlay code (lines 653-699) is **untouched** — it already calls `transition_fn` with the correct `is_incoming` flag.

### 4.2 `vexo_uikit/src/transitions.rs` (lines 66-77)

Update `default_mobile_transition`'s underneath-page branches to match the new dim behavior, for API consistency. These branches are not called by the navigator under Approach A (the base is computed inline), but updating them keeps the public function consistent with the new visual design:

```rust
(TransitionDir::Push, false) => (-0.3 * t, 1.0 - 0.4 * t),           // was: (-0.3 * t, 1.0 - t)
(TransitionDir::Pop, true)   => (-0.3 * (1.0 - t), 0.6 + 0.4 * t),  // was: (-0.3 * (1.0 - t), t)
(TransitionDir::PopToRoot, true) => (-0.3 * (1.0 - t), 0.6 + 0.4 * t), // was: (-0.3 * (1.0 - t), t)
```

Also update the doc comment (lines 56-65) to reflect dim-to-0.6 instead of fade-to-0.

### 4.3 No other files change

The overlay transition, `AnimationController`, `PendingOp` state machine, `IndexedStack` state preservation, and all animation primitives are untouched.

---

## 5. Type-Stability & State Preservation

### 5.1 Type-stability invariant

The base widget tree must have a constant structure across steady and transition states so the reconciler's `can_update()` (type-based) does not remount the `IndexedStack` (which would lose page state like `TextEditingController` edits).

| State | Base widget tree |
|---|---|
| Before (current) | `Opacity` → `IndexedStack` |
| After (this change) | `Opacity` → `FractionalTranslation` → `IndexedStack` |

The new structure is **always** `Opacity → FractionalTranslation → IndexedStack` — in steady state (`fx=0.0, alpha=1.0`), during push, and during pop. `FractionalTranslation` with `fx=0.0` returns `None` from `paint_transform()` and is layout pass-through, so steady-state rendering is identical to before. The reconciler sees the same widget types at every position, every frame. No remounts.

### 5.2 State preservation (unchanged)

The base remains an in-flow child of the `Stack` (not `Positioned`). The `IndexedStack` still keeps all pages mounted (wrapped in `Offstage`), preserving `ComponentState`, focus, and `TextEditingController` across transitions.

### 5.3 Custom `transition_fn` interaction

The base animation is computed inline — it does **not** go through `transition_fn`. A custom `transition_fn` still controls only the overlay (moving) page, exactly as before. This is consistent with the current design where `base_alpha` is also computed inline.

---

## 6. Testing

### 6.1 Existing tests

Run `vexo_uikit/tests/navigation_stack_tests.rs` and `navigation_animation_tests.rs` unchanged. The controller semantics, `PendingOp` snapshots, and dirty-callback tests do not depend on base offset/alpha values. Verify they still pass.

### 6.2 New unit tests

1. **`base_offset_during_push`**: Build a `NavigationStackView` with a mock/no-ticker fallback, trigger a push, and assert the base widget tree contains a `FractionalTranslation` with `fx != 0.0` at `t=0.5`. Widget tree introspection via downcasting `as_any()`.

2. **`base_offset_zero_at_steady`**: Assert that in steady state, the base `FractionalTranslation` has `fx = 0.0` and `alpha = 1.0`.

3. **`desktop_base_no_offset`**: Assert that on `Platform::Desktop`, `base_fx = 0.0` during transition (fade-only preserved).

4. **`base_dims_to_0_6_at_push_end`**: Assert that at push `t=1.0` on mobile, `base_alpha = 0.6` (not 0.0).

### 6.3 Regression test

Re-run `vexo/src/passthrough_integration.rs:399` (`test_nav_transition_text_does_not_wrap`) to confirm the pass-through layout invariant still holds with the added `FractionalTranslation` wrapper.

### 6.4 Manual verification

Ask the user to run `cargo run -p desktop_demo` (on a Retina display) and navigate between pages to visually confirm the dual-view offset animation matches SwiftUI's push/pop behavior.

---

## 7. Alternatives Considered

### Approach B: Route base through `transition_fn`

Call `transition_fn` on the base with `is_incoming: false` (Push) / `is_incoming: true` (Pop). The `default_mobile_transition` already has these branches.

- **Pro**: Fully customizable — a custom `transition_fn` controls both pages.
- **Con (critical)**: Type-stability risk. `transition_fn` returns `Opacity(FractionalTranslation(child, fx, 0.0), alpha)` during transition, but steady state has `Opacity(IndexedStack)`. The `Opacity`'s child type changes (`IndexedStack` → `FractionalTranslation`) → reconciler remounts → loses page state. Fixing this requires always wrapping in `FractionalTranslation` even at steady state, which couples steady-state rendering to the transition function's output structure — fragile with custom builders.

### Approach C: Separate `base_transition` builder

Add a new optional `base_transition` builder on `NavigationStackView`, specifically for the underneath page.

- **Pro**: Independent customization of both pages.
- **Con**: Adds API surface. Same type-stability concern as Approach B if the custom builder returns a different tree structure. Over-engineered for the current need.

**Selected: Approach A** — simplest, safest, most consistent with the existing architecture.
