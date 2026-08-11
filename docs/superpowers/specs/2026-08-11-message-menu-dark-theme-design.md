# Message Context Menu Dark Theme Support Design

Date: 2026-08-11
Status: Draft

## Problem

When the user toggles the appearance picker
(`shared_app/src/me/profile_screen.rs:358-401`) to Dark, the message-bubble
context menu (`shared_app/src/chats/message_menu.rs`) does **not** re-theme:
the reactions pill and the actions card stay white-background with black text,
regardless of the ambient dark theme.

### Root Cause

The bug is a widget **wrap-order mistake** in
`shared_app/src/app.rs:157-164`:

```rust
let themed = Theme::new(theme, inner).boxed();
ContextMenu::new(themed, state.context_menu.clone()).boxed()
```

This puts `ContextMenu` as the **parent** of `Theme`. But
`ContextMenu::render` reads the theme via `Theme::of(ctx)` at
`vexo_uikit/src/context_menu.rs:422`:

```rust
let theme = vexo::Theme::of(ctx);
// ...
let content = builder(&controller, &theme);
```

`Theme::of` walks **up** the widget tree to ancestors only
(`vexo/src/widgets/theme.rs:128-131`,
`vexo/src/stateful_widget.rs:365-378`). Since `Theme` is a **child**
(descendant) of `ContextMenu` — not an ancestor — the lookup finds no
provider and falls back to `ThemeData::light()`
(`vexo/src/widgets/theme.rs:130`). The builder therefore always receives
the light theme, producing:

- `theme.surface` = `0xFFFFFFFF` (white) for the pill/card backgrounds
  (`message_menu.rs:478, 520`)
- `theme.on_surface` = `0x1C1B1FFF` (near-black) for the Copy/Reply/Delete
  text (`message_menu.rs:129, 139`)
- `theme.outline` = `0xC7C7CCFF` (light gray) for the borders
  (`message_menu.rs:479, 521`)

…regardless of dark mode.

### Why the existing test didn't catch it

`test_builder_reads_current_theme` (`vexo_uikit/src/context_menu.rs:1009-1100`)
uses the **opposite** (correct) wrap order —
`Theme::new(theme, ContextMenu::new(...))` — so it passes, but it does not
reflect production's tree. The framework test validates correct framework
behavior under correct usage; it does not validate the app's integration.

### Secondary issue (cosmetic)

Even after fixing the wrap order, both menu surfaces use a hardcoded
`Color::BLACK.with_alpha(0.20)` drop shadow
(`message_menu.rs:481` reactions pill, `:523` actions card). In dark mode,
`theme.surface` is `0x1C1C1E` (near-black); a 20%-alpha black shadow
composited over near-black is effectively invisible. The elevation cue the
shadow provides in light mode is lost. (Same class of issue as the
documented Send-button shadow in the input-bar dark-mode spec.)

## Goal

1. Make the message menu inherit the ambient theme — light **or** dark.
2. Ensure the menu looks polished (visible separation) in dark mode.
3. Add a regression test using the **production** wrap order so this can't
   recur.

## Non-Goals

- **Defensive re-reading.** Not making `message_menu` widgets call
  `Theme::of(ctx)` themselves (Approach 3). The builder-snapshot contract is
  correct when the wrap order is correct; adding belt-and-suspenders reads
  complicates the builder contract without meaningful benefit.
- **Send-button shadow.** `chat_screen.rs:357` has the same dark-mode
  shadow issue but is out of scope for this menu fix. Noted as a follow-up.
- **OS-driven theme.** Theme remains app-state-driven via the
  `is_dark: Signal<bool>` in `shared_app/src/data.rs:119`. Not changing
  this.
- **Reaction semantic colors.** The `reaction_visual` palette
  (`message_menu.rs:29-38`) is intentional and correct in both modes
  (full-saturation glyph on surface). Not touched.
- **Touching the framework test.** `test_builder_reads_current_theme` stays
  as-is — it's a correct framework test, just testing a different thing.

## Approach

Three approaches were considered:

1. **Minimal — invert wrap order only.** Smallest diff. Fixes the bug but
   leaves the shadow invisible in dark mode. No regression test.
2. **Fix + shadow tuning + regression test — *chosen.*** Inverts the wrap
   order, makes shadows theme-aware, adds a production-wrap-order regression
   test. Complete and polished.
