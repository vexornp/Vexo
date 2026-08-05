# Custom Context Menu View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hardcoded `menu_view` in `vexo_uikit`'s `ContextMenu` with a caller-supplied `MenuBuilder` newtype, so callers can render any widget as the menu content; rewrite ChatScreen's placeholder menu as a builder.

**Architecture:** A `MenuBuilder` newtype wraps `Rc<dyn Fn(&ContextMenuController, &ThemeData) -> Box<dyn Widget>>`. The controller stores the latest builder in an `Rc<RefCell<Option<MenuBuilder>>>`; `show(pos, builder)` sets it. The host invokes the builder inside `render` (so it always sees the live theme), wrapping the result in `Positioned` + dismiss barrier as before. `MenuItem` and `menu_view` are removed entirely. `context_menu_trigger` takes a `MenuBuilder` instead of `Vec<MenuItem>`.

**Tech Stack:** Rust, vexo framework (wgpu/Taffy/glyphon), `cargo test` per crate.

## Global Constraints

- `Rc<dyn Fn>` (not `Arc`/`FnMut`) — single-threaded, matches the existing `MenuItem.on_select` pattern; no `Send + Sync` bounds.
- Builder signature is exactly `Fn(&ContextMenuController, &ThemeData) -> Box<dyn Widget>`. No position parameter.
- `MenuItem` is removed entirely — no convenience helper in this plan.
- The host/trigger/controller split, dismiss barrier, hit-test order, and raw-coordinate positioning are unchanged.
- Right-click plumbing (`on_secondary_press`, arena gating) is unchanged — out of scope.
- Per CLAUDE.md: run `cargo build` after Rust edits, `cargo test` after features; never run `cargo run -p desktop_demo` yourself.
- Spec: `docs/superpowers/specs/2026-08-05-custom-context-menu-view-design.md`.

---

## File Structure

- **Modify:** `vexo_uikit/src/context_menu.rs` — rewrite the trio (`MenuBuilder` added, `ContextMenuController` payload changed, `ContextMenu::render` calls builder, `context_menu_trigger` signature changed, `MenuItem` + `menu_view` removed, tests rewritten).
- **Modify:** `vexo_uikit/src/lib.rs:23` — re-export drops `MenuItem`, adds `MenuBuilder`.
- **Modify:** `shared_app/src/chats/chat_screen.rs` — import swap; `placeholder_menu_items()` → `placeholder_menu_builder()`; the 2 context-menu tests stay structurally identical (assertions unchanged; the path producing "Copy" differs).

No new files. No changes to `data.rs`, `chats/desktop.rs`, `chats/mod.rs`, or `app.rs` (they only thread `ContextMenuController`, never `MenuItem`).

---

## Task 1: Rewrite the `ContextMenu` trio to use `MenuBuilder`

**Files:**
- Modify: `vexo_uikit/src/context_menu.rs` (full rewrite of non-test code + test rewrites)
- Modify: `vexo_uikit/src/lib.rs:23` (re-export swap)

**Interfaces:**
- Consumes: `vexo::Signal`, `vexo::core::{Logical, Point}`, `vexo::{Component, RenderContext, SimpleState, Widget}`, `vexo::Theme`/`ThemeData`, `vexo::GestureDetector::on_tap`, `vexo::Positioned`, `vexo::Stack`, `vexo::WithLayout`, `vexo::Layout`, `vexo::Text`, `vexo::DecoratedBox`, `vexo::Style`, `vexo::BoxShadow`, `vexo::Color` — all pre-existing, unchanged.
- Produces (public API downstream tasks rely on):
  - `pub struct MenuBuilder(Rc<dyn Fn(&ContextMenuController, &ThemeData) -> Box<dyn Widget>>)` with `new`, `Clone`, `Deref`.
  - `ContextMenuController::show(&self, position: Point<Logical>, builder: MenuBuilder)`
  - `ContextMenuController::builder_snapshot(&self) -> Option<MenuBuilder>`
  - `context_menu_trigger(child: impl Widget + 'static, controller: ContextMenuController, builder: MenuBuilder) -> Box<dyn Widget>`

