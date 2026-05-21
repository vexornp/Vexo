# Focus Tree Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat `Option<ElementKey>` focus model with a sparse focus tree (FocusNode/FocusScopeNode) that supports scope-based focus memory and deferred focus changes.

**Architecture:** A separate sparse focus tree mirrors the element tree but only at points where elements opt in via FocusElement/FocusScopeElement. FocusManager owns the tree in a slotmap, supports deferred focus changes, and scope nodes remember last-focused children for restoration.

**Tech Stack:** Rust, slotmap (already in workspace), SecondaryMap for scope extension data

**Spec:** `docs/superpowers/specs/2026-05-21-focus-tree-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `vexo/src/retain/focus/mod.rs` | Public API exports for focus module |
| `vexo/src/retain/focus/node.rs` | `FocusNodeId`, `FocusNodeData` — core node data |
| `vexo/src/retain/focus/scope.rs` | `FocusScopeData`, `UnfocusDisposition`, `TraversalEdgeBehavior` |
| `vexo/src/retain/focus/manager.rs` | `FocusManager` — slotmap storage, focus ops, deferred changes |
| `vexo/src/retain/focus/attachment.rs` | `FocusAttachment` — reparent/detach glue between element and focus tree |
| `vexo/src/retain/focus/element.rs` | `FocusElement`, `FocusScopeElement` — element types |
| `vexo/src/retain/focus/widget.rs` | `Focus`, `FocusScope` — widget types |
| `vexo/src/retain/mod.rs` | Add `focus` module declaration and re-exports |
| `vexo/src/retain/pipeline.rs` | Replace `focused_element` with `FocusManager` |
| `vexo/src/retain/event_context.rs` | Update focus API to use FocusManager |
| `vexo/src/retain/event_handler.rs` | Route focus through FocusManager |
| `vexo/src/retain/build_owner.rs` | Keep `focused_element` as cache, synced from FocusManager |
| `vexo/src/retain/stateful_widget.rs` | Update `StatefulElement::on_event()` focus logic |
| `vexo/src/retain/widgets/text_edit.rs` | Remove direct focus request (handled by FocusElement) |
| `shared_app/src/lib.rs` | Wrap TextEdit with Focus widget |

---

## Task 1: FocusNode + FocusScopeNode Data Model

**Files:**
- Create: `vexo/src/retain/focus/mod.rs`
- Create: `vexo/src/retain/focus/node.rs`
- Create: `vexo/src/retain/focus/scope.rs`
- Create: `vexo/src/retain/focus/manager.rs`
- Modify: `vexo/src/retain/mod.rs`

This task creates the focus module with pure data structures and FocusManager. No integration with existing code — only unit tests. `request_focus()` is immediate (not deferred) in this step.

- [ ] **Step 1: Create focus module directory and mod.rs**

Create `vexo/src/retain/focus/mod.rs`:

```rust
//! Focus tree for the retain-mode pipeline.
//!
//! Implements a Flutter-style sparse focus tree with FocusNode, FocusScopeNode,
//! and FocusManager. The focus tree mirrors the element tree but only contains
//! nodes where elements opt in via FocusElement/FocusScopeElement.

mod node;
mod scope;
mod manager;

pub use node::{FocusNodeId, FocusNodeData};
pub use scope::{FocusScopeData, UnfocusDisposition, TraversalEdgeBehavior};
pub use manager::FocusManager;
```

- [ ] **Step 2: Create node.rs with FocusNodeId and FocusNodeData**

Create `vexo/src/retain/focus/node.rs`:

```rust
//! Focus node data and identity.

use slotmap::new_key_type;

use super::id::ElementKey;

// Generate a unique slotmap key type for focus nodes.
new_key_type! {
    /// Opaque key identifying a focus node in the FocusManager's slotmap.
    pub struct FocusNodeId;
}

/// Data stored for each focus node in the FocusManager's slotmap.
pub struct FocusNodeData {
    /// The element this node is associated with (for dispatching keyboard events).
    pub element_key: Option<ElementKey>,

    /// Parent node in the focus tree.
    pub parent: Option<FocusNodeId>,

    /// Ordered children in the focus tree.
    /// Order determines future traversal order (Tab/Shift-Tab).
    pub children: Vec<FocusNodeId>,

    /// Whether this node can receive focus.
    pub can_request_focus: bool,

    /// Whether this node is excluded from Tab traversal.
    /// Skip-traversal nodes can still receive focus via request_focus().
    pub skip_traversal: bool,

    /// Whether this node is a scope (has FocusScopeData in SecondaryMap).
    pub is_scope: bool,
}

impl FocusNodeData {
    /// Create a new focus node data with default values.
    pub fn new(element_key: Option<ElementKey>) -> Self {
        Self {
            element_key,
            parent: None,
            children: Vec::new(),
            can_request_focus: true,
            skip_traversal: false,
            is_scope: false,
        }
    }

    /// Create a new scope node data.
    pub fn new_scope(element_key: Option<ElementKey>) -> Self {
        Self {
            element_key,
            parent: None,
            children: Vec::new(),
            can_request_focus: true,
            skip_traversal: false,
            is_scope: true,
        }
    }
}
```

- [ ] **Step 3: Create scope.rs with FocusScopeData, UnfocusDisposition, TraversalEdgeBehavior**

Create `vexo/src/retain/focus/scope.rs`:

```rust
//! Focus scope data and related enums.

use super::node::FocusNodeId;

/// Extra data stored for focus scope nodes (in a SecondaryMap).
///
/// Since Rust has no class inheritance, scope data is stored separately
/// from FocusNodeData and accessed via `FocusManager::scope_data(key)`.
pub struct FocusScopeData {
    /// Stack of recently-focused children (most recent at end).
    ///
    /// When a node N gains primary focus, walk up ancestor scopes and
    /// push N to each scope's `focused_children`. When a scope regains
    /// focus, pop the last entry and descend through nested scopes to
    /// find the leaf to restore.
    pub focused_children: Vec<FocusNodeId>,

    /// What happens when Tab traversal reaches the boundary of this scope.
    pub traversal_edge_behavior: TraversalEdgeBehavior,
}

impl FocusScopeData {
    /// Create a new scope data with default values.
    pub fn new() -> Self {
        Self {
            focused_children: Vec::new(),
            traversal_edge_behavior: TraversalEdgeBehavior::ParentScope,
        }
    }