3. **Fix + defensive re-reading.** Approach 2 plus making `message_menu`
   widgets call `Theme::of(ctx)` themselves. Belt-and-suspenders against
   future wrap mistakes, but more invasive — changes the builder contract
   for a single tree-ordering bug.

**Why Approach 2:** the wrap-order inversion is the necessary root-cause
fix; the shadow tuning ensures the menu looks polished (not just functional)
in dark mode; the regression test prevents recurrence. Approach 3 adds
complexity without meaningful benefit since the wrap order is the single
point of failure.

## Architecture

Three changes, all small. No new modules, no new public types.

### Change 1 — Invert the wrap order (root cause)

**File:** `shared_app/src/app.rs:157-164`

Swap the two wraps so `Theme` is the **outer** widget:

```rust
// Before (broken):
let themed = Theme::new(theme, inner).boxed();
ContextMenu::new(themed, state.context_menu.clone()).boxed()

// After (fixed):
let menu_host = ContextMenu::new(inner, state.context_menu.clone());
Theme::new(theme, menu_host).boxed()
```

**Why this is safe:** `Theme` is an `InheritedWidget` backed by a
`ProxyRenderObject` (`vexo/src/inherited_widget.rs:119` via the
`impl_widget_for_inherited!` macro at `:105-147`). The proxy is
layout-pass-through — it forwards layout and paint to its single child
unchanged. So `ContextMenu`'s `Stack` still fills the window and
`Positioned::left(click_x).top(click_y)` still maps to window-logical
coords. The full-window coordinate mapping that the existing comment
(app.rs:159-163) cares about is preserved; only the inherited-widget
propagation direction is corrected.

**Comment rewrite:** rewrite the comment at `app.rs:159-163` to explain
*why* `ContextMenu` must be inside `Theme`, so a future reader doesn't
re-invert it. Something like:

> `ContextMenu` must be a descendant of `Theme` so its `render()` reads the
> live theme via `Theme::of(ctx)`. `Theme` is layout-pass-through
> (`ProxyRenderObject`), so `ContextMenu`'s `Stack` still fills the window
> for window-local coordinate mapping (`Positioned::left(click_x).top(
> click_y)`).

### Change 2 — Theme-aware menu card style (cosmetic)

**File:** `shared_app/src/chats/message_menu.rs`

Add a local helper next to the existing local helpers (`close_after`,
`reaction_visual`):

```rust
/// Build the card chrome `Style` for a menu surface (pill or card).
/// In light mode: surface bg + outline border + black drop shadow.
/// In dark mode:  surface bg + outline border only — the border already
/// provides separation against the dark backdrop, and a black shadow
/// would be invisible against near-black surface anyway (Material dark-
/// mode guidance: de-emphasize shadows).
fn menu_card_style(theme: &vexo::ThemeData, corner_radius: f32) -> Style {
    let style = Style::default()
        .corner_radius(corner_radius)
        .background(theme.surface)
        .border(theme.outline, 1.0);
    if theme.is_dark() {
        style
    } else {
        style.shadow(
            BoxShadow::new(Color::BLACK.with_alpha(0.20))
                .blur(12.0)
                .offset(0.0, 4.0),
        )
    }
}
```

Both `reaction_pill` (currently `message_menu.rs:474-486`) and
`actions_card` (currently `:516-528`) replace their hand-built
`Style::default()...` block with `menu_card_style(&theme, 18.0)` /
`menu_card_style(&theme, 12.0)` respectively.

**Why drop rather than lighten in dark mode:**

- A "lighter" (white-ish) shadow on dark surfaces creates a halo — looks
  wrong.
- Material dark-mode guidance explicitly de-emphasizes shadows; elevation
  comes from surface tint + borders.
- The menu already has a `theme.outline` border (`0x49454F` against
  `0x1C1C1E` surface in dark) — clearly visible separation without a
  shadow.
- Reducing alpha (e.g., `0.10`) is effectively the same as dropping it
  (still invisible) but leaves dead code.

This mirrors the `navigation::colors` token pattern
(`vexo_uikit/src/theme/tokens.rs:83-128`) which branches on `t.is_dark()`
for values that need a different source color in dark mode. Here it's a
local helper rather than a `tokens::*` function because `message_menu` is
app-specific, not a reusable widget — matches the existing call-site-passes-
`&theme` pattern used throughout `chat_screen.rs`.

