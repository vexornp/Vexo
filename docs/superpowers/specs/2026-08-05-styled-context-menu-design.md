# Styled Context Menu — Design

**Date:** 2026-08-05
**Scope:** Upgrade the ChatScreen placeholder context menu's visual style to
match a chat-app-style context menu image (top reaction row, divider, item rows
with leading icons + hover highlight). All new helpers live inline in
`shared_app/src/chats/chat_screen.rs` (Approach A). Builds on the
`ContextMenu` host/`ContextMenuController`/`MenuBuilder` trio shipped in
`2026-08-05-custom-context-menu-view-design.md` — only the builder's *content*
changes; the framework plumbing is untouched.

## Goal

Replace the bare `placeholder_menu_builder()` (3 plain `Text` rows with 8px
padding, no icons, no hover) with a richer menu: a FontAwesome reaction icon
row at the top, a hairline divider, and three item rows each with a leading
FA icon, label, and hover-tint background. Delete uses `theme.error` color to
read as destructive.

## Non-goals (explicitly out of scope)

- **No real emoji.** The text render path applies a single monochrome `color`
  per glyph (`vexo/src/render_objects/text.rs:33` — no COLR/sbix/color-font
  support), and `new_font_system()` (`vexo/src/resource.rs:18`) loads only
  the embedded Roboto font (no emoji font, no system fallback). FA reaction
  icons stand in for emoji. Decided in brainstorming.
- **No hover highlight on reaction icons.** Reactions stay a flat
  stateless strip (cursor still flips to pointer). Decided in brainstorming.
- **No new `vexo_uikit` module or `theme::context_menu` tokens.** All helpers
  are local to `chat_screen.rs`; colors are derived inline from `ThemeData`.
  Decided in brainstorming (Approach A).
- **No keyboard shortcuts** (e.g. ⌘C) — not selected in brainstorming; would
  need a key-event path not built here.
- **No hover integration test.** Hover is driven by `Signal<bool>` toggled in
  `on_enter`/`on_exit`; testing it requires synthesizing pointer-move events
  through `pipeline.handle_event`, disproportionately heavy for a demo.
  Rely on manual visual verification.
- **No `should_rebuild()` override on `MenuRow`.** The menu is a short-lived
  overlay, not a measured hot path (keyboard/scroll). Default `true` is
  correct. Per CLAUDE.md's three-level ladder, level-3 overrides are reserved
  for hot paths.
- **No edge-aware positioning, no `Escape` dismiss, no item right-click.**
  All documented as v1 limitations or future work in the prior spec;
  unchanged here.
- **No migration impact.** `placeholder_menu_builder()`'s signature
  (`fn() -> MenuBuilder`) is unchanged; the breaking `MenuItem` removal
  happened in the prior spec. This change only rewrites a private fn body and
  adds private helpers.

## Feasibility findings (from brainstorming)

| Visual element | Feasible? | How |
|---|---|---|
| Leading icon per item | ✅ | `vexo_fontawesome::Icon` — already registered in `shared_app/src/app.rs:32`. `Icons::Copy`, `Icons::Reply`, `Icons::Trash` all exist in the generated enum. |
| Separator dividers | ✅ | Thin 1px `DecoratedBox` with a pre-composited outline color. Trivial. |
| Hover/selection highlight on items | ✅ | `MouseRegion` is `pub(crate)`, but the fluent `Widget::on_enter(..)`/`on_exit(..)`/`cursor(..)` API (`vexo/src/widgets/mod.rs:237`) wraps a child in `MouseRegion` internally — callers get hover without touching the crate-private type. Exactly how `Button` does it (`vexo_uikit/src/button.rs:291`). |
| Emoji row at top | ⚠️ Blocked | Two blockers: (1) `new_font_system()` loads only Roboto — no emoji font; (2) text render path is monochrome-only (`vexo/src/render_objects/text.rs:33`). Real emoji would render as tofu/monochrome. **FA icons stand in.** |

Confirmed available FA icons (from `vexo_fontawesome/assets/icons.json`):
`thumbs-up`, `heart`, `face-laugh`, `face-surprise`, `face-sad-tear`,
`face-angry`, `copy`, `reply`, `trash`. PascalCase enum variants:
`Icons::ThumbsUp`, `Icons::Heart`, `Icons::FaceLaugh`, `Icons::FaceSurprise`,
`Icons::FaceSadTear`, `Icons::FaceAngry`, `Icons::Copy`, `Icons::Reply`,
`Icons::Trash`. (If a variant name differs at compile time, the fix is a
one-line rename during implementation, not a design change.)

