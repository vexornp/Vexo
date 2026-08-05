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
        // A builder that encodes theme.surface.r into the rendered text label.
        // The builder runs inside `ContextMenu::render`, so it must re-run with
        // the *current* theme whenever the `Theme` InheritedWidget changes —
        // this is the whole justification for running the builder at render
        // time instead of pre-building the menu widget.
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        // Two distinct themes so the assertion can tell them apart. We compute
        // the expected labels from the themes themselves (rather than hardcoding
        // float strings) so the test stays robust to color-preset tweaks.
        let light_theme = vexo::ThemeData::light();
        let dark_theme = vexo::ThemeData::dark();
        let light_label = format!("surface-r-{}", light_theme.surface.r);
        let dark_label = format!("surface-r-{}", dark_theme.surface.r);
        assert_ne!(
            light_label, dark_label,
            "light and dark surface.r must differ for this test to be meaningful"
        );

        // Wrap the host in Theme(light) so the builder reads the light theme
        // via Theme::of(ctx) during render.
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(vexo::Theme::new(light_theme.clone(), host.clone()).boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open the menu. The builder runs in render() and must read the light
        // theme's surface.r.
        let builder = MenuBuilder::new(|_ctrl, theme| {
            let r = theme.surface.r;
            vexo::Text::new(format!("surface-r-{}", r)).boxed()
        });
        controller.show(Point::new(10.0, 10.0), builder);
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, &light_label),
            "builder should have rendered the light theme's surface.r ({:?})",
            light_label
        );

        // Toggle: re-wrap the host in Theme(dark). The InheritedWidget change
        // invalidates the ContextMenu element (a Theme::of dependent), forcing
        // render() — and thus the builder — to re-run with the dark theme.
        // The controller state (position + builder) is shared via Rc/Signal,
        // so the menu stays open across the toggle.
        pipeline.update(vexo::Theme::new(dark_theme.clone(), host.clone()).boxed());
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, &dark_label),
            "builder should have re-run with the dark theme's surface.r ({:?}) after the toggle",
            dark_label
        );
        assert!(
            !find_text_in_tree(ro_reg, root, &light_label),
            "light theme's label must be gone after toggling to dark — the builder re-ran"
        );
    }
}