### Change 3 — Regression test

**File:** `shared_app/src/chats/message_menu.rs` (in `#[cfg(test)] mod tests`)

Add `test_message_menu_inherits_dark_theme`:

1. Build the app's actual top-level tree shape —
   `Theme::new(ThemeData::dark(), ContextMenu::new(inner, controller))` —
   matching the **fixed production wrap**. This is the key difference from
   the framework test, which uses the opposite order.
2. Open the menu via `controller.show(point, builder(0, callback))`.
3. Settle the open spring (sleep + tick + rebuild), mirroring the existing
   `test_metrics_match_real_sizes` setup at `message_menu.rs:601-629`.
4. Walk the render tree to find the actions card by reusing the existing
   `find_decorated_box_by_corner_radius` helper (`message_menu.rs:548`)
   with `radius = 12.0`.
5. Assert the card's `Style.background` equals `ThemeData::dark().surface`
   (`0x1C1C1EFF`).

This mirrors the existing dark-mode regression test
`test_chat_screen_input_bar_uses_theme_colors` (`chat_screen.rs:566-702`)
in spirit: wrap in dark theme, assert a theme-derived color on a render
object.

**Why assert on the card background specifically:** it's the single most
visible symptom (white bg vs dark bg) and is read directly from the
`DecoratedBoxRenderObject`'s `Style` — no ambiguity. A failing assertion
prints `real=0xFFFFFFFF expected=0x1C1C1EFF`, making the failure mode
obvious.

**Why not also fix `test_builder_reads_current_theme`:** that test is in
`vexo_uikit` and validates framework behavior with correct usage — it's not
wrong, it's just testing a different thing. The new test in `shared_app`
validates the app's actual integration. Leaving the framework test as-is
keeps the layering clean.

## Data Flow

```
User taps Dark in AppearancePicker
  └─ is_dark Signal flips true
       (shared_app/src/data.rs:119)
     └─ root view() re-runs
          (shared_app/src/app.rs:48-57)
       └─ ThemeData::dark() constructed
       └─ Theme::new(dark, menu_host) — Theme is now the OUTERMOST widget
            └─ ContextMenu::render runs
                 (vexo_uikit/src/context_menu.rs:417)
              └─ Theme::of(ctx) walks up, finds the wrapping Theme
                   → returns ThemeData::dark()   ✓ (was: light() fallback)
              └─ builder(&controller, &dark_theme)
                   (shared_app/src/chats/message_menu.rs:63-85)
                └─ reaction_pill / actions_card receive dark theme
                └─ menu_card_style(&dark_theme, …)
                     └─ dark branch: no shadow, surface bg, outline border
                └─ MenuRow::render uses dark theme.on_surface / .error
```

The theme-invalidation path bypasses `should_rebuild` entirely (state-driven
rebuild from a `Signal` flip) per the three-level ladder in `CLAUDE.md` —
same path the input-bar dark-mode fix uses.

## Testing

| Test | File | What it covers |
|---|---|---|
| `test_message_menu_inherits_dark_theme` (new) | `message_menu.rs` | Production wrap order + dark theme → card bg is `dark.surface` |
| `test_metrics_match_real_sizes` (existing) | `message_menu.rs:588` | Menu metrics unchanged by the refactor (still 222×40 pill, 200×98 card) |
| `test_builder_reads_current_theme` (existing) | `context_menu.rs:1009` | Framework-level theme toggle with correct wrap order |

The new test uses the existing `find_decorated_box_by_corner_radius` helper
and the existing pipeline-setup pattern — no new test infrastructure.

Manual verification: after the fix, run `cargo run -p desktop_demo`, open a
chat, right-click a bubble, then toggle Dark in the Me tab — the open menu
should re-theme live (the builder runs inside `ContextMenu::render`, which
re-runs on `Theme` invalidation).

## Follow-ups (out of scope)

- **Send-button shadow** (`chat_screen.rs:357`): same `Color::BLACK.with_
  alpha(0.25)` issue in dark mode. Same `menu_card_style`-style helper
  would fix it. Left for a separate change.
- **OS-driven theme** (`docs/superpowers/specs/2026-07-26-media-query-
  design.md:64` notes this as a future MediaQuery aspect): not changing
  the app-state-driven model.
