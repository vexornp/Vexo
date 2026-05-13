//! Build owner for tracking dirty elements and driving targeted rebuilds.
//!
//! Inspired by Flutter's BuildOwner, this module tracks which elements
//! need to rebuild and performs those rebuilds efficiently.
//!
//! # Depth Ordering
//!
//! Dirty elements are sorted by tree depth before rebuild so that parents
//! always rebuild before children. This is a critical Flutter invariant:
//! a parent's rebuild may change which children exist, so children must
//! not rebuild until the parent has reconciled its subtree.
//!
//! # Interior Mutability
//!
//! The dirty tracking uses `RefCell` for interior mutability. This allows
//! the `mark_needs_build()` method to be called from within callbacks
//! (e.g., `StatefulMutable::set()` dirty callbacks) that fire during
//! event handling, when the pipeline has a mutable borrow on itself.
//! Using `RefCell` avoids aliasing UB that would occur with raw pointers.

use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashSet;

use super::global_key_registry::GlobalKeyRegistry;
use super::id::ElementKey;

/// Tracks dirty elements and drives targeted rebuilds.
///
/// In Flutter, when `setState()` is called, only that element's subtree
/// rebuilds. The BuildOwner tracks which elements are dirty and ensures
/// only those elements (and their descendants) are reconciled.
///
/// Dirty elements are stored in a `Vec` for ordering (sorted by depth
/// before rebuild) and a `HashSet` for O(1) membership checks.
///
/// # Interior Mutability
///
/// The `dirty_elements` and `dirty_set` are stored in `RefCell` to allow
/// `mark_needs_build()` to be called from within event callbacks without
/// requiring a mutable reference to the BuildOwner. This is necessary
/// because the pipeline holds a mutable borrow during event handling,
/// and the `StatefulMutable` dirty callbacks need to mark elements dirty.
pub struct BuildOwner {
    /// Elements that need rebuild, in insertion order.
    /// Sorted by depth before rebuild via `sort_dirty_by_depth()`.
    /// Wrapped in RefCell for interior mutability.
    dirty_elements: RefCell<Vec<ElementKey>>,

    /// Set for O(1) membership check (kept in sync with dirty_elements).
    /// Wrapped in RefCell for interior mutability.
    dirty_set: RefCell<HashSet<ElementKey>>,

    /// Elements in the current build scope.
    ///
    /// During a rebuild, we track which elements are being built
    /// to detect cycles.
    building: HashSet<ElementKey>,

    /// Global key registry for cross-parent element identity.
    /// Wrapped in RefCell so it can be borrowed mutably via &self,
    /// enabling simultaneous shared borrow of BuildOwner and mutable
    /// borrow of GlobalKeyRegistry.
    global_keys: RefCell<GlobalKeyRegistry>,
}

impl BuildOwner {
    /// Create a new empty build owner.
    pub fn new() -> Self {
        Self {
            dirty_elements: RefCell::new(Vec::new()),
            dirty_set: RefCell::new(HashSet::new()),
            building: HashSet::new(),
            global_keys: RefCell::new(GlobalKeyRegistry::new()),
        }
    }

    /// Mark an element as needing rebuild.
    ///
    /// The element will be rebuilt during the next `perform_rebuilds()`.
    /// This is equivalent to Flutter's `markNeedsBuild()`.
    ///
    /// Idempotent: calling this multiple times with the same element ID
    /// only adds it once.
    ///
    /// This method uses interior mutability (RefCell), so it can be called
    /// from within callbacks that fire during event handling without
    /// requiring a mutable reference to the BuildOwner.
    pub fn mark_needs_build(&self, element_id: ElementKey) {
        if self.dirty_set.borrow_mut().insert(element_id) {
            self.dirty_elements.borrow_mut().push(element_id);
        }
    }

    /// Check if an element is marked as dirty.
    pub fn is_dirty(&self, element_id: ElementKey) -> bool {
        self.dirty_set.borrow().contains(&element_id)
    }

    /// Check if there are any pending rebuilds.
    pub fn has_pending_rebuilds(&self) -> bool {
        !self.dirty_set.borrow().is_empty()
    }

    /// Get the number of dirty elements.
    pub fn dirty_count(&self) -> usize {
        self.dirty_set.borrow().len()
    }

    /// Sort dirty elements by tree depth (parents before children).
    ///
    /// Must be called before draining. The `depth` function is provided
    /// by the caller (typically via `ElementRegistry::depth()`).
    ///
    /// This ensures the Flutter invariant that parents rebuild before
    /// children, which is critical because a parent's rebuild may
    /// change which children exist.
    pub fn sort_dirty_by_depth<F>(&self, mut depth: F)
    where
        F: FnMut(ElementKey) -> usize,
    {
        self.dirty_elements.borrow_mut().sort_by_key(|id| depth(*id));
    }

    /// Drain dirty elements in depth order.
    ///
    /// Call `sort_dirty_by_depth()` first to ensure parents come
    /// before children. Clears both the vec and the set.
    pub fn drain_dirty_sorted(&mut self) -> Vec<ElementKey> {
        self.dirty_set.borrow_mut().clear();
        self.dirty_elements.borrow_mut().drain(..).collect()
    }

    /// Drain dirty elements, returning them for processing.
    ///
    /// This clears the dirty set and returns all elements that need rebuild.
    /// Elements are returned in insertion order (not depth-sorted).
    /// Prefer `sort_dirty_by_depth()` + `drain_dirty_sorted()` for
    /// correct parent-before-child ordering.
    pub fn drain_dirty(&mut self) -> Vec<ElementKey> {
        self.dirty_set.borrow_mut().clear();
        self.dirty_elements.borrow_mut().drain(..).collect()
    }

    /// Clear all dirty elements without rebuilding.
    pub fn clear_dirty(&mut self) {
        self.dirty_elements.borrow_mut().clear();
        self.dirty_set.borrow_mut().clear();
    }

    /// Enter a build scope for an element.
    ///
    /// Used to detect cycles during rebuild. Returns false if the element
    /// is already being built (cycle detected).
    pub fn enter_build_scope(&mut self, element_id: ElementKey) -> bool {
        if self.building.contains(&element_id) {
            // Cycle detected
            return false;
        }
        self.building.insert(element_id);
        true
    }

    /// Exit a build scope for an element.
    pub fn exit_build_scope(&mut self, element_id: ElementKey) {
        self.building.remove(&element_id);
    }

    /// Check if currently building an element.
    pub fn is_building(&self, element_id: ElementKey) -> bool {
        self.building.contains(&element_id)
    }

    /// Get a reference to the global key registry.
    pub fn global_keys(&self) -> Ref<'_, GlobalKeyRegistry> {
        self.global_keys.borrow()
    }

    /// Get a mutable reference to the global key registry via interior mutability.
    ///
    /// Takes `&self` (not `&mut self`) so that a shared `&BuildOwner` reference
    /// can coexist with a mutable borrow of the global key registry.
    /// This is needed because the pipeline passes `&BuildOwner` to elements
    /// while also needing mutable access to global keys for registration.
    pub fn global_keys_mut(&self) -> RefMut<'_, GlobalKeyRegistry> {
        self.global_keys.borrow_mut()
    }
}

impl Default for BuildOwner {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a rebuild operation.
#[derive(Debug, Default)]
pub struct RebuildResult {
    /// Number of elements rebuilt.
    pub elements_rebuilt: usize,

    /// Whether any cycles were detected.
    pub cycles_detected: bool,
}
