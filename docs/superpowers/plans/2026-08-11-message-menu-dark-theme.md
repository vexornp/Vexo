# Message Menu Dark Theme Support — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the message-bubble context menu inherit the ambient dark theme (currently it always renders light: white bg + black text) and drop the invisible-in-dark-mode shadows.

**Architecture:** Two changes. (1) Invert the `Theme`/`ContextMenu` wrap order in `app.rs` so `ContextMenu::render`'s `Theme::of(ctx)` call finds the `Theme` as an ancestor instead of falling back to `ThemeData::light()`. (2) Add a `menu_card_style` helper that drops the drop shadow in dark mode (where it's invisible against near-black surface) and use it for both the reactions pill and the actions card. A TDD regression test drives (2) and locks in the theme-inheritance contract.

**Tech Stack:** Rust, Vexo framework (InheritedWidget theme system, `Theme::of(ctx)` lookup), `vexo_uikit::ContextMenu`, Taffy layout, `cargo test`.

## Global Constraints

- **No new modules, no new public types.** All changes are local to `shared_app/src/app.rs` and `shared_app/src/chats/message_menu.rs`.
- **`Theme` is layout-pass-through** (`ProxyRenderObject` via `impl_widget_for_inherited!` at `vexo/src/inherited_widget.rs:119`) — inverting the wrap order does NOT change layout; `ContextMenu`'s `Stack` still fills the window for window-local coordinate mapping.
- **`Theme::of(ctx)` walks UP only** (`vexo/src/widgets/theme.rs:128-131`) — `ContextMenu` must be a DESCENDANT of `Theme`, not a parent.
- **Dark theme `surface` = `0x1C1C1E`** (near-black); a `Color::BLACK.with_alpha(0.20)` shadow composited over it is invisible — that's why shadows are dropped in dark mode.
- **`Style.shadows: Vec<BoxShadow>`** is public (`vexo/src/style.rs:81`); `DecoratedBoxRenderObject::style()` returns `&Style` (`vexo/src/render_objects/decorated_box.rs:60`).
- **Reaction semantic colors (`reaction_visual`, `message_menu.rs:29-38`) are NOT touched** — they're intentional and correct in both modes.
- **`should_rebuild` bypass:** theme invalidation is state-driven (from the `is_dark` Signal flip), so the menu re-renders on toggle even though `ChatScreen` overrides `should_rebuild`. Per `CLAUDE.md` three-level ladder.
- **Test framework:** `cargo test -p shared_app`. No new test infrastructure — reuse the pipeline-setup pattern from `test_metrics_match_real_sizes` (`message_menu.rs:588`).

---

## Task 1: Invert Theme/ContextMenu wrap order in app.rs (root-cause fix)

**Files:**
- Modify: `shared_app/src/app.rs:157-164`

**Interfaces:**
- Consumes: `vexo::Theme::new(data, child)` (`vexo/src/widgets/theme.rs:109`), `vexo_uikit::ContextMenu::new(child, controller)`, `state.context_menu: ContextMenuController`.
- Produces: a widget tree where `Theme` is the OUTERMOST widget and `ContextMenu` is its child — so `ContextMenu::render`'s `Theme::of(ctx)` finds the `Theme` ancestor and returns `ThemeData::dark()` in dark mode instead of falling back to `ThemeData::light()`.

**Note on testing:** This is a one-line production tree-shape fix. It cannot be caught by a unit test in `message_menu.rs` (which builds its own tree with the correct wrap order) — it's verified by `cargo build` + the existing test suite not regressing + manual verification (toggle dark, open menu, see dark bg). The automated regression test in Task 2 covers the builder→theme contract but not the `app.rs` wrap order specifically.

- [ ] **Step 1: Read the current app.rs:157-164 to confirm exact content**

Run: `read shared_app/src/app.rs` offset 155, limit 12.

