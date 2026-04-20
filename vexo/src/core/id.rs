//! Widget identity types.
//!
//! Widgets that need stable identity across frames (for focus tracking,
//! hover state, etc.) use a `WidgetId` derived from a stable key.

use std::hash::{Hash, Hasher};

/// Unique identifier for a widget instance.
///
/// WidgetIds are deterministically derived from stable key strings,
/// allowing widgets to maintain identity across tree rebuilds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u64);

impl WidgetId {
    /// Create a WidgetId deterministically from a stable `key` string.
    ///
    /// The same key will always produce the same WidgetId, which is
    /// essential for maintaining widget identity across frame rebuilds.
    pub fn from_key(key: &str) -> Self {
        let mut s = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut s);
        WidgetId(s.finish())
    }

    /// Create a WidgetId from a raw u64 value.
    ///
    /// This is useful for testing or when you have a pre-computed ID.
    pub const fn from_raw(id: u64) -> Self {
        WidgetId(id)
    }

    /// Get the raw u64 value of this WidgetId.
    pub const fn as_raw(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widget_id_deterministic() {
        let id1 = WidgetId::from_key("my-widget");
        let id2 = WidgetId::from_key("my-widget");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_widget_id_different_keys() {
        let id1 = WidgetId::from_key("widget-1");
        let id2 = WidgetId::from_key("widget-2");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_widget_id_from_raw() {
        let id = WidgetId::from_raw(12345);
        assert_eq!(id.as_raw(), 12345);
    }
}
