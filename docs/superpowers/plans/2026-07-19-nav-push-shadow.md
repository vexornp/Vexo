# Navigation Push Shadow (iOS-Native) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an iOS-native drop shadow to the moving page during mobile push/pop transitions — a soft strip on the moving page's leading edge, cast onto the underneath page. Desktop transitions are unchanged.

**Architecture:** Attach a full-perimeter `BoxShadow` (`Color::BLACK.with_alpha(0.3)`, blur `12.0`, zero offset/spread) to the moving page inside `default_mobile_transition` (innermost: `Opacity(FractionalTranslation(DecoratedContainer(page, shadow)))`). Wrap the nav content `Stack` in a `DecoratedContainer(clip=true)` (always present, all platforms) so the full-perimeter shadow is clipped to the nav content area — only the leading-edge strip is visible. Constants live in `tokens::navigation`. Mobile-only shadow; unconditional clip.

**Tech Stack:** Rust, `vexo` (`BoxShadow`, `Color`, `DecoratedContainer`, `FractionalTranslation`, `Opacity`), `vexo_uikit` (`NavigationStackView`, `default_mobile_transition`, `tokens::navigation`).

**Spec:** `docs/superpowers/specs/2026-07-19-nav-push-shadow-design.md`

## Global Constraints

- `Color::BLACK` is `pub const`, `Color::with_alpha(a: f32) -> Color` is `pub const` — `Color::BLACK.with_alpha(0.3)` is const-evaluable.
- `BoxShadow::new(color: Color) -> Self` — takes color by value.
- `BoxShadow::blur(self, radius: f32) -> Self` — builder.
- `BoxShadow.offset`/`.spread_radius` default to `Point::zero()` / `0.0` (per `vexo/src/style.rs:27`).
- `DecoratedContainer::new(child: impl Widget + 'static) -> Self`.
- `DecoratedContainer::clip(self) -> Self` — sets `style.clip = true` (no argument; `vexo/src/widgets/decorated_container.rs:367`).
- `DecoratedContainer::shadow(self, shadow: BoxShadow) -> Self` — appends to `style.shadows`.
- `DecoratedContainer::style_ref() -> &Style` — for test introspection.
- `Style.clip: bool`, `Style.shadows: Vec<BoxShadow>` — public fields.
- Shadow `Rect`s respect **ancestor** clips (`vexo/src/frame_builder.rs:271`) but bypass their **own** container's clip (`vexo/src/render_objects/container.rs:171-172`). This is what makes the ancestor-clip approach work.
- `tokens::navigation` is `pub mod` in `vexo_uikit/src/theme/tokens.rs:62`.
- Build command: `cargo build -p vexo_uikit`
- Test command: `cargo test -p vexo_uikit`
- Full workspace test: `cargo test`
- No comments in code unless explaining a non-obvious invariant.

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `vexo_uikit/src/theme/tokens.rs` | Add `PAGE_SHADOW_ALPHA` and `PAGE_SHADOW_BLUR` constants in `pub mod navigation`. | Modify |
| `vexo_uikit/src/transitions.rs` | Modify `default_mobile_transition` to wrap `child` in `DecoratedContainer(child).shadow(...)` before `FractionalTranslation`/`Opacity`. Add imports. Update doc comment. | Modify |
| `vexo_uikit/src/navigation.rs` | Wrap `content_stack` in `DecoratedContainer::new(stack).clip()` before `SafeArea`. Add `DecoratedContainer` import. | Modify |
| `vexo_uikit/tests/navigation_animation_tests.rs` | Add 4 widget-tree-downcasting tests for shadow presence/absence and clip wrapper. | Modify |

No other files change. `default_desktop_transition`, `AnimationController`, `PendingOp` state machine, `IndexedStack` state preservation, `base_fx_alpha`, the shader, the frame builder, and the render command structures are all untouched.

---

## Task 1: Add shadow constants to `tokens::navigation`

