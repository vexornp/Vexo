use std::fmt;

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