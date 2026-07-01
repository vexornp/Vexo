# Button Intrinsic Sizing & Padding Fix

**Date:** 2026-06-30
**Scope:** `vexo_uikit/src/button.rs`, `vexo/src/widgets/mod.rs`, `vexo/src/lib.rs`

## Motivation

The `Button` component in `vexo_uikit/src/button.rs` exhibits two visible bugs
in the `shared_app` demo (`shared_app/src/lib.rs`):

1. **Width stretch:** Buttons render at the full width of the parent `Column`,
   rather than sizing to their text content. A "Submit" button fills the column
   width identically to a "More" button despite the labels differing in length.

2. **No leading padding:** The button label sits flush against the left edge of
   the button background. The configured horizontal padding (Desktop 16px,
   Mobile 20px — `vexo_uikit/src/theme/tokens.rs:31,36`) is not visible.

### Root cause

Both bugs share one root cause: **visual decoration (background, padding,
border, corner_radius) is placed on the `Text` leaf widget instead of on a
container wrapping it.**

`Button::render()` (`vexo_uikit/src/button.rs:218-227`) builds the widget tree
as:

```
Opacity → MouseRegion ×2 → GestureDetector ×2 → Text(carries bg+padding+radius+border)
```

- **Stretch bug:** `Column::new()` defaults to `AlignItems::Stretch`
  (`vexo/src/widgets/container.rs:44`). None of the pass-through wrappers
  (`GestureDetector`, `MouseRegion`, `Opacity`) nor the `Text` leaf sets
  `align_self`. The cross-axis stretch cascades through every wrapper down to
  the `Text`, which fills the column width. By contrast,
  `DecoratedContainer::new()` defaults to `align_self(Start).flex_shrink(0.0)`
  (`vexo/src/widgets/decorated_container.rs:293`) — but `Button` does not use
  it.

- **Padding bug:** `padding_each(...)` is set on `Text`'s `Layout`
  (`button.rs:221`). But `TextRenderObject::paint()`
  (`vexo/src/render_objects/text.rs:168+`) draws the background over the full
  padded bounds, then positions the text glyphs at the top-left corner of those
  bounds (with only vertical centering at `text.rs:177-178`). The padding
  inflates the box but never offsets the glyphs inward — so the text sits at
  the left edge with the empty space on the right.

A secondary finding reinforces the stretch fix: even if a `DecoratedContainer`
were placed as the innermost widget, the pass-through wrappers above it would
still stretch to the column width. Their render objects all use
`Column + AlignItems::Stretch` internally
(`gesture_detector.rs:408-410`, `mouse_region.rs:411-413`,
`render_objects/opacity.rs:62-63`). So the visible button would be
intrinsic-sized, but the hover region and hit-test area would span the full
column — triggering hover/press in empty space. The **outermost** widget
returned by `Button::render()` must carry `align_self(Start)` to break the
stretch cascade at the top.

## Design

### Widget tree structure

Replace the innermost `Text`-with-modifiers with a `DecoratedContainer`
wrapping a plain `Text` leaf, and add `align_self(Start)` as the outermost
modifier.

**Before:**
```
Opacity
  └── MouseRegion (on_exit)
       └── MouseRegion (on_enter)
            └── GestureDetector (on_release)
                 └── GestureDetector (on_press)
                      └── Text  ← carries bg+padding+radius+border directly
```

**After:**
```
WithLayout(align_self=Start)          ← outermost; breaks Column stretch cascade
  └── Opacity
       └── MouseRegion (on_exit)
            └── MouseRegion (on_enter)
                 └── GestureDetector (on_release)
                      └── GestureDetector (on_press)
                           └── DecoratedContainer  ← carries bg+padding+radius+border
                                └── Text           ← plain leaf, no modifiers
```

### Why this fixes both bugs

- **Stretch:** `align_self(Start)` on the outermost widget overrides the
  parent Column's `AlignItems::Stretch`. The whole subtree sizes to the
  `Text`'s intrinsic width + padding + border. The hit-test and hover areas
  match the visible button.
- **Padding:** `padding_each` now lives on `DecoratedContainer`, a real
  Taffy container. Taffy insets the `Text` child by the padding, so glyphs
  render inside the padded area. The background paints over the full padded
  bounds.

### `Button::render()` body

The new render body, replacing `vexo_uikit/src/button.rs:194-247`:

```rust
fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
    let is_pressed = state.is_pressed.get();
    let is_hovered = state.is_hovered.get();

    let bg = self.resolve_bg(is_pressed, is_hovered);
    let (border_color, border_width) = self.resolve_border();
    let _text_color = self.resolve_text_color(is_hovered); // still unused; Text color TBD separately
    let corner_radius = self.resolve_corner_radius();
    let (pt, pr, pb, pl) = self.resolve_padding();
    let opacity = if self.disabled { tokens::button::DISABLED_OPACITY } else { 1.0 };

    let disabled = self.disabled;
    let on_press_cb = self.on_press.clone();
    let is_pressed_signal = state.is_pressed.clone();
    let is_pressed_signal_release = state.is_pressed.clone();
    let is_pressed_signal_exit = state.is_pressed.clone();
    let is_hovered_signal = state.is_hovered.clone();
    let is_hovered_signal_exit = state.is_hovered.clone();

    // Plain leaf — no modifiers on Text itself
    let text = Text::new(&self.label).with_font_size(24.0);

    // All decoration on the container (default align_self=Start, flex_shrink=0)
    let mut container = DecoratedContainer::new(text)
        .background(bg)
        .corner_radius(corner_radius)
        .padding_each(pt, pr, pb, pl);   // TRBL order (matches current call)

    if border_width > 0.0 {
        container = container.border(border_color, border_width);
    }

    container.boxed()
        .on_press(move || {
            if !disabled {
                is_pressed_signal.set(true);
                (on_press_cb.borrow_mut())();
            }
        })
        .on_release(move || {
            is_pressed_signal_release.set(false);
        })
        .on_enter(move || {
            if !disabled {
                is_hovered_signal.set(true);
            }
        })
        .on_exit(move || {
            is_hovered_signal_exit.set(false);
            is_pressed_signal_exit.set(false);
        })
        .opacity(opacity)
        .align_self(AlignSelf::Start)   // outermost; breaks Column stretch cascade
}
```

### Notable points

- `padding_each(pt, pr, pb, pl)` keeps the same TRBL call signature as today.
  `DecoratedContainer` inherits `padding_each` from `layout_builder_methods!()`
  (`vexo/src/widgets/decorated_container.rs:329`), which delegates to
  `Layout::padding_each(left, right, top, bottom)`. Token values are aligned
  to SwiftUI `.bordered` (regular control size): Desktop H=12/V=4, radius=5;
  Mobile H=16/V=8, radius=8.
- `.border()` on `DecoratedContainer`
  (`vexo/src/widgets/decorated_container.rs:337-350`) automatically adds
  border-width to padding so the child insets from the border. Existing
  behavior, now actually visible because the child is `Text` not
  "decoration-on-Text".
- `.align_self(Start)` is applied **last**, after `.opacity()`, so it wraps
  the outermost `WithLayout`. Order matters: it must be the outermost widget
  so the whole subtree escapes Column's stretch.
- The callback bodies are unchanged from the current implementation — only
  the widget structure around them changes.
- `with_font_size(self.resolve_font_size())` is platform-adaptive to match
  SwiftUI `.bordered` defaults: 13pt on macOS (system body), 17pt on iOS
  (system body). Sourced from `FONT_SIZE_DESKTOP` / `FONT_SIZE_MOBILE` tokens.

### vexo public API change

`DecoratedContainer` is currently `pub(crate)` (`vexo/src/widgets/mod.rs:37`).
To construct `DecoratedContainer::new(Text).background(..).padding_each(..)`
directly from `vexo_uikit`, it must be public.

**Two-line change:**

1. `vexo/src/widgets/mod.rs:37`:
   ```rust
   // before
   pub(crate) use decorated_container::DecoratedContainer;
   // after
   pub use decorated_container::DecoratedContainer;
   ```

2. `vexo/src/lib.rs:190` — add `DecoratedContainer` to the re-export:
   ```rust
   pub use widgets::{
       Column, DecoratedContainer, Flex, Grid, Image, Opacity, Row, ScrollView, Text,
       TextEdit, TextEditState, TextEditingController, Widget,
   };
   ```

No other vexo changes. `AlignSelf` is already exported at `vexo::AlignSelf`
(`vexo/src/lib.rs:49`). `GestureDetector` and `MouseRegion` stay `pub(crate)`
— `Button` reaches them via the `Widget::on_press`/`on_enter`/etc. trait
methods, which is the intended public API for behavioral modifiers.