- [ ] **Step 1: Rewrite the non-test portion of `vexo_uikit/src/context_menu.rs`**

Replace the entire file content above the `#[cfg(test)] mod tests` block. The new code: adds `MenuBuilder`, switches the controller payload from `Vec<MenuItem>` to `Option<MenuBuilder>`, updates `show`/`builder_snapshot`, removes `MenuItem` and `menu_view`, and calls the builder in `render`.

```rust
//! Context menu widget trio: `MenuBuilder`, `ContextMenuController`, `ContextMenu` host.
//!
//! Mirrors the `ScrollController` pattern: the screen owns a controller,
//! wraps its root in `ContextMenu::new(child, controller)`, and wraps each
//! right-clickable element in `context_menu_trigger(child, controller, builder)`.
//!
//! The menu's visual content is fully caller-supplied via `MenuBuilder`. The
//! builder runs at render time (inside `ContextMenu::render`), so it always
//! reads the current theme. Each trigger captures its own builder, so different
//! bubbles can render different menu styles.

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use vexo::core::{Logical, Point};
use vexo::Signal;
use vexo::{Component, RenderContext, SimpleState, Widget};

// ============================================================================
// MenuBuilder
// ============================================================================

/// Caller-supplied factory that produces the menu's widget content.
///
/// Wraps `Rc<dyn Fn(&ContextMenuController, &ThemeData) -> Box<dyn Widget>>`.
/// `Rc<dyn Fn>` (not `FnMut`): the builder is cloned into the controller's
/// cell and re-invoked on every rebuild; `Rc` keeps clones cheap and matches
/// the single-threaded pattern used elsewhere in `vexo_uikit` (no `Send +
/// Sync` bounds that `Arc` would impose).
///
/// The builder runs inside `ContextMenu::render`, so it always sees the live
/// `ThemeData` — theme toggles re-render the menu correctly. It receives
/// `&ContextMenuController` so its item rows can call `controller.close()` on
/// tap.
#[derive(Clone)]
pub struct MenuBuilder(Rc<dyn Fn(&ContextMenuController, &vexo::ThemeData) -> Box<dyn Widget>>);

impl MenuBuilder {
    pub fn new(
        f: impl Fn(&ContextMenuController, &vexo::ThemeData) -> Box<dyn Widget> + 'static,
    ) -> Self {
        Self(Rc::new(f))
    }
}

impl Deref for MenuBuilder {
    type Target = dyn Fn(&ContextMenuController, &vexo::ThemeData) -> Box<dyn Widget>;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

// ============================================================================
// ContextMenuController
// ============================================================================

/// Controller for a context menu — owns open/close state and the current builder.
///
/// Created by the screen's caller (alongside `ScrollController::new()`), held
/// as a field, `.clone()`d into triggers and the host. The `Signal` and
/// `Rc<RefCell>` share underlying state across clones, so widget-struct
/// recreation on rebuild doesn't lose menu state.
///
/// The `Signal` carries only `Option<Point<Logical>>` (position when open,
/// `None` when closed) — not the builder. This is because `MenuBuilder`
/// contains `Rc<dyn Fn>` which is `!Send + !Sync`, violating `signal_value`'s
/// `T: Send + Sync` bound. The builder is stored separately in
/// `Rc<RefCell<Option<MenuBuilder>>>`.
#[derive(Clone)]
pub struct ContextMenuController {
    position: Signal<Option<Point<Logical>>>,
    builder: Rc<RefCell<Option<MenuBuilder>>>,
}

impl ContextMenuController {
    pub fn new() -> Self {
        Self {
            position: Signal::new(None),
            builder: Rc::new(RefCell::new(None)),
        }
    }

    /// Open the menu at `position` with the given `builder`.
    pub fn show(&self, position: Point<Logical>, builder: MenuBuilder) {
        *self.builder.borrow_mut() = Some(builder);
        self.position.set(Some(position));
    }

    /// Close the menu.
    pub fn close(&self) {
        self.position.set(None);
    }

    /// The position signal — read by the `ContextMenu` host via `signal_value`.
    pub fn position_signal(&self) -> &Signal<Option<Point<Logical>>> {
        &self.position
    }

