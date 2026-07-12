# Dual-View Push Animation (SwiftUI-Style) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an offset animation to the underneath (pushing) view during mobile push/pop transitions, so both the pushing and pushed views translate in concert — matching SwiftUI's native `UINavigationController` push animation.

**Architecture:** Compute the base page's fractional offset (`base_fx`) and alpha (`base_alpha`) inline in `navigation.rs`, extracted into a pure testable function `base_fx_alpha()`. Always wrap the base `IndexedStack` in `Opacity(FractionalTranslation(...))` — even at steady state where `fx=0.0` is a paint-time no-op. This preserves the type-stability invariant (constant widget tree structure across steady/transition) required by the reconciler's `can_update()`. Desktop keeps fade-only (`base_fx=0.0` always). The overlay transition code is untouched.

**Tech Stack:** Rust, `vexo` (animation primitives, `FractionalTranslation`/`Opacity` widgets), `vexo_uikit` (`NavigationStackView`, `default_mobile_transition`).

**Spec:** `docs/superpowers/specs/2026-07-12-dual-view-push-animation-design.md`

## Global Constraints

- `Platform` is `#[derive(Clone, Copy, Debug, PartialEq, Eq)]` — pass by value in match arms.
- `FractionalTranslation::new(child: impl Widget + 'static, fx: f32, fy: f32) -> Self` — takes `f32` fractions.
- `FractionalTranslation::offset() -> (f32, f32)` — public accessor (for tests).
- `Opacity::new(child: impl Widget + 'static, opacity: f32) -> Self` — takes `f32` alpha.
- `Opacity::opacity_value() -> f32` — public accessor (for tests).
- `Curve::transform(&self, t: f64) -> f64` — returns `f64`; cast to `f32` when passing to widgets.
- `TransitionDir` is `#[derive(Clone, Copy, Debug, PartialEq, Eq)]` — pass by value.
- `FractionalTranslation` with `fx=0.0` returns `None` from `paint_transform()` — zero-cost at steady state (no `PushTransform`/`PopTransform` emitted).
- `FractionalTranslation` is layout pass-through — does not affect the child's laid-out size or position.
- Build command: `cargo build -p vexo_uikit`
- Test command: `cargo test -p vexo_uikit`
- Full workspace test: `cargo test`
- No comments in code unless explaining a non-obvious invariant.

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `vexo_uikit/src/navigation.rs` | `NavigationStackView` render logic. Extract `base_fx_alpha()` pure function; replace `base_alpha`-only block with `base_fx` + `base_alpha`; wrap base in `FractionalTranslation`. | Modify |
| `vexo_uikit/src/transitions.rs` | `default_mobile_transition`. Update underneath-page branches (dim-to-0.6 instead of fade-to-0) + doc comment. | Modify |
| `vexo_uikit/tests/navigation_animation_tests.rs` | Tests for `base_fx_alpha()` pure function + steady-state widget tree structure. | Modify |

No other files change. The overlay transition code (`navigation.rs:653-699`), `AnimationController`, `PendingOp` state machine, `IndexedStack` state preservation, and all animation primitives are untouched.

---

## Task 1: Add and test `base_fx_alpha()` pure function

**Files:**
- Modify: `vexo_uikit/src/navigation.rs` (add function, ~line 720 after the `NavigationStackView` impl block or near the `NavTransition` definition ~line 395)
- Test: `vexo_uikit/tests/navigation_animation_tests.rs`

**Interfaces:**
- Consumes: `TransitionDir` (from `crate::transitions`), `Platform` (from `crate::platform`)
- Produces: `fn base_fx_alpha(direction: TransitionDir, platform: Platform, eased: f64) -> (f32, f32)` — returns `(base_fx, base_alpha)`. `base_fx` is the fractional horizontal offset (negative = left). `base_alpha` is the opacity multiplier `0.0..=1.0`.

**Why extract a pure function:** Mid-transition rendering requires a wired `AnimationTicker` (set in `on_mount`), which the existing test harness does not provide (see `navigation_animation_tests.rs:5-7`). Extracting the animation math into a pure function makes it directly testable without the full pipeline.

- [ ] **Step 1: Write the failing tests**

