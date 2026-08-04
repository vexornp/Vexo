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

#[cfg(test)]
mod tests {
    use super::*;

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
}
