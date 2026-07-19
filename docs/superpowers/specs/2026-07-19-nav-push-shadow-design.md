# Navigation Push Shadow (iOS-Native) — Design Spec

**Date:** 2026-07-19
**Status:** Approved (pending implementation)
**Scope:** Add an iOS-native drop shadow to the moving page during mobile push/pop transitions — a soft strip on the moving page's leading edge, cast onto the underneath page. Desktop transitions are unchanged.

---

## 1. Background & Motivation

Vexo's `NavigationStackView` (`vexo_uikit/src/navigation.rs`) already implements a dual-view push/pop transition (see `2026-07-12-dual-view-push-animation-design.md`): the overlay (moving page) slides in/out via `FractionalTranslation`, while the base (underneath page) slides slightly and dims.

On iOS native `UINavigationController` push, the moving page casts a soft drop shadow onto the underneath page — visible as a darkening strip along the moving page's leading edge. This shadow is a primary depth cue: it tells the user which page is "on top" and reinforces the slide metaphor. Vexo's current transition has no such shadow; the moving page is a flat slab.

Vexo's `BoxShadow` system is fully implemented end-to-end (style → render object → command → shader; see `2026-07-19-shadow-design.md`). `BoxShadow` is a full-perimeter Gaussian-blurred silhouette emitted *behind* its owning `DecoratedContainer`'s fill (`vexo/src/render_objects/container.rs:168`). Crucially, shadow `Rect`s respect **ancestor** clips (`vexo/src/frame_builder.rs:271` — every shadow op captures `self.current_clip()`), even though they bypass their *own* container's clip (`container.rs:171-172`).

**Goal:** Attach a full-perimeter `BoxShadow` to the moving page during mobile push/pop, clipped to the nav content area by an ancestor clip wrapper, so only the leading-edge strip is visible — matching iOS native.

**Non-goals:**
- Per-edge shadow geometry (no shader changes, no `BoxShadow.edges` API).
- Dark-mode-aware shadow color (fixed black, both themes).
- Desktop shadow (desktop is fade-only; no slide metaphor).
- Hit-testing changes during transition (clip affects paint only).
- Gesture-driven swipe-back.

---

## 2. Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Shadow semantics | One shadow per moving page, on its leading edge (Option C) | iOS-faithful for both push (incoming page's left edge) and pop (outgoing page's left edge, which is its leading edge while moving right). Single shadow per frame; cheap. |
| Where attached | Inside `default_mobile_transition` (Option A) | Localized to the transition fn, which already "owns" the moving-page visual. Custom `transition_fn` users opt out of the default shadow — they can add their own. Navigator stays shadow-agnostic. |
| Edge restriction | Full-perimeter shadow + ancestor clip (Option B) | Uses existing `BoxShadow` infrastructure + existing clip infrastructure. iOS-faithful (iOS relies on screen-edge clipping the same way). Fixes a latent bleed bug where the sliding page can paint outside the nav stack bounds. Zero shader changes. |
| Shadow params | `Color::BLACK.with_alpha(0.1)`, blur `6.0`, offset `(0,0)`, spread `0.0` | iOS-faithful values (subtle ambient shadow). Ancestor clip does the edge restriction, not the offset. |
| Clip placement | Wrap `content_stack` inside `SafeArea`, outside `Stack` (Option A) | Minimum scope; exactly the region containing base + overlay; no semantic changes to `SafeArea`. |
| Shadow position in overlay tree | Innermost: `Opacity(FractionalTranslation(DecoratedContainer(page, shadow)))` (Option A) | Shadow must translate and dim with the page (it's the page's cast shadow). Innermost placement guarantees both. |
| Endpoint frames | Constant shadow throughout transition (Option A) | One-frame visibility at stationary endpoints is imperceptible at 60fps over a 350ms animation. Avoids per-frame `BoxShadow` mutation or extra `Opacity` wrapper. |
| Platform scope | Mobile only (Option A) | Shadow is part of the slide metaphor; desktop has no slide. The clip wrapper is **unconditional** (always present, all platforms) for type-stability and to fix the latent bleed bug. |
| Constants location | `vexo_uikit/src/theme/tokens.rs` under `pub mod navigation` (Option B) | Consistent with existing nav visual tokens (`MOBILE_HEADER_HEIGHT`, `MOBILE_TITLE_FONT_SIZE`, etc.). Discoverable, single tuning location. |
| Corner radius on clip | None — pure rectangle (Option A) | iOS native uses a rectangular content area. Keeps `PushClip` a pure rectangle (no `PushCornerRadius` interaction). |
| Dark mode | Fixed black shadow, both themes (Option A) | Matches iOS lighting-effect model (shadow becomes less visible against dark surfaces; slide motion is the primary cue). Avoids expanding `ThemeData` with a shadow role for one consumer. |
| Hit-testing | Unchanged (Option A) | Clip affects paint only. Hit-testing during transitions is a pre-existing concern orthogonal to the shadow work. |
| Page background | Transparent (Option A) | Shadow is cast by the container's *bounds*, not its fill. The page's existing background shows through. Forcing a background would break dark mode and per-page choices. |
| Composition with base dim | Accept (Option A) | Shadow paints on top of the dimmed underneath page — exactly iOS behavior. Dimming and shadow accumulate, not cancel. |
| Testing | Widget-tree downcasting (Option A) | Matches existing nav test pattern. `container.rs` paint path is already extensively tested in isolation; new tests verify structure (shadow attached, content clipped). |
| Verification | User runs iOS project | Visual quality (blur feel, alpha) confirmed by the user on real iOS. Structural correctness verified by tests. |
| Documentation | Spec + plan (Option A) | Matches repo's `2026-07-12-dual-view-push-animation` pattern. Decisions are non-obvious; future maintainers benefit from rationale. |

