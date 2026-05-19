//! Event handling for the retain-mode three-tree pipeline.
//!
//! Provides the `EventHandler` struct that holds event-related logic
//! extracted from `ThreeTreePipeline`. This is a zero-sized struct
//! used as a namespace for associated functions.

use std::any::Any;
use std::sync::mpsc;

use crate::core::{Absolute, Bounds, Logical, Point, Position};
use crate::input::{ButtonState, InputEvent, Modifiers};

use super::build_owner::BuildOwner;
use super::element::ElementRegistry;
use super::event_context::EventContext;
use super::hit_test::HitTestResult;
use super::id::ElementKey;
use super::render_object::RenderObjectRegistry;
use super::state::StateStorage;

// ============================================================================
// EVENT HANDLER
// ============================================================================

/// Zero-sized struct that serves as a namespace for event handling logic.
///
/// All methods are associated functions that take explicit parameters instead
/// of accessing `ThreeTreePipeline` fields. This keeps event handling
/// independent of the pipeline struct.
pub struct EventHandler;

impl EventHandler {
    /// Handle an input event.
    ///
    /// For pointer events, performs hit testing to find the target element.
    /// For keyboard events, dispatches to the focused element.
    ///
    /// Returns `Some(message)` if the event was handled and produced a message.
    /// The message is returned as `Box<dyn Any>` and should be downcast to the
    /// specific message type by the caller.
    pub fn handle_event(
        element_registry: &mut ElementRegistry,
        render_objects: &RenderObjectRegistry,
        state: &mut StateStorage,
        font_system: &mut glyphon::FontSystem,
        build_owner: &BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focused_element: &mut Option<ElementKey>,
        _position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<Box<dyn Any>> {
        match event {
            InputEvent::PointerMoved { position } => Self::handle_pointer_event(
                element_registry,
                render_objects,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focused_element,
                *position,
                event,
                modifiers,
            ),
            InputEvent::PointerButton { position, .. } => Self::handle_pointer_event(
                element_registry,
                render_objects,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focused_element,
                *position,
                event,
                modifiers,
            ),
            InputEvent::Keyboard { .. } => Self::handle_keyboard_event(
                element_registry,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focused_element,
                event,
                modifiers,
            ),
            _ => None,
        }
    }

    /// Handle a pointer event (moved or button).
    ///
    /// Events are dispatched using Flutter-style bubbling: the event is sent
    /// to each element in the hit test path from deepest (innermost) to
    /// shallowest (root). The first element that handles the event stops
    /// propagation. This allows modifier elements like GestureDetector to
    /// intercept events before they reach the child element.
    pub(crate) fn handle_pointer_event(
        element_registry: &mut ElementRegistry,
        render_objects: &RenderObjectRegistry,
        state: &mut StateStorage,
        font_system: &mut glyphon::FontSystem,
        build_owner: &BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focused_element: &mut Option<ElementKey>,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<Box<dyn Any>> {
        // Convert Point to Position (absolute window coordinates)
        let absolute_position = Position::<Logical, Absolute>::new(position.x, position.y);

        // 1. Hit test to find target and build element path
        let hit_result = render_objects.hit_test(absolute_position);

        if !hit_result.is_hit() {
            return None;
        }

        // 2. Get absolute bounds for context (from hit test result)
        let bounds = hit_result.absolute_bounds().unwrap_or_default();

        // 3. Bubble event up the element path (deepest to shallowest)
        // This matches Flutter's event propagation: innermost element gets
        // the event first, then it bubbles up to parent elements.
        // Modifier elements like GestureDetector can intercept events
        // before they reach the child.
        let element_path = hit_result.element_path();
        let mut any_message: Option<Box<dyn Any>> = None;

        // Iterate from deepest (last) to shallowest (first)
        for &element_id in element_path.iter().rev() {
            if let Some(element) = element_registry.get_mut(element_id) {
                let mut ctx = EventContext::with_build_owner(
                    position,
                    *focused_element,
                    bounds,
                    modifiers,
                    state,
                    font_system,
                    build_owner,
                    dirty_sender,
                );

                let message = element.on_event(event, &mut ctx);

                // Handle focus requests from this element
                if let Some(focus) = ctx.focus_request() {
                    *focused_element = Some(focus);
                } else if ctx.should_clear_focus() {
                    *focused_element = None;
                }

                if message.is_some() {
                    any_message = message;
                    break; // Event handled - stop bubbling
                }
            }
        }

        // If no element handled the event and it's a press, clear focus
        if any_message.is_none() {
            if let InputEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } = event
            {
                *focused_element = None;
            }
        }

        any_message
    }

    /// Handle a keyboard event.
    pub(crate) fn handle_keyboard_event(
        element_registry: &mut ElementRegistry,
        state: &mut StateStorage,
        font_system: &mut glyphon::FontSystem,
        build_owner: &BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focused_element: &mut Option<ElementKey>,
        event: &InputEvent,
        modifiers: Modifiers,
    ) -> Option<Box<dyn Any>> {
        // Get focused element
        let focused = focused_element.as_ref()?;

        // Bounds not critical for keyboard events
        let bounds = Bounds::default();

        let mut ctx = EventContext::with_build_owner(
            Point::zero(),
            *focused_element,
            bounds,
            modifiers,
            state,
            font_system,
            build_owner,
            dirty_sender,
        );

        let any_message = element_registry
            .get_mut(*focused)?
            .on_event(event, &mut ctx);

        // Handle focus requests
        if let Some(focus) = ctx.focus_request() {
            *focused_element = Some(focus);
        } else if ctx.should_clear_focus() {
            *focused_element = None;
        }

        // Return the message directly (already Box<dyn Any>)
        any_message
    }

    /// Hit test at a given position.
    ///
    /// Determines which render object (if any) is at the given position.
    /// Returns a `HitTestResult` containing the path from root to the hit target.
    pub fn hit_test(
        render_objects: &RenderObjectRegistry,
        position: Position<Logical, Absolute>,
    ) -> HitTestResult {
        render_objects.hit_test(position)
    }
}
