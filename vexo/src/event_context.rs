//! Event context for input event handling.
//!
//! Provides context during element event handling, including pointer position,
//! focus state, and font system access. State storage is passed separately
//! to `Element::on_event()` rather than being embedded here, to avoid
//! borrow conflicts when StatefulElement needs both `&mut W::State` and
//! `&mut EventContext`.

use crate::core::{Bounds, Logical, Point, Scale, ScaleSource};
use crate::input::Modifiers;

use super::id::ElementKey;
use super::build_owner::BuildOwner;
use super::render_object::RenderObjectRegistry;

/// Context provided to elements during event handling.
///
/// Contains information about the event environment:
/// - The element's own ID (for focus requests and focus checks)
/// - Pointer position for hit testing
/// - Focus state for keyboard event routing
/// - Font system for text editing operations
/// - Build owner for marking elements dirty from event handlers
pub struct EventContext<'a> {
    /// The element receiving this event.
    element_id: ElementKey,

    /// Current pointer position in logical coordinates.
    pub pointer_position: Point<Logical>,

    /// Currently focused element (if any).
    pub focused_element: Option<ElementKey>,

    /// Bounds of the element receiving the event.
    pub bounds: Bounds<Logical>,

    /// Current keyboard modifiers.
    pub modifiers: Modifiers,

    /// Font system for text editing operations.
    ///
    /// Required by TextEdit for editor actions (insert, delete, cursor movement)
    /// which need font_system for text shaping.
    pub font_system: &'a mut glyphon::FontSystem,

    /// Build owner for marking elements dirty from event handlers.
    ///
    /// Uses a shared reference (`&BuildOwner`) because `mark_needs_build()`
    /// takes `&self` via RefCell interior mutability.
    ///
    /// This is `Some` when the pipeline provides BuildOwner access
    /// (which is the normal case), and `None` in test contexts.
    pub build_owner: Option<&'a BuildOwner>,

    /// Channel sender for dirty element signals from Signal callbacks.
    ///
    /// When a `Signal::set()` fires its dirty callback from within
    /// an event handler, it sends the element ID through this channel.
    pub dirty_sender: Option<&'a std::sync::mpsc::Sender<ElementKey>>,

    /// Render object registry for element-to-render-object communication.
    ///
    /// Available when the event handler provides render object access.
    /// Used by scroll-aware elements to query render object state.
    render_objects: Option<&'a RenderObjectRegistry>,

    /// Focus request from the element (if any).
    /// Set by `request_focus()`.
    focus_request: Option<ElementKey>,

    /// Whether the element requested to clear focus.
    clear_focus_request: bool,

    /// Pointer position in the deepest hit target's local coordinate space.
    /// Equivalent to Flutter's `localPosition` — computed as
    /// `pointer_position - inner_bounds.origin`.
    local_position: Point<Logical>,

    /// Shared scale factor source.
    scale_source: ScaleSource,
}

impl<'a> EventContext<'a> {
    /// Create a new event context.
    pub fn new(
        element_id: ElementKey,
        pointer_position: Point<Logical>,
        local_position: Point<Logical>,
        focused_element: Option<ElementKey>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        scale_source: ScaleSource,
        font_system: &'a mut glyphon::FontSystem,
        render_objects: Option<&'a RenderObjectRegistry>,
    ) -> Self {
        Self {
            element_id,
            pointer_position,
            local_position,
            focused_element,
            bounds,
            modifiers,
            scale_source,
            font_system,
            build_owner: None,
            dirty_sender: None,
            render_objects,
            focus_request: None,
            clear_focus_request: false,
        }
    }

    /// Create a new event context with BuildOwner access.
    pub fn with_build_owner(
        element_id: ElementKey,
        pointer_position: Point<Logical>,
        local_position: Point<Logical>,
        focused_element: Option<ElementKey>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        scale_source: ScaleSource,
        font_system: &'a mut glyphon::FontSystem,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a std::sync::mpsc::Sender<ElementKey>,
        render_objects: Option<&'a RenderObjectRegistry>,
    ) -> Self {
        Self {
            element_id,
            pointer_position,
            local_position,
            focused_element,
            bounds,
            modifiers,
            scale_source,
            font_system,
            build_owner: Some(build_owner),
            dirty_sender: Some(dirty_sender),
            render_objects,
            focus_request: None,
            clear_focus_request: false,
        }
    }

