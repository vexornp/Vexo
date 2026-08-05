# Styled Context Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the bare ChatScreen placeholder context menu with a styled chat-app-style menu: top FontAwesome reaction icon row, hairline divider, three item rows with leading icons + hover highlight.

**Architecture:** All new helpers (`MenuRow` component, `menu_divider`, `reaction_row`, `close_after`) live as private items inline in `shared_app/src/chats/chat_screen.rs` (Approach A). The existing `ContextMenu` host / `ContextMenuController` / `MenuBuilder` trio is unchanged — only the builder's content (the `placeholder_menu_builder()` fn body) changes. `MenuRow` mirrors `Button`'s hover pattern: `Signal<bool>` state toggled via the fluent `Widget::on_enter`/`on_exit` API, re-rendered with a tinted background.

**Tech Stack:** Rust, vexo framework (three-tree architecture: Widget → Element → RenderObject), `vexo_fontawesome` for icons, glyphon/cosmic-text for fonts.

## Global Constraints

- **No real emoji.** The text render path is monochrome-only (`vexo/src/render_objects/text.rs:33`), and `new_font_system()` (`vexo/src/resource.rs:18`) loads only Roboto. FontAwesome icons stand in.
- **No new `vexo_uikit` module or theme tokens.** All helpers are private to `chat_screen.rs`. Colors are derived inline from `ThemeData`.
- **No hover on reaction icons.** Reactions stay a flat stateless strip (cursor still flips to pointer via `.cursor(...)`).
- **No `should_rebuild()` override on `MenuRow`.** The menu is a short-lived overlay, not a hot path. Default `true` is correct.
- **Signature of `placeholder_menu_builder()` is unchanged** (`fn() -> MenuBuilder`) — no caller changes.
- **FontAwesome font is already registered** in `shared_app/src/app.rs:32` (`vexo_fontawesome::register_fonts(font_system)`). No font registration work needed.
- **`MouseRegion` is `pub(crate)`** — callers use the fluent `Widget::on_enter(..)`/`on_exit(..)`/`cursor(..)` methods (`vexo/src/widgets/mod.rs:237`) which wrap the child in a `MouseRegion` internally. This is exactly how `Button` does it (`vexo_uikit/src/button.rs:291`).
- **API shapes verified:**
  - `DecoratedBox::with_style(child, style)` (not `new` — `new` takes only a child) (`vexo/src/widgets/decorated_box.rs:303`)
  - `Layout::padding(value: f32)` for uniform; `Layout::padding_each(left, right, top, bottom)` for asymmetric (`vexo/src/layout/style.rs:365,371`)
  - `Layout::height`, `width_percent`, `min_width` exist (`vexo/src/layout/style.rs:405,411,423`)
  - `GestureDetector::on_tap(callback: impl FnMut() + 'static)` (`vexo/src/widgets/gesture_detector.rs:111`)
  - Fluent `Widget::on_enter`/`on_exit`/`cursor` take `self` (Sized) and return `Box<dyn Widget>` (`vexo/src/widgets/mod.rs:237`)
  - `JustifyContent`, `MouseCursor`, `SystemCursorKind` are re-exported from `vexo` (`vexo/src/lib.rs:145,257`)
  - `row!`/`column!` produce `MultiChild`; `.gap(f32)` and `.justify(JustifyContent)` are fluent methods on `MultiChild` (`vexo/src/widgets/multi_child.rs:134`)

---

## File Structure

Only **one file** is modified: `shared_app/src/chats/chat_screen.rs` (currently 1016 lines).

| Section | Lines (approx) | Change |
|---|---|---|
| Imports (top) | 6-15 | Add `Icon, Icons`, `JustifyContent`, `MouseCursor`, `SystemCursorKind` |
| `placeholder_menu_builder()` | 260-299 | Rewrite body to assemble the new menu |
| New private items | (new, after `placeholder_menu_builder`) | `MenuRowState`, `MenuRow`, `menu_divider`, `reaction_row`, `close_after` |
| Tests `#[cfg(test)]` | 301-1016 | Add `test_right_click_menu_contains_reactions_and_items` |

No other files are touched. The `ContextMenu` host/controller/trigger trio (`vexo_uikit/src/context_menu.rs`) is unchanged.

---

