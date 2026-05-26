use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Key for widget identity across frames.
///
/// Widgets with the same key are considered "the same" across
/// reconciliation, enabling state preservation and efficient updates.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key(String);

impl Key {
    /// Create a new key from a string.
    pub fn new(s: impl Into<String>) -> Self {
        Key(s.into())
    }

    /// Get the key as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Key {
    fn from(s: &str) -> Self {
        Key(s.to_string())
    }
}

impl From<String> for Key {
    fn from(s: String) -> Self {
        Key(s)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for global element identity.
///
/// Unlike local Keys, GlobalKeys work across the entire element tree,
/// allowing elements to move between parents while preserving state.
///
/// # Example
///
/// ```ignore
/// let key = GlobalKey::new();
/// // Use in widget
/// let widget = MyWidget::new().with_key(key.clone());
///
/// // Later, access the element
/// if let Some(element_id) = key.current_element(&registry) {
///     // Access element state
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GlobalKey(usize);

impl GlobalKey {
    /// Create a new unique GlobalKey.
    ///
    /// Each call generates a unique identifier using an atomic counter.
    pub fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        GlobalKey(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Get the internal ID of this key.
    pub fn id(&self) -> usize {
        self.0
    }
}

impl Default for GlobalKey {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for GlobalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GlobalKey({})", self.0)
    }
}

/// Unified key type supporting both local and global keys.
///
/// This enum allows widgets to use either:
/// - `Local(Key)` - Key that only matches within a parent's children
/// - `Global(GlobalKey)` - Key that matches anywhere in the tree
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WidgetKey {
    /// Local key - only matches within parent's children.
    Local(Key),
    /// Global key - matches anywhere in the tree.
    Global(GlobalKey),
}

impl WidgetKey {
    /// Check if this is a local key.
    pub fn is_local(&self) -> bool {
        matches!(self, WidgetKey::Local(_))
    }

    /// Check if this is a global key.
    pub fn is_global(&self) -> bool {
        matches!(self, WidgetKey::Global(_))
    }

    /// Get the local key if this is a Local variant.
    pub fn as_local(&self) -> Option<&Key> {
        match self {
            WidgetKey::Local(k) => Some(k),
            WidgetKey::Global(_) => None,
        }
    }

    /// Get the global key if this is a Global variant.
    pub fn as_global(&self) -> Option<&GlobalKey> {
        match self {
            WidgetKey::Local(_) => None,
            WidgetKey::Global(k) => Some(k),
        }
    }
}

impl From<Key> for WidgetKey {
    fn from(key: Key) -> Self {
        WidgetKey::Local(key)
    }
}

impl From<GlobalKey> for WidgetKey {
    fn from(key: GlobalKey) -> Self {
        WidgetKey::Global(key)
    }
}

impl From<&str> for WidgetKey {
    fn from(s: &str) -> Self {
        WidgetKey::Local(Key::new(s))
    }
}

impl From<String> for WidgetKey {
    fn from(s: String) -> Self {
        WidgetKey::Local(Key::new(s))
    }
}

impl fmt::Display for WidgetKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WidgetKey::Local(k) => write!(f, "Local({})", k),
            WidgetKey::Global(k) => write!(f, "Global({})", k),
        }
    }
}