    /// Snapshot the current builder (clones the `Option<MenuBuilder>` out of
    /// the `RefCell` so the borrow is released immediately). Called by the
    /// host during `render()` only when the position signal is `Some`.
    /// Returns `None` if the menu is closed (no builder set).
    pub fn builder_snapshot(&self) -> Option<MenuBuilder> {
        self.builder.borrow().clone()
    }
}

impl Default for ContextMenuController {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ContextMenu host (Component)
// ============================================================================

/// A host widget that renders a context menu overlay on top of its child.
///
/// Wrap the screen's root content in `ContextMenu::new(content, controller)`.
/// When `controller.show(pos, builder)` is called (e.g. by a
/// `context_menu_trigger` on right-click), the host rebuilds and shows a
/// floating menu at `pos` — the menu's content is whatever the caller's
/// `builder` returns — with a full-size dismiss barrier behind it.
///
/// The host must be OUTSIDE any `ScrollView` (which clips with `overflow:
/// Hidden`). Place it at the screen root so the menu floats above all content.
pub struct ContextMenu {
    controller: ContextMenuController,
    child: Box<dyn Widget>,
}

impl ContextMenu {
    pub fn new(child: impl Widget + 'static, controller: ContextMenuController) -> Self {
        Self {
            controller,
            child: Box::new(child),
        }
    }
}

impl Clone for ContextMenu {
    fn clone(&self) -> Self {
        Self {
            controller: self.controller.clone(),
            child: self.child.clone_boxed(),
        }
    }
}

impl Component for ContextMenu {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut SimpleState<()>, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = vexo::Theme::of(ctx);
        let position = ctx.signal_value(self.controller.position_signal());
        let controller = self.controller.clone();

        let mut stack = vexo::Stack::new().push(self.child.clone_boxed());

        if let Some(pos) = position {
            let builder = self.controller.builder_snapshot();
            if let Some(builder) = builder {
                let ctrl_for_barrier = controller.clone();

                // Child 1: dismiss barrier — full-size transparent press target.
                // Positioned with all insets 0 so it overlaps the content (non-
                // positioned children flow in the Stack's column, they don't
                // overlap). Hit-tested AFTER the menu (reverse order) but BEFORE
                // the content.
                //
                // The empty Text is wrapped in a WithLayout with
                // width_percent(1.0).height_percent(1.0) so the GestureDetector
                // (pass-through) fills the Positioned's content box. Without this,
                // Text::new("") has zero intrinsic size, the GestureDetector's
                // computed_bounds would be zero, and clicks inside the barrier
                // would not hit the GestureDetector — they'd stop at the
                // Positioned (whose on_event is a no-op) and never fire on_press.
                let barrier = vexo::Positioned::new(
                    vexo::GestureDetector::new(vexo::WithLayout::new(
                        vexo::Text::new(""),
                        vexo::Layout::default()
                            .width_percent(1.0)
                            .height_percent(1.0),
                    ))
                    .on_press(move || ctrl_for_barrier.close()),
                )
                .left(0.0)
                .top(0.0)
                .right(0.0)
                .bottom(0.0);

                stack = stack.push(barrier);

                // Child 2: the menu itself — built by the caller's builder,
                // positioned at the click coordinates. The builder runs here
                // (inside render), so it reads the live theme.
                let menu = builder(&controller, &theme);
                let positioned_menu = vexo::Positioned::new(menu).left(pos.x).top(pos.y);
                stack = stack.push(positioned_menu);
            }
        }

