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
//!
//! let mut registry = WidgetStateRegistry::new();
//! let editor = registry.get_or_create_editor("my-editor", "initial text");
//! ```

mod editor;
mod focus;
mod registry;

pub use editor::EditorState;
pub use focus::FocusState;
pub use registry::WidgetStateRegistry;