## Task 1: Add the `MenuRow` component (stateful, hover-highlighted)

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs` (add imports + `MenuRowState` + `MenuRow` + `close_after`, after the existing `placeholder_menu_builder` fn at line 299)

**Interfaces:**
- Consumes: `vexo::{Component, ComponentState, Signal, Color, Layout, Style, DecoratedBox, row, WithLayout, Text, Widget, JustifyContent (unused here), MouseCursor, SystemCursorKind, RenderContext}`, `vexo_fontawesome::{Icon, Icons}`, `vexo_uikit::ContextMenuController`, `std::rc::Rc`
- Produces: `struct MenuRow` (a `Component` with `type State = MenuRowState`), constructible as `MenuRow { icon, label, destructive, on_tap, theme }` and usable as a child in `column!`. Also produces `fn close_after(ctrl, msg) -> Rc<dyn Fn()>`.

- [ ] **Step 1: Add the new imports**

In `shared_app/src/chats/chat_screen.rs`, the existing import block is:

```rust
use vexo::{
    column, row, AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox,
    FlexDirection, ImageData, Key, Layout, LifecycleContext, RenderContext, ScrollController,
    ScrollView, Signal, Spacer, Style, Text, TextEdit, TextEditingController, Theme, Widget,
    WidgetKey, WithLayout,
};
use vexo_uikit::{
    context_menu_trigger, Button, ButtonVariant, ContextMenu, ContextMenuController,
    KeyboardAvoider, MenuBuilder,
};
```

Add `JustifyContent`, `MouseCursor`, `SystemCursorKind` to the `vexo` import (after `Image` or alphabetically — keep the existing alphabetical-ish order), and add a new `use vexo_fontawesome::{Icon, Icons};` line after the `vexo_uikit` import. The result:

```rust
use vexo::{
    column, row, AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox,
    FlexDirection, ImageData, JustifyContent, Key, Layout, LifecycleContext, MouseCursor,
    RenderContext, ScrollController, ScrollView, Signal, Spacer, Style, SystemCursorKind, Text,
    TextEdit, TextEditingController, Theme, Widget, WidgetKey, WithLayout,
};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::{
    context_menu_trigger, Button, ButtonVariant, ContextMenu, ContextMenuController,
    KeyboardAvoider, MenuBuilder,
};
```

- [ ] **Step 2: Add the `close_after` helper and `MenuRow`/`MenuRowState` types**

Insert this block **after** the existing `placeholder_menu_builder()` fn (which ends at line 299) and **before** the `#[cfg(test)]` mod (which starts at line 301). Do NOT modify `placeholder_menu_builder()` yet — that's Task 4.

```rust
// ============================================================================
// Context menu helpers (private) — assembled by `placeholder_menu_builder`.
// ============================================================================

/// Build the `on_tap` closure for a menu item: log `msg` and close the menu.
/// Sugar so the `column!` in `placeholder_menu_builder` stays readable.
fn close_after(ctrl: ContextMenuController, msg: &'static str) -> Rc<dyn Fn()> {
    Rc::new(move || {
        log::debug!("{}", msg);
        ctrl.close();
    })
}

/// State for `MenuRow` — tracks hover via a reactive `Signal<bool>`.
/// Auto-wired by `#[derive(ComponentState)]` (mirrors `ButtonState` in
/// `vexo_uikit/src/button.rs:37`).
#[derive(ComponentState, Default)]
struct MenuRowState {
    hovered: Signal<bool>,
}

/// One context-menu item row: leading FontAwesome icon + label, with a
/// hover-tint background and a tap handler that logs + closes the menu.
///
/// `destructive: true` renders icon + text in `theme.error` (used for Delete).
/// `theme` is a snapshot taken in the builder at render time (the builder runs
/// inside `ContextMenu::render`, so this is the live theme).
#[derive(Clone)]
struct MenuRow {
    icon: Icons,
    label: &'static str,
    destructive: bool,
    on_tap: Rc<dyn Fn()>,
    theme: vexo::ThemeData,
}

impl Component for MenuRow {
    type State = MenuRowState;