In `vexo_uikit/tests/navigation_animation_tests.rs`, add a new test module at the end of the file (after the `CLONE SEMANTICS` section). The tests cover all direction × platform combinations at key `eased` values:

```rust
// ============================================================================
// BASE FX / ALPHA (DUAL-VIEW OFFSET ANIMATION)
// ============================================================================

mod base_fx_alpha_tests {
    use vexo_uikit::base_fx_alpha;
    use vexo_uikit::platform::Platform;
    use vexo_uikit::transitions::TransitionDir;

    #[test]
    fn push_mobile_slides_left_and_dims() {
        // t=0: in place, full opacity
        let (fx, alpha) = base_fx_alpha(TransitionDir::Push, Platform::Mobile, 0.0);
        assert!((fx - 0.0).abs() < 1e-6, "fx at t=0 must be 0, got {}", fx);
        assert!((alpha - 1.0).abs() < 1e-6, "alpha at t=0 must be 1.0, got {}", alpha);

        // t=0.5: slid 15% left, dimmed to 0.8
        let (fx, alpha) = base_fx_alpha(TransitionDir::Push, Platform::Mobile, 0.5);
        assert!((fx - (-0.15)).abs() < 1e-6, "fx at t=0.5 must be -0.15, got {}", fx);
        assert!((alpha - 0.8).abs() < 1e-6, "alpha at t=0.5 must be 0.8, got {}", alpha);

        // t=1.0: slid 30% left, dimmed to 0.6
        let (fx, alpha) = base_fx_alpha(TransitionDir::Push, Platform::Mobile, 1.0);
        assert!((fx - (-0.3)).abs() < 1e-6, "fx at t=1.0 must be -0.3, got {}", fx);
        assert!((alpha - 0.6).abs() < 1e-6, "alpha at t=1.0 must be 0.6, got {}", alpha);
    }

    #[test]
    fn pop_mobile_slides_back_and_un_dims() {
        // t=0: slid 30% left, dimmed to 0.6 (reverse of push end)
        let (fx, alpha) = base_fx_alpha(TransitionDir::Pop, Platform::Mobile, 0.0);
        assert!((fx - (-0.3)).abs() < 1e-6, "fx at t=0 must be -0.3, got {}", fx);
        assert!((alpha - 0.6).abs() < 1e-6, "alpha at t=0 must be 0.6, got {}", alpha);

        // t=1.0: in place, full opacity
        let (fx, alpha) = base_fx_alpha(TransitionDir::Pop, Platform::Mobile, 1.0);
        assert!((fx - 0.0).abs() < 1e-6, "fx at t=1.0 must be 0, got {}", fx);
        assert!((alpha - 1.0).abs() < 1e-6, "alpha at t=1.0 must be 1.0, got {}", alpha);
    }

    #[test]
    fn pop_to_root_mobile_matches_pop() {
        let pop = base_fx_alpha(TransitionDir::Pop, Platform::Mobile, 0.3);
        let pop_to_root = base_fx_alpha(TransitionDir::PopToRoot, Platform::Mobile, 0.3);
        assert!((pop.0 - pop_to_root.0).abs() < 1e-6);
        assert!((pop.1 - pop_to_root.1).abs() < 1e-6);
    }

    #[test]
    fn push_desktop_no_offset_fade_only() {
        let (fx, alpha) = base_fx_alpha(TransitionDir::Push, Platform::Desktop, 0.5);
        assert!((fx - 0.0).abs() < 1e-6, "desktop must have no offset, got {}", fx);
        assert!((alpha - 0.5).abs() < 1e-6, "desktop alpha at t=0.5 must be 0.5, got {}", alpha);
    }

    #[test]
    fn pop_desktop_no_offset_fade_only() {
        let (fx, alpha) = base_fx_alpha(TransitionDir::Pop, Platform::Desktop, 0.5);
        assert!((fx - 0.0).abs() < 1e-6, "desktop must have no offset, got {}", fx);
        assert!((alpha - 0.5).abs() < 1e-6, "desktop alpha at t=0.5 must be 0.5, got {}", alpha);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo_uikit base_fx_alpha_tests`