Expected current content:
```rust
        let themed = Theme::new(theme, inner).boxed();

        // Wrap the entire app in `ContextMenu` so the menu's `Stack` fills the
        // window. This makes Stack-local coords == window-logical coords, so
        // `Positioned::left(click_x).top(click_y)` places the menu at the
        // correct on-screen position regardless of which pane the right-clicked
        // bubble lives in.
        ContextMenu::new(themed, state.context_menu.clone()).boxed()
    }
```

- [ ] **Step 2: Edit — invert the wraps and rewrite the comment**

Replace the block above with:

```rust
        // `ContextMenu` must be a DESCENDANT of `Theme` so its `render()`
        // reads the live theme via `Theme::of(ctx)` — the builder it invokes
        // threads that theme into the reactions pill + actions card. If
        // `ContextMenu` wraps `Theme` (as it did before 2026-08-11),
        // `Theme::of` finds no ancestor and falls back to `ThemeData::light()`
        // — the menu renders white-on-black even in dark mode.
        //
        // `Theme` is layout-pass-through (`ProxyRenderObject`), so wrapping it
        // OUTSIDE `ContextMenu` still lets the menu's `Stack` fill the window:
        // `Positioned::left(click_x).top(click_y)` keeps mapping to
        // window-logical coords regardless of which pane the right-clicked
        // bubble lives in.
        let menu_host = ContextMenu::new(inner, state.context_menu.clone());
        Theme::new(theme, menu_host).boxed()
    }
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build -p shared_app`
Expected: BUILD SUCCEEDED. If it fails, check that `inner`, `theme`, `state.context_menu` are still in scope (they are — only the wrap order changed, no new bindings except `menu_host`).

- [ ] **Step 4: Run the existing test suite to verify no regression**

Run: `cargo test -p shared_app`
Expected: ALL TESTS PASS. In particular `test_metrics_match_real_sizes` (`message_menu.rs:588`) must still pass — the menu's laid-out sizes must not change (the wrap inversion is layout-neutral because `Theme` is pass-through).

- [ ] **Step 5: Commit**

```bash
git add shared_app/src/app.rs
git commit -m "fix: wrap ContextMenu inside Theme so message menu inherits dark theme

ContextMenu::render reads the theme via Theme::of(ctx), which walks up to
ancestors only. The old wrap order put ContextMenu OUTSIDE Theme, so the
lookup fell back to ThemeData::light() and the menu rendered white-on-black
regardless of dark mode. Inverting the wrap puts ContextMenu inside Theme;
Theme is layout-pass-through (ProxyRenderObject) so the menu's Stack still
fills the window for window-local coordinate mapping."
```

---

## Task 2: Theme-aware menu card shadows + regression test (TDD)

**Files:**
- Modify: `shared_app/src/chats/message_menu.rs` — add `menu_card_style` helper (after `close_after`, ~line 174); replace the hand-built `Style` blocks in `reaction_pill` (~line 474) and `actions_card` (~line 516); add `test_message_menu_inherits_dark_theme` + `find_decorated_box_key_by_corner_radius` helper in `#[cfg(test)] mod tests`.

**Interfaces:**
- Consumes: `Style::default().corner_radius(.).background(.).border(.,.).shadow(.)` (`vexo/src/style.rs:100-138`), `ThemeData::is_dark()` (`vexo/src/widgets/theme.rs:89-91`), `BoxShadow::new(.).blur(.).offset(.)`, `Color::BLACK.with_alpha(.)`.
- Produces: `fn menu_card_style(theme: &vexo::ThemeData, corner_radius: f32) -> Style` — a local (private) helper that builds the card chrome style: surface bg + outline border + (light mode only) black drop shadow. Used by `reaction_pill` (radius 18) and `actions_card` (radius 12).

**TDD shape:** The test asserts BOTH (a) the card background is `dark.surface` (characterization — passes today, locks in the builder→theme contract) AND (b) the card shadows vec is empty in dark mode (FAILS today — current code always adds a shadow). The failing assertion (b) drives the `menu_card_style` implementation.