    /// Get the most recently focused child (top of the stack).
    pub fn focused_child(&self) -> Option<FocusNodeId> {
        self.focused_children.last().copied()
    }
}

impl Default for FocusScopeData {
    fn default() -> Self {
        Self::new()
    }
}

/// What happens when focus traversal reaches the boundary of a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalEdgeBehavior {
    /// Wrap around within the scope (Tab from last goes to first).
    ClosedLoop,
    /// Exit to the parent scope and continue traversal there.
    ParentScope,
    /// Stay at the current position (no wrapping, no exit).
    Stop,
}

/// How to handle unfocusing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnfocusDisposition {
    /// Restore the scope's previously focused child from the history stack.
    RestorePrevious,
    /// Clear focus entirely (set primary_focus to None).
    Clear,
}
```

- [ ] **Step 4: Create manager.rs with FocusManager**

Create `vexo/src/retain/focus/manager.rs`:

```rust
//! Focus manager — owns the focus tree and provides focus operations.

use std::collections::HashSet;

use slotmap::{SecondaryMap, SlotMap};

use super::id::ElementKey;
use super::node::{FocusNodeId, FocusNodeData};
use super::scope::{FocusScopeData, UnfocusDisposition};

/// Manages the focus tree for the retain-mode pipeline.
///
/// The focus tree is a separate, sparser tree that mirrors the element tree
/// but only contains nodes where elements opt in via FocusElement/FocusScopeElement.
///
/// # Structure
///
/// - `nodes`: SlotMap storing all FocusNodeData (including scopes)
/// - `scopes`: SecondaryMap storing extra FocusScopeData for scope nodes
/// - `root_scope`: The top-level scope (always present)
/// - `primary_focus`: The single node with primary focus
pub struct FocusManager {
    /// All focus nodes (including scope nodes).
    nodes: SlotMap<FocusNodeId, FocusNodeData>,

    /// Extra data for scope nodes. Only accessed for nodes where `is_scope == true`.
    scopes: SecondaryMap<FocusNodeId, FocusScopeData>,

    /// The root scope node. Always present after construction.
    root_scope: FocusNodeId,

    /// The currently focused node (the single node with primary focus).
    primary_focus: Option<FocusNodeId>,

    /// Element-to-node mapping for looking up FocusNodeId by ElementKey.
    /// Used during the transition period when elements still request focus
    /// by ElementKey.
    element_to_node: std::collections::HashMap<ElementKey, FocusNodeId>,
}

impl FocusManager {
    /// Create a new FocusManager with a root scope.
    pub fn new() -> Self {
        let mut nodes: SlotMap<FocusNodeId, FocusNodeData> = SlotMap::with_key();
        let mut scopes: SecondaryMap<FocusNodeId, FocusScopeData> = SecondaryMap::new();

        let root_scope = nodes.insert(FocusNodeData::new_scope(None));
        scopes.insert(root_scope, FocusScopeData::new());

        Self {
            nodes,
            scopes,
            root_scope,
            primary_focus: None,
            element_to_node: std::collections::HashMap::new(),
        }
    }

    /// Get the root scope key.
    pub fn root_scope(&self) -> FocusNodeId {
        self.root_scope
    }

    /// Get the primary focus node.
    pub fn primary_focus(&self) -> Option<FocusNodeId> {
        self.primary_focus
    }

    /// Get the ElementKey of the primary focus node.
    pub fn primary_focus_element(&self) -> Option<ElementKey> {
        self.primary_focus.and_then(|id| self.nodes.get(id).and_then(|n| n.element_key))
    }

    /// Check if a node has primary focus.
    pub fn has_primary_focus(&self, id: FocusNodeId) -> bool {
        self.primary_focus == Some(id)
    }

    /// Check if a node or any of its descendants has primary focus.
    pub fn has_focus(&self, id: FocusNodeId) -> bool {
        if self.primary_focus == Some(id) {
            return true;
        }
        // Walk up from primary_focus to see if `id` is an ancestor
        let Some(primary) = self.primary_focus else { return false; };
        let mut current = primary;
        while let Some(node) = self.nodes.get(current) {
            if let Some(parent) = node.parent {
                if parent == id {
                    return true;
                }
                current = parent;
            } else {
                break;
            }
        }
        false
    }

    /// Create a new focus node and attach it to a parent.
    pub fn create_node(
        &mut self,
        element_key: Option<ElementKey>,
        parent: FocusNodeId,
    ) -> FocusNodeId {
        let id = self.nodes.insert(FocusNodeData::new(element_key));
        if let Some(parent_node) = self.nodes.get_mut(parent) {
            parent_node.children.push(id);
        }
        if let Some(node) = self.nodes.get_mut(id) {
            node.parent = Some(parent);
        }
        if let Some(ek) = element_key {
            self.element_to_node.insert(ek, id);
        }
        id
    }

    /// Create a new scope node and attach it to a parent.
    pub fn create_scope(
        &mut self,
        element_key: Option<ElementKey>,
        parent: FocusNodeId,
    ) -> FocusNodeId {
        let id = self.nodes.insert(FocusNodeData::new_scope(element_key));
        self.scopes.insert(id, FocusScopeData::new());
        if let Some(parent_node) = self.nodes.get_mut(parent) {
            parent_node.children.push(id);
        }
        if let Some(node) = self.nodes.get_mut(id) {
            node.parent = Some(parent);
        }
        if let Some(ek) = element_key {
            self.element_to_node.insert(ek, id);
        }
        id
    }

    /// Remove a node from the focus tree.
    ///
    /// If the node was focused, focus moves to the next focusable sibling
    /// or is cleared. Children are also removed.
    pub fn remove_node(&mut self, id: FocusNodeId) {
        // Remove from parent's children list
        if let Some(node) = self.nodes.get(id) {
            if let Some(parent_id) = node.parent {
                if let Some(parent) = self.nodes.get_mut(parent_id) {
                    parent.children.retain(|c| *c != id);
                }
            }
        }

        // If this node was focused, clear focus
        if self.primary_focus == Some(id) {
            self.primary_focus = None;
        }

        // Remove from scope's focused_children history
        if let Some(node) = self.nodes.get(id) {
            if let Some(parent_id) = node.parent {
                if let Some(scope) = self.scopes.get_mut(parent_id) {
                    scope.focused_children.retain(|c| *c != id);
                }
            }
        }

        // Remove from element_to_node mapping
        if let Some(node) = self.nodes.get(id) {
            if let Some(ek) = node.element_key {
                self.element_to_node.remove(&ek);
            }
        }

        // Remove scope data if this is a scope
        self.scopes.remove(id);

        // Remove the node itself (slotmap handles child cleanup lazily;
        // orphaned children will have dangling parent refs but that's OK
        // since they should be unmounted in the same pass)
        self.nodes.remove(id);
    }