**Files:**
- Modify: `vexo_uikit/src/theme/tokens.rs`

**Interfaces:**
- Produces: `pub const PAGE_SHADOW_ALPHA: f32 = 0.3;` and `pub const PAGE_SHADOW_BLUR: f32 = 12.0;` in `pub mod navigation`.

- [ ] **Step 1: Add the constants**

In `vexo_uikit/src/theme/tokens.rs`, find `pub mod navigation {` (line 62). At the end of the module (after `pub const MOBILE_TITLE_FONT_SIZE: f32 = 17.0;` at line 130, before the closing `}` at line 131), add:

```rust
    /// Drop shadow cast by the moving page during mobile push/pop transitions.
    ///
    /// Full-perimeter `BoxShadow` clipped to the nav content area by the
    /// ancestor clip wrapper in `NavigationStackView::render`, so only the
    /// leading-edge strip is visible. Matches iOS native push animation.
    ///
    /// Constructed as `Color::BLACK.with_alpha(PAGE_SHADOW_ALPHA)` with
    /// `.blur(PAGE_SHADOW_BLUR)`; zero offset, zero spread (the ancestor clip
    /// does the edge restriction, not the offset).
    pub const PAGE_SHADOW_ALPHA: f32 = 0.3;
    pub const PAGE_SHADOW_BLUR: f32 = 12.0;
```

- [ ] **Step 2: Build to verify it compiles**

Run: `cargo build -p vexo_uikit`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add vexo_uikit/src/theme/tokens.rs
git commit -m "feat(tokens): add PAGE_SHADOW_ALPHA and PAGE_SHADOW_BLUR nav constants

Visual tokens for the moving-page drop shadow during mobile push/pop
transitions. Not yet consumed — wiring follows in subsequent tasks."
```

---

## Task 2: Attach shadow in `default_mobile_transition`

**Files:**
- Modify: `vexo_uikit/src/transitions.rs`

**Interfaces:**
- Consumes: `BoxShadow`, `Color`, `DecoratedContainer` from `vexo`; `tokens::navigation::PAGE_SHADOW_ALPHA`, `PAGE_SHADOW_BLUR` from `crate::theme::tokens`.
- Produces: modified `default_mobile_transition` returning `Opacity(FractionalTranslation(DecoratedContainer(child, shadow), fx, 0.0), alpha)`.

**Type-stability note:** The overlay tree becomes `Opacity → FractionalTranslation → DecoratedContainer → page` on every frame of a transition. The overlay only exists during a transition (it's a conditionally-pushed `Positioned` sibling), so there's no steady/transition structural mismatch. Within a transition, structure is constant — no remounts.

- [ ] **Step 1: Update imports**

In `vexo_uikit/src/transitions.rs`, the current import block (line 18) is:

```rust
use vexo::{FractionalTranslation, Opacity, Widget};
```

Replace with:

```rust
use vexo::{BoxShadow, Color, DecoratedContainer, FractionalTranslation, Opacity, Widget};