        stack.boxed()
    }
}

// ============================================================================
// context_menu_trigger — sugar for wrapping a child with right-click detection
// ============================================================================

/// Wrap `child` with a right-click handler that opens the context menu at the
/// cursor position, rendering content from `builder`.
///
/// Equivalent to:
/// ```ignore
/// child.on_secondary_press(move |pos| controller.show(pos, builder))
/// ```
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    builder: MenuBuilder,
) -> Box<dyn Widget> {
    let ctrl = controller.clone();
    child.on_secondary_press(move |pos| {
        ctrl.show(pos, builder.clone());
    })
}
```

- [ ] **Step 2: Update the `vexo_uikit/src/lib.rs` re-export**

Change line 23 from:
```rust
pub use context_menu::{context_menu_trigger, ContextMenu, ContextMenuController, MenuItem};
```
to:
```rust
pub use context_menu::{context_menu_trigger, ContextMenu, ContextMenuController, MenuBuilder};
```

- [ ] **Step 3: Rewrite the test module in `vexo_uikit/src/context_menu.rs`**

Replace the entire `#[cfg(test)] mod tests { ... }` block. The shared test builder helper `test_builder(label)` produces a `Text` widget. The 5 existing tests are adapted from `Vec<MenuItem>` to `MenuBuilder`; assertions (position state, render-tree presence, item-tap fires + closes, barrier dismisses) stay structurally identical. A new `test_builder_reads_current_theme` locks in the render-time theme guarantee.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::core::ScaleSource;
    use vexo::core::Size;
    use vexo::input::{ButtonState, InputEvent, Modifiers, PointerButton};
    use vexo::layout::TaffyLayoutEngine;
    use vexo::platform::stub_clipboard::StubClipboard;
    use vexo::platform::Clipboard;
    use vexo::render_objects::TextRenderObject;
    use vexo::resource::new_font_system;
    use vexo::RenderObjectKey;
    use vexo::RenderObjectRegistry;
    use vexo::Text;
    use vexo::ThreeTreePipeline;

    fn test_clipboard() -> Arc<dyn Clipboard> {
        Arc::new(StubClipboard)
    }

    /// A minimal builder that renders a single `Text` widget with the given
    /// label. Ignores controller + theme — enough for layout/render assertions.
    fn test_builder(label: &'static str) -> MenuBuilder {
        MenuBuilder::new(move |_ctrl, _theme| vexo::Text::new(label).boxed())
    }

    fn find_text_in_tree(reg: &RenderObjectRegistry, key: RenderObjectKey, needle: &str) -> bool {
        let ro = match reg.get(key) {
            Some(ro) => ro,
            None => return false,
        };
        if ro
            .as_any()
            .downcast_ref::<TextRenderObject>()
            .map_or(false, |t| t.content().contains(needle))
        {
            return true;
        }
        for &child in ro.children() {
            if find_text_in_tree(reg, child, needle) {
                return true;
            }
        }
        false
    }

    #[test]
    fn test_controller_show_close() {
        let controller = ContextMenuController::new();
        assert_eq!(controller.position_signal().get(), None);
        assert!(controller.builder_snapshot().is_none());

        controller.show(Point::new(100.0, 200.0), test_builder("Copy"));

        assert_eq!(
            controller.position_signal().get(),
            Some(Point::new(100.0, 200.0))
        );
        assert!(controller.builder_snapshot().is_some());

        controller.close();
        assert_eq!(controller.position_signal().get(), None);
        // The builder remains in the cell after close — host doesn't render it
        // because position is None. It'll be overwritten on next show().
    }

    #[test]
    fn test_controller_clone_shares_state() {
        let controller = ContextMenuController::new();
        let cloned = controller.clone();

        controller.show(Point::new(50.0, 60.0), test_builder("A"));

        // The clone sees the same state (shared via Signal's Arc + Rc).
        assert_eq!(cloned.position_signal().get(), Some(Point::new(50.0, 60.0)));
        assert!(cloned.builder_snapshot().is_some());
    }

    #[test]
    fn test_host_closed_has_only_content() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // When closed, the render tree should NOT contain the menu content.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            !find_text_in_tree(ro_reg, root, "Copy"),
            "menu content 'Copy' should not be rendered when menu is closed"
        );
    }

    #[test]
    fn test_host_open_renders_menu_at_position() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open the menu at (100, 200) with a builder that renders "Copy".
        controller.show(Point::new(100.0, 200.0), test_builder("Copy"));
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // The menu content should now appear in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "menu content 'Copy' should be rendered when menu is open"
        );
    }

    #[test]
    fn test_item_tap_fires_on_select_and_closes() {
        let selected = Rc::new(std::cell::Cell::new(false));
        let selected_clone = selected.clone();

        // A builder that renders a single tappable row. on_tap flips the cell
        // and closes the menu — mirrors a real menu item.
        let builder = MenuBuilder::new(move |ctrl, _theme| {
            let ctrl = ctrl.clone();
            let selected = selected_clone.clone();
            vexo::GestureDetector::new(vexo::WithLayout::new(
                vexo::Text::new("Copy"),
                vexo::Layout::default().padding(8.0).width(160.0),
            ))
            .on_tap(move || {
                selected.set(true);
                ctrl.close();
            })
            .boxed()
        });

        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(Point::new(10.0, 10.0), builder);
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // The menu is Positioned at (10, 10), the item row starts at (10, 10)
        // in window coords. Text padding is 8px, so clicking at (15, 15)
        // hits the row.
        let primary_press = InputEvent::PointerButton {
            position: Point::new(15.0, 15.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        let primary_release = InputEvent::PointerButton {
            position: Point::new(15.0, 15.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(15.0, 15.0),
            &primary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        pipeline.handle_event(
            Point::new(15.0, 15.0),
            &primary_release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(selected.get(), "on_tap should have fired");
        // After the tap, the menu should close (controller.close() called
        // by the item's on_tap closure). The Signal::set triggers a rebuild;
        // perform_rebuilds processes it.
        pipeline.perform_rebuilds();
        assert_eq!(
            controller.position_signal().get(),
            None,
            "menu should be closed after item tap"
        );
    }

    #[test]
    fn test_barrier_dismiss_on_outside_click() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(Point::new(10.0, 10.0), test_builder("Copy"));
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Click far away from the menu — should hit the barrier and close.
        let primary_press = InputEvent::PointerButton {
            position: Point::new(350.0, 550.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(350.0, 550.0),
            &primary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        pipeline.perform_rebuilds();
        assert_eq!(
            controller.position_signal().get(),
            None,
            "menu should be closed after clicking outside (barrier dismiss)"
        );
    }

    #[test]
    fn test_builder_reads_current_theme() {
        // A builder that encodes theme.surface into the rendered text label,
        // so we can assert the builder saw the live theme at render time.
        // theme.surface is a Color; we read its red channel into the label.
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let builder = MenuBuilder::new(|_ctrl, theme| {
            let r = theme.surface.r();
            vexo::Text::new(format!("surface-r-{}", r)).boxed()
        });
        controller.show(Point::new(10.0, 10.0), builder);
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // The builder ran during render, so the surface color's red channel
        // must appear in the tree. We assert the *prefix* (proving the builder
        // ran and read theme) rather than a specific value (which depends on
        // the default theme's surface color).
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "surface-r-"),
            "builder output (derived from theme.surface) should be in the render tree"
        );
    }
}
```

Note: if `theme.surface.r()` does not exist (the `Color` API may name the channel accessor differently), adjust to the actual accessor — check `vexo/src/...` for the `Color` type's red-channel getter. The intent is to read *some* numeric property of `theme.surface` into the label; any accessor that compiles satisfies the test's goal (prove the builder read the live theme).

- [ ] **Step 4: Build `vexo_uikit` to catch compile errors**

Run: `cargo build -p vexo_uikit`
Expected: BUILD SUCCEEDS. If it fails, the most likely causes are: (a) `theme.surface.r()` accessor name in `test_builder_reads_current_theme` — fix to the real accessor; (b) a stale `MenuItem` reference somewhere in `vexo_uikit` — search and remove.

- [ ] **Step 5: Run `vexo_uikit` tests**

Run: `cargo test -p vexo_uikit`
Expected: all 6 context-menu tests pass (`test_controller_show_close`, `test_controller_clone_shares_state`, `test_host_closed_has_only_content`, `test_host_open_renders_menu_at_position`, `test_item_tap_fires_on_select_and_closes`, `test_barrier_dismiss_on_outside_click`, `test_builder_reads_current_theme`) plus any pre-existing tests in the crate.

- [ ] **Step 6: Commit**

```bash
git add vexo_uikit/src/context_menu.rs vexo_uikit/src/lib.rs
git commit -m "feat(vexo_uikit): replace MenuItem with MenuBuilder for custom context menu views"
```

---

## Task 2: Rebuild ChatScreen's placeholder menu as a `MenuBuilder`

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs:12-15` (imports)
- Modify: `shared_app/src/chats/chat_screen.rs:260-266` (`placeholder_menu_items` → `placeholder_menu_builder`)
- Modify: `shared_app/src/chats/chat_screen.rs:131` (call site: pass builder instead of items)

**Interfaces:**
- Consumes: `vexo_uikit::{context_menu_trigger, ContextMenuController, MenuBuilder}` (produced by Task 1).
- Produces: a `ChatScreen` whose placeholder menu is a builder closure. The 2 context-menu tests (`test_right_click_bubble_opens_context_menu`, `test_left_click_bubble_does_not_open_context_menu`) stay structurally identical — assertions unchanged; only the path producing "Copy" differs.

- [ ] **Step 1: Swap the imports in `shared_app/src/chats/chat_screen.rs`**

Change lines 12-15 from:
```rust
use vexo_uikit::{
    context_menu_trigger, Button, ButtonVariant, ContextMenu, ContextMenuController,
    KeyboardAvoider, MenuItem,
};
```
to:
```rust
use vexo_uikit::{
    context_menu_trigger, Button, ButtonVariant, ContextMenu, ContextMenuController,
    KeyboardAvoider, MenuBuilder,
};
```

- [ ] **Step 2: Replace `placeholder_menu_items` with `placeholder_menu_builder`**

Replace lines 260-266 (the `placeholder_menu_items` fn) with the builder version. The visual recipe (DecoratedBox + column of padded Text rows, surface/outline/8px/shadow) is preserved verbatim from the old `menu_view`; only the wrapping shape changes.

```rust
fn placeholder_menu_builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| {
        // (label, log message) pairs. Bound as a single `item` per iteration
        // (not destructured) to stay within the `column!` macro's known
        // `for x in iter` single-binding form.
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

- [ ] **Step 3: Update the call site in `render`**

Change line 131 from:
```rust
                    placeholder_menu_items(),
```
to:
```rust
                    placeholder_menu_builder(),
```

The surrounding `context_menu_trigger(...)` call (lines 123-132) is otherwise unchanged — `ctrl.clone()` and the bubble builder stay as-is.

- [ ] **Step 4: Build `shared_app` to confirm the wiring compiles**

Run: `cargo build -p shared_app`
Expected: BUILD SUCCEEDS. The `Rc` import on line 4 is still needed by other code in the file (`on_send: Rc<dyn Fn(&str)>`, etc.) — do not remove it.

- [ ] **Step 5: Run `shared_app` tests (ChatScreen regression net)**

Run: `cargo test -p shared_app`
Expected: all ChatScreen tests pass, including:
- `test_right_click_bubble_opens_context_menu` — right-click a bubble, `"Copy"` (now produced by `placeholder_menu_builder`) appears in the render tree. The assertion at line 911 is unchanged.
- `test_left_click_bubble_does_not_open_context_menu` — left-click does not open the menu. The assertion at line 978 is unchanged.
- The 4 existing ChatScreen test constructors (which set `context_menu: ContextMenuController::new()`) are unchanged and must still pass.

If `test_right_click_bubble_opens_context_menu` fails to find `"Copy"` after right-click, do NOT "fix" the test by loosening the assertion — investigate whether the builder's output shape diverged from the old `menu_view` (it should not; the recipe is preserved).

- [ ] **Step 6: Commit**

```bash
git add shared_app/src/chats/chat_screen.rs
git commit -m "feat(shared_app): rebuild ChatScreen context menu as a MenuBuilder"
```

---

## Verification (after both tasks)

- [ ] `cargo build` (whole workspace) succeeds.
- [ ] `cargo test` (whole workspace) succeeds — all `vexo_uikit` and `shared_app` tests green.
- [ ] No remaining references to `MenuItem` or `menu_view` anywhere in the tree:
  Run: `rg "MenuItem|menu_view" vexo_uikit/src shared_app/src`
  Expected: no matches (the spec doc and this plan are outside `src/`, so they're fine).