Expected: FAIL — `base_fx_alpha` is not exported (function doesn't exist yet). Error: `cannot find function 'base_fx_alpha' in crate 'vexo_uikit'`.

- [ ] **Step 3: Implement `base_fx_alpha()` in `navigation.rs`**

In `vexo_uikit/src/navigation.rs`, add the function after the `NavigationStackView<Dest>` impl block (after line 716, before the `impl<Dest: ...> NavigationStackView<Dest>` block at line 719, or at the end of the file). Place it as a free function (not a method):

```rust
/// Compute the base (underneath) page's fractional offset and alpha for a
/// navigation transition.
///
/// On mobile, the underneath page slides left ~30% and dims to 0.6 alpha
/// (SwiftUI-style dual-view offset animation). On desktop, it fades in place
/// (no offset — desktop has no stack metaphor).
///
/// - Push: base is the outgoing (old top) page — slides left, dims 1.0 → 0.6.
/// - Pop/PopToRoot: base is the incoming (destination) page — slides back to
///   0, un-dims 0.6 → 1.0.
///
/// Returns `(base_fx, base_alpha)`. `base_fx` is the fractional horizontal
/// offset (negative = left, resolved against page width at paint time).
/// `base_alpha` is the opacity multiplier `0.0..=1.0`.
pub fn base_fx_alpha(
    direction: TransitionDir,
    platform: Platform,
    eased: f64,
) -> (f32, f32) {
    match (direction, platform) {
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
```

Then export it from `vexo_uikit/src/lib.rs`. Check the current exports and add `base_fx_alpha`:

```rust
pub use navigation::{NavigationController, NavigationStackView, NavigationStackViewState, base_fx_alpha};
```

Also export `platform::Platform` and `transitions::TransitionDir` if not already exported (check `lib.rs` — `TransitionDir` is already re-exported via `transitions`; `Platform` may need adding). Verify by reading `vexo_uikit/src/lib.rs` and ensure the test's `use vexo_uikit::platform::Platform;` and `use vexo_uikit::transitions::TransitionDir;` resolve. If `platform` and `transitions` modules are not `pub`, make them `pub` or re-export the types.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo_uikit base_fx_alpha_tests`
Expected: PASS — all 5 tests pass.

- [ ] **Step 5: Build the full crate to verify no regressions**

Run: `cargo build -p vexo_uikit`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/navigation.rs vexo_uikit/src/lib.rs vexo_uikit/tests/navigation_animation_tests.rs
git commit -m "feat(navigation): add base_fx_alpha() pure function for dual-view offset animation

Extracts the underneath-page animation math (offset + dim) into a testable
pure function. Mobile: slides left 30% + dims to 0.6. Desktop: fade only.
Not yet wired into render() — that follows in the next task."
```

---

## Task 2: Wire `base_fx_alpha()` into `render()`

**Files:**
- Modify: `vexo_uikit/src/navigation.rs:631-642` (replace `base_alpha`-only block with `base_fx` + `base_alpha`; wrap base in `FractionalTranslation`)

**Interfaces:**
- Consumes: `base_fx_alpha()` from Task 1, `FractionalTranslation` widget from `vexo`
- Produces: Modified `render()` that always wraps the base `IndexedStack` in `Opacity(FractionalTranslation(IndexedStack, base_fx, 0.0), base_alpha)`

**Type-stability invariant (critical):** The base widget tree must be `Opacity → FractionalTranslation → IndexedStack` in ALL states (steady, push, pop, all platforms). This prevents the reconciler's `can_update()` from remounting the `IndexedStack` (which would lose page state like `TextEditingController` edits). At steady state, `base_fx=0.0` makes `FractionalTranslation` a paint-time no-op (`paint_transform()` returns `None`) — zero rendering cost.

- [ ] **Step 1: Add `FractionalTranslation` to the imports**

In `vexo_uikit/src/navigation.rs`, line 44-48, the current import block is:

```rust
use vexo::{
    AlignItems, AnimationController, Component, ComponentState, Curve, EaseInOutCurve, Flex,
    IndexedStack, LifecycleContext, Opacity, Positioned, RenderContext, SafeArea, Stack, Text,
    Widget,
};
```

Add `FractionalTranslation` to the import list (alphabetical order, after `Flex`):

```rust
use vexo::{
    AlignItems, AnimationController, Component, ComponentState, Curve, EaseInOutCurve, Flex,
    FractionalTranslation, IndexedStack, LifecycleContext, Opacity, Positioned, RenderContext,
    SafeArea, Stack, Text, Widget,
};
```

- [ ] **Step 2: Replace the `base_alpha` block with `base_fx` + `base_alpha`**

In `vexo_uikit/src/navigation.rs`, find lines 631-642. The current code is:

```rust
        let base_alpha = match state.transition.as_ref() {
            None => 1.0,
            Some(t) => {
                let raw_t = t.controller.value();
                let eased = self.transition_curve.transform(raw_t);
                match t.direction {
                    TransitionDir::Push => 1.0 - eased as f32,
                    TransitionDir::Pop | TransitionDir::PopToRoot => eased as f32,
                }
            }
        };
        let base_widget: Box<dyn Widget> = Opacity::new(base_stack, base_alpha).boxed();
```

Replace it with:

```rust
        let (base_fx, base_alpha): (f32, f32) = match state.transition.as_ref() {
            None => (0.0, 1.0),
            Some(t) => {
                let raw_t = t.controller.value();
                let eased = self.transition_curve.transform(raw_t);
                base_fx_alpha(t.direction, self.effective_platform(), eased)
            }
        };
        let base_widget: Box<dyn Widget> =
            Opacity::new(FractionalTranslation::new(base_stack, base_fx, 0.0), base_alpha)
                .boxed();
```

- [ ] **Step 3: Update the surrounding comment block**

The comment at lines 605-630 explains the old `base_alpha` behavior (fade-to-0 on push). Update it to describe the new dual-view behavior. Find the comment block starting around line 605 (`// The base is ALWAYS wrapped in an 'Opacity'...`) and update the alpha/offset rules section (lines 616-630) to:

```rust
        // The base is ALWAYS wrapped in an `Opacity(FractionalTranslation(...))`
        // (stable widget types), even in steady state. This is critical for the
        // same reason the outer `Stack` is always a `Stack`: if the base widget
        // type flipped between bare `IndexedStack` (steady) and
        // `Opacity(FractionalTranslation(IndexedStack))` (transition), the
        // reconciler's `can_update()` (type-based) would replace the subtree on
        // the swap, unmounting the page elements and losing their state (e.g.
        // TextEditingController edits).
        //
        // `Opacity` and `FractionalTranslation` are both layout pass-through and
        // preserve their child element across changes, so wrapping is safe. At
        // steady state `base_fx = 0.0` makes `FractionalTranslation` a paint-time
        // no-op (`paint_transform()` returns `None`) — zero rendering cost.
        //
        // Offset/alpha rules (SwiftUI-style dual-view animation on mobile;
        // fade-only on desktop):
        //   Push (mobile) : base (old top) slides left 30%, dims 1.0 → 0.6.
        //   Pop  (mobile) : base (destination) slides back to 0, un-dims 0.6 → 1.0.
        //   Desktop       : base_fx = 0.0 always (no slide); alpha fades as before.
        //   Steady        : base_fx = 0.0, alpha = 1.0 (no-op wrappers).
```

- [ ] **Step 4: Build to verify it compiles**

Run: `cargo build -p vexo_uikit`
Expected: PASS

- [ ] **Step 5: Run existing navigation tests to verify no regressions**

Run: `cargo test -p vexo_uikit`
Expected: All existing tests PASS. The controller semantics, `PendingOp` snapshots, and dirty-callback tests don't depend on base offset/alpha values. The `custom_transition_builder_is_invoked` test still passes (the overlay transition code is untouched).

If any test fails, it likely depends on the exact widget tree structure of the base. Inspect the failure and adjust — but the existing tests use `all_text()` which traverses via `child()`/`children()`, so adding a `FractionalTranslation` wrapper (which has a `child()`) should not break text collection.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/navigation.rs
git commit -m "feat(navigation): wire base_fx_alpha into render for dual-view push animation

The underneath (pushing) view now slides left 30% and dims to 0.6 alpha on
mobile push, matching SwiftUI's UINavigationController. Desktop stays
fade-only. The base widget tree is always Opacity(FractionalTranslation(
IndexedStack)) for type-stability across steady/transition states."
```

---

## Task 3: Add steady-state widget tree structure test

**Files:**
- Test: `vexo_uikit/tests/navigation_animation_tests.rs`

**Why this test:** Verify that the base widget tree at steady state contains `FractionalTranslation` with `offset == (0.0, 0.0)` wrapped in `Opacity` with `opacity_value == 1.0`. This confirms the type-stability invariant (the wrapper is always present) and that steady-state rendering is visually a no-op.

- [ ] **Step 1: Add module-level imports for the steady-state test**

The `base_fx_alpha_tests` module (created in Task 1) currently has these imports:

```rust
mod base_fx_alpha_tests {
    use vexo_uikit::base_fx_alpha;
    use vexo_uikit::platform::Platform;
    use vexo_uikit::transitions::TransitionDir;

    // ... base_fx_alpha tests from Task 1 ...
}
```

Add imports for the types used by the steady-state test. The module needs `render_stack` (from the parent test file), `NavigationController`/`NavigationStackView` (from `vexo_uikit`), and `Text`/`Widget`/`FractionalTranslation`/`Opacity` (from `vexo`). Update the module's import block to:

```rust
mod base_fx_alpha_tests {
    use vexo_uikit::base_fx_alpha;
    use vexo_uikit::platform::Platform;
    use vexo_uikit::transitions::TransitionDir;
    use vexo_uikit::{NavigationController, NavigationStackView};
    use vexo::{FractionalTranslation, Opacity, Text, Widget};
    use super::render_stack;

    // ... base_fx_alpha tests from Task 1 ...
}
```

Note: `render_stack` is a helper function defined at the top level of the test file (lines 49-60). Inside the nested `base_fx_alpha_tests` module, it must be imported via `use super::render_stack;`. `NavigationController` and `NavigationStackView` are also re-exported from `vexo_uikit` (line 22-23 of the test file imports them at the top level, but that doesn't propagate into nested modules).

- [ ] **Step 2: Write the test**

Add the test and `visit` helper at the end of the `base_fx_alpha_tests` module (after the Task 1 tests):

```rust
    #[test]
    fn steady_state_base_has_zero_offset_and_full_opacity() {
        let controller: NavigationController<&'static str> = NavigationController::new();
        controller.push("a");
        controller.clear_pending();

        let view = NavigationStackView::new(controller.clone(), Text::new("Root"))
            .destination(|d| Text::new(format!("Body-{}", d)).boxed());
        let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();

        let tree = render_stack(view, &mut state);

        let mut found_ft = false;
        let mut found_opacity = false;
        visit(&*tree, &mut |w: &dyn Widget| {
            if let Some(ft) = w.as_any().downcast_ref::<FractionalTranslation>() {
                let (fx, fy) = ft.offset();
                assert!((fx - 0.0).abs() < 1e-6, "steady fx must be 0, got {}", fx);
                assert!((fy - 0.0).abs() < 1e-6, "steady fy must be 0, got {}", fy);
                found_ft = true;
            }
            if let Some(op) = w.as_any().downcast_ref::<Opacity>() {
                assert!(
                    (op.opacity_value() - 1.0).abs() < 1e-6,
                    "steady alpha must be 1.0, got {}",
                    op.opacity_value()
                );
                found_opacity = true;
            }
        });

        assert!(found_ft, "FractionalTranslation must be present in steady state");
        assert!(found_opacity, "Opacity must be present in steady state");
    }

    fn visit<F: FnMut(&dyn Widget)>(w: &dyn Widget, f: &mut F) {
        f(w);
        if let Some(child) = w.child() {
            visit(child, f);
        }
        for child in w.children() {
            visit(child.as_ref(), f);
        }
    }
```

The `visit` helper traverses the widget tree using `child()` (single-child widgets like `Opacity`, `FractionalTranslation`, `SafeArea`) and `children()` (multi-child widgets like `Stack`, `Flex`, `IndexedStack`). This is the same traversal pattern as the existing `collect_text` helper (lines 62-72 of the test file).

- [ ] **Step 3: Run the test to verify it passes**

Run: `cargo test -p vexo_uikit steady_state_base_has_zero_offset_and_full_opacity`
Expected: PASS — the base is always wrapped in `Opacity(FractionalTranslation(IndexedStack, 0.0, 0.0), 1.0)` at steady state (from Task 2).

- [ ] **Step 4: Commit**

```bash
git add vexo_uikit/tests/navigation_animation_tests.rs
git commit -m "test(navigation): verify steady-state base widget tree structure

Confirms FractionalTranslation (offset 0,0) and Opacity (alpha 1.0) are
always present at steady state — the type-stability invariant required by
the reconciler's can_update()."
```

---

## Task 4: Update `default_mobile_transition` underneath branches for API consistency

**Files:**
- Modify: `vexo_uikit/src/transitions.rs:56-77` (update underneath branches + doc comment)
- Test: `vexo_uikit/src/transitions.rs` (update existing tests or add new ones in the `#[cfg(test)] mod tests` block)

**Why:** Under Approach A, the base animation is computed inline via `base_fx_alpha()` — it does NOT go through `default_mobile_transition`. However, the underneath branches of `default_mobile_transition` (`is_incoming: false` for Push, `is_incoming: true` for Pop/PopToRoot) still exist as dead code. They should be updated to match the new dim-to-0.6 behavior so the public API is consistent if a caller ever uses it directly.

- [ ] **Step 1: Write the failing test**

In `vexo_uikit/src/transitions.rs`, in the `#[cfg(test)] mod tests` block, add a test that checks the underneath branches produce dim-to-0.6 (not fade-to-0):

```rust
    #[test]
    fn push_outgoing_dims_to_0_6_not_zero() {
        let ctx = TransitionCtx {
            t: 1.0,
            is_incoming: false,
            direction: TransitionDir::Push,
            platform: Platform::Mobile,
        };
        let child = Text::new("Page").boxed();
        let result = default_mobile_transition(&ctx, child);

        let opacity = result
            .as_any()
            .downcast_ref::<vexo::Opacity>()
            .expect("top-level wrapper must be Opacity");
        assert!(
            (opacity.opacity_value() - 0.6).abs() < 1e-6,
            "push outgoing at t=1 must dim to 0.6, got {}",
            opacity.opacity_value()
        );
    }

    #[test]
    fn pop_incoming_un_dims_from_0_6_to_1() {
        let ctx = TransitionCtx {
            t: 1.0,
            is_incoming: true,
            direction: TransitionDir::Pop,
            platform: Platform::Mobile,
        };
        let child = Text::new("Page").boxed();
        let result = default_mobile_transition(&ctx, child);

        let opacity = result
            .as_any()
            .downcast_ref::<vexo::Opacity>()
            .expect("top-level wrapper must be Opacity");
        assert!(
            (opacity.opacity_value() - 1.0).abs() < 1e-6,
            "pop incoming at t=1 must be 1.0 (un-dimmed), got {}",
            opacity.opacity_value()
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo_uikit transitions::tests`
Expected: FAIL — the current underneath branches produce alpha `1.0 - t = 0.0` (Push outgoing) and `t = 1.0` (Pop incoming, this one passes). The `push_outgoing_dims_to_0_6_not_zero` test fails because it gets 0.0 instead of 0.6.

- [ ] **Step 3: Update `default_mobile_transition` underneath branches**

In `vexo_uikit/src/transitions.rs`, find the `default_mobile_transition` function (line 66). Replace the match arms (lines 68-75):

Current:
```rust
    let (fx, alpha) = match (ctx.direction, ctx.is_incoming) {
        (TransitionDir::Push, true) => (1.0 - t, 1.0),
        (TransitionDir::Push, false) => (-0.3 * t, 1.0 - t),
        (TransitionDir::Pop, true) => (-0.3 * (1.0 - t), t),
        (TransitionDir::Pop, false) => (t, 1.0),
        (TransitionDir::PopToRoot, true) => (-0.3 * (1.0 - t), t),
        (TransitionDir::PopToRoot, false) => (t, 1.0),
    };
```

Replace with:
```rust
    let (fx, alpha) = match (ctx.direction, ctx.is_incoming) {
        (TransitionDir::Push, true) => (1.0 - t, 1.0),
        (TransitionDir::Push, false) => (-0.3 * t, 1.0 - 0.4 * t),
        (TransitionDir::Pop, true) => (-0.3 * (1.0 - t), 0.6 + 0.4 * t),
        (TransitionDir::Pop, false) => (t, 1.0),
        (TransitionDir::PopToRoot, true) => (-0.3 * (1.0 - t), 0.6 + 0.4 * t),
        (TransitionDir::PopToRoot, false) => (t, 1.0),
    };
```

- [ ] **Step 4: Update the doc comment**

In `vexo_uikit/src/transitions.rs`, update the doc comment above `default_mobile_transition` (lines 48-65). Replace lines 56-65:

Current:
```rust
/// - **Push, incoming**: slides in from the right (fraction `1.0 → 0.0`), full opacity.
/// - **Push, outgoing**: slides slightly left (fraction `0.0 → -0.3`), fades to 0.
/// - **Pop, incoming**: reverse of Push.outgoing (slides back to 0, un-fades).
/// - **Pop, outgoing**: reverse of Push.incoming (slides out to the right).
///
/// The outgoing page's alpha reaches 0.0 at `t=1` so the transition's hard
/// switch to steady-state rendering (which drops the outgoing page in a single
/// frame) produces no visible jump. If the outgoing page only dimmed to a
/// non-zero alpha, that hard cut would be perceived as a fade-out happening
/// *after* the offset animation — a sequential artifact.
```

Replace with:
```rust
/// - **Push, incoming**: slides in from the right (fraction `1.0 → 0.0`), full opacity.
/// - **Push, outgoing**: slides slightly left (fraction `0.0 → -0.3`), dims to 0.6 alpha.
/// - **Pop, incoming**: slides back to 0 (fraction `-0.3 → 0.0`), un-dims 0.6 → 1.0.
/// - **Pop, outgoing**: slides out to the right (fraction `0.0 → 1.0`), full opacity.
///
/// The underneath page dims to 0.6 (not 0.0) so it stays visible peeking from
/// the left edge during the transition — matching SwiftUI's `UINavigationController`
/// dual-view animation. The dimming mitigates text bleed-through when page
/// backgrounds are transparent.
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vexo_uikit transitions::tests`
Expected: PASS — all tests including the two new ones pass.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/transitions.rs
git commit -m "refactor(transitions): update default_mobile_transition underneath branches to dim-to-0.6

Aligns the public transition builder's outgoing/incoming branches with the
new dual-view animation (dim to 0.6, not fade to 0). These branches are not
called by the navigator (which uses base_fx_alpha inline), but kept
consistent for direct API use."
```

---

## Task 5: Full verification and manual testing prompt

**Files:** None modified

- [ ] **Step 1: Run the full workspace test suite**

Run: `cargo test`
Expected: ALL tests pass — `vexo`, `vexo_uikit`, `shared_app`, `desktop_demo`.

- [ ] **Step 2: Run the regression test specifically**

Run: `cargo test -p vexo test_nav_transition_text_does_not_wrap`
Expected: PASS — confirms the pass-through layout invariant still holds with the added `FractionalTranslation` wrapper around the base `IndexedStack`.

- [ ] **Step 3: Build in release mode to catch any issues**

Run: `cargo build --release`
Expected: PASS

- [ ] **Step 4: Prompt the user to run the desktop demo for visual verification**

The desktop demo runs on `Platform::Desktop` by default, which means `base_fx = 0.0` always (fade-only). To visually verify the mobile animation, either:
- Run on iOS simulator (`./build_for_ios.sh` + Xcode), OR
- Temporarily check if the demo has a platform override.

Ask the user:

> The dual-view push animation is implemented. To visually verify:
> - On desktop (fade-only): `cargo run -p desktop_demo` — push/pop should show fade transitions as before (no slide on the underneath view).
> - On iOS (SwiftUI-style slide): build and run via `./build_for_ios.sh` + Xcode — push should slide the new view in from the right while the old view slides left ~30% and dims. Pop should reverse.
>
> Please run the iOS build and confirm the animation matches SwiftUI's behavior.

- [ ] **Step 5: Final commit (if any cleanup needed)**

If the user reports issues, fix them and commit. If everything looks good, no commit needed — the implementation is complete.