use crate::theme::tokens;
```

- [ ] **Step 2: Update the doc comment**

In `vexo_uikit/src/transitions.rs`, the doc comment above `default_mobile_transition` (lines 48-65) currently ends with:

```rust
/// The underneath page dims to 0.85 (subtle, closer to iOS native than the
/// previous 0.6) so it stays visible peeking from the left edge during the
/// transition — matching SwiftUI's `UINavigationController` dual-view
/// animation. The dimming mitigates text bleed-through when page backgrounds
/// are transparent.
```

Append a paragraph describing the shadow:

```rust
///
/// The moving page also casts a soft drop shadow (`Color::BLACK` at 0.3
/// alpha, 12px blur, zero offset/spread) via a `DecoratedContainer`. The
/// shadow is full-perimeter; `NavigationStackView` wraps the content `Stack`
/// in a clipping `DecoratedContainer` so only the leading-edge strip is
/// visible — matching iOS native push. Desktop transition is unchanged
/// (fade-only, no shadow).
```

- [ ] **Step 3: Wrap `child` in a shadow-bearing `DecoratedContainer`**

In `vexo_uikit/src/transitions.rs`, the `default_mobile_transition` function (line 66) currently is:

```rust
pub fn default_mobile_transition(ctx: &TransitionCtx, child: Box<dyn Widget>) -> Box<dyn Widget> {
    let t = ctx.t as f32;
    let (fx, alpha) = match (ctx.direction, ctx.is_incoming) {
        (TransitionDir::Push, true) => (1.0 - t, 1.0),
        (TransitionDir::Push, false) => (-0.3 * t, 1.0 - 0.15 * t),
        (TransitionDir::Pop, true) => (-0.3 * (1.0 - t), 0.85 + 0.15 * t),
        (TransitionDir::Pop, false) => (t, 1.0),
        (TransitionDir::PopToRoot, true) => (-0.3 * (1.0 - t), 0.85 + 0.15 * t),
        (TransitionDir::PopToRoot, false) => (t, 1.0),
    };
    Opacity::new(FractionalTranslation::new(child, fx, 0.0), alpha).boxed()
}
```

Replace the final expression (the `Opacity::new(...)` line) with:

```rust
    let shadowed = DecoratedContainer::new(child).shadow(
        BoxShadow::new(Color::BLACK.with_alpha(tokens::navigation::PAGE_SHADOW_ALPHA))
            .blur(tokens::navigation::PAGE_SHADOW_BLUR),
    );
    Opacity::new(FractionalTranslation::new(shadowed, fx, 0.0), alpha).boxed()
```

The match arms (fx/alpha computation) are unchanged.

- [ ] **Step 4: Build to verify it compiles**

Run: `cargo build -p vexo_uikit`
Expected: PASS

- [ ] **Step 5: Run existing transitions tests to verify no regressions**

Run: `cargo test -p vexo_uikit transitions::tests`
Expected: PASS — the existing tests downcast the top-level wrapper to `Opacity`, which is still `Opacity` (the `DecoratedContainer` is one level deeper). The `push_outgoing_dims_to_0_85_not_zero` and `pop_incoming_un_dims_from_0_85_to_1` tests still pass.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/transitions.rs
git commit -m "feat(transitions): attach iOS-native drop shadow to moving page

default_mobile_transition now wraps the page in a DecoratedContainer with
a full-perimeter BoxShadow (black 0.3 alpha, 12px blur). The shadow is
clipped to the nav content area by an ancestor clip wrapper (added in the
next task), so only the leading-edge strip is visible — matching iOS
native push. Desktop transition unchanged."
```

---

## Task 3: Wrap nav content `Stack` in clipping `DecoratedContainer`

**Files:**
- Modify: `vexo_uikit/src/navigation.rs`

**Interfaces:**
- Consumes: `DecoratedContainer` from `vexo`.
- Produces: modified `render()` that wraps `content_stack` in `DecoratedContainer::new(content_stack).clip()` before `SafeArea`.

**Type-stability invariant (critical):** The clip wrapper must be present in BOTH steady and transition states (and all platforms) so the reconciler's `can_update()` does not remount the `Stack` subtree on state transitions. At steady state, the base page fills the content area exactly, so the clip's `PushClip` is a cheap scissor that clips nothing visible. The wrapper is layout pass-through (no `Style.background`, no padding) — it does not affect the `Stack`'s in-flow layout.

- [ ] **Step 1: Add `DecoratedContainer` to the imports**

In `vexo_uikit/src/navigation.rs` (lines 44-48), the current import block is:

```rust
use vexo::{
    AlignItems, AnimationController, Component, ComponentState, CubicBezierCurve, Curve, Flex,
    FractionalTranslation, IndexedStack, LifecycleContext, Opacity, Positioned, RenderContext,
    SafeArea, Stack, Text, Theme, Widget,
};
```