---

## 3. Visual Behavior

### 3.1 Push (mobile)

`t: 0 → 1`, `eased = transition_curve.transform(t)`

| `t` | Base (underneath) | Overlay (incoming, moving) |
|---|---|---|
| 0.0 | dimmed 1.0, in place (per existing dual-view spec) | fx=1.0 (off-screen right), full opacity. Shadow: off-screen right, clipped away. Invisible. |
| 0.5 | dimmed ~0.925, slid ~15% left | fx=0.5 (halfway). Shadow: visible strip on the page's left edge, cast onto the dimmed base. |
| 1.0 | dimmed 0.85, slid 30% left | fx=0.0 (in place). Shadow: still painted, mostly covered by the now-stationary page. Brief one-frame presence before overlay unmounts. |

The shadow is a full-perimeter `BoxShadow` on the overlay's `DecoratedContainer`. The ancestor clip restricts it to the nav content area, so only the leading-edge (left) strip is visible — the top/bottom/right bleed is clipped.

### 3.2 Pop (mobile)

`base_fx_alpha` reverses; the overlay is now the outgoing page.

| `t` | Base (destination, underneath) | Overlay (outgoing, moving) |
|---|---|---|
| 0.0 | dimmed 0.85, slid 30% left | fx=0.0 (in place), full opacity. Shadow: visible strip on the page's left edge. |
| 0.5 | un-dimming, sliding back | fx=0.5 (sliding right). Shadow: visible strip on the page's left edge (its leading edge while moving right). |
| 1.0 | un-dimmed 1.0, in place | fx=1.0 (off-screen right). Shadow: clipped away. Invisible. |

### 3.3 PopToRoot

Same curves as Pop.

### 3.4 Steady state

No overlay exists. The clip wrapper is still present (type-stability) but clips nothing visible — the base page fills the content area exactly.

### 3.5 Desktop

`default_desktop_transition` is unchanged — fade-only, no shadow. The clip wrapper is present (unconditional) but the page doesn't slide, so the clip is a no-op.

### 3.6 Composition with base dim

The shadow is painted *after* the base's `Opacity(0.85)` has been applied (overlay is a `Positioned` sibling painted on top of the base in the `Stack`). The shadow's effective alpha is `0.1 × 1.0 (overlay opacity) = 0.1`, painted on top of the dimmed base. The composite is slightly darker than either effect alone — exactly iOS behavior.

### 3.7 Paint order within the overlay

Within the overlay tree `Opacity → FractionalTranslation → DecoratedContainer(page, shadow)`:

1. `DecoratedContainer` emits shadow `Rect`s **first** (`container.rs:168` — shadows before fill/children).
2. `DecoratedContainer` emits the page's own background/border/children.
3. The page paints over its own shadow where they overlap (correct — page is opaque from the user's perspective).
4. The shadow is visible only where it extends beyond the page bounds (the blur falloff) — cast onto the underneath page, visible through the ancestor clip.

---

## 4. Code Changes

### 4.1 `vexo_uikit/src/theme/tokens.rs`

Add two constants in `pub mod navigation` (after the existing `MOBILE_TITLE_FONT_SIZE`):

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
pub const PAGE_SHADOW_ALPHA: f32 = 0.1;
pub const PAGE_SHADOW_BLUR: f32 = 6.0;
```

### 4.2 `vexo_uikit/src/transitions.rs`

Modify `default_mobile_transition` to wrap `child` in a shadow-bearing `DecoratedContainer` **before** wrapping in `FractionalTranslation`/`Opacity`:

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
    let shadowed = DecoratedContainer::new(child).shadow(
        BoxShadow::new(Color::BLACK.with_alpha(tokens::navigation::PAGE_SHADOW_ALPHA))
            .blur(tokens::navigation::PAGE_SHADOW_BLUR),
    );
    Opacity::new(FractionalTranslation::new(shadowed, fx, 0.0), alpha).boxed()
}
```

New imports at the top of `transitions.rs`:

```rust
use vexo::{BoxShadow, Color, DecoratedContainer, FractionalTranslation, Opacity, Widget};
use crate::theme::tokens;
```

Update the doc comment above `default_mobile_transition` to mention the shadow.

**Why innermost:** the shadow must translate with the page (it's the page's cast shadow) and dim with the page (if alpha ever changes). Innermost placement guarantees both. Putting the shadow outside `FractionalTranslation` would leave it stationary while the page slides — wrong. Putting it outside `Opacity` would leave it at full alpha while the page dims — inconsistent.

### 4.3 `vexo_uikit/src/navigation.rs`

Wrap `content_stack` in a clipping `DecoratedContainer` before wrapping in `SafeArea`. Around line 721, the current code is:

```rust
let content: Box<dyn Widget> = content_stack.boxed();
let content = SafeArea::new(content).top(false).flex_fill();
```

Replace with:

```rust
let clipped: Box<dyn Widget> = DecoratedContainer::new(content_stack).clip().boxed();
let content = SafeArea::new(clipped).top(false).flex_fill();
```

New imports in `navigation.rs`:

```rust
use vexo::{
    ..., DecoratedContainer, ...,
};
```

(`BoxShadow`, `Color`, `tokens` are not needed in `navigation.rs` — the shadow is attached in `transitions.rs`.)

**Why unconditional (always present):**
1. **Type-stability.** The clip wrapper must be present in both steady and transition states so the reconciler's `can_update()` does not remount the `Stack` subtree on state transitions (which would lose page state).
2. **Latent bleed fix.** Today, the sliding overlay's `Positioned` page can paint outside the nav stack bounds (only invisible because the stack happens to fill the window). The clip wrapper makes this guarantee structural rather than incidental.
3. **No steady-state cost.** `DecoratedContainer` with no background/border/shadow and `clip=true` is a paint-time no-op for the base page (which fills the content area exactly); the clip's `PushClip` is a cheap scissor rect.

### 4.4 No other files change

`default_desktop_transition`, `AnimationController`, `PendingOp` state machine, `IndexedStack` state preservation, the `base_fx_alpha` computation, and all animation primitives are untouched. The shader, frame builder, and render command structures are untouched (the existing `BoxShadow` pipeline handles everything).

---

## 5. Type-Stability & State Preservation

### 5.1 Type-stability invariants

**Base widget tree (unchanged from `2026-07-12-dual-view-push-animation`):**
`Opacity → FractionalTranslation → IndexedStack` — always, all states, all platforms.

**Overlay widget tree (new):**
`Opacity → FractionalTranslation → DecoratedContainer(page, shadow) → page` — constant within a transition. The overlay only exists during a transition (it's a conditionally-pushed `Positioned` sibling), so there's no steady/transition structural mismatch to worry about. Within the transition, the structure is identical every frame — no remounts.

**Content area tree (new):**
`SafeArea → DecoratedContainer(clip=true) → Stack { base, Positioned(overlay) }` — always, all states, all platforms. The `DecoratedContainer(clip=true)` is present in steady state (clip is a no-op since the base fills the area), during push, and during pop. Constant type, no remounts.

### 5.2 State preservation (unchanged)

The base remains an in-flow child of the `Stack` (not `Positioned`). The `IndexedStack` still keeps all pages mounted (wrapped in `Offstage`), preserving `ComponentState`, focus, and `TextEditingController` across transitions. The new clip wrapper is layout pass-through (no `Style.background`, no padding) — it does not affect the `Stack`'s in-flow layout.

### 5.3 Custom `transition_fn` interaction

The shadow is attached inside `default_mobile_transition`. A custom `transition_fn` replaces the entire overlay visual — if the caller supplies a custom `transition_fn`, they get no shadow unless they add one themselves. This is consistent with the existing design where `default_mobile_transition` owns the moving-page visual (offset, alpha) and a custom `transition_fn` opts out of all of it.

The ancestor clip wrapper is **unconditional** — it applies regardless of `transition_fn`. This is correct: the clip is a navigator-level concern (keep the moving page inside the content area), not a transition-visual concern.

---

## 6. Testing

### 6.1 Existing tests

Run `vexo_uikit/tests/navigation_stack_tests.rs` and `navigation_animation_tests.rs` unchanged. The controller semantics, `PendingOp` snapshots, `base_fx_alpha` math, and dirty-callback tests do not depend on the shadow. Verify they still pass.

The existing `transitions.rs` tests (`push_outgoing_dims_to_0_85_not_zero`, `pop_incoming_un_dims_from_0_85_to_1`) downcast the top-level wrapper to `Opacity`. After this change, the top-level wrapper is still `Opacity` — the `DecoratedContainer` is one level deeper. These tests continue to pass without modification.

### 6.2 New unit tests (widget-tree downcasting)

Added to `vexo_uikit/tests/navigation_animation_tests.rs`:

1. **`mobile_push_overlay_has_shadow_decorated_container`**: Build a `NavigationStackView` with `.platform(Platform::Mobile)`, trigger a push, render at `t=0.5`, traverse the overlay widget tree, assert a `DecoratedContainer` exists with `style_ref().shadows.len() == 1`, `shadows[0].blur_radius == PAGE_SHADOW_BLUR`, `shadows[0].color == Color::BLACK.with_alpha(PAGE_SHADOW_ALPHA)`, `shadows[0].offset == (0,0)`, `shadows[0].spread_radius == 0.0`.

2. **`mobile_pop_overlay_has_shadow_decorated_container`**: Same, for `TransitionDir::Pop`.

3. **`desktop_overlay_has_no_shadow`**: Build with `.platform(Platform::Desktop)`, render at `t=0.5`, assert no `DecoratedContainer` with shadows exists in the overlay tree (desktop transition is fade-only).

4. **`nav_content_is_clipped`**: Render the nav stack in both steady state and mid-transition; assert the content tree contains a `DecoratedContainer` with `style_ref().clip == true`, present in both states.

### 6.3 Regression test

Re-run `vexo/src/passthrough_integration.rs:399` (`test_nav_transition_text_does_not_wrap`) to confirm the pass-through layout invariant still holds with the added `DecoratedContainer(clip)` wrapper around the content `Stack`.

### 6.4 Manual verification

The user runs the iOS project (`./build_for_ios.sh` + Xcode) and visually confirms:
- Push: soft dark strip on the incoming page's left edge, visible against the dimmed underneath page.
- Pop: soft dark strip on the outgoing page's left edge.
- No shadow bleed at the top (nav bar) or bottom (tab bar).
- Steady state: no shadow visible.

---

## 7. Alternatives Considered

### 7.1 Shadow semantics (Q1)

- **Option A — One shadow, incoming page's leading edge only:** omits the pop case. Incomplete.
- **Option B — Two separate shadows, one per page per edge:** non-iOS-faithful. A trailing-edge shadow on the outgoing page would extend off-screen and be invisible (covered by the incoming page) or require non-iOS inner-shadow geometry.
- **Option C (selected) — One shadow per moving page, on its leading edge:** iOS-faithful for both push and pop. Single shadow per frame.

### 7.2 Edge restriction (Q3)

- **Option A — Asymmetric offset only:** top/bottom bleed falls on nav bar and tab bar. Visually approximate.
- **Option B (selected) — Full-perimeter shadow + ancestor clip:** iOS-faithful, uses existing infrastructure, fixes a latent bleed bug.
- **Option C — Extend `BoxShadow` API with per-edge support:** shader + render-object + API changes across 6+ files. Over-engineered.

### 7.3 Shadow position in overlay tree (Q6)

- **Option A (selected) — Innermost:** shadow translates and dims with the page. Correct.
- **Option B — Outside translation:** shadow stationary while page slides. Wrong.
- **Option C — Outside opacity:** shadow not dimmed with page. Inconsistent.

### 7.4 Endpoint frames (Q7)

- **Option A (selected) — Constant shadow:** simplest, one-frame endpoint visibility imperceptible.
- **Option B — Fade shadow in/out with `sin(π·t)`:** requires per-frame `BoxShadow` mutation (not in API) or structural change to wrap shadow in its own `Opacity` (breaks Q6 Option A).
- **Option C — Skip shadow at t<ε and t>1-ε:** discrete jump, more visible than A's brief flash.

### 7.5 Constants location (Q9)

- **Option A — Inline in `transitions.rs`:** scatters constants.
- **Option B (selected) — `tokens::navigation` module:** consistent with existing nav tokens, single tuning location.
- **Option C — Builder knob on `NavigationStackView`:** over-engineers API for a default-visual value.

### 7.6 Dark mode (Q12)

- **Option A (selected) — Fixed black, both themes:** matches iOS lighting-effect model. Avoids `ThemeData` expansion.
- **Option B — Theme-aware shadow color:** requires plumbing `Theme::of(ctx)` into `default_mobile_transition` (no `ctx` today), changes transition fn signature, breaks custom `transition_fn` callers.
- **Option C — New `ThemeData.shadow` field:** expands theme API for one consumer.
