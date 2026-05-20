//! Event context for input event handling.
//!
//! Provides context during element event handling.

use std::sync::mpsc;

use crate::core::{Bounds, Logical, Point};
use crate::input::Modifiers;

use super::{ElementKey, StateStorage};
use super::build_owner::BuildOwner;
use super::focus::{FocusManager, FocusNodeKey, UnfocusDisposition};

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
/// - Build owner for marking elements dirty from event handlers
/// - Font system for text editing operations
/// - Focus manager for focus requests via FocusNodeKey
pub struct EventContext<'a> {
    /// Current pointer position in logical coordinates.
    pub pointer_position: Point<Logical>,

    /// Currently focused element (if any).
    pub focused_element: Option<ElementKey>,

    /// Bounds of the element receiving the event.
    pub bounds: Bounds<Logical>,

    /// Current keyboard modifiers.
    pub modifiers: Modifiers,

    /// State storage for element-local state.
    pub state: &'a mut StateStorage,

    /// Font system for text editing operations.
    ///
    /// Required by TextEdit for editor actions (insert, delete, cursor movement)
    /// which need font_system for text shaping. Follows the same pattern as
    /// LayoutContext which also provides font_system.
    pub font_system: &'a mut glyphon::FontSystem,

    /// Build owner for marking elements dirty from event handlers.
    ///
    /// When an event handler calls `setState()` or `StatefulMutable::set()`,
    /// the dirty callback needs access to the BuildOwner. This field
    /// provides that access.
    ///
    /// Uses a shared reference (`&BuildOwner`) because `mark_needs_build()`
    /// takes `&self` via RefCell interior mutability. This is critical for
    /// event handling: the pipeline holds `&mut self` during event dispatch,
    /// and the dirty callbacks fire from within that context. Using a shared
    /// reference avoids aliasing UB that would occur with `&mut BuildOwner`.
    ///
    /// This is `Some` when the pipeline provides BuildOwner access
    /// (which is the normal case), and `None` in test contexts.
    pub build_owner: Option<&'a BuildOwner>,

    /// Channel sender for dirty element signals from StatefulMutable callbacks.
    ///
    /// When a `StatefulMutable::set()` fires its dirty callback from within
    /// an event handler, it sends the element ID through this channel.
    /// The pipeline drains the channel and calls `mark_needs_build()` itself.
    pub dirty_sender: Option<&'a mpsc::Sender<ElementKey>>,

    /// Focus request from the element (if any).
    /// Set by `request_focus()`.
    focus_request: Option<ElementKey>,

    /// Whether the element requested to clear focus.
    clear_focus_request: bool,

    /// Focus manager for focus requests via FocusNodeKey.
    ///
    /// This is `Some` when the pipeline provides FocusManager access
    /// (which is the normal case), and `None` in test contexts.
    focus_manager: Option<&'a mut FocusManager>,
}

impl<'a> EventContext<'a> {
    /// Create a new event context.
    pub fn new(
        pointer_position: Point<Logical>,
        focused_element: Option<ElementKey>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        state: &'a mut StateStorage,
        font_system: &'a mut glyphon::FontSystem,
    ) -> Self {
        Self {
            pointer_position,
            focused_element,
            bounds,
            modifiers,
            state,
            font_system,
            build_owner: None,
            dirty_sender: None,
            focus_request: None,
            clear_focus_request: false,
            focus_manager: None,
        }
    }

    /// Create a new event context with BuildOwner access.
    pub fn with_build_owner(
        pointer_position: Point<Logical>,
        focused_element: Option<ElementKey>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        state: &'a mut StateStorage,
        font_system: &'a mut glyphon::FontSystem,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a mpsc::Sender<ElementKey>,
    ) -> Self {
        Self {
            pointer_position,
            focused_element,
            bounds,
            modifiers,
            state,
            font_system,
            build_owner: Some(build_owner),
            dirty_sender: Some(dirty_sender),
            focus_request: None,
            clear_focus_request: false,
            focus_manager: None,
        }
    }

    /// Create a new event context with BuildOwner and FocusManager access.
    pub fn with_focus_manager(
        pointer_position: Point<Logical>,
        focused_element: Option<ElementKey>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        state: &'a mut StateStorage,
        font_system: &'a mut glyphon::FontSystem,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a mpsc::Sender<ElementKey>,
        focus_manager: &'a mut FocusManager,
    ) -> Self {
        Self {
            pointer_position,
            focused_element,
            bounds,
            modifiers,
            state,
            font_system,
            build_owner: Some(build_owner),
            dirty_sender: Some(dirty_sender),
            focus_request: None,
            clear_focus_request: false,
            focus_manager: Some(focus_manager),
        }
    }

