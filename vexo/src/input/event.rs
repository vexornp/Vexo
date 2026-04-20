//! Input event abstraction for the Vexo UI framework.
//!
//! This module provides platform-independent input events that decouple
//! widget event handling from winit. This enables testing input handling
//! without requiring a windowing system.
//!
//! # Design Goals
//!
//! - Decouple widgets from winit
//! - Enable testing of input handling
//! - Provide a clean, minimal event model

use crate::core::{Point, Logical};

// ============================================================================
// INPUT EVENT
// ============================================================================

/// Platform-independent input event.
///
/// These events abstract away the details of the underlying windowing system
/// and provide a clean model for widget event handling.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// Pointer (mouse/touch) moved.
    PointerMoved {
        /// Position in logical coordinates.
        position: Point<Logical>,
    },

    /// Pointer button pressed or released.
    PointerButton {
        /// Position in logical coordinates.
        position: Point<Logical>,
        /// Which button was pressed.
        button: PointerButton,
        /// Whether the button was pressed or released.
        state: ButtonState,
    },

    /// Keyboard input.
    Keyboard {
        /// The key that was pressed or released.
        key: Key,
        /// Text input (if any) associated with the key press.
        text: Option<String>,
        /// Whether the key was pressed or released.
        state: ButtonState,
        /// Modifier keys held during the event.
        modifiers: Modifiers,
    },

    /// Scroll wheel input.
    Scroll {
        /// Scroll delta in logical coordinates.
        delta: Point<Logical>,
    },

    /// Window gained or lost focus.
    WindowFocus {
        /// Whether the window is focused.
        focused: bool,
    },

    /// Modifiers changed (Ctrl, Shift, Alt, etc.).
    ModifiersChanged {
        /// The new modifier state.
        modifiers: Modifiers,
    },
}

// ============================================================================
// POINTER BUTTON
// ============================================================================

/// Mouse/pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerButton {
    /// Primary button (left mouse button).
    Primary,
    /// Secondary button (right mouse button).
    Secondary,
    /// Tertiary button (middle mouse button).
    Tertiary,
}

// ============================================================================
// BUTTON STATE
// ============================================================================

/// State of a button (pressed or released).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonState {
    /// Button was pressed down.
    Pressed,
    /// Button was released.
    Released,
}

impl ButtonState {
    /// Returns true if the button is pressed.
    pub fn is_pressed(&self) -> bool {
        matches!(self, ButtonState::Pressed)
    }

    /// Returns true if the button is released.
    pub fn is_released(&self) -> bool {
        matches!(self, ButtonState::Released)
    }
}

// ============================================================================
// KEY
// ============================================================================

/// Keyboard key representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// A named key (arrows, function keys, etc.).
    Named(NamedKey),
    /// A character key.
    Character(String),
    /// An unknown key.
    Unknown,
}

/// Named keyboard keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    // Arrow keys
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Navigation keys
    Home,
    End,
    PageUp,
    PageDown,

    // Editing keys
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,

    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Modifier keys
    Shift,
    Control,
    Alt,
    Super,
    CapsLock,
    NumLock,
}

// ============================================================================
// MODIFIERS
// ============================================================================

/// Keyboard modifier state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Shift key held.
    pub shift: bool,
    /// Control key held.
    pub control: bool,
    /// Alt key held.
    pub alt: bool,
    /// Super/Command/Windows key held.
    pub super_key: bool,
}

impl Modifiers {
    /// Create a new modifier state with all modifiers released.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a modifier state with shift held.
    pub fn shift() -> Self {
        Self { shift: true, ..Self::default() }
    }

    /// Create a modifier state with control held.
    pub fn control() -> Self {
        Self { control: true, ..Self::default() }
    }

    /// Create a modifier state with alt held.
    pub fn alt() -> Self {
        Self { alt: true, ..Self::default() }
    }

    /// Check if any modifier is held.
    pub fn any(&self) -> bool {
        self.shift || self.control || self.alt || self.super_key
    }

