# Button Disabled State: `.opacity()` Modifier Instead of Color Alpha

**Date:** 2026-06-30
**Scope:** `vexo_uikit/src/button.rs`, `vexo_uikit/src/theme/tokens.rs`, `vexo_uikit/tests/token_tests.rs`

## Motivation

The `Button` component currently renders its disabled state by computing a
per-color alpha multiplier (`DISABLED_ALPHA = 0.5`) and applying
`Color::with_alpha()` separately in three resolve helpers (`resolve_bg`,
`resolve_border`, `resolve_text_color`). This duplicates the alpha logic across
three sites and ties a presentation concern (overall button dimming) to per-color
mutation.

vexo already provides an `Opacity` widget and a `.opacity(value)` modifier
(`vexo/src/widgets/opacity.rs`) that wraps a subtree and applies an alpha
multiplier to the whole painted output via `PushOpacity`/`PopOpacity` render
commands. The modifier is paint-only — layout and hit-testing pass through to
the child — so it does not affect the button's gesture handling.

Switching to `.opacity()` consolidates the dimming concern into a single
declarative modifier and aligns with Vexo's "everything is a widget" design
philosophy: a paint-time visual concern is expressed as a widget, not as
imperative color mutation.

### Behavior fix

A side effect of the switch: disabled buttons will now fade **uniformly** —
background, border, **and text** all drop to 0.5 opacity. Under the current
implementation, text stays at full opacity because `resolve_text_color`'s
faded value is computed but never applied (existing `// TODO: apply text_color`
at `button.rs:209`). Uniform dimming is the intended, more correct "disabled"
appearance and resolves that TODO's gap as a consequence of the refactor.

## Design

### Wrapper strategy

The `Opacity` wrapper is **always present** in the button's widget tree, with
value `DISABLED_ALPHA` (0.5) when disabled and `1.0` when enabled.

Rationale: `disabled` is frequently toggled at runtime (e.g., a submit button
that enables after a form becomes valid). A stable tree shape preserves
element/render-object reconciliation across disabled-state transitions. The cost
when enabled is one extra `OpacityElement` + `OpacityRenderObject` per button
and two no-op `PushOpacity { opacity: 1.0 }` / `PopOpacity` commands per frame,
which the command processor handles without altering color alphas
(`command_processor.rs:180-186`).

Rejected alternative: conditionally wrap only when disabled. Avoids the
enabled-state overhead but forces a subtree remount on every disabled toggle,
losing reconciliation efficiency. Not worth the trade for a value that changes
at runtime.

### `render()` change

Append `.opacity(opacity)` as the outermost modifier, after `.on_exit()`:

```rust
let opacity = if self.disabled { tokens::button::DISABLED_ALPHA } else { 1.0 };
// ...
text.boxed()
    .on_press(move || { /* ... */ })
    .on_release(move || { /* ... */ })
    .on_enter(move || { /* ... */ })
    .on_exit(move || { /* ... */ })
    .opacity(opacity)
```

The opacity modifier wraps the entire button subtree, so the dimming applies to
background, border, and text uniformly.

### Resolve helper simplifications

**`resolve_bg`** — remove the `alpha` local and the trailing `.with_alpha(alpha)`
call. Return the full-opacity base color directly:

```rust
fn resolve_bg(&self, is_pressed: bool, is_hovered: bool) -> Color {
    match self.variant {
        ButtonVariant::Primary => {
            if is_pressed {
                tokens::button::PRIMARY_BG_PRESSED
            } else if is_hovered && self.effective_platform() == Platform::Desktop {
                tokens::button::PRIMARY_BG_HOVER
            } else {
                tokens::button::PRIMARY_BG
            }
        }
        ButtonVariant::Secondary => tokens::button::SECONDARY_BG,
        ButtonVariant::Destructive => {
            if is_pressed {
                tokens::button::DESTRUCTIVE_BG_PRESSED
            } else if is_hovered && self.effective_platform() == Platform::Desktop {
                tokens::button::DESTRUCTIVE_BG_HOVER
            } else {
                tokens::button::DESTRUCTIVE_BG
            }
        }
        ButtonVariant::Ghost => tokens::button::GHOST_BG,
    }
}
```

