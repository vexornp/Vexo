# Chat Input Bar Dark Mode Support Design

Date: 2026-08-02
Status: Draft

## Problem

The chat input bar in `shared_app/src/chats/chat_screen.rs` does not adapt
to dark mode. When the user toggles the appearance picker
(`shared_app/src/me/profile_screen.rs:358-401`) to Dark, the Send button
restyles correctly (it reads `Theme::of(ctx)` via
`vexo_uikit/src/button.rs:225`), but the `TextEdit` next to it stays
white-on-black: a `Color::WHITE` box with `Color::BLACK` glyphs.

This happens because `TextEdit`'s colors are hardcoded in two places:

- `vexo/src/widgets/text_edit.rs:584-602` — `render()` builds a
  `DecoratedBox` with `Color::WHITE` background, hardcoded
  `Color::rgb(0.6, 0.6, 0.6)` (unfocused) / `Color::rgb(0.2, 0.4, 0.8)`
  (focused) borders.
- `vexo/src/render_objects/text_edit.rs:321` — `paint()` emits
  `RenderCommand::Text { color: Color::BLACK, ... }` with a literal.

`TextEditContent` (`vexo/src/widgets/text_edit_content.rs`) has no color
field at all, so the glyph color cannot flow from the widget tree to the
render object.

Meanwhile the `Text` widget works in dark mode — but only because
**callers pass theme colors** via `.with_color(...)` (see
`chat_screen.rs:183-187`). `TextEdit` exposes no equivalent setters.

## Goal

Make the chat input bar's `TextEdit` adapt to dark mode by giving
`build_input_bar` a way to pass theme-derived colors through, matching the
existing `Text` widget pattern.

## Non-Goals

- Making `TextEdit` read `Theme::of(ctx)` internally. The user explicitly
  chose the call-site pattern (matching `Text`) over the internal-read
  pattern (matching `Button`). `TextEdit` stays theme-agnostic.
- Adding placeholder text or placeholder color. No placeholder concept
  exists today; out of scope.
- Changing the cursor color. `CURSOR_COLOR` at
  `vexo/src/render_objects/text_edit.rs:18` stays hardcoded — it is a
  reasonable accent in both modes.
- Migrating other `TextEdit` call sites. None exist in `shared_app`
  beyond the chat input bar; the change is backward-compatible anyway.
- Focus color shift on the border. Per Approach B (chosen below), focus
  changes only border width (1 → 2), not color.

## Approach

Three API shapes were considered. All three plumb a `color` field through
`TextEditContent` → `TextEditRenderObject` (this plumbing is common and
unavoidable — the glyph color lives in the render object). They differ in
how the focus state is exposed.

1. **Full control (4 builders)** — `.with_background()`,
   `.with_text_color()`, `.with_border_color()`,
   `.with_focused_border_color()`. Preserves today's gray→blue focus
   color transition, theme-driven. Most knobs.
2. **Simplified focus (3 builders) — *chosen*** — `.with_background()`,
   `.with_text_color()`, `.with_border_color()`. Focus keeps the border
   color the caller passed; only width changes (1 → 2). Matches the
   `Text` widget's fluent-builder idiom; `TextEdit` stays theme-agnostic.
3. **Struct bundle (1 builder)** — `.with_colors(InputFieldColors {
   background, text, border })`. One call-site line, but introduces a new
   public type and breaks the fluent-builder idiom used by `Text`.

**Why Approach B:** it matches `Text`'s `.with_color()` / `.with_font_size()`
fluent style, keeps `TextEdit` theme-agnostic (honoring the call-site
decision), and the width-only focus affordance is already a clear signal
today. The lost color-shift on focus is minor; if it's ever wanted back,
adding `.with_focused_border_color()` as an opt-in is backward-compatible.

## Architecture

Four files change, all small. No new modules, no new public types, no new
crates.

### 1. `vexo/src/widgets/text_edit_content.rs` — add `color` field

`TextEditContent` gains a `color: Color` field, defaulting to
`Color::BLACK` to preserve current behavior. Plumbed through `new()` and
any `update()` path. Exposes an accessor (e.g. `pub fn color(&self) -> Color`)
for the render object to read.

### 2. `vexo/src/widgets/text_edit.rs` — add 3 builders, replace hardcoded colors

`TextEdit` gains three `Option<Color>` fields: `background`, `text_color`,
`border_color`, each defaulting to `None`. Three builders:

```rust
pub fn with_background(mut self, color: Color) -> Self {
    self.background = Some(color);
    self
}
pub fn with_text_color(mut self, color: Color) -> Self {
    self.text_color = Some(color);
    self
}
pub fn with_border_color(mut self, color: Color) -> Self {
    self.border_color = Some(color);
    self
}
```

In `render()` (`text_edit.rs:584-602`), replace hardcoded values:

| Today | After |
|---|---|
| `Color::WHITE` background | `self.background.unwrap_or(Color::WHITE)` |
| `Color::rgb(0.6, 0.6, 0.6)` unfocused border | `self.border_color.unwrap_or(Color::rgb(0.6, 0.6, 0.6))` |
| `Color::rgb(0.2, 0.4, 0.8)` focused border | **removed** — border color is now constant; focus changes only width |
| `border_width = if focused { 2.0 } else { 1.0 }` | unchanged |
| (no text color passed to `TextEditContent`) | `TextEditContent::new(...).with_color(self.text_color.unwrap_or(Color::BLACK))` — see below |

`TextEditContent` gains a `with_color(Color)` builder mirroring `Text`'s
`.with_color()` idiom (`vexo/src/widgets/text.rs:50-53`). This is the
single explicit way the color flows from `TextEdit::render()` into the
content leaf.

The `is_focused` branch collapses from "two colors + two widths" to
"one color + two widths".

### 3. `vexo/src/render_objects/text_edit.rs:321` — read color from content

Replace the literal `color: Color::BLACK` in the `RenderCommand::Text`
emission with the `color` field read from the `TextEditContent` the
render object was built from.

### 4. `shared_app/src/chats/chat_screen.rs:223-241` — pass theme colors

`build_input_bar` takes `&vexo::ThemeData` and threads it through:

```rust
fn build_input_bar(
    controller: TextEditingController,
    on_send: impl FnMut() + 'static,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
    row! {
        WithLayout::new(
            TextEdit::new(controller)
                .with_background(theme.surface)
                .with_text_color(theme.on_surface)
                .with_border_color(theme.outline),
            Layout::default().flex_grow(1.0),
        ),
        Button::new("Send")
            .variant(ButtonVariant::Primary)
            .shadow(/* unchanged */)
            .on_tap(on_send),
    }
    .gap(8.0)
    .padding(8.0)
    .boxed()
}
```

The single call site at `chat_screen.rs:146` becomes
`build_input_bar(tc, on_send_closure, &theme)`.

## Data Flow

Per render, with dark theme active:

```
ChatScreen::render (reads Theme::of(ctx) at :111)
  → build_input_bar(tc, on_send, &theme)
    → TextEdit { background: Some(surface), text_color: Some(on_surface), border_color: Some(outline) }
      → TextEdit::render()
        → DecoratedBox { background: surface, border: (outline, 1.0 or 2.0) }   // wraps
        → TextEditContent { color: on_surface, ... }                             // leaf
          → TextEditRenderObject::paint()
            → RenderCommand::Text { color: on_surface, ... }                     // was :321
```

## Default Behavior Preserved

Every new field defaults to today's hardcoded value (`None` → unwrap-or):

| Field | `None` fallback | Today's hardcoded value |
|---|---|---|
| `background` | `Color::WHITE` | `Color::WHITE` |
| `text_color` | `Color::BLACK` | `Color::BLACK` |
| `border_color` | `Color::rgb(0.6, 0.6, 0.6)` | `Color::rgb(0.6, 0.6, 0.6)` |

A bare `TextEdit::new(controller)` with no `.with_*` calls renders exactly
as before — backward compatible.

## Theme Invalidation

No new dependency. `build_input_bar` runs inside `ChatScreen::render`,
which already calls `Theme::of(ctx)` at `chat_screen.rs:111`. When
`is_dark` flips, the root `Theme` widget invalidates descendants →
`ChatScreen` rebuilds → `build_input_bar` reads the new `theme` → fresh
colors flow through. This is the same path that already repaints message
bubbles (`chat_screen.rs:197`) and the screen background
(`chat_screen.rs:166`).

## Edge Cases

- **No `Theme` ancestor** — `Theme::of(ctx)` falls back to
  `ThemeData::light()` (`theme.rs:128-131`). `build_input_bar` is
  unaffected because it receives `&ThemeData` as an argument, not from
  the tree. Standalone `TextEdit::new(...)` without `.with_*` renders the
  old white/black — backward compatible.
