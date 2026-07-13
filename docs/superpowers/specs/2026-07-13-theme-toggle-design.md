# Dark/Light Theme Toggle Design

**Date:** 2026-07-13
**Status:** Approved (pending user spec review)
**Scope:** `vexo/`, `vexo_uikit/`, `shared_app/` (3 crates)

## Motivation

The demo app (`shared_app/src/lib.rs`) hardcodes light colors everywhere
(`Color::WHITE`, `Color::rgb(0.9, 0.9, 0.92)`, etc.) with no way to switch to a
dark theme. Two parallel color systems exist in the codebase but neither is
wired to a toggle, and they don't talk to each other:

1. **`vexo::Theme` / `ThemeData`** — an `InheritedWidget` (just landed in
   `2026-07-12-inherited-widget-design.md`) with `light()`/`dark()` presets.
   Propagates through the tree; descendants calling `Theme::of(ctx)` auto-rebuild
   when the data changes. But it has only 8 generic Material-ish roles, and the
   `dark()` preset is an unusable placeholder (`primary: 0x121434` — near-black).
   **Not used by the demo at all.**

2. **`vexo_uikit::theme::tokens`** — flat `const Color` values for specific UI
   pieces (`SIDEBAR_BG`, `MOBILE_HEADER_BG`, `PRIMARY_BG`, etc.), light-only, no
   dark variants. Read **internally** by `Button` (`button.rs:121-166`) and
   `NavigationStackView` (`navigation.rs:745-795`) at render time.

The demo also **duplicates** uikit's nav token values exactly (e.g. its sidebar
header `Color::rgb(0.9, 0.9, 0.92)` == `tokens::navigation::HEADER_BG`) rather
than using either system — a smell this work cleans up.

**Goal:** add a dark/light toggle to the demo that themes the sidebar, header,
detail pane, body text, buttons, and mobile nav header — covering both the
demo's own colors and the `vexo_uikit` widgets it uses.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Scope | Demo app **+** `vexo_uikit` widgets | User-selected. `Button`/`NavigationStackView` read light-only const tokens internally; leaving them untouched would leave the mobile nav header white in dark mode. |
| Theme architecture | Approach A: single `Theme` InheritedWidget, extend `ThemeData` with 3 roles, uikit tokens become resolvers | One source of truth; leverages the InheritedWidget built for exactly this; removes demo↔uikit duplication; new roles are generic Material-3 (reusable). Rejected B (second uikit InheritedWidget — double plumbing, drift risk) and C (brightness-inferred token struct — two parallel token sets that can drift). |
| New `ThemeData` roles | `surface_variant`, `outline`, `on_surface_variant` | Standard Material-3 semantic roles. Every existing uikit token maps to one of the 11 roles. |
| Brand `primary` in dark | `0x6775FF` (same as light) | Accent consistency across modes (macOS/iOS convention); selected row / primary button / back chevron read correctly in both themes. Replaces the placeholder `0x121434`. |
| Hover/pressed shades | Derived via `Color::lerp(primary, WHITE/BLACK, 0.15)` | `Color::lerp` already exists; stays correct if `primary` ever changes; avoids storing per-theme hover/pressed consts. |
| Toggle placement | Sidebar header (desktop) / top of list (mobile), sun/moon icon | User-selected. `Icons::Sun` (f185) and `Icons::Moon` (f186) are available in `vexo_fontawesome`. |
| Toggle state | `Signal<bool>` in `State`, default `false` (light) | `RootComponent` auto-wires Signal fields; `Signal::set()` re-runs `view()` (lib.rs:235-238). |
| `PageContent` reactivity | Convert `build_page_content` to a `Component` reading `Theme::of(ctx)` | The `NavigationStackView::destination` closure bakes colors at push time; only a Component establishing an inherited dependency auto-rebuilds on a post-push toggle. |
| Persistence | None | YAGNI for a demo. |
| Modes | 2-way (light ↔ dark) only | YAGNI; no "system" mode. |

