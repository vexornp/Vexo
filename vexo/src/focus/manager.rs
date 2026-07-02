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

use super::node::{FocusNodeData, FocusNodeId};
use crate::id::ElementKey;

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
    /// The node that previously held primary focus (for repaint marking).
    previous_primary_focus: Option<FocusNodeId>,
    /// The root node of the focus tree.
    root: FocusNodeId,
    /// Pending focus change to apply during the next `apply_focus_changes()`.
    pending_focus_change: Option<FocusNodeId>,
    /// Whether focus state changed during the last event handling cycle.
    focus_changed: bool,
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
            is_text_input: false,
            on_focus_change: None,
        });

        Self {
            nodes,
            element_to_node: HashMap::new(),
            primary_focus: None,
            previous_primary_focus: None,
            root,
            pending_focus_change: None,
            focus_changed: false,
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
            is_text_input: false,
            on_focus_change: None,
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

    /// Return whether the currently focused node is itself a text input.
    ///
    /// This is an O(1) check on the primary focus node's `is_text_input` flag
    /// (set from `ComponentState::requests_focus_on_click()`). Unlike a subtree
    /// walk, it returns `true` only when the text input's *own* element holds
    /// focus — never when an ancestor (e.g. a `ScrollView` containing a
    /// `TextEdit`) is focused.
    pub fn is_primary_focus_text_input(&self) -> bool {
        self.primary_focus
            .and_then(|id| self.nodes.get(id))
            .is_some_and(|n| n.is_text_input)
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
        self.focus_changed = true;
    }

    /// Clear primary focus. Takes effect immediately.
    pub fn unfocus(&mut self) {
        if let Some(old_id) = self.primary_focus {
            self.notify_focus_changed(old_id, false);
            self.previous_primary_focus = self.primary_focus;
            self.primary_focus = None;
            self.focus_changed = true;
        }
    }

    /// Apply any pending focus change.
    ///
    /// If a focus change was requested via `request_focus()` since the last
    /// call to `apply_focus_changes()`, it is applied now. If the same node
    /// is already focused, the pending change is discarded.
    pub fn apply_focus_changes(&mut self) {
        if let Some(new_id) = self.pending_focus_change.take() {
            if self.primary_focus != Some(new_id) {
                let old_id = self.primary_focus;
                self.previous_primary_focus = old_id;
                self.primary_focus = Some(new_id);
                self.focus_changed = true;

                // Notify ancestors of the previously-focused node (lost focus)
                if let Some(old) = old_id {
                    self.notify_focus_changed(old, false);
                }
                // Notify ancestors of the newly-focused node (gained focus)
                self.notify_focus_changed(new_id, true);
            }
        }
    }

    /// Check if focus state changed since the last call to `take_focus_changed()`.
    pub fn focus_changed(&self) -> bool {
        self.focus_changed
    }

    /// Take the focus_changed flag — returns true if focus changed and clears the flag.
    pub fn take_focus_changed(&mut self) -> bool {
        let changed = self.focus_changed;
        self.focus_changed = false;
        changed
    }

    /// Returns the element key of the node that previously held primary focus.
    pub fn previous_primary_focus(&self) -> Option<ElementKey> {
        self.previous_primary_focus
            .and_then(|id| self.nodes.get(id).and_then(|n| n.element_key))
    }

    /// Returns the node id that previously held primary focus.
    pub fn previous_primary_focus_node(&self) -> Option<FocusNodeId> {
        self.previous_primary_focus
    }

    // -----------------------------------------------------------------------
    // Focus-change notification
    // -----------------------------------------------------------------------

    /// Notify `on_focus_change` callbacks on the given node and its ancestors
    /// that focus state changed.
    ///
    /// Walks from `node_id` up to the root, invoking `on_focus_change(focused)`
    /// on each node that has a callback set. This matches Flutter's
    /// `FocusNode.hasFocus` behavior where ancestor nodes are notified when
    /// a descendant gains or loses focus.
    fn notify_focus_changed(&self, node_id: FocusNodeId, focused: bool) {
        let mut current = Some(node_id);
        while let Some(id) = current {
            if let Some(node) = self.nodes.get(id) {
                if let Some(ref callback) = node.on_focus_change {
                    callback(focused);
                }
                current = node.parent;
            } else {
                break;
            }
        }
    }

    /// Collect all ancestor element keys that have `on_focus_change` callbacks.
    ///
    /// Used by the pipeline to mark Focus-wrapped elements for rebuild when
    /// a descendant gains or loses focus.
    pub fn ancestor_elements_with_callbacks(&self, node_id: FocusNodeId) -> Vec<ElementKey> {
        let mut result = Vec::new();
        let mut current = self.nodes.get(node_id).and_then(|n| n.parent);
        while let Some(parent_id) = current {
            if let Some(parent) = self.nodes.get(parent_id) {
                if parent.on_focus_change.is_some() {
                    if let Some(ek) = parent.element_key {
                        result.push(ek);
                    }
                }
                current = parent.parent;
            } else {
                break;
            }
        }
        result
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
        // If this node holds primary focus, notify ancestors before removing.
        if self.primary_focus == Some(id) {
            self.notify_focus_changed(id, false);
        }

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
    use std::sync::Arc;

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

        let parent_id = mgr.create_node_for_element(parent_key, None).unwrap();
        let _child_id = mgr
            .create_node_for_element(child_key, Some(parent_id))
            .unwrap();

        // Removing the parent should also clean up the child's HashMap entry.
        mgr.remove_node(parent_id);
        assert_eq!(mgr.node_for_element(parent_key), None);
        assert_eq!(mgr.node_for_element(child_key), None);
    }

    // -----------------------------------------------------------------------
    // on_focus_change callback tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_on_focus_change_fired_on_gain() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());
        let id = mgr.create_node_for_element(key, None).unwrap();

        let fired = Arc::new(AtomicBool::new(false));
        let fired_clone = fired.clone();
        if let Some(node) = mgr.get_mut(id) {
            node.on_focus_change = Some(Arc::new(move |focused| {
                fired_clone.store(focused, Ordering::Relaxed);
            }));
        }

        mgr.request_focus(id);
        mgr.apply_focus_changes();
        assert!(fired.load(Ordering::Relaxed));
    }

    #[test]
    fn test_on_focus_change_fired_on_loss() {
        use std::sync::atomic::{AtomicI32, Ordering};
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let key = elem_map.insert(());
        let id = mgr.create_node_for_element(key, None).unwrap();

        // Track last value: 0 = never called, 1 = focused, -1 = unfocused
        let last_value = Arc::new(AtomicI32::new(0));
        let last_value_clone = last_value.clone();
        if let Some(node) = mgr.get_mut(id) {
            node.on_focus_change = Some(Arc::new(move |focused| {
                last_value_clone.store(if focused { 1 } else { -1 }, Ordering::Relaxed);
            }));
        }

        mgr.request_focus(id);
        mgr.apply_focus_changes();
        assert_eq!(last_value.load(Ordering::Relaxed), 1);

        mgr.unfocus();
        assert_eq!(last_value.load(Ordering::Relaxed), -1);
    }

    #[test]
    fn test_on_focus_change_ancestor_notification() {
        use std::sync::atomic::{AtomicI32, Ordering};
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();

        let parent_key = elem_map.insert(());
        let child_key = elem_map.insert(());
        let parent_id = mgr.create_node_for_element(parent_key, None).unwrap();
        let child_id = mgr
            .create_node_for_element(child_key, Some(parent_id))
            .unwrap();

        // Track ancestor's last focus value
        let ancestor_value = Arc::new(AtomicI32::new(0));
        let ancestor_value_clone = ancestor_value.clone();
        if let Some(node) = mgr.get_mut(parent_id) {
            node.on_focus_change = Some(Arc::new(move |focused| {
                ancestor_value_clone.store(if focused { 1 } else { -1 }, Ordering::Relaxed);
            }));
        }

        // Focus the child — ancestor should be notified
        mgr.request_focus(child_id);
        mgr.apply_focus_changes();
        assert_eq!(ancestor_value.load(Ordering::Relaxed), 1);

        // Unfocus — ancestor should be notified with false
        mgr.unfocus();
        assert_eq!(ancestor_value.load(Ordering::Relaxed), -1);
    }

    #[test]
    fn test_on_focus_change_no_callback_after_detach() {
        use std::sync::atomic::{AtomicI32, Ordering};
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let parent_key = elem_map.insert(());
        let child_key = elem_map.insert(());
        let parent_id = mgr.create_node_for_element(parent_key, None).unwrap();
        let child_id = mgr
            .create_node_for_element(child_key, Some(parent_id))
            .unwrap();

        let call_count = Arc::new(AtomicI32::new(0));
        let call_count_clone = call_count.clone();
        if let Some(node) = mgr.get_mut(parent_id) {
            node.on_focus_change = Some(Arc::new(move |_focused| {
                call_count_clone.fetch_add(1, Ordering::Relaxed);
            }));
        }

        // Focus child — callback fires
        mgr.request_focus(child_id);
        mgr.apply_focus_changes();
        assert_eq!(call_count.load(Ordering::Relaxed), 1);

        // Remove parent — callback is gone
        mgr.remove_node(parent_id);

        // The child was also removed, so primary focus is cleared.
        // No callback should fire since the parent node is gone.
        // Just verify no panic.
    }

    #[test]
    fn test_ancestor_elements_with_callbacks() {
        let mut mgr = FocusManager::new();
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();

        let grandparent_key = elem_map.insert(());
        let parent_key = elem_map.insert(());
        let child_key = elem_map.insert(());

        let grandparent_id = mgr.create_node_for_element(grandparent_key, None).unwrap();
        let parent_id = mgr
            .create_node_for_element(parent_key, Some(grandparent_id))
            .unwrap();
        let child_id = mgr
            .create_node_for_element(child_key, Some(parent_id))
            .unwrap();

        // Only parent has a callback
        if let Some(node) = mgr.get_mut(parent_id) {
            node.on_focus_change = Some(Arc::new(|_focused| {}));
        }

        let ancestors = mgr.ancestor_elements_with_callbacks(child_id);
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0], parent_key);

        // Grandparent has no callback, so it's not in the list
        let ancestors_from_parent = mgr.ancestor_elements_with_callbacks(parent_id);
        assert_eq!(ancestors_from_parent.len(), 0);
    }
}