- [ ] **Step 1: Write the failing test + test helper in `#[cfg(test)] mod tests`**

Add the following two items at the END of the `mod tests` block (after `test_metrics_match_real_sizes`, before the closing `}` of `mod tests` at line 685). All needed imports (`Arc`, `Rc`, `Size`, `AnimationTicker`, `TaffyLayoutEngine`, `DecoratedBoxRenderObject`, `new_font_system`, `RenderObjectKey`, `RenderObjectRegistry`, `ThreeTreePipeline`, `ContextMenu`, `ContextMenuController`) are already in scope from the existing test module's `use` statements at `message_menu.rs:533-541`.

```rust
    /// Walk the render tree and return the key of the first
    /// `DecoratedBoxRenderObject` whose `Style.corner_radius` matches `radius`.
    /// Sibling of `find_decorated_box_by_corner_radius` (which returns bounds);
    /// this variant returns the key so the caller can read `style()`.
    fn find_decorated_box_key_by_corner_radius(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        radius: f32,
    ) -> Option<RenderObjectKey> {
        if let Some(ro) = reg.get(key) {
            let matches = ro
                .as_any()
                .downcast_ref::<DecoratedBoxRenderObject>()
                .map_or(false, |d| {
                    d.style()
                        .corner_radius
                        .as_ref()
                        .map_or(false, |cr| (cr.radius - radius).abs() < 0.01)
                });
            if matches {
                return Some(key);
            }
            for &child in ro.children() {
                if let Some(k) = find_decorated_box_key_by_corner_radius(reg, child, radius) {
                    return Some(k);
                }
            }
        }
        None
    }

    /// Regression test for dark-theme inheritance (2026-08-11 design). Wraps
    /// the menu host in a DARK `Theme` using the production wrap order
    /// (`Theme::new(dark, ContextMenu::new(...))`), opens the menu, settles
    /// the open spring, then asserts the actions card:
    ///   1. background == dark.surface  (menu inherited the dark theme)
    ///   2. shadows is empty            (menu_card_style drops shadow in dark)
    /// Catches: builder ignoring the theme arg, ContextMenu not reading
    /// `Theme::of`, `menu_card_style` always adding a shadow.
    #[test]
    fn test_message_menu_inherits_dark_theme() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());

        // Production wrap order: Theme OUTSIDE ContextMenu (the fixed order
        // from Task 1). The test builds its own tree, so it validates the
        // builder→theme contract, not the app.rs wrap order per se.
        let dark_theme = vexo::ThemeData::dark();
        let host = vexo::Theme::new(dark_theme.clone(), host);

        // Wrap in MediaQuery (so edge-detection reads a real window size) —
        // mirrors production + test_metrics_match_real_sizes.
        let mq_data = vexo::MediaQueryData {
            size: Size::new(400.0, 600.0),
            ..vexo::MediaQueryData::all_zero()
        };
        let host = vexo::MediaQuery::new(mq_data, host);

        let ticker = Arc::new(AnimationTicker::new());
        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        // Register FontAwesome so the pill's FA icons shape with real glyphs
        // (mirrors test_metrics_match_real_sizes).
        let mut font_system = new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open the menu in the middle of the screen — plenty of room.
        controller.show(
            vexo::core::Point::new(150.0, 280.0),
            builder(0, Rc::new(|_, _| ())),
        );
        pipeline.perform_rebuilds();

        // Settle the open spring (v→1.0) so the card is at full scale and
        // its laid-out size/style reflects the real content.
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Find the actions card (corner_radius=12) and read its style.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let card_key = find_decorated_box_key_by_corner_radius(ro_reg, root, 12.0)
            .expect("actions card (corner_radius=12) should exist when menu is open");
        let card_ro = ro_reg
            .get(card_key)
            .and_then(|ro| ro.as_any().downcast_ref::<DecoratedBoxRenderObject>())
            .expect("downcast DecoratedBoxRenderObject");
        let style = card_ro.style();

        // 1. Background must be the dark theme's surface — proves the menu
        //    inherited the dark theme through ContextMenu → builder. If this
        //    is white (0xFFFFFFFF) the menu fell back to ThemeData::light()
        //    — check the Theme/ContextMenu wrap order in app.rs.
        assert_eq!(
            style.background,
            Some(dark_theme.surface),
            "actions card background should be dark theme surface",
        );

        // 2. No shadow in dark mode — menu_card_style drops it (a black
        //    shadow is invisible against near-black dark surface anyway;
        //    Material dark-mode guidance: de-emphasize shadows).
        assert!(
            style.shadows.is_empty(),
            "actions card should have no shadow in dark mode; \
             menu_card_style must branch on theme.is_dark()",
        );
    }
```

