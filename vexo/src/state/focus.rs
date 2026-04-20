//! Focus state management.
//!
//! Provides focus tracking for widgets that can receive keyboard input.

use crate::core::WidgetId;

/// Manages which widget currently has focus.
///
/// Only one widget can be focused at a time. Focus is used to
/// route keyboard input to the appropriate widget.
#[derive(Debug, Clone)]
pub struct FocusState {
    focused_widget: Option<WidgetId>,
}

impl Default for FocusState {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusState {
    /// Create a new focus state with no widget focused.
    pub fn new() -> Self {
        Self {
            focused_widget: None,
        }
    }

    /// Get the currently focused widget.
    pub fn focused(&self) -> Option<WidgetId> {
        self.focused_widget
    }

    /// Check if a specific widget is focused.
    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.focused_widget == Some(id)
    }

    /// Check if any widget is focused.
    pub fn has_focus(&self) -> bool {
        self.focused_widget.is_some()
    }

    /// Request focus for a widget.
    ///
    /// Returns the previously focused widget (if any).
    pub fn request_focus(&mut self, id: WidgetId) -> Option<WidgetId> {
        let previous = self.focused_widget;
        self.focused_widget = Some(id);
        previous
    }

    /// Clear focus from the currently focused widget.
    ///
    /// Returns the widget that was focused (if any).
    pub fn clear_focus(&mut self) -> Option<WidgetId> {
        let previous = self.focused_widget;
        self.focused_widget = None;
        previous
    }

    /// Set focus to a specific widget (or clear if None).
    pub fn set_focus(&mut self, id: Option<WidgetId>) {
        self.focused_widget = id;
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_state_default() {
        let state = FocusState::default();
        assert!(!state.has_focus());
        assert!(state.focused().is_none());
    }

    #[test]
    fn test_focus_state_request() {
        let mut state = FocusState::new();
        let id = WidgetId::from_key("test");

        let previous = state.request_focus(id);
        assert!(previous.is_none());
        assert!(state.is_focused(id));
        assert!(state.has_focus());
    }

    #[test]
    fn test_focus_state_clear() {
        let mut state = FocusState::new();
        let id = WidgetId::from_key("test");

        state.request_focus(id);
        let cleared = state.clear_focus();

        assert_eq!(cleared, Some(id));
        assert!(!state.has_focus());
    }

    #[test]
    fn test_focus_state_replace() {
        let mut state = FocusState::new();
        let id1 = WidgetId::from_key("first");
        let id2 = WidgetId::from_key("second");

        state.request_focus(id1);
        let previous = state.request_focus(id2);

        assert_eq!(previous, Some(id1));
        assert!(state.is_focused(id2));
        assert!(!state.is_focused(id1));
    }

    #[test]
    fn test_focus_state_set() {
        let mut state = FocusState::new();
        let id = WidgetId::from_key("test");

        state.set_focus(Some(id));
        assert!(state.is_focused(id));

        state.set_focus(None);
        assert!(!state.has_focus());
    }
}
