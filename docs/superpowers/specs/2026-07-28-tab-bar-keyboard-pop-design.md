# Tab Bar Keyboard-Dismiss "Pop" — Design

**Date:** 2026-07-28
**Topic:** Eliminate the end-of-keyboard-dismiss height "pop" on the bottom tab bar.

## Problem

On iOS, when the software keyboard dismisses in a screen hosted inside `TabBarView` (e.g. `ChatScreen`), the tab bar visibly grows upward at the tail end of the dismiss animation. Observed as the bar sitting at ~44pt height for most of the animation, then "popping" to ~64pt (or whatever the device's home-indicator inset dictates) right as the animation finishes. The bar's final resting position is correct; only the animation is wrong.

### Root cause

`RootMediaQuery::render()` (`vexo/src/widgets/media_query.rs:321-332`) derives `MediaQueryData.padding` from two sources per Flutter's invariant:

```rust
let viewPadding = sources.safe_area;                              // ~34pt home-indicator inset, static
let viewInsets   = { bottom: sources.keyboard_current_height, .. }; // interpolated 300 → 0
let padding = {
    bottom: (viewPadding.bottom - viewInsets.bottom).max(0.0),   // ← line 329
    ..
};
```

`SafeArea` (`vexo/src/widgets/safe_area.rs:143-152`) reads `MediaQuery::of(ctx).padding` and applies `padding.bottom` as bottom padding on its child. `TabBarView` (`vexo_uikit/src/tab_bar.rs:197-200`) wraps its 49pt bar content in `SafeArea`, so the bar's total height = `49 + padding.bottom`, bottom-aligned in the column.

Tracing `padding.bottom` through a dismiss animation (300 → 0, home-indicator = 34):

| Frame | keyboard_height | `max(34 - kh, 0)` | bar height | bar top edge |
|-------|-----------------|-------------------|------------|--------------|
| start | 300             | 0                 | 49         | screen_h − 49 |
| 50%   | 150             | 0                 | 49         | screen_h − 49 |
| 85%   | 34              | 0                 | 49         | screen_h − 49 |
| 92%   | 20              | 14                | 63         | screen_h − 63 |
| 97%   | 10              | 24                | 73         | screen_h − 73 |
| end   | 0               | 34                | 83         | screen_h − 83 |

`padding.bottom` is clamped at 0 for the entire animation while `keyboard_height > home_indicator_inset`. It only begins growing in the final ~10-15%, once the interpolated keyboard height drops below the home-indicator inset. Because the bar is bottom-aligned in the column, that delayed height growth manifests as the observed "pop".

The symmetric bug exists at keyboard *show* start: `padding.bottom` animates 34 → 0 as the keyboard grows past 34pt, so the bar visibly shrinks 83 → 49 during show.

### Why the final position is correct

At animation end, `keyboard_height = 0`, so `padding.bottom = viewPadding.bottom = 34`. The bar reaches its correct resting height of 83pt. The bug is purely temporal — the safe-area padding recovery is delayed to the tail of the animation instead of being constant.

## Scope

- Fix the tab bar height pop during keyboard show/dismiss on iOS.
- Bar stays pinned to the screen bottom (Flutter `Scaffold` behavior) — no slide, no height animation.
- No changes to `SafeArea`, `RootMediaQuery`, `MediaQuery::remove_view_insets`, or any other `SafeArea` consumer.

## Non-goals

- Native iOS "ride the keyboard" behavior (bar slides up/down with keyboard). Explicitly rejected by the user in favor of the simpler pinned behavior.
- Changing `RootMediaQuery`'s `padding = max(viewPadding - viewInsets, 0)` invariant (would break other `SafeArea` + keyboard screens).
- Android keyboard avoidance (out of scope for this fix).

## Approach

Two coordinated changes, both local to `vexo_uikit/src/tab_bar.rs`. Both reference `viewPadding.bottom` (the raw, un-clamped home-indicator inset) as the single source of truth for the bar's safe-area consumption.

### Change 1 — Bar inset source uses `viewPadding.bottom`

Currently (`tab_bar.rs:197-200`):

```rust
let bar = DecoratedBox::with_style(
    SafeArea::new(bar.boxed()).top(false).boxed(),
    Style::default().background(nav.mobile_header_bg),
);
```

`SafeArea` reads `padding.bottom`, which is clamped by keyboard height. Wrap the `SafeArea` in a `MediaQueryMutator` that forces `padding.bottom = viewPadding.bottom`:

```rust
let bar = DecoratedBox::with_style(
    MediaQueryMutator::new(
        SafeArea::new(bar.boxed()).top(false).boxed(),
        |parent: &MediaQueryData| {
            let mut p = parent.padding;
            p.bottom = parent.viewPadding.bottom;
            parent.copy_with_padding(p)
        },
    )
    .boxed(),
    Style::default().background(nav.mobile_header_bg),
);
```

`SafeArea` still reads `padding` per its contract; the mutator just feeds it the un-clamped value for the bottom edge. Top, left, right padding is untouched (those aren't affected by keyboard).

### Change 2 — Page's `tab_bar_height` uses `viewPadding.bottom`

Currently (`tab_bar.rs:218-223`):

```rust
let page = MediaQueryMutator::new(stack.boxed(), |parent: &MediaQueryData| {
    let tab_bar_height = TAB_BAR_HEIGHT + parent.padding.bottom;  // ← line 219
    let mut v = parent.viewInsets;
    v.bottom = (v.bottom - tab_bar_height).max(0.0);
    parent.copy_with_view_insets(v)
});
```

Change `parent.padding.bottom` → `parent.viewPadding.bottom`:

```rust
let page = MediaQueryMutator::new(stack.boxed(), |parent: &MediaQueryData| {
    let tab_bar_height = TAB_BAR_HEIGHT + parent.viewPadding.bottom;
    let mut v = parent.viewInsets;
    v.bottom = (v.bottom - tab_bar_height).max(0.0);
    parent.copy_with_view_insets(v)
});
```

The page's keyboard-avoidance math (`viewInsets.bottom - tab_bar_height`) is unchanged in spirit — it still subtracts the bar's footprint from the keyboard inset so chat content sits above the bar. Only the `tab_bar_height` source changed, to match the actual bar height produced by Change 1.

### Why this works

Both changes reference `viewPadding.bottom`, which is the OS-reported home-indicator inset — constant at ~34pt throughout keyboard animation (it's independent of the keyboard). During animation:

- `viewPadding.bottom` = 34 (constant)
- `viewInsets.bottom` interpolates 0 → 300 (or 300 → 0) as before
- Bar height = `49 + 34 = 83` for the entire animation (was: clamped `max(0, 34-kh)` + 49)
- `tab_bar_height` for page = `49 + 34 = 83` (matches bar height exactly)

The pop disappears because `padding.bottom` is no longer derived from the interpolated keyboard height — the bar's safe-area inset is the constant `viewPadding.bottom` regardless of keyboard state.

### What doesn't change

- `SafeArea` widget — unchanged, still reads `padding` per Flutter semantics.
- `RootMediaQuery` — unchanged, still computes `padding = max(viewPadding - viewInsets, 0)`.
- `KeyboardAvoider` — unchanged.
- `ChatScreen` — unchanged.
- All other `SafeArea` consumers — unaffected.

## Risk: coordination

The two changes must stay consistent — both use `viewPadding.bottom`. If they drift, the chat content would sit 34pt too high or too low relative to the bar. This is contained to a single file (`tab_bar.rs`) and verifiable by a layout test (below).

## Testing

One new integration test in `tab_bar.rs`'s test module, mirroring the existing `test_tab_bar_top_hairline_paints` style (which already exercises `pipeline.layout()` + render-object tree walking).

**`test_tab_bar_height_stays_constant_during_keyboard_animation`**

Build a `TabBarView`. Inject two `MediaQuery` source states into the pipeline's `BuildOwner`:

1. **Mid-animation state**: `keyboard_inset_source` set to 150pt (mid-dismiss), `safe_area_source` bottom = 34pt. Layout at 390×600. Assert:
   - Tab bar total height ≈ 83pt (49 + 34) — the constant target.
   - Tab bar top edge = 600 − 83 = 517 (bottom-aligned, no growth).

2. **Animation-end state**: `keyboard_inset_source` set to 0pt, `safe_area_source` unchanged. Layout again. Assert:
   - Tab bar total height ≈ 83pt (same as mid-animation).
   - Tab bar top edge = 517 (same).

The before/after equality is the regression guard — if the pop is present, mid-animation height would be ~49 and end height would be ~83, failing the equality assertion.

The test needs to walk the render-object tree to find the tab bar's `DecoratedBox` bounds. The existing `test_chat_screen_input_bar_pinned_to_bottom_with_few_messages` (`shared_app/src/chats/chat_screen.rs:290-374`) demonstrates the proxy-walking pattern for navigating through `Component` / `Shared` proxy layers; reuse that pattern here.

### Test infrastructure

The existing `tab_bar.rs` tests construct a `ThreeTreePipeline` directly (`ThreeTreePipeline::new(Arc::new(AnimationTicker::new()))`) and call `pipeline.update(view)` + `pipeline.layout(size, engine, font_system)`. `ThreeTreePipeline::new` constructs its `BuildOwner` internally (`pipeline.rs:173`); it does not accept an injected one. However, `ThreeTreePipeline` already exposes public setters that delegate to the `BuildOwner`:

- `set_safe_area_source(source)` (`pipeline.rs:201`)
- `set_keyboard_inset_source(source)` (`pipeline.rs:213`)
- `set_media_query_data_source(source)` (`pipeline.rs:224`)

These are called once at window init by `WindowState`, but they are plain `pub fn` — no `#[cfg(test)]` gate needed. The test constructs `SafeAreaSource` and `KeyboardInsetSource` directly (they are `Default` + have `set(...)` methods), installs them on the pipeline via these existing setters, mutates the source's atomics between layout calls to simulate the animation, and re-layouts.

No new test-only API surface is required.
