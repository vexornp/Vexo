//! Focus node types: [`FocusNodeId`] and [`FocusNodeData`].

use std::fmt;
use std::sync::Arc;

use crate::id::ElementKey;
use slotmap::new_key_type;

new_key_type! {
    /// Opaque slotmap key for focus nodes.
    ///
    /// Provides generational ABA protection: when a node is removed from
    /// the `SlotMap`, the generation increments, invalidating any stale keys.
    pub struct FocusNodeId;
}

/// Data stored for every focus node in the focus tree.
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
    /// Whether the element owning this node is a text input (e.g. `TextEdit`).
    ///
    /// Set from `ComponentState::requests_focus_on_click()` when a
    /// `StatefulElement` mounts. Used by the pipeline to decide whether the
    /// software keyboard should be shown: only the text input's *own* focus
    /// node returns `true`, never an ancestor (like a `ScrollView`) that merely
    /// *contains* a text input. This avoids an unbounded subtree walk that would
    /// incorrectly find a `TextEditRenderObject` beneath any focused ancestor.
    pub is_text_input: bool,
    /// Callback invoked when this node or a descendant gains/loses primary focus.
    /// Called with `true` when focus is gained, `false` when lost.
    /// Set by the Focus widget during mount.
    pub on_focus_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl fmt::Debug for FocusNodeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FocusNodeData")
            .field("element_key", &self.element_key)
            .field("parent", &self.parent)
            .field("children", &self.children)
            .field("can_request_focus", &self.can_request_focus)
            .field("skip_traversal", &self.skip_traversal)
            .field("is_text_input", &self.is_text_input)
            .field(
                "on_focus_change",
                &self.on_focus_change.as_ref().map(|_| "..."),
            )
            .finish()
    }
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
            is_text_input: false,
            on_focus_change: None,
        }
    }
}

impl Default for FocusNodeData {
    fn default() -> Self {
        Self::new()
    }
}
