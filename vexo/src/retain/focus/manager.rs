//! [`FocusManager`] — owns the focus tree and provides all focus operations.

use std::collections::HashMap;

use slotmap::{SlotMap, SecondaryMap};

use crate::retain::id::ElementKey;
use super::node::{FocusNodeId, FocusNodeData};
use super::scope::{FocusScopeData, UnfocusDisposition};

/// Owns the focus tree and provides operations for focus requests, unfocus,
/// reparenting, and scope-aware traversal.
///
/// The tree is stored in a single `SlotMap<FocusNodeId, FocusNodeData>`.
/// Scope nodes additionally carry `FocusScopeData` in a `SecondaryMap`.
/// The root scope is created on initialization and never removed.
pub struct FocusManager {
    /// All focus nodes (leaf nodes and scope nodes).
    nodes: SlotMap<FocusNodeId, FocusNodeData>,
    /// Extra data for scope nodes only. Entries exist iff the corresponding
    /// node has `is_scope == true`.
    scopes: SecondaryMap<FocusNodeId, FocusScopeData>,
    /// The root scope node. Created in `FocusManager::new()`.
    root_scope: FocusNodeId,
    /// The node that currently has primary focus, if any.
    primary_focus: Option<FocusNodeId>,
    /// Maps element keys to focus nodes. Used during the transition period
    /// from the flat `Option<ElementKey>` focus model.
    element_to_node: HashMap<ElementKey, FocusNodeId>,
}

impl FocusManager {
    /// Create a new `FocusManager` with a root scope node.
    ///
    /// The root scope has no parent and no element key.
    pub fn new() -> Self {
        let mut nodes = SlotMap::with_key();
        let mut scopes = SecondaryMap::new();

        let root_data = FocusNodeData::new_scope();
        let root_scope = nodes.insert(root_data);
        scopes.insert(root_scope, FocusScopeData::new());

        Self {
            nodes,
            scopes,
            root_scope,
            primary_focus: None,
            element_to_node: HashMap::new(),
        }
    }

    /// Return the root scope node id.
    pub fn root_scope(&self) -> FocusNodeId {
        self.root_scope
    }

    /// Return the node that currently has primary focus, if any.
    pub fn primary_focus(&self) -> Option<FocusNodeId> {
        self.primary_focus
    }

    /// Return the element key of the node with primary focus, if any.
    pub fn primary_focus_element(&self) -> Option<ElementKey> {
        self.primary_focus.and_then(|id| {
            self.nodes.get(id).and_then(|n| n.element_key)
        })
    }

    /// Return `true` if `node_id` currently has primary focus.
    pub fn has_primary_focus(&self, node_id: FocusNodeId) -> bool {
        self.primary_focus == Some(node_id)
    }

    /// Return `true` if `node_id` or any of its descendants has primary focus.
    pub fn has_focus(&self, node_id: FocusNodeId) -> bool {
        let Some(focused) = self.primary_focus else {
            return false;
        };
        if focused == node_id {
            return true;
        }
        // Walk up from the focused node to see if we reach node_id.
        self.is_ancestor_of(node_id, focused)
    }

    /// Create a new leaf focus node as a child of `parent_scope`.
    ///
    /// Returns the id of the newly created node.
    pub fn create_node(&mut self, parent_scope: FocusNodeId) -> FocusNodeId {
        self.create_node_internal(parent_scope, None)
    }

    /// Create a new leaf focus node with an associated element key.
    pub fn create_node_with_element(
        &mut self,
        parent_scope: FocusNodeId,
        element_key: ElementKey,
    ) -> FocusNodeId {
        self.create_node_internal(parent_scope, Some(element_key))
    }

    fn create_node_internal(
        &mut self,
        parent_scope: FocusNodeId,
        element_key: Option<ElementKey>,
    ) -> FocusNodeId {
        let mut data = FocusNodeData::new();
        data.parent = Some(parent_scope);
        data.element_key = element_key;
        let id = self.nodes.insert(data);

        // Register in parent's children list.
        if let Some(parent) = self.nodes.get_mut(parent_scope) {
            parent.children.push(id);
        }

        // Register element mapping if present.
        if let Some(ek) = element_key {
            self.element_to_node.insert(ek, id);
        }

        id
    }

