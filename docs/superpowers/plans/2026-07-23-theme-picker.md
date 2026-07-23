# iOS-Style Theme Picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the sun/moon `ThemeToggle` (sidebar on desktop, single-row on Me page) with an iOS Display & Brightness-style two-cell picker on the Me page: each cell shows a widget-drawn theme preview, a "Light"/"Dark" label, and a checkbox; tapping a cell selects that mode. Desktop sidebar loses its toggle entirely.

**Architecture:** A new `AppearancePicker` `Component` lives inline in `profile_screen.rs`. Each of its two cells wraps an abstract swatch-stack preview in a *local* `Theme::new(ThemeData::light()/dark(), …)` so the preview always renders in its own mode regardless of the app's current theme. The cell's checkbox + label + card chrome use the ambient `Theme::of(ctx)`. Tapping a cell sets `is_dark: Signal<bool>` to a fixed bool (`false` for light, `true` for dark); the root `view()` cascade rebuilds the tree and swaps the ambient `Theme`. `DesktopShell` loses its `is_dark` field, spacer, and `ThemeToggle`; the `theme_toggle.rs` file and the two original theme-toggle design docs are deleted.

**Tech Stack:** Rust, Vexo framework (`Component`, `Signal`, `Theme`/`ThemeData` `InheritedWidget`, `DecoratedBox`, `GestureDetector`, `MultiChild`, `WithLayout`, `Layout`), `vexo_fontawesome` (`Icon`/`Icons::Check`).

## Global Constraints

