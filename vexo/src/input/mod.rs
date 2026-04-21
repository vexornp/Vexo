//! Input handling abstractions for the Vexo UI framework.
//!
//! This module provides platform-independent input events and context
//! for widget event handling. The key abstraction is `InputEvent`, which
//! decouples widget logic from platform-specific event types (winit, iOS, etc.).
//!
//! # Architecture
//!
//! Input events flow through the system as follows:
//! 1. Platform (winit/iOS) generates native events
//! 2. Events are converted to `InputEvent` via `InputEvent::from_winit()`
//! 3. Widgets receive `InputEvent` in `on_event()` method
//! 4. Widgets return `WidgetResponse<M>` with messages/state changes
//!
//! # Example
//!
//! ```
//! use vexo::input::{InputEvent, ButtonState, Key};
//!
//! fn handle_input(event: &InputEvent) {
//!     match event {
//!         InputEvent::PointerButton { position, state, .. } => {
//!             if *state == ButtonState::Pressed {
//!                 println!("Clicked at ({}, {})", position.x, position.y);
//!             }
//!         }
//!         InputEvent::Keyboard { key, text, .. } => {
//!             if let Key::Character(ch) = key {
//!                 println!("Typed: {}", ch);
//!             }
//!         }
//!         _ => {}
//!     }
//! }
//! ```

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