    /// Create a new scope node as a child of `parent_scope`.
    ///
    /// Returns the id of the newly created scope.
    pub fn create_scope(&mut self, parent_scope: FocusNodeId) -> FocusNodeId {
        self.create_scope_internal(parent_scope, None)
    }

    /// Create a new scope node with an associated element key.
    pub fn create_scope_with_element(
        &mut self,
        parent_scope: FocusNodeId,
        element_key: ElementKey,
    ) -> FocusNodeId {
        self.create_scope_internal(parent_scope, Some(element_key))
    }

    fn create_scope_internal(
        &mut self,
        parent_scope: FocusNodeId,
        element_key: Option<ElementKey>,
    ) -> FocusNodeId {
        let mut data = FocusNodeData::new_scope();
        data.parent = Some(parent_scope);
        data.element_key = element_key;
        let id = self.nodes.insert(data);

        // Insert scope extension data.
        self.scopes.insert(id, FocusScopeData::new());

        // Register in parent's children list.
        if let Some(parent) = self.nodes.get_mut(parent_scope) {
            parent.children.push(id);
        }

        // Register element mapping if present.
        if let Some(ek) = element_key {
            self.element_to_node.insert(ek, id);
        }

        id
    }

    /// Remove a node from the focus tree.
    ///
    /// If the node has primary focus, it is unfocused first.
    /// If the node is a scope, all its children are also removed.
    /// The root scope cannot be removed.
    ///
    /// Returns `true` if the node was removed, `false` if it was not found
    /// or was the root scope.
    pub fn remove_node(&mut self, node_id: FocusNodeId) -> bool {
        // Cannot remove the root scope.
        if node_id == self.root_scope {
            return false;
        }

        let Some(node_data) = self.nodes.get(node_id) else {
            return false;
        };

        let is_scope = node_data.is_scope;
        let parent_id = node_data.parent;
        let element_key = node_data.element_key;

        // Collect children to remove (for scopes).
        let children_to_remove: Vec<FocusNodeId> = if is_scope {
            node_data.children.clone()
        } else {
            Vec::new()
        };

        // Clear primary focus if this node or a descendant has it.
        if self.has_focus(node_id) {
            self.primary_focus = None;
        }

        // Remove from parent's children list.
        if let Some(pid) = parent_id {
            if let Some(parent) = self.nodes.get_mut(pid) {
                parent.children.retain(|c| *c != node_id);
            }
        }

        // Remove from parent scope's focused_children.
        if let Some(pid) = parent_id {
            if let Some(scope) = self.scopes.get_mut(pid) {
                scope.focused_children.retain(|c| *c != node_id);
            }
        }

        // Remove element mapping.
        if let Some(ek) = element_key {
            self.element_to_node.remove(&ek);
        }

        // Remove scope extension data.
        if is_scope {
            self.scopes.remove(node_id);
        }

        // Remove the node itself.
        self.nodes.remove(node_id);

        // Recursively remove children (for scopes).
        for child_id in children_to_remove {
            self.remove_node_recursive(child_id);
        }

        true
    }

    fn remove_node_recursive(&mut self, node_id: FocusNodeId) {
        let Some(node_data) = self.nodes.get(node_id) else {
            return;
        };

        let is_scope = node_data.is_scope;
        let element_key = node_data.element_key;
        let children: Vec<FocusNodeId> = node_data.children.clone();

        // Clear primary focus if needed.
        if self.primary_focus == Some(node_id) {
            self.primary_focus = None;
        }

        // Remove element mapping.
        if let Some(ek) = element_key {
            self.element_to_node.remove(&ek);
        }

        // Remove scope extension data.
        if is_scope {
            self.scopes.remove(node_id);
        }

        // Remove the node itself.
        self.nodes.remove(node_id);

        // Recursively remove children.
        for child_id in children {
            self.remove_node_recursive(child_id);
        }
    }

