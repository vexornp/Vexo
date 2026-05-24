//! Focus tree manager.
//!
//! [`FocusManager`] owns a tree of [`FocusNodeData`] entries stored in a
//! `SlotMap<FocusNodeId, FocusNodeData>`. It tracks which node holds
//! primary focus and provides methods for requesting focus, unfocusing,
//! and querying the focus state.
//!
//! # Tree structure
//!
//! The tree has a single root node (created in [`FocusManager::new`]).
//! All application focus nodes are descendants of this root. The root
//! itself is never focusable — it exists solely as the top-level
//! container for the focus tree.

use std::collections::HashMap;

use slotmap::SlotMap;

use crate::retain::id::ElementKey;
use super::node::{FocusNodeId, FocusNodeData};

// ---------------------------------------------------------------------------
// FocusManager
// ---------------------------------------------------------------------------

/// Manages the focus tree and primary focus state.
pub struct FocusManager {
    /// All focus nodes, keyed by `FocusNodeId`.
    nodes: SlotMap<FocusNodeId, FocusNodeData>,
    /// Reverse mapping from `ElementKey` to `FocusNodeId` for O(1) lookups.
    element_to_node: HashMap<ElementKey, FocusNodeId>,
    /// The node that currently holds primary focus, if any.
    primary_focus: Option<FocusNodeId>,
    /// The root node of the focus tree.
    root: FocusNodeId,
    /// Pending focus change to apply during the next `apply_focus_changes()`.
    pending_focus_change: Option<FocusNodeId>,
}

impl FocusManager {
    /// Create a new `FocusManager` with a root node.
    ///
    /// The root node is not focusable and has no element association.
    pub fn new() -> Self {
        let mut nodes = SlotMap::with_key();
        let root = nodes.insert(FocusNodeData {
            element_key: None,
            parent: None,
            children: Vec::new(),
            can_request_focus: false,
            skip_traversal: true,
        });

        Self {
            nodes,
            element_to_node: HashMap::new(),
            primary_focus: None,
            root,
            pending_focus_change: None,
        }
    }

    /// Returns the root node id.
    ///
    /// This is the top-level container of the focus tree. All application
    /// focus nodes are descendants of this root.
    pub fn root_scope(&self) -> FocusNodeId {
        self.root
    }

    // -----------------------------------------------------------------------
    // Node creation
    // -----------------------------------------------------------------------

    /// Create a focus node associated with `element_key` as a child of
    /// `parent`, or return the existing node if one is already associated
    /// with this element.
    ///
    /// This method is idempotent: calling it multiple times with the same
    /// `element_key` returns the same `FocusNodeId` without creating
    /// duplicates.
    ///
    /// If `parent` is `None`, the node is attached to the root.
    pub fn create_node_for_element(
        &mut self,
        element_key: ElementKey,
        parent_id: Option<FocusNodeId>,
    ) -> Option<FocusNodeId> {
        // If a node already exists for this element, return it (idempotent).
        if let Some(existing) = self.node_for_element(element_key) {
            return Some(existing);
        }

        let parent = parent_id.unwrap_or(self.root);

        let node_id = self.nodes.insert(FocusNodeData {
            element_key: Some(element_key),
            parent: Some(parent),
            children: Vec::new(),
            can_request_focus: true,
            skip_traversal: false,
        });

        // Register in parent's children list.
        if let Some(parent_data) = self.nodes.get_mut(parent) {
            parent_data.children.push(node_id);
        }

        // Register in element-to-node HashMap for O(1) lookup.
        self.element_to_node.insert(element_key, node_id);

        Some(node_id)
    }

    // -----------------------------------------------------------------------
    // Node queries
    // -----------------------------------------------------------------------

    /// Get a reference to the data for `id`.
    pub fn get(&self, id: FocusNodeId) -> Option<&FocusNodeData> {
        self.nodes.get(id)
    }