    fn render(&self, state: &mut MenuRowState, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let hovered = state.hovered.get();
        // ~8% primary wash over surface — slightly stronger than nav's
        // ROW_HOVER_TINT (0.95) so it reads inside the compact menu.
        let row_hover_bg = Color::lerp(self.theme.primary, self.theme.surface, 0.92);
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
            }
            .gap(10.0),
            // padding_each(left, right, top, bottom) — 12h, 8v.
            Layout::default()
                .padding_each(12.0, 12.0, 8.0, 8.0)
                .min_width(200.0),
        );

        let decorated = DecoratedBox::with_style(
            content,
            Style::default().background(bg).corner_radius(6.0),
        );

        // Fluent .on_enter/.on_exit/.cursor wrap `decorated` in MouseRegion(s)
        // (pub(crate) — callers use the fluent Widget trait methods, exactly
        // like Button does at vexo_uikit/src/button.rs:291). Each returns
        // Box<dyn Widget>, so chain them, then wrap the result in
        // GestureDetector for the tap.
        let hovered_area = decorated
            .on_enter(move || on_enter.set(true))
            .on_exit(move || on_exit.set(false))
            .cursor(MouseCursor::System(SystemCursorKind::Pointer));

        GestureDetector::new(hovered_area)
            .on_tap(move || on_tap())
            .boxed()
    }
}
```

- [ ] **Step 3: Verify it compiles (the new types are unused yet — expect a dead-code warning, not an error)**

Run: `cargo build -p shared_app 2>&1 | tail -20`
Expected: build **succeeds**. You may see `warning: function close_after is never used` / `struct MenuRow is never constructed` / `struct MenuRowState is never constructed` — these are expected dead-code warnings and will disappear after Task 4 wires them in. If you see **errors** (red), fix them before proceeding — likely causes: typo in an import, wrong method name, or `ComponentState` derive not in scope (it is — `vexo::ComponentState` is imported at line 7).

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "feat(shared_app): add MenuRow component + close_after helper for styled context menu

Stateful hover-highlight row mirroring Button's Signal<bool> pattern.
Not yet wired into the menu builder — that's the next task."
```

---