    /// Request focus on `node_id`.
    ///
    /// This is an immediate (non-deferred) focus change. Returns the
    /// previously focused node, if any.
    ///
    /// If `can_request_focus` is `false` on the target node, this is a
    /// no-op and returns `None`.
    pub fn request_focus(&mut self, node_id: FocusNodeId) -> Option<FocusNodeId> {
        let can_request = self.nodes.get(node_id)
            .map(|n| n.can_request_focus)
            .unwrap_or(false);

        if !can_request {
            return None;
        }

        let previous = self.primary_focus;
        self.primary_focus = Some(node_id);

        // Update scope focused-children memory.
        self.set_as_focused_child_for_scope(node_id);

        previous
    }

    /// Request focus by element key.
    ///
    /// Looks up the focus node associated with `element_key` and requests
    /// focus on it. Returns the previously focused node, if any.
    /// Returns `None` if no node is associated with the element key or
    /// if the node cannot request focus.
    pub fn request_focus_by_element(&mut self, element_key: ElementKey) -> Option<FocusNodeId> {
        let node_id = self.element_to_node.get(&element_key).copied()?;
        self.request_focus(node_id)
    }

    /// Unfocus the current primary focus.
    ///
    /// Equivalent to `unfocus_with_disposition(UnfocusDisposition::default())`.
    pub fn unfocus(&mut self) -> Option<FocusNodeId> {
        self.unfocus_with_disposition(UnfocusDisposition::default())
    }

    /// Unfocus with a specific disposition.
    ///
    /// - `PreviouslyFocusedChild`: Move focus to the nearest ancestor scope's
    ///   previously focused child. If none, focus falls back to the root scope.
    /// - `Scope`: Move focus to the nearest ancestor scope node itself.
    pub fn unfocus_with_disposition(&mut self, disposition: UnfocusDisposition) -> Option<FocusNodeId> {
        let previous = self.primary_focus;
        let Some(focused_id) = previous else {
            return None;
        };

        // Find the enclosing scope of the currently focused node.
        let scope_id = self.enclosing_scope(focused_id);

        match disposition {
            UnfocusDisposition::PreviouslyFocusedChild => {
                // Walk up the scope tree looking for a scope with a
                // focused_child memory that is not the node being unfocused.
                let mut current_scope = scope_id;
                let mut new_focus: Option<FocusNodeId> = None;

                while let Some(sid) = current_scope {
                    if let Some(scope_data) = self.scopes.get(sid) {
                        // Try to find a focused child that is not the one being unfocused.
                        for &child_id in scope_data.focused_children.iter().rev() {
                            if child_id != focused_id && self.nodes.contains_key(child_id) {
                                new_focus = Some(child_id);
                                break;
                            }
                        }
                        if new_focus.is_some() {
                            break;
                        }
                    }
                    // Move to parent scope.
                    current_scope = self.nodes.get(sid)
                        .and_then(|n| n.parent)
                        .filter(|&pid| self.nodes.get(pid).map(|n| n.is_scope).unwrap_or(false));
                }

                if let Some(nf) = new_focus {
                    self.primary_focus = Some(nf);
                    self.set_as_focused_child_for_scope(nf);
                } else {
                    // No previously focused child found; clear primary focus.
                    self.primary_focus = None;
                }
            }
            UnfocusDisposition::Scope => {
                // Focus the enclosing scope node itself.
                if let Some(sid) = scope_id {
                    self.primary_focus = Some(sid);
                    self.set_as_focused_child_for_scope(sid);
                } else {
                    self.primary_focus = None;
                }
            }
        }

        previous
    }

