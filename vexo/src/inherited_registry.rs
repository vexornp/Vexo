//! Per-element nearest-ancestor cache for inherited values.
//!
//! Each element holds an `Arc<InheritedMap>` built at mount by cloning the
//! parent's map (cheap: few entries) and inserting self if the element is an
//! `InheritedElement`. Lookups are O(1) `HashMap` reads.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::id::ElementKey;

/// Nearest-ancestor cache: `TypeId` of the exposed value → provider element.
///
/// Built top-down at mount. Never mutated post-mount (only swapped wholesale
/// on rebuild of an ancestor provider). Vexo never reparents, so the map is
/// always consistent with tree position.
#[derive(Clone, Default)]
pub struct InheritedMap {
    inner: HashMap<TypeId, ElementKey>,
}

impl InheritedMap {
    /// Empty map (used by root and by descendants with no providers).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Look up the nearest provider element for value type `V`.
    pub fn get(&self, type_id: TypeId) -> Option<ElementKey> {
        self.inner.get(&type_id).copied()
    }

    /// Return a new map with `type_id → key` inserted. Used by
    /// `InheritedElement::mount` to produce the map its subtree will see.
    pub fn with_insert(&self, type_id: TypeId, key: ElementKey) -> Self {
        let mut clone = self.clone();
        clone.inner.insert(type_id, key);
        clone
    }
}

/// Pipeline-owned registry of inherited-value providers and their dependents.
///
/// Uses `RefCell` interior mutability so it can be borrowed as
/// `&InheritedRegistry` from both `ElementContext` and `RenderContext` while
/// methods take `&self` (same pattern as `BuildOwner`).
///
/// # Borrow safety
///
/// `InheritedElement::mount`/`update`/`unmount` never invoke user code while
/// holding a `RefCell` borrow, so re-entry is structurally prevented.
pub struct InheritedRegistry {
    /// Value each provider exposes, keyed by provider element.
    /// Stored as `Box<dyn Any + Send + Sync>` so lookups don't touch the
    /// element tree.
    values: RefCell<HashMap<ElementKey, (TypeId, Box<dyn Any + Send + Sync>)>>,

    /// Dependents per (provider, type). Used by `InheritedElement::update`
    /// to mark dependents dirty when the value changes.
    dependents: RefCell<HashMap<ElementKey, HashMap<TypeId, HashSet<ElementKey>>>>,
}