- **`should_rebuild` returns `false` during keyboard frames**
  (`chat_screen.rs:106`) — irrelevant to color. Colors ride on the
  state-driven rebuild from `is_dark` (a `Signal` flip), which bypasses
  `should_rebuild` per the three-level ladder. Same mechanism that
  already repaints bubbles on theme toggle.
- **Focus width change doesn't affect layout** — border width is already
  part of the box model today; the 1 → 2 toggle was already exercising
  layout. No new layout path.
- **`Theme::of(ctx)` returns light defaults in tests** — the new
  integration test (see Testing) constructs a dark `Theme` explicitly
  rather than relying on the ambient tree, so this is a non-issue.

## Error Handling

No new failure modes. Color values are `Copy` plain `Color` structs;
`Option::unwrap_or` cannot panic. No I/O, no fonts touched. `Theme::of`
fallback to light is the same guarantee every other theme reader in this
file already relies on.

## Testing

Four tests, all unit/integration-level — no visual/Playwright needed
(these are pure color-plumbing checks).

### Layer 1: Widget unit tests (`vexo/src/widgets/text_edit.rs` test module)

Pattern matches the existing `Text` widget color tests.

1. **`test_text_edit_default_colors_preserved`** —
   `TextEdit::new(controller)` with no `.with_*` produces a
   `DecoratedBox` whose `Style.background == Color::WHITE` and border
   color `== Color::rgb(0.6, 0.6, 0.6)`, and a `TextEditContent` whose
   `color == Color::BLACK`. Guards backward compat.
2. **`test_text_edit_with_colors_applied`** —
   `.with_background(A).with_text_color(B).with_border_color(C)`
   produces bg=A, border=C, content.color=B.
3. **`test_text_edit_focus_keeps_border_color_changes_width`** — building
   the focused state yields the same border color as unfocused but width
   `2.0` instead of `1.0`.

### Layer 2: Integration test (`shared_app/src/chats/chat_screen.rs` test module)

Pattern matches the existing tree-walk at `chat_screen.rs:335-364`.

4. **`test_chat_screen_input_bar_uses_theme_colors`** — build
   `ChatScreen` wrapped in a dark `Theme`, walk the render tree to the
   input-bar `DecoratedBox` and its `TextEditContent` child, assert:
   - `Style.background == ThemeData::dark().surface`
   - `TextEditContent.color == ThemeData::dark().on_surface`

## Rollout Plan

```
Step 1: TextEditContent color field + accessor
   │   (vexo/src/widgets/text_edit_content.rs; default BLACK)
   ▼
Step 2: TextEditRenderObject reads color from content
   │   (vexo/src/render_objects/text_edit.rs:321)
   ▼
Step 3: TextEdit 3 builders + render() uses them
   │   (vexo/src/widgets/text_edit.rs; None fallbacks preserve defaults)
   ▼
Step 4: build_input_bar takes &ThemeData, passes theme colors
   │   (shared_app/src/chats/chat_screen.rs:146, :223-241)
   ▼
Step 5: tests (3 widget unit + 1 integration)
```

**Why this order:** Step 1 before Step 2 so the render object has a field
to read. Step 2 before Step 3 so `TextEdit::render` can pass a color
through a `TextEditContent` that already accepts it. Step 3 before Step 4
so the call site has builders to call. Step 5 last — tests exercise the
wired-up end-to-end path.

Each step ends with `cargo build` (and `cargo test` for Step 5). Per
CLAUDE.md, the assistant never runs the GUI; a manual smoke test by the
user (`cargo run -p desktop_demo`, toggle appearance to Dark) is the
final visual verification.

## Success Criteria

The feature is done when:
1. `cargo build` and `cargo test` pass across all crates.
2. `TextEdit` exposes `.with_background()`, `.with_text_color()`,
   `.with_border_color()`; defaults preserve today's white/black/gray.
3. Focus state on `TextEdit` changes border width 1 → 2 but keeps border
   color constant.
4. The chat input bar's `TextEdit` renders `theme.surface` background,
   `theme.on_surface` text, `theme.outline` border in both light and
   dark mode (user-verified by toggling the appearance picker).
5. The 4 tests listed above pass.

## Out of Scope (Deferred)

- **Internal theme reading in `TextEdit`** — declined by user choice;
  call-site pattern preferred.
- **`.with_focused_border_color()` opt-in** — can be added later if the
  focus color shift is missed. Backward-compatible.
- **Placeholder text / placeholder color** — no placeholder concept
  today.
- **Cursor color theming** — `CURSOR_COLOR` stays hardcoded.
- **Other `TextEdit` call sites** — none exist; change is
  backward-compatible regardless.