    /// Move `node_id` from its current parent to `new_parent_scope`.
    ///
    /// Returns `true` if the reparent was successful.
    pub fn reparent(&mut self, node_id: FocusNodeId, new_parent_scope: FocusNodeId) -> bool {
        if node_id == self.root_scope {
            return false;
        }

        let Some(node_data) = self.nodes.get(node_id) else {
            return false;
        };

        let old_parent = node_data.parent;

        // Remove from old parent's children list.
        if let Some(old_pid) = old_parent {
            if let Some(old_parent_node) = self.nodes.get_mut(old_pid) {
                old_parent_node.children.retain(|c| *c != node_id);
            }
            // Remove from old parent scope's focused_children.
            if let Some(scope) = self.scopes.get_mut(old_pid) {
                scope.focused_children.retain(|c| *c != node_id);
            }
        }

        // Update parent reference.
        if let Some(node_data) = self.nodes.get_mut(node_id) {
            node_data.parent = Some(new_parent_scope);
        }

        // Add to new parent's children list.
        if let Some(new_parent_node) = self.nodes.get_mut(new_parent_scope) {
            new_parent_node.children.push(node_id);
        }

        true
    }

    /// Return the enclosing scope of `node_id`.
    ///
    /// If `node_id` is itself a scope, return its parent scope.
    /// If `node_id` is a leaf, return the nearest ancestor scope.
    /// Returns `None` for the root scope.
    pub fn enclosing_scope(&self, node_id: FocusNodeId) -> Option<FocusNodeId> {
        let node_data = self.nodes.get(node_id)?;

        if node_data.is_scope {
            // A scope's enclosing scope is its parent (if the parent is a scope).
            node_data.parent.filter(|&pid| {
                self.nodes.get(pid).map(|n| n.is_scope).unwrap_or(false)
            })
        } else {
            // A leaf's enclosing scope is its parent.
            node_data.parent
        }
    }

    /// Return the nearest ancestor scope (including `node_id` itself if it
    /// is a scope).
    pub fn nearest_parent_scope(&self, node_id: FocusNodeId) -> Option<FocusNodeId> {
        let mut current = Some(node_id);
        while let Some(cid) = current {
            if let Some(data) = self.nodes.get(cid) {
                if data.is_scope {
                    return Some(cid);
                }
                current = data.parent;
            } else {
                return None;
            }
        }
        None
    }

    /// Record `node_id` as the most-recently focused child in its
    /// enclosing scope's `focused_children` stack.
    fn set_as_focused_child_for_scope(&mut self, node_id: FocusNodeId) {
        let parent_scope = self.nodes.get(node_id)
            .and_then(|n| n.parent);

        let Some(scope_id) = parent_scope else {
            return;
        };

        // Remove any previous occurrence of node_id in the focused_children stack.
        if let Some(scope) = self.scopes.get_mut(scope_id) {
            scope.focused_children.retain(|c| *c != node_id);
            scope.focused_children.push(node_id);
        }
    }

    /// Return a reference to the data for `node_id`, or `None` if not found.
    pub fn get_node(&self, node_id: FocusNodeId) -> Option<&FocusNodeData> {
        self.nodes.get(node_id)
    }

    /// Return a mutable reference to the data for `node_id`, or `None` if not found.
    pub fn get_node_mut(&mut self, node_id: FocusNodeId) -> Option<&mut FocusNodeData> {
        self.nodes.get_mut(node_id)
    }

    /// Return a reference to the scope data for `node_id`, or `None` if not
    /// a scope or not found.
    pub fn get_scope(&self, node_id: FocusNodeId) -> Option<&FocusScopeData> {
        self.scopes.get(node_id)
    }

    /// Return a mutable reference to the scope data for `node_id`.
    pub fn get_scope_mut(&mut self, node_id: FocusNodeId) -> Option<&mut FocusScopeData> {
        self.scopes.get_mut(node_id)
    }

    /// Return `true` if `node_id` exists in the focus tree.
    pub fn contains(&self, node_id: FocusNodeId) -> bool {
        self.nodes.contains_key(node_id)
    }

    /// Return the focus node id associated with `element_key`, if any.
    pub fn node_for_element(&self, element_key: ElementKey) -> Option<FocusNodeId> {
        self.element_to_node.get(&element_key).copied()
    }

