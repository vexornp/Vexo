//! Input handling abstractions for the Vexo UI framework.
//!
//! This module provides platform-independent input events and context
//! for widget event handling.

mod event;

pub use event::{
    ButtonState,
    InputEvent,
    InteractionContext,
    InteractionResponse,
    FocusRequest,
    Key,
    Modifiers,
    NamedKey,
    PointerButton,
};