    /// Get a mutable reference to the data for `id`.
    pub fn get_mut(&mut self, id: FocusNodeId) -> Option<&mut FocusNodeData> {
        self.nodes.get_mut(id)
    }

    /// Look up the focus node associated with `element_key`, if any.
    pub fn node_for_element(&self, element_key: ElementKey) -> Option<FocusNodeId> {
        self.element_to_node.get(&element_key).copied()
    }

    /// Return the currently focused node id, if any.
    pub fn primary_focus(&self) -> Option<FocusNodeId> {
        self.primary_focus
    }

    /// Return the element key of the currently focused node, if any.
    pub fn primary_focus_element(&self) -> Option<ElementKey> {
        self.primary_focus
            .and_then(|id| self.nodes.get(id))
            .and_then(|n| n.element_key)
    }

    // -----------------------------------------------------------------------
    // Focus operations
    // -----------------------------------------------------------------------

    /// Request that `id` become the primary focus.
    ///
    /// If the node has `can_request_focus == false`, this is a no-op.
    /// Otherwise the focus change is deferred until `apply_focus_changes()`
    /// is called.
    pub fn request_focus(&mut self, id: FocusNodeId) {
        let can = self.nodes.get(id).map_or(false, |n| n.can_request_focus);
        if !can {
            return;
        }
        self.pending_focus_change = Some(id);
    }

    /// Clear primary focus. Takes effect immediately.
    pub fn unfocus(&mut self) {
        self.primary_focus = None;
    }

    /// Apply any pending focus change.
    ///
    /// If a focus change was requested via `request_focus()` since the last
    /// call to `apply_focus_changes()`, it is applied now. If the same node
    /// is already focused, the pending change is discarded.
    pub fn apply_focus_changes(&mut self) {
        if let Some(new_id) = self.pending_focus_change.take() {
            if self.primary_focus != Some(new_id) {
                self.primary_focus = Some(new_id);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Node removal
    // -----------------------------------------------------------------------

    /// Remove a node from the focus tree.
    ///
    /// The node is detached from its parent and all children are removed
    /// recursively. If the removed node (or a descendant) held primary
    /// focus, primary focus is cleared.
    pub fn remove_node(&mut self, id: FocusNodeId) {
        if id == self.root {
            return; // Never remove the root.
        }
        self.remove_node_recursive(id);
    }

    fn remove_node_recursive(&mut self, id: FocusNodeId) {
        // Collect children first so we can remove them without borrow issues.
        let children: Vec<FocusNodeId> = self
            .nodes
            .get(id)
            .map(|n| n.children.clone())
            .unwrap_or_default();

        for child in children {
            self.remove_node_recursive(child);
        }

        // Detach from parent.
        if let Some(node) = self.nodes.get(id) {
            if let Some(parent_id) = node.parent {
                if let Some(parent) = self.nodes.get_mut(parent_id) {
                    parent.children.retain(|c| *c != id);
                }
            }
        }

        // Remove from element_to_node HashMap if this node has an element_key.
        if let Some(node) = self.nodes.get(id) {
            if let Some(ek) = node.element_key {
                self.element_to_node.remove(&ek);
            }
        }

        // Clear primary focus if this node held it.
        if self.primary_focus == Some(id) {
            self.primary_focus = None;
        }

        // Clear pending focus if this node was pending.
        if self.pending_focus_change == Some(id) {
            self.pending_focus_change = None;
        }

        self.nodes.remove(id);
    }

    // -----------------------------------------------------------------------
    // Reparent
    // -----------------------------------------------------------------------

    /// Move `id` so that it becomes a child of `new_parent`.
    ///
    /// If `new_parent` is `None`, the node is attached to the root.
    pub fn reparent(&mut self, id: FocusNodeId, new_parent: Option<FocusNodeId>) {
        let new_parent_id = new_parent.unwrap_or(self.root);
        if id == self.root || id == new_parent_id {
            return;
        }

        // Detach from old parent.
        if let Some(old_parent_id) = self.nodes.get(id).and_then(|n| n.parent) {
            if let Some(old_parent) = self.nodes.get_mut(old_parent_id) {
                old_parent.children.retain(|c| *c != id);
            }
        }

        // Attach to new parent.
        if let Some(node) = self.nodes.get_mut(id) {
            node.parent = Some(new_parent_id);
        }
        if let Some(new_parent) = self.nodes.get_mut(new_parent_id) {
            new_parent.children.push(id);
        }
    }

    /// Returns the number of application focus nodes (excludes the root node).
    pub fn app_node_count(&self) -> usize {
        self.nodes.len().saturating_sub(1)
    }

    /// Returns whether a focus node exists for the given element.
    pub fn has_node_for_element(&self, element_key: ElementKey) -> bool {
        self.element_to_node.contains_key(&element_key)
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_manager_new() {
        let mgr = FocusManager::new();
        let root = mgr.root_scope();
        let root_data = mgr.get(root).expect("root should exist");
        assert!(root_data.element_key.is_none());
        assert!(root_data.parent.is_none());
        assert!(root_data.children.is_empty());
        assert!(!root_data.can_request_focus);
        assert!(root_data.skip_traversal);
        assert!(mgr.primary_focus().is_none());
    }

    #[test]
    fn test_create_node_for_element_no_parent() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());
        let id = mgr.create_node_for_element(key, None).unwrap();
        let data = mgr.get(id).expect("node should exist");
        assert_eq!(data.element_key, Some(key));
        assert_eq!(data.parent, Some(mgr.root_scope()));
        assert!(data.children.is_empty());
        assert!(data.can_request_focus);
        assert!(!data.skip_traversal);

        // Should be a child of root.
        let root_data = mgr.get(mgr.root_scope()).unwrap();
        assert!(root_data.children.contains(&id));
    }

    #[test]
    fn test_request_focus() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());
        let id = mgr.create_node_for_element(key, None).unwrap();
        mgr.request_focus(id);
        // Not applied yet.
        assert!(mgr.primary_focus().is_none());
        mgr.apply_focus_changes();
        assert_eq!(mgr.primary_focus(), Some(id));
    }

