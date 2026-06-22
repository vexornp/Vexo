//! [`FocusAttachment`] — glue between an element and the focus tree.
//!
//! Each element that participates in the focus tree holds a `FocusAttachment`.
//! It wraps a [`FocusNodeId`] and provides `reparent()` (called during rebuild)
//! and `detach()` (called during unmount) to keep the focus tree in sync with
//! the element tree.

use std::sync::Arc;

use super::node::FocusNodeId;
use super::manager::FocusManager;

/// Glue between an element and its focus-tree node.
///
/// Created when an element is inflated into the element tree. The attachment
/// is responsible for two lifecycle operations:
///
/// - **reparent** — called during `rebuild()` when the element's parent
///   changes, so the focus tree stays consistent with the element tree.
/// - **detach** — called during `unmount()` to remove the focus node from
///   the tree.
pub struct FocusAttachment {
    node_id: FocusNodeId,
    is_attached: bool,
}

impl FocusAttachment {
    /// Create a new attachment for the given focus node.
    ///
    /// The attachment starts in the attached state.
    pub fn new(node_id: FocusNodeId) -> Self {
        Self {
            node_id,
            is_attached: true,
        }
    }

    /// Return the focus node id this attachment wraps.
    pub fn node_id(&self) -> FocusNodeId {
        self.node_id
    }

    /// Return `true` if the attachment is still connected to the focus tree.
    pub fn is_attached(&self) -> bool {
        self.is_attached
    }

    /// Reparent the focus node to a new parent, attaching to root if `None`.
    ///
    /// This is the primary variant called from element `rebuild()` methods.
    /// When `new_parent` is `None`, the focus node is reparented to the
    /// root of the focus tree. This is a no-op if the attachment has been
    /// detached.
    pub fn reparent_to(&self, new_parent: Option<FocusNodeId>, manager: &mut FocusManager) {
        if self.is_attached {
            manager.reparent(self.node_id, new_parent);
        }
    }

    /// Detach the focus node from the focus tree.
    ///
    /// Called during unmount. Removes the node from the focus tree and marks
    /// this attachment as detached. Subsequent calls to `reparent()` or
    /// `detach()` are no-ops.
    pub fn detach(&mut self, manager: &mut FocusManager) {
        if self.is_attached {
            manager.remove_node(self.node_id);
            self.is_attached = false;
        }
    }

    /// Set the `on_focus_change` callback on this focus node.
    ///
    /// The callback is invoked when this node or a descendant gains/loses
    /// primary focus. Called with `true` when focus is gained, `false` when lost.
    pub fn set_on_focus_change(
        &self,
        callback: Arc<dyn Fn(bool) + Send + Sync>,
        manager: &mut FocusManager,
    ) {
        if self.is_attached {
            if let Some(node) = manager.get_mut(self.node_id) {
                node.on_focus_change = Some(callback);
            }
        }
    }

    /// Clear the `on_focus_change` callback from this focus node.
    pub fn clear_on_focus_change(&self, manager: &mut FocusManager) {
        if self.is_attached {
            if let Some(node) = manager.get_mut(self.node_id) {
                node.on_focus_change = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ElementKey;

    /// Helper: create a plain focus node for testing (no element association).
    ///
    /// Uses a shared `SlotMap` to avoid key collisions — `ElementKey` is a
    /// slotmap key type, so separate `SlotMap` instances can generate the same
    /// key values, causing `create_node_for_element` to return existing nodes
    /// instead of creating new ones.
    fn create_plain_nodes(mgr: &mut FocusManager, count: usize) -> Vec<FocusNodeId> {
        let mut elem_map: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        (0..count)
            .map(|_| {
                let key = elem_map.insert(());
                mgr.create_node_for_element(key, None).unwrap()
            })
            .collect()
    }

    #[test]
    fn test_new_attachment_is_attached() {
        let mut mgr = FocusManager::new();
        let nodes = create_plain_nodes(&mut mgr, 1);
        let node = nodes[0];

        let attachment = FocusAttachment::new(node);
        assert!(attachment.is_attached());
        assert_eq!(attachment.node_id(), node);
    }

    #[test]
    fn test_detach_removes_node_and_marks_detached() {
        let mut mgr = FocusManager::new();
        let nodes = create_plain_nodes(&mut mgr, 1);
        let node = nodes[0];

        let mut attachment = FocusAttachment::new(node);
        assert!(mgr.get(node).is_some());

        attachment.detach(&mut mgr);

        assert!(!attachment.is_attached());
        assert!(mgr.get(node).is_none());
    }

    #[test]
    fn test_detach_is_idempotent() {
        let mut mgr = FocusManager::new();
        let nodes = create_plain_nodes(&mut mgr, 1);
        let node = nodes[0];

        let mut attachment = FocusAttachment::new(node);
        attachment.detach(&mut mgr);

        // Second detach should be a no-op (node already removed from manager).
        attachment.detach(&mut mgr);
        assert!(!attachment.is_attached());
    }

    #[test]
    fn test_reparent_moves_node() {
        let mut mgr = FocusManager::new();
        let nodes = create_plain_nodes(&mut mgr, 3);
        let parent_a = nodes[0];
        let parent_b = nodes[1];
        let node = nodes[2];

        // Manually reparent `node` under parent_a for this test.
        mgr.reparent(node, Some(parent_a));

        let attachment = FocusAttachment::new(node);

        // Node starts under parent_a.
        assert!(mgr.get(parent_a).unwrap().children.contains(&node));

        // Reparent to parent_b.
        attachment.reparent_to(Some(parent_b), &mut mgr);

        // Node is now under parent_b.
        assert!(!mgr.get(parent_a).unwrap().children.contains(&node));
        assert!(mgr.get(parent_b).unwrap().children.contains(&node));
        assert_eq!(mgr.get(node).unwrap().parent, Some(parent_b));
    }

    #[test]
    fn test_reparent_is_noop_after_detach() {
        let mut mgr = FocusManager::new();
        let nodes = create_plain_nodes(&mut mgr, 3);
        let parent_a = nodes[0];
        let parent_b = nodes[1];
        let node = nodes[2];

        // Manually reparent `node` under parent_a for this test.
        mgr.reparent(node, Some(parent_a));

        let mut attachment = FocusAttachment::new(node);
        attachment.detach(&mut mgr);

        // Reparent after detach should be a no-op (node no longer exists).
        attachment.reparent_to(Some(parent_b), &mut mgr);

        // parent_b should not have gained any children from this.
        assert!(mgr.get(parent_b).unwrap().children.is_empty());
    }
}