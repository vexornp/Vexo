//! Per-element nearest-ancestor cache for inherited values.
//!
//! Each element holds an `Arc<InheritedMap>` built at mount by cloning the
//! parent's map (cheap: few entries) and inserting self if the element is an
//! `InheritedElement`. Lookups are O(1) `HashMap` reads.

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

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
}
