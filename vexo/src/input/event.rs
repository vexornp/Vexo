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

use crate::core::{Point, Logical, Physical, Scale, ScaleSource};

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
        /// Position of the pointer in logical coordinates when the scroll occurred.
        position: Point<Logical>,
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
    Meta,
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
// WINIT CONVERSION
// ============================================================================

impl InputEvent {
    /// Convert a winit WindowEvent to an InputEvent.
    ///
    /// Returns None if the event is not relevant to widgets
    /// (e.g., surface resize, close requested).
    pub fn from_winit(
        event: &winit::event::WindowEvent,
        scale_source: &ScaleSource,
        pointer_position: Point<Logical>,
    ) -> Option<Self> {
        let scale = scale_source.get();
        use winit::event::ElementState;

        match event {
            WindowEvent::PointerMoved { position, .. } => {
                let physical = Point::<Physical>::new(position.x as f32, position.y as f32);
                let logical = physical.to_logical(scale);
                Some(InputEvent::PointerMoved {
                    position: logical,
                })
            }

            WindowEvent::PointerButton {
                state,
                button: _,
                position,
                ..
            } => {
                let physical = Point::<Physical>::new(position.x as f32, position.y as f32);
                let logical = physical.to_logical(scale);

                // For now, treat all pointer button events as primary button
                // The button field in winit 0.31 is a ButtonSource which is more complex
                let button_state = match state {
                    ElementState::Pressed => ButtonState::Pressed,
                    ElementState::Released => ButtonState::Released,
                };
                Some(InputEvent::PointerButton {
                    position: logical,
                    button: PointerButton::Primary,
                    state: button_state,
                })
            }

            WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::{Key, NamedKey as WinitNamedKey};

                let key = match &event.logical_key {
                    Key::Named(named) => {
                        let named_key = match named {
                            WinitNamedKey::ArrowUp => NamedKey::ArrowUp,
                            WinitNamedKey::ArrowDown => NamedKey::ArrowDown,
                            WinitNamedKey::ArrowLeft => NamedKey::ArrowLeft,
                            WinitNamedKey::ArrowRight => NamedKey::ArrowRight,
                            WinitNamedKey::Home => NamedKey::Home,
                            WinitNamedKey::End => NamedKey::End,
                            WinitNamedKey::PageUp => NamedKey::PageUp,
                            WinitNamedKey::PageDown => NamedKey::PageDown,
                            WinitNamedKey::Backspace => NamedKey::Backspace,
                            WinitNamedKey::Delete => NamedKey::Delete,
                            WinitNamedKey::Enter => NamedKey::Enter,
                            WinitNamedKey::Escape => NamedKey::Escape,
                            WinitNamedKey::Tab => NamedKey::Tab,
                            WinitNamedKey::F1 => NamedKey::F1,
                            WinitNamedKey::F2 => NamedKey::F2,
                            WinitNamedKey::F3 => NamedKey::F3,
                            WinitNamedKey::F4 => NamedKey::F4,
                            WinitNamedKey::F5 => NamedKey::F5,
                            WinitNamedKey::F6 => NamedKey::F6,
                            WinitNamedKey::F7 => NamedKey::F7,
                            WinitNamedKey::F8 => NamedKey::F8,
                            WinitNamedKey::F9 => NamedKey::F9,
                            WinitNamedKey::F10 => NamedKey::F10,
                            WinitNamedKey::F11 => NamedKey::F11,
                            WinitNamedKey::F12 => NamedKey::F12,
                            WinitNamedKey::Shift => NamedKey::Shift,
                            WinitNamedKey::Control => NamedKey::Control,
                            WinitNamedKey::Alt => NamedKey::Alt,
                            #[allow(deprecated)]
                            WinitNamedKey::Meta | WinitNamedKey::Super => NamedKey::Meta,
                            WinitNamedKey::CapsLock => NamedKey::CapsLock,
                            WinitNamedKey::NumLock => NamedKey::NumLock,
                            _ => return Some(InputEvent::Keyboard {
                                key: crate::input::Key::Unknown,
                                text: None,
                                state: ButtonState::Released,
                                modifiers: Modifiers::default(),
                            }),
                        };
                        crate::input::Key::Named(named_key)
                    }
                    Key::Character(ch) => crate::input::Key::Character(ch.to_string()),
                    _ => crate::input::Key::Unknown,
                };

                let state = match event.state {
                    ElementState::Pressed => ButtonState::Pressed,
                    ElementState::Released => ButtonState::Released,
                };

                let text = if event.state == ElementState::Pressed {
                    match &event.logical_key {
                        Key::Character(ch) => Some(ch.to_string()),
                        _ => None,
                    }
                } else {
                    None
                };

                Some(InputEvent::Keyboard {
                    key,
                    text,
                    state,
                    modifiers: Modifiers::default(), // Caller should fill this
                })
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        (*x * 20.0, *y * 20.0) // Approximate line height
                    }
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        (pos.x as f32, pos.y as f32)
                    }
                };
                Some(InputEvent::Scroll {
                    position: pointer_position,
                    delta: Point::new(dx, dy),
                })
            }

            WindowEvent::Focused(focused) => {
                Some(InputEvent::WindowFocus {
                    focused: *focused,
                })
            }

            WindowEvent::ModifiersChanged(modifiers) => {
                let mods = modifiers.state();
                Some(InputEvent::ModifiersChanged {
                    modifiers: Modifiers {
                        shift: mods.shift_key(),
                        control: mods.control_key(),
                        alt: mods.alt_key(),
                        super_key: false, // winit 0.31 doesn't expose this directly
                    },
                })
            }

            _ => None,
        }
    }

    /// Set the pointer position for pointer events.
    ///
    /// This is used when converting from winit events that don't include
    /// position information.
    pub fn with_position(self, position: Point<Logical>) -> Self {
        match self {
            InputEvent::PointerButton { button, state, .. } => {
                InputEvent::PointerButton {
                    position,
                    button,
                    state,
                }
            }
            _ => self,
        }
    }

    /// Set the modifiers for keyboard events.
    pub fn with_modifiers(self, modifiers: Modifiers) -> Self {
        match self {
            InputEvent::Keyboard { key, text, state, .. } => {
                InputEvent::Keyboard {
                    key,
                    text,
                    state,
                    modifiers,
                }
            }
            _ => self,
        }
    }
}

use winit::event::WindowEvent;

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
}
