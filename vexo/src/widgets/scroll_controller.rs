//! ScrollController — external handle for programmatic control of ScrollView.
//!
//! Mirrors `TextEditingController`: the caller owns the controller, passes it
//! into `ScrollView`, and the framework wires a dirty callback on mount so
//! `jump_to_bottom()` etc. trigger rebuilds via the deferred-apply pattern.
//!
//! # Deferred-apply pattern
//!
//! `jump_to_bottom` / `jump_to` only store a pending target offset and fire
//! the dirty callback (which sends the element ID through the pipeline's mpsc
//! channel). The actual apply happens in `ScrollViewElement::rebuild_from_state`,
//! which receives a safe `&mut ElementContext` with `&mut RenderObjectRegistry`
//! access — no `unsafe` raw pointers needed. This works both pre-mount (target
//! stored, no callback yet → applied on first `rebuild_from_state` after mount)
//! and post-mount (target stored + callback fires → applied on next pump).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct ScrollController {
    /// Pending target offset requested by `jump_to_bottom` / `jump_to`.
    /// Consumed by `ScrollViewElement::rebuild_from_state` on the next rebuild.
    /// `Some(f32::INFINITY)` is a sentinel meaning "jump to max scroll".
    target_offset: Rc<RefCell<Option<f32>>>,
    /// Last applied offset, written by the element. Read by `current_offset()`.
    current_offset: Rc<RefCell<f32>>,
    /// Dirty callback wired by `ScrollViewElement::mount` (and re-wired on
    /// `update` when the controller instance changes). Sends the element ID
    /// through the pipeline's mpsc channel, which the pipeline drains into the
    /// BuildOwner to schedule a `rebuild_from_state`.
    dirty_callback: Rc<RefCell<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl ScrollController {
    pub fn new() -> Self {
        Self {
            target_offset: Rc::new(RefCell::new(None)),
            current_offset: Rc::new(RefCell::new(0.0)),
            dirty_callback: Rc::new(RefCell::new(None)),
        }
    }

    /// Last applied scroll offset (0.0 before any apply).
    pub fn current_offset(&self) -> f32 {
        *self.current_offset.borrow()
    }

    /// Jump to the bottom of the scrollable content on the next rebuild.
    pub fn jump_to_bottom(&self) {
        *self.target_offset.borrow_mut() = Some(f32::INFINITY);
        self.notify();
    }

    /// Jump to the given offset (clamped to `[0, max_scroll]` on apply) on
    /// the next rebuild.
    pub fn jump_to(&self, offset: f32) {
        *self.target_offset.borrow_mut() = Some(offset);
        self.notify();
    }

    pub fn set_dirty_callback(&self, cb: Arc<dyn Fn() + Send + Sync>) {
        *self.dirty_callback.borrow_mut() = Some(cb);
    }

    pub fn clear_dirty_callback(&self) {
        *self.dirty_callback.borrow_mut() = None;
    }

    fn notify(&self) {
        if let Some(cb) = self.dirty_callback.borrow().as_ref() {
            cb();
        }
    }

    // --- Called by ScrollViewElement ---

    /// Identity check used by `ScrollViewElement::update` to detect controller
    /// swaps on widget replacement (mirrors `TextEditingController`'s
    /// `Rc::ptr_eq` comparison in `TextEditState::on_update`).
    pub(crate) fn is_same_instance(&self, other: &ScrollController) -> bool {
        Rc::ptr_eq(&self.target_offset, &other.target_offset)
    }

    /// Consume and return the pending target offset, if any.
    /// Called by `ScrollViewElement::rebuild_from_state` (deferred-apply).
    pub(crate) fn take_target_offset(&self) -> Option<f32> {
        self.target_offset.borrow_mut().take()
    }

    /// Write back the applied offset so `current_offset()` reads correctly.
    /// Called by `ScrollViewElement::rebuild_from_state` (deferred-apply) and
    /// by `apply_scroll_offset` (interactive scroll wheel / keyboard).
    pub(crate) fn set_current_offset(&self, offset: f32) {
        *self.current_offset.borrow_mut() = offset;
    }
}

impl Default for ScrollController {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ScrollController {
    fn clone(&self) -> Self {
        Self {
            target_offset: Rc::clone(&self.target_offset),
            current_offset: Rc::clone(&self.current_offset),
            dirty_callback: Rc::clone(&self.dirty_callback),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jump_to_bottom_before_mount_stores_pending() {
        let ctrl = ScrollController::new();
        ctrl.jump_to_bottom();
        // No panic; pending offset stored.
        assert_eq!(ctrl.current_offset(), 0.0);
    }

    #[test]
    fn test_jump_to_before_mount_stores_pending() {
        let ctrl = ScrollController::new();
        ctrl.jump_to(150.0);
        assert_eq!(ctrl.current_offset(), 0.0);
    }

    #[test]
    fn test_current_offset_defaults_to_zero() {
        let ctrl = ScrollController::new();
        assert_eq!(ctrl.current_offset(), 0.0);
    }

    #[test]
    fn test_take_target_offset_consumes_pending() {
        let ctrl = ScrollController::new();
        ctrl.jump_to(120.0);
        assert_eq!(ctrl.take_target_offset(), Some(120.0));
        // Second take returns None — target was consumed.
        assert_eq!(ctrl.take_target_offset(), None);
    }

    #[test]
    fn test_set_current_offset_updates_readback() {
        let ctrl = ScrollController::new();
        ctrl.set_current_offset(42.0);
        assert_eq!(ctrl.current_offset(), 42.0);
    }

    #[test]
    fn test_is_same_instance_identifies_clones() {
        let a = ScrollController::new();
        let b = a.clone();
        let c = ScrollController::new();
        assert!(a.is_same_instance(&b));
        assert!(!a.is_same_instance(&c));
    }

    #[test]
    fn test_notify_noops_without_callback() {
        let ctrl = ScrollController::new();
        // No dirty callback wired — should not panic.
        ctrl.jump_to_bottom();
        ctrl.jump_to(50.0);
    }
}
