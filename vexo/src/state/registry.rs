//! Centralized widget state registry.
//!
//! Combines all state management into a single registry for convenience.

use glyphon::FontSystem;

use crate::component::ComponentStateStorage;
use crate::core::WidgetId;
use crate::state::editor::{EditorRef, EditorState};
use crate::state::focus::FocusState;

/// Centralized state management for widgets.
///
/// This registry combines:
/// - Editor state (text input persistence)
/// - Focus state (keyboard focus tracking)
///
/// # Example
///
/// ```
/// use vexo::state::WidgetStateRegistry;
/// use glyphon::FontSystem;
/// use vexo::core::WidgetId;
///
/// let mut font_system = FontSystem::new();
/// let mut registry = WidgetStateRegistry::new();
///
/// // Manage editors
/// let editor = registry.get_or_create_editor("my-editor", "initial text", &mut font_system);
///
/// // Manage focus
/// let focused = registry.focused_widget();
/// registry.request_focus(WidgetId::from_key("my-editor"));
/// ```
pub struct WidgetStateRegistry {
    editor_state: EditorState,
    focus_state: FocusState,
    component_storage: ComponentStateStorage,
}

impl std::fmt::Debug for WidgetStateRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetStateRegistry")
            .field("editor_count", &self.editor_state.len())
            .field("focused_widget", &self.focus_state.focused())
            .field("component_state_count", &self.component_storage.len())
            .finish()
    }
}

impl Default for WidgetStateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetStateRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            editor_state: EditorState::new(),
            focus_state: FocusState::new(),
            component_storage: ComponentStateStorage::new(),
        }
    }

    // ========================================================================
    // Editor Management
    // ========================================================================

    /// Get or create an editor by ID.
    pub fn get_or_create_editor(
        &mut self,
        id: &str,
        initial_text: &str,
        font_system: &mut FontSystem,
    ) -> EditorRef {
        self.editor_state.get_or_create(id, initial_text, font_system)
    }

    /// Check if an editor exists.
    pub fn has_editor(&self, id: &str) -> bool {
        self.editor_state.contains(id)
    }

    /// Get an existing editor without creating one.
    pub fn get_editor(&self, id: &str) -> Option<EditorRef> {
        self.editor_state.get(id)
    }

    /// Remove an editor by ID.
    pub fn remove_editor(&mut self, id: &str) -> Option<EditorRef> {
        self.editor_state.remove(id)
    }

    /// Clear all editors.
    pub fn clear_editors(&mut self) {
        self.editor_state.clear();
    }

    /// Get the number of editors.
    pub fn editor_count(&self) -> usize {
        self.editor_state.len()
    }

    // ========================================================================
    // Focus Management
    // ========================================================================

    /// Get the currently focused widget.
    pub fn focused_widget(&self) -> Option<WidgetId> {
        self.focus_state.focused()
    }

    /// Check if a widget is focused.
    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.focus_state.is_focused(id)
    }

    /// Check if any widget is focused.
    pub fn has_focus(&self) -> bool {
        self.focus_state.has_focus()
    }

    /// Request focus for a widget.
    ///
    /// Returns the previously focused widget (if any).
    pub fn request_focus(&mut self, id: WidgetId) -> Option<WidgetId> {
        self.focus_state.request_focus(id)
    }

    /// Clear focus from the currently focused widget.
    ///
    /// Returns the widget that was focused (if any).
    pub fn clear_focus(&mut self) -> Option<WidgetId> {
        self.focus_state.clear_focus()
    }

    /// Set focus to a specific widget (or clear if None).
    pub fn set_focus(&mut self, id: Option<WidgetId>) {
        self.focus_state.set_focus(id);
    }

    // ========================================================================
    // Component State Management
    // ========================================================================

    /// Get the component state storage.
    pub fn component_storage(&mut self) -> &mut ComponentStateStorage {
        &mut self.component_storage
    }

    /// Get component state by key (convenience method).
    pub fn get_or_create_component_state<S: Default + 'static>(
        &mut self,
        key: &str,
    ) -> &mut S {
        self.component_storage.get_or_create(key)
    }

    /// Check if component state exists.
    pub fn has_component_state(&self, key: &str) -> bool {
        self.component_storage.contains(key)
    }

    /// Remove component state.
    pub fn remove_component_state(&mut self, key: &str) {
        self.component_storage.remove(key);
    }

    // ========================================================================
    // Bulk Operations
    // ========================================================================

    /// Clear all state (editors and focus).
    pub fn clear(&mut self) {
        self.editor_state.clear();
        self.focus_state.clear_focus();
        self.component_storage.clear();
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_font_system() -> FontSystem {
        FontSystem::new()
    }

    #[test]
    fn test_registry_editor_management() {
        let mut registry = WidgetStateRegistry::new();
        let mut font_system = create_test_font_system();

        assert!(!registry.has_editor("test"));

        let _editor = registry.get_or_create_editor("test", "hello", &mut font_system);
        assert!(registry.has_editor("test"));
        assert_eq!(registry.editor_count(), 1);
    }

    #[test]
    fn test_registry_focus_management() {
        let mut registry = WidgetStateRegistry::new();
        let id = WidgetId::from_key("test");

        assert!(!registry.has_focus());

        registry.request_focus(id);
        assert!(registry.is_focused(id));

        registry.clear_focus();
        assert!(!registry.has_focus());
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = WidgetStateRegistry::new();
        let mut font_system = create_test_font_system();
        let id = WidgetId::from_key("test");

        registry.get_or_create_editor("editor", "text", &mut font_system);
        registry.request_focus(id);

        registry.clear();

        assert!(!registry.has_editor("editor"));
        assert!(!registry.has_focus());
    }
}
