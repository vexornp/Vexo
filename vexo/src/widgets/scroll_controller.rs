//! ScrollController — external handle for programmatic control of ScrollView.
//!
//! Mirrors `TextEditingController` and `NavigationController`: the caller owns
//! the controller, passes it into `ScrollView`, and the framework wires a
//! dirty callback on mount so `jump_to_bottom()` etc. trigger rebuilds.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// Callbacks the element wires on mount so the controller can drive the
/// render object directly (no rebuild needed for offset changes — just
/// `request_frame`).
pub struct ElementState {
    pub apply_offset: Box<dyn Fn(f32) + 'static>, // svro.set_scroll_offset + cache
    pub current_offset: Box<dyn Fn() -> f32 + 'static>,
    pub max_scroll: Box<dyn Fn() -> f32 + 'static>,
    pub request_frame: Box<dyn Fn() + 'static>,
}

pub struct ScrollController {
    target_offset: Rc<RefCell<Option<f32>>>,
    current_offset: Rc<RefCell<f32>>,
    max_scroll: Rc<RefCell<f32>>,
    dirty_callback: Rc<RefCell<Option<Arc<dyn Fn() + Send + Sync>>>>,
    element_state: Rc<RefCell<Option<ElementState>>>,
}

impl ScrollController {
    pub fn new() -> Self {
        Self {
            target_offset: Rc::new(RefCell::new(None)),
            current_offset: Rc::new(RefCell::new(0.0)),
            max_scroll: Rc::new(RefCell::new(0.0)),
            dirty_callback: Rc::new(RefCell::new(None)),
            element_state: Rc::new(RefCell::new(None)),
        }
    }

    pub fn current_offset(&self) -> f32 {
        if let Some(es) = self.element_state.borrow().as_ref() {
            (es.current_offset)()
        } else {
            *self.current_offset.borrow()
        }
    }

    pub fn jump_to_bottom(&self) {
        if let Some(es) = self.element_state.borrow().as_ref() {
            let max = (es.max_scroll)();
            self.apply_offset(max, &es);
        } else {
            // Not mounted yet — defer: store max-scroll sentinel as +inf,
            // applied on mount.
            *self.target_offset.borrow_mut() = Some(f32::INFINITY);
            self.notify();
        }
    }

    pub fn jump_to(&self, offset: f32) {
        if let Some(es) = self.element_state.borrow().as_ref() {
            let max = (es.max_scroll)();
            let clamped = offset.clamp(0.0, max);
            self.apply_offset(clamped, &es);
        } else {
            *self.target_offset.borrow_mut() = Some(offset);
            self.notify();
        }
    }

    fn apply_offset(&self, offset: f32, es: &ElementState) {
        (es.apply_offset)(offset);
        *self.current_offset.borrow_mut() = offset;
        (es.request_frame)();
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

    pub(crate) fn set_element_state(&self, state: ElementState) {
        *self.element_state.borrow_mut() = Some(state);
        // Apply any pending target offset from before mount.
        if let Some(target) = self.target_offset.borrow_mut().take() {
            if let Some(es) = self.element_state.borrow().as_ref() {
                let max = (es.max_scroll)();
                let clamped = if target.is_infinite() {
                    max
                } else {
                    target.clamp(0.0, max)
                };
                self.apply_offset(clamped, &es);
            }
        }
    }

    pub(crate) fn clear_element_state(&self) {
        // Cache final offset so `current_offset()` still works after unmount.
        if let Some(es) = self.element_state.borrow().as_ref() {
            *self.current_offset.borrow_mut() = (es.current_offset)();
        }
        *self.element_state.borrow_mut() = None;
    }

    pub(crate) fn update_max_scroll(&self, max: f32) {
        *self.max_scroll.borrow_mut() = max;
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
            max_scroll: Rc::clone(&self.max_scroll),
            dirty_callback: Rc::clone(&self.dirty_callback),
            element_state: Rc::clone(&self.element_state),
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
}