    /// Return `true` if the element with `element_key` currently has primary focus.
    pub fn is_element_focused(&self, element_key: ElementKey) -> bool {
        self.element_to_node.get(&element_key)
            .map(|&nid| self.primary_focus == Some(nid))
            .unwrap_or(false)
    }

    /// Check if `ancestor_id` is an ancestor of `descendant_id`.
    fn is_ancestor_of(&self, ancestor_id: FocusNodeId, descendant_id: FocusNodeId) -> bool {
        let mut current = self.nodes.get(descendant_id)
            .and_then(|n| n.parent);
        while let Some(pid) = current {
            if pid == ancestor_id {
                return true;
            }
            current = self.nodes.get(pid)
                .and_then(|n| n.parent);
        }
        false
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_manager_new() {
        let mgr = FocusManager::new();
        // Root scope exists and is a scope.
        assert!(mgr.contains(mgr.root_scope()));
        let root = mgr.get_node(mgr.root_scope()).unwrap();
        assert!(root.is_scope);
        assert!(root.parent.is_none());
        assert!(root.children.is_empty());
        // No primary focus initially.
        assert!(mgr.primary_focus().is_none());
        assert!(mgr.primary_focus_element().is_none());
        // Root scope has scope data.
        assert!(mgr.get_scope(mgr.root_scope()).is_some());
    }

    #[test]
    fn test_create_node() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        let node = mgr.create_node(root);
        assert!(mgr.contains(node));

        // Node is a child of root.
        let root_data = mgr.get_node(root).unwrap();
        assert!(root_data.children.contains(&node));

        // Node defaults.
        let node_data = mgr.get_node(node).unwrap();
        assert!(!node_data.is_scope);
        assert_eq!(node_data.parent, Some(root));
        assert!(node_data.can_request_focus);
        assert!(!node_data.skip_traversal);
        assert!(node_data.element_key.is_none());
    }

    #[test]
    fn test_create_scope() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        let scope = mgr.create_scope(root);
        assert!(mgr.contains(scope));

        // Scope is a child of root.
        let root_data = mgr.get_node(root).unwrap();
        assert!(root_data.children.contains(&scope));

        // Scope node data.
        let scope_data = mgr.get_node(scope).unwrap();
        assert!(scope_data.is_scope);
        assert_eq!(scope_data.parent, Some(root));

