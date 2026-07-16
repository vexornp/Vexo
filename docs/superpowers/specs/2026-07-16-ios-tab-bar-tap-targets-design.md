# iOS-Style Tab Bar Tap Targets — Design

**Date:** 2026-07-16
**Status:** Approved (pending spec review)
**Scope:** `vexo/src/widgets/gesture_detector.rs`, `vexo_uikit/src/tab_bar.rs`, `shared_app/src/app.rs`

## Problem

The demo app's tab bar (`vexo_uikit/src/tab_bar.rs:166-178`) lays out its items with
`Flex::row()` + `JustifyContent::SpaceBetween` + `width_percent(1.0)`. Each item is
sized to its *intrinsic* content width (icon 22px + label + 8pt padding), then the
items are spread across the bar: first flush-left, last flush-right, with **dead
(non-tappable) space between them**. Tapping between two icons does nothing.

This diverges from iOS `UITabBar`, where:

1. **Bigger click area** — each tab owns an equal, fully-tappable fraction of the
   bar (`bar_width / N` × full bar height). No dead space between items; tapping
   between icons still selects the nearest tab.
2. **Visual edge gap** — the icon+label content is *centered* within each slot, so
   the first/last items' visible content appears inset from the screen edge, even
   though the tap target reaches edge-to-edge.
3. **Fixed bar height** — 49pt (iOS standard), independent of content size.

## Goal

Bring the demo app's tab bar in line with iOS:

- Each tab item's tap target = full slot (`bar_width / N` × 49pt).
- No dead space between items.
- Icon+label visually centered within each slot (edge gap emerges naturally; no
  special-casing first/last items).
- Fixed 49pt bar height.

## Approach

Make `GestureDetector` a flex citizen by giving it a configurable `Layout` field,
then use equal-width slots in the tab bar.

### Why a framework change is required

`GestureDetector` is currently pass-through with a *hardcoded* layout
(`gesture_detector.rs:412-416`):

```rust
let layout = Layout::default()
    .flex_direction(FlexDirection::Column)
    .align(AlignItems::Stretch);
```

It has no `flex_grow`, so on its own it cannot claim an equal slot width, and it
cannot center content vertically in a fixed-height slot. Approach B (fixed bar
height + full slot tap target) requires the detector to participate in flex
sizing on both axes — hence the framework change below.

### How the layout resolves once the change is in

- Bar row is `Flex::row()` with `AlignItems::Stretch` (default for `Flex::row`,
  see `widgets/container.rs:50`) and `height(49.0)`. Stretch grows each child to
  the bar's full 49pt height.
- Each item is a `GestureDetector` with `flex_grow(1.0)` → equal share of bar
  width = `bar_width / N`.
- The detector's own `Column + Stretch + justify(Center)` layout stretches its
  child (the icon+label Column) to the full slot width and centers it
  vertically within the 49pt slot.
- The detector's `LayoutResult.size: zero` (pass-through) is discarded by the
  layouter (`layouter.rs:139`); the parent's flex resolution wins, so the
  detector's hit-test bounds = full slot.
- The content Column's `align(Center)` centers icon+label horizontally within
  the slot → first/last items' content sits ~40-45px from the screen edge on a
  390px-wide screen with 3 tabs. Tapping the very edge still works.

## Changes

### 1. `vexo/src/widgets/gesture_detector.rs` — configurable layout

Add a `layout: Layout` field to `GestureDetector`:

- **Default** preserves today's behavior exactly:
  `Layout::default().flex_direction(Column).align(AlignItems::Stretch)`.
  Every existing `.on_press()` caller (via the `Widget::on_press` trait method
  on `Box<dyn Widget>`) is unaffected.
- **Builder**: add an inherent method
  `pub fn with_layout(mut self, layout: Layout) -> Self` that replaces the
  field. Inherent dispatch shadows the `Widget::with_layout` trait method on
  the concrete type, so chaining works without trait disambiguation:
  ```rust
  GestureDetector::new(content)
      .on_press(move || ctrl.switch_to(tab_clone))
      .with_layout(Layout::default()
          .flex_direction(FlexDirection::Column)
          .align(AlignItems::Stretch)
          .flex_grow(1.0)
          .justify(JustifyContent::Center))
      .boxed()
  ```
- **`GestureDetectorRenderObject`**: add a `layout: Layout` field, set via
  a new constructor `GestureDetectorRenderObject::new_with_layout(Layout)`.
  `create_render_object` passes `self.layout.clone()` into it. `layout()`
  uses `self.layout` instead of the hardcoded `Layout`.
- **Render-object lifecycle note**: Vexo is retain-mode — the render object
  persists across rebuilds; only `create_render_object` runs at mount. The
  widget currently has no `update_render_object` override (uses the default
  `UpdateResult::NONE`). For this design's use case (tab bar always passes a
  constant layout on every rebuild), storing the layout at mount time is
  sufficient — the layout never changes, so no RO update is needed.
  Adding `update_render_object` for hot-reload of a changing layout is a
  straightforward future enhancement (clone the new layout into the RO,
  return `UpdateResult::LAYOUT` on change) but is **out of scope** here.

### 2. `vexo_uikit/src/tab_bar.rs` — equal-width 49pt slots

In `TabBarView::render()`, replace the items-row construction:

**Before** (`tab_bar.rs:166-178`):