## Chosen approach: inline local helpers (Approach A)

All new code lives as private items in `shared_app/src/chats/chat_screen.rs`.
No changes to `vexo_uikit` or `vexo`. We reuse existing primitives:
`row!`/`column!` macros, `DecoratedBox`, `GestureDetector`, `Icon`,
`WithLayout`, `Signal`, `Component`/`ComponentState`, and the fluent
`Widget::on_enter`/`on_exit`/`cursor` API.

### Rejected alternatives

- **Approach B — reusable widgets in `vexo_uikit`:** would add a
  `ContextMenuRow` Component + `ContextMenuDivider` widget + `theme::context_menu`
  tokens submodule, mirroring the `button`/`navigation` pattern. More reusable
  and testable in isolation, but the user chose Approach A to keep the demo
  contained. `MenuRow` can graduate to `vexo_uikit` later without API churn
  if it proves reusable.
- **Approach C — `styled_menu_builder(reactions, items)` helper:** a one-call
  API building the whole menu. Convenient but over-engineered against the
  just-shipped `MenuBuilder` design (the prior spec deliberately chose
  caller-supplied builders over an items API).

## Architecture

### New local items in `chat_screen.rs`

| Item | Kind | Purpose |
|---|---|---|
| `MenuRow` | `Component` (stateful) | One menu item: leading FA icon + label, hover-highlight background, `on_tap` → action + `controller.close()`. |
| `MenuRowState` | struct | `#[derive(ComponentState, Default)]` — `hovered: Signal<bool>`, auto-wired. Mirrors `ButtonState` (`vexo_uikit/src/button.rs:37`). |
| `menu_divider(theme)` | fn → `Box<dyn Widget>` | Thin 1px hairline separator. |
| `reaction_row(ctrl, theme)` | fn → `Box<dyn Widget>` | `row!` of 6 FA reaction icons, each a `GestureDetector` → log + close. Stateless (no hover). |
| `close_after(ctrl, msg)` | fn → `Rc<dyn Fn()>` | Sugar for the `on_tap` closure (log + close) to keep the `column!` readable. |
| `placeholder_menu_builder()` | fn (rewritten) | Assembles: `DecoratedBox` wrapping `column!` of `[reaction_row, divider, MenuRow×3]`. |

### Unchanged

- The `ContextMenu` host/controller/`context_menu_trigger` trio
  (`vexo_uikit/src/context_menu.rs`).
- The `ContextMenu::new(...)` wrap at the app root
  (`shared_app/src/chats/chat_screen.rs`).
- `on_secondary_press` plumbing (`vexo/src/widgets/...`).
- The builder's signature — `MenuBuilder::new(|ctrl, theme| { ... })` runs at
  render time, reads the live `ThemeData`, and re-runs on theme toggle (proven
  by `test_builder_reads_current_theme` at `context_menu.rs:476`).

## Visual layout & widget tree

### Menu overall

- Outer `DecoratedBox`: `theme.surface` bg, `theme.outline` 1px border,
  **12px** corner radius (up from 8 — matches the image's softer corners),
  shadow `BLACK@0.20`, blur 12, offset `(0, 4)`.
- `min_width: 200.0` on the inner `WithLayout` — rows are equal width;
  reactions + divider stretch to match via the parent column's cross-axis
  stretch.

### Tree

```
DecoratedBox(surface, outline@1, radius=12, shadow=BLACK@0.20 blur=12 offset=(0,4))
└── WithLayout(min_width=200)
    └── column!(gap=0)
        ├── reaction_row(ctrl, theme)            // top section
        │   └── WithLayout(padding_each=8h, 4v)
        │       └── row!(gap=8.0).justify(JustifyContent::Center)
        │           ├── reaction_icon(ThumbsUp,    "context menu: 👍")
        │           ├── reaction_icon(Heart,       "context menu: ❤")
        │           ├── reaction_icon(FaceLaugh,   "context menu: 😆")
        │           ├── reaction_icon(FaceSurprise,"context menu: 😮")
        │           ├── reaction_icon(FaceSadTear, "context menu: 😢")
        │           └── reaction_icon(FaceAngry,   "context menu: 😠")
        ├── menu_divider(theme)                   // hairline separator
        └── column!(gap=0)                        // items section
            ├── MenuRow(Copy,   Icons::Copy,   destructive=false)
            ├── MenuRow(Reply,  Icons::Reply,  destructive=false)
            └── MenuRow(Delete, Icons::Trash,  destructive=true)
```

### `reaction_row(ctrl, theme)`

A `row!` of 6 FA reaction icons. Each icon:
- `Icon::new(icon).with_size(16.0).with_color(theme.on_surface_variant)`
- wrapped in `WithLayout(padding=6.0)` for the tappable area
- wrapped in `GestureDetector.on_tap(move || { log::debug!("{}", msg); ctrl.close(); })`

The `row!` is wrapped in `WithLayout` with `padding_each(8.0, 8.0, 4.0, 4.0)`
(8h, 4v) and configured with `.justify(JustifyContent::Center)` so the 6 icons
center within the 200px menu width. Alignment uses `JustifyContent` (the
framework's name for main-axis alignment — set via `.justify()` on
`MultiChild`, `vexo/src/widgets/multi_child.rs:134`), not `MainAxisAlignment`.

Reactions are **stateless** — no hover background, no `Signal`. The cursor
flips to pointer via `.cursor(MouseCursor::System(SystemCursorKind::Pointer))`
on each icon's wrapper.

### `menu_divider(theme)`

```rust
fn menu_divider(theme: ThemeData) -> Box<dyn Widget> {
    let color = Color::lerp(theme.outline, theme.surface, 1.0 - 0.35); // outline @ ~0.35 alpha, pre-composited
    WithLayout::new(
        DecoratedBox::new(Text::new(""), Style::default().background(color)),
        Layout::default().height(1.0).width_percent(1.0),
    ).boxed()
}
```

Same formula as `NavColors::divider` (`vexo_uikit/src/theme/tokens.rs:122`):
`Color::lerp(outline, surface, 1.0 - DIVIDER_ALPHA)` with `DIVIDER_ALPHA = 0.35`.
Hairline is 1px tall (Taffy floors sub-pixel heights to 0, per
`HAIRLINE_THICKNESS` comment at `tokens.rs:192`).

### `MenuRow` component (stateful — the only new stateful piece)

Mirrors `Button` (`vexo_uikit/src/button.rs:37`) exactly, just simpler.

**State + struct:**
```rust
#[derive(ComponentState, Default)]
struct MenuRowState {
    hovered: Signal<bool>,
}

#[derive(Clone)]
struct MenuRow {
    icon: Icons,
    label: &'static str,
    destructive: bool,
    on_tap: Rc<dyn Fn()>,        // caller closure (log + ctrl.close())
    theme: vexo::ThemeData,      // snapshot taken in builder at render time
}
```

- `on_tap: Rc<dyn Fn()>` (not `FnMut`) — matches the old `MenuItem` pattern;
  `Rc` for cheap clones into the `GestureDetector` closure. Single-threaded,
  no `Send + Sync` needed.
- `theme` is a snapshot: the builder runs at render time and reads the live
  theme, so passing it in is correct and avoids re-reading in `render`.
  `ThemeData::clone()` is cheap (small struct of `Color` fields).

**`render`:**
```rust
impl Component for MenuRow {
    type State = MenuRowState;

    fn render(&self, state: &mut MenuRowState, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let hovered = state.hovered.get();
        let row_hover_bg = Color::lerp(self.theme.primary, self.theme.surface, 0.92); // ~8% primary wash
        let bg = if hovered { row_hover_bg } else { Color::TRANSPARENT };
        let (icon_color, text_color) = if self.destructive {
            (self.theme.error, self.theme.error)
        } else {
            (self.theme.on_surface_variant, self.theme.on_surface)
        };
        let on_enter = state.hovered.clone();
        let on_exit = state.hovered.clone();
        let on_tap = self.on_tap.clone();

        let content = WithLayout::new(
            row! {
                Icon::new(self.icon).with_size(14.0).with_color(icon_color),
                Text::new(self.label).with_color(text_color),
            }.gap(10.0),
            // padding_each(left, right, top, bottom) — asymmetric: 12h, 8v.
            // (Layout::padding takes a single f32 for uniform padding.)
            Layout::default().padding_each(12.0, 12.0, 8.0, 8.0).min_width(200.0),
        );

        let decorated = DecoratedBox::with_style(content, Style::default()
            .background(bg)
            .corner_radius(6.0));

        GestureDetector::new(
            decorated
                .on_enter(move || on_enter.set(true))
                .on_exit(move || on_exit.set(false))
                .cursor(MouseCursor::System(SystemCursorKind::Pointer)),
        )
        .on_tap(move || on_tap())
        .boxed()
    }
}
```

Note: `.on_enter()`/`.on_exit()`/`.cursor()` are fluent methods on the
`Widget` trait (`vexo/src/widgets/mod.rs:237`) that wrap the child in a
`MouseRegion` internally. Since `MouseRegion` is `pub(crate)`, callers use
these fluent methods — exactly how `Button` does it (`button.rs:291`).

**`should_rebuild`:** not overridden. Default `true`. `MenuRow` is not in a
measured hot path.

### `close_after(ctrl, msg)` helper

```rust
fn close_after(ctrl: ContextMenuController, msg: &'static str) -> Rc<dyn Fn()> {
    Rc::new(move || {
        log::debug!("{}", msg);
        ctrl.close();
    })
}
```

### `placeholder_menu_builder()` — the assembled menu

```rust
fn placeholder_menu_builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| {
        let column = vexo::column! {
            reaction_row(ctrl.clone(), theme.clone()),
            menu_divider(theme.clone()),
            MenuRow { icon: Icons::Copy,  label: "Copy",  destructive: false,
                      on_tap: close_after(ctrl.clone(), "context menu: Copy"),
                      theme: theme.clone() },
            MenuRow { icon: Icons::Reply, label: "Reply", destructive: false,
                      on_tap: close_after(ctrl.clone(), "context menu: Reply"),
                      theme: theme.clone() },
            MenuRow { icon: Icons::Trash, label: "Delete", destructive: true,
                      on_tap: close_after(ctrl.clone(), "context menu: Delete"),
                      theme: theme.clone() },
        };
        vexo::DecoratedBox::with_style(
            vexo::WithLayout::new(column, vexo::Layout::default().min_width(200.0)),
            vexo::Style::default()
                .corner_radius(12.0)
                .background(theme.surface)
                .border(theme.outline, 1.0)
                .shadow(
                    vexo::BoxShadow::new(vexo::Color::BLACK.with_alpha(0.20))
                        .blur(12.0)
                        .offset(0.0, 4.0),
                ),
        )
        .boxed()
    })
}
```

Each `MenuRow` carries a `theme.clone()` snapshot (the builder runs at render
time, so this is the live theme). `theme.clone()` is cheap — `ThemeData` is a
small struct of `Color` fields.

### Imports added to `chat_screen.rs`

Existing imports at `chat_screen.rs:6-15` already cover: `column`, `row`,
`BoxShadow`, `Color`, `Component`, `ComponentState`, `DecoratedBox`, `Layout`,
`Rc`, `RenderContext`, `Signal`, `Style`, `Text`, `Theme`, `Widget`,
`WithLayout`, `ContextMenuController`, `MenuBuilder`.

**New imports needed:**
- `use vexo_fontawesome::{Icon, Icons};` (the crate is already a
  `shared_app` dependency — used in `app.rs:4` and `me/profile_screen.rs:15`).
- `use vexo::{JustifyContent, MouseCursor, SystemCursorKind};` — verify
  these are re-exported from `vexo/src/lib.rs:145`; if not, import from
  `vexo::layout::JustifyContent` and `vexo::input::MouseCursor` /
  `vexo::input::SystemCursorKind` (match whatever `Button` uses).

## Data flow (unchanged from prior spec, summarized)

```
right-click bubble
  → context_menu_trigger's on_secondary_press(global_pos)
  → controller.show(pos, builder)        [builder cell ← builder; Signal::set(Some(pos))]
  → host's signal_value dependency marked dirty
  → host rebuilds: barrier + Positioned(builder(&controller, &theme), pos) mount
  → builder runs INSIDE render → reads live theme → assembles reaction_row + divider + MenuRow×3
  → user hovers a row  →  MenuRowState.hovered.set(true)  →  MenuRow rebuilds with tinted bg
  → user clicks row    →  on_tap()  →  log + controller.close()
  OR user clicks reaction → on_tap() → log + controller.close()
  OR user clicks outside  → barrier.on_press → controller.close()
  → host rebuilds: barrier + menu unmount
```

The only difference from the existing spec: the builder now assembles a richer
widget tree. The builder still runs **inside `render`**, so it sees the live
theme and re-runs on theme toggle.

## Dismiss behavior (unchanged from prior spec)

- **Outside click (any button)** on the barrier → `controller.close()`.
- **Item left-click** → arena-mediated `on_tap` → user closure + `controller.close()`.
- **Reaction left-click** → same as item left-click.
- **Item/reaction right-click** → no arena (`Secondary` gated) → no-op. (v1 limitation.)
- **Right-click another bubble while open** → barrier catches it → closes. (v1 limitation: close-then-right-click-again.)
- **Escape key** → not handled in v1.
- **Scroll while open** → barrier catches the press → closes.

## Position assumption (unchanged)

Global click coordinates == Stack-local coordinates, because the host `Stack`
fills the window (the `ContextMenu` host is lifted to the app root — commit
`465938c`). The builder does not receive the position.

## Testing

### Philosophy

Minimal, targeted at the *new* behavior. The host/controller/barrier plumbing
is already covered by existing tests (`vexo_uikit/src/context_menu.rs:280-544`);
we don't re-test it. We add tests only for what changed.

### `shared_app/src/chats/chat_screen.rs` `#[cfg(test)]` — render-tree presence

The existing `test_right_click_bubble_opens_context_menu` already asserts
`"Copy"` appears after right-click. **That test stays as-is** — it's the
regression net proving the new builder still produces renderable content.

**New/extended test:** `test_right_click_menu_contains_reactions_and_items`
— right-click a bubble → `perform_rebuilds` → walk the render tree, assert
the three item labels (`"Copy"`, `"Reply"`, `"Delete"`) are present. (Reaction
icons are FA codepoints, not human-readable text — asserting their presence
requires matching the codepoint strings, which is fragile to FA version
changes. The item-label assertions are sufficient to prove the full assembled
menu renders; the reaction row's presence is visually verified.)

If extending the existing test would clutter it, add a separate test instead.

### Hover behavior — no new test

Rely on manual visual verification (see checklist below). The hover mechanism
mirrors `Button` exactly, which is already tested in
`vexo_uikit/tests/button_tests.rs`.

### Theme reactivity — covered by existing test

`test_builder_reads_current_theme` (`context_menu.rs:476`) already proves the
builder re-runs on theme toggle. Our new builder inherits this guarantee for
free — no new test needed.

### Verification gates (per CLAUDE.md)

```bash
cargo build -p shared_app          # after the rewrite
cargo test   -p shared_app         # ChatScreen regression + new presence test
cargo build -p desktop_demo        # confirm the demo still compiles
# Then ask the user to run cargo run -p desktop_demo and right-click a bubble
```

### Manual visual checklist (handed to the user)

1. Right-click a bubble → menu appears at cursor with rounded corners + shadow
2. Top row: 6 FA icons in a horizontal strip, centered, equal-spaced
3. Hairline divider below the reaction row
4. Three rows below: Copy (copy icon), Reply (reply icon), Delete (trash icon, **red**)
5. Hover a row → soft primary-tint background appears, cursor → pointer
6. Click any row or reaction → menu closes, log line appears in the terminal
7. Click outside → menu closes
8. Toggle theme → menu re-renders with new colors (if open during toggle)

## Open questions / future work

- **`MenuRow` graduation:** if `MenuRow` proves reusable beyond ChatScreen, it
  can move to `vexo_uikit` (Approach B) later without API churn — the local
  version is the prototype.
- **Hover on reactions:** if wanted, wrap each reaction in the same
  `MenuRow`-style stateful component (or a shared `HoverTile`).
- **Real emoji:** pipeline work (color-font support + emoji font bundling) is
  a separate, larger spec.
- **Keyboard shortcuts (⌘C etc.):** would need a key-event path on the menu;
  not built here.
- **`Icons::FaceLaugh` etc. name verification:** confirmed the source icon
  names exist (`face-laugh`, `face-surprise`, `face-sad-tear`, `face-angry`).
  If a generated PascalCase variant differs at compile time, the fix is a
  one-line rename during implementation.
