# shared_app Demo: Button Showcase

**Date:** 2026-06-30
**Scope:** `shared_app/src/lib.rs`

## Motivation

The `shared_app` demo currently renders 20 placeholder "Row N" text rows
(`shared_app/src/lib.rs:18-26`). It exercises `Column` and `Text` but does not
showcase any `vexo_uikit` component, despite `shared_app` depending on
`vexo_uikit` (`shared_app/Cargo.toml:15`).

The Button component — now refactored to use the `.opacity()` modifier for its
disabled state — is the natural showcase target. It exercises: all four visual
variants, the `Component` + `Signal` reactivity path, the `on_press` callback
wiring, and the disabled-state dimming we just shipped.

## Design

### State

```rust
#[derive(ComponentState, Default)]
pub struct State {
    count: Signal<u32>,
}
```

Single shared counter, auto-wired by `#[derive(ComponentState)]`. The placeholder
`_placeholder: ()` field is removed.

### View

`Application::view` produces a `Column` with the following children, in order:

1. Title `Text`: `"Button Showcase"` via `Text::new(...).with_font_size(32.0)`
   (default is 24.0; larger size gives the title visual prominence — vexo `Text`
   has no font-weight API, so size is the only differentiation lever).
2. Subtitle `Text`: `format!("Pressed: {} times", count)` — updates reactively
   on each press.
3. Four enabled buttons, one per variant, each with `.on_press(increment)`:
   - `Button::new("Submit").variant(ButtonVariant::Primary)`
   - `Button::new("Cancel").variant(ButtonVariant::Secondary)`
   - `Button::new("Delete").variant(ButtonVariant::Destructive)`
   - `Button::new("More").variant(ButtonVariant::Ghost)`
4. One disabled Primary button to showcase the `.opacity()` dimming:
   - `Button::new("Submit").variant(ButtonVariant::Primary).disabled(true)`

### Layout

```rust
Column::new()
    .gap(16.0)
    .padding(24.0)
    .background(Color::WHITE)
    .push(title)
    .push(subtitle)
    .push(primary_button)
    .push(secondary_button)
    .push(destructive_button)
    .push(ghost_button)
    .push(disabled_button)
    .boxed()
```

A 16px gap separates every child. 24px padding insets the whole column from the
window edges. White background ensures the disabled button's 0.5 opacity is
visible against a clean surface.

### Callback wiring

The `count` Signal is `Clone` (Rc-backed). Each enabled button's `on_press`
closure clones `count` and increments:

```rust
let count = state.count.clone();
// ...per button:
let count_for_this = count.clone();
Button::new("Submit")
    .variant(ButtonVariant::Primary)
    .on_press(move || {
        count_for_this.set(count_for_this.get() + 1);
    })
```

Because `view` borrows `&mut State`, the Signal is cloned out of state before
building closures; each closure owns its own clone.

### Reactivity

`Signal::set` marks the `Component` dirty. The framework's `BuildOwner` +
`DirtyTracking` re-runs `view()` on the next frame, producing a new subtitle
`Text` with the updated count. This is the existing reactivity model — no new
mechanism is introduced.

The buttons themselves are stateless from the demo's perspective (their internal
hover/press Signals are managed by `ButtonState`); only the subtitle text changes
between presses.

### Imports

`shared_app/src/lib.rs:1` currently imports:
```rust
use vexo::{Application, Color, Column, ComponentState, Text, Widget};
```

Add:
```rust
use vexo_uikit::{Button, ButtonVariant};
```

`Button` is a `Component` (implements `vexo::Widget` via the `Component` trait's
blanket impl), so it can be pushed into a `Column` directly — no `.boxed()`
needed at the call site because `Column::push` accepts `impl Widget`.

## Testing

### Build verification

`cargo build -p shared_app` must compile. This is the primary gate — the demo
is a thin view function with no new logic.

### Manual verification

Run `cargo run -p desktop_demo` and confirm:
- Title and subtitle render at the top.
- Five buttons render in a vertical column with consistent spacing.
- Clicking any of the four enabled buttons increments the subtitle counter.
- The disabled "Submit" button renders at 0.5 opacity (faded bg + text) and does
  not increment the counter when clicked.
- Hover/press states apply their dedicated colors on the enabled buttons.

No automated test is added. The demo is a visual smoke surface; existing
`vexo_uikit` tests cover Button behavior. Adding a render test for the demo
view would couple the demo to internal widget structure and is not worth the
maintenance cost.

## Out of scope

- ScrollView wrapper (5 buttons + 2 text lines fit any reasonable viewport).
- Per-button counters (single shared counter chosen for simplicity).
- Disabled variants for Secondary/Destructive/Ghost (Primary disabled is
  sufficient to demonstrate the `.opacity()` dimming against a filled bg).
- Platform override (uses `Platform::current()` automatically via
  `Button::effective_platform`).
- Any change to the `MobileApp` UniFFI wrapper at `lib.rs:31-48` — unaffected.
