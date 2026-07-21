//! Event context for input event handling.
//!
//! Provides context during element event handling, including pointer position,
//! focus state, and font system access. State storage is passed separately
//! to `Element::on_event()` rather than being embedded here, to avoid
//! borrow conflicts when StatefulElement needs both `&mut W::State` and
//! `&mut EventContext`.

use crate::core::{Bounds, Logical, Point};
use crate::input::Modifiers;

use super::build_owner::BuildOwner;
use super::id::ElementKey;
use super::render_object::RenderObjectRegistry;

/// Context provided to elements during event handling.
///
/// Contains information about the event environment:
/// - The element's own ID (for focus requests)
/// - The pointer position in the hit target's local space
/// - Bounds, keyboard modifiers, and font system for text editing
/// - Clipboard for copy/paste/cut
/// - Build owner and dirty sender for marking elements dirty from event handlers
pub struct EventContext<'a> {
    /// The element receiving this event.
    element_id: ElementKey,

    /// Bounds of the element receiving the event.
    bounds: Bounds<Logical>,

    /// Current keyboard modifiers.
    modifiers: Modifiers,

    /// Font system for text editing operations.
    ///
    /// Required by TextEdit for editor actions (insert, delete, cursor movement)
    /// which need font_system for text shaping.
    font_system: &'a mut glyphon::FontSystem,

    /// Clipboard access for copy/paste/cut operations.
    ///
    /// Shared via `Arc` so that the same backend (arboard on desktop, stub on iOS)
    /// can be cheaply cloned into every `EventContext` constructed during event
    /// dispatch without taking ownership of the underlying platform handle.
    clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,

    /// Build owner for marking elements dirty from event handlers.
    ///
    /// Uses a shared reference (`&BuildOwner`) because `mark_needs_build()`
    /// takes `&self` via RefCell interior mutability.
    ///
    /// This is `Some` when the pipeline provides BuildOwner access
    /// (which is the normal case), and `None` in test contexts.
    build_owner: Option<&'a BuildOwner>,

    /// Channel sender for dirty element signals from Signal callbacks.
    ///
    /// When a `Signal::set()` fires its dirty callback from within
    /// an event handler, it sends the element ID through this channel.
    dirty_sender: Option<&'a std::sync::mpsc::Sender<ElementKey>>,

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
}

impl<'a> EventContext<'a> {
    /// Create a new event context.
    pub fn new(
        element_id: ElementKey,
        local_position: Point<Logical>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        font_system: &'a mut glyphon::FontSystem,
        render_objects: Option<&'a RenderObjectRegistry>,
        clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,
    ) -> Self {
        Self {
            element_id,
            local_position,
            bounds,
            modifiers,
            font_system,
            clipboard,
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
        local_position: Point<Logical>,
        bounds: Bounds<Logical>,
        modifiers: Modifiers,
        font_system: &'a mut glyphon::FontSystem,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a std::sync::mpsc::Sender<ElementKey>,
        render_objects: Option<&'a RenderObjectRegistry>,
        clipboard: std::sync::Arc<dyn crate::platform::Clipboard>,
    ) -> Self {
        Self {
            element_id,
            local_position,
            bounds,
            modifiers,
            font_system,
            clipboard,
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

    /// Get the pointer position in the deepest hit target's local space.
    /// Equivalent to Flutter's `localPosition`.
    pub fn local_position(&self) -> Point<Logical> {
        self.local_position
    }

    /// Get the bounds of the element receiving the event.
    pub fn bounds(&self) -> Bounds<Logical> {
        self.bounds
    }

    /// Get the keyboard modifiers active during this event.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Get the font system for text editing operations.
    ///
    /// Returns `&mut` so handlers can pass it to controller methods
    /// (insert, delete, cursor movement) that need font_system for
    /// text shaping.
    pub fn font_system(&mut self) -> &mut glyphon::FontSystem {
        self.font_system
    }

    /// Get the clipboard for copy/paste/cut operations.
    pub fn clipboard(&self) -> &std::sync::Arc<dyn crate::platform::Clipboard> {
        &self.clipboard
    }

    /// Get the build owner, if available.
    ///
    /// Returns `None` in test contexts; `Some` in production.
    pub fn build_owner(&self) -> Option<&BuildOwner> {
        self.build_owner
    }

    /// Get the dirty sender, if available.
    ///
    /// Used by `Signal::set()` callbacks fired from event handlers
    /// to send the element ID through the channel for rebuild scheduling.
    pub fn dirty_sender(&self) -> Option<&std::sync::mpsc::Sender<ElementKey>> {
        self.dirty_sender
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

    /// Get the render object registry, if available.
    pub fn render_objects(&self) -> Option<&RenderObjectRegistry> {
        self.render_objects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_clipboard() -> std::sync::Arc<dyn crate::platform::Clipboard> {
        std::sync::Arc::new(crate::platform::stub_clipboard::StubClipboard)
    }

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

    #[test]
    fn test_event_context_element_id() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let ctx = EventContext::new(
            element,
            Point::zero(),
            Bounds::default(),
            Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );
        assert_eq!(ctx.element_id(), element);
    }

    #[test]
    fn test_event_context_focus_request() {
        let element = make_key();
        let mut font_system = create_test_font_system();
        let mut ctx = EventContext::new(
            element,
            Point::zero(),
            Bounds::default(),
            Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
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
            Bounds::default(),
            Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        ctx.clear_focus();
        assert!(ctx.should_clear_focus());
        assert_eq!(ctx.focus_request(), None);
    }
}