## Architecture

```
                ┌───────────────────────────────────────────┐
                │  State (shared_app)                       │
                │  - is_dark: Signal<bool>                  │
                └───────────────────────────────────────────┘
                              │ view() reads is_dark.get()
                              ▼
                ┌───────────────────────────────────────────┐
                │  Theme::new(theme_data, child)            │
                │  theme_data = light() or dark()           │
                │  (InheritedWidget — single source)        │
                └───────────────────────────────────────────┘
                              │ propagates ThemeData to descendants
          ┌───────────────────┼────────────────────────┐
          ▼                   ▼                        ▼
   ┌─────────────┐   ┌──────────────────┐   ┌────────────────────┐
   │ Sidebar     │   │ NavigationStack  │   │ DetailPage /       │
   │ (free fns,  │   │ View (Component) │   │ PageContent        │
   │  ThemeData  │   │ - Theme::of(ctx) │   │ (Components)       │
   │  passed in) │   │ - NavColors      │   │ - Theme::of(ctx)   │
   │             │   │ - build_nav_bar  │   │ - background/text  │
   │ + toggle    │   │   uses NavColors │   │   from ThemeData   │
   └─────────────┘   └──────────────────┘   └────────────────────┘
                              │
                              ▼
                   ┌────────────────────┐
                   │ Button (Component) │
                   │ - Theme::of(ctx)   │
                   │ - ButtonColors     │
                   └────────────────────┘
```

**Two color-resolution paths converge on the same `ThemeData`:**

1. **`view()` path** (sidebar, item rows): free functions with no `RenderContext`
   receive `ThemeData` as a parameter. `view()` re-runs on every `is_dark`
   `Signal::set()`, so these get fresh colors each toggle.
2. **`Theme::of(ctx)` path** (DetailPage, PageContent, Button,
   NavigationStackView): Components establish an inherited-widget dependency and
   auto-rebuild when the `Theme` ancestor's data changes (`update_should_notify`).

## `ThemeData` Role Extension

Add 3 Material-3 semantic roles (8 → 11 fields). Full preset table:

| Role | Light | Dark | Used for |
|---|---|---|---|
| `primary` | `0x6775FF` | `0x6775FF` *(changed from `0x121434`)* | selected row bg, button primary bg, back chevron |
| `on_primary` | `WHITE` | `WHITE` | selected row text, primary button text |
| `background` | `WHITE` | `0x1C1B1F` | detail pane bg |
| `on_background` | `BLACK` | `WHITE` | body text |
| `surface` | `WHITE` | `0x2B2930` | sidebar bg, mobile header bg |
| `on_surface` | `0x1C1B1F` | `WHITE` | sidebar row text, mobile title |
| `surface_variant` *(new)* | `0xE6E6EB` | `0x38353C` | sidebar header bg (slightly darker than surface) |
| `outline` *(new)* | `0xC7C7CC` | `0x49454F` | dividers, secondary button border |
| `on_surface_variant` *(new)* | `0x999999` | `0x9E9CA6` | placeholder/muted text |
| `error` | `0xB3261E` | `0xF2B8B5` | destructive button bg |
| `on_error` | `WHITE` | `BLACK` | destructive button text |

All `Color::from_hex` values use 8-digit RRGGBBAA (per commit `7702201`).

### uikit token → role mapping

Every existing uikit token maps to one role:

| Token | Role |
|---|---|
| `button::PRIMARY_BG` / `GHOST_TEXT` / `SECONDARY_TEXT` / `navigation::SELECTED_BG` / `BACK_COLOR` | `primary` |
| `button::PRIMARY_TEXT` / `navigation::SELECTED_TEXT_COLOR` | `on_primary` |
| `navigation::DETAIL_BG` | `background` |
| `navigation::SIDEBAR_BG` / `MOBILE_HEADER_BG` | `surface` |
| `navigation::ROW_TEXT_COLOR` / `MOBILE_TITLE_COLOR` | `on_surface` |
| `navigation::HEADER_BG` | `surface_variant` |
| `button::SECONDARY_BORDER` / `navigation::DIVIDER_COLOR` / `MOBILE_HEADER_DIVIDER` | `outline` |
| `navigation::PLACEHOLDER_TEXT_COLOR` | `on_surface_variant` |
| `button::DESTRUCTIVE_BG` | `error` |
| `button::DESTRUCTIVE_TEXT` | `on_error` |
| `button::SECONDARY_BG` / `GHOST_BG` / `navigation::ROW_BG` | `TRANSPARENT` |

Sizing/padding/font-size/string consts (`SIDEBAR_WIDTH`, `MOBILE_HEADER_HEIGHT`,
paddings, `BACK_CHEVRON`, `BACK_LABEL`, etc.) stay as `const` — they are
theme-independent.

## `vexo_uikit` Token Resolvers

Color `const`s in `vexo_uikit/src/theme/tokens.rs` become resolver structs:

```rust
// button
pub struct ButtonColors {
    pub primary_bg: Color, pub primary_bg_hover: Color, pub primary_bg_pressed: Color,
    pub primary_text: Color,
    pub secondary_bg: Color, pub secondary_border: Color, pub secondary_text: Color,
    pub destructive_bg: Color, pub destructive_bg_hover: Color,
    pub destructive_bg_pressed: Color, pub destructive_text: Color,
    pub ghost_bg: Color, pub ghost_text: Color, pub ghost_text_hover: Color,
}
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

// navigation
pub struct NavColors {
    pub sidebar_bg: Color, pub header_bg: Color, pub header_text: Color,
    pub row_bg: Color, pub row_text: Color,
    pub selected_bg: Color, pub selected_text: Color,
    pub detail_bg: Color, pub divider: Color, pub placeholder_text: Color,
    pub mobile_header_bg: Color, pub mobile_title: Color, pub back_color: Color,
}
pub fn colors(t: &ThemeData) -> NavColors { /* map per table above */ }
```

## `vexo_uikit` Widget Changes

- **`Button::render(&self, state, ctx)`** (`button.rs:204`): `_ctx` → `ctx`;
  `let c = tokens::button::colors(&Theme::of(ctx));`; replace `tokens::button::*`
  color refs in `resolve_bg`/`resolve_border`/`resolve_text_color` with `c.*`.

- **`NavigationStackView::render`** (`navigation.rs:484`): resolve
  `let nav = tokens::navigation::colors(&Theme::of(ctx));` once; pass `nav` into
  `build_nav_bar(...)`. `build_nav_bar` (`navigation.rs:745`) gains a
  `nav: NavColors` parameter and uses `nav.mobile_title_color`,
  `nav.mobile_header_bg`, `nav.back_color` instead of the removed consts.

### Compatibility

The `const` color tokens are **removed**. Callers must migrate to the resolvers.
Grep confirmed callers exist only within `vexo_uikit/src/{button,navigation}.rs`
and `vexo_uikit/tests/token_tests.rs`. The test file is updated to assert
resolver output against `ThemeData::light()`/`dark()`.

**Fallback property:** `Theme::of(ctx)` falls back to `ThemeData::light()` when
no `Theme` ancestor exists, and `light().primary` == the old `PRIMARY_BG` const.
So existing tests that render `Button` without wrapping a `Theme` see identical
colors — minimal test breakage expected.

## Demo App Wiring (`shared_app/src/lib.rs`)

### State

Add `is_dark: Signal<bool>` to `State` (default `false` = light). Update the
manual `Default` impl.

### `view()`

```rust
fn view(state: &mut Self::State) -> Box<dyn Widget> {
    let theme = if state.is_dark.get() { ThemeData::dark() } else { ThemeData::light() };
    let inner = match Platform::current() { /* desktop/mobile branches, passing `theme` to build_sidebar */ };
    Theme::new(theme, inner).boxed()
}
```

