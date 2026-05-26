//! Focus node types: [`FocusNodeId`] and [`FocusNodeData`].

use slotmap::new_key_type;
use crate::id::ElementKey;

new_key_type! {
    /// Opaque slotmap key for focus nodes.
    ///
    /// Provides generational ABA protection: when a node is removed from
    /// the `SlotMap`, the generation increments, invalidating any stale keys.
    pub struct FocusNodeId;
}

/// Data stored for every focus node in the focus tree.
#[derive(Debug, Clone)]
pub struct FocusNodeData {
    /// The element this node is associated with, if any.
    pub element_key: Option<ElementKey>,
    /// Parent node in the focus tree. `None` only for the root node.
    pub parent: Option<FocusNodeId>,
    /// Child nodes in the focus tree.
    pub children: Vec<FocusNodeId>,
    /// Whether this node can receive focus via `request_focus()`.
    /// Defaults to `true`. When `false`, `request_focus()` is a no-op.
    pub can_request_focus: bool,
    /// Whether this node should be skipped during directional traversal
    /// (Tab / Shift+Tab). Defaults to `false`.
    pub skip_traversal: bool,
}

impl FocusNodeData {
    /// Create a new focus node with default values.
    pub fn new() -> Self {
        Self {
            element_key: None,
            parent: None,
            children: Vec::new(),
            can_request_focus: true,
            skip_traversal: false,
        }
    }
}

impl Default for FocusNodeData {
    fn default() -> Self {
        Self::new()
    }
}