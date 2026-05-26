//! Global registry for GlobalKey to Element mappings.
//!
//! This module provides the `GlobalKeyRegistry` which enables cross-parent
//! element identity. When an element with a GlobalKey is mounted, it registers
//! with this registry. During reconciliation, GlobalKey widgets can find their
//! associated elements anywhere in the tree.

use std::collections::HashMap;

use slotmap::SecondaryMap;

use super::id::ElementKey;
use super::key::GlobalKey;

/// Registry tracking GlobalKey to Element mappings.
///
/// This is the global key registry that enables cross-parent element identity.
/// It is owned by the `BuildOwner` and passed through `ElementContext` during
/// element lifecycle operations.
#[derive(Debug)]
pub struct GlobalKeyRegistry {
    /// Map from GlobalKey to the associated ElementKey.
    key_to_element: HashMap<GlobalKey, ElementKey>,

    /// Reverse map for efficient unregistration by element key.
    /// Uses SecondaryMap so stale keys return None automatically.
    element_to_key: SecondaryMap<ElementKey, Option<GlobalKey>>,
}

impl GlobalKeyRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            key_to_element: HashMap::new(),
            element_to_key: SecondaryMap::new(),
        }
    }

    /// Register a GlobalKey with an element.
    ///
    /// Returns `Ok(())` if registration succeeded.
    /// Returns `Err(GlobalKeyError::KeyAlreadyRegistered)` if the key is
    /// already registered with a different element.
    ///
    /// # Arguments
    ///
    /// * `key` - The GlobalKey to register
    /// * `element_id` - The ElementKey to associate with this key
    pub fn register(
        &mut self,
        key: GlobalKey,
        element_id: ElementKey,
    ) -> Result<(), GlobalKeyError> {
        if let Some(&existing) = self.key_to_element.get(&key) {
            if existing != element_id {
                return Err(GlobalKeyError::KeyAlreadyRegistered {
                    key,
                    existing_element: existing,
                });
            }
            // Same element re-registering the same key - idempotent
            return Ok(());
        }

        // Remove any previous key for this element
        if let Some(Some(old_key)) = self.element_to_key.get(element_id).cloned() {
            self.key_to_element.remove(&old_key);
        }

        self.key_to_element.insert(key.clone(), element_id);
        self.element_to_key.insert(element_id, Some(key));
        Ok(())
    }

    /// Unregister a GlobalKey.
    ///
    /// Removes the mapping from the registry. This is called when an element
    /// with a GlobalKey is unmounted.
    pub fn unregister(&mut self, key: &GlobalKey) {
        if let Some(element_id) = self.key_to_element.remove(key) {
            self.element_to_key.remove(element_id);
        }
    }

    /// Unregister by element key.
    ///
    /// Removes any GlobalKey associated with this element. This is called
    /// during element unmount when the element has a GlobalKey.
    pub fn unregister_element(&mut self, element_id: ElementKey) {
        if let Some(Some(key)) = self.element_to_key.remove(element_id) {
            self.key_to_element.remove(&key);
        }
    }

    /// Get the element key for a GlobalKey.
    ///
    /// Returns `None` if the key is not registered.
    pub fn get_element(&self, key: &GlobalKey) -> Option<ElementKey> {
        self.key_to_element.get(key).copied()
    }

    /// Check if a GlobalKey is registered.
    pub fn contains_key(&self, key: &GlobalKey) -> bool {
        self.key_to_element.contains_key(key)
    }

    /// Get the number of registered GlobalKeys.
    pub fn len(&self) -> usize {
        self.key_to_element.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.key_to_element.is_empty()
    }

    /// Clear all registrations.
    pub fn clear(&mut self) {
        self.key_to_element.clear();
        self.element_to_key.clear();
    }
}

impl Default for GlobalKeyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Error type for GlobalKey operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalKeyError {
    /// The key is already registered with another element.
    KeyAlreadyRegistered {
        key: GlobalKey,
        existing_element: ElementKey,
    },
}