    /// Get the element ID receiving this event.
    pub fn element_id(&self) -> ElementKey {
        self.element_id
    }

    /// Check if the pointer is inside the element bounds.
    pub fn is_pointer_inside(&self) -> bool {
        self.bounds.contains(&self.pointer_position)
    }

    /// Get the pointer position in the deepest hit target's local space.
    /// Equivalent to Flutter's `localPosition`.
    pub fn local_position(&self) -> Point<Logical> {
        self.local_position
    }

    /// Get the DPI scale factor.
    pub fn scale(&self) -> Scale {
        self.scale_source.get()
    }

    /// Check if this element is currently focused.
    /// Uses the element's own ID stored in this context.
    pub fn is_focused_self(&self) -> bool {
        self.focused_element == Some(self.element_id)
    }

    /// Check if a specific element is currently focused.
    pub fn is_focused(&self, element: ElementKey) -> bool {
        self.focused_element == Some(element)
    }

    /// Check if any element has focus.
    pub fn has_focus(&self) -> bool {
        self.focused_element.is_some()
    }

    /// Request focus for an element.
    ///
    /// The pipeline will process this request after the event is handled.
    pub fn request_focus(&mut self, element: ElementKey) {
        self.focus_request = Some(element);
        self.clear_focus_request = false;
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
    pub fn mark_needs_build(&self, element_id: ElementKey) {
        if let Some(bo) = self.build_owner {
            bo.mark_needs_build(element_id);
        }
    }

    /// Get the render object registry, if available.
    pub fn render_objects(&self) -> Option<&RenderObjectRegistry> {
        self.render_objects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Bounds, ScaleSource};
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
    fn test_event_context_element_id() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            element,
            Point::zero(),
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            ScaleSource::default(),
            &mut font_system,
            None,
        );
        assert_eq!(ctx.element_id(), element);
    }

    #[test]
    fn test_event_context_is_pointer_inside() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            element,
            Point::new(50.0, 50.0),
            Point::new(50.0, 50.0),
            None,
            Bounds::from_xywh(0.0, 0.0, 100.0, 100.0),
            Modifiers::default(),
            ScaleSource::default(),
            &mut font_system,
            None,
        );
        assert!(ctx.is_pointer_inside());

        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            element,
            Point::new(150.0, 50.0),
            Point::new(150.0, 50.0),
            None,
            Bounds::from_xywh(0.0, 0.0, 100.0, 100.0),
            Modifiers::default(),
            ScaleSource::default(),
            &mut font_system,
            None,
        );
        assert!(!ctx.is_pointer_inside());
    }

    #[test]
    fn test_event_context_is_focused_self() {
        let (element, _other) = make_two_keys();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            element,
            Point::zero(),
            Point::zero(),
            Some(element),
            Bounds::default(),
            Modifiers::default(),
            ScaleSource::default(),
            &mut font_system,
            None,
        );
        assert!(ctx.is_focused_self());

        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            element,
            Point::zero(),
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            ScaleSource::default(),
            &mut font_system,
            None,
        );
        assert!(!ctx.is_focused_self());
    }

    #[test]
    fn test_event_context_focus_request() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let mut ctx = EventContext::new(
            element,
            Point::zero(),
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            ScaleSource::default(),
            &mut font_system,
            None,
        );

        let target = make_key();
        ctx.request_focus(target);
        assert_eq!(ctx.focus_request(), Some(target));
        assert!(!ctx.should_clear_focus());
    }

    #[test]
    fn test_event_context_clear_focus_request() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let mut ctx = EventContext::new(
            element,
            Point::zero(),
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::default(),
            ScaleSource::default(),
            &mut font_system,
            None,
        );

        ctx.clear_focus();
        assert!(ctx.should_clear_focus());
        assert_eq!(ctx.focus_request(), None);
    }

    #[test]
    fn test_event_context_modifiers() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            element,
            Point::zero(),
            Point::zero(),
            None,
            Bounds::default(),
            Modifiers::control(),
            ScaleSource::default(),
            &mut font_system,
            None,
        );

        assert!(ctx.is_control_pressed());
        assert!(!ctx.is_shift_pressed());
        assert!(!ctx.is_alt_pressed());
    }
}