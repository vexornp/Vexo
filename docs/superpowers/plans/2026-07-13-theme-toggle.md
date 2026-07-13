# Dark/Light Theme Toggle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dark/light theme toggle to the demo app that themes the sidebar, header, detail pane, body text, buttons, and mobile nav header — covering both the demo's own colors and the `vexo_uikit` widgets it uses.

**Architecture:** Extend `vexo::ThemeData` with 3 Material-3 roles (`surface_variant`, `outline`, `on_surface_variant`). Convert `vexo_uikit::theme::tokens` color `const`s into resolver structs (`ButtonColors`, `NavColors`) that map from `&ThemeData`. Wire `Button` and `NavigationStackView` to read `Theme::of(ctx)`. In the demo, add an `is_dark: Signal<bool>` to `State`, wrap the tree in `Theme::new(theme, child)`, and add a sun/moon toggle in the sidebar header (desktop) / list top (mobile).

**Tech Stack:** Rust workspace — `vexo` (core framework), `vexo_uikit` (Button + NavigationStackView), `shared_app` (demo). `vexo::Theme`/`ThemeData` InheritedWidget (landed 2026-07-12). `vexo_fontawesome` (`Icons::Sun` f185, `Icons::Moon` f186). `Color::lerp` for hover/pressed shades.

## Global Constraints

- All `Color::from_hex` values use 8-digit RRGGBBAA (per commit `7702201` — `from_hex` expects RRGGBBAA, not RRGGBB).
- `Theme::of(ctx)` falls back to `ThemeData::light()` when no `Theme` ancestor exists (so unwrapped tests render light colors).
- Never run `cargo run -p desktop_demo` — the assistant cannot interact with the GUI (per `CLAUDE.md`). Visual verification is user-run only.
- After every Rust edit, run `cargo build`. After implementing a feature, run `cargo test`.
- Commit messages follow the repo style: `feat(scope): ...`, `fix(scope): ...`, `refactor(scope): ...`, `test(scope): ...`. No "Co-Authored-By" attribution.
- The spec is at `docs/superpowers/specs/2026-07-13-theme-toggle-design.md`. The role-to-token mapping table there is the source of truth for colors.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `vexo/src/widgets/theme.rs` | Modify | Add 3 fields to `ThemeData`, tune `dark()` preset, add tests |
| `vexo_uikit/src/theme/tokens.rs` | Modify | Replace color `const`s with `ButtonColors`/`NavColors` resolver structs + `colors(&ThemeData)` fns; keep sizing/padding/string consts |
| `vexo_uikit/src/button.rs` | Modify | `Button::render` reads `Theme::of(ctx)`, resolves `ButtonColors`, uses them in `resolve_*` methods |
| `vexo_uikit/src/navigation.rs` | Modify | `NavigationStackView::render` reads `Theme::of(ctx)`, resolves `NavColors`, passes to `build_nav_bar` |
| `vexo_uikit/tests/token_tests.rs` | Modify | Replace `const` assertions with resolver-output assertions |
| `shared_app/src/lib.rs` | Modify | Add `is_dark: Signal<bool>` to `State`; wrap tree in `Theme`; theme sidebar/item-rows/DetailPage; add toggle control; convert `PageContent` to a `Component` |

---

### Task 1: Extend `ThemeData` with 3 new roles + tune dark preset

**Files:**
- Modify: `vexo/src/widgets/theme.rs:18-58` (struct + `light()`/`dark()` impls)
- Test: `vexo/src/widgets/theme.rs:128-179` (test module)

**Interfaces:**
- Produces: `ThemeData` now has `pub surface_variant: Color`, `pub outline: Color`, `pub on_surface_variant: Color`. `light()` and `dark()` return these. `dark().primary` is now `0x6775FFFF` (was `0x121434FF`).

- [ ] **Step 1: Write the failing tests**

Add these tests to the `#[cfg(test)] mod tests` block in `vexo/src/widgets/theme.rs`, after the existing `theme_data_default_is_light` test:

```rust
    #[test]
    fn theme_data_has_new_roles() {
        let l = ThemeData::light();
        // New fields must be non-default (not pure black/white/transparent).
        assert_ne!(l.surface_variant, Color::TRANSPARENT);
        assert_ne!(l.outline, Color::TRANSPARENT);
        assert_ne!(l.on_surface_variant, Color::TRANSPARENT);
    }

    #[test]
    fn theme_data_light_and_dark_differ_on_new_roles() {
        let l = ThemeData::light();
        let d = ThemeData::dark();
        assert_ne!(l.surface_variant, d.surface_variant);
        assert_ne!(l.outline, d.outline);
        assert_ne!(l.on_surface_variant, d.on_surface_variant);
    }

    #[test]
    fn theme_data_dark_primary_is_brand_blue() {
        // dark().primary changed from the placeholder 0x121434 to the same
        // brand blue as light(), so accent stays consistent across modes.
        assert_eq!(ThemeData::dark().primary, ThemeData::light().primary);
        assert_eq!(
            ThemeData::dark().primary,
            Color::from_hex(0x6775FFFF)
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib widgets::theme::tests`
Expected: FAIL — `no field surface_variant on type ThemeData` (compile error), and `dark().primary` assertion fails.

- [ ] **Step 3: Add the 3 new fields to `ThemeData`**

In `vexo/src/widgets/theme.rs`, replace the `ThemeData` struct definition (lines 18-28) with:

```rust
#[derive(Clone, PartialEq, Debug)]
pub struct ThemeData {
    pub primary: Color,
    pub on_primary: Color,
    pub background: Color,
    pub on_background: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub surface_variant: Color,
    pub outline: Color,
    pub on_surface_variant: Color,
    pub error: Color,
    pub on_error: Color,
}
```

- [ ] **Step 4: Update `light()` preset**

Replace the `light()` method (lines 32-43) with:

```rust
    pub fn light() -> Self {
        Self {
            primary: Color::from_hex(0x6775FFFF),
            on_primary: Color::WHITE,
            background: Color::WHITE,
            on_background: Color::BLACK,
            surface: Color::from_hex(0xFFFFFFFF),
            on_surface: Color::from_hex(0x1C1B1FFF),
            surface_variant: Color::from_hex(0xE6E6EBFF),
            outline: Color::from_hex(0xC7C7CCFF),
            on_surface_variant: Color::from_hex(0x999999FF),
            error: Color::from_hex(0xB3261EFF),
            on_error: Color::WHITE,
        }
    }
```

- [ ] **Step 5: Update `dark()` preset**

Replace the `dark()` method (lines 46-57) with:

```rust
    pub fn dark() -> Self {
        Self {
            primary: Color::from_hex(0x6775FFFF),
            on_primary: Color::WHITE,
            background: Color::from_hex(0x1C1B1FFF),
            on_background: Color::WHITE,
            surface: Color::from_hex(0x2B2930FF),
            on_surface: Color::WHITE,
            surface_variant: Color::from_hex(0x38353CFF),
            outline: Color::from_hex(0x49454FFF),
            on_surface_variant: Color::from_hex(0x9E9CA6FF),
            error: Color::from_hex(0xF2B8B5FF),
            on_error: Color::BLACK,
        }
    }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vexo --lib widgets::theme::tests`
