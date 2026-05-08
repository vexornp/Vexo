//! Build owner for tracking dirty elements and driving targeted rebuilds.
//!
//! Inspired by Flutter's BuildOwner, this module tracks which elements
//! need to rebuild and performs those rebuilds efficiently.

use std::collections::HashSet;

use super::global_key_registry::GlobalKeyRegistry;
use super::id::ElementId;

/// Tracks dirty elements and drives targeted rebuilds.
///
/// In Flutter, when `setState()` is called, only that element's subtree
/// rebuilds. The BuildOwner tracks which elements are dirty and ensures
/// only those elements (and their descendants) are reconciled.
///
/// # Example
///
/// ```ignore
/// let mut build_owner = BuildOwner::new();
///
/// // Element requests rebuild
/// build_owner.mark_needs_build(element_id);
///
/// // Later, perform all pending rebuilds
/// build_owner.perform_rebuilds(&mut pipeline);
/// ```
pub struct BuildOwner {
    /// Elements that need to rebuild.
    dirty_elements: HashSet<ElementId>,

    /// Elements in the current build scope.
    ///
    /// During a rebuild, we track which elements are being built
    /// to detect cycles.
    building: HashSet<ElementId>,

    /// Global key registry for cross-parent element identity.
    global_keys: GlobalKeyRegistry,
}

impl BuildOwner {
    /// Create a new empty build owner.
    pub fn new() -> Self {
        Self {
            dirty_elements: HashSet::new(),
            building: HashSet::new(),
            global_keys: GlobalKeyRegistry::new(),
        }
    }

    /// Mark an element as needing rebuild.
    ///
    /// The element will be rebuilt during the next `perform_rebuilds()`.
    /// This is equivalent to Flutter's `markNeedsBuild()`.
    pub fn mark_needs_build(&mut self, element_id: ElementId) {
        self.dirty_elements.insert(element_id);
    }

    /// Check if an element is marked as dirty.
    pub fn is_dirty(&self, element_id: ElementId) -> bool {
        self.dirty_elements.contains(&element_id)
    }

    /// Check if there are any pending rebuilds.
    pub fn has_pending_rebuilds(&self) -> bool {
        !self.dirty_elements.is_empty()
    }

    /// Get the number of dirty elements.
    pub fn dirty_count(&self) -> usize {
        self.dirty_elements.len()
    }

    /// Drain dirty elements, returning them for processing.
    ///
    /// This clears the dirty set and returns all elements that need rebuild.
    pub fn drain_dirty(&mut self) -> impl Iterator<Item = ElementId> + '_ {
        self.dirty_elements.drain()
    }

    /// Clear all dirty elements without rebuilding.
    pub fn clear_dirty(&mut self) {
        self.dirty_elements.clear();
    }

    /// Enter a build scope for an element.
    ///
    /// Used to detect cycles during rebuild. Returns false if the element
    /// is already being built (cycle detected).
    pub fn enter_build_scope(&mut self, element_id: ElementId) -> bool {
        if self.building.contains(&element_id) {
            // Cycle detected
            return false;
        }
        self.building.insert(element_id);
        true
    }

    /// Exit a build scope for an element.
    pub fn exit_build_scope(&mut self, element_id: ElementId) {
        self.building.remove(&element_id);
    }

    /// Check if currently building an element.
    pub fn is_building(&self, element_id: ElementId) -> bool {
        self.building.contains(&element_id)
    }

    /// Get a reference to the global key registry.
    pub fn global_keys(&self) -> &GlobalKeyRegistry {
        &self.global_keys
    }

    /// Get a mutable reference to the global key registry.
    pub fn global_keys_mut(&mut self) -> &mut GlobalKeyRegistry {
        &mut self.global_keys
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
