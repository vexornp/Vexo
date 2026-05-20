use slotmap::SlotMap;
use slotmap::SecondaryMap;

use super::key::FocusNodeKey;
use super::node::FocusNodeData;
use super::scope::{FocusScopeData, UnfocusDisposition};
use super::traversal::TraversalPolicy;
use crate::retain::id::ElementKey;

/// Central focus management type.
///
/// Owns the focus tree (a slotmap of nodes), manages primary focus,
/// handles focus requests, unfocus, traversal, deferred callbacks,
/// and keyboard tokens.
pub struct FocusManager {
    /// All focus nodes, including scopes.
    nodes: SlotMap<FocusNodeKey, FocusNodeData>,
    /// Extra data for scope nodes only.
    scopes: SecondaryMap<FocusNodeKey, FocusScopeData>,
    /// Currently focused node.
    primary_focus: Option<FocusNodeKey>,
    /// Top-level scope that contains all other nodes.
    root_scope: FocusNodeKey,
    /// Deferred callback queue: nodes that gained focus.
    pending_focus_gained: Vec<FocusNodeKey>,
    /// Deferred callback queue: nodes that lost focus.
    pending_focus_lost: Vec<FocusNodeKey>,
}

impl FocusManager {
    /// Create a new FocusManager with a root scope node.
    pub fn new() -> Self {
        let mut nodes: SlotMap<FocusNodeKey, FocusNodeData> = SlotMap::with_key();
        let mut scopes: SecondaryMap<FocusNodeKey, FocusScopeData> = SecondaryMap::new();

        // Create the root scope node.
        let root_key = nodes.insert(FocusNodeData::new());
        scopes.insert(root_key, FocusScopeData::new());

        Self {
            nodes,
            scopes,
            primary_focus: None,
            root_scope: root_key,
            pending_focus_gained: Vec::new(),
            pending_focus_lost: Vec::new(),
        }
    }

    /// Create a focus node attached to `parent` (or root_scope if None).
    pub fn create_node(&mut self, parent: Option<FocusNodeKey>) -> FocusNodeKey {
        let parent_key = parent.unwrap_or(self.root_scope);
        let mut data = FocusNodeData::new();
        data.parent = Some(parent_key);
        let key = self.nodes.insert(data);
        if let Some(parent_node) = self.nodes.get_mut(parent_key) {
            parent_node.children.push(key);
        }
        key
    }

    /// Create a scope node attached to `parent` (or root_scope if None).
    pub fn create_scope(&mut self, parent: Option<FocusNodeKey>) -> FocusNodeKey {
        let parent_key = parent.unwrap_or(self.root_scope);
        let mut data = FocusNodeData::new();
        data.parent = Some(parent_key);
        let key = self.nodes.insert(data);
        self.scopes.insert(key, FocusScopeData::new());
        if let Some(parent_node) = self.nodes.get_mut(parent_key) {
            parent_node.children.push(key);
        }
        key
    }

    /// Remove a node from the tree.
    ///
    /// Removes it from its parent's children list, clears primary focus if
    /// the removed node was focused, and removes scope data if present.
    pub fn remove_node(&mut self, key: FocusNodeKey) {
        // Remove from parent's children list.
        if let Some(node) = self.nodes.get(key) {
            if let Some(parent_key) = node.parent {
                if let Some(parent_node) = self.nodes.get_mut(parent_key) {
                    parent_node.children.retain(|&c| c != key);
                }
                // If parent is a scope, clear its focused_child if it points to us.
                if let Some(scope) = self.scopes.get_mut(parent_key) {
                    if scope.focused_child == Some(key) {
                        scope.focused_child = None;
                    }
                    scope.focused_child_history.retain(|&c| c != key);
                }
            }
        }

        // Clear primary focus if this node was focused.
        if self.primary_focus == Some(key) {
            self.primary_focus = None;
        }

        // Remove scope data if present.
        self.scopes.remove(key);

        // Remove the node itself.
        self.nodes.remove(key);
    }