- [ ] **Step 2: Run the test to verify it FAILS on the shadows assertion**

Run: `cargo test -p shared_app test_message_menu_inherits_dark_theme`
Expected: FAIL. The `background == dark.surface` assertion PASSES (current `actions_card` already uses `theme.surface`), but the `style.shadows.is_empty()` assertion FAILS because the current `actions_card` always adds `BoxShadow::new(Color::BLACK.with_alpha(0.20))`.

The failure message should look like:
```
---- chats::message_menu::tests::test_message_menu_inherits_dark_theme stdout ----
thread '...' panicked at 'actions card should have no shadow in dark mode; menu_card_style must branch on theme.is_dark()'
```

- [ ] **Step 3: Add the `menu_card_style` helper after `close_after`**

Insert this helper between `close_after` (ends at line 174) and the `ReactionIconState` doc comment (starts at line 176). The helper is placed next to `close_after` because both are local style-building helpers used by the menu construction functions.

```rust
/// Build the card chrome `Style` for a menu surface (pill or card).
/// In light mode: surface bg + outline border + black drop shadow.
/// In dark mode:  surface bg + outline border only — the border already
/// provides separation against the dark backdrop, and a black shadow
/// would be invisible against near-black surface anyway (Material dark-
/// mode guidance: de-emphasize shadows).
fn menu_card_style(theme: &vexo::ThemeData, corner_radius: f32) -> Style {
    let style = Style::default()
        .corner_radius(corner_radius)
        .background(theme.surface)
        .border(theme.outline, 1.0);
    if theme.is_dark() {
        style
    } else {
        style.shadow(
            BoxShadow::new(Color::BLACK.with_alpha(0.20))
                .blur(12.0)
                .offset(0.0, 4.0),
        )
    }
}
```

- [ ] **Step 4: Replace `reaction_pill`'s hand-built `Style` with `menu_card_style`**

In `reaction_pill` (around line 474-486), replace this block:

```rust
    DecoratedBox::with_style(
        WithLayout::new(row, Layout::default().padding_each(6.0, 6.0, 5.0, 5.0)),
        Style::default()
            .corner_radius(18.0)
            .background(theme.surface)
            .border(theme.outline, 1.0)
            .shadow(
                BoxShadow::new(Color::BLACK.with_alpha(0.20))
                    .blur(12.0)
                    .offset(0.0, 4.0),
            ),
    )
    .boxed()
```

with:

```rust
    DecoratedBox::with_style(
        WithLayout::new(row, Layout::default().padding_each(6.0, 6.0, 5.0, 5.0)),
        menu_card_style(&theme, 18.0),
    )
    .boxed()
```

- [ ] **Step 5: Replace `actions_card`'s hand-built `Style` with `menu_card_style`**

In `actions_card` (around line 516-528), replace this block:

```rust
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
```

with:

```rust
    DecoratedBox::with_style(
        WithLayout::new(column, Layout::default().min_width(200.0)),
        menu_card_style(&theme, 12.0),
    )
    .boxed()
```

- [ ] **Step 6: Run the new test to verify it now PASSES**

