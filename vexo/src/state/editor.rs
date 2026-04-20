//! Editor state management.
//!
//! Provides persistent editor state for text input widgets.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glyphon::{Buffer, Edit, Metrics, Shaping, Attrs};

use crate::editor::Editor;

/// Reference-counted editor handle.
pub type EditorRef = Rc<RefCell<Editor>>;

/// Manages persistent editor instances.
///
/// Editors are created on-demand and stored by ID. This allows
/// text widgets to maintain their content across frame rebuilds.
pub struct EditorState {
    editors: HashMap<String, EditorRef>,
    default_font_size: f32,
}

impl std::fmt::Debug for EditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorState")
            .field("editor_count", &self.editors.len())
            .field("default_font_size", &self.default_font_size)
            .finish()
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorState {
    /// Create a new editor state manager.
    pub fn new() -> Self {
        Self {
            editors: HashMap::new(),
            default_font_size: 16.0,
        }
    }

    /// Create with a custom default font size.
    pub fn with_font_size(font_size: f32) -> Self {
        Self {
            editors: HashMap::new(),
            default_font_size: font_size,
        }
    }

    /// Get or create an editor by ID.
    ///
    /// If the editor doesn't exist, it's created with the initial text.
    /// If it exists, the initial text is ignored.
    pub fn get_or_create(
        &mut self,
        id: &str,
        initial_text: &str,
        font_system: &mut glyphon::FontSystem,
    ) -> EditorRef {
        self.editors
            .entry(id.to_string())
            .or_insert_with(|| {
                let metrics = Metrics::new(self.default_font_size, self.default_font_size * 1.25);
                let mut raw_editor = glyphon::Editor::new(Buffer::new_empty(metrics));
                raw_editor.with_buffer_mut(|buffer| {
                    buffer.set_text(font_system, initial_text, &Attrs::new(), Shaping::Advanced);
                });
                raw_editor.with_buffer_mut(|buffer| {
                    buffer.shape_until_scroll(font_system, true);
                });
                Rc::new(RefCell::new(Editor::new(raw_editor)))
            })
            .clone()
    }

    /// Check if an editor exists.
    pub fn contains(&self, id: &str) -> bool {
        self.editors.contains_key(id)
    }

    /// Get an existing editor without creating one.
    pub fn get(&self, id: &str) -> Option<EditorRef> {
        self.editors.get(id).cloned()
    }

    /// Remove an editor by ID.
    ///
    /// Returns the editor if it existed.
    pub fn remove(&mut self, id: &str) -> Option<EditorRef> {
        self.editors.remove(id)
    }

    /// Clear all editors.
    pub fn clear(&mut self) {
        self.editors.clear();
    }

    /// Get the number of editors.
    pub fn len(&self) -> usize {
        self.editors.len()
    }

    /// Check if there are no editors.
    pub fn is_empty(&self) -> bool {
        self.editors.is_empty()
    }

    /// Get all editor IDs.
    pub fn ids(&self) -> impl Iterator<Item = &String> {
        self.editors.keys()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use glyphon::FontSystem;

    fn create_test_font_system() -> FontSystem {
        FontSystem::new()
    }

    #[test]
    fn test_editor_state_create() {
        let mut state = EditorState::new();
        let mut font_system = create_test_font_system();

        assert!(!state.contains("test"));
        assert!(state.is_empty());

        let _editor = state.get_or_create("test", "hello", &mut font_system);
        assert!(state.contains("test"));
        assert_eq!(state.len(), 1);

        // Getting again returns the same editor
        let _editor2 = state.get_or_create("test", "ignored", &mut font_system);
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn test_editor_state_get() {
        let mut state = EditorState::new();
        let mut font_system = create_test_font_system();

        assert!(state.get("missing").is_none());

        state.get_or_create("test", "hello", &mut font_system);
        assert!(state.get("test").is_some());
    }

    #[test]
    fn test_editor_state_remove() {
        let mut state = EditorState::new();
        let mut font_system = create_test_font_system();

        state.get_or_create("test", "hello", &mut font_system);
        assert!(state.remove("test").is_some());
        assert!(!state.contains("test"));
        assert!(state.remove("test").is_none());
    }

    #[test]
    fn test_editor_state_clear() {
        let mut state = EditorState::new();
        let mut font_system = create_test_font_system();

        state.get_or_create("a", "1", &mut font_system);
        state.get_or_create("b", "2", &mut font_system);
        assert_eq!(state.len(), 2);

        state.clear();
        assert!(state.is_empty());
    }

    #[test]
    fn test_editor_state_custom_font_size() {
        let state = EditorState::with_font_size(24.0);
        assert_eq!(state.default_font_size, 24.0);
    }
}
