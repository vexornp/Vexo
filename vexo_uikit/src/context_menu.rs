//! Context menu widget trio: `MenuItem`, `ContextMenuController`, `ContextMenu` host.
//!
//! Mirrors the `ScrollController` pattern: the screen owns a controller,
//! wraps its root in `ContextMenu::new(child, controller)`, and wraps each
//! right-clickable element in `context_menu_trigger(child, controller, items)`.

use std::cell::RefCell;
use std::rc::Rc;

use vexo::core::{Logical, Point};
use vexo::Signal;
use vexo::{Component, RenderContext, SimpleState, Widget};

// ============================================================================
// MenuItem
// ============================================================================

/// A single context menu entry.
///
/// `on_select` is `Rc<dyn Fn()>` (not `FnMut`) — it is cloned into each
/// `GestureDetector::on_tap` closure. `Rc` makes cloning cheap and avoids
/// `Send + Sync` bounds that `Arc` would impose (the closures capture
/// single-threaded `Rc`-based controllers).
#[derive(Clone)]
pub struct MenuItem {
    pub label: String,
    pub on_select: Rc<dyn Fn()>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, on_select: Rc<dyn Fn()>) -> Self {
        Self {
            label: label.into(),
            on_select,
        }
    }
}

// ============================================================================
// ContextMenuController
// ============================================================================

/// Controller for a context menu — owns open/close state and the current items.
///
/// Created by the screen's caller (alongside `ScrollController::new()`), held
/// as a field, `.clone()`d into triggers and the host. The `Signal` and
/// `Rc<RefCell>` share underlying state across clones, so widget-struct
/// recreation on rebuild doesn't lose menu state.
///
/// The `Signal` carries only `Option<Point<Logical>>` (position when open,
/// `None` when closed) — not the items. This is because `MenuItem` contains
/// `Rc<dyn Fn()>` which is `!Send + !Sync`, violating `signal_value`'s
/// `T: Send + Sync` bound. Items are stored separately in `Rc<RefCell<...>>`.
#[derive(Clone)]
pub struct ContextMenuController {
    position: Signal<Option<Point<Logical>>>,
    items: Rc<RefCell<Vec<MenuItem>>>,
}