```rust
let mut bar = Flex::row().layout(
    Layout::default()
        .justify(JustifyContent::SpaceBetween)
        .width_percent(1.0),
);
for tab in &self.tabs {
    let is_selected = *tab == self.controller.current();
    let ctrl = self.controller.clone();
    let tab_clone = tab.clone();
    let item = (self.tab_bar_builder)(tab, is_selected)
        .on_press(move || ctrl.switch_to(tab_clone.clone()));
    bar = bar.push(item);
}
```

**After**:

```rust
let mut bar = Flex::row()
    .layout(Layout::default().width_percent(1.0))
    .height(49.0); // iOS standard tab bar height
for tab in &self.tabs {
    let is_selected = *tab == self.controller.current();
    let ctrl = self.controller.clone();
    let tab_clone = tab.clone();
    let content = (self.tab_bar_builder)(tab, is_selected);
    let item = GestureDetector::new(content)
        .on_press(move || ctrl.switch_to(tab_clone.clone()))
        .with_layout(Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .flex_grow(1.0)             // equal share of bar width
            .justify(JustifyContent::Center)) // center content vertically
        .boxed();
    bar = bar.push(item);
}
```

Notes:
- Drop `JustifyContent::SpaceBetween` — equal-width slots replace it.
- Set explicit `height(49.0)` on the bar row.
- The `SafeArea` wrapping and the top hairline stay unchanged: hairline (1pt)
  above, 49pt items row, home-indicator inset below.

### 3. `shared_app/src/app.rs` — drop redundant content padding

The `tab_bar_builder` (`app.rs:55-73`) currently returns:

```rust
Column::new()
    .gap(2.0)
    .align(AlignItems::Center)
    .push(Icon::new(icon).with_size(22.0).with_color(color))
    .push(Text::new(label).with_font_size(11.0).with_color(color))
    .boxed()
    .padding(8.0)
```

The trailing `.padding(8.0)` was the only thing giving the item any height under
the old `SpaceBetween` layout. With the 49pt slot + `justify(Center)`, vertical
centering is handled by the slot, and the padding would double-pad. Drop it:

```rust
Column::new()
    .gap(2.0)
    .align(AlignItems::Center)
    .push(Icon::new(icon).with_size(22.0).with_color(color))
    .push(Text::new(label).with_font_size(11.0).with_color(color))
    .boxed()
```

Content intrinsic height ≈ 22 (icon) + 2 (gap) + ~13 (11pt text + line height) =
~37pt, centered in 49pt → ~6pt breathing room top and bottom.

## Testing

### Framework unit test (`gesture_detector.rs`)

Add a test that a `GestureDetector` constructed with a custom `Layout`
(`flex_grow(1.0)`, `justify(Center)`) produces a layout node whose Taffy style
reflects those properties. Verify by inspecting the render object's stored
`layout` field (or the Taffy node style after `layout()`).

### Tab bar tests (`tab_bar.rs`)

Extend the existing `ThreeTreePipeline`-based tests:

1. **Slot geometry**: after `layout(Size::new(390.0, 600.0))`, each item's
   render-object bounds = `{width: 130.0, height: 49.0}` (390/3 × 49),
   positions `x = 0 / 130 / 260`, `y` near the bottom of the screen (above
   the hairline + home-indicator inset, which is 0 on desktop/tests).
2. **No dead space**: item widths sum to 390.0.
3. **Bigger tap target**: simulate `InputEvent::PointerButton::Pressed` at a
   point *between* two icons (e.g., `x = 110.0`, within slot 0 but not over
   the icon) and assert tab 0's `switch_to` fires (controller's current
   becomes tab 0). A symmetric test for the slot-1/slot-2 boundary confirms
   edge behavior.
4. **Hairline still paints**: existing `test_tab_bar_top_hairline_paints`
   must still pass (its `400×600` window, 390+px-wide divider at the seam).
5. **Active page renders**: existing `test_tab_bar_view_renders_active_page`
   must still pass.

### Runtime verification

Per `CLAUDE.md`, do **not** run `cargo run -p desktop_demo` from the agent
session. Instead:

- Add `log::debug!("[tabbar]")` of each item's computed bounds and the
  hit-test result during pointer events.
- Give the user the run command:
  `RUST_LOG=debug cargo run -p desktop_demo 2>&1 | grep '\[tabbar\]' | tee tabbar.log`
- Ask the user to tap at slot edges and between icons; confirm the log shows
  the correct tab selected every time.

## Scope / Non-Goals

- No change to `TabController`, `IndexedStack`, page builders, or the nav bar.
- No animation or selection-indicator changes (selected state stays color-only).
- Not adding landscape notch side-insets beyond what `SafeArea` already does.
- 49pt is hardcoded (matching iOS); not parameterized yet (YAGNI).
- `GestureDetector`'s new `Layout` field is set only via the inherent builder;
  the `Widget::on_press` trait method still produces a default-layout
  detector (no API breakage).

## Risks

- **Existing `.on_press()` callers**: any caller relying on the detector's
  hardcoded `Column + Stretch` layout continues to work because the default
  is identical. No call sites need updating except `tab_bar.rs`.
- **Pass-through size zero**: the detector's `LayoutResult.size: zero` is
  already discarded by the layouter (`layouter.rs:139`); the parent's flex
  resolution governs the final size. Confirmed by reading the layouter —
  `apply_layout_recursive` reads computed bounds from Taffy, not from
  `LayoutResult.size`.
- **Hit-test bounds**: `GestureDetectorRenderObject::hit_test` uses
  `self.computed_bounds` set in `apply_layout`, which reflects Taffy's
  resolved size (full slot). No change needed.
