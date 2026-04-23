//! Interact trait for widget input handling.

use crate::core::{Point, Rect, Scale, WidgetId, Logical};
use crate::input::{CursorIcon, InputEvent, Modifiers};

/// Context provided to widgets during event handling.
#[derive(Debug, Clone)]
pub struct InteractionContext {
    /// Current pointer position in logical coordinates.
    pub pointer_position: Point<Logical>,
    /// Currently focused widget (if any).
    pub focused_widget: Option<WidgetId>,
    /// Bounds of the widget receiving the event.
    pub bounds: Rect<Logical>,
    /// Current DPI scale factor.
    pub scale: Scale,
    /// Current keyboard modifiers.
    pub modifiers: Modifiers,
}

impl Default for InteractionContext {
    fn default() -> Self {
        Self {
            pointer_position: Point::new(0.0, 0.0),
            focused_widget: None,
            bounds: Rect::from_xywh(0.0, 0.0, 0.0, 0.0),
            scale: Scale::default(),
            modifiers: Modifiers::default(),
        }
    }
}

impl InteractionContext {
    /// Create a new interaction context.
    pub fn new(
        pointer_position: Point<Logical>,
        focused_widget: Option<WidgetId>,
        bounds: Rect<Logical>,
        scale: Scale,
    ) -> Self {
        Self {
            pointer_position,
            focused_widget,
            bounds,
            scale,
            modifiers: Modifiers::default(),
        }
    }

    /// Check if the pointer is inside the widget bounds.
    pub fn is_pointer_inside(&self) -> bool {
        self.bounds.contains(&self.pointer_position)
    }

    /// Check if this widget is currently focused.
    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.focused_widget == Some(id)
    }

    /// Check if the control key is pressed.
    pub fn is_control_pressed(&self) -> bool {
        self.modifiers.control
    }

    /// Check if the shift key is pressed.
    pub fn is_shift_pressed(&self) -> bool {
        self.modifiers.shift
    }
}

/// Response from widget event handling.
#[derive(Debug)]
pub struct InteractionResponse<M> {
    /// User-defined message to emit.
    pub message: Option<M>,
    /// Request to change focus.
    pub focus_request: Option<FocusRequest>,
    /// Whether the event was consumed.
    pub handled: bool,
    /// Whether to clear focus from the currently focused widget.
    pub clear_focus: bool,
    /// Request to change the mouse cursor.
    pub cursor: Option<CursorIcon>,
}

/// Focus change request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRequest {
    /// Request focus for a specific widget.
    Gain(WidgetId),
    /// Clear focus from the currently focused widget.
    Clear,
}

impl<M> Default for InteractionResponse<M> {
    fn default() -> Self {
        Self {
            message: None,
            focus_request: None,
            handled: false,
            clear_focus: false,
            cursor: None,
        }
    }
}

impl<M> InteractionResponse<M> {
    /// Create a response indicating the event was not handled.
    pub fn ignored() -> Self {
        Self::default()
    }

    /// Create a response indicating the event was handled.
    pub fn handled() -> Self {
        Self {
            handled: true,
            ..Self::default()
        }
    }

    /// Create a response with a user message.
    pub fn with_message(message: M) -> Self {
        Self {
            message: Some(message),
            handled: true,
            ..Self::default()
        }
    }

    /// Create a response requesting focus.
    pub fn request_focus(id: WidgetId) -> Self {
        Self {
            focus_request: Some(FocusRequest::Gain(id)),
            handled: true,
            ..Self::default()
        }
    }

    /// Create a response requesting focus to be cleared.
    pub fn clear_focus() -> Self {
        Self {
            focus_request: Some(FocusRequest::Clear),
            handled: true,
            ..Self::default()
        }
    }
}

/// Trait for widgets that handle input events.
///
/// Widgets implement `on_event` to process user input like pointer events,
/// keyboard input, and scroll events.
///
/// # Example
///
/// ```
/// use vexo::testable::{Interact, InteractionContext, InteractionResponse};
/// use vexo::input::{InputEvent, ButtonState};
///
/// #[derive(Clone, Debug)]
/// enum Message { Clicked }
///
/// struct Clickable;
///
/// impl Interact<Message> for Clickable {
///     fn on_event(
///         &mut self,
///         event: &InputEvent,
///         interaction_context: &InteractionContext,
///     ) -> InteractionResponse<Message> {
///         match event {
///             InputEvent::PointerButton { state, .. } if *state == ButtonState::Pressed => {
///                 if interaction_context.is_pointer_inside() {
///                     InteractionResponse::with_message(Message::Clicked)
///                 } else {
///                     InteractionResponse::default()
///                 }
///             }
///             _ => InteractionResponse::default(),
///         }
///     }
/// }
/// ```
pub trait Interact<M: Clone + std::fmt::Debug + Send> {
    /// Handle an input event.
    ///
    /// Returns an `InteractionResponse` indicating whether the event was
    /// handled, any messages to emit, and any focus changes.
    fn on_event(
        &mut self,
        event: &InputEvent,
        interaction_context: &InteractionContext,
    ) -> InteractionResponse<M>;
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interaction_context_pointer_inside() {
        let ctx = InteractionContext::new(
            Point::new(50.0, 50.0),
            None,
            Rect::from_xywh(0.0, 0.0, 100.0, 100.0),
            Scale::new(1.0),
        );
        assert!(ctx.is_pointer_inside());

        let ctx = InteractionContext::new(
            Point::new(150.0, 50.0),
            None,
            Rect::from_xywh(0.0, 0.0, 100.0, 100.0),
            Scale::new(1.0),
        );
        assert!(!ctx.is_pointer_inside());
    }

    #[test]
    fn test_interaction_context_focus() {
        let id = WidgetId::from_key("test");
        let ctx = InteractionContext::new(
            Point::new(0.0, 0.0),
            Some(id),
            Rect::from_xywh(0.0, 0.0, 100.0, 100.0),
            Scale::new(1.0),
        );

        assert!(ctx.is_focused(id));
        assert!(!ctx.is_focused(WidgetId::from_key("other")));
    }

    #[test]
    fn test_interaction_response() {
        let r: InteractionResponse<()> = InteractionResponse::default();
        assert!(!r.handled);

        let r = InteractionResponse::<()>::handled();
        assert!(r.handled);

        let r = InteractionResponse::with_message("test");
        assert_eq!(r.message, Some("test"));
        assert!(r.handled);
    }

    #[test]
    fn test_focus_request() {
        let id = WidgetId::from_key("test");
        let r: InteractionResponse<()> = InteractionResponse::request_focus(id);

        assert!(matches!(r.focus_request, Some(FocusRequest::Gain(_))));
        assert!(r.handled);
    }
}