Add `DecoratedContainer` (alphabetical order, after `CubicBezierCurve`/before `Curve`):

```rust
use vexo::{
    AlignItems, AnimationController, Component, ComponentState, CubicBezierCurve, Curve,
    DecoratedContainer, Flex, FractionalTranslation, IndexedStack, LifecycleContext, Opacity,
    Positioned, RenderContext, SafeArea, Stack, Text, Theme, Widget,
};
```

- [ ] **Step 2: Wrap `content_stack` in a clipping `DecoratedContainer`**

In `vexo_uikit/src/navigation.rs`, find line 721:

```rust
        let content: Box<dyn Widget> = content_stack.boxed();
```

Replace with:

```rust
        // Wrap the content `Stack` in a clipping `DecoratedContainer` so the
        // moving page's full-perimeter shadow (attached in
        // `default_mobile_transition`) is clipped to the nav content area —
        // only the leading-edge strip is visible, matching iOS native. Also
        // fixes a latent bleed bug where the sliding overlay's `Positioned`
        // page could paint outside the nav stack bounds.
        //
        // The wrapper is ALWAYS present (steady + transition, all platforms)
        // for type-stability: if the type flipped between bare `Stack`
        // (steady) and `DecoratedContainer(Stack)` (transition), the
        // reconciler would remount the subtree and lose page state. At steady
        // state the base page fills the content area exactly, so the clip is
        // a cheap no-op scissor.
        let clipped: Box<dyn Widget> = DecoratedContainer::new(content_stack).clip().boxed();
```

Then on the next line, update the `SafeArea::new` call to take `clipped` instead of `content`:

Current (line 722-726):

```rust
        let content = SafeArea::new(content).top(false).flex_fill();
```

Replace with:

```rust
        let content = SafeArea::new(clipped).top(false).flex_fill();
```

(Or rename the variable — `content` is fine; just ensure `SafeArea::new` receives the clipped widget.)

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build -p vexo_uikit`
Expected: PASS

- [ ] **Step 4: Run existing navigation tests to verify no regressions**

Run: `cargo test -p vexo_uikit`
Expected: All existing tests PASS. The controller semantics, `PendingOp` snapshots, `base_fx_alpha` math, and dirty-callback tests don't depend on the clip wrapper. The widget-tree-text-collection tests (`all_text`, `collect_text`) traverse via `child()`/`children()`, which `DecoratedContainer` exposes — traversal still works.

If a test fails because it asserts a specific widget type at a specific position in the tree (e.g., "SafeArea's child must be Stack"), update the assertion to expect `DecoratedContainer` between `SafeArea` and `Stack`. Inspect the failure first.

- [ ] **Step 5: Run the regression test for pass-through layout**

Run: `cargo test -p vexo test_nav_transition_text_does_not_wrap`
Expected: PASS — confirms the pass-through layout invariant still holds with the added `DecoratedContainer(clip)` wrapper.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/navigation.rs
git commit -m "feat(navigation): clip nav content to enable iOS-native push shadow

Wrap the content Stack in DecoratedContainer(clip=true) so the moving
page's full-perimeter shadow is clipped to the content area — only the
leading-edge strip is visible. Also fixes a latent bleed bug. The wrapper
is always present (steady + transition, all platforms) for type-stability."
```

---

## Task 4: Add widget-tree-downcasting tests

**Files:**
- Test: `vexo_uikit/tests/navigation_animation_tests.rs`

**Why these tests:** Verify (1) the shadow is attached to the overlay on mobile, (2) the shadow is absent on desktop, (3) the clip wrapper is present in both steady and transition states. Widget-tree downcasting matches the existing nav test pattern (see `push_outgoing_dims_to_0_85_not_zero` at `transitions.rs:160`, `steady_state_base_has_zero_offset_and_full_opacity` at `navigation_animation_tests.rs` Task 3 of the dual-view plan).