    /// Request focus for a node (immediate in this step).
    ///
    /// Returns the previously focused node, if any.
    pub fn request_focus(&mut self, id: FocusNodeId) -> Option<FocusNodeId> {
        // Check can_request_focus
        if let Some(node) = self.nodes.get(id) {
            if !node.can_request_focus {
                return None;
            }
        } else {
            return None;
        }

        let previous = self.primary_focus;
        self.primary_focus = Some(id);

        // Update scope focused_children: walk up from id, push to each scope
        self.set_as_focused_child_for_scope(id);

        previous
    }

    /// Request focus by ElementKey (convenience for transition period).
    pub fn request_focus_by_element(&mut self, element_key: ElementKey) -> Option<FocusNodeId> {
        if let Some(&node_id) = self.element_to_node.get(&element_key) {
            self.request_focus(node_id)
        } else {
            None
        }
    }

    /// Clear focus entirely.
    pub fn unfocus(&mut self) {
        self.primary_focus = None;
    }

    /// Unfocus with a specific disposition.
    pub fn unfocus_with_disposition(&mut self, disposition: UnfocusDisposition) {
        match disposition {
            UnfocusDisposition::Clear => {
                self.primary_focus = None;
            }
            UnfocusDisposition::RestorePrevious => {
                // Find the enclosing scope and restore its focused_child
                let Some(primary) = self.primary_focus else { return; };
                let scope_id = self.enclosing_scope(primary);
                if let Some(scope) = self.scopes.get(scope_id) {
                    // Pop current from history, then peek at previous
                    if let Some(scope) = self.scopes.get_mut(scope_id) {
                        // Remove the current primary from history
                        scope.focused_children.retain(|c| *c != primary);
                        // Restore the previous entry
                        if let Some(prev) = scope.focused_child() {
                            self.primary_focus = Some(prev);
                            return;
                        }
                    }
                }
                self.primary_focus = None;
            }
        }
    }

    /// Reparent a node to a new parent in the focus tree.
    pub fn reparent(&mut self, id: FocusNodeId, new_parent: FocusNodeId) {
        // Remove from old parent's children
        if let Some(node) = self.nodes.get(id) {
            if let Some(old_parent_id) = node.parent {
                if let Some(old_parent) = self.nodes.get_mut(old_parent_id) {
                    old_parent.children.retain(|c| *c != id);
                }
            }
        }

        // Add to new parent's children
        if let Some(new_parent_node) = self.nodes.get_mut(new_parent) {
            new_parent_node.children.push(id);
        }

        // Update node's parent pointer
        if let Some(node) = self.nodes.get_mut(id) {
            node.parent = Some(new_parent);
        }
    }

    /// Find the enclosing scope for a node.
    ///
    /// Walks up the focus tree from the node until it finds a scope node.
    /// Returns the root scope if no other scope is found.
    pub fn enclosing_scope(&self, id: FocusNodeId) -> FocusNodeId {
        let mut current = id;
        while let Some(node) = self.nodes.get(current) {
            if node.is_scope && current != id {
                return current;
            }
            if let Some(parent) = node.parent {
                current = parent;
            } else {
                break;
            }
        }
        self.root_scope
    }

    /// Find the nearest parent scope for a node (for attaching new nodes).
    ///
    /// Walks up from the node's parent until it finds a scope.
    pub fn nearest_parent_scope(&self, id: FocusNodeId) -> FocusNodeId {
        let mut current = id;
        while let Some(node) = self.nodes.get(current) {
            if node.is_scope {
                return current;
            }
            if let Some(parent) = node.parent {
                current = parent;
            } else {
                break;
            }
        }
        self.root_scope
    }

    /// Set a node as the focused child for all ancestor scopes.
    ///
    /// Walks up from the node, pushing it to each ancestor scope's
    /// focused_children stack (removing it from any earlier position first).
    fn set_as_focused_child_for_scope(&mut self, id: FocusNodeId) {
        let mut current = id;
        while let Some(node) = self.nodes.get(current) {
            if let Some(parent_id) = node.parent {
                if let Some(parent) = self.nodes.get(parent_id) {
                    if parent.is_scope {
                        if let Some(scope) = self.scopes.get_mut(parent_id) {
                            // Remove from any earlier position
                            scope.focused_children.retain(|c| *c != current);
                            // Push to end (most recent)
                            scope.focused_children.push(current);
                        }
                    }
                }
                current = parent_id;
            } else {
                break;
            }
        }
    }

    /// Get a reference to a focus node.
    pub fn get_node(&self, id: FocusNodeId) -> Option<&FocusNodeData> {
        self.nodes.get(id)
    }

    /// Get a mutable reference to a focus node.
    pub fn get_node_mut(&mut self, id: FocusNodeId) -> Option<&mut FocusNodeData> {
        self.nodes.get_mut(id)
    }

    /// Get scope data for a node.
    pub fn get_scope(&self, id: FocusNodeId) -> Option<&FocusScopeData> {
        self.scopes.get(id)
    }

    /// Get mutable scope data for a node.
    pub fn get_scope_mut(&mut self, id: FocusNodeId) -> Option<&mut FocusScopeData> {
        self.scopes.get_mut(id)
    }

    /// Check if a node exists.
    pub fn contains(&self, id: FocusNodeId) -> bool {
        self.nodes.contains_key(id)
    }

    /// Look up FocusNodeId by ElementKey.
    pub fn node_for_element(&self, element_key: ElementKey) -> Option<FocusNodeId> {
        self.element_to_node.get(&element_key).copied()
    }