impl ContextMenuController {
    pub fn new() -> Self {
        Self {
            position: Signal::new(None),
            items: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Open the menu at `position` with the given `items`.
    pub fn show(&self, position: Point<Logical>, items: Vec<MenuItem>) {
        *self.items.borrow_mut() = items;
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

    /// Snapshot the current items (clones the `Vec<MenuItem>` out of the
    /// `RefCell` so the borrow is released immediately). Called by the host
    /// during `render()`.
    pub fn items_snapshot(&self) -> Vec<MenuItem> {
        self.items.borrow().clone()
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
/// When `controller.show(pos, items)` is called (e.g. by a
/// `context_menu_trigger` on right-click), the host rebuilds and shows a
/// floating menu at `pos` with a full-size dismiss barrier behind it.
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
            let items = self.controller.items_snapshot();
            let ctrl_for_barrier = controller.clone();
            let ctrl_for_menu = controller.clone();

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

            // Child 2: the menu itself, positioned at the click coordinates.
            let menu = menu_view(&items, ctrl_for_menu, &theme);
            let positioned_menu = vexo::Positioned::new(menu).left(pos.x).top(pos.y);
            stack = stack.push(positioned_menu);
        }

        stack.boxed()
    }
}

// ============================================================================
// menu_view — builds the visual menu from items
// ============================================================================

fn menu_view(
    items: &[MenuItem],
    controller: ContextMenuController,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
    let column = vexo::column! {
        for item in items {
            let on_select = Rc::clone(&item.on_select);
            let ctrl = controller.clone();
            vexo::GestureDetector::new(
                vexo::WithLayout::new(
                    vexo::Text::new(item.label.as_str()).with_color(theme.on_surface),
                    vexo::Layout::default().padding(8.0).width(160.0),
                ),
            )
            .on_tap(move || {
                on_select();
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
}

// ============================================================================
// context_menu_trigger — sugar for wrapping a child with right-click detection
// ============================================================================

/// Wrap `child` with a right-click handler that opens the context menu at the
/// cursor position with the given `items`.
///
/// Equivalent to:
/// ```ignore
/// child.on_secondary_press(move |pos| controller.show(pos, items))
/// ```
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    items: Vec<MenuItem>,
) -> Box<dyn Widget> {
    let ctrl = controller.clone();
    child.on_secondary_press(move |pos| {
        ctrl.show(pos, items.clone());
    })
}

#[cfg(test)]
#[allow(unused_imports)]
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
    use vexo::render_objects::PositionedRenderObject;
    use vexo::render_objects::TextRenderObject;
    use vexo::resource::new_font_system;
    use vexo::RenderObject;
    use vexo::RenderObjectKey;
    use vexo::RenderObjectRegistry;
    use vexo::Stack;
    use vexo::Text;
    use vexo::Theme;
    use vexo::ThreeTreePipeline;

    fn test_clipboard() -> Arc<dyn Clipboard> {
        Arc::new(StubClipboard)
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
        assert!(controller.items_snapshot().is_empty());

        let items = vec![
            MenuItem::new("Copy", Rc::new(|| {})),
            MenuItem::new("Delete", Rc::new(|| {})),
        ];
        controller.show(Point::new(100.0, 200.0), items);

        assert_eq!(
            controller.position_signal().get(),
            Some(Point::new(100.0, 200.0))
        );
        assert_eq!(controller.items_snapshot().len(), 2);
        assert_eq!(controller.items_snapshot()[0].label, "Copy");

        controller.close();
        assert_eq!(controller.position_signal().get(), None);
        // Items remain in the cell after close — host doesn't render them
        // because position is None. They'll be overwritten on next show().
    }

    #[test]
    fn test_controller_clone_shares_state() {
        let controller = ContextMenuController::new();
        let cloned = controller.clone();

        controller.show(
            Point::new(50.0, 60.0),
            vec![MenuItem::new("A", Rc::new(|| {}))],
        );

        // The clone sees the same state (shared via Signal's Arc + Rc).
        assert_eq!(cloned.position_signal().get(), Some(Point::new(50.0, 60.0)));
        assert_eq!(cloned.items_snapshot().len(), 1);
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

        // When closed, the render tree should NOT contain the menu items.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            !find_text_in_tree(ro_reg, root, "Copy"),
            "menu item 'Copy' should not be rendered when menu is closed"
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

        // Open the menu at (100, 200).
        controller.show(
            Point::new(100.0, 200.0),
            vec![MenuItem::new("Copy", Rc::new(|| {}))],
        );
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // The menu item text should now appear in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "menu item 'Copy' should be rendered when menu is open"
        );
    }

    #[test]
    fn test_item_tap_fires_on_select_and_closes() {
        let selected = Rc::new(std::cell::Cell::new(false));
        let selected_clone = selected.clone();

        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            Point::new(10.0, 10.0),
            vec![MenuItem::new(
                "Copy",
                Rc::new(move || {
                    selected_clone.set(true);
                }),
            )],
        );
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Tap the menu item. The item text is at (10, 10) with padding 8 +
        // text size. A click at (15, 15) should be inside the item row.
        // We need to find the actual position via layout — but for a
        // simple test, click at a position within the menu area.
        // The menu is Positioned at (10, 10), the item row starts at (10, 10)
        // in window coords. Text padding is 8px, so clicking at (15, 15)
        // hits the first item row.
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

        assert!(selected.get(), "on_select should have fired");
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

        controller.show(
            Point::new(10.0, 10.0),
            vec![MenuItem::new("Copy", Rc::new(|| {}))],
        );
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
}