- **No new files for picker code.** `AppearancePicker` is inline in `shared_app/src/me/profile_screen.rs`.
- **No new crate dependencies.** Uses existing `vexo`, `vexo_fontawesome`, `vexo_uikit`.
- **No comments in code** unless explicitly shown in a step (per repo convention in `CLAUDE.md`).
- **Metrics (exact values, verbatim):** preview `120×80`; preview `corner_radius(6.0)` + `theme.outline` border `1.0`; swatch bands — header `surface_variant` height `16.0`, content `surface` (fills remaining ~48pt) with `primary` accent rect `24×8` left-inset `12.0` vertically centered + `outline` divider `1.0` full-width near bottom, bottom `surface_variant` height `16.0`; label `15.0pt` `theme.on_background`; checkbox `22×22` `corner_radius(6.0)`; cell padding `12.0` all sides; vertical gap inside cell `8.0`; gap between the two cells `8.0`; cells `flex_grow(1.0)` for 50/50 split.
- **Previews render in their LOCAL theme** (hardcoded `ThemeData::light()` / `ThemeData::dark()`), not the ambient theme. Card chrome (background, label, checkbox, border) uses the AMBIENT `Theme::of(ctx)`.
- **Tapping a cell sets a fixed bool.** Light cell: `is_dark.set(false)`. Dark cell: `is_dark.set(true)`. No toggle.
- **Whole cell is the tap target** — one `GestureDetector` per cell wrapping preview + label + checkbox.
- **No divider between the two cells.** No per-preview borders (the preview's own `outline` frame is the only border).
- **`AppearancePicker` uses `SimpleState<()>`** and relies on the root cascade for rebuilds (same pattern as the deleted `ThemeToggle`).
- **Commit messages:** no "Co-Authored-By" attribution (per `CLAUDE.md`).
- **Never run `cargo run -p desktop_demo`** — ask the user (per `CLAUDE.md`).

---

## File Structure

**Modify:**
- `shared_app/src/me/profile_screen.rs` — delete `build_toggle_row`; add `AppearancePicker` component + `build_swatch_preview` + `build_checkbox` helpers; update `ProfileScreen::render` to use the picker; add picker test; bump element-count threshold.
- `shared_app/src/desktop_shell.rs` — remove `ThemeToggle` import, `is_dark` field, `Clone` arm, spacer, toggle push in `build_sidebar`.
- `shared_app/src/app.rs` — remove `is_dark_signal` clone/arg threaded into `DesktopShell` (the Me-tab page builder still needs it); keep `is_dark` for the ambient theme swap.
- `shared_app/src/widgets/mod.rs` — remove `pub(crate) mod theme_toggle;` line.
- `shared_app/src/data.rs` — update `is_dark` doc comment (lines 77-78) to reference `AppearancePicker`.

**Delete:**
- `shared_app/src/widgets/theme_toggle.rs`
- `docs/superpowers/plans/2026-07-13-theme-toggle.md`
- `docs/superpowers/specs/2026-07-13-theme-toggle-design.md`

---

## Task 1: Delete the `ThemeToggle` widget file and mod entry

**Files:**
- Delete: `shared_app/src/widgets/theme_toggle.rs`
- Modify: `shared_app/src/widgets/mod.rs:4`

**Interfaces:**
- Consumes: nothing.
- Produces: `theme_toggle` module is gone; downstream tasks remove all `use crate::widgets::theme_toggle::ThemeToggle;` imports.

- [ ] **Step 1: Delete the file**

```bash
rm shared_app/src/widgets/theme_toggle.rs
```

- [ ] **Step 2: Remove the mod entry**

In `shared_app/src/widgets/mod.rs`, delete this line:

```rust
pub(crate) mod theme_toggle;
```

The file should become:

```rust
//! Cross-feature reusable widgets.

pub(crate) mod avatar;
pub(crate) mod titled_container;
```

- [ ] **Step 3: Verify the delete is recognized (expect compile errors elsewhere)**

Run: `cargo build -p shared_app 2>&1 | head -40`
Expected: FAIL — `error[E0432]: unresolved import crate::widgets::theme_toggle` in `profile_screen.rs` and `desktop_shell.rs`. This is expected; subsequent tasks fix these.

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/widgets/theme_toggle.rs shared_app/src/widgets/mod.rs
git commit -m "refactor(shared_app): remove ThemeToggle widget file and mod entry

The sun/moon toggle is replaced by the iOS-style AppearancePicker on the
Me page (subsequent tasks). Removing the file first surfaces all call
sites that need updating."
```

---

## Task 2: Strip `ThemeToggle` and `is_dark` from `DesktopShell`

**Files:**
- Modify: `shared_app/src/desktop_shell.rs` (lines 24, 47-48, 58, 152-167)

**Interfaces:**
- Consumes: nothing.
- Produces: `DesktopShell` no longer has an `is_dark` field; `app.rs` (Task 4) must stop passing it.

- [ ] **Step 1: Remove the `ThemeToggle` import**

In `shared_app/src/desktop_shell.rs`, delete this line (line 24):

```rust
use crate::widgets::theme_toggle::ThemeToggle;
```

- [ ] **Step 2: Remove the `is_dark` field from `DesktopShell`**

Delete lines 47-48 (the doc comment + field):

```rust
    /// Drives the theme toggle pinned to the sidebar bottom.
    pub is_dark: Signal<bool>,
```

- [ ] **Step 3: Remove the `is_dark` arm from the `Clone` impl**

In the `impl<D> Clone for DesktopShell<D>` block, delete this line (line 58):

```rust
            is_dark: self.is_dark.clone(),
```

- [ ] **Step 4: Remove the spacer + toggle push from `build_sidebar`**

In `build_sidebar`, delete lines 152-167 (the flex-grow spacer comment+push and the theme-toggle comment+push). The end of the `for tab in &shell.tabs` loop (line 149-150) should now be immediately followed by the `// Sidebar content on sidebar_bg…` block (originally line 169).

Concretely, delete:

```rust
    // Flex-grow spacer pushes the toggle to the sidebar bottom.
    items = items.push(MultiChild::empty(Layout::default().flex_grow(1.0)));

    // Theme toggle pinned to the bottom of the sidebar.
    items = items.push(
        WithLayout::new(
            ThemeToggle::new(shell.is_dark.clone()),
            Layout::default()
                .width_percent(1.0)
                .height(48.0)
                .flex_shrink(0.0)
                .align(AlignItems::Center)
                .justify(JustifyContent::Center),
        )
        .boxed(),
    );
```

- [ ] **Step 5: Remove now-unused imports if any**

Run: `cargo build -p shared_app 2>&1 | grep "warning: unused import"`
If `Signal` or `WithLayout` or `JustifyContent` become unused in `desktop_shell.rs`, remove them from the `use vexo::{…}` block. (Likely `Signal` becomes unused — remove it. `WithLayout` is still used elsewhere in the file? Check: it was only used for the toggle wrapper. If so, remove. `JustifyContent::Center` was used in the toggle AND in the tab-item GestureDetector (line 146) — keep `JustifyContent`.)

- [ ] **Step 6: Build check (expect failure in `app.rs`, not `desktop_shell.rs`)**

Run: `cargo build -p shared_app 2>&1 | head -40`
Expected: FAIL in `app.rs` with `error: missing field is_dark` (or similar) when constructing `DesktopShell { … }`. `desktop_shell.rs` itself should compile clean.

- [ ] **Step 7: Commit**

```bash
git add shared_app/src/desktop_shell.rs
git commit -m "refactor(desktop_shell): remove ThemeToggle and is_dark field

The sidebar no longer hosts a theme toggle. Theme selection now happens
only on the Me page (iOS-style picker). The is_dark signal still lives
on ImState and flows to the Me tab via build_me_tab."
```

---

## Task 3: Add `AppearancePicker` component to `profile_screen.rs`

**Files:**
- Modify: `shared_app/src/me/profile_screen.rs`

**Interfaces:**
- Consumes: `vexo::Signal<bool>`, `vexo::Theme`/`ThemeData` (local + ambient), `vexo::GestureDetector`, `vexo_fontawesome::Icons::Check`.
- Produces: `AppearancePicker::new(is_dark: Signal<bool>) -> AppearancePicker` — a `Component` whose `render()` returns a two-cell row widget (the card's inner content). `ProfileScreen::render()` wraps it via `build_card(vec![AppearancePicker::new(...).boxed()], &theme)`.

- [ ] **Step 1: Add the new imports**

At the top of `shared_app/src/me/profile_screen.rs`, the existing `use vexo::{…}` block (lines 10-13) needs `GestureDetector` and `ThemeData` added. Replace:

```rust
use vexo::{
    children, AlignItems, Color, Component, DecoratedBox, Layout, MultiChild, RenderContext,
    ScrollView, SimpleState, Style, Text, Theme, Widget, WithLayout,
};
```

with:

```rust
use vexo::{
    children, AlignItems, Color, Component, DecoratedBox, GestureDetector, Layout, MultiChild,
    RenderContext, ScrollView, SimpleState, Style, Text, Theme, ThemeData, Widget, WithLayout,
};
```

Also remove the now-deleted import on line 19:

```rust
use crate::widgets::theme_toggle::ThemeToggle;
```

- [ ] **Step 2: Add picker metrics constants**

Below the existing constants block (after line 47 `const DIVIDER_RIGHT_INSET: f32 = ROW_PAD_H;`), add:

```rust
// --- Appearance picker metrics ------------------------------------------------

/// Preview tile size (3:2 landscape).
const PREVIEW_WIDTH: f32 = 120.0;
const PREVIEW_HEIGHT: f32 = 80.0;
const PREVIEW_RADIUS: f32 = 6.0;
const PREVIEW_BORDER_WIDTH: f32 = 1.0;
/// Swatch band heights inside the preview.
const SWATCH_BAND_HEIGHT: f32 = 16.0;
/// Accent rect in the content band.
const ACCENT_RECT_WIDTH: f32 = 24.0;
const ACCENT_RECT_HEIGHT: f32 = 8.0;
const ACCENT_RECT_LEFT_INSET: f32 = 12.0;
/// Content-band divider.
const SWATCH_DIVIDER_THICKNESS: f32 = 1.0;
/// Checkbox metrics.
const CHECKBOX_SIZE: f32 = 22.0;
const CHECKBOX_RADIUS: f32 = 6.0;
/// Cell internal padding and gaps.
const CELL_PAD: f32 = 12.0;
const CELL_GAP: f32 = 8.0;
const PICKER_LABEL_FONT_SIZE: f32 = 15.0;
```

- [ ] **Step 3: Write the failing test**

In the `#[cfg(test)] mod tests` block (after the existing `test_profile_screen_renders_in_pipeline` test, around line 306), add:

```rust
    #[test]
    fn test_appearance_picker_renders_two_tappable_cells() {
        let is_dark = vexo::Signal::new(false);
        let view = AppearancePicker::new(is_dark).boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);

        let reg = pipeline.element_registry();
        let root = reg.root().expect("root element");
        let mut gesture_count = 0;
        for &eid in reg.iter() {
            if reg.widget(eid)
                .map(|w| w.as_any().downcast_ref::<GestureDetector>().is_some())
                .unwrap_or(false)
            {
                gesture_count += 1;
            }
        }
        assert_eq!(
            gesture_count, 2,
            "picker should have exactly two GestureDetectors (one per cell)"
        );
    }
```

Note: if `element_registry().iter()` or `.widget(eid)` doesn't exist with those exact names, the implementer should use whatever the registry exposes for "iterate all elements" and "get a widget by id" — check `vexo/src/element.rs` for the real API. The intent is: walk every element, count those whose widget is a `GestureDetector`.

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p shared_app test_appearance_picker_renders_two_tappable_cells 2>&1 | tail -20`
Expected: FAIL — `cannot find type AppearancePicker in this scope` (compile error).

- [ ] **Step 5: Implement `build_swatch_preview` helper**

Add this free function (after `icon_tile`, around line 213). It builds the abstract swatch stack for a given `ThemeData` and wraps it in a local `Theme` so the bands resolve to that theme. The `outline` border frame uses the SAME local theme's `outline` (so the frame matches the preview's mode).

```rust
fn build_swatch_preview(mode_theme: ThemeData) -> Box<dyn Widget> {
    let band_bg = mode_theme.surface_variant;
    let content_bg = mode_theme.surface;
    let accent = mode_theme.primary;
    let divider_color = mode_theme.outline;
    let border_color = mode_theme.outline;

    let header_band = DecoratedBox::with_style(
        MultiChild::empty(
            Layout::row()
                .width_percent(1.0)
                .height(SWATCH_BAND_HEIGHT)
                .flex_shrink(0.0),
        ),
        Style::default().background(band_bg),
    );

    let accent_rect = DecoratedBox::with_style(
        MultiChild::empty(
            Layout::row()
                .width(ACCENT_RECT_WIDTH)
                .height(ACCENT_RECT_HEIGHT)
                .flex_shrink(0.0),
        ),
        Style::default().background(accent),
    );

    let content_divider = DecoratedBox::with_style(
        MultiChild::empty(
            Layout::row()
                .width_percent(1.0)
                .height(SWATCH_DIVIDER_THICKNESS)
                .flex_shrink(0.0),
        ),
        Style::default().background(divider_color),
    );

    let content_band = WithLayout::new(
        MultiChild::new(
            children![accent_rect, content_divider],
            Layout::column()
                .width_percent(1.0)
                .flex_grow(1.0)
                .padding_each(ACCENT_RECT_LEFT_INSET, 0.0, 0.0, 0.0)
                .justify(JustifyContent::SpaceBetween),
        ),
        Style::default().background(content_bg),
    );

    let bottom_band = DecoratedBox::with_style(
        MultiChild::empty(
            Layout::row()
                .width_percent(1.0)
                .height(SWATCH_BAND_HEIGHT)
                .flex_shrink(0.0),
        ),
        Style::default().background(band_bg),
    );

    let swatch_stack = DecoratedBox::with_style(
        MultiChild::new(
            children![header_band, content_band, bottom_band],
            Layout::column()
                .width(PREVIEW_WIDTH)
                .height(PREVIEW_HEIGHT)
                .flex_shrink(0.0),
        ),
        Style::default()
            .border(border_color, PREVIEW_BORDER_WIDTH)
            .corner_radius(PREVIEW_RADIUS),
    );

    Theme::new(mode_theme, swatch_stack).boxed()
}
```

Note on the content band: the `padding_each(top, right, bottom, left)` order must match whatever the existing `Layout::padding_each` signature uses — verify by reading `vexo/src/layout/mod.rs` or the existing `ProfileScreen::render` call at line 77-82 which uses `padding_each(CARD_SIDE_MARGIN, CARD_SIDE_MARGIN, SECTION_GAP, SECTION_GAP)`. That order is `(left, right, top, bottom)` per the existing usage (left=16, right=16, top=20, bottom=20). So `padding_each` here for the accent left-inset should be `padding_each(ACCENT_RECT_LEFT_INSET, 0.0, 0.0, 0.0)` meaning left=12. **Verify the arg order before finalizing.**

The `content_divider` sits at the bottom of the content band via `JustifyContent::SpaceBetween` (accent at top-left after padding, divider pushed to bottom). The divider's `width_percent(1.0)` makes it span the full content-band width minus the left padding — to make it truly full-width, the divider should NOT be inside the padded column. **Fix:** move the divider OUTSIDE the padded content, or restructure. Simplest correct version:

```rust
    let content_band = DecoratedBox::with_style(
        MultiChild::new(
            children![
                WithLayout::new(
                    accent_rect,
                    Layout::default()
                        .padding_each(ACCENT_RECT_LEFT_INSET, 0.0, 0.0, 0.0)
                        .flex_grow(1.0),
                ),
                content_divider,
            ],
            Layout::column().width_percent(1.0).flex_grow(1.0),
        ),
        Style::default().background(content_bg),
    );
```

Here `content_divider` is the second child of the unpadded column, so it spans full width. The `accent_rect` is wrapped in a padded `WithLayout` that `flex_grow(1.0)`s to fill the space above the divider. The implementer should use THIS corrected version, not the first draft.

- [ ] **Step 6: Implement `build_checkbox` helper**

Add after `build_swatch_preview`. Uses AMBIENT theme colors (caller passes `&ThemeData`).

```rust
fn build_checkbox(selected: bool, ambient: &ThemeData) -> Box<dyn Widget> {
    if selected {
        DecoratedBox::with_style(
            WithLayout::new(
                Icon::new(Icons::Check)
                    .with_size(14.0)
                    .with_color(Color::WHITE),
                Layout::default()
                    .width(CHECKBOX_SIZE)
                    .height(CHECKBOX_SIZE)
                    .justify(JustifyContent::Center)
                    .align(AlignItems::Center)
                    .flex_shrink(0.0),
            ),
            Style::default()
                .background(ambient.primary)
                .corner_radius(CHECKBOX_RADIUS),
        )
        .boxed()
    } else {
        DecoratedBox::with_style(
            MultiChild::empty(
                Layout::row()
                    .width(CHECKBOX_SIZE)
                    .height(CHECKBOX_SIZE)
                    .flex_shrink(0.0),
            ),
            Style::default()
                .border(ambient.outline, PREVIEW_BORDER_WIDTH)
                .corner_radius(CHECKBOX_RADIUS),
        )
        .boxed()
    }
}
```

- [ ] **Step 7: Implement `AppearancePicker` component**

Add after `build_checkbox`. This is the public entry point.

```rust
#[derive(Clone)]
pub(crate) struct AppearancePicker {
    is_dark: vexo::Signal<bool>,
}

impl AppearancePicker {
    pub(crate) fn new(is_dark: vexo::Signal<bool>) -> Self {
        Self { is_dark }
    }
}

impl Component for AppearancePicker {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let ambient = Theme::of(ctx);
        let current = self.is_dark.get();

        let light_cell = build_picker_cell(
            ThemeData::light(),
            "Light",
            false,
            current,
            self.is_dark.clone(),
            false,
            &ambient,
        );
        let dark_cell = build_picker_cell(
            ThemeData::dark(),
            "Dark",
            true,
            current,
            self.is_dark.clone(),
            true,
            &ambient,
        );

        MultiChild::new(
            children![light_cell, dark_cell],
            Layout::row().gap(CELL_GAP).align(AlignItems::Stretch),
        )
        .boxed()
    }
}
```

- [ ] **Step 8: Implement `build_picker_cell` helper**

Add before `AppearancePicker`. This composes one cell: preview → label → checkbox, wrapped in a `GestureDetector`.

```rust
fn build_picker_cell(
    mode_theme: ThemeData,
    label: &str,
    target_is_dark: bool,
    current_is_dark: bool,
    is_dark: vexo::Signal<bool>,
    set_value: bool,
    ambient: &ThemeData,
) -> Box<dyn Widget> {
    let preview = build_swatch_preview(mode_theme);
    let label_widget = WithLayout::new(
        Text::new(label)
            .with_font_size(PICKER_LABEL_FONT_SIZE)
            .with_color(ambient.on_background),
        Layout::default(),
    );
    let checkbox = build_checkbox(current_is_dark == target_is_dark, ambient);

    let content = MultiChild::new(
        children![preview, label_widget, checkbox],
        Layout::column()
            .gap(CELL_GAP)
            .align(AlignItems::Center)
            .padding_each(CELL_PAD, CELL_PAD, CELL_PAD, CELL_PAD),
    );

    GestureDetector::new(content)
        .on_press(move || {
            is_dark.set(set_value);
        })
        .with_layout(
            Layout::default()
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .flex_basis(0.0),
        )
        .boxed()
}
```

Note: the `set_value` param is `false` for the light cell and `true` for the dark cell — passed literally from `AppearancePicker::render`. The `target_is_dark` param is the same value but named for the checkbox comparison `current_is_dark == target_is_dark`. (Could collapse to one param, but keeping both makes the call site self-documenting.)

- [ ] **Step 9: Run the test to verify it passes**

Run: `cargo test -p shared_app test_appearance_picker_renders_two_tappable_cells 2>&1 | tail -20`
Expected: PASS — exactly 2 `GestureDetector` elements.

If the test fails because `element_registry().iter()` or `.widget(eid)` has different names, fix the test to use the real API (check `vexo/src/element.rs` for the `ElementRegistry` API). The assertion intent is unchanged.

- [ ] **Step 10: Commit**

```bash
git add shared_app/src/me/profile_screen.rs
git commit -m "feat(me): add iOS-style AppearancePicker with theme-preview cells

Two side-by-side cells, each showing a widget-drawn swatch-stack preview
wrapped in a local Theme (light cell always light, dark cell always dark),
a Light/Dark label, and a rounded-square checkbox. Tapping a cell sets
is_dark to that mode. Replaces the old single-row toggle."
```

---

## Task 4: Wire `AppearancePicker` into `ProfileScreen`, remove `build_toggle_row`

**Files:**
- Modify: `shared_app/src/me/profile_screen.rs` (lines 90-97, 239-259)

**Interfaces:**
- Consumes: `AppearancePicker::new` (Task 3).
- Produces: `ProfileScreen::render` emits the new picker in the Appearance section card.

- [ ] **Step 1: Replace the `build_toggle_row` call in `ProfileScreen::render`**

In `ProfileScreen::render`, find the Appearance section (lines 90-97):

```rust
        // Section "Appearance": the Dark Mode toggle row.
        content = content.push(spacer(SECTION_GAP));
        content = content.push(section_header("Appearance", &theme));
        content = content.push(spacer(HEADER_TO_CARD_GAP));
        content = content.push(build_card(
            vec![build_toggle_row(self.is_dark.clone(), &theme)],
            &theme,
        ));
```

Replace the `build_card(vec![build_toggle_row(...)], &theme)` line with:

```rust
        // Section "Appearance": iOS-style light/dark picker.
        content = content.push(spacer(SECTION_GAP));
        content = content.push(section_header("Appearance", &theme));
        content = content.push(spacer(HEADER_TO_CARD_GAP));
        content = content.push(build_card(
            vec![AppearancePicker::new(self.is_dark.clone()).boxed()],
            &theme,
        ));
```

- [ ] **Step 2: Delete the `build_toggle_row` function**

Delete lines 239-259 (the entire `build_toggle_row` function including its doc comment):

```rust
/// The Dark Mode toggle row: icon tile + label on the left, `ThemeToggle`
/// on the right. No chevron (the toggle is the trailing control).
fn build_toggle_row(is_dark: vexo::Signal<bool>, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let dark = is_dark.get();
    let icon = if dark { Icons::Sun } else { Icons::Moon };
    let tile = icon_tile(icon, theme.primary);
    let label = WithLayout::new(
        Text::new("Dark Mode")
            .with_font_size(16.0)
            .with_color(theme.on_background),
        Layout::default().flex_grow(1.0),
    );
    WithLayout::new(
        MultiChild::new(
            children![tile, label, ThemeToggle::new(is_dark)],
            Layout::row().gap(TILE_LABEL_GAP).align(AlignItems::Center),
        ),
        Layout::default().padding_each(ROW_PAD_H, ROW_PAD_H, ROW_PAD_V, ROW_PAD_V),
    )
    .boxed()
}
```

- [ ] **Step 3: Bump the existing profile-screen test threshold**

In `test_profile_screen_renders_in_pipeline` (around line 303), the assertion `pipeline.element_registry().len() > 30` should still hold (the picker has MORE elements than the old toggle row). Leave the threshold as `> 30` — it remains valid. No change needed unless the test fails; if it fails, lower to `> 25`.

- [ ] **Step 4: Build + run profile-screen tests**

Run: `cargo test -p shared_app profile_screen 2>&1 | tail -20`
Expected: PASS — both `test_profile_screen_renders_in_pipeline` and `test_appearance_picker_renders_two_tappable_cells` pass.

- [ ] **Step 5: Commit**

```bash
git add shared_app/src/me/profile_screen.rs
git commit -m "feat(me): wire AppearancePicker into ProfileScreen, remove build_toggle_row

The Appearance section now shows the two-cell iOS-style picker instead of
the single Dark Mode toggle row."
```

---

## Task 5: Remove `is_dark` threading into `DesktopShell` from `app.rs`

**Files:**
- Modify: `shared_app/src/app.rs` (lines 111, 146)

**Interfaces:**
- Consumes: `DesktopShell` no longer has `is_dark` (Task 2).
- Produces: `app.rs` constructs `DesktopShell` without `is_dark`; Me-tab page builder still gets `is_dark_signal`.

- [ ] **Step 1: Remove the `is_dark_for_shell` clone**

In `app.rs`, the Desktop branch (around line 111) has:

```rust
                let is_dark_for_shell = is_dark_signal.clone();
```

Delete this line. (The `is_dark_signal.clone()` for the Me-tab page builder at line 130 stays — `build_me_tab` still needs it.)

- [ ] **Step 2: Remove the `is_dark` field from the `DesktopShell` construction**

In the `DesktopShell { … }` literal (around lines 113-147), delete the last field:

```rust
                    is_dark: is_dark_signal.clone(),
```

The `DesktopShell` literal should end after `sidebar_builder: …`.

- [ ] **Step 3: Build the whole workspace**

Run: `cargo build 2>&1 | tail -20`
Expected: PASS — no errors. (`is_dark_signal` is still used for the ambient theme swap at line 47-53 AND for the Me-tab page builder at line 130, so no unused-variable warnings.)

- [ ] **Step 4: Run all shared_app tests**

Run: `cargo test -p shared_app 2>&1 | tail -30`
Expected: PASS — all tests including integration tests. The sidebar-width test (`test_desktop_sidebar_is_narrow_and_fits_window`) still passes because `SIDEBAR_WIDTH` (64px) is unchanged; only the toggle widget at the bottom is gone.

- [ ] **Step 5: Commit**

```bash
git add shared_app/src/app.rs
git commit -m "refactor(app): stop threading is_dark into DesktopShell

The desktop sidebar no longer has a theme toggle, so DesktopShell doesn't
need the signal. is_dark still flows to the Me tab (build_me_tab) and to
the root view() for the ambient Theme swap."
```

---

## Task 6: Update `data.rs` doc comment and delete old theme-toggle design docs

**Files:**
- Modify: `shared_app/src/data.rs:77-78`
- Delete: `docs/superpowers/plans/2026-07-13-theme-toggle.md`
- Delete: `docs/superpowers/specs/2026-07-13-theme-toggle-design.md`

**Interfaces:**
- Consumes: nothing.
- Produces: accurate doc comment; old design docs removed.

- [ ] **Step 1: Update the `is_dark` doc comment**

In `shared_app/src/data.rs`, replace lines 77-78:

```rust
    /// Dark/light mode. Toggled by `ThemeToggle`. Root `view()` reads this to
    /// pick `ThemeData::dark()`/`light()` and wraps the tree in `Theme::new`.
```

with:

```rust
    /// Dark/light mode. Selected from the Me page's `AppearancePicker`. Root
    /// `view()` reads this to pick `ThemeData::dark()`/`light()` and wraps
    /// the tree in `Theme::new`.
```

- [ ] **Step 2: Delete the two theme-toggle design docs**

```bash
rm docs/superpowers/plans/2026-07-13-theme-toggle.md
rm docs/superpowers/specs/2026-07-13-theme-toggle-design.md
```

- [ ] **Step 3: Verify no dangling references**

Run: `rg "ThemeToggle" shared_app/ docs/ 2>&1 | head`
Expected: no matches (all `ThemeToggle` references were removed in Tasks 1-5).

Run: `rg "2026-07-13-theme-toggle" . 2>&1 | head`
Expected: no matches (the deleted docs aren't referenced anywhere).

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/data.rs docs/superpowers/plans/2026-07-13-theme-toggle.md docs/superpowers/specs/2026-07-13-theme-toggle-design.md
git commit -m "docs: update is_dark comment, remove stale theme-toggle design docs

The ThemeToggle widget is gone; the comment now points to AppearancePicker.
The 2026-07-13 theme-toggle plan/spec recorded the original sun/moon
toggle design, which is superseded by the iOS-style picker."
```

---

## Task 7: Final verification

**Files:** none (verification only).

- [ ] **Step 1: Full workspace build**

Run: `cargo build 2>&1 | tail -10`
Expected: PASS, no warnings related to `theme_toggle`, `is_dark`, or unused imports.

- [ ] **Step 2: Full workspace test**

Run: `cargo test 2>&1 | tail -40`
Expected: all tests pass. Key tests to confirm:
- `test_appearance_picker_renders_two_tappable_cells` (new, Task 3)
- `test_profile_screen_renders_in_pipeline` (existing, Task 4)
- `test_desktop_sidebar_is_narrow_and_fits_window` (existing, sidebar width unchanged)
- `test_full_app_view_renders_desktop_shell` (existing, element count still > 15)
- `test_desktop_chats_tab_shows_three_column_layout` (existing, layout unchanged)

- [ ] **Step 3: Ask the user to run the desktop demo for visual verification**

Per `CLAUDE.md`, never run `cargo run -p desktop_demo` yourself. Ask the user:

> The implementation is complete. Please run `cargo run -p desktop_demo` and verify:
> 1. The sidebar has NO sun/moon toggle at the bottom (just the three tab icons).
> 2. Click the "Me" tab. The Appearance section shows two side-by-side preview cells (Light | Dark), each with a swatch preview, a "Light"/"Dark" label, and a checkbox.
> 3. The Light cell's preview always looks light; the Dark cell's preview always looks dark — regardless of the app's current mode.
> 4. Tapping the Light cell switches the whole app to light mode; the checkbox moves to the Light cell.
> 5. Tapping the Dark cell switches to dark mode; the checkbox moves to the Dark cell.
> 6. Tapping the already-selected cell is a no-op (no flicker).

- [ ] **Step 4: (No commit — verification only.)**

If the user reports visual issues, file follow-up tasks. Do not commit fixes without a new task.

---

## Self-Review

**Spec coverage:** All 25 grilled decisions map to tasks:
- Q1 (widget preview) → Task 3 Step 5 (`build_swatch_preview`)
- Q2 (rounded checkbox) → Task 3 Step 6 (`build_checkbox`)
- Q3 (whole card tappable) → Task 3 Step 8 (`GestureDetector` wraps cell)
- Q4 (single card, two cells, gap, no divider) → Task 3 Step 7 (`MultiChild::row().gap(CELL_GAP)`)
- Q5 (preview→label→checkbox order) → Task 3 Step 8 (children order)
- Q6 (abstract swatch stack) → Task 3 Step 5
- Q7 (remove sidebar toggle + field + spacer) → Task 2
- Q8 (delete theme_toggle.rs) → Task 1
- Q9 (inline in profile_screen.rs) → Task 3
- Q10 (hardcode ThemeData, local Theme, ambient chrome) → Task 3 Steps 5-8
- Q11 (Component, Theme::of(ctx)) → Task 3 Step 7
- Q12 (DecoratedBox + Icons::Check) → Task 3 Step 6
- Q13 (exact metrics) → Task 3 Step 2 (constants)
- Q14 (no divider, 8pt gap) → Task 3 Step 7
- Q15 (fixed-bool closures) → Task 3 Step 8 (`set_value` param)
- Q16 (SimpleState, root cascade) → Task 3 Step 7
- Q17 (replace toggle row, header stays) → Task 4 Step 1
- Q18 (AppearancePicker returns row, build_card wraps) → Task 4 Step 1
- Q19 (bump threshold + picker test) → Task 3 Step 3, Task 4 Step 3
- Q20 (update data.rs comment) → Task 6 Step 1
- Q21 (delete old docs) → Task 6 Step 2
- Q22 (outline + corner_radius frame) → Task 3 Step 5
- Q23 (bands read local theme) → Task 3 Step 5
- Q24 (3 bands: surface_variant/surface+accent+outline/surface_variant) → Task 3 Step 5
- Q25 (accent 24×8, left-inset 12, centered) → Task 3 Step 5

**Placeholder scan:** No "TBD"/"TODO"/"implement later". The two `build_swatch_preview` drafts in Step 5 are resolved inline (the second corrected version is the one to use). The test in Step 3 has a note about verifying the `ElementRegistry` API names — this is a real uncertainty, not a placeholder; the implementer must check `vexo/src/element.rs`. Same for `padding_each` arg order in Step 5 — explicitly flagged with a verification instruction.

**Type consistency:** `AppearancePicker::new(is_dark: Signal<bool>)` is consistent across Task 3 (definition), Task 4 (call site), and the doc comment in Task 6. `build_picker_cell`'s signature is consistent between its definition (Step 8) and its two call sites in `AppearancePicker::render` (Step 7). `build_swatch_preview(mode_theme: ThemeData)` and `build_checkbox(selected: bool, ambient: &ThemeData)` signatures match their call sites.

**One open risk:** The `ElementRegistry` iteration/widget-lookup API used in the Task 3 Step 3 test (`reg.iter()`, `reg.widget(eid)`) may not match the real method names. The implementer must read `vexo/src/element.rs` to confirm. If the API is different (e.g. `reg.elements()` returning an iterator, or no public widget lookup), the test should be adapted to use whatever walking/inspection API exists — or, if none is suitable, fall back to asserting `pipeline.element_registry().len()` is in an expected range for the picker alone (the picker has ~2 GestaltDetectors + 2 previews×~7 + 2 labels + 2 checkboxes ≈ 20+ elements; assert `len() > 15`).
