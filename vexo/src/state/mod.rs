//! State management for widgets.
//!
//! This module provides centralized state management for widgets that need
//! to persist state across frames, such as text editors and focus tracking.
//!
//! # Architecture
//!
//! State is separated from widget context to enable:
//! - Clear ownership of state
//! - Independent testing of state management
//! - Explicit state lifecycle
//!
//! # Example
//!
//! ```
//! use vexo::state::WidgetStateRegistry;
//! use glyphon::FontSystem;
//!
//! let mut font_system = FontSystem::new();
//! let mut registry = WidgetStateRegistry::new();
//! let editor = registry.get_or_create_editor("my-editor", "initial text", &mut font_system);
//! ```

pub mod cursor_blink;
pub mod editor;
mod focus;
mod registry;

pub use cursor_blink::CursorBlinkState;
pub use editor::EditorRef;
pub use focus::FocusState;
pub use registry::WidgetStateRegistry;