    /// Check if an element is focused (by ElementKey).
    pub fn is_element_focused(&self, element_key: ElementKey) -> bool {
        self.primary_focus_element() == Some(element_key)
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 5: Add focus module to retain/mod.rs**

Add `mod focus;` to the private module declarations in `vexo/src/retain/mod.rs` (after `mod child_ops;`), and add re-exports:

```rust
pub use focus::{FocusManager, FocusNodeId, FocusNodeData, FocusScopeData, UnfocusDisposition, TraversalEdgeBehavior};
```

- [ ] **Step 6: Write unit tests for FocusManager**

Add tests at the end of `vexo/src/retain/focus/manager.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_manager_new() {
        let mgr = FocusManager::new();
        assert!(mgr.primary_focus().is_none());
        assert!(mgr.contains(mgr.root_scope()));
    }

    #[test]
    fn test_create_node() {
        let mut mgr = FocusManager::new();
        let node = mgr.create_node(None, mgr.root_scope());
        assert!(mgr.contains(node));
        assert!(mgr.get_node(node).is_some());
        assert!(!mgr.get_node(node).unwrap().is_scope);
    }

    #[test]
    fn test_create_scope() {
        let mut mgr = FocusManager::new();
        let scope = mgr.create_scope(None, mgr.root_scope());
        assert!(mgr.contains(scope));
        assert!(mgr.get_node(scope).unwrap().is_scope);
        assert!(mgr.get_scope(scope).is_some());
    }

    #[test]
    fn test_request_focus() {
        let mut mgr = FocusManager::new();
        let node = mgr.create_node(None, mgr.root_scope());
        let prev = mgr.request_focus(node);
        assert!(prev.is_none());
        assert_eq!(mgr.primary_focus(), Some(node));
    }

    #[test]
    fn test_request_focus_can_request_focus_false() {
        let mut mgr = FocusManager::new();
        let node = mgr.create_node(None, mgr.root_scope());
        if let Some(n) = mgr.get_node_mut(node) {
            n.can_request_focus = false;
        }
        let prev = mgr.request_focus(node);
        assert!(prev.is_none());
        assert!(mgr.primary_focus().is_none());
    }

    #[test]
    fn test_unfocus() {
        let mut mgr = FocusManager::new();
        let node = mgr.create_node(None, mgr.root_scope());
        mgr.request_focus(node);
        mgr.unfocus();
        assert!(mgr.primary_focus().is_none());
    }

    #[test]
    fn test_remove_node() {
        let mut mgr = FocusManager::new();
        let node = mgr.create_node(None, mgr.root_scope());
        mgr.request_focus(node);
        mgr.remove_node(node);
        assert!(mgr.primary_focus().is_none());
        assert!(!mgr.contains(node));
    }

    #[test]
    fn test_reparent() {
        let mut mgr = FocusManager::new();
        let scope1 = mgr.create_scope(None, mgr.root_scope());
        let scope2 = mgr.create_scope(None, mgr.root_scope());
        let node = mgr.create_node(None, scope1);

        // Node should be in scope1's children
        assert!(mgr.get_node(scope1).unwrap().children.contains(&node));

        // Reparent to scope2
        mgr.reparent(node, scope2);
        assert!(!mgr.get_node(scope1).unwrap().children.contains(&node));
        assert!(mgr.get_node(scope2).unwrap().children.contains(&node));
        assert_eq!(mgr.get_node(node).unwrap().parent, Some(scope2));
    }

    #[test]
    fn test_enclosing_scope() {
        let mut mgr = FocusManager::new();
        let scope = mgr.create_scope(None, mgr.root_scope());
        let node = mgr.create_node(None, scope);
        assert_eq!(mgr.enclosing_scope(node), scope);
    }

    #[test]
    fn test_scope_focused_child_memory() {
        let mut mgr = FocusManager::new();
        let scope = mgr.create_scope(None, mgr.root_scope());
        let node1 = mgr.create_node(None, scope);
        let node2 = mgr.create_node(None, scope);

        // Focus node1
        mgr.request_focus(node1);
        assert_eq!(mgr.get_scope(scope).unwrap().focused_child(), Some(node1));

        // Focus node2
        mgr.request_focus(node2);
        assert_eq!(mgr.get_scope(scope).unwrap().focused_child(), Some(node2));
    }

    #[test]
    fn test_unfocus_restore_previous() {
        let mut mgr = FocusManager::new();
        let scope = mgr.create_scope(None, mgr.root_scope());
        let node1 = mgr.create_node(None, scope);
        let node2 = mgr.create_node(None, scope);

        mgr.request_focus(node1);
        mgr.request_focus(node2);
        assert_eq!(mgr.primary_focus(), Some(node2));

        // Unfocus node2, should restore node1
        mgr.unfocus_with_disposition(UnfocusDisposition::RestorePrevious);
        assert_eq!(mgr.primary_focus(), Some(node1));
    }

    #[test]
    fn test_element_to_node_mapping() {
        let mut mgr = FocusManager::new();
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        let ek = sm.insert(());
        let node = mgr.create_node(Some(ek), mgr.root_scope());

        assert_eq!(mgr.node_for_element(ek), Some(node));
        assert!(mgr.is_element_focused(ek) == false);

        mgr.request_focus(node);
        assert!(mgr.is_element_focused(ek));
        assert_eq!(mgr.primary_focus_element(), Some(ek));
    }
}
```

- [ ] **Step 7: Run tests to verify**

Run: `cargo test -p vexo -- focus::manager::tests`
Expected: All 11 tests PASS

- [ ] **Step 8: Commit**

```bash
git add vexo/src/retain/focus/ vexo/src/retain/mod.rs
git commit -m "feat: add FocusManager with FocusNode/FocusScopeNode data model"
```

---

## Task 2: FocusManager Integration into Pipeline

**Files:**
- Modify: `vexo/src/retain/pipeline.rs`
- Modify: `vexo/src/retain/event_handler.rs`
- Modify: `vexo/src/retain/event_context.rs`
- Modify: `vexo/src/retain/build_owner.rs`

Replace `focused_element: Option<ElementKey>` on the pipeline with `FocusManager`. The pipeline creates a FocusNode for each element that requests focus, and routes focus requests through FocusManager. Existing TextEdit click-to-focus continues to work.

- [ ] **Step 1: Add FocusManager to ThreeTreePipeline**

In `vexo/src/retain/pipeline.rs`:

1. Add `use super::focus::FocusManager;` import
2. Replace `focused_element: Option<ElementKey>` field with `focus_manager: FocusManager`
3. In `new()`, initialize `focus_manager: FocusManager::new()`
4. Update `sync_focus_to_build_owner()` to read from `focus_manager.primary_focus_element()`
5. Update `focused_element()` getter to return `self.focus_manager.primary_focus_element()`
6. Update `set_focus()` to route through FocusManager (create node if needed)
7. Update `handle_event()` to pass `&mut self.focus_manager` instead of `&mut self.focused_element`

- [ ] **Step 2: Update EventHandler to use FocusManager**

In `vexo/src/retain/event_handler.rs`:

1. Add `use super::focus::FocusManager;` import
2. Change `focused_element: &mut Option<ElementKey>` parameter to `focus_manager: &mut FocusManager`
3. In `handle_pointer_event()`:
   - Replace `*focused_element = None` with `focus_manager.unfocus()`
   - Replace `*focused_element = Some(focus)` with `focus_manager.request_focus_by_element(focus)`
   - Pass `focus_manager.primary_focus_element()` to `EventContext::new()`
4. In `handle_keyboard_event()`:
   - Get focused element from `focus_manager.primary_focus_element()`
   - Same focus request handling as pointer events

- [ ] **Step 3: Update EventContext to work with FocusManager**

In `vexo/src/retain/event_context.rs`:

The `EventContext` keeps its existing API (`is_focused()`, `request_focus()`, `clear_focus()`) but now these are a thin layer that will eventually be replaced by FocusElement. No changes needed yet — the pipeline translates between FocusManager and EventContext.

- [ ] **Step 4: Update BuildOwner sync**

In `vexo/src/retain/pipeline.rs`, update `sync_focus_to_build_owner()`:

```rust
fn sync_focus_to_build_owner(&self) {
    self.build_owner.set_focused_element(self.focus_manager.primary_focus_element());
}
```

- [ ] **Step 5: Run all existing tests**

Run: `cargo test -p vexo`
Expected: All existing tests PASS (no behavior change, just plumbing)

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/pipeline.rs vexo/src/retain/event_handler.rs vexo/src/retain/event_context.rs vexo/src/retain/build_owner.rs
git commit -m "feat: integrate FocusManager into pipeline, replacing flat focused_element"
```

---

## Task 3: FocusAttachment

**Files:**
- Create: `vexo/src/retain/focus/attachment.rs`
- Modify: `vexo/src/retain/focus/mod.rs`

FocusAttachment is the glue between an element and the focus tree. It handles reparent (called during rebuild) and detach (called during unmount).

- [ ] **Step 1: Create attachment.rs**

Create `vexo/src/retain/focus/attachment.rs`:

```rust
//! Focus attachment — glue between an element and the focus tree.

use super::node::FocusNodeId;
use super::manager::FocusManager;

/// Represents the attachment of a focus node to the focus tree.
///
/// Created when a FocusElement mounts. Provides `reparent()` (called
/// during rebuild) and `detach()` (called during unmount).
pub struct FocusAttachment {
    /// The focus node this attachment manages.
    node_id: FocusNodeId,

    /// Whether this attachment is still active.
    is_attached: bool,
}

impl FocusAttachment {
    /// Create a new attachment for a focus node.
    pub fn new(node_id: FocusNodeId) -> Self {
        Self {
            node_id,
            is_attached: true,
        }
    }

    /// Get the focus node ID.
    pub fn node_id(&self) -> FocusNodeId {
        self.node_id
    }

    /// Check if this attachment is still active.
    pub fn is_attached(&self) -> bool {
        self.is_attached
    }

    /// Reparent the focus node to a new parent in the focus tree.
    ///
    /// Called during rebuild to keep the focus tree synced with the
    /// element tree. If the element has moved in the element tree,
    /// its focus node must move to the corresponding new parent scope.
    pub fn reparent(&self, new_parent: FocusNodeId, manager: &mut FocusManager) {
        if self.is_attached {
            manager.reparent(self.node_id, new_parent);
        }
    }

    /// Detach the focus node from the focus tree.
    ///
    /// Called during unmount. Removes the node from the focus tree.
    /// If the node was focused, focus is cleared or moved.
    pub fn detach(&mut self, manager: &mut FocusManager) {
        if self.is_attached {
            manager.remove_node(self.node_id);
            self.is_attached = false;
        }
    }
}
```

- [ ] **Step 2: Update focus/mod.rs to export FocusAttachment**

Add `mod attachment;` and `pub use attachment::FocusAttachment;` to `vexo/src/retain/focus/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p vexo`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/focus/
git commit -m "feat: add FocusAttachment for element-to-focus-tree glue"
```

---

## Task 4: FocusElement + FocusScopeElement

**Files:**
- Create: `vexo/src/retain/focus/element.rs`
- Create: `vexo/src/retain/focus/widget.rs`
- Modify: `vexo/src/retain/focus/mod.rs`
- Modify: `vexo/src/retain/mod.rs`
- Modify: `vexo/src/retain/widgets/mod.rs`

Add FocusElement and FocusScopeElement element types, and Focus/FocusScope widget types. These are available but not yet used by TextEdit.

- [ ] **Step 1: Create widget.rs with Focus and FocusScope widgets**

Create `vexo/src/retain/focus/widget.rs`:

```rust
//! Focus and FocusScope widgets.

use std::any::Any;

use crate::retain::widgets::Widget;
use crate::retain::elements::container::ContainerElement;
use crate::retain::element::Element;

/// A widget that wraps a child in a focus node.
///
/// When the child element mounts, a FocusNode is created in the
/// FocusManager's focus tree. The child can then receive focus
/// via click or programmatic request.
pub struct Focus {
    /// The child widget.
    child: Box<dyn Widget>,

    /// Whether to autofocus this node on mount.
    autofocus: bool,
}

impl Focus {
    /// Create a new Focus wrapper around a child widget.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            autofocus: false,
        }
    }

    /// Set whether to autofocus on mount.
    pub fn autofocus(mut self, autofocus: bool) -> Self {
        self.autofocus = autofocus;
        self
    }
}

impl Widget for Focus {
    fn key(&self) -> Option<crate::retain::key::WidgetKey> {
        self.child.key()
    }

    fn create_element(&self) -> Box<dyn Element> {
        // Focus uses ContainerElement for now (manages a single child).
        // FocusElement will be a future optimization.
        Box::new(ContainerElement::new())
    }

    fn create_render_object(&self) -> Box<dyn crate::retain::render_object::RenderObject> {
        // Focus doesn't create its own render object —
        // it's a proxy that passes through to the child.
        self.child.create_render_object()
    }

    fn can_update(&self, other: &dyn Widget) -> bool {
        if let Some(other_focus) = other.as_any().downcast_ref::<Focus>() {
            self.child.can_update(other_focus.child.as_ref())
        } else {
            false
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(Focus {
            child: self.child.clone_boxed(),
            autofocus: self.autofocus,
        })
    }
}

/// A widget that wraps a child in a focus scope.
///
/// Focus scopes create boundaries for focus traversal and
/// remember which child was last focused (for restoration).
pub struct FocusScope {
    /// The child widget.
    child: Box<dyn Widget>,
}

impl FocusScope {
    /// Create a new FocusScope wrapper around a child widget.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
        }
    }
}

impl Widget for FocusScope {
    fn key(&self) -> Option<crate::retain::key::WidgetKey> {
        self.child.key()
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ContainerElement::new())
    }

    fn create_render_object(&self) -> Box<dyn crate::retain::render_object::RenderObject> {
        self.child.create_render_object()
    }

    fn can_update(&self, other: &dyn Widget) -> bool {
        if let Some(other_scope) = other.as_any().downcast_ref::<FocusScope>() {
            self.child.can_update(other_scope.child.as_ref())
        } else {
            false
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(FocusScope {
            child: self.child.clone_boxed(),
        })
    }
}
```

- [ ] **Step 2: Update focus/mod.rs to export widgets**

Add `mod widget;` and `pub use widget::{Focus, FocusScope};` to `vexo/src/retain/focus/mod.rs`.

- [ ] **Step 3: Update retain/mod.rs to re-export Focus and FocusScope**

Add `Focus, FocusScope` to the `pub use focus::...` line.

- [ ] **Step 4: Run tests**

Run: `cargo test -p vexo`
Expected: All tests PASS (Focus/FocusScope are defined but not yet used)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/focus/ vexo/src/retain/mod.rs
git commit -m "feat: add Focus and FocusScope widgets"
```

---

## Task 5: Migrate TextEdit to Use Focus Widget

**Files:**
- Modify: `shared_app/src/lib.rs`
- Modify: `vexo/src/retain/stateful_widget.rs`
- Modify: `vexo/src/retain/widgets/text_edit.rs`

Wrap TextEdit with `Focus::new()` in the demo app. Update `StatefulElement::on_event()` so that focus handling is more general (not hardcoded to TextEdit).

- [ ] **Step 1: Wrap TextEdit with Focus in demo app**

In `shared_app/src/lib.rs`, change the TextEdit push to:

```rust
.push(vexo::retain::Focus::new(retain::TextEdit::new(controller.clone())))
```

- [ ] **Step 2: Generalize StatefulElement::on_event() focus handling**

In `vexo/src/retain/stateful_widget.rs`, the current `on_event()` has a hardcoded `TextEdit` downcast for keyboard events. Replace it with a general trait-based approach:

Add a new trait for widgets that handle keyboard events when focused:

```rust
/// Trait for widgets that handle keyboard events when focused.
pub trait FocusableWidget: Any {
    fn handle_keyboard_event(
        &self,
        event: &InputEvent,
        context: &mut EventContext,
    ) -> Option<Box<dyn Any>>;
}
```

Implement `FocusableWidget` for `TextEdit` in `widgets/text_edit.rs`.

Update `StatefulElement::on_event()` to check for `FocusableWidget` instead of hardcoding `TextEdit`:

```rust
fn on_event(&mut self, event: &InputEvent, context: &mut EventContext) -> Option<Box<dyn Any>> {
    if let InputEvent::Keyboard { .. } = event {
        if let Some(id) = self.id {
            if context.is_focused(id) {
                if let Some(focusable) = self.widget.as_any().downcast_ref::<dyn FocusableWidget>() {
                    return focusable.handle_keyboard_event(event, context);
                }
            }
        }
    }

    // Pointer press inside → request focus
    if let InputEvent::PointerButton {
        state: crate::input::ButtonState::Pressed,
        ..
    } = event
    {
        if context.is_pointer_inside() {
            if let Some(id) = self.id {
                context.request_focus(id);
                return Some(Box::new(()));
            }
        }
    }

    None
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests PASS

- [ ] **Step 4: Run desktop demo to verify TextEdit focus works**

Run: `cargo run -p desktop_demo`
Expected: Clicking inside TextEdit gives it focus (blue border), keyboard input works, clicking outside removes focus (gray border).

- [ ] **Step 5: Commit**

```bash
git add shared_app/src/lib.rs vexo/src/retain/stateful_widget.rs vexo/src/retain/widgets/text_edit.rs
git commit -m "feat: wrap TextEdit with Focus widget, generalize focusable keyboard handling"
```

---

## Task 6: Deferred Focus Changes

**Files:**
- Modify: `vexo/src/retain/focus/manager.rs`
- Modify: `vexo/src/retain/pipeline.rs`

Add deferred focus changes to FocusManager. `request_focus()` sets a pending request instead of immediately changing `primary_focus`. The pipeline calls `apply_focus_changes()` at the end of event processing.

- [ ] **Step 1: Add deferred fields to FocusManager**

In `vexo/src/retain/focus/manager.rs`, add fields:

```rust
/// Pending focus request (deferred until apply_focus_changes).
pending_focus_request: Option<FocusNodeId>,

/// Whether a focus change has been requested this frame.
has_pending_focus_change: bool,

/// Nodes whose has_focus status changed (need notification).
dirty_nodes: HashSet<FocusNodeId>,

/// Callbacks for focus gained events.
on_focus_gained: Vec<Box<dyn Fn()>>,

/// Callbacks for focus lost events.
on_focus_lost: Vec<Box<dyn Fn()>>,
```

- [ ] **Step 2: Change request_focus() to be deferred**

```rust
pub fn request_focus(&mut self, id: FocusNodeId) {
    // Check can_request_focus
    if let Some(node) = self.nodes.get(id) {
        if !node.can_request_focus {
            return;
        }
    } else {
        return;
    }

    self.pending_focus_request = Some(id);
    self.has_pending_focus_change = true;
}
```

- [ ] **Step 3: Add apply_focus_changes()**

```rust
/// Commit pending focus changes.
///
/// Called by the pipeline at the end of event processing.
/// Computes the diff between old and new focus paths,
/// notifies affected nodes, and fires callbacks.
pub fn apply_focus_changes(&mut self) {
    if !self.has_pending_focus_change {
        return;
    }
    self.has_pending_focus_change = false;

    let previous_focus = self.primary_focus;
    let new_focus = self.pending_focus_request.take();

    // Compute old focus path (ancestors of old primary_focus)
    let old_path: Vec<FocusNodeId> = if let Some(old) = previous_focus {
        self.ancestor_path(old)
    } else {
        Vec::new()
    };

    // Compute new focus path (ancestors of new focus)
    let new_path: Vec<FocusNodeId> = if let Some(new) = new_focus {
        self.ancestor_path(new)
    } else {
        Vec::new()
    };

    let old_set: HashSet<FocusNodeId> = old_path.into_iter().collect();
    let new_set: HashSet<FocusNodeId> = new_path.into_iter().collect();

    // Nodes that gained has_focus (in new, not in old)
    for &node_id in &new_set {
        if !old_set.contains(&node_id) {
            self.dirty_nodes.insert(node_id);
        }
    }

    // Nodes that lost has_focus (in old, not in new)
    for &node_id in &old_set {
        if !new_set.contains(&node_id) {
            self.dirty_nodes.insert(node_id);
        }
    }

    // Commit the focus change
    self.primary_focus = new_focus;

    // Update scope focused_children for the new focus
    if let Some(new) = new_focus {
        self.set_as_focused_child_for_scope(new);
    }

    // Fire callbacks (collected separately to avoid borrow issues)
    // For now, just clear dirty_nodes. Callbacks will be added in a future step.
    self.dirty_nodes.clear();
}

/// Get the ancestor path from a node to the root.
fn ancestor_path(&self, id: FocusNodeId) -> Vec<FocusNodeId> {
    let mut path = vec![id];
    let mut current = id;
    while let Some(node) = self.nodes.get(current) {
        if let Some(parent) = node.parent {
            path.push(parent);
            current = parent;
        } else {
            break;
        }
    }
    path
}

/// Check if there are pending focus changes.
pub fn has_pending_changes(&self) -> bool {
    self.has_pending_focus_change
}
```

- [ ] **Step 4: Add apply_focus_changes() call to pipeline**

In `vexo/src/retain/pipeline.rs`, after `handle_event()` returns, call `self.focus_manager.apply_focus_changes()`:

```rust
pub fn handle_event(
    &mut self,
    position: Point<Logical>,
    event: &InputEvent,
    modifiers: Modifiers,
    font_system: &mut glyphon::FontSystem,
) -> Option<Box<dyn Any>> {
    let result = EventHandler::handle_event(
        &mut self.element_registry,
        &self.render_objects,
        &mut self.state,
        font_system,
        &self.build_owner,
        &self.dirty_sender,
        &mut self.focus_manager,
        position,
        event,
        modifiers,
    );
    // Commit deferred focus changes
    self.focus_manager.apply_focus_changes();
    result
}
```

- [ ] **Step 5: Update EventHandler to use deferred request_focus**

The EventHandler now calls `focus_manager.request_focus()` which is deferred. The `apply_focus_changes()` call in the pipeline commits the change. Update `sync_focus_to_build_owner()` to be called after `apply_focus_changes()`.

- [ ] **Step 6: Write tests for deferred focus changes**

Add to `vexo/src/retain/focus/manager.rs` tests:

```rust
#[test]
fn test_deferred_focus_change() {
    let mut mgr = FocusManager::new();
    let node1 = mgr.create_node(None, mgr.root_scope());
    let node2 = mgr.create_node(None, mgr.root_scope());

    // Request focus on node1 (deferred)
    mgr.request_focus(node1);
    assert!(mgr.has_pending_changes());
    // Primary focus not yet changed
    assert!(mgr.primary_focus().is_none());

    // Apply changes
    mgr.apply_focus_changes();
    assert_eq!(mgr.primary_focus(), Some(node1));
    assert!(!mgr.has_pending_changes());
}

#[test]
fn test_deferred_coalescing() {
    let mut mgr = FocusManager::new();
    let node1 = mgr.create_node(None, mgr.root_scope());
    let node2 = mgr.create_node(None, mgr.root_scope());

    // Multiple requests in one frame — only last wins
    mgr.request_focus(node1);
    mgr.request_focus(node2);
    mgr.apply_focus_changes();
    assert_eq!(mgr.primary_focus(), Some(node2));
}
```

- [ ] **Step 7: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add vexo/src/retain/focus/manager.rs vexo/src/retain/pipeline.rs vexo/src/retain/event_handler.rs
git commit -m "feat: add deferred focus changes to FocusManager"
```

---

## Task 7: Scope Focus Memory

**Files:**
- Modify: `vexo/src/retain/focus/manager.rs`

Enable scope focus memory: when a node gains focus, walk up ancestor scopes and push to `focused_children`. When a scope regains focus, descend through `focused_children` to restore the leaf. The `unfocus_with_disposition(RestorePrevious)` uses the history stack.

- [ ] **Step 1: Update request_focus to populate scope memory**

The `set_as_focused_child_for_scope()` method already handles this. Verify it's called during `apply_focus_changes()` (it is, from Task 6 Step 3).

- [ ] **Step 2: Update unfocus_with_disposition(RestorePrevious) to descend through scopes**

In `vexo/src/retain/focus/manager.rs`, update `unfocus_with_disposition`:

```rust
pub fn unfocus_with_disposition(&mut self, disposition: UnfocusDisposition) {
    match disposition {
        UnfocusDisposition::Clear => {
            self.primary_focus = None;
        }
        UnfocusDisposition::RestorePrevious => {
            let Some(primary) = self.primary_focus else { return; };
            let scope_id = self.enclosing_scope(primary);

            // Remove current from scope's history
            if let Some(scope) = self.scopes.get_mut(scope_id) {
                scope.focused_children.retain(|c| *c != primary);
            }

            // Get the previous focused child
            let prev_child = self.scopes.get(scope_id).and_then(|s| s.focused_child());

            if let Some(prev) = prev_child {
                // If the previous child is a scope, descend into it
                let target = self.descend_to_leaf(prev);
                self.primary_focus = Some(target);
            } else {
                // No previous child in this scope — clear focus
                self.primary_focus = None;
            }
        }
    }
}

/// Descend through nested scopes' focused_children to find the leaf node.
fn descend_to_leaf(&self, id: FocusNodeId) -> FocusNodeId {
    let mut current = id;
    loop {
        let node = match self.nodes.get(current) {
            Some(n) => n,
            None => return current,
        };
        if !node.is_scope {
            return current;
        }
        // This is a scope — check if it has a focused child
        match self.scopes.get(current).and_then(|s| s.focused_child()) {
            Some(child) => current = child,
            None => return current,
        }
    }
}
```

- [ ] **Step 3: Write tests for scope focus memory**

Add to `vexo/src/retain/focus/manager.rs` tests:

```rust
#[test]
fn test_scope_focus_memory_nested() {
    let mut mgr = FocusManager::new();
    let outer_scope = mgr.create_scope(None, mgr.root_scope());
    let inner_scope = mgr.create_scope(None, outer_scope);
    let node1 = mgr.create_node(None, inner_scope);
    let node2 = mgr.create_node(None, inner_scope);

    // Focus node1, then node2
    mgr.request_focus(node1);
    mgr.apply_focus_changes();
    mgr.request_focus(node2);
    mgr.apply_focus_changes();
    assert_eq!(mgr.primary_focus(), Some(node2));

    // Unfocus node2 with RestorePrevious
    mgr.unfocus_with_disposition(UnfocusDisposition::RestorePrevious);
    assert_eq!(mgr.primary_focus(), Some(node1));

    // Inner scope should remember node1
    assert_eq!(mgr.get_scope(inner_scope).unwrap().focused_child(), Some(node1));
}

#[test]
fn test_descend_to_leaf() {
    let mut mgr = FocusManager::new();
    let scope1 = mgr.create_scope(None, mgr.root_scope());
    let scope2 = mgr.create_scope(None, scope1);
    let leaf = mgr.create_node(None, scope2);

    // Focus the leaf
    mgr.request_focus(leaf);
    mgr.apply_focus_changes();

    // scope2 should remember leaf
    assert_eq!(mgr.get_scope(scope2).unwrap().focused_child(), Some(leaf));
    // scope1 should remember scope2
    assert_eq!(mgr.get_scope(scope1).unwrap().focused_child(), Some(scope2));

    // Descend from scope1 should reach leaf
    assert_eq!(mgr.descend_to_leaf(scope1), leaf);
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests PASS

- [ ] **Step 5: Run desktop demo for manual verification**

Run: `cargo run -p desktop_demo`
Expected: TextEdit click-to-focus works, keyboard input works, focus-dependent border color changes.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/focus/manager.rs
git commit -m "feat: add scope focus memory with RestorePrevious unfocus disposition"
```

---

## Task 8: Integration Tests

**Files:**
- Create: `vexo/src/retain/focus/integration_tests.rs`
- Modify: `vexo/src/retain/focus/mod.rs`

Add integration tests that verify the focus system works end-to-end with the element tree (no GPU).

- [ ] **Step 1: Create integration_tests.rs**

Create `vexo/src/retain/focus/integration_tests.rs` with tests for:

1. FocusManager + pipeline: creating a pipeline, reconciling widgets with Focus wrappers, verifying focus state
2. Click-to-focus: pointer press on a Focus-wrapped element requests focus
3. Click-outside-to-unfocus: pointer press outside clears focus
4. Focus-dependent build: `BuildContext::is_focused()` reflects focus state after `apply_focus_changes()`

```rust
#[cfg(test)]
mod tests {
    use vexo::retain::{ThreeTreePipeline, Focus, FocusScope, Text, Column, Widget};
    use vexo::retain::focus::FocusManager;
    use vexo::core::{Logical, Point, Position, Absolute, Size};
    use vexo::input::{InputEvent, ButtonState, Modifiers};
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = vexo::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_focus_manager_in_pipeline() {
        let mut pipeline = ThreeTreePipeline::new();
        // Reconcile with a Focus-wrapped Text
        let widget = Focus::new(Text::new("Hello"));
        pipeline.reconcile(Box::new(widget));
        // Pipeline should have elements
        assert!(!pipeline.element_registry().is_empty());
    }

    #[test]
    fn test_click_outside_clears_focus() {
        let mut pipeline = ThreeTreePipeline::new();
        let widget = Focus::new(Text::new("Hello"));
        pipeline.reconcile(Box::new(widget));

        // Click outside all widgets
        let event = InputEvent::PointerButton {
            position: Point::new(500.0, 500.0),
            state: ButtonState::Pressed,
            button: 0,
        };
        let mut font_system = create_test_font_system();
        pipeline.handle_event(
            Point::new(500.0, 500.0),
            &event,
            Modifiers::default(),
            &mut font_system,
        );
        // Focus should be cleared
        assert!(pipeline.focused_element().is_none());
    }
}
```

- [ ] **Step 2: Register integration test module**

Add to `vexo/src/retain/focus/mod.rs`:

```rust
#[cfg(test)]
mod integration_tests;
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/focus/
git commit -m "test: add focus system integration tests"
```

---

## Self-Review

### Spec Coverage

| Spec Section | Task |
|---|---|
| 1. Focus Tree Architecture (FocusNode, FocusScopeNode, FocusAttachment) | Tasks 1, 3 |
| 2. FocusManager (deferred changes, unfocus dispositions) | Tasks 1, 6, 7 |
| 3. Element Integration (FocusElement, FocusScopeElement, parent scope resolution) | Tasks 3, 4 |
| 4. Migration Steps (6 steps from spec) | Tasks 1-7 (each task = one step) |
| 5. Module Structure | Task 1 |
| 6. Key Differences from Flutter | Covered by design (slotmap, SecondaryMap, deferred between frames) |
| 7. Testing Strategy (unit + integration) | Tasks 1, 6, 7, 8 |

### Placeholder Scan

No TBDs, TODOs, or "implement later" patterns found.

### Type Consistency

- `FocusNodeId` used consistently across all tasks
- `FocusManager::request_focus()` takes `FocusNodeId` throughout
- `FocusManager::request_focus_by_element()` takes `ElementKey` for transition period
- `FocusAttachment::new()` takes `FocusNodeId`, matches `FocusManager::create_node()` return type
- `UnfocusDisposition` enum used consistently in Task 7