Expected: PASS — all theme tests including the 3 new ones.

- [ ] **Step 7: Build the whole workspace to catch downstream breakage**

Run: `cargo build`
Expected: PASS (vexo_uikit / shared_app still compile because they don't construct `ThemeData` directly — they only read it via `Theme::of` or the const tokens which are still light-only at this point).

- [ ] **Step 8: Commit**

```bash
git add vexo/src/widgets/theme.rs
git commit -m "feat(theme): add surface_variant, outline, on_surface_variant roles; tune dark primary to brand blue"
```

---

### Task 2: Convert `vexo_uikit` button tokens to a resolver struct

**Files:**
- Modify: `vexo_uikit/src/theme/tokens.rs:1-40` (replace `button` module)
- Test: `vexo_uikit/src/theme/tokens.rs` (add `#[cfg(test)]` module at file end)

**Interfaces:**
- Consumes: `vexo::ThemeData` (from Task 1 — needs the 3 new fields).
- Produces: `vexo_uikit::theme::tokens::button::ButtonColors` struct and `vexo_uikit::theme::tokens::button::colors(&ThemeData) -> ButtonColors`. The color `const`s (`PRIMARY_BG`, etc.) are **removed**. Sizing/padding/font-size/disabled-opacity consts remain.

- [ ] **Step 1: Write the failing tests**

Append a test module at the end of `vexo_uikit/src/theme/tokens.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::button::{colors, ButtonColors};
    use vexo::{Color, ThemeData};

    #[test]
    fn button_colors_light_maps_roles() {
        let t = ThemeData::light();
        let c = colors(&t);
        assert_eq!(c.primary_bg, t.primary);
        assert_eq!(c.primary_text, t.on_primary);
        assert_eq!(c.secondary_bg, Color::TRANSPARENT);
        assert_eq!(c.secondary_border, t.outline);
        assert_eq!(c.secondary_text, t.primary);
        assert_eq!(c.destructive_bg, t.error);
        assert_eq!(c.destructive_text, t.on_error);
        assert_eq!(c.ghost_bg, Color::TRANSPARENT);
        assert_eq!(c.ghost_text, t.primary);
    }

    #[test]
    fn button_colors_hover_pressed_are_lerp() {
        let t = ThemeData::light();
        let c = colors(&t);
        assert_eq!(c.primary_bg_hover, Color::lerp(t.primary, Color::WHITE, 0.15));
        assert_eq!(c.primary_bg_pressed, Color::lerp(t.primary, Color::BLACK, 0.15));
        assert_eq!(c.destructive_bg_hover, Color::lerp(t.error, Color::WHITE, 0.15));
        assert_eq!(c.destructive_bg_pressed, Color::lerp(t.error, Color::BLACK, 0.15));
        assert_eq!(c.ghost_text_hover, Color::lerp(t.primary, Color::WHITE, 0.15));
    }

    #[test]
    fn button_colors_dark_maps_roles() {
        let t = ThemeData::dark();
        let c = colors(&t);
        assert_eq!(c.primary_bg, t.primary);
        assert_eq!(c.destructive_bg, t.error);
        assert_eq!(c.secondary_border, t.outline);
    }

    #[test]
    fn button_colors_is_a_struct() {
        // Compile-time check that ButtonColors is nameable and field-accessible.
        let _ = ButtonColors {
            primary_bg: Color::WHITE, primary_bg_hover: Color::WHITE,
            primary_bg_pressed: Color::WHITE, primary_text: Color::WHITE,
            secondary_bg: Color::WHITE, secondary_border: Color::WHITE,
            secondary_text: Color::WHITE,
            destructive_bg: Color::WHITE, destructive_bg_hover: Color::WHITE,
            destructive_bg_pressed: Color::WHITE, destructive_text: Color::WHITE,
            ghost_bg: Color::WHITE, ghost_text: Color::WHITE, ghost_text_hover: Color::WHITE,
        };
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo_uikit --lib theme::tokens::tests`
Expected: FAIL — `cannot find function colors in module button` / `cannot find type ButtonColors` (compile error).

- [ ] **Step 3: Replace the `button` module with resolver struct + `colors()` fn**

In `vexo_uikit/src/theme/tokens.rs`, replace the entire `pub mod button { ... }` block (lines 1-40) with:

```rust
pub mod button {
    use vexo::{Color, ThemeData};

    /// Theme-aware button colors resolved from a `ThemeData`.
    ///
    /// Produced by [`colors`]. Hover/pressed shades are derived via
    /// `Color::lerp` so they stay correct if `primary`/`error` change.
    pub struct ButtonColors {
        pub primary_bg: Color,
        pub primary_bg_hover: Color,
        pub primary_bg_pressed: Color,
        pub primary_text: Color,
        pub secondary_bg: Color,
        pub secondary_border: Color,
        pub secondary_text: Color,
        pub destructive_bg: Color,
        pub destructive_bg_hover: Color,
        pub destructive_bg_pressed: Color,
        pub destructive_text: Color,
        pub ghost_bg: Color,
        pub ghost_text: Color,
        pub ghost_text_hover: Color,
    }

    /// Resolve button colors from a `ThemeData`.
    pub fn colors(t: &ThemeData) -> ButtonColors {
        ButtonColors {
            primary_bg: t.primary,
            primary_bg_hover: Color::lerp(t.primary, Color::WHITE, 0.15),
            primary_bg_pressed: Color::lerp(t.primary, Color::BLACK, 0.15),
            primary_text: t.on_primary,
            secondary_bg: Color::TRANSPARENT,
            secondary_border: t.outline,
            secondary_text: t.primary,
            destructive_bg: t.error,
            destructive_bg_hover: Color::lerp(t.error, Color::WHITE, 0.15),
            destructive_bg_pressed: Color::lerp(t.error, Color::BLACK, 0.15),
            destructive_text: t.on_error,
            ghost_bg: Color::TRANSPARENT,
            ghost_text: t.primary,
            ghost_text_hover: Color::lerp(t.primary, Color::WHITE, 0.15),
        }
    }

    // Theme-independent constants (sizing, padding, font sizes).

    pub const DISABLED_OPACITY: f32 = 0.5;

    // Desktop sizing (matches macOS SwiftUI .bordered, regular control size)
    pub const CORNER_RADIUS_DESKTOP: f32 = 5.0;
    pub const PADDING_H_DESKTOP: f32 = 12.0;
    pub const PADDING_V_DESKTOP: f32 = 4.0;
    pub const FONT_SIZE_DESKTOP: f32 = 13.0;

    // Mobile sizing (matches iOS SwiftUI .bordered, regular control size)
    pub const CORNER_RADIUS_MOBILE: f32 = 8.0;
    pub const PADDING_H_MOBILE: f32 = 16.0;
    pub const PADDING_V_MOBILE: f32 = 8.0;
    pub const FONT_SIZE_MOBILE: f32 = 17.0;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo_uikit --lib theme::tokens::tests`
Expected: PASS.

- [ ] **Step 5: Build to confirm button.rs still compiles**

Run: `cargo build -p vexo_uikit`
Expected: FAIL — `button.rs` references `tokens::button::PRIMARY_BG` etc. which no longer exist. **This is expected**; Task 4 fixes `button.rs`. Do not commit yet — the workspace is temporarily broken. Proceed directly to Task 3 to fix navigation tokens in the same file, then Task 4 fixes button.rs.

> **Note:** Tasks 2 and 3 intentionally leave the workspace non-compiling because they remove consts that `button.rs`/`navigation.rs` still reference. The commit happens after Task 3 (both resolver structs in place) but before Task 4/5 (wire widgets). If you prefer a compiling commit boundary, do Tasks 2+3+4+5 as a single commit instead. The plan keeps them separate for reviewer clarity, with this note flagging the interim state.

---

### Task 3: Convert `vexo_uikit` navigation tokens to a resolver struct

**Files:**
- Modify: `vexo_uikit/src/theme/tokens.rs:42-88` (replace `navigation` module)
- Test: `vexo_uikit/src/theme/tokens.rs` (extend the test module from Task 2)

**Interfaces:**
- Consumes: `vexo::ThemeData` (from Task 1).
- Produces: `vexo_uikit::theme::tokens::navigation::NavColors` struct and `vexo_uikit::theme::tokens::navigation::colors(&ThemeData) -> NavColors`. The navigation color `const`s are **removed**. Sizing/padding/string consts (`SIDEBAR_WIDTH`, `MOBILE_HEADER_HEIGHT`, `BACK_CHEVRON`, `BACK_LABEL`, font sizes, paddings) remain.

- [ ] **Step 1: Write the failing tests**

Append to the `#[cfg(test)] mod tests` block in `vexo_uikit/src/theme/tokens.rs` (added in Task 2):

```rust
    use super::navigation::{colors as nav_colors, NavColors};

    #[test]
    fn nav_colors_light_maps_roles() {
        let t = ThemeData::light();
        let n = nav_colors(&t);
        assert_eq!(n.sidebar_bg, t.surface);
        assert_eq!(n.header_bg, t.surface_variant);
        assert_eq!(n.header_text, t.on_surface);
        assert_eq!(n.row_bg, Color::TRANSPARENT);
        assert_eq!(n.row_text, t.on_surface);
        assert_eq!(n.selected_bg, t.primary);
        assert_eq!(n.selected_text, t.on_primary);
        assert_eq!(n.detail_bg, t.background);
        assert_eq!(n.divider, t.outline);
        assert_eq!(n.placeholder_text, t.on_surface_variant);
        assert_eq!(n.mobile_header_bg, t.surface);
        assert_eq!(n.mobile_title, t.on_surface);
        assert_eq!(n.back_color, t.primary);
    }

    #[test]
    fn nav_colors_dark_maps_roles() {
        let t = ThemeData::dark();
        let n = nav_colors(&t);
        assert_eq!(n.sidebar_bg, t.surface);
        assert_eq!(n.selected_bg, t.primary);
        assert_eq!(n.divider, t.outline);
    }

    #[test]
    fn nav_colors_is_a_struct() {
        let _ = NavColors {
            sidebar_bg: Color::WHITE, header_bg: Color::WHITE, header_text: Color::WHITE,
            row_bg: Color::WHITE, row_text: Color::WHITE,
            selected_bg: Color::WHITE, selected_text: Color::WHITE,
            detail_bg: Color::WHITE, divider: Color::WHITE, placeholder_text: Color::WHITE,
            mobile_header_bg: Color::WHITE, mobile_title: Color::WHITE, back_color: Color::WHITE,
        };
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo_uikit --lib theme::tokens::tests`
Expected: FAIL — `cannot find function nav_colors` / `cannot find type NavColors`.

- [ ] **Step 3: Replace the `navigation` module with resolver struct + `colors()` fn**

In `vexo_uikit/src/theme/tokens.rs`, replace the entire `pub mod navigation { ... }` block with:

```rust
pub mod navigation {
    use vexo::{Color, ThemeData};

    /// Theme-aware navigation colors resolved from a `ThemeData`.
    pub struct NavColors {
        pub sidebar_bg: Color,
        pub header_bg: Color,
        pub header_text: Color,
        pub row_bg: Color,
        pub row_text: Color,
        pub selected_bg: Color,
        pub selected_text: Color,
        pub detail_bg: Color,
        pub divider: Color,
        pub placeholder_text: Color,
        pub mobile_header_bg: Color,
        pub mobile_title: Color,
        pub back_color: Color,
    }

    /// Resolve navigation colors from a `ThemeData`.
    pub fn colors(t: &ThemeData) -> NavColors {
        NavColors {
            sidebar_bg: t.surface,
            header_bg: t.surface_variant,
            header_text: t.on_surface,
            row_bg: Color::TRANSPARENT,
            row_text: t.on_surface,
            selected_bg: t.primary,
            selected_text: t.on_primary,
            detail_bg: t.background,
            divider: t.outline,
            placeholder_text: t.on_surface_variant,
            mobile_header_bg: t.surface,
            mobile_title: t.on_surface,
            back_color: t.primary,
        }
    }

    // Theme-independent constants (sizing, padding, strings, font sizes).

    pub const SIDEBAR_WIDTH: f32 = 240.0;
    pub const COLLAPSED_WIDTH: f32 = 44.0;

    pub const HEADER_PADDING: f32 = 12.0;
    pub const HEADER_FONT_SIZE: f32 = 16.0;

    pub const ROW_PADDING: f32 = 10.0;
    pub const ROW_FONT_SIZE: f32 = 16.0;

    pub const PLACEHOLDER_FONT_SIZE: f32 = 16.0;

    pub const MOBILE_HEADER_HEIGHT: f32 = 44.0;
    pub const MOBILE_HEADER_PADDING: f32 = 8.0;

    pub const BACK_CHEVRON: &str = "\u{2039}"; // ‹
    pub const BACK_LABEL: &str = "Back";
    pub const BACK_FONT_SIZE: f32 = 17.0;

    pub const MOBILE_TITLE_FONT_SIZE: f32 = 17.0;
}
```

- [ ] **Step 4: Run token tests to verify they pass**

Run: `cargo test -p vexo_uikit --lib theme::tokens::tests`
Expected: PASS — all button + navigation resolver tests.

- [ ] **Step 5: Commit (workspace still broken at button.rs/navigation.rs — fixed in Tasks 4 & 5)**

```bash
git add vexo_uikit/src/theme/tokens.rs
git commit -m "refactor(uikit): replace color const tokens with ThemeData resolvers

button::colors(&ThemeData) -> ButtonColors and
navigation::colors(&ThemeData) -> NavColors. Sizing/padding/string
consts remain. button.rs and navigation.rs still reference removed
consts; fixed in the next commit."
```

---

### Task 4: Wire `Button` to `Theme::of(ctx)`

**Files:**
- Modify: `vexo_uikit/src/button.rs:1-10` (imports), `:117-166` (`resolve_*` methods), `:201-270` (`render`)

**Interfaces:**
- Consumes: `vexo_uikit::theme::tokens::button::{ButtonColors, colors}` (from Task 2), `vexo::Theme` (re-exported at `vexo::Theme`).
- Produces: `Button::render` now reads `Theme::of(ctx)` and resolves colors from it. When no `Theme` ancestor exists, falls back to `ThemeData::light()` (via `Theme::of`). Note: `light().primary` (`0x6775FF`, indigo) differs from the old `PRIMARY_BG` const (`rgb(0.0, 0.478, 1.0)`, pure blue) — buttons change color. Integration tests asserting the old value are migrated in Task 6.

- [ ] **Step 1: Update imports in `button.rs`**

In `vexo_uikit/src/button.rs`, replace lines 4-7 (the `use vexo::{...}` block) with:

```rust
use vexo::{
    AlignSelf, Color, Component, ComponentState, DecoratedContainer, RenderContext, Text, Theme,
    ThemeData, Widget,
};
```

And replace line 10 (`use crate::theme::tokens;`) with:

```rust
use crate::theme::tokens;
use crate::theme::tokens::button::ButtonColors;
```

- [ ] **Step 2: Change `resolve_*` methods to take `&ButtonColors`**

In `vexo_uikit/src/button.rs`, replace the three `resolve_*` methods (lines 121-166) with:

```rust
    fn resolve_bg(&self, c: &ButtonColors, is_pressed: bool, is_hovered: bool) -> Color {
        match self.variant {
            ButtonVariant::Primary => {
                if is_pressed {
                    c.primary_bg_pressed
                } else if is_hovered && self.effective_platform() == Platform::Desktop {
                    c.primary_bg_hover
                } else {
                    c.primary_bg
                }
            }
            ButtonVariant::Secondary => c.secondary_bg,
            ButtonVariant::Destructive => {
                if is_pressed {
                    c.destructive_bg_pressed
                } else if is_hovered && self.effective_platform() == Platform::Desktop {
                    c.destructive_bg_hover
                } else {
                    c.destructive_bg
                }
            }
            ButtonVariant::Ghost => c.ghost_bg,
        }
    }

    fn resolve_border(&self, c: &ButtonColors) -> (Color, f32) {
        match self.variant {
            ButtonVariant::Secondary => (c.secondary_border, 1.0),
            _ => (Color::TRANSPARENT, 0.0),
        }
    }

    fn resolve_text_color(&self, c: &ButtonColors, is_hovered: bool) -> Color {
        match self.variant {
            ButtonVariant::Primary => c.primary_text,
            ButtonVariant::Destructive => c.destructive_text,
            ButtonVariant::Secondary => c.secondary_text,
            ButtonVariant::Ghost => {
                if is_hovered && self.effective_platform() == Platform::Desktop {
                    c.ghost_text_hover
                } else {
                    c.ghost_text
                }
            }
        }
    }
```

- [ ] **Step 3: Update `render` to resolve colors from `Theme::of(ctx)`**

In `vexo_uikit/src/button.rs`, replace the `render` method signature and the color-resolution lines (lines 204-217) with:

```rust
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let is_pressed = state.is_pressed.get();
        let is_hovered = state.is_hovered.get();

        let colors = tokens::button::colors(&Theme::of(ctx));
        let bg = self.resolve_bg(&colors, is_pressed, is_hovered);
        let (border_color, border_width) = self.resolve_border(&colors);
        let text_color = self.resolve_text_color(&colors, is_hovered);
        let corner_radius = self.resolve_corner_radius();
        let (pt, pr, pb, pl) = self.resolve_padding();
        let opacity = if self.disabled {
            tokens::button::DISABLED_OPACITY
        } else {
            1.0
        };
```

(The rest of `render` from line 219 onward stays unchanged — it already uses `bg`, `border_color`, `text_color`, etc. as local variables.)

- [ ] **Step 4: Build to verify button.rs compiles**

Run: `cargo build -p vexo_uikit`
Expected: FAIL — `navigation.rs` still references removed navigation color consts. `button.rs` itself should compile cleanly. Proceed to Task 5.

- [ ] **Step 5: Run button lib tests**

Run: `cargo test -p vexo_uikit --lib button`
Expected: PASS — existing button lib tests (in `src/`) render without a `Theme` ancestor; `Theme::of(ctx)` falls back to `light()`.

> **Note:** The old `PRIMARY_BG` was `Color::rgb(0.0, 0.478, 1.0)` (pure blue); `ThemeData::light().primary` is `Color::from_hex(0x6775FFFF)` (indigo, r≈0.404). **These differ** — buttons will change from pure blue to indigo after this task. This is deliberate (the spec's role table sets `primary = 0x6775FF`). The integration test `tests/button_render_tests.rs` asserts the old `PRIMARY_BG` value and will break — Task 6 migrates it. The `--lib` flag above avoids compiling the broken test files.

---

### Task 5: Wire `NavigationStackView` to `Theme::of(ctx)`

**Files:**
- Modify: `vexo_uikit/src/navigation.rs:44-53` (imports), `:484-546` (`render`), `:745-801` (`build_nav_bar`)

**Interfaces:**
- Consumes: `vexo_uikit::theme::tokens::navigation::{NavColors, colors}` (from Task 3), `vexo::Theme`.
- Produces: `NavigationStackView::render` reads `Theme::of(ctx)`, resolves `NavColors`, passes to `build_nav_bar`. `build_nav_bar` signature gains a `nav: &NavColors` parameter.

- [ ] **Step 1: Update imports in `navigation.rs`**

In `vexo_uikit/src/navigation.rs`, replace the `use vexo::{...}` block (lines 44-48) with:

```rust
use vexo::{
    AlignItems, AnimationController, Component, ComponentState, Curve, EaseInOutCurve, Flex,
    FractionalTranslation, IndexedStack, LifecycleContext, Opacity, Positioned, RenderContext,
    SafeArea, Stack, Text, Theme, Widget,
};
```

And replace line 52 (`use crate::theme::tokens;`) with:

```rust
use crate::theme::tokens;
use crate::theme::tokens::navigation::NavColors;
```

- [ ] **Step 2: Update `render` to resolve `NavColors` and pass to `build_nav_bar`**

In `vexo_uikit/src/navigation.rs`, find the line (around line 545-546):

```rust
        let safe_insets = ctx.safe_area();
        let nav_bar = self.build_nav_bar(&title, can_pop, &safe_insets);
```

Replace with:

```rust
        let safe_insets = ctx.safe_area();
        let nav = tokens::navigation::colors(&Theme::of(ctx));
        let nav_bar = self.build_nav_bar(&title, can_pop, &safe_insets, &nav);
```

- [ ] **Step 3: Update `build_nav_bar` signature and body**

In `vexo_uikit/src/navigation.rs`, replace the `build_nav_bar` method signature (line 745-750) and its body's color references. The full replacement for the method (lines 745-801) is:

```rust
    fn build_nav_bar(
        &self,
        title: &str,
        can_pop: bool,
        safe: &vexo::layout::EdgeInsets,
        nav: &NavColors,
    ) -> Box<dyn Widget> {
        let title_text = Text::new(title)
            .with_font_size(tokens::navigation::MOBILE_TITLE_FONT_SIZE)
            .with_color(nav.mobile_title);

        // Leading segment: back button (if any), left-aligned, grows to fill.
        // Padded on the left by the safe-area inset + header padding so it
        // clears the notch and has breathing room.
        let h_pad = tokens::navigation::MOBILE_HEADER_PADDING;
        let mut leading = Flex::row()
            .align(AlignItems::Center)
            .flex_grow(1.0)
            .flex_shrink(0.0)
            .padding_each(safe.left + h_pad, 0.0, 0.0, 0.0);
        if can_pop {
            let controller = self.controller.clone();
            let back_label = format!(
                "{} {}",
                tokens::navigation::BACK_CHEVRON,
                tokens::navigation::BACK_LABEL
            );
            let back_button = Button::new(back_label)
                .variant(ButtonVariant::Ghost)
                .on_press(move || {
                    controller.pop();
                })
                .boxed();
            leading = leading.push(back_button);
        }

        // Trailing segment: empty, grows to fill (balances the leading segment
        // so the title centers in the remaining space). Padded on the right
        // by the safe-area inset + header padding.
        let trailing = Flex::row().flex_grow(1.0).flex_shrink(0.0).padding_each(
            0.0,
            safe.right + h_pad,
            0.0,
            0.0,
        );

        // Outer bar: background edge-to-edge, height includes top inset.
        Flex::row()
            .align(AlignItems::Center)
            .padding_each(0.0, 0.0, safe.top, 0.0)
            .background(nav.mobile_header_bg)
            .height(tokens::navigation::MOBILE_HEADER_HEIGHT + safe.top)
            .flex_shrink(0.0)
            .push(leading)
            .push(title_text)
            .push(trailing)
            .boxed()
    }
```

> **Note:** `nav.back_color` is not used directly in `build_nav_bar` because the back button is a `Button` with `Ghost` variant, which reads `Theme::of(ctx)` itself during its own `render` (Task 4). `NavColors.back_color` is kept for callers that build custom back affordances. The `nav.mobile_title` and `nav.mobile_header_bg` are the two colors `build_nav_bar` needs that aren't owned by `Button`.

- [ ] **Step 4: Build the whole workspace**

Run: `cargo build`
Expected: PASS — both `button.rs` and `navigation.rs` now compile; `shared_app` should also compile (it doesn't construct `ThemeData` or call removed consts yet).

- [ ] **Step 5: Run vexo_uikit lib tests (test files still broken — Task 6)**

Run: `cargo test -p vexo_uikit --lib`
Expected: PASS for lib tests. The `tests/token_tests.rs` and `tests/button_render_tests.rs` files still reference removed consts (`PRIMARY_BG`, etc.) — they will FAIL to compile. **That's Task 6.** Do not commit yet.

- [ ] **Step 6: Commit Tasks 4 + 5 together (workspace now compiles except token_tests.rs)**

```bash
git add vexo_uikit/src/button.rs vexo_uikit/src/navigation.rs
git commit -m "feat(uikit): wire Button and NavigationStackView to Theme::of(ctx)

Button::render resolves ButtonColors from Theme::of(ctx); the three
resolve_* methods now take &ButtonColors. NavigationStackView::render
resolves NavColors and passes it to build_nav_bar. Both fall back to
ThemeData::light() when no Theme ancestor. Test files that assert
removed color consts are migrated in the next commit."
```

---

### Task 6: Migrate test files to resolver assertions

**Files:**
- Modify: `vexo_uikit/tests/token_tests.rs` (full rewrite — 13 lines)
- Modify: `vexo_uikit/tests/button_render_tests.rs:1-5` (imports), `:141`, `:194`, `:200`, `:237`, `:244`, `:251`, `:258`, `:279` (8 color-const references)

**Interfaces:**
- Consumes: `vexo_uikit::theme::tokens::button::colors`, `vexo::ThemeData` (from Tasks 2-3).

- [ ] **Step 1: Replace `token_tests.rs` contents**

Replace the entire contents of `vexo_uikit/tests/token_tests.rs` with:

```rust
use vexo::{Color, ThemeData};
use vexo_uikit::theme::tokens::{button, navigation};

#[test]
fn button_primary_bg_maps_to_theme_primary() {
    let c = button::colors(&ThemeData::light());
    assert_eq!(c.primary_bg, ThemeData::light().primary);
}

#[test]
fn button_disabled_opacity_is_half() {
    assert!((button::DISABLED_OPACITY - 0.5).abs() < 0.01);
}

#[test]
fn navigation_sidebar_bg_maps_to_theme_surface() {
    let n = navigation::colors(&ThemeData::light());
    assert_eq!(n.sidebar_bg, ThemeData::light().surface);
}

#[test]
fn resolvers_differ_between_light_and_dark() {
    let l = button::colors(&ThemeData::light());
    let d = button::colors(&ThemeData::dark());
    // primary_bg is the same (brand blue in both), but destructive differs.
    assert_ne!(l.destructive_bg, d.destructive_bg);

    let ln = navigation::colors(&ThemeData::light());
    let dn = navigation::colors(&ThemeData::dark());
    assert_ne!(ln.sidebar_bg, dn.sidebar_bg);
    assert_ne!(ln.mobile_header_bg, dn.mobile_header_bg);
}
```

- [ ] **Step 2: Add `ThemeData` import to `button_render_tests.rs`**

In `vexo_uikit/tests/button_render_tests.rs`, add `ThemeData` to the `vexo` imports. Replace line 2:

```rust
use vexo::{BuildOwner, DirtyTracking, ElementKey, RenderContext, RenderObjectRegistry};
```

with:

```rust
use vexo::{BuildOwner, DirtyTracking, ElementKey, RenderContext, RenderObjectRegistry, ThemeData};
```

- [ ] **Step 3: Replace removed color-const references in `button_render_tests.rs`**

The test renders `Button` without a `Theme` ancestor, so `Theme::of(ctx)` falls back to `ThemeData::light()`. Each removed `tokens::button::*` const maps to `ThemeData::light()`'s role or a resolver value. Make these replacements:

**Line 141** — replace:
```rust
        Some(tokens::button::PRIMARY_BG),
```
with:
```rust
        Some(ThemeData::light().primary),
```

**Line 194** — replace:
```rust
    assert_eq!(border.color, tokens::button::SECONDARY_BORDER);
```
with:
```rust
    assert_eq!(border.color, ThemeData::light().outline);
```

**Line 200** — replace:
```rust
        Some(tokens::button::SECONDARY_BG)
```
with:
```rust
        Some(vexo::Color::TRANSPARENT)
```

**Line 237** — replace:
```rust
    assert_eq!(text.color(), tokens::button::PRIMARY_TEXT);
```
with:
```rust
    assert_eq!(text.color(), ThemeData::light().on_primary);
```

**Line 244** — replace:
```rust
    assert_eq!(text.color(), tokens::button::SECONDARY_TEXT);
```
with:
```rust
    assert_eq!(text.color(), ThemeData::light().primary);
```

**Line 251** — replace:
```rust
    assert_eq!(text.color(), tokens::button::DESTRUCTIVE_TEXT);
```
with:
```rust
    assert_eq!(text.color(), ThemeData::light().on_error);
```

**Line 258** — replace:
```rust
    assert_eq!(text.color(), tokens::button::GHOST_TEXT);
```
with:
```rust
    assert_eq!(text.color(), ThemeData::light().primary);
```

**Line 279** — replace:
```rust
    assert_eq!(text.color(), tokens::button::GHOST_TEXT_HOVER);
```
with:
```rust
    assert_eq!(
        text.color(),
        vexo::Color::lerp(ThemeData::light().primary, vexo::Color::WHITE, 0.15)
    );
```

> The sizing/padding/opacity consts (`PADDING_V_DESKTOP`, `PADDING_H_DESKTOP`, `DISABLED_OPACITY`) are kept (not removed), so those references at lines 153-156 and 215 are unchanged.

- [ ] **Step 4: Run the migrated tests**

Run: `cargo test -p vexo_uikit --test token_tests && cargo test -p vexo_uikit --test button_render_tests`
Expected: PASS.

- [ ] **Step 5: Run the full workspace test suite**

Run: `cargo test`
Expected: PASS — all vexo, vexo_uikit, and shared_app tests pass. (shared_app has no tests yet; the demo wiring is Tasks 7-11.)

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/tests/token_tests.rs vexo_uikit/tests/button_render_tests.rs
git commit -m "test(uikit): migrate token_tests and button_render_tests to resolver assertions"
```

---

### Task 7: Add `is_dark: Signal<bool>` to `State` + wrap tree in `Theme`

**Files:**
- Modify: `shared_app/src/lib.rs:4-8` (imports), `:49-85` (`State` + `Default`), `:100-184` (`view`)

**Interfaces:**
- Consumes: `vexo::Theme`, `vexo::ThemeData` (re-exported from `vexo`).
- Produces: `State` has `is_dark: Signal<bool>` (default `false`). `view()` wraps its output in `Theme::new(theme, child)`. Later tasks pass `theme: ThemeData` and `is_dark: Signal<bool>` into `build_sidebar`.

- [ ] **Step 1: Add `Theme`, `ThemeData` to imports**

In `shared_app/src/lib.rs`, replace lines 4-8 (the `use vexo::{...}` block) with:

```rust
use vexo::{
    Application, Color, Column, Component, ComponentState, DecoratedContainer, Flex, IndexedStack,
    LifecycleContext, RenderContext, Row, SafeArea, ScrollView, Signal, Text, TextEdit,
    TextEditingController, Theme, ThemeData, Widget,
};
```

- [ ] **Step 2: Add `is_dark` field to `State`**

In `shared_app/src/lib.rs`, find the `State` struct (lines 49-64). Add `is_dark: Signal<bool>,` as a new field after `selection_log`:

```rust
#[derive(ComponentState)]
pub struct State {
    selection_log: Signal<u32>,
    is_dark: Signal<bool>,
    /// Desktop sidebar selection (mobile uses the nav stack for everything).
    selected: Signal<Option<&'static str>>,
    /// Desktop: one controller per sidebar item, indexed by `ITEMS` position.
    /// Each item's nav stack persists across sidebar toggles because the
    /// corresponding `NavigationStackView` stays mounted inside the
    /// `IndexedStack` (wrapped in `Offstage`).
    nav_controllers: Vec<NavigationController<Dest>>,
    /// Mobile: single shared nav stack. Semantically distinct from desktop's
    /// per-item stacks; must persist in `State` (not be created per `view()`)
    /// because `NavigationStackView`'s `on_mount` wires its dirty callback and
    /// its path must survive across rebuilds.
    mobile_nav_controller: NavigationController<Dest>,
}
```

- [ ] **Step 3: Update `Default for State` to initialize `is_dark`**

In `shared_app/src/lib.rs`, replace the `Default` impl body (lines 78-84) with:

```rust
        Self {
            selection_log: Signal::new(0),
            is_dark: Signal::new(false),
            selected: Signal::new(None),
            nav_controllers,
            mobile_nav_controller: NavigationController::new(),
        }
```

- [ ] **Step 4: Wrap `view()`'s output in `Theme::new`**

In `shared_app/src/lib.rs`, replace the `view` method (lines 100-184) with:

```rust
    fn view(state: &mut Self::State) -> Box<dyn Widget> {
        let selected_signal = state.selected.clone();
        let selection_count = state.selection_log.clone();
        let is_dark = state.is_dark.get();
        let theme = if is_dark {
            ThemeData::dark()
        } else {
            ThemeData::light()
        };
        let is_dark_signal = state.is_dark.clone();

        let inner: Box<dyn Widget> = match Platform::current() {
            Platform::Desktop => {
                let current = selected_signal.get_cloned();
                let index = selected_index(current);

                let selected_for_cb = selected_signal.clone();
                let sidebar = build_sidebar(
                    current,
                    Rc::new(move |id| {
                        selected_for_cb.set(Some(id));
                    }),
                    false,
                    theme,
                    is_dark_signal.clone(),
                );

                let mut stack = IndexedStack::new(index);
                for (i, (id, label)) in ITEMS.iter().enumerate() {
                    let ctrl = state.nav_controllers[i].clone();
                    let detail = build_detail_content(id, selection_count.clone(), ctrl.clone());
                    let nav_for_dest = ctrl.clone();
                    stack = stack.push(
                        NavigationStackView::new(ctrl, detail)
                            .root_title(label.to_string())
                            .title(|d| match d {
                                Dest::Page(n) => format!("Page: {}", n),
                                _ => String::new(),
                            })
                            .destination(move |d| match d {
                                Dest::Page(n) => build_page_content(*n, nav_for_dest.clone()),
                                _ => Text::new("").boxed(),
                            })
                            .boxed(),
                    );
                }

                SafeArea::new(
                    Flex::row()
                        .background(theme.background)
                        .push(sidebar)
                        .push(stack.flex_grow(1.0)),
                )
                .boxed()
            }
            Platform::Mobile => {
                let nav_for_select = state.mobile_nav_controller.clone();
                let sidebar = build_sidebar(
                    None,
                    Rc::new(move |id| {
                        nav_for_select.push(Dest::Item(id));
                    }),
                    true,
                    theme,
                    is_dark_signal.clone(),
                );

                let nav_for_dest = state.mobile_nav_controller.clone();
                let count_for_dest = selection_count.clone();

                NavigationStackView::new(state.mobile_nav_controller.clone(), sidebar)
                    .root_title("Navigation")
                    .title(|d| match d {
                        Dest::Item(id) => item_label(*id),
                        Dest::Page(n) => format!("Page: {}", n),
                    })
                    .destination(move |d| match d {
                        Dest::Item(id) => {
                            build_detail_content(*id, count_for_dest.clone(), nav_for_dest.clone())
                        }
                        Dest::Page(n) => build_page_content(*n, nav_for_dest.clone()),
                    })
                    .boxed()
            }
        };

        Theme::new(theme, inner).boxed()
    }
```

> **Note:** `build_sidebar` now takes `theme: ThemeData` and `is_dark: Signal<bool>` params. Task 8 updates the `build_sidebar` signature. This task changes the call sites; the next task changes the function. The workspace will not compile between Tasks 7 and 8 — that's expected. Commit after Task 8.

- [ ] **Step 5: Do not commit yet — proceed to Task 8**

The workspace does not compile because `build_sidebar` still has the old 3-param signature. Task 8 fixes it.

---

### Task 8: Theme `build_sidebar` and `build_item_row` via `NavColors`

**Files:**
- Modify: `shared_app/src/lib.rs:186-244` (`build_sidebar` + `build_item_row`)

**Interfaces:**
- Consumes: `vexo_uikit::theme::tokens::navigation::{colors, NavColors}` (from Task 3), `vexo::ThemeData`, `vexo::Theme` (from Task 1), `vexo_fontawesome::{Icon, Icons}` (already imported).
- Produces: `build_sidebar` signature is now `(selected, on_select, full_width, theme: ThemeData, is_dark: Signal<bool>)`. `build_item_row` takes `nav: &NavColors`. Both use resolved colors instead of hardcoded `Color::rgb(...)`.

- [ ] **Step 1: Add `vexo_uikit::theme::tokens::navigation` import**

In `shared_app/src/lib.rs`, replace line 10 (`use vexo_uikit::{...};`) with:

```rust
use vexo_uikit::{
    theme::tokens::navigation, Button, ButtonVariant, NavigationController, NavigationStackView,
    Platform,
};
```

- [ ] **Step 2: Rewrite `build_sidebar` with theme + toggle**

In `shared_app/src/lib.rs`, replace the entire `build_sidebar` function (lines 186-219) with:

```rust
fn build_sidebar(
    selected: Option<&str>,
    on_select: Rc<dyn Fn(&'static str)>,
    full_width: bool,
    theme: ThemeData,
    is_dark: Signal<bool>,
) -> Box<dyn Widget> {
    let nav = navigation::colors(&theme);
    let dark = is_dark.get();

    // Icon shows the TARGET mode (tap to go there): moon when light, sun when dark.
    let (icon, target_label) = if dark {
        (Icons::Sun, "Light")
    } else {
        (Icons::Moon, "Dark")
    };
    let icon_color = theme.on_surface;
    let toggle_is_dark = is_dark.clone();

    let toggle_button = DecoratedContainer::new(
        Icon::new(icon).with_size(20.0).with_color(icon_color),
    )
    .padding(8.0)
    .boxed()
    .on_press(move || {
        toggle_is_dark.set(!toggle_is_dark.get());
    });

    let header = Flex::row()
        .padding(12.0)
        .background(nav.header_bg)
        .push(
            Text::new("Navigation")
                .with_font_size(navigation::HEADER_FONT_SIZE)
                .with_color(nav.header_text),
        )
        .push(Flex::new().flex_grow(1.0))
        .push(toggle_button)
        .boxed();

    let mut list = Flex::column();
    // Mobile: no header, so prepend a toggle row to the list.
    // Styled like build_item_row but with an icon + label (spec: "icon + label").
    if full_width {
        let row_is_dark = is_dark.clone();
        let toggle_content = Row::new()
            .gap(8.0)
            .push(
                Icon::new(icon)
                    .with_size(16.0)
                    .with_color(nav.row_text),
            )
            .push(
                Text::new(target_label)
                    .with_font_size(navigation::ROW_FONT_SIZE)
                    .with_color(nav.row_text),
            );
        let toggle_row = DecoratedContainer::new(toggle_content)
            .background(nav.row_bg)
            .padding(navigation::ROW_PADDING)
            .boxed()
            .on_press(move || {
                row_is_dark.set(!row_is_dark.get());
            });
        list = list.push(toggle_row);
    }
    for &(id, label) in ITEMS {
        let is_selected = selected == Some(id);
        let on_select = on_select.clone();
        let row = build_item_row(label, is_selected, move || on_select(id), &nav);
        list = list.push(row);
    }

    let mut sidebar = Flex::column().background(nav.sidebar_bg);
    if full_width {
        sidebar = sidebar.flex_grow(1.0);
    } else {
        sidebar = sidebar.width(240.0).flex_shrink(0.0);
        sidebar = sidebar.push(header);
    }
    sidebar
        .push(ScrollView::new(list.boxed()).flex_grow(1.0))
        .boxed()
}
```

- [ ] **Step 3: Rewrite `build_item_row` to take `&NavColors`**

In `shared_app/src/lib.rs`, replace the entire `build_item_row` function (lines 221-244) with:

```rust
fn build_item_row(
    label: &str,
    is_selected: bool,
    on_press: impl FnMut() + 'static,
    nav: &navigation::NavColors,
) -> Box<dyn Widget> {
    let text_color = if is_selected {
        nav.selected_text
    } else {
        nav.row_text
    };
    let bg = if is_selected {
        nav.selected_bg
    } else {
        nav.row_bg
    };

    let label_text = Text::new(label)
        .with_font_size(navigation::ROW_FONT_SIZE)
        .with_color(text_color);

    DecoratedContainer::new(label_text)
        .background(bg)
        .padding(navigation::ROW_PADDING)
        .boxed()
        .on_press(on_press)
}
```

- [ ] **Step 4: Build the whole workspace**

Run: `cargo build`
Expected: PASS — `shared_app` compiles. The demo now wraps in `Theme`, themes the sidebar/item-rows, and has a toggle. DetailPage and PageContent still use hardcoded colors (Tasks 10-11), but the app builds and runs.

- [ ] **Step 5: Run all tests**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 6: Commit Tasks 7 + 8 together**

```bash
git add shared_app/src/lib.rs
git commit -m "feat(demo): add is_dark Signal, wrap tree in Theme, theme sidebar + toggle

State gains is_dark: Signal<bool>. view() picks ThemeData::light/dark
and wraps the tree in Theme::new. build_sidebar/build_item_row take
ThemeData/NavColors instead of hardcoded colors. Sun/moon toggle in the
desktop sidebar header; mobile prepends a toggle row to the list."
```

---

### Task 9: Theme `DetailPage` via `Theme::of(ctx)`

**Files:**
- Modify: `shared_app/src/lib.rs:355-418` (`DetailPage::render`)

**Interfaces:**
- Consumes: `vexo::Theme` (via `Theme::of(ctx)`), `vexo::ThemeData`.

- [ ] **Step 1: Update `DetailPage::render` to read theme**

In `shared_app/src/lib.rs`, replace the `DetailPage::render` method (lines 358-417) with:

```rust
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let title_widget = Text::new(self.id.as_str())
            .with_font_size(32.0)
            .with_color(theme.on_background);

        let body: Box<dyn Widget> = if self.id == "inbox" {
            let controller = state
                .text_controller
                .as_ref()
                .expect("inbox DetailPage must have a controller after on_mount")
                .clone();
            Column::new()
                .gap(8.0)
                .push(
                    Row::new()
                        .gap(8.0)
                        .push(
                            Icon::new(Icons::FloppyDisk)
                                .with_size(24.0)
                                .with_color(theme.on_background),
                        )
                        .push(
                            Text::new("Text Edit Showcase")
                                .with_font_size(24.0)
                                .with_color(theme.on_background),
                        ),
                )
                .push(TextEdit::new(controller))
                .boxed()
        } else {
            Column::new()
                .push(
                    Text::new(format!(
                        "This is the detail content for \"{}\".",
                        self.id
                    ))
                    .with_color(theme.on_background),
                )
                .boxed()
        };

        let count = self.selection_count.clone();
        let root_nav = self.nav_controller.clone();
        Column::new()
            .gap(16.0)
            .padding(24.0)
            .background(theme.background)
            .push(title_widget)
            .push(body)
            .push(
                Button::new("Bump counter")
                    .variant(ButtonVariant::Primary)
                    .on_press(move || {
                        count.set(count.get() + 1);
                    }),
            )
            .push(
                Text::new(format!("Counter: {}", self.selection_count.get()))
                    .with_color(theme.on_background),
            )
            .push(
                Button::new("Next page")
                    .variant(ButtonVariant::Primary)
                    .on_press(move || {
                        root_nav.push(Dest::Page(1));
                    }),
            )
            .boxed()
    }
```

- [ ] **Step 2: Build and run tests**

Run: `cargo build && cargo test`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat(demo): theme DetailPage via Theme::of(ctx)

Title, body text, icon, and counter text use theme.on_background;
detail pane background uses theme.background."
```

---

### Task 10: Convert `PageContent` to a `Component` reading `Theme::of(ctx)`

**Files:**
- Modify: `shared_app/src/lib.rs:420-435` (`build_page_content` function → `PageContent` component)

**Interfaces:**
- Consumes: `vexo::{Component, ComponentState, RenderContext, Theme}`, `vexo_uikit::NavigationController`, `vexo_uikit::Button`.
- Produces: `PageContent` struct + `PageContentState` (unit state). The `NavigationStackView::destination` closures in `view()` (already returning `build_page_content(n, ctrl)`) now return `PageContent { n, nav_controller }.boxed()`.

- [ ] **Step 1: Replace `build_page_content` with a `PageContent` component**

In `shared_app/src/lib.rs`, replace the `build_page_content` function (lines 420-435) with:

```rust
// ============================================================================
// PAGE CONTENT COMPONENT
// ============================================================================

/// Pushed page content. Implemented as a `Component` (not a free function)
/// so it establishes an inherited-widget dependency via `Theme::of(ctx)` and
/// auto-rebuilds when the theme toggles after the page has been pushed.
#[derive(Default, ComponentState)]
struct PageContentState;

struct PageContent {
    n: u32,
    nav_controller: NavigationController<Dest>,
}

impl Clone for PageContent {
    fn clone(&self) -> Self {
        Self {
            n: self.n,
            nav_controller: self.nav_controller.clone(),
        }
    }
}

impl Component for PageContent {
    type State = PageContentState;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let ctrl = self.nav_controller.clone();
        let n = self.n;
        Column::new()
            .gap(16.0)
            .padding(24.0)
            .background(theme.background)
            .push(
                Text::new(format!("Page: {}", n))
                    .with_font_size(24.0)
                    .with_color(theme.on_background),
            )
            .push(
                Text::new(format!("You are on pushed page \"{}\".", n))
                    .with_color(theme.on_background),
            )
            .push(
                Button::new("Next page")
                    .variant(ButtonVariant::Primary)
                    .on_press(move || {
                        ctrl.push(Dest::Page(n + 1));
                    }),
            )
            .boxed()
    }
}
```

- [ ] **Step 2: Update `view()`'s destination closures to return `PageContent`**

In `shared_app/src/lib.rs`, in the `view()` method, find the desktop `destination` closure (inside the `for (i, (id, label)) in ITEMS.iter().enumerate()` loop):

```rust
                            .destination(move |d| match d {
                                Dest::Page(n) => build_page_content(*n, nav_for_dest.clone()),
                                _ => Text::new("").boxed(),
                            })
```

Replace `build_page_content(*n, nav_for_dest.clone())` with `PageContent { n: *n, nav_controller: nav_for_dest.clone() }.boxed()`:

```rust
                            .destination(move |d| match d {
                                Dest::Page(n) => {
                                    PageContent { n: *n, nav_controller: nav_for_dest.clone() }.boxed()
                                }
                                _ => Text::new("").boxed(),
                            })
```

Then find the mobile `destination` closure:

```rust
                    .destination(move |d| match d {
                        Dest::Item(id) => {
                            build_detail_content(*id, count_for_dest.clone(), nav_for_dest.clone())
                        }
                        Dest::Page(n) => build_page_content(*n, nav_for_dest.clone()),
                    })
```

Replace `build_page_content(*n, nav_for_dest.clone())` with `PageContent { n: *n, nav_controller: nav_for_dest.clone() }.boxed()`:

```rust
                    .destination(move |d| match d {
                        Dest::Item(id) => {
                            build_detail_content(*id, count_for_dest.clone(), nav_for_dest.clone())
                        }
                        Dest::Page(n) => {
                            PageContent { n: *n, nav_controller: nav_for_dest.clone() }.boxed()
                        }
                    })
```

- [ ] **Step 3: Build and run tests**

Run: `cargo build && cargo test`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat(demo): convert PageContent to a Component reading Theme::of(ctx)

Pushed pages now auto-rebuild when the theme toggles (the destination
closure baked colors at push-time before, so a post-push toggle left
pushed pages on the old theme). PageContentState is a unit struct."
```

---

### Task 11: Final workspace build + test verification

**Files:**
- None modified — verification only.

**Interfaces:**
- None.

- [ ] **Step 1: Clean build the whole workspace**

Run: `cargo build`
Expected: PASS with no warnings related to theme code.

- [ ] **Step 2: Run the full test suite**

Run: `cargo test`
Expected: PASS — all vexo, vexo_uikit, and shared_app tests pass.

- [ ] **Step 3: Check for leftover hardcoded colors in the demo**

Run: `rg -n "Color::rgb|Color::WHITE|Color::BLACK" shared_app/src/lib.rs`
Expected: Only matches in contexts where the color is genuinely theme-independent (e.g. if any remain). If hardcoded `Color::WHITE`/`Color::rgb(0.9, ...)` remain in sidebar/detail/page code, they were missed — fix them to use the `theme`/`nav` resolver before finishing. (The `DetailPage` body text and `PageContent` should now use `theme.on_background`/`theme.background`.)

- [ ] **Step 4: Hand off to user for visual verification**

Per `CLAUDE.md`, the assistant never runs `cargo run -p desktop_demo`. Report to the user:

> Theme toggle implementation complete. Please run `cargo run -p desktop_demo` and verify:
> 1. Default light theme on launch.
> 2. Click the moon icon in the sidebar header (desktop) / "Dark" row at the top of the list (mobile): sidebar, header, detail bg, body text, button, mobile nav header all switch to dark.
> 3. Click the sun icon / "Light" row: switches back to light.
> 4. Push a page ("Next page" button), then toggle: the pushed page should also switch theme (proves PageContent reactivity).
> 5. Note whether the TextEdit text is readable in dark mode (known gap — TextEdit may need a foreground-color prop in a follow-up).

- [ ] **Step 5: If the user reports the TextEdit is unreadable in dark mode**

This is the known gap documented in the spec. Do NOT expand scope here. Note it as a follow-up:

> TextEdit dark-mode readability is a known gap (spec "Out of Scope"). File a follow-up spec for a `TextEdit::foreground_color` API if needed. The core theme toggle (sidebar, header, detail, body text, buttons, nav header) is complete.

- [ ] **Step 6: Final commit (only if Step 3 found leftover hardcoded colors that were fixed)**

```bash
git add shared_app/src/lib.rs
git commit -m "fix(demo): replace leftover hardcoded colors with theme resolver values"
```

If no fixes were needed, no commit — the implementation is complete.