**`resolve_border`** — remove the `alpha` local. For `Secondary`, return
`tokens::button::SECONDARY_BORDER` directly; other variants unchanged
(`(Color::TRANSPARENT, 0.0)`):

```rust
fn resolve_border(&self) -> (Color, f32) {
    match self.variant {
        ButtonVariant::Secondary => (tokens::button::SECONDARY_BORDER, 1.0),
        _ => (Color::TRANSPARENT, 0.0),
    }
}
```

**`resolve_text_color`** — remove the `alpha` local and `.with_alpha(alpha)`.
Return the base color directly. This helper remains unused in `render()` (the
pre-existing `// TODO: apply text_color` at `button.rs:209` is unchanged by this
work); simplifying it here keeps the three helpers consistent and removes the
now-misleading per-color alpha logic from all three.

### Token rename (optional, recommended)

Rename `DISABLED_ALPHA` → `DISABLED_OPACITY` in
`vexo_uikit/src/theme/tokens.rs:27`. The value now feeds an opacity modifier
rather than a color alpha, so the new name is more accurate.

Updates required:
- `vexo_uikit/src/theme/tokens.rs:27` — rename constant.
- `vexo_uikit/src/button.rs` — update the single reference site (the new
  `opacity` local in `render()`); the resolve helpers no longer reference it
  after the simplifications above.
- `vexo_uikit/tests/token_tests.rs:11` — update the test
  `button_disabled_alpha_is_half` (rename test to
  `button_disabled_opacity_is_half`, update the constant reference).

If the rename is declined to minimize churn, leave the name as
`DISABLED_ALPHA`; the refactor is still correct, just less self-documenting.

## Testing

### Existing tests (must continue to pass)

- `vexo_uikit/tests/button_render_tests.rs`:
  - `button_primary_render_does_not_panic`
  - `button_disabled_render_does_not_panic`
  - `button_hover_state_render_does_not_panic`
  - `button_all_variants_render`
  These render-no-panic tests exercise the same code paths and are unaffected
  by the dimming mechanism swap.
- `vexo_uikit/tests/button_tests.rs`:
  - `button_disabled_does_not_fire_callback` — disabled callback suppression
    lives in the gesture handlers, not in the resolve helpers, so it is
    unaffected.
- `vexo_uikit/tests/token_tests.rs` — updated for the rename (if applied).

### New test (optional, deferred)

A render test asserting the outermost widget produced by `Button::render()` is
an `Opacity` with the expected value (0.5 disabled, 1.0 enabled). Requires a
widget-downcast path that is not currently exposed. Deferred — the existing
render-no-panic tests cover the smoke path, and visual verification is the
authoritative check for opacity. Flag if you want this added.

### Manual verification

Run `cargo run -p desktop_demo` and visually confirm:
- Enabled buttons render at full opacity across all four variants.
- Disabled buttons render with uniform 0.5 dimming across bg, border, and text.
- Hover/press states still apply their dedicated colors (those colors are then
  dimmed by the opacity wrapper when disabled, which is correct — a disabled
  button does not receive hover/press, so this only matters if state changes
  overlap a disabled toggle).

## Out of scope

- Applying `resolve_text_color` to the `Text` widget (separate TODO at
  `button.rs:209`; not introduced by this work and not required to ship the
  uniform-dimming fix, since `.opacity()` now handles text dimming directly).
- Disabled-state treatment in other `vexo_uikit` components. Audit confirms
  `Button` is the only component using `Color::with_alpha()` for disabled
  dimming in `vexo_uikit`.
- Any change to the `Opacity` widget, `OpacityRenderObject`, or the
  `PushOpacity`/`PopOpacity` command processor. The existing implementation
  fully supports this use case.