## Task 2: Add the `menu_divider` helper

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs` (add `menu_divider` fn, right after `close_after`)

**Interfaces:**
- Consumes: `vexo::{Color, DecoratedBox, Layout, Style, Text, Widget, WithLayout, ThemeData}`
- Produces: `fn menu_divider(theme: vexo::ThemeData) -> Box<dyn Widget>` — a 1px hairline separator, full-width.

- [ ] **Step 1: Add the `menu_divider` fn**

Insert this **immediately after** the `close_after` fn (before `MenuRowState`):

```rust
/// A 1px hairline separator between menu sections. Full-width (100%).
///
/// Uses the same pre-composited outline formula as `NavColors::divider`
/// (`vexo_uikit/src/theme/tokens.rs:122`): `Color::lerp(outline, surface,
/// 1.0 - 0.35)` — outline at ~0.35 alpha, pre-composited over surface so it
/// renders identically regardless of backdrop. Taffy floors sub-pixel heights
/// to 0, so 1.0 is the smallest height that survives layout (see
/// `HAIRLINE_THICKNESS` at tokens.rs:192).
fn menu_divider(theme: vexo::ThemeData) -> Box<dyn Widget> {
    let color = Color::lerp(theme.outline, theme.surface, 1.0 - 0.35);
    WithLayout::new(
        DecoratedBox::with_style(Text::new(""), Style::default().background(color)),
        Layout::default().height(1.0).width_percent(1.0),
    )
    .boxed()
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p shared_app 2>&1 | tail -10`
Expected: build succeeds. Expect a `function menu_divider is never used` warning — will disappear after Task 4.

- [ ] **Step 3: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "feat(shared_app): add menu_divider helper for styled context menu

1px hairline separator matching NavColors::divider's pre-composited outline
formula. Not yet wired in."
```

---

## Task 3: Add the `reaction_row` helper

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs` (add `reaction_row` fn, right after `menu_divider`)

**Interfaces:**
- Consumes: `vexo::{Color, JustifyContent, Layout, MouseCursor, SystemCursorKind, Widget, WithLayout, row, ThemeData}`, `vexo_fontawesome::{Icon, Icons}`, `vexo::GestureDetector` (via `.on_tap` builder method), `vexo_uikit::ContextMenuController`
- Produces: `fn reaction_row(ctrl: ContextMenuController, theme: vexo::ThemeData) -> Box<dyn Widget>` — a centered `row!` of 6 FA reaction icons, each tappable (log + close).

- [ ] **Step 1: Add the `reaction_row` fn**

Insert this **immediately after** the `menu_divider` fn (before `MenuRowState`):

```rust
/// The top reaction strip: a centered row of 6 FontAwesome reaction icons
/// (standing in for emoji — the text pipeline is monochrome-only and no emoji
/// font is loaded). Each icon is tappable: logs a message and closes the menu.
///
/// Stateless (no hover background) — the cursor still flips to pointer via
/// `.cursor(...)`. Matches the image's compact, low-affordance reaction strip.
fn reaction_row(ctrl: ContextMenuController, theme: vexo::ThemeData) -> Box<dyn Widget> {
    // (icon, log message) pairs. The log messages use emoji codepoints in the
    // string literal for grep-ability — they're just log text, never rendered.
    let reactions: [(Icons, &str); 6] = [
        (Icons::ThumbsUp, "context menu: thumbsup"),
        (Icons::Heart, "context menu: heart"),
        (Icons::FaceLaugh, "context menu: laugh"),
        (Icons::FaceSurprise, "context menu: surprise"),
        (Icons::FaceSadTear, "context menu: sad"),
        (Icons::FaceAngry, "context menu: angry"),
    ];

    let row = row! {
        for (icon, msg) in reactions {
            let ctrl = ctrl.clone();
            GestureDetector::new(
                WithLayout::new(
                    Icon::new(icon)
                        .with_size(16.0)
                        .with_color(theme.on_surface_variant),
                    Layout::default().padding(6.0),
                )
                .boxed()
                .cursor(MouseCursor::System(SystemCursorKind::Pointer)),
            )
            .on_tap(move || {
                log::debug!("{}", msg);
                ctrl.close();
            })
        }
    }
    .gap(8.0)
    .justify(JustifyContent::Center);

    WithLayout::new(
        row,
        Layout::default().padding_each(8.0, 8.0, 4.0, 4.0), // 8h, 4v
    )
    .boxed()
}
```

**Note on icon variant names:** the source FA icon names are `thumbs-up`, `heart`, `face-laugh`, `face-surprise`, `face-sad-tear`, `face-angry` (confirmed in `vexo_fontawesome/assets/icons.json`). The generated PascalCase enum variants **should** be `ThumbsUp`, `Heart`, `FaceLaugh`, `FaceSurprise`, `FaceSadTear`, `FaceAngry`. If any variant name differs at compile time (e.g. `FaceLaughBeam` vs `FaceLaugh`), the compiler error will name the correct variant — fix the name in the `reactions` array and move on. This is a one-line fix, not a design change.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p shared_app 2>&1 | tail -15`
Expected: build succeeds. If a `Icons::FaceXxx` variant doesn't exist, the error will say "no variant named `FaceLaugh`" (or similar) — run `cargo doc -p vexo_fontawesome --no-deps --open` or grep `vexo_fontawesome/src/generated/` (it's generated at build time into `OUT_DIR`, so instead grep `assets/icons.json` for the source name, then PascalCase it). Fix and rebuild. Expect a `function reaction_row is never used` warning otherwise.

- [ ] **Step 3: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "feat(shared_app): add reaction_row helper for styled context menu

Centered row of 6 FontAwesome reaction icons (thumbs-up, heart, face-laugh,
face-surprise, face-sad-tear, face-angry). Stateful hover deferred — flat
strip with pointer cursor only."
```

---

## Task 4: Rewrite `placeholder_menu_builder()` to assemble the new menu

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs:260-299` (replace the fn body)

**Interfaces:**
- Consumes: `MenuRow`, `menu_divider`, `reaction_row`, `close_after` (all from Tasks 1-3), `vexo::{Color, DecoratedBox, Layout, Style, BoxShadow, Widget, WithLayout, column}`, `vexo_fontawesome::Icons`, `vexo_uikit::{ContextMenuController, MenuBuilder}`
- Produces: an unchanged-signature `fn placeholder_menu_builder() -> MenuBuilder` that now assembles the styled menu.

- [ ] **Step 1: Replace the `placeholder_menu_builder` fn body**

The current fn (lines 260-299) is:

```rust
fn placeholder_menu_builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| {
        let labels: [(&str, &str); 3] = [
            ("Copy", "context menu: Copy"),
            ("Reply", "context menu: Reply"),
            ("Delete", "context menu: Delete"),
        ];
        let column = vexo::column! {
            for item in labels {
                let ctrl = ctrl.clone();
                vexo::GestureDetector::new(
                    vexo::WithLayout::new(
                        vexo::Text::new(item.0).with_color(theme.on_surface),
                        vexo::Layout::default().padding(8.0).width(160.0),
                    ),
                )
                .on_tap(move || {
                    log::debug!("{}", item.1);
                    ctrl.close();
                })
            }
        };
        vexo::DecoratedBox::with_style(
            vexo::WithLayout::new(column, vexo::Layout::default().min_width(160.0)),
            vexo::Style::default()
                .corner_radius(8.0)
                .background(theme.surface)
                .border(theme.outline, 1.0)
                .shadow(
                    vexo::BoxShadow::new(vexo::Color::BLACK.with_alpha(0.25))
                        .blur(6.0)
                        .offset(0.0, 2.0),
                ),
        )
        .boxed()
    })
}
```

Replace the **entire fn** (lines 260-299) with:

```rust
fn placeholder_menu_builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| {
        // Assemble: reaction row, hairline divider, three item rows.
        // Each MenuRow carries a `theme.clone()` snapshot — the builder runs
        // inside ContextMenu::render, so this is the live theme (and re-runs
        // on theme toggle, per test_builder_reads_current_theme).
        let column = column! {
            reaction_row(ctrl.clone(), theme.clone()),
            menu_divider(theme.clone()),
            MenuRow {
                icon: Icons::Copy,
                label: "Copy",
                destructive: false,
                on_tap: close_after(ctrl.clone(), "context menu: Copy"),
                theme: theme.clone(),
            },
            MenuRow {
                icon: Icons::Reply,
                label: "Reply",
                destructive: false,
                on_tap: close_after(ctrl.clone(), "context menu: Reply"),
                theme: theme.clone(),
            },
            MenuRow {
                icon: Icons::Trash,
                label: "Delete",
                destructive: true,
                on_tap: close_after(ctrl.clone(), "context menu: Delete"),
                theme: theme.clone(),
            },
        };

        DecoratedBox::with_style(
            WithLayout::new(column, Layout::default().min_width(200.0)),
            Style::default()
                .corner_radius(12.0)
                .background(theme.surface)
                .border(theme.outline, 1.0)
                .shadow(
                    BoxShadow::new(Color::BLACK.with_alpha(0.20))
                        .blur(12.0)
                        .offset(0.0, 4.0),
                ),
        )
        .boxed()
    })
}
```

Note: the `vexo::` prefixes on `column!`, `DecoratedBox`, `WithLayout`, `Layout`, `Style`, `BoxShadow`, `Color` are **dropped** — these are all imported at the top of the file (Task 1's import block). Keep `column!` lowercase (it's a macro imported via `use vexo::{column, row}`).

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p shared_app 2>&1 | tail -15`
Expected: build **succeeds** with no dead-code warnings (all helpers are now used). If you see errors:
- `cannot find variant Copy/Reply/Trash in Icons` → check the generated enum; `Copy`, `Reply`, `Trash` are confirmed (used in `vexo_fontawesome/src/lib.rs:102` for `Trash`/`ThumbsUp`).
- `expected Box<dyn Widget>, found MenuRow` → `MenuRow` implements `Component` which implements `Widget`, so it should coerce via `.boxed()` if needed. If the `column!` macro won't take a bare `MenuRow { ... }`, wrap each as `MenuRow { ... }.boxed()` — but try without `.boxed()` first (the macro accepts any `Widget`).

- [ ] **Step 3: Run the existing ChatScreen tests (regression net)**

Run: `cargo test -p shared_app 2>&1 | tail -25`
Expected: **all tests pass**, including `test_right_click_bubble_opens_context_menu` (which asserts `"Copy"` appears after right-click — still true with the new builder). If this test fails, the new builder's output shape diverged from the old one — investigate before "fixing" the test (per spec: the test is the regression guard).

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "feat(shared_app): assemble styled context menu in placeholder_menu_builder

Replaces the bare 3-text-row menu with: reaction icon row + hairline divider
+ three icon+label rows (Copy/Reply/Delete, Delete in error red). 12px corner
radius, softer shadow. Existing right-click test still passes (regression net)."
```