    /// Request focus on a node.
    ///
    /// This is the main focus change method. It:
    /// 1. Checks can_request_focus, returns if false
    /// 2. Updates enclosing scopes' focused_child chain
    /// 3. Sets primary_focus
    /// 4. Sets keyboard_token = user_initiated
    /// 5. Queues on_focus_lost for old, on_focus_gained for new (if different)
    pub fn request_focus(&mut self, key: FocusNodeKey, user_initiated: bool) {
        // Step 1: Check can_request_focus.
        if !self.can_request_focus(key) {
            return;
        }

        // If already the primary focus, just update the keyboard token.
        if self.primary_focus == Some(key) {
            if let Some(node) = self.nodes.get_mut(key) {
                node.keyboard_token = user_initiated;
            }
            return;
        }

        let old_focus = self.primary_focus;

        // Step 2: Update enclosing scopes' focused_child chain.
        self.set_focused_child_chain(key);

        // Step 3: Set primary_focus.
        self.primary_focus = Some(key);

        // Step 4: Set keyboard_token.
        if let Some(node) = self.nodes.get_mut(key) {
            node.keyboard_token = user_initiated;
        }

        // Step 5: Queue deferred callbacks.
        if let Some(old) = old_focus {
            self.pending_focus_lost.push(old);
        }
        self.pending_focus_gained.push(key);
    }

    /// Unfocus the current primary focus.
    ///
    /// If disposition is `Clear`, primary focus becomes None.
    /// If `RestorePrevious`, the previous focused child in the enclosing scope
    /// is restored.
    pub fn unfocus(&mut self, disposition: UnfocusDisposition) {
        let current = match self.primary_focus {
            Some(k) => k,
            None => return,
        };

        // Find the enclosing scope.
        let scope_key = self.enclosing_scope(current);

        match disposition {
            UnfocusDisposition::Clear => {
                // Clear focused_child in the enclosing scope.
                if let Some(sk) = scope_key {
                    if let Some(scope) = self.scopes.get_mut(sk) {
                        scope.focused_child = None;
                    }
                }
                self.primary_focus = None;
                self.pending_focus_lost.push(current);
            }
            UnfocusDisposition::RestorePrevious => {
                // Try to restore from the enclosing scope's history.
                let restored = if let Some(sk) = scope_key {
                    self.scopes.get_mut(sk).and_then(|scope| scope.pop_focused_child())
                } else {
                    None
                };

                if let Some(restored_key) = restored {
                    // Update the focused_child chain for the restored node.
                    self.set_focused_child_chain(restored_key);
                    self.primary_focus = Some(restored_key);
                    self.pending_focus_lost.push(current);
                    self.pending_focus_gained.push(restored_key);
                } else {
                    // No previous focus to restore; just clear.
                    self.primary_focus = None;
                    self.pending_focus_lost.push(current);
                }
            }
        }
    }

    /// Fire deferred focus callbacks and clear the queues.
    ///
    /// This should be called once per frame after all focus changes have been
    /// requested. Lost callbacks fire before gained callbacks.
    pub fn dispatch_focus_changes(&mut self) {
        // Take ownership of the pending queues to avoid borrow issues.
        let lost: Vec<FocusNodeKey> = self.pending_focus_lost.drain(..).collect();
        let gained: Vec<FocusNodeKey> = self.pending_focus_gained.drain(..).collect();

        // Fire on_focus_lost callbacks first.
        for key in lost {
            if let Some(node) = self.nodes.get(key) {
                if let Some(ref cb) = node.on_focus_lost {
                    cb();
                }
            }
        }

        // Then fire on_focus_gained callbacks.
        for key in gained {
            if let Some(node) = self.nodes.get(key) {
                if let Some(ref cb) = node.on_focus_gained {
                    cb();
                }
            }
        }
    }

    /// Returns the currently focused node key, if any.
    pub fn primary_focus(&self) -> Option<FocusNodeKey> {
        self.primary_focus
    }

    /// Returns the ElementKey of the currently focused node, if any.
    pub fn focused_element(&self) -> Option<ElementKey> {
        self.primary_focus.and_then(|key| {
            self.nodes.get(key).and_then(|node| node.element_key)
        })
    }

    /// Returns true if `key` is the primary focus.
    pub fn is_focused(&self, key: FocusNodeKey) -> bool {
        self.primary_focus == Some(key)
    }