`Theme::new(theme, inner)` is the single root that all descendants read via
`Theme::of(ctx)`.

### Sidebar / item rows

`build_sidebar` and `build_item_row` gain a `theme: ThemeData` param and use
`tokens::navigation::colors(&theme)` instead of hardcoded `Color::rgb(...)`.
This removes the existing duplication. `view()` passes the resolved `ThemeData`
in (fresh each toggle, since `view()` re-runs on the `is_dark` Signal).

### Toggle control

- **Desktop** (`full_width=false`): header becomes `Flex::row()` of
  `[Text "Navigation"]` + `[icon button]`. Icon shows the **target** mode
  (`Icons::Moon` when light → dark; `Icons::Sun` when dark → light). Icon button
  = tappable `DecoratedContainer` + `.on_press`, colored `theme.on_surface`.
- **Mobile** (`full_width=true`, no header): a toggle row prepended to the list,
  styled like `build_item_row` (icon + "Light"/"Dark" label).
- Callback: `is_dark.set(!is_dark)` → `view()` re-runs → `Theme` re-wraps with
  new data → `update_should_notify` fires → all `Theme::of` dependents rebuild.

### DetailPage

Already a `Component`. `render` reads `Theme::of(ctx)`; `background(Color::WHITE)`
→ `theme.background`; title/body text → `theme.on_background`; icon color →
`theme.on_background`.

### PageContent (new Component)

`build_page_content` currently returns a plain widget tree baked at push-time,
so it won't react to a post-push toggle. Convert to a `PageContent` Component
with a unit state:

```rust
#[derive(Default, ComponentState)]
struct PageContentState;

struct PageContent { n: u32, nav_controller: NavigationController<Dest> }
impl Component for PageContent {
    type State = PageContentState;
    fn render(&self, _state, ctx) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        Column::new()... /* colors from theme */ .boxed()
    }
}
```

The `NavigationStackView::destination` closure returns
`PageContent { ... }.boxed()`. Pushed pages now auto-rebuild on theme change.

### Known gap

`TextEdit` may default to black text and need a foreground-color prop for dark
mode. If `TextEdit` does not expose text color, that is a follow-up; the core
sidebar/detail/page/button/nav theming proceeds regardless. Verify during visual
review.

## Testing

### Unit tests — `vexo`

- `ThemeData`: `light()` ≠ `dark()` on the 3 new fields; `default() == light()`;
  new fields present.
- `Color::lerp` already covered (used for hover/pressed).

### Unit tests — `vexo_uikit`

- Token resolvers: `tokens::button::colors(&ThemeData::light())` maps roles
  correctly (`primary_bg == light.primary`, `secondary_border == light.outline`,
  `destructive_bg == light.error`, hover/pressed == `lerp(...)`); same for
  `dark()`. Same for `tokens::navigation::colors`.
- Update `vexo_uikit/tests/token_tests.rs` to assert resolver output against
  `ThemeData` roles.
- Existing Button render integration tests pass unchanged (fallback property).

### Build & test gates

`cargo build` (whole workspace) then `cargo test` (whole workspace) must pass.

### Visual verification — user runs

Per `CLAUDE.md`, the assistant never runs `cargo run -p desktop_demo`. The user
runs it and verifies:

1. Default light theme on launch.
2. Toggle to dark: sidebar, header, detail bg, body text, button, mobile nav
   header all dark.
3. Toggle back to light.
4. Push a page, then toggle: pushed page updates (proves `PageContent`
   Component reactivity).
5. `TextEdit` readability in dark mode (known gap).

## Out of Scope

- No persistence across restarts.
- No 3-way "system" mode.
- No animation on toggle.
- No `TextEdit` foreground-color work unless it blocks visual acceptance.
