use std::any::Any;

use slotmap::SecondaryMap;

use super::id::ElementKey;

/// Type-erased state storage for elements.
///
/// Each element can store arbitrary state that persists across
/// reconciliation as long as the element is not unmounted.
pub struct StateStorage {
    states: SecondaryMap<ElementKey, Box<dyn Any>>,
}

impl StateStorage {
    /// Create a new empty state storage.
    pub fn new() -> Self {
        Self {
            states: SecondaryMap::new(),
        }
    }

    /// Insert state for an element.
    pub fn insert<T: 'static>(&mut self, element: ElementKey, state: T) {
        self.states.insert(element, Box::new(state));
    }

    /// Get a reference to state for an element.
    pub fn get<T: 'static>(&self, element: ElementKey) -> Option<&T> {
        self.states
            .get(element)
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    /// Get a mutable reference to state for an element.
    pub fn get_mut<T: 'static>(&mut self, element: ElementKey) -> Option<&mut T> {
        self.states
            .get_mut(element)
            .and_then(|boxed| boxed.downcast_mut::<T>())
    }

    /// Remove state for an element.
    pub fn remove(&mut self, element: ElementKey) {
        self.states.remove(element);
    }

    /// Check if state exists for an element.
    pub fn contains(&self, element: ElementKey) -> bool {
        self.states.contains_key(element)
    }

    /// Clear all stored state.
    pub fn clear(&mut self) {
        self.states.clear();
    }
}

impl Default for StateStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    #[test]
    fn test_insert_and_get() {
        let mut storage = StateStorage::new();
        let id = make_key();

        storage.insert(id, 42i32);

        assert_eq!(storage.get::<i32>(id), Some(&42));
    }

    #[test]
    fn test_get_mut() {
        let mut storage = StateStorage::new();
        let id = make_key();

        storage.insert(id, String::from("hello"));
        storage.get_mut::<String>(id).map(|s| s.push_str(" world"));

        assert_eq!(storage.get::<String>(id), Some(&String::from("hello world")));
    }

    #[test]
    fn test_remove() {
        let mut storage = StateStorage::new();
        let id = make_key();

        storage.insert(id, 100u64);
        storage.remove(id);

        assert_eq!(storage.get::<u64>(id), None);
    }

    #[test]
    fn test_different_types() {
        let mut storage = StateStorage::new();
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let id1 = sm.insert(());
        let id2 = sm.insert(());

        storage.insert(id1, 42i32);
        storage.insert(id2, String::from("text"));

        assert_eq!(storage.get::<i32>(id1), Some(&42));
        assert_eq!(storage.get::<String>(id2), Some(&String::from("text")));
    }
}