### Imports in `vexo_uikit/src/button.rs`

Add to the existing imports:
```rust
use vexo::{AlignSelf, DecoratedContainer};
```

`vexo` is already a dependency of `vexo_uikit` (it imports `Color`,
`Component`, `ComponentState`, `RenderContext`, `Signal`, `Text`, `Widget`).

## Testing

### Unit test in `vexo_uikit/src/button.rs`

Build a `Button::new("Submit").variant(ButtonVariant::Primary)` and assert
the widget tree shape by downcasting via `Widget::as_any()` and traversing
via `Widget::child()`:

- Outermost widget is `WithLayout` with
  `layout.align_self == Some(AlignSelf::Start)`.
- Peeling `Opacity` → `MouseRegion` ×2 → `GestureDetector` ×2 yields a
  `DecoratedContainer` with:
  - `style.background == Some(expected_bg)` (e.g., `PRIMARY_BG`).
  - `layout.padding == Some(EdgeInsets { top, right, bottom, left })`
    matching the resolved platform tokens.
- `DecoratedContainer`'s child is `Text` with `style.background == None`
  (pure leaf).

Peeling is possible because `Widget::child()` and `Widget::as_any()` are
public, and after the visibility change `DecoratedContainer` is downcastable
from `vexo_uikit`.

### Existing tests stay green

- The `modifier_methods!()` tests in `vexo/src/macros.rs` still pass — `Text`
  still has those methods; we just don't call them from `Button`.
- The `DecoratedContainer` tests in
  `vexo/src/widgets/decorated_container.rs` still pass — no behavior change,
  only visibility.
- The `Button`-related tests in `vexo_uikit` (if any) still pass — `press()`
  and the public API are unchanged.

### Build verification

- `cargo build -p vexo` — confirms the public-API change compiles and does
  not break anything.
- `cargo build -p vexo_uikit` — confirms `Button::render()` compiles with the
  new structure.
- `cargo test -p vexo` and `cargo test -p vexo_uikit` — run existing plus new
  tests.

### Manual verification

Per `CLAUDE.md`, the assistant will not run `cargo run -p desktop_demo`
itself. After implementation, the user runs it and confirms:

- Buttons size to their text content + padding (not full column width).
- "Submit" is narrower than the column; "More" is narrower still.
- Leading padding is visible between the text and the left edge of the
  background.
- Trailing padding is also visible (text is left-aligned but not flush-right).
- Hover/press color changes only trigger when the pointer is inside the
  visible button bounds — not in the empty column space beside it.
- The disabled "Submit" button still renders at 0.5 opacity and does not
  respond to clicks.

## Error handling

None — pure widget composition change. No new failure modes, no I/O, no
fallible operations introduced.

## Out of scope

- **Text color:** `Button::resolve_text_color()` is still computed but unused
  (`_text_color` — `button.rs:201`). The `vexo` `Text` widget has no text
  color API yet. Wiring text color is a separate concern; this fix only
  addresses sizing and padding.
- **`TextRenderObject` honoring `layout.padding`:** `Text` still does not
  offset glyphs by its own `layout.padding`. This design avoids relying on
  that behavior by moving padding to `DecoratedContainer`. A future fix to
  `TextRenderObject` would let `Text` be decorated directly, but is not
  required here.
- **Changing pass-through wrapper defaults:** `GestureDetectorRenderObject`,
  `MouseRegionRenderObject`, and `OpacityRenderObject` still default to
  `Column + AlignItems::Stretch`. Changing those defaults would affect every
  consumer and is out of scope. The outermost `align_self(Start)` on
  `Button::render()`'s return value neutralizes them for the Button case.
- **Button font size:** Platform-adaptive via `resolve_font_size()` — 13pt
  on macOS, 17pt on iOS — matching SwiftUI `.bordered` regular control size.
- **Platform token values:** Aligned to SwiftUI `.bordered` regular control
  size: Desktop H=12/V=4, radius=5, font=13; Mobile H=16/V=8, radius=8,
  font=17.
- **`Text::new()` default (24.0):** Intentionally left untouched in
  `vexo/src/widgets/text.rs:29`; unrelated to Button's platform-adaptive
  sizing.