    /// Returns true if `key` is on the focus chain (i.e., is the primary
    /// focus or is a scope whose focused_child leads to the primary focus).
    pub fn has_focus(&self, key: FocusNodeKey) -> bool {
        let chain = self.focus_chain();
        chain.contains(&key)
    }

    /// Returns the focus chain: path from primary focus up to root scope.
    pub fn focus_chain(&self) -> Vec<FocusNodeKey> {
        let mut chain = Vec::new();
        let mut current = self.primary_focus;
        while let Some(key) = current {
            chain.push(key);
            current = self.nodes.get(key).and_then(|node| node.parent);
        }
        chain
    }

    /// Returns true if the node can request focus.
    pub fn can_request_focus(&self, key: FocusNodeKey) -> bool {
        self.nodes.get(key).map_or(false, |n| n.can_request_focus)
    }

    /// Returns true if the node should be skipped during traversal.
    pub fn skip_traversal(&self, key: FocusNodeKey) -> bool {
        self.nodes.get(key).map_or(true, |n| n.skip_traversal)
    }

    /// Returns true if the node is a scope.
    pub fn is_scope(&self, key: FocusNodeKey) -> bool {
        self.scopes.get(key).is_some()
    }

    /// Returns the children of a node.
    pub fn children(&self, key: FocusNodeKey) -> Vec<FocusNodeKey> {
        self.nodes.get(key).map_or(Vec::new(), |n| n.children.clone())
    }

    /// Returns the root scope key.
    pub fn root_scope(&self) -> FocusNodeKey {
        self.root_scope
    }

    /// Walk up from `key` to find the nearest enclosing scope.
    ///
    /// The node itself is not considered; we look at ancestors only.
    pub fn enclosing_scope(&self, key: FocusNodeKey) -> Option<FocusNodeKey> {
        let mut current = self.nodes.get(key).and_then(|n| n.parent);
        while let Some(ancestor) = current {
            if self.scopes.get(ancestor).is_some() {
                return Some(ancestor);
            }
            current = self.nodes.get(ancestor).and_then(|n| n.parent);
        }
        None
    }

    /// Returns a mutable reference to a node's data.
    pub fn get_node_mut(&mut self, key: FocusNodeKey) -> Option<&mut FocusNodeData> {
        self.nodes.get_mut(key)
    }

    /// Returns a mutable reference to a node's scope data, if it is a scope.
    pub fn get_scope_mut(&mut self, key: FocusNodeKey) -> Option<&mut FocusScopeData> {
        self.scopes.get_mut(key)
    }

    /// Consume the keyboard token for a node, returning its previous value.
    pub fn consume_keyboard_token(&mut self, key: FocusNodeKey) -> bool {
        self.nodes.get_mut(key).map_or(false, |n| n.consume_keyboard_token())
    }

    /// Tab navigation: traverse forward to the next focusable node.
    pub fn traverse_forward(&mut self) -> Option<FocusNodeKey> {
        let current = self.primary_focus.unwrap_or(self.root_scope);

        // Find the enclosing scope of the current node.
        let scope_key = self.enclosing_scope(current).unwrap_or(self.root_scope);

        // Get the traversal policy for the scope.
        let policy = self.scopes.get(scope_key)
            .map_or(TraversalPolicy::WidgetOrder, |s| s.traversal_policy.clone());

        // Try to find the next node in the current scope.
        let next = if current == self.root_scope {
            // If nothing is focused, find the first focusable node.
            policy.find_first(self.root_scope, self)
        } else {
            policy.next(current, scope_key, self)
        };

        if let Some(next_key) = next {
            self.request_focus(next_key, false);
            return Some(next_key);
        }

        // If at boundary, try parent scope.
        if let Some(parent_scope) = self.enclosing_scope(scope_key) {
            let parent_policy = self.scopes.get(parent_scope)
                .map_or(TraversalPolicy::WidgetOrder, |s| s.traversal_policy.clone());
            if let Some(next_key) = parent_policy.next(scope_key, parent_scope, self) {
                self.request_focus(next_key, false);
                return Some(next_key);
            }
        }

        // Wrap around in root scope.
        let root_policy = self.scopes.get(self.root_scope)
            .map_or(TraversalPolicy::WidgetOrder, |s| s.traversal_policy.clone());
        if let Some(first) = root_policy.find_first(self.root_scope, self) {
            self.request_focus(first, false);
            return Some(first);
        }

        None
    }