---

## Task 5: Add the render-tree presence test for the full menu

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs` (add a new test inside the `#[cfg(test)] mod tests` block, near the existing `test_right_click_bubble_opens_context_menu` at line 877)

**Interfaces:**
- Consumes: the existing test helpers `seed_messages_signal()`, `seed_avatar()`, `seed_me_avatar()`, `find_text_in_tree()` (all defined in the `tests` mod at lines 312-352), `vexo::input::{InputEvent, PointerButton, ButtonState, Modifiers}`, `vexo::core::{Point, Size, ScaleSource}`, `vexo::platform::{Clipboard, stub_clipboard::StubClipboard}`, `vexo::resource::new_font_system`, `vexo::layout::TaffyLayoutEngine`, `vexo::animation::AnimationTicker`, `vexo::ThreeTreePipeline`, `vexo::{RenderObjectKey, RenderObjectRegistry, TextRenderObject}` (all already imported in the test mod).
- Produces: a new `#[test] fn test_right_click_menu_contains_reactions_and_items`.

- [ ] **Step 1: Add the new test**

Insert this test **immediately after** `test_right_click_bubble_opens_context_menu` (which ends at line 947, before `test_left_click_bubble_does_not_open_context_menu` at line 949). It reuses the same fixture pattern but asserts all three item labels:

```rust
    #[test]
    fn test_right_click_menu_contains_reactions_and_items() {
        // Regression + presence net for the styled menu: after right-click,
        // the render tree must contain all three item labels (Copy/Reply/Delete).
        // Reaction icons are FA codepoints (not human-readable), so we don't
        // assert them here — their presence is visually verified.
        let messages_signal = seed_messages_signal();
        let controller = ContextMenuController::new();
        let view = ContextMenu::new(
            ChatScreen {
                conv_id: ConvId(1),
                messages: Signal::derive(messages_signal, |map| {
                    map.get(&ConvId(1)).cloned().unwrap_or_default()
                }),
                avatar_bytes: seed_avatar(ConvId(1)),
                me_avatar_bytes: seed_me_avatar(),
                on_send: Rc::new(|_| ()),
                scroll_controller: ScrollController::new(),
                context_menu: controller.clone(),
            },
            controller.clone(),
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Right-click at a position inside the first message bubble.
        // (Same coordinates as test_right_click_bubble_opens_context_menu:
        // first bubble starts at approx (52, 12) — avatar 32 + gap 8 + 12 pad.)
        let secondary_press = vexo::input::InputEvent::PointerButton {
            position: vexo::core::Point::new(60.0, 20.0),
            button: vexo::input::PointerButton::Secondary,
            state: vexo::input::ButtonState::Pressed,
        };
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        pipeline.handle_event(
            vexo::core::Point::new(60.0, 20.0),
            &secondary_press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // All three item labels must appear in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        for label in ["Copy", "Reply", "Delete"] {
            assert!(
                find_text_in_tree(ro_reg, root, label),
                "menu item '{}' should appear in render tree after right-clicking a bubble",
                label,
            );
        }
    }
```

- [ ] **Step 2: Run the new test — verify it passes**

Run: `cargo test -p shared_app test_right_click_menu_contains_reactions_and_items 2>&1 | tail -15`
Expected: PASS. If it fails with "menu item 'Reply' should appear...", the builder isn't rendering all three rows — check that `column!` accepted all three `MenuRow` children (Task 4 Step 2's note about `.boxed()`).

- [ ] **Step 3: Run the full shared_app test suite (regression)**

Run: `cargo test -p shared_app 2>&1 | tail -15`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "test(shared_app): assert all 3 context-menu items render after right-click

