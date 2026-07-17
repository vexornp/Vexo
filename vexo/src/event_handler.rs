//! Event handling for the retain-mode three-tree pipeline.
//!
//! Provides the `EventHandler` struct that holds event-related logic
//! extracted from `ThreeTreePipeline`. This is a zero-sized struct
//! used as a namespace for associated functions.

use std::any::Any;
use std::sync::{mpsc, Arc};

use crate::core::{Absolute, Bounds, Logical, Point, Position, ScaleSource};
use crate::input::{ButtonState, InputEvent, Modifiers};
use crate::platform::Clipboard;

use super::build_owner::BuildOwner;
use super::element::ElementRegistry;
use super::element_state::StateStorage;
use super::event_context::EventContext;
use super::focus::FocusManager;
use super::hit_test::HitTestResult;
use super::id::ElementKey;
use super::render_object::RenderObjectRegistry;
use crate::gestures::{ArenaEvent, ArenaOutcome, GestureArena};

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
        current_arena: &mut Option<GestureArena>,
        _position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
        scale_source: &ScaleSource,
        clipboard: &Arc<dyn Clipboard>,
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
                current_arena,
                *position,
                event,
                modifiers,
                scale_source,
                clipboard,
            ),
            InputEvent::PointerButton { position, .. } => Self::handle_pointer_event(
                element_registry,
                render_objects,
                state,
                font_system,
                build_owner,
                dirty_sender,
                focus_manager,
                current_arena,
                *position,
                event,
                modifiers,
                scale_source,
                clipboard,
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
                clipboard,
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
                clipboard,
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
        current_arena: &mut Option<GestureArena>,
        position: Point<Logical>,
        event: &InputEvent,
        modifiers: Modifiers,
        scale_source: &ScaleSource,
        clipboard: &Arc<dyn Clipboard>,
    ) -> Option<Box<dyn Any>> {
        let absolute_position = Position::<Logical, Absolute>::new(position.x, position.y);
        let hit_result = render_objects.hit_test(absolute_position);

        if !hit_result.is_hit() {
            if let InputEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } = event
            {
                focus_manager.unfocus();
            }
            return None;
        }

        let local_position = hit_result
            .inner_bounds()
            .map(|b| Point::new(position.x - b.position().x, position.y - b.position().y))
            .unwrap_or(position);

        let element_path = hit_result.element_path();

        // Determine if this is a press, move, or release.
        let is_press = matches!(
            event,
            InputEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            }
        );
        let is_release = matches!(
            event,
            InputEvent::PointerButton {
                state: ButtonState::Released,
                ..
            }
        );
        let is_move = matches!(event, InputEvent::PointerMoved { .. });

        // === PRESS: create arena, register gestures, feed Down, then bubble press ===
        if is_press {
            // Defensive: if a stale arena exists (e.g. window blurred mid-press),
            // drop it and start fresh.
            *current_arena = Some(GestureArena::new(position));

            if let Some(arena) = current_arena.as_mut() {
                // Walk deepest→shallowest so deepest recognizer is at index 0.
                for &element_id in element_path.iter().rev() {
                    if let Some(element) = element_registry.get_mut(element_id) {
                        element.register_gestures(arena, element_id);
                    }
                }
                // Feed Down.
                arena.handle_event(ArenaEvent::Down { position });
            }
        }

        // === MOVE: feed Move to arena; if drag won, call winner (no bubble);
        //     if still open, bubble for MouseRegion hover ===
        if is_move {
            if let Some(arena) = current_arena.as_mut() {
                // Detect the Open→Resolved transition: the arena was open
                // before this Move and is now closed with a winner. This is
                // the FIRST winning Move — the element needs a Down call to
                // initialize its drag state (e.g. ScrollViewElement sets
                // last_drag_y from the recognizer's down_position). On
                // subsequent Moves the arena is already closed; the
                // recognizer is no longer fed, so we must NOT call Down
                // (that would reset last_drag_y each time) — Move only.
                let was_closed = arena.is_closed();
                let outcome = arena.handle_event(ArenaEvent::Move { position });
                if let ArenaOutcome::Resolved { winner_index: _ } = outcome {
                    if let Some(winner_id) = arena.winner_owner() {
                        let bounds = hit_result.bounds_for_element(winner_id).unwrap_or_default();
                        if let Some(element) = element_registry.get_mut(winner_id) {
                            let mut ctx = EventContext::with_build_owner(
                                winner_id,
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
                                clipboard.clone(),
                            );
                            let winner_recognizer = arena.winner_recognizer().unwrap();
                            if !was_closed {
                                // First winning move — initialize element
                                // state from Down, then apply the Move.
                                element.on_arena_winner_update(
                                    winner_recognizer,
                                    &ArenaEvent::Down { position },
                                    &mut ctx,
                                );
                            }
                            element.on_arena_winner_update(
                                winner_recognizer,
                                &ArenaEvent::Move { position },
                                &mut ctx,
                            );
                        }
                        // Drag owns the pointer — do NOT bubble.
                        return Some(Box::new(()));
                    }
                }
                // Arena still open — fall through to bubble (MouseRegion hover).
            }
        }

        // === RELEASE: feed Up + sweep; if tap won, call winner + bubble release;
        //     if drag won, call winner (no bubble); drop arena ===
        if is_release {
            let mut drag_won = false;
            if let Some(arena) = current_arena.as_mut() {
                arena.handle_event(ArenaEvent::Up { position });
                arena.sweep_on_up();
                if let Some(winner_id) = arena.winner_owner() {
                    let bounds = hit_result.bounds_for_element(winner_id).unwrap_or_default();
                    // Check if the winner is a drag (not a tap) — drag consumes release.
                    let is_drag_winner = arena
                        .winner_recognizer()
                        .map(|r| {
                            r.as_any()
                                .downcast_ref::<crate::gestures::VerticalDragRecognizer>()
                                .is_some()
                        })
                        .unwrap_or(false);
                    drag_won = is_drag_winner;

                    if let Some(element) = element_registry.get_mut(winner_id) {
                        let mut ctx = EventContext::with_build_owner(
                            winner_id,
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
                            clipboard.clone(),
                        );
                        let winner_recognizer = arena.winner_recognizer().unwrap();
                        element.on_arena_winner_update(
                            winner_recognizer,
                            &ArenaEvent::Up { position },
                            &mut ctx,
                        );
                    }
                }
            }
            // Drop the arena — gesture sequence complete.
            *current_arena = None;

            if drag_won {
                // Drag consumed the release — do NOT bubble (on_release won't fire).
                return Some(Box::new(()));
            }
            // Tap won (or no arena) — fall through to bubble release so
            // on_release fires (release feedback).
        }

        // === BUBBLE: deepest→shallowest, first handler stops propagation ===
        let mut any_message: Option<Box<dyn Any>> = None;
        for &element_id in element_path.iter().rev() {
            if let Some(element) = element_registry.get_mut(element_id) {
                let bounds = hit_result
                    .bounds_for_element(element_id)
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
                    clipboard.clone(),
                );
                let message = element.on_event(event, &mut ctx, state);
                if let Some(focus_element) = ctx.focus_request() {
                    let node_id = focus_manager
                        .node_for_element(focus_element)
                        .expect("Focus node must exist");
                    focus_manager.request_focus(node_id);
                } else if ctx.should_clear_focus() {
                    focus_manager.unfocus();
                }
                if message.is_some() {
                    any_message = message;
                    break;
                }
            }
        }

        if any_message.is_none() {
            if is_press {
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
        clipboard: &Arc<dyn Clipboard>,
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
            clipboard.clone(),
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
        clipboard: &Arc<dyn Clipboard>,
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
                    let bounds = hit_result
                        .bounds_for_element(element_id)
                        .unwrap_or_default();
                    let local_position = hit_result
                        .inner_bounds()
                        .map(|b| {
                            Point::new(position.x - b.position().x, position.y - b.position().y)
                        })
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
                            clipboard.clone(),
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