- [ ] **Step 1: Add imports for the new tests**

At the top of `vexo_uikit/tests/navigation_animation_tests.rs`, the existing imports are (lines 14-25):

```rust
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use vexo::inherited_registry::{InheritedMap, InheritedRegistry};
use vexo::{
    BuildOwner, DirtyTracking, ElementKey, RenderContext, RenderObjectRegistry, Text, Widget,
};
use vexo_uikit::{NavigationController, NavigationStackView};

use vexo_uikit::transitions::TransitionDir;
```

Add to the `vexo::` import block:

```rust
use vexo::{
    BoxShadow, BuildOwner, DecoratedContainer, DirtyTracking, ElementKey, RenderContext,
    RenderObjectRegistry, Text, Widget,
};
```

And add a `Platform` import (needed to force `.platform(Platform::Mobile)` / `Platform::Desktop`):

```rust
use vexo_uikit::platform::Platform;
```

(Verify `vexo_uikit::platform` is `pub` — if not, use `vexo_uikit::Platform` if re-exported, or make the module `pub`. Check `vexo_uikit/src/lib.rs`.)

- [ ] **Step 2: Add a `visit` helper if not already present**

The existing `collect_text` helper (line 76) traverses via `child()`/`children()`. Add a generic `visit` helper near it (if not already present from the dual-view plan's Task 3):

```rust
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

If `visit` already exists from the dual-view plan's Task 3, do not duplicate it.

- [ ] **Step 3: Write the four tests**

Add a new test module at the end of the file:

```rust
// ============================================================================
// NAV PUSH SHADOW (iOS-NATIVE)
// ============================================================================

mod nav_push_shadow_tests {
    use super::*;
    use vexo::{Color, FractionalTranslation, Opacity};

    fn find_first_shadowed_decorated_container<'a>(
        w: &'a dyn Widget,
    ) -> Option<&'a DecoratedContainer> {
        let mut found: Option<&DecoratedContainer> = None;
        let mut visit = |w: &dyn Widget| {
            if found.is_some() {
                return;
            }
            if let Some(dc) = w.as_any().downcast_ref::<DecoratedContainer>() {
                if !dc.style_ref().shadows.is_empty() {
                    found = Some(dc);
                }
            }
        };
        super::visit(w, &mut visit);
        found
    }

    fn find_clipped_decorated_container<'a>(
        w: &'a dyn Widget,
    ) -> Option<&'a DecoratedContainer> {
        let mut found: Option<&DecoratedContainer> = None;
        let mut visit = |w: &dyn Widget| {
            if found.is_some() {
                return;
            }
            if let Some(dc) = w.as_any().downcast_ref::<DecoratedContainer>() {
                if dc.style_ref().clip {
                    found = Some(dc);
                }
            }
        };
        super::visit(w, &mut visit);
        found
    }

    fn build_view_with_push(
        platform: Platform,
    ) -> (
        Box<dyn Widget>,
        vexo_uikit::NavigationStackViewState<&'static str>,
    ) {
        let controller: NavigationController<&'static str> = NavigationController::new();
        controller.push("a");
        // PendingOp is now set; the view's render() will see a transition.
        let view = NavigationStackView::new(controller.clone(), Text::new("Root"))
            .platform(platform)
            .destination(|d| Text::new(format!("Body-{}", d)).boxed());
        let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
        // Force the transition to mid-progress for snapshotting.
        if let Some(t) = state.transition.as_mut() {
            t.controller.set_value_for_test(0.5);
        }
        let tree = render_stack(view, &mut state);
        (tree, state)
    }

    #[test]
    fn mobile_push_overlay_has_shadow_decorated_container() {
        let (tree, _state) = build_view_with_push(Platform::Mobile);
        let dc = find_first_shadowed_decorated_container(&*tree)
            .expect("overlay must contain a DecoratedContainer with a shadow on mobile push");

        let shadows = &dc.style_ref().shadows;
        assert_eq!(shadows.len(), 1, "exactly one shadow expected");
        let s = &shadows[0];
        assert!(
            (s.color.r - 0.0).abs() < 1e-6
                && (s.color.g - 0.0).abs() < 1e-6
                && (s.color.b - 0.0).abs() < 1e-6
                && (s.color.a - 0.3).abs() < 1e-6,
            "shadow color must be BLACK at alpha 0.3, got {:?}",
            s.color
        );
        assert!((s.blur_radius - 12.0).abs() < 1e-6, "blur must be 12.0, got {}", s.blur_radius);
        assert!((s.offset.x - 0.0).abs() < 1e-6, "offset.x must be 0, got {}", s.offset.x);
        assert!((s.offset.y - 0.0).abs() < 1e-6, "offset.y must be 0, got {}", s.offset.y);
        assert!((s.spread_radius - 0.0).abs() < 1e-6, "spread must be 0, got {}", s.spread_radius);
    }

    #[test]
    fn mobile_pop_overlay_has_shadow_decorated_container() {
        // Build a view with one page pushed, then pop to start a Pop transition.
        let controller: NavigationController<&'static str> = NavigationController::new();
        controller.push("a");
        controller.clear_pending();
        controller.pop();

        let view = NavigationStackView::new(controller.clone(), Text::new("Root"))
            .platform(Platform::Mobile)
            .destination(|d| Text::new(format!("Body-{}", d)).boxed());
        let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
        if let Some(t) = state.transition.as_mut() {
            t.controller.set_value_for_test(0.5);
        }
        let tree = render_stack(view, &mut state);

        let dc = find_first_shadowed_decorated_container(&*tree)
            .expect("overlay must contain a DecoratedContainer with a shadow on mobile pop");
        assert_eq!(dc.style_ref().shadows.len(), 1);
    }

    #[test]
    fn desktop_overlay_has_no_shadow() {
        let (tree, _state) = build_view_with_push(Platform::Desktop);
        let dc = find_first_shadowed_decorated_container(&*tree);
        assert!(
            dc.is_none(),
            "desktop transition must not attach a shadow; found {:?}",
            dc.map(|d| d.style_ref().shadows.len())
        );
    }

    #[test]
    fn nav_content_is_clipped_in_steady_and_transition() {
        // Steady state.
        let controller: NavigationController<&'static str> = NavigationController::new();
        controller.push("a");
        controller.clear_pending();
        let view = NavigationStackView::new(controller.clone(), Text::new("Root"))
            .platform(Platform::Mobile)
            .destination(|d| Text::new(format!("Body-{}", d)).boxed());
        let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
        let steady_tree = render_stack(view, &mut state);
        let steady_clip = find_clipped_decorated_container(&*steady_tree)
            .expect("steady state must have a clipped DecoratedContainer wrapping the content");
        assert!(steady_clip.style_ref().clip);

        // Transition state.
        let controller2: NavigationController<&'static str> = NavigationController::new();
        controller2.push("a");
        let view2 = NavigationStackView::new(controller2.clone(), Text::new("Root"))
            .platform(Platform::Mobile)
            .destination(|d| Text::new(format!("Body-{}", d)).boxed());
        let mut state2 = vexo_uikit::NavigationStackViewState::<&'static str>::default();
        if let Some(t) = state2.transition.as_mut() {
            t.controller.set_value_for_test(0.5);
        }
        let trans_tree = render_stack(view2, &mut state2);
        let trans_clip = find_clipped_decorated_container(&*trans_tree)
            .expect("transition state must have a clipped DecoratedContainer wrapping the content");
        assert!(trans_clip.style_ref().clip);
    }
}
```

**Notes on the test scaffolding:**

- `set_value_for_test(0.5)` — check whether `AnimationController` exposes a test-only value setter. If not, the existing nav animation tests must already have a way to drive the transition to a known `t`. Inspect `navigation_animation_tests.rs` for the existing pattern; if mid-transition snapshotting isn't possible without a ticker, simplify the tests to assert structural presence (shadow attached / clip present) without forcing a specific `t` value — the overlay exists as soon as `pending()` is set, even at `t=0`.

- `Style.shadows` field access — verify `Style.shadows` is `pub` (per `vexo/src/style.rs:81` it is: `pub shadows: Vec<BoxShadow>`).

- `Style.clip` field access — verify `Style.clip` is `pub` (per `vexo/src/style.rs` it is).

- `BoxShadow.color` / `.blur_radius` / `.offset` / `.spread_radius` field access — verify these are `pub` (per `vexo/src/style.rs:21-26` they are).

- `DecoratedContainer::style_ref()` — verify this method exists (used by existing tests at `decorated_container.rs:612-616`).

- [ ] **Step 4: Run the new tests to verify they pass**

Run: `cargo test -p vexo_uikit nav_push_shadow_tests`
Expected: PASS — all 4 tests pass.

If `set_value_for_test` does not exist or the transition state cannot be driven without a ticker, adjust the tests to:
- For shadow tests: assert that *if* an overlay exists, its `DecoratedContainer` has the expected shadow. Use the same pattern the existing tests use for mid-transition inspection (or fall back to inspecting the steady-state `default_mobile_transition` output directly by calling it with a synthetic `TransitionCtx`).
- For the clip test: the steady-state assertion is sufficient to prove the clip wrapper is present; the transition-state assertion is a bonus.

- [ ] **Step 5: Run the full vexo_uikit test suite**

Run: `cargo test -p vexo_uikit`
Expected: All tests pass — both existing and new.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/tests/navigation_animation_tests.rs
git commit -m "test(navigation): verify iOS-native push shadow and content clip

Four widget-tree-downcasting tests: mobile push overlay has shadow
DecoratedContainer with correct params; mobile pop overlay same; desktop
overlay has no shadow; nav content is clipped in steady and transition
states."
```