impl std::fmt::Display for GlobalKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GlobalKeyError::KeyAlreadyRegistered { key, existing_element } => {
                write!(
                    f,
                    "GlobalKey {:?} is already registered with element {:?}",
                    key, existing_element
                )
            }
        }
    }
}

impl std::error::Error for GlobalKeyError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    fn make_two_keys() -> (ElementKey, ElementKey) {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let k1 = sm.insert(());
        let k2 = sm.insert(());
        (k1, k2)
    }

    #[test]
    fn test_global_key_registry_new() {
        let registry = GlobalKeyRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_global_key_register_and_lookup() {
        let mut registry = GlobalKeyRegistry::new();
        let key = GlobalKey::new();
        let element_id = make_key();

        registry.register(key.clone(), element_id).unwrap();
        assert_eq!(registry.get_element(&key), Some(element_id));
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_global_key_collision_detection() {
        let mut registry = GlobalKeyRegistry::new();
        let key = GlobalKey::new();
        let (element1, element2) = make_two_keys();

        registry.register(key.clone(), element1).unwrap();
        let result = registry.register(key.clone(), element2);

        assert!(matches!(
            result,
            Err(GlobalKeyError::KeyAlreadyRegistered { existing_element, .. }) if existing_element == element1
        ));
        assert_eq!(registry.get_element(&key), Some(element1));
    }

    #[test]
    fn test_global_key_same_element_idempotent() {
        let mut registry = GlobalKeyRegistry::new();
        let key = GlobalKey::new();
        let element_id = make_key();

        registry.register(key.clone(), element_id).unwrap();
        // Registering same key with same element should succeed (idempotent)
        registry.register(key.clone(), element_id).unwrap();
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_global_key_unregister() {
        let mut registry = GlobalKeyRegistry::new();
        let key = GlobalKey::new();
        let element_id = make_key();

        registry.register(key.clone(), element_id).unwrap();
        registry.unregister(&key);

        assert_eq!(registry.get_element(&key), None);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_global_key_unregister_element() {
        let mut registry = GlobalKeyRegistry::new();
        let key = GlobalKey::new();
        let element_id = make_key();

        registry.register(key.clone(), element_id).unwrap();
        registry.unregister_element(element_id);

        assert_eq!(registry.get_element(&key), None);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_global_key_unregister_nonexistent() {
        let mut registry = GlobalKeyRegistry::new();
        let key = GlobalKey::new();

        // Should not panic
        registry.unregister(&key);
        registry.unregister_element(make_key());
    }

    #[test]
    fn test_global_key_contains_key() {
        let mut registry = GlobalKeyRegistry::new();
        let key = GlobalKey::new();

        assert!(!registry.contains_key(&key));

        registry.register(key.clone(), make_key()).unwrap();
        assert!(registry.contains_key(&key));
    }

    #[test]
    fn test_global_key_clear() {
        let mut registry = GlobalKeyRegistry::new();
        let (elem1, elem2) = make_two_keys();

        registry.register(GlobalKey::new(), elem1).unwrap();
        registry.register(GlobalKey::new(), elem2).unwrap();

        assert_eq!(registry.len(), 2);
        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_global_key_element_gets_new_key() {
        let mut registry = GlobalKeyRegistry::new();
        let key1 = GlobalKey::new();
        let key2 = GlobalKey::new();
        let element_id = make_key();

        // Register with key1
        registry.register(key1.clone(), element_id).unwrap();
        assert_eq!(registry.get_element(&key1), Some(element_id));

        // Register same element with key2 (element gets new key)
        registry.register(key2.clone(), element_id).unwrap();

        // key1 should no longer be associated
        assert_eq!(registry.get_element(&key1), None);
        // key2 should be associated
        assert_eq!(registry.get_element(&key2), Some(element_id));
    }
}