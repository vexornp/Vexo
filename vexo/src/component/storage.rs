//! Type-erased storage for component state.

use std::any::Any;
use std::collections::HashMap;

/// Storage for component state, keyed by scoped string ID.
pub struct ComponentStateStorage {
    states: HashMap<String, Box<dyn Any>>,
}

impl Default for ComponentStateStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentStateStorage {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    pub fn get_or_create<S: Default + 'static>(&mut self, key: &str) -> &mut S {
        self.states
            .entry(key.to_string())
            .or_insert_with(|| Box::new(S::default()))
            .downcast_mut::<S>()
            .expect("State type mismatch - same key used with different types")
    }

    pub fn contains(&self, key: &str) -> bool {
        self.states.contains_key(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<Box<dyn Any>> {
        self.states.remove(key)
    }

    pub fn clear(&mut self) {
        self.states.clear();
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_create() {
        let mut storage = ComponentStateStorage::new();
        let state = storage.get_or_create::<i32>("counter");
        assert_eq!(*state, 0);
    }

    #[test]
    fn test_storage_persist() {
        let mut storage = ComponentStateStorage::new();
        *storage.get_or_create::<i32>("counter") = 42;
        let state = storage.get_or_create::<i32>("counter");
        assert_eq!(*state, 42);
    }

    #[test]
    fn test_storage_multiple_keys() {
        let mut storage = ComponentStateStorage::new();
        *storage.get_or_create::<i32>("a") = 1;
        *storage.get_or_create::<i32>("b") = 2;
        assert_eq!(*storage.get_or_create::<i32>("a"), 1);
        assert_eq!(*storage.get_or_create::<i32>("b"), 2);
    }

    #[test]
    fn test_storage_different_types() {
        let mut storage = ComponentStateStorage::new();
        *storage.get_or_create::<i32>("int") = 42;
        *storage.get_or_create::<String>("string") = "hello".to_string();
        assert_eq!(*storage.get_or_create::<i32>("int"), 42);
        assert_eq!(*storage.get_or_create::<String>("string"), "hello");
    }

    #[test]
    fn test_storage_remove() {
        let mut storage = ComponentStateStorage::new();
        *storage.get_or_create::<i32>("counter") = 42;
        assert!(storage.contains("counter"));
        storage.remove("counter");
        assert!(!storage.contains("counter"));
        let state = storage.get_or_create::<i32>("counter");
        assert_eq!(*state, 0);
    }

    #[test]
    fn test_storage_clear() {
        let mut storage = ComponentStateStorage::new();
        *storage.get_or_create::<i32>("a") = 1;
        *storage.get_or_create::<i32>("b") = 2;
        storage.clear();
        assert!(storage.is_empty());
    }

    #[derive(Default, Debug, PartialEq)]
    struct MyState {
        count: u32,
        name: String,
    }

    #[test]
    fn test_storage_custom_type() {
        let mut storage = ComponentStateStorage::new();
        let state = storage.get_or_create::<MyState>("my_state");
        state.count = 10;
        state.name = "test".to_string();
        let state = storage.get_or_create::<MyState>("my_state");
        assert_eq!(state.count, 10);
        assert_eq!(state.name, "test");
    }
}