---

## Task 5: Full verification and manual testing prompt

**Files:** None modified

- [ ] **Step 1: Run the full workspace test suite**

Run: `cargo test`
Expected: ALL tests pass — `vexo`, `vexo_uikit`, `shared_app`, `desktop_demo`.

- [ ] **Step 2: Run the regression test specifically**

Run: `cargo test -p vexo test_nav_transition_text_does_not_wrap`
Expected: PASS — confirms the pass-through layout invariant still holds with the added `DecoratedContainer(clip)` wrapper around the content `Stack`.

- [ ] **Step 3: Build in release mode to catch any issues**

Run: `cargo build --release`
Expected: PASS

- [ ] **Step 4: Prompt the user to run the iOS project for visual verification**

The shadow is mobile-only; desktop runs fade-only with no shadow. The user must run the iOS project to visually verify.

Ask the user:

> The iOS-native push shadow is implemented. To visually verify:
> - Build and run the iOS project via `./build_for_ios.sh` + Xcode.
> - Push a page: a soft dark strip should appear on the incoming page's left edge, cast onto the dimmed underneath page. No bleed at the top (nav bar) or bottom (tab bar).
> - Pop a page: a soft dark strip should appear on the outgoing page's left edge.
> - Steady state: no shadow visible.
>
> Please run the iOS build and confirm the shadow matches iOS native behavior. If the blur feel or alpha is off, the constants `PAGE_SHADOW_ALPHA` and `PAGE_SHADOW_BLUR` in `vexo_uikit/src/theme/tokens.rs` are the tuning knobs.

- [ ] **Step 5: Final commit (if any cleanup needed)**

If the user reports issues, fix them (likely just tuning `PAGE_SHADOW_ALPHA` / `PAGE_SHADOW_BLUR`) and commit. If everything looks good, no commit needed — the implementation is complete.
