//! Identifiable trait for widget identity.

use crate::core::WidgetId;

/// Trait for widgets that need stable identity across frames.
///
/// Widgets that participate in focus tracking, hover state, or need
/// persistent state must implement this trait to provide a stable ID.
///
/// # Example
///
/// ```
/// use vexo::widget::Identifiable;
/// use vexo::core::WidgetId;
///
/// struct MyWidget {
///     id: WidgetId,
/// }
///
/// impl Identifiable for MyWidget {
///     fn id(&self) -> Option<WidgetId> {
///         Some(self.id)
///     }
/// }
/// ```
pub trait Identifiable {
    /// Return the stable identifier for this widget.
    ///
    /// Returns `None` if the widget doesn't need identity tracking.
    /// Widgets without identity cannot receive focus or maintain hover state.
    fn id(&self) -> Option<WidgetId>;
}

// ============================================================================
// BLANKET IMPLEMENTATION
// ============================================================================

impl Identifiable for () {
    fn id(&self) -> Option<WidgetId> {
        None
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identifiable_unit() {
        let unit = ();
        assert!(unit.id().is_none());
    }

    #[test]
    fn test_widget_id_from_key() {
        let id1 = WidgetId::from_key("widget-1");
        let id2 = WidgetId::from_key("widget-1");
        let id3 = WidgetId::from_key("widget-2");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }
}