        // Scope extension data exists.
        assert!(mgr.get_scope(scope).is_some());
        assert!(mgr.get_scope(scope).unwrap().focused_children.is_empty());
    }

    #[test]
    fn test_request_focus() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        let node_a = mgr.create_node(root);
        let node_b = mgr.create_node(root);

        // Request focus on A.
        let prev = mgr.request_focus(node_a);
        assert!(prev.is_none());
        assert_eq!(mgr.primary_focus(), Some(node_a));
        assert!(mgr.has_primary_focus(node_a));
        assert!(!mgr.has_primary_focus(node_b));

        // Request focus on B — should return A as previous.
        let prev = mgr.request_focus(node_b);
        assert_eq!(prev, Some(node_a));
        assert_eq!(mgr.primary_focus(), Some(node_b));
        assert!(mgr.has_primary_focus(node_b));
    }

    #[test]
    fn test_request_focus_can_request_focus_false() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        let node_a = mgr.create_node(root);
        let node_b = mgr.create_node(root);

        // Focus A first.
        mgr.request_focus(node_a);

        // Make B unable to request focus.
        mgr.get_node_mut(node_b).unwrap().can_request_focus = false;

        // Requesting focus on B should be a no-op.
        let prev = mgr.request_focus(node_b);
        assert!(prev.is_none());
        assert_eq!(mgr.primary_focus(), Some(node_a));
    }

    #[test]
    fn test_unfocus() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        let node = mgr.create_node(root);
        mgr.request_focus(node);
        assert_eq!(mgr.primary_focus(), Some(node));

        let prev = mgr.unfocus();
        assert_eq!(prev, Some(node));
        assert!(mgr.primary_focus().is_none());
    }

    #[test]
    fn test_remove_node() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        let node = mgr.create_node(root);
        mgr.request_focus(node);

        // Remove the focused node.
        assert!(mgr.remove_node(node));
        assert!(!mgr.contains(node));
        assert!(mgr.primary_focus().is_none());

        // Root no longer lists it as a child.
        let root_data = mgr.get_node(root).unwrap();
        assert!(!root_data.children.contains(&node));

        // Cannot remove the root scope.
        assert!(!mgr.remove_node(root));
    }

    #[test]
    fn test_reparent() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        let scope_a = mgr.create_scope(root);
        let scope_b = mgr.create_scope(root);
        let node = mgr.create_node(scope_a);

        // Node is a child of scope_a.
        assert!(mgr.get_node(scope_a).unwrap().children.contains(&node));
        assert_eq!(mgr.get_node(node).unwrap().parent, Some(scope_a));

        // Reparent to scope_b.
        assert!(mgr.reparent(node, scope_b));

        // Node is now a child of scope_b.
        assert!(!mgr.get_node(scope_a).unwrap().children.contains(&node));
        assert!(mgr.get_node(scope_b).unwrap().children.contains(&node));
        assert_eq!(mgr.get_node(node).unwrap().parent, Some(scope_b));

        // Cannot reparent the root scope.
        assert!(!mgr.reparent(root, scope_a));
    }

    #[test]
    fn test_enclosing_scope() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        let scope = mgr.create_scope(root);
        let node = mgr.create_node(scope);

        // Leaf node's enclosing scope is its parent (the scope).
        assert_eq!(mgr.enclosing_scope(node), Some(scope));

        // Scope's enclosing scope is root.
        assert_eq!(mgr.enclosing_scope(scope), Some(root));

        // Root scope has no enclosing scope.
        assert!(mgr.enclosing_scope(root).is_none());
    }

    #[test]
    fn test_scope_focused_child_memory() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        let scope = mgr.create_scope(root);
        let node_a = mgr.create_node(scope);
        let node_b = mgr.create_node(scope);

        // Focus A — it should be recorded in scope's focused_children.
        mgr.request_focus(node_a);
        let scope_data = mgr.get_scope(scope).unwrap();
        assert_eq!(scope_data.focused_child(), Some(node_a));

        // Focus B — B should replace A as the focused child.
        mgr.request_focus(node_b);
        let scope_data = mgr.get_scope(scope).unwrap();
        assert_eq!(scope_data.focused_child(), Some(node_b));
        // A should still be in the stack (just not at the top).
        assert!(scope_data.focused_children.contains(&node_a));
    }

    #[test]
    fn test_unfocus_restore_previous() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        let scope = mgr.create_scope(root);
        let node_a = mgr.create_node(scope);
        let node_b = mgr.create_node(scope);

        // Focus A, then B.
        mgr.request_focus(node_a);
        mgr.request_focus(node_b);
        assert_eq!(mgr.primary_focus(), Some(node_b));

        // Unfocus with PreviouslyFocusedChild — should restore A.
        let prev = mgr.unfocus_with_disposition(UnfocusDisposition::PreviouslyFocusedChild);
        assert_eq!(prev, Some(node_b));
        assert_eq!(mgr.primary_focus(), Some(node_a));
    }

    #[test]
    fn test_element_to_node_mapping() {
        let mut mgr = FocusManager::new();
        let root = mgr.root_scope();

        // We need an ElementKey. Since ElementKey is a slotmap key,
        // we create one via a temporary SlotMap.
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = SlotMap::with_key();
        let ek = elem_map.insert(());

        let node = mgr.create_node_with_element(root, ek);

        // Mapping exists.
        assert_eq!(mgr.node_for_element(ek), Some(node));
        assert!(!mgr.is_element_focused(ek));

        // Focus the node.
        mgr.request_focus(node);
        assert!(mgr.is_element_focused(ek));

        // Remove the node — mapping should be gone.
        mgr.remove_node(node);
        assert!(mgr.node_for_element(ek).is_none());
        assert!(!mgr.is_element_focused(ek));
    }
}