    /// Check if the pointer is inside the element bounds.
    pub fn is_pointer_inside(&self) -> bool {
        self.bounds.contains(&self.pointer_position)
    }

    /// Check if this element is currently focused.
    pub fn is_focused(&self, element: ElementKey) -> bool {
        self.focused_element == Some(element)
    }

    /// Check if any element has focus.
    pub fn has_focus(&self) -> bool {
        self.focused_element.is_some()
    }

    /// Request focus for an element (backward compat).
    ///
    /// The pipeline will process this request after the event is handled.
    pub fn request_focus(&mut self, element: ElementKey) {
        self.focus_request = Some(element);
        self.clear_focus_request = false;
    }

    /// Request focus via FocusManager with a FocusNodeKey.
    ///
    /// This is the preferred way to request focus when a FocusNodeKey
    /// is available. It directly calls FocusManager::request_focus().
    pub fn request_focus_via_manager(&mut self, node_key: FocusNodeKey, user_initiated: bool) {
        if let Some(ref mut fm) = self.focus_manager {
            fm.request_focus(node_key, user_initiated);
        }
    }

    /// Request to clear focus from the currently focused element.
    pub fn clear_focus(&mut self) {
        self.clear_focus_request = true;
        self.focus_request = None;
    }

    /// Get the focus request (if any).
    pub fn focus_request(&self) -> Option<ElementKey> {
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

    /// Mark an element as needing rebuild.
    ///
    /// Convenience method for event handlers that need to trigger
    /// a rebuild after modifying state.
    pub fn mark_needs_build(&self, element_id: ElementKey) {
        if let Some(bo) = self.build_owner {
            bo.mark_needs_build(element_id);
        }
    }

    /// Get a mutable reference to the FocusManager (if available).
    pub fn focus_manager_mut(&mut self) -> Option<&mut FocusManager> {
        self.focus_manager.as_deref_mut()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bounds;
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    fn make_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    fn make_two_keys() -> (ElementKey, ElementKey) {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let k1 = sm.insert(());
        let k2 = sm.insert(());
        (k1, k2)
    }

    #[test]
    fn test_event_context_is_pointer_inside() {
        let mut state = StateStorage::new();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            Point::new(50.0, 50.0),
            None,
            Bounds::from_xywh(0.0, 0.0, 100.0, 100.0),
            Modifiers::default(),
            &mut state,
            &mut font_system,
        );

        assert!(ctx.is_pointer_inside());

        let mut state = StateStorage::new();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            Point::new(150.0, 50.0),
            None,
            Bounds::from_xywh(0.0, 0.0, 100.0, 100.0),
            Modifiers::default(),
            &mut state,
            &mut font_system,
        );

        assert!(!ctx.is_pointer_inside());
    }

    #[test]
    fn test_event_context_focus() {
        let (element, other) = make_two_keys();
        let mut state = StateStorage::new();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            Point::zero(),
            Some(element),
            Bounds::default(),
            Modifiers::default(),
            &mut state,
            &mut font_system,
        );

        assert!(ctx.is_focused(element));
        assert!(ctx.has_focus());
        assert!(!ctx.is_focused(other));
    }

    #[test]
    fn test_event_context_focus_request() {
        let mut state = StateStorage::new();
        let mut font_system = create_test_font_system();
        let mut ctx = EventContext::new(
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            &mut state,
            &mut font_system,
        );

        let element = make_key();
        ctx.request_focus(element);

        assert_eq!(ctx.focus_request(), Some(element));
        assert!(!ctx.should_clear_focus());
    }

    #[test]
    fn test_event_context_clear_focus_request() {
        let mut state = StateStorage::new();
        let mut font_system = create_test_font_system();
        let mut ctx = EventContext::new(
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            &mut state,
            &mut font_system,
        );

        ctx.clear_focus();

        assert!(ctx.should_clear_focus());
        assert_eq!(ctx.focus_request(), None);
    }

    #[test]
    fn test_event_context_modifiers() {
        let mut state = StateStorage::new();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::control(),
            &mut state,
            &mut font_system,
        );

        assert!(ctx.is_control_pressed());
        assert!(!ctx.is_shift_pressed());
        assert!(!ctx.is_alt_pressed());
    }
}
