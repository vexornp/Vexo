//! Event context for input event handling.
//!
//! Provides context during element event handling.

use crate::core::{Bounds, Logical, Point};
use crate::input::Modifiers;

use super::{ElementId, StateStorage};

// ============================================================================
// EVENT CONTEXT
// ============================================================================

/// Context provided to elements during event handling.
///
/// Contains information about the event environment:
/// - Pointer position for hit testing
/// - Focus state for keyboard event routing
/// - Element bounds for position calculations
/// - State storage for element-local state
pub struct EventContext<'a> {
    /// Current pointer position in logical coordinates.
    pub pointer_position: Point<Logical>,

    /// Currently focused element (if any).
    pub focused_element: Option<ElementId>,

    /// Bounds of the element receiving the event.
    pub bounds: Bounds<Logical>,

    /// Current keyboard modifiers.
    pub modifiers: Modifiers,

    /// State storage for element-local state.
    pub state: &'a mut StateStorage,

    /// Focus request from the element (if any).
    /// Set by `request_focus()`.
    focus_request: Option<ElementId>,

    /// Whether the element requested to clear focus.
    clear_focus_request: bool,
}

impl<'a> EventContext<'a> {
    /// Create a new event context.
    pub fn new(
        pointer_position: Point<Logical>,
        focused_element: Option<ElementId>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        state: &'a mut StateStorage,
    ) -> Self {
        Self {
            pointer_position,
            focused_element,
            bounds,
            modifiers,
            state,
            focus_request: None,
            clear_focus_request: false,
        }
    }

    /// Check if the pointer is inside the element bounds.
    pub fn is_pointer_inside(&self) -> bool {
        self.bounds.contains(&self.pointer_position)
    }

    /// Check if this element is currently focused.
    pub fn is_focused(&self, element: ElementId) -> bool {
        self.focused_element == Some(element)
    }

    /// Check if any element has focus.
    pub fn has_focus(&self) -> bool {
        self.focused_element.is_some()
    }

    /// Request focus for an element.
    ///
    /// The pipeline will process this request after the event is handled.
    pub fn request_focus(&mut self, element: ElementId) {
        self.focus_request = Some(element);
        self.clear_focus_request = false;
    }

    /// Request to clear focus from the currently focused element.
    pub fn clear_focus(&mut self) {
        self.clear_focus_request = true;
        self.focus_request = None;
    }

    /// Get the focus request (if any).
    pub fn focus_request(&self) -> Option<ElementId> {
        self.focus_request
    }

    /// Check if the element requested to clear focus.
    pub fn should_clear_focus(&self) -> bool {
        self.clear_focus_request
    }

    /// Check if the control key is pressed.
    pub fn is_control_pressed(&self) -> bool {
        self.modifiers.control
    }

    /// Check if the shift key is pressed.
    pub fn is_shift_pressed(&self) -> bool {
        self.modifiers.shift
    }

    /// Check if the alt key is pressed.
    pub fn is_alt_pressed(&self) -> bool {
        self.modifiers.alt
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bounds;

    #[test]
    fn test_event_context_is_pointer_inside() {
        let mut state = StateStorage::new();
        let ctx = EventContext::new(
            Point::new(50.0, 50.0),
            None,
            Bounds::from_xywh(0.0, 0.0, 100.0, 100.0),
            Modifiers::default(),
            &mut state,
        );

        assert!(ctx.is_pointer_inside());

        let mut state = StateStorage::new();
        let ctx = EventContext::new(
            Point::new(150.0, 50.0),
            None,
            Bounds::from_xywh(0.0, 0.0, 100.0, 100.0),
            Modifiers::default(),
            &mut state,
        );

        assert!(!ctx.is_pointer_inside());
    }

    #[test]
    fn test_event_context_focus() {
        let element = ElementId::new();
        let mut state = StateStorage::new();
        let ctx = EventContext::new(
            Point::zero(),
            Some(element),
            Bounds::default(),
            Modifiers::default(),
            &mut state,
        );

        assert!(ctx.is_focused(element));
        assert!(ctx.has_focus());
        assert!(!ctx.is_focused(ElementId::new()));
    }

    #[test]
    fn test_event_context_focus_request() {
        let mut state = StateStorage::new();
        let mut ctx = EventContext::new(
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            &mut state,
        );

        let element = ElementId::new();
        ctx.request_focus(element);

        assert_eq!(ctx.focus_request(), Some(element));
        assert!(!ctx.should_clear_focus());
    }

    #[test]
    fn test_event_context_clear_focus_request() {
        let mut state = StateStorage::new();
        let mut ctx = EventContext::new(
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            &mut state,
        );

        ctx.clear_focus();

        assert!(ctx.should_clear_focus());
        assert_eq!(ctx.focus_request(), None);
    }

    #[test]
    fn test_event_context_modifiers() {
        let mut state = StateStorage::new();
        let ctx = EventContext::new(
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::control(),
            &mut state,
        );

        assert!(ctx.is_control_pressed());
        assert!(!ctx.is_shift_pressed());
        assert!(!ctx.is_alt_pressed());
    }
}
