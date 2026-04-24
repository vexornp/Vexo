//! Component state storage.
//! This module will be implemented in a future task.

/// Storage for component state (placeholder).
pub struct ComponentStateStorage {
    _inner: (),
}

impl ComponentStateStorage {
    pub fn new() -> Self {
        Self { _inner: () }
    }
}

impl Default for ComponentStateStorage {
    fn default() -> Self {
        Self::new()
    }
}