impl InheritedRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            values: RefCell::new(HashMap::new()),
            dependents: RefCell::new(HashMap::new()),
        }
    }

    /// Register a provider. Stores `value` under `key` for later lookup via
    /// `value::<V>(key)`. Called by `InheritedElement::mount`.
    pub fn register_provider(
        &self,
        key: ElementKey,
        type_id: TypeId,
        value: Box<dyn Any + Send + Sync>,
    ) {
        self.values.borrow_mut().insert(key, (type_id, value));
    }

    /// Replace the stored value for an existing provider. Called by
    /// `InheritedElement::update` when `update_should_notify` returned true.
    pub fn update_value(
        &self,
        key: ElementKey,
        type_id: TypeId,
        value: Box<dyn Any + Send + Sync>,
    ) {
        self.values.borrow_mut().insert(key, (type_id, value));
    }

    /// Remove a provider and all its dependents. Called by
    /// `InheritedElement::unmount`.
    pub fn remove_provider(&self, key: ElementKey) {
        self.values.borrow_mut().remove(&key);
        self.dependents.borrow_mut().remove(&key);
    }

    /// Register `dep` as a dependent of `provider` for value type `type_id`.
    /// Idempotent: adding the same dependent twice has no effect.
    pub fn add_dependent(&self, provider: ElementKey, type_id: TypeId, dep: ElementKey) {
        self.dependents
            .borrow_mut()
            .entry(provider)
            .or_default()
            .entry(type_id)
            .or_default()
            .insert(dep);
    }

    /// Read the value exposed by `provider` as type `V`, cloned out of the
    /// registry. Values are `Clone + PartialEq` by the `InheritedWidget` trait
    /// requirement, so cloning is always available.
    ///
    /// Returns `None` if `provider` is not registered or the stored value
    /// is not a `V`.
    pub fn value_clone<V: Clone + 'static>(&self, provider: ElementKey) -> Option<V> {
        self.values
            .borrow()
            .get(&provider)
            .and_then(|(_, v)| v.downcast_ref::<V>())
            .cloned()
    }

    /// Snapshot of dependents for `provider`. Returns an owned `Vec` so the
    /// caller can iterate without holding a `RefCell` borrow (important: the
    /// caller will call `BuildOwner::mark_needs_build` during iteration).
    pub fn dependents_for(&self, provider: ElementKey) -> Vec<ElementKey> {
        self.dependents
            .borrow()
            .get(&provider)
            .map(|by_type| {
                by_type
                    .values()
                    .flat_map(|set| set.iter().copied())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for InheritedRegistry {
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
    fn empty_map_returns_none() {
        let map = InheritedMap::empty();
        assert_eq!(map.get(TypeId::of::<u32>()), None);
    }

    #[test]
    fn with_insert_then_get() {
        let k = make_key();
        let map = InheritedMap::empty().with_insert(TypeId::of::<u32>(), k);
        assert_eq!(map.get(TypeId::of::<u32>()), Some(k));
    }

    #[test]
    fn with_insert_does_not_mutate_original() {
        let k = make_key();
        let base = InheritedMap::empty();
        let _child = base.with_insert(TypeId::of::<u32>(), k);
        // Original is unchanged (COW).
        assert_eq!(base.get(TypeId::of::<u32>()), None);
    }

    #[test]
    fn with_insert_overrides_existing_type() {
        let k1 = make_key();
        let k2 = make_key();
        let map = InheritedMap::empty()
            .with_insert(TypeId::of::<u32>(), k1)
            .with_insert(TypeId::of::<u32>(), k2);
        // Nearest ancestor wins (last insert).
        assert_eq!(map.get(TypeId::of::<u32>()), Some(k2));
    }

    #[test]
    fn registry_register_and_value() {
        let reg = InheritedRegistry::new();
        let k = make_key();
        reg.register_provider(k, TypeId::of::<u32>(), Box::new(42u32));
        let v = reg
            .value_clone::<u32>(k)
            .expect("provider should expose u32");
        assert_eq!(v, 42);
    }

    #[test]
    fn registry_value_missing_provider_returns_none() {
        let reg = InheritedRegistry::new();
        let k = make_key();
        assert!(reg.value_clone::<u32>(k).is_none());
    }

    #[test]
    fn registry_update_value() {
        let reg = InheritedRegistry::new();
        let k = make_key();
        reg.register_provider(k, TypeId::of::<u32>(), Box::new(1u32));
        reg.update_value(k, TypeId::of::<u32>(), Box::new(99u32));
        let v = reg.value_clone::<u32>(k).unwrap();
        assert_eq!(v, 99);
    }

    #[test]
    fn registry_remove_provider_drops_value_and_dependents() {
        let reg = InheritedRegistry::new();
        let provider = make_key();
        let dep = make_key();
        reg.register_provider(provider, TypeId::of::<u32>(), Box::new(7u32));
        reg.add_dependent(provider, TypeId::of::<u32>(), dep);
        reg.remove_provider(provider);
        assert!(reg.value_clone::<u32>(provider).is_none());
        assert!(reg.dependents_for(provider).is_empty());
    }

    #[test]
    fn registry_add_dependent_idempotent() {
        let reg = InheritedRegistry::new();
        let provider = make_key();
        let dep = make_key();
        reg.register_provider(provider, TypeId::of::<u32>(), Box::new(0u32));
        reg.add_dependent(provider, TypeId::of::<u32>(), dep);
        reg.add_dependent(provider, TypeId::of::<u32>(), dep);
        let deps = reg.dependents_for(provider);
        assert_eq!(deps, vec![dep]);
    }

    #[test]
    fn registry_dependents_snapshot_does_not_hold_borrow() {
        // This test verifies that dependents_for returns an owned Vec, so the
        // caller can iterate while calling other &self methods.
        let reg = InheritedRegistry::new();
        let provider = make_key();
        let dep = make_key();
        reg.register_provider(provider, TypeId::of::<u32>(), Box::new(0u32));
        reg.add_dependent(provider, TypeId::of::<u32>(), dep);
        let deps = reg.dependents_for(provider);
        // Can still call other methods while iterating the snapshot.
        for d in deps {
            reg.add_dependent(provider, TypeId::of::<u32>(), d);
        }
    }
}