    /// Shift+Tab navigation: traverse backward to the previous focusable node.
    pub fn traverse_backward(&mut self) -> Option<FocusNodeKey> {
        let current = self.primary_focus.unwrap_or(self.root_scope);

        // Find the enclosing scope of the current node.
        let scope_key = self.enclosing_scope(current).unwrap_or(self.root_scope);

        // Get the traversal policy for the scope.
        let policy = self.scopes.get(scope_key)
            .map_or(TraversalPolicy::WidgetOrder, |s| s.traversal_policy.clone());

        // Try to find the previous node in the current scope.
        let prev = if current == self.root_scope {
            // If nothing is focused, find the last focusable node.
            policy.find_last(self.root_scope, self)
        } else {
            policy.previous(current, scope_key, self)
        };

        if let Some(prev_key) = prev {
            self.request_focus(prev_key, false);
            return Some(prev_key);
        }

        // If at boundary, try parent scope.
        if let Some(parent_scope) = self.enclosing_scope(scope_key) {
            let parent_policy = self.scopes.get(parent_scope)
                .map_or(TraversalPolicy::WidgetOrder, |s| s.traversal_policy.clone());
            if let Some(prev_key) = parent_policy.previous(scope_key, parent_scope, self) {
                self.request_focus(prev_key, false);
                return Some(prev_key);
            }
        }

        // Wrap around in root scope.
        let root_policy = self.scopes.get(self.root_scope)
            .map_or(TraversalPolicy::WidgetOrder, |s| s.traversal_policy.clone());
        if let Some(last) = root_policy.find_last(self.root_scope, self) {
            self.request_focus(last, false);
            return Some(last);
        }

        None
    }

    /// Set the element key for a node.
    pub fn set_element_key(&mut self, key: FocusNodeKey, element_key: Option<ElementKey>) {
        if let Some(node) = self.nodes.get_mut(key) {
            node.element_key = element_key;
        }
    }

    /// Set the focus callbacks for a node.
    pub fn set_callbacks(
        &mut self,
        key: FocusNodeKey,
        on_focus_gained: Option<Box<dyn Fn()>>,
        on_focus_lost: Option<Box<dyn Fn()>>,
    ) {
        if let Some(node) = self.nodes.get_mut(key) {
            node.on_focus_gained = on_focus_gained;
            node.on_focus_lost = on_focus_lost;
        }
    }

    /// Set whether a node can request focus.
    pub fn set_can_request_focus(&mut self, key: FocusNodeKey, value: bool) {
        if let Some(node) = self.nodes.get_mut(key) {
            node.can_request_focus = value;
        }
    }

    /// Set whether a node should be skipped during traversal.
    pub fn set_skip_traversal(&mut self, key: FocusNodeKey, value: bool) {
        if let Some(node) = self.nodes.get_mut(key) {
            node.skip_traversal = value;
        }
    }

    /// Set the traversal policy for a scope node.
    pub fn set_traversal_policy(&mut self, key: FocusNodeKey, policy: TraversalPolicy) {
        if let Some(scope) = self.scopes.get_mut(key) {
            scope.traversal_policy = policy;
        }
    }

    /// Walk up from `key`, updating each enclosing scope's focused_child.
    fn set_focused_child_chain(&mut self, key: FocusNodeKey) {
        let mut current = Some(key);
        while let Some(ck) = current {
            // Find the parent scope of the current node.
            let parent_key = self.nodes.get(ck).and_then(|n| n.parent);
            if let Some(pk) = parent_key {
                if self.scopes.get(pk).is_some() {
                    // Parent is a scope; update its focused_child.
                    if let Some(scope) = self.scopes.get_mut(pk) {
                        scope.push_focused_child(ck);
                    }
                }
            }
            current = parent_key;
        }
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}