Run: `cargo test -p shared_app test_message_menu_inherits_dark_theme`
Expected: PASS. Both assertions now hold: `background == dark.surface` (unchanged) AND `shadows.is_empty()` (the new `menu_card_style` dark branch returns the style without a shadow).

- [ ] **Step 7: Run the full test suite to verify no regression**

Run: `cargo test -p shared_app`
Expected: ALL TESTS PASS. In particular:
- `test_metrics_match_real_sizes` — menu sizes unchanged (222×40 pill, 200×98 card). The `menu_card_style` refactor keeps the same bg/border/corner_radius/shadow values; only the shadow is dropped in dark mode, which doesn't affect layout (shadows are paint-only).
- `test_builder_reads_current_theme` (in `vexo_uikit`) — framework theme toggle still works.

- [ ] **Step 8: Commit**

```bash
git add shared_app/src/chats/message_menu.rs
git commit -m "feat(message_menu): drop menu shadows in dark mode + regression test

Add menu_card_style helper: surface bg + outline border always; black drop
shadow in light mode only. In dark mode the shadow is invisible against
near-black surface (0x1C1C1E) — Material guidance is to de-emphasize
shadows in dark mode; the outline border already provides separation.

Add test_message_menu_inherits_dark_theme: wraps the menu in a dark Theme,
opens it, asserts the actions card bg == dark.surface AND shadows is empty.
Catches future builder regressions (hardcoded colors, shadow-always-on)."
```

---

## Self-Review

**1. Spec coverage:**

| Spec section | Covered by |
|---|---|
| Change 1 — Invert wrap order (root cause) | Task 1 (Steps 2-5) |
| Change 1 — Comment rewrite | Task 1 (Step 2, the new comment explains why ContextMenu must be inside Theme) |
| Change 2 — `menu_card_style` helper | Task 2 (Step 3) |
| Change 2 — Use in `reaction_pill` | Task 2 (Step 4) |
| Change 2 — Use in `actions_card` | Task 2 (Step 5) |
| Change 3 — Regression test | Task 2 (Step 1) — `test_message_menu_inherits_dark_theme` |
| Change 3 — Production wrap order in test | Task 2 (Step 1) — test wraps `Theme::new(dark, ContextMenu::new(...))` |
| Change 3 — Assert on card background | Task 2 (Step 1) — `assert_eq!(style.background, Some(dark_theme.surface))` |
| Data Flow diagram | N/A (documentation only, no code) |
| Manual verification | Task 1 (Steps 3-4 verify build + tests; manual dark-toggle is noted in the spec) |

**2. Placeholder scan:** No TBD/TODO. Every step has exact code or exact commands. ✓

**3. Type consistency:**
- `menu_card_style(theme: &vexo::ThemeData, corner_radius: f32) -> Style` — defined in Step 3, called as `menu_card_style(&theme, 18.0)` / `menu_card_style(&theme, 12.0)` in Steps 4-5. `theme` in `reaction_pill`/`actions_card` is `vexo::ThemeData` (owned, by value), so `&theme` borrows it. ✓
- `find_decorated_box_key_by_corner_radius(reg: &RenderObjectRegistry, key: RenderObjectKey, radius: f32) -> Option<RenderObjectKey>` — defined and called in Step 1's test. ✓
- `Style`, `BoxShadow`, `Color`, `ThemeData`, `is_dark()` — all confirmed public from the API checks. ✓
- `style.background: Option<Color>` / `style.shadows: Vec<BoxShadow>` — asserted as `Some(dark_theme.surface)` / `.is_empty()`. ✓

**4. Honest scope note:** The `app.rs` wrap-order fix (Task 1) has no automated test — it's a production tree-shape decision that can't be unit-tested without instantiating the full `ImState`. The Task 2 test validates the builder→theme contract (using the correct wrap order in its own tree) but does NOT specifically catch a future re-inversion of the `app.rs` wraps. That's covered by the explanatory comment added in Task 1 Step 2. This is an acceptable tradeoff for a one-line fix; a full app-state integration test would be disproportionate.