    /// Check if no modifiers are held.
    pub fn none(&self) -> bool {
        !self.any()
    }
}

// ============================================================================
// INTERACTION CONTEXT
// ============================================================================

/// Context provided to widgets during event handling.
#[derive(Debug, Clone)]
pub struct InteractionContext {
    /// Current pointer position in logical coordinates.
    pub pointer_position: Point<Logical>,
    /// Currently focused widget (if any).
    pub focused_widget: Option<crate::core::WidgetId>,
    /// Bounds of the widget receiving the event.
    pub bounds: crate::core::Rect<Logical>,
    /// Current DPI scale factor.
    pub scale: f32,
}

impl InteractionContext {
    /// Create a new interaction context.
    pub fn new(
        pointer_position: Point<Logical>,
        focused_widget: Option<crate::core::WidgetId>,
        bounds: crate::core::Rect<Logical>,
        scale: f32,
    ) -> Self {
        Self {
            pointer_position,
            focused_widget,
            bounds,
            scale,
        }
    }

    /// Check if the pointer is inside the widget bounds.
    pub fn is_pointer_inside(&self) -> bool {
        self.bounds.contains(&self.pointer_position)
    }

    /// Check if this widget is currently focused.
    pub fn is_focused(&self, id: crate::core::WidgetId) -> bool {
        self.focused_widget == Some(id)
    }
}

impl Default for InteractionContext {
    fn default() -> Self {
        Self {
            pointer_position: Point::new(0.0, 0.0),
            focused_widget: None,
            bounds: crate::core::Rect::from_xywh(0.0, 0.0, 0.0, 0.0),
            scale: 1.0,
        }
    }
}

// ============================================================================
// INTERACTION RESPONSE
// ============================================================================

/// Response from widget event handling.
#[derive(Debug)]
pub struct InteractionResponse<M> {
    /// User-defined message to emit.
    pub message: Option<M>,
    /// Focus change request.
    pub focus_request: Option<FocusRequest>,
    /// Whether the event was consumed.
    pub handled: bool,
    /// Whether to clear focus from the currently focused widget.
    pub clear_focus: bool,
}

/// Focus change request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRequest {
    /// Request focus for a specific widget.
    Gain(crate::core::WidgetId),
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
    pub fn request_focus(id: crate::core::WidgetId) -> Self {
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

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_state() {
        assert!(ButtonState::Pressed.is_pressed());
        assert!(!ButtonState::Pressed.is_released());
        assert!(!ButtonState::Released.is_pressed());
        assert!(ButtonState::Released.is_released());
    }

    #[test]
    fn test_modifiers() {
        let m = Modifiers::default();
        assert!(m.none());
        assert!(!m.any());

        let m = Modifiers::shift();
        assert!(m.shift);
        assert!(m.any());
        assert!(!m.none());

        let m = Modifiers::control();
        assert!(m.control);
    }

    #[test]
    fn test_interaction_context_pointer_inside() {
        let ctx = InteractionContext::new(
            Point::new(50.0, 50.0),
            None,
            crate::core::Rect::from_xywh(0.0, 0.0, 100.0, 100.0),
            1.0,
        );
        assert!(ctx.is_pointer_inside());

        let ctx = InteractionContext::new(
            Point::new(150.0, 50.0),
            None,
            crate::core::Rect::from_xywh(0.0, 0.0, 100.0, 100.0),
            1.0,
        );
        assert!(!ctx.is_pointer_inside());
    }

    #[test]
    fn test_interaction_response() {
        let r: InteractionResponse<()> = InteractionResponse::default();
        assert!(!r.handled);
        assert!(r.message.is_none());

        let r: InteractionResponse<()> = InteractionResponse::handled();
        assert!(r.handled);

        let r = InteractionResponse::with_message("test");
        assert_eq!(r.message, Some("test"));
        assert!(r.handled);
    }
}