Extends the right-click regression net to verify Copy/Reply/Delete all appear
in the render tree with the new styled builder."
```

---

## Task 6: Final build + desktop demo compile check + hand off to manual visual verification

**Files:**
- No file changes. This task is verification only.

- [ ] **Step 1: Clean build of all affected crates**

Run: `cargo build -p shared_app -p desktop_demo 2>&1 | tail -15`
Expected: both crates build with no errors and no dead-code warnings.

- [ ] **Step 2: Full test run**

Run: `cargo test -p shared_app -p vexo_uikit 2>&1 | tail -20`
Expected: all tests pass. The `vexo_uikit` tests are unchanged but confirm we didn't break the shared `context_menu.rs` trio (we didn't touch it, but the `shared_app` tests exercise it via `ContextMenu::new`).

- [ ] **Step 3: Hand off to the user for manual visual verification**

Per CLAUDE.md: **never run `cargo run -p desktop_demo` yourself** — you can't interact with the GUI and your terminal may be on a non-Retina display producing misleading results. Ask the user to run it.

Print this message to the user verbatim (do NOT run the command):

```
All code changes are complete and tests pass. Please run the desktop demo to
visually verify:

    cargo run -p desktop_demo

Then right-click a message bubble and check:

1. Menu appears at the cursor with rounded corners + shadow
2. Top row: 6 FA icons (thumbs-up, heart, laugh, surprise, sad, angry) in a
   horizontal strip, centered
3. Hairline divider below the reaction row
4. Three rows below: Copy (copy icon), Reply (reply icon), Delete (trash icon,
   RED text/icon)
5. Hover a row → soft primary-tint background appears, cursor → pointer
6. Click any row or reaction → menu closes, log line appears in the terminal
7. Click outside → menu closes
8. Toggle theme (if the demo has a toggle) → menu re-renders with new colors
   if open during the toggle
```

Wait for the user's visual verification report. If something looks wrong, debug from there (the spec's manual checklist maps 1:1 to these points). Do not proceed to "done" until the user confirms the visual checks pass.

---

## Self-Review

**1. Spec coverage:**
- Leading icon per item → Task 1 (`MenuRow` renders `Icon::new(self.icon)`) + Task 4 (Copy/Reply/Trash icons wired). ✓
- Separator dividers → Task 2 (`menu_divider`) + Task 4 (assembled between reactions and items). ✓
- Hover/selection highlight → Task 1 (`MenuRowState::hovered: Signal<bool>`, `.on_enter`/`.on_exit`, tinted `bg`). ✓
- Reaction row at top (FA icons, not emoji) → Task 3 (`reaction_row`) + Task 4 (assembled as first child). ✓
- 12px corner radius, shadow BLACK@0.20 blur 12 offset (0,4) → Task 4. ✓
- min_width 200px → Task 4 (outer `WithLayout`) + Task 1 (`MenuRow` inner `WithLayout`). ✓
- Delete destructive (error color) → Task 1 (`destructive` field) + Task 4 (`destructive: true` on Delete). ✓
- Theme reactivity → unchanged (builder runs at render time; covered by existing `test_builder_reads_current_theme`). ✓
- No new `vexo_uikit` module / tokens → all helpers in `chat_screen.rs`. ✓
- No `should_rebuild` override → not mentioned in any task. ✓
- Test: `test_right_click_menu_contains_reactions_and_items` → Task 5. ✓
- Verification gates → Task 6. ✓

**2. Placeholder scan:** No "TBD", "TODO", "implement later", "add error handling", or "similar to Task N". All code blocks contain real code. The one genuine uncertainty (FA variant names) is called out with a concrete fix path in Task 3 Step 2.

**3. Type consistency:**
- `MenuRow` fields: `icon: Icons`, `label: &'static str`, `destructive: bool`, `on_tap: Rc<dyn Fn()>`, `theme: vexo::ThemeData` — consistent between Task 1 (definition) and Task 4 (construction).
- `close_after(ctrl: ContextMenuController, msg: &'static str) -> Rc<dyn Fn()>` — consistent between Task 1 (definition) and Task 4 (call sites).
- `menu_divider(theme: vexo::ThemeData) -> Box<dyn Widget>` — consistent between Task 2 and Task 4.
- `reaction_row(ctrl: ContextMenuController, theme: vexo::ThemeData) -> Box<dyn Widget>` — consistent between Task 3 and Task 4.
- `MenuRowState::hovered: Signal<bool>` — toggled in `on_enter`/`on_exit` (Task 1), read in `render` (Task 1). ✓
- Imports added in Task 1 (`JustifyContent`, `MouseCursor`, `SystemCursorKind`, `Icon`, `Icons`) are all used by Task 1 (`MouseCursor`, `SystemCursorKind`) or Task 3 (`JustifyContent`, `Icon`, `Icons`). ✓

No issues found. Plan is complete.
