//! Event handling for the retain-mode three-tree pipeline.
//!
//! Provides the `EventHandler` struct that holds event-related logic
//! extracted from `ThreeTreePipeline`. This is a zero-sized struct
//! used as a namespace for associated functions.

use std::any::Any;
use std::sync::mpsc;

use crate::core::{Absolute, Bounds, Logical, Point, Position, ScaleSource};
use crate::input::{ButtonState, InputEvent, Modifiers};

use super::build_owner::BuildOwner;
use super::element::ElementRegistry;
use super::event_context::EventContext;
use super::focus::FocusManager;
use super::hit_test::HitTestResult;
use super::id::ElementKey;
use super::render_object::RenderObjectRegistry;
use super::element_state::StateStorage;

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
        focus_manager: &mut FocusManager,
        _position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
        scale_source: &ScaleSource,
    ) -> Option<Box<dyn Any>> {
        match event {
            InputEvent::PointerMoved { position } => Self::handle_pointer_event(
                element_registry,
                render_objects,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focus_manager,
                *position,
                event,
                modifiers,
                scale_source,
            ),
            InputEvent::PointerButton { position, .. } => Self::handle_pointer_event(
                element_registry,
                render_objects,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focus_manager,
                *position,
                event,
                modifiers,
                scale_source,
            ),
            InputEvent::Scroll { .. } => Self::handle_scroll_event(
                element_registry,
                render_objects,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focus_manager,
                _position,
                event,
                modifiers,
                scale_source,
            ),
            InputEvent::Keyboard { .. } => Self::handle_keyboard_event(
                element_registry,
                render_objects,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focus_manager,
                event,
                modifiers,
                scale_source,
            ),
            _ => None,
        }
    }

    /// Handle a pointer event (moved or button).
    ///
    /// Events are dispatched using single-phase bubbling: the event is sent
    /// to each element in the hit test path from deepest (innermost) to
    /// shallowest (root). The first element that handles the event stops
    /// propagation. This allows modifier elements like GestureDetector to
    /// intercept events before they reach the child element.
    ///
    /// All elements (including StatefulElement) appear in the hit test path
    /// because they own ProxyRenderObjects that participate in the render tree.
    pub(crate) fn handle_pointer_event(
        element_registry: &mut ElementRegistry,
        render_objects: &RenderObjectRegistry,
        state: &mut StateStorage,
        font_system: &mut glyphon::FontSystem,
        build_owner: &BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
        scale_source: &ScaleSource,
    ) -> Option<Box<dyn Any>> {
        // Convert Point to Position (absolute window coordinates)
        let absolute_position = Position::<Logical, Absolute>::new(position.x, position.y);

        // 1. Hit test to find target and build element path
        let hit_result = render_objects.hit_test(absolute_position);

        if !hit_result.is_hit() {
            // Click outside all widgets — clear focus on pointer press
            if let InputEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } = event
            {
                focus_manager.unfocus();
            }
            return None;
        }

        // 2. Compute local_position relative to the innermost hit target.
        // This is equivalent to Flutter's globalToLocal().
        let local_position = hit_result
            .inner_bounds()
            .map(|b| Point::new(position.x - b.position().x, position.y - b.position().y))
            .unwrap_or(position);

        // 3. Bubble event up the element path (deepest to shallowest)
        let element_path = hit_result.element_path();
        let mut any_message: Option<Box<dyn Any>> = None;

        // Iterate from deepest (last) to shallowest (first)
        for &element_id in element_path.iter().rev() {
            if let Some(element) = element_registry.get_mut(element_id) {
                // Each element gets its own bounds from the hit test,
                // so is_pointer_inside() works correctly even for
                // ancestors of scrolled content.
                let bounds = hit_result.bounds_for_element(element_id)
                    .unwrap_or_default();

                let mut ctx = EventContext::with_build_owner(
                    element_id,
                    position,
                    local_position,
                    focus_manager.primary_focus_element(),
                    bounds,
                    modifiers,
                    scale_source.clone(),
                    font_system,
                    build_owner,
                    dirty_sender,
                    Some(render_objects),
                );

                let message = element.on_event(event, &mut ctx, state);

                // Handle focus requests from this element
                if let Some(focus_element) = ctx.focus_request() {
                    let node_id = focus_manager
                        .node_for_element(focus_element)
                        .expect("Focus node must exist — all mounted elements have FocusAttachments");
                    focus_manager.request_focus(node_id);
                } else if ctx.should_clear_focus() {
                    focus_manager.unfocus();
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
                focus_manager.unfocus();
            }
        }

        any_message
    }

    /// Handle a keyboard event.
    pub(crate) fn handle_keyboard_event(
        element_registry: &mut ElementRegistry,
        render_objects: &RenderObjectRegistry,
        state: &mut StateStorage,
        font_system: &mut glyphon::FontSystem,
        build_owner: &BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        event: &InputEvent,
        modifiers: Modifiers,
        scale_source: &ScaleSource,
    ) -> Option<Box<dyn Any>> {
        // Get focused element
        let focused = focus_manager.primary_focus_element()?;

        // Bounds not critical for keyboard events
        let bounds = Bounds::default();

        let mut ctx = EventContext::with_build_owner(
            focused,
            Point::zero(),
            Point::zero(), // no pointer position for keyboard events
            focus_manager.primary_focus_element(),
            bounds,
            modifiers,
            scale_source.clone(),
            font_system,
            build_owner,
            dirty_sender,
            Some(render_objects),
        );

        let any_message = element_registry
            .get_mut(focused)?
            .on_event(event, &mut ctx, state);

        // Handle focus requests
        if let Some(focus_element) = ctx.focus_request() {
            let node_id = focus_manager
                .node_for_element(focus_element)
                .expect("Focus node must exist — all mounted elements have FocusAttachments");
            focus_manager.request_focus(node_id);
        } else if ctx.should_clear_focus() {
            focus_manager.unfocus();
        }

        any_message
    }

    /// Handle a scroll event by dispatching to the nearest scrollable ancestor.
    ///
    /// Walks the hit path from deepest to shallowest, looking for the first
    /// render object with a scroll offset. When found, dispatches the scroll
    /// event to the corresponding element.
    pub(crate) fn handle_scroll_event(
        element_registry: &mut ElementRegistry,
        render_objects: &RenderObjectRegistry,
        state: &mut StateStorage,
        font_system: &mut glyphon::FontSystem,
        build_owner: &BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
        focus_manager: &mut FocusManager,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
        scale_source: &ScaleSource,
    ) -> Option<Box<dyn Any>> {
        let absolute_position = Position::<Logical, Absolute>::new(position.x, position.y);
        let hit_result = render_objects.hit_test(absolute_position);

        if !hit_result.is_hit() {
            return None;
        }

        let element_path = hit_result.element_path();
        let ro_path = hit_result.path();

        for (&ro_key, &element_id) in ro_path.iter().zip(element_path.iter()).rev() {
            if let Some(ro) = render_objects.get(ro_key) {
                if ro.scroll_offset().is_some() {
                    let bounds = hit_result.bounds_for_element(element_id)
                        .unwrap_or_default();
                    let local_position = hit_result
                        .inner_bounds()
                        .map(|b| Point::new(position.x - b.position().x, position.y - b.position().y))
                        .unwrap_or(position);

                    if let Some(element) = element_registry.get_mut(element_id) {
                        let mut ctx = EventContext::with_build_owner(
                            element_id,
                            position,
                            local_position,
                            focus_manager.primary_focus_element(),
                            bounds,
                            modifiers,
                            scale_source.clone(),
                            font_system,
                            build_owner,
                            dirty_sender,
                            Some(render_objects),
                        );

                        return element.on_event(event, &mut ctx, state);
                    }
                }
            }
        }

        None
    }

    /// Hit test at a given position.
    pub fn hit_test(
        render_objects: &RenderObjectRegistry,
        position: Position<Logical, Absolute>,
    ) -> HitTestResult {
        render_objects.hit_test(position)
    }
}