    #[test]
    fn test_request_focus_can_request_focus_false() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());
        let id = mgr.create_node_for_element(key, None).unwrap();
        if let Some(data) = mgr.get_mut(id) {
            data.can_request_focus = false;
        }
        mgr.request_focus(id);
        mgr.apply_focus_changes();
        assert!(mgr.primary_focus().is_none());
    }

    #[test]
    fn test_unfocus() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());
        let id = mgr.create_node_for_element(key, None).unwrap();
        mgr.request_focus(id);
        mgr.apply_focus_changes();
        assert_eq!(mgr.primary_focus(), Some(id));
        mgr.unfocus();
        assert!(mgr.primary_focus().is_none());
    }

    #[test]
    fn test_remove_node() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());
        let id = mgr.create_node_for_element(key, None).unwrap();
        mgr.request_focus(id);
        mgr.apply_focus_changes();
        assert_eq!(mgr.primary_focus(), Some(id));

        mgr.remove_node(id);
        assert!(mgr.primary_focus().is_none());
        assert!(mgr.get(id).is_none());

        // Root should have no children now.
        let root_data = mgr.get(mgr.root_scope()).unwrap();
        assert!(!root_data.children.contains(&id));
    }

    #[test]
    fn test_element_to_node_mapping() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());
        let id = mgr.create_node_for_element(key, None).unwrap();
        assert_eq!(mgr.node_for_element(key), Some(id));
        assert_eq!(mgr.get(id).unwrap().element_key, Some(key));
    }

    #[test]
    fn test_deferred_focus_change() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());
        let id = mgr.create_node_for_element(key, None).unwrap();
        mgr.request_focus(id);
        // Focus not applied yet.
        assert!(mgr.primary_focus().is_none());
        mgr.apply_focus_changes();
        assert_eq!(mgr.primary_focus(), Some(id));
    }

    #[test]
    fn test_deferred_coalescing() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key1 = elem_map.insert(());
        let key2 = elem_map.insert(());
        let id1 = mgr.create_node_for_element(key1, None).unwrap();
        let id2 = mgr.create_node_for_element(key2, None).unwrap();
        mgr.request_focus(id1);
        mgr.request_focus(id2);
        // Only the last request should win.
        mgr.apply_focus_changes();
        assert_eq!(mgr.primary_focus(), Some(id2));
    }

    // -----------------------------------------------------------------------
    // create_node_for_element tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_node_for_element_basic() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());

        let node_id = mgr
            .create_node_for_element(key, None)
            .expect("should return a node id");

        // The node should be findable via node_for_element.
        assert_eq!(mgr.node_for_element(key), Some(node_id));

        // The node data should have the correct element_key.
        let data = mgr.get(node_id).expect("node should exist");
        assert_eq!(data.element_key, Some(key));
        assert_eq!(data.parent, Some(mgr.root_scope()));
        assert!(data.can_request_focus);
        assert!(!data.skip_traversal);

        // It should be a child of root.
        let root_data = mgr.get(mgr.root_scope()).unwrap();
        assert!(root_data.children.contains(&node_id));
    }

    #[test]
    fn test_create_node_for_element_with_parent() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let parent_key = elem_map.insert(());
        let child_key = elem_map.insert(());

        let parent_id = mgr
            .create_node_for_element(parent_key, None)
            .expect("parent node id");

        let child_id = mgr
            .create_node_for_element(child_key, Some(parent_id))
            .expect("child node id");

        // Child should have parent_id as its parent.
        let child_data = mgr.get(child_id).unwrap();
        assert_eq!(child_data.parent, Some(parent_id));

        // Parent should list child in its children.
        let parent_data = mgr.get(parent_id).unwrap();
        assert!(parent_data.children.contains(&child_id));
    }

    #[test]
    fn test_create_node_for_element_idempotent() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());

        let id1 = mgr.create_node_for_element(key, None).unwrap();
        let id2 = mgr.create_node_for_element(key, None).unwrap();

        // Should return the same node id both times.
        assert_eq!(id1, id2);

        // Should not create duplicates in parent's children list.
        let root_data = mgr.get(mgr.root_scope()).unwrap();
        let count = root_data.children.iter().filter(|c| **c == id1).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_create_node_for_element_removed_node_not_returned() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());

        let id1 = mgr.create_node_for_element(key, None).unwrap();
        mgr.remove_node(id1);

        // After removal, node_for_element should return None.
        assert_eq!(mgr.node_for_element(key), None);

        // Creating again should produce a new, different node.
        let id2 = mgr.create_node_for_element(key, None).unwrap();
        assert_ne!(id1, id2);
        assert_eq!(mgr.node_for_element(key), Some(id2));
    }

    #[test]
    fn test_remove_node_cleans_up_hashmap() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());

        let id = mgr.create_node_for_element(key, None).unwrap();
        assert_eq!(mgr.node_for_element(key), Some(id));

        mgr.remove_node(id);

        // The HashMap should no longer contain the mapping.
        assert_eq!(mgr.node_for_element(key), None);
    }

    #[test]
    fn test_remove_node_recursive_cleans_up_child_hashmap() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let parent_key = elem_map.insert(());
        let child_key = elem_map.insert(());

        let parent_id = mgr
            .create_node_for_element(parent_key, None)
            .unwrap();
        let _child_id = mgr
            .create_node_for_element(child_key, Some(parent_id))
            .unwrap();

        // Removing the parent should also clean up the child's HashMap entry.
        mgr.remove_node(parent_id);
        assert_eq!(mgr.node_for_element(parent_key), None);
        assert_eq!(mgr.node_for_element(child_key), None);
    }
}
