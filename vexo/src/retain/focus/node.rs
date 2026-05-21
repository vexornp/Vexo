//! Focus node types: [`FocusNodeId`] and [`FocusNodeData`].

use slotmap::new_key_type;
use crate::retain::id::ElementKey;

new_key_type! {
    /// Opaque slotmap key for focus nodes.
    ///
    /// Provides generational ABA protection: when a node is removed from
    /// the `SlotMap`, the generation increments, invalidating any stale keys.
    pub struct FocusNodeId;
}

/// Data stored for every focus node (both leaf nodes and scope nodes).
///
/// Scope nodes additionally carry [`FocusScopeData`] in a `SecondaryMap`
/// keyed by the same `FocusNodeId`.
#[derive(Debug, Clone)]
pub struct FocusNodeData {
    /// The element this node is associated with, if any.
    /// During the transition period, this maps back to the element tree.
    pub element_key: Option<ElementKey>,
    /// Parent node in the focus tree. `None` only for the root scope.
    pub parent: Option<FocusNodeId>,
    /// Child nodes in the focus tree. For scope nodes, these are the
    /// nodes that belong to this scope.
    pub children: Vec<FocusNodeId>,
    /// Whether this node can receive focus via `request_focus()`.
    /// Defaults to `true`. When `false`, `request_focus()` is a no-op.
    pub can_request_focus: bool,
    /// Whether this node should be skipped during directional traversal
    /// (Tab / Shift+Tab). Defaults to `false`.
    pub skip_traversal: bool,
    /// Whether this node is a scope node. Scope nodes carry extra data
    /// in the `SecondaryMap<FocusNodeId, FocusScopeData>`.
    pub is_scope: bool,
}

impl FocusNodeData {
    /// Create a new leaf focus node with default values.
    pub fn new() -> Self {
        Self {
            element_key: None,
            parent: None,
            children: Vec::new(),
            can_request_focus: true,
            skip_traversal: false,
            is_scope: false,
        }
    }

    /// Create a new scope focus node.
    ///
    /// Sets `is_scope = true`. The caller must also insert a
    /// [`FocusScopeData`] entry into the `SecondaryMap`.
    pub fn new_scope() -> Self {
        Self {
            element_key: None,
            parent: None,
            children: Vec::new(),
            can_request_focus: true,
            skip_traversal: false,
            is_scope: true,
        }
    }
}

impl Default for FocusNodeData {
    fn default() -> Self {
        Self::new()
    }
}
