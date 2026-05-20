# Flutter-Style Focus System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Vexo's flat single-slot focus model with a Flutter-style sparse focus tree supporting scopes, traversal, callbacks, and keyboard tokens.

**Architecture:** A standalone `FocusManager` module with slotmap-stored `FocusNodeData` and `FocusScopeData`. Elements opt in via `FocusElement` and `FocusScopeElement`. The focus tree mirrors the element tree at the sparse subset where these elements exist. Focus changes are deferred between frames.

**Tech Stack:** Rust, slotmap (already in workspace), SecondaryMap for scope data

---

## File Structure

### New Files
- `vexo/src/retain/focus/mod.rs` — Public API exports
- `vexo/src/retain/focus/key.rs` — `FocusNodeKey` slotmap key type
- `vexo/src/retain/focus/node.rs` — `FocusNodeData` struct
- `vexo/src/retain/focus/scope.rs` — `FocusScopeData`, `UnfocusDisposition`
- `vexo/src/retain/focus/manager.rs` — `FocusManager` (slotmap, primary_focus, root_scope, deferred callbacks)
- `vexo/src/retain/focus/traversal.rs` — `TraversalPolicy` trait, `WidgetOrderPolicy`
- `vexo/src/retain/focus/element.rs` — `FocusElement`, `FocusScopeElement`
- `vexo/src/retain/focus/widget.rs` — `Focus` widget, `FocusScope` widget

### Modified Files
- `vexo/src/retain/mod.rs` — Add `focus` module, update exports
- `vexo/src/retain/pipeline.rs` — Replace `focused_element: Option<ElementKey>` with `FocusManager`, update `handle_event`, `sync_focus_to_build_owner`
- `vexo/src/retain/event_handler.rs` — Route focus requests through `FocusManager`, handle Tab/Shift+Tab
- `vexo/src/retain/event_context.rs` — Replace `focus_request`/`clear_focus_request` with `FocusManager` reference
- `vexo/src/retain/build_owner.rs` — Remove `focused_element` field (read from `FocusManager` instead)
- `vexo/src/retain/element_context.rs` — Add `focus_manager` reference for mount/unmount lifecycle
- `vexo/src/retain/stateful_widget.rs` — Update `BuildContext::is_focused()` to read from `FocusManager`
- `vexo/src/retain/widgets/mod.rs` — Add `Focus`, `FocusScope` to widget exports
- `vexo/src/retain/widgets/text_edit.rs` — Remove direct focus request from `StatefulElement`, rely on `FocusElement` wrapper

---

## Task 1: FocusNodeKey and FocusNodeData

**Files:**
- Create: `vexo/src/retain/focus/key.rs`
- Create: `vexo/src/retain/focus/node.rs`
- Create: `vexo/src/retain/focus/mod.rs`
- Test: inline in `node.rs`

- [ ] **Step 1: Create the focus module directory and key type**

```rust
// vexo/src/retain/focus/key.rs
use slotmap::new_key_type;

new_key_type! {
    pub struct FocusNodeKey;
}
```

- [ ] **Step 2: Create FocusNodeData**

```rust
// vexo/src/retain/focus/node.rs
use std::any::Any;

use super::key::FocusNodeKey;
use crate::core::Rect;
use crate::retain::id::ElementKey;

/// Data for a focus node in the focus tree.
///
/// Focus nodes form a sparse tree that mirrors the element tree
/// at the points where FocusElement/FocusScopeElement exist.
pub struct FocusNodeData {
    /// Parent in the focus tree.
    pub parent: Option<FocusNodeKey>,
    /// Children in the focus tree (ordered, determines traversal order).
    pub children: Vec<FocusNodeKey>,
    /// Callback when this node gains primary focus.
    pub on_focus_gained: Option<Box<dyn Fn()>>,
    /// Callback when this node loses primary focus.
    pub on_focus_lost: Option<Box<dyn Fn()>>,
    /// Whether this node can receive focus via request_focus().
    pub can_request_focus: bool,
    /// Whether this node is skipped during Tab traversal.
    pub skip_traversal: bool,
    /// Keyboard token: true when focus was user-initiated.
    pub keyboard_token: bool,
    /// The element this node is associated with (for keyboard dispatch).
    pub element_key: Option<ElementKey>,
    /// Cached layout rect for reading-order traversal.
    pub layout_rect: Option<Rect>,
}

impl FocusNodeData {
    pub fn new() -> Self {
        Self {
            parent: None,
            children: Vec::new(),
            on_focus_gained: None,
            on_focus_lost: None,
            can_request_focus: true,
            skip_traversal: false,
            keyboard_token: false,
            element_key: None,
            layout_rect: None,
        }
    }

    /// Check if this node has primary focus.
    pub fn has_primary_focus(&self, primary: Option<FocusNodeKey>, own_key: FocusNodeKey) -> bool {
        primary == Some(own_key)
    }

    /// Consume the keyboard token, returning its previous value.
    pub fn consume_keyboard_token(&mut self) -> bool {
        let token = self.keyboard_token;
        self.keyboard_token = false;
        token
    }
}

impl Default for FocusNodeData {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 3: Create the module root with exports**

```rust
// vexo/src/retain/focus/mod.rs
mod key;
mod node;

pub use key::FocusNodeKey;
pub use node::FocusNodeData;
```

- [ ] **Step 4: Add focus module to retain/mod.rs**

Add `mod focus;` after `mod child_ops;` and add `pub use focus::{FocusNodeKey, FocusNodeData};` to the exports.

- [ ] **Step 5: Build and verify**

Run: `cargo build -p vexo 2>&1 | head -20`
Expected: BUILD SUCCEEDS (no errors, only unused warnings)

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/focus/ vexo/src/retain/mod.rs
git commit -m "feat: add FocusNodeKey and FocusNodeData for focus tree"
```

---

## Task 2: FocusScopeData and UnfocusDisposition

**Files:**
- Create: `vexo/src/retain/focus/scope.rs`
- Modify: `vexo/src/retain/focus/mod.rs`

- [ ] **Step 1: Create FocusScopeData and UnfocusDisposition**

```rust
// vexo/src/retain/focus/scope.rs
use super::key::FocusNodeKey;
use super::traversal::TraversalPolicy;

/// How to handle unfocusing.
pub enum UnfocusDisposition {
    /// Restore the previously focused child in the scope.
    RestorePrevious,
    /// Clear focus entirely.
    Clear,
}

/// Extra data for a focus scope node.
///
/// Stored in a SecondaryMap keyed by FocusNodeKey.
/// A node is a scope if it has an entry in this map.
pub struct FocusScopeData {
    /// The most recently focused child in this scope.
    pub focused_child: Option<FocusNodeKey>,
    /// Stack of previously focused children (for restore-on-unfocus).
    pub focused_child_history: Vec<FocusNodeKey>,
    /// How Tab/Shift+Tab navigates within this scope.
    pub traversal_policy: TraversalPolicy,
}

impl FocusScopeData {
    pub fn new() -> Self {
        Self {
            focused_child: None,
            focused_child_history: Vec::new(),
            traversal_policy: TraversalPolicy::WidgetOrder,
        }
    }

    /// Push a child onto the focused history stack.
    pub fn push_focused_child(&mut self, child: FocusNodeKey) {
        if self.focused_child != Some(child) {
            if let Some(old) = self.focused_child.take() {
                self.focused_child_history.push(old);
            }
            self.focused_child = Some(child);
        }
    }

    /// Pop the most recent focused child from the history stack.
    pub fn pop_focused_child(&mut self) -> Option<FocusNodeKey> {
        self.focused_child.take().or_else(|| self.focused_child_history.pop())
    }
}

impl Default for FocusScopeData {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Create TraversalPolicy (minimal — WidgetOrder only for now)**

```rust
// vexo/src/retain/focus/traversal.rs
use super::key::FocusNodeKey;
use super::manager::FocusManager;

/// Policy for focus traversal (Tab/Shift+Tab) within a scope.
#[derive(Clone, Debug, PartialEq)]
pub enum TraversalPolicy {
    /// Tab order follows the order nodes were added as children.
    WidgetOrder,
    /// Tab order follows visual reading order (left-to-right, top-to-bottom).
    /// Requires layout rect data.
    ReadingOrder,
}

impl TraversalPolicy {
    /// Find the first focusable node in the scope.
    pub fn find_first(&self, scope: FocusNodeKey, manager: &FocusManager) -> Option<FocusNodeKey> {
        match self {
            TraversalPolicy::WidgetOrder => {
                let children = manager.children(scope);
                for &child in &children {
                    if manager.can_request_focus(child) && !manager.skip_traversal(child) {
                        // If child is a scope, recurse into it
                        if manager.is_scope(child) {
                            if let Some(first) = self.find_first(child, manager) {
                                return Some(first);
                            }
                        } else {
                            return Some(child);
                        }
                    }
                }
                None
            }
            TraversalPolicy::ReadingOrder => {
                // Deferred: requires layout rects
                None
            }
        }
    }

    /// Find the next focusable node after `current` in the scope.
    pub fn next(&self, current: FocusNodeKey, scope: FocusNodeKey, manager: &FocusManager) -> Option<FocusNodeKey> {
        match self {
            TraversalPolicy::WidgetOrder => {
                let children = manager.children(scope);
                let current_idx = children.iter().position(|&c| c == current)?;
                // Search forward from current+1, wrapping around
                let len = children.len();
                for i in 1..=len {
                    let idx = (current_idx + i) % len;
                    let child = children[idx];
                    if manager.can_request_focus(child) && !manager.skip_traversal(child) {
                        if manager.is_scope(child) {
                            if let Some(first) = self.find_first(child, manager) {
                                return Some(first);
                            }
                        } else {
                            return Some(child);
                        }
                    }
                }
                None
            }
            TraversalPolicy::ReadingOrder => None,
        }
    }

    /// Find the previous focusable node before `current` in the scope.
    pub fn previous(&self, current: FocusNodeKey, scope: FocusNodeKey, manager: &FocusManager) -> Option<FocusNodeKey> {
        match self {
            TraversalPolicy::WidgetOrder => {
                let children = manager.children(scope);
                let current_idx = children.iter().position(|&c| c == current)?;
                // Search backward from current-1, wrapping around
                let len = children.len();
                for i in 1..=len {
                    let idx = (current_idx + len - i) % len;
                    let child = children[idx];
                    if manager.can_request_focus(child) && !manager.skip_traversal(child) {
                        if manager.is_scope(child) {
                            if let Some(last) = self.find_last(child, manager) {
                                return Some(last);
                            }
                        } else {
                            return Some(child);
                        }
                    }
                }
                None
            }
            TraversalPolicy::ReadingOrder => None,
        }
    }

    /// Find the last focusable node in the scope.
    pub fn find_last(&self, scope: FocusNodeKey, manager: &FocusManager) -> Option<FocusNodeKey> {
        match self {
            TraversalPolicy::WidgetOrder => {
                let children = manager.children(scope);
                for &child in children.iter().rev() {
                    if manager.can_request_focus(child) && !manager.skip_traversal(child) {
                        if manager.is_scope(child) {
                            if let Some(last) = self.find_last(child, manager) {
                                return Some(last);
                            }
                        } else {
                            return Some(child);
                        }
                    }
                }
                None
            }
            TraversalPolicy::ReadingOrder => None,
        }
    }
}
```

- [ ] **Step 3: Update focus/mod.rs with new exports**

```rust
// vexo/src/retain/focus/mod.rs
mod key;
mod node;
mod scope;
mod traversal;

pub use key::FocusNodeKey;
pub use node::FocusNodeData;
pub use scope::{FocusScopeData, UnfocusDisposition};
pub use traversal::TraversalPolicy;
```

- [ ] **Step 4: Build and verify**

Run: `cargo build -p vexo 2>&1 | head -30`
Expected: FAIL — `TraversalPolicy` references `FocusManager` which doesn't exist yet. This is expected; we'll create FocusManager in Task 3.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/focus/
git commit -m "feat: add FocusScopeData, UnfocusDisposition, and TraversalPolicy"
```

---

## Task 3: FocusManager Core

**Files:**
- Create: `vexo/src/retain/focus/manager.rs`
- Modify: `vexo/src/retain/focus/mod.rs`

- [ ] **Step 1: Create FocusManager**

```rust
// vexo/src/retain/focus/manager.rs
use slotmap::{SlotMap, SecondaryMap};

use super::key::FocusNodeKey;
use super::node::FocusNodeData;
use super::scope::{FocusScopeData, UnfocusDisposition};
use crate::retain::id::ElementKey;

/// Manages the focus tree — a sparse tree of focus nodes
/// that mirrors the element tree at focusable points.
pub struct FocusManager {
    /// All focus nodes (including scopes).
    nodes: SlotMap<FocusNodeKey, FocusNodeData>,
    /// Extra data for scope nodes. A node is a scope if it has an entry here.
    scopes: SecondaryMap<FocusNodeKey, FocusScopeData>,
    /// The currently focused node (primary focus).
    primary_focus: Option<FocusNodeKey>,
    /// The root scope of the focus tree.
    root_scope: FocusNodeKey,
    /// Nodes that gained focus this frame (deferred callbacks).
    pending_focus_gained: Vec<FocusNodeKey>,
    /// Nodes that lost focus this frame (deferred callbacks).
    pending_focus_lost: Vec<FocusNodeKey>,
}

impl FocusManager {
    /// Create a new FocusManager with a root scope.
    pub fn new() -> Self {
        let mut nodes: SlotMap<FocusNodeKey, FocusNodeData> = SlotMap::with_key();
        let mut scopes = SecondaryMap::new();

        let root_data = FocusNodeData::new();
        let root_key = nodes.insert(root_data);
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

    /// Create a new focus node and attach it to a parent.
    pub fn create_node(&mut self, parent: Option<FocusNodeKey>) -> FocusNodeKey {
        let parent_key = parent.unwrap_or(self.root_scope);
        let mut data = FocusNodeData::new();
        data.parent = Some(parent_key);
        let key = self.nodes.insert(data);
        self.nodes.get_mut(parent_key).unwrap().children.push(key);
        key
    }

    /// Create a new focus scope node and attach it to a parent.
    pub fn create_scope(&mut self, parent: Option<FocusNodeKey>) -> FocusNodeKey {
        let key = self.create_node(parent);
        self.scopes.insert(key, FocusScopeData::new());
        key
    }

    /// Remove a focus node from the tree.
    pub fn remove_node(&mut self, key: FocusNodeKey) {
        // Remove from parent's children list
        if let Some(node) = self.nodes.get(key) {
            if let Some(parent_key) = node.parent {
                if let Some(parent) = self.nodes.get_mut(parent_key) {
                    parent.children.retain(|&c| c != key);
                }
            }
        }

        // If this node was focused, clear focus
        if self.primary_focus == Some(key) {
            self.primary_focus = None;
        }

        // If this node was a scope's focused_child, clear it
        if let Some(node) = self.nodes.get(key) {
            if let Some(parent_key) = node.parent {
                if let Some(scope) = self.scopes.get_mut(parent_key) {
                    if scope.focused_child == Some(key) {
                        scope.focused_child = scope.focused_child_history.pop();
                    }
                }
            }
        }

        // Remove scope data if present
        self.scopes.remove(key);

        // Remove the node itself
        self.nodes.remove(key);
    }

    /// Request focus for a node.
    ///
    /// `user_initiated` should be true for pointer clicks and explicit
    /// user actions, false for programmatic requests like autofocus.
    pub fn request_focus(&mut self, key: FocusNodeKey, user_initiated: bool) {
        if !self.can_request_focus(key) {
            return;
        }

        let old_focus = self.primary_focus;

        // Update enclosing scopes' focused_child
        self.set_focused_child_chain(key);

        // Set primary focus
        self.primary_focus = Some(key);

        // Set keyboard token
        if let Some(node) = self.nodes.get_mut(key) {
            node.keyboard_token = user_initiated;
        }

        // Queue callbacks
        if old_focus != Some(key) {
            if let Some(old) = old_focus {
                self.pending_focus_lost.push(old);
            }
            self.pending_focus_gained.push(key);
        }
    }

    /// Unfocus with the given disposition.
    pub fn unfocus(&mut self, disposition: UnfocusDisposition) {
        let old_focus = self.primary_focus;
        if old_focus.is_none() {
            return;
        }

        match disposition {
            UnfocusDisposition::Clear => {
                self.primary_focus = None;
            }
            UnfocusDisposition::RestorePrevious => {
                // Find the enclosing scope and restore its previous focused child
                if let Some(old) = old_focus {
                    if let Some(node) = self.nodes.get(old) {
                        if let Some(parent_key) = node.parent {
                            if let Some(scope) = self.scopes.get_mut(parent_key) {
                                let restored = scope.pop_focused_child();
                                self.primary_focus = restored;
                            } else {
                                self.primary_focus = None;
                            }
                        }
                    }
                }
            }
        }

        // Queue callbacks
        if let Some(old) = old_focus {
            if self.primary_focus != Some(old) {
                self.pending_focus_lost.push(old);
            }
        }
        if let Some(new) = self.primary_focus {
            if old_focus != Some(new) {
                self.pending_focus_gained.push(new);
            }
        }
    }

    /// Dispatch deferred focus change callbacks.
    ///
    /// Call this after event processing is complete (between frames).
    pub fn dispatch_focus_changes(&mut self) {
        let lost: Vec<FocusNodeKey> = self.pending_focus_lost.drain(..).collect();
        let gained: Vec<FocusNodeKey> = self.pending_focus_gained.drain(..).collect();

        for key in lost {
            if let Some(node) = self.nodes.get(key) {
                if let Some(ref cb) = node.on_focus_lost {
                    cb();
                }
            }
        }

        for key in gained {
            if let Some(node) = self.nodes.get(key) {
                if let Some(ref cb) = node.on_focus_gained {
                    cb();
                }
            }
        }
    }

    /// Get the primary focus node key.
    pub fn primary_focus(&self) -> Option<FocusNodeKey> {
        self.primary_focus
    }

    /// Get the element key for the primary focus node.
    pub fn focused_element(&self) -> Option<ElementKey> {
        self.primary_focus.and_then(|key| {
            self.nodes.get(key).and_then(|n| n.element_key)
        })
    }

    /// Check if a node has primary focus.
    pub fn is_focused(&self, key: FocusNodeKey) -> bool {
        self.primary_focus == Some(key)
    }

    /// Check if a node is on the focus chain (primary focus or ancestor of it).
    pub fn has_focus(&self, key: FocusNodeKey) -> bool {
        let mut current = self.primary_focus;
        while let Some(k) = current {
            if k == key {
                return true;
            }
            current = self.nodes.get(k).and_then(|n| n.parent);
        }
        false
    }

    /// Get the focus chain from primary focus to root.
    pub fn focus_chain(&self) -> Vec<FocusNodeKey> {
        let mut chain = Vec::new();
        let mut current = self.primary_focus;
        while let Some(k) = current {
            chain.push(k);
            current = self.nodes.get(k).and_then(|n| n.parent);
        }
        chain
    }

    /// Check if a node can request focus.
    pub fn can_request_focus(&self, key: FocusNodeKey) -> bool {
        self.nodes.get(key).map(|n| n.can_request_focus).unwrap_or(false)
    }

    /// Check if a node is skipped during traversal.
    pub fn skip_traversal(&self, key: FocusNodeKey) -> bool {
        self.nodes.get(key).map(|n| n.skip_traversal).unwrap_or(true)
    }

    /// Check if a node is a scope.
    pub fn is_scope(&self, key: FocusNodeKey) -> bool {
        self.scopes.contains_key(key)
    }

    /// Get the children of a node.
    pub fn children(&self, key: FocusNodeKey) -> Vec<FocusNodeKey> {
        self.nodes.get(key).map(|n| n.children.clone()).unwrap_or_default()
    }

    /// Get the root scope key.
    pub fn root_scope(&self) -> FocusNodeKey {
        self.root_scope
    }

    /// Find the enclosing scope for a node.
    pub fn enclosing_scope(&self, key: FocusNodeKey) -> Option<FocusNodeKey> {
        let mut current = self.nodes.get(key).and_then(|n| n.parent);
        while let Some(k) = current {
            if self.scopes.contains_key(k) {
                return Some(k);
            }
            current = self.nodes.get(k).and_then(|n| n.parent);
        }
        None
    }

    /// Get mutable access to a node's data.
    pub fn get_node_mut(&mut self, key: FocusNodeKey) -> Option<&mut FocusNodeData> {
        self.nodes.get_mut(key)
    }

    /// Get mutable access to a scope's data.
    pub fn get_scope_mut(&mut self, key: FocusNodeKey) -> Option<&mut FocusScopeData> {
        self.scopes.get_mut(key)
    }

    /// Consume the keyboard token for a node.
    pub fn consume_keyboard_token(&mut self, key: FocusNodeKey) -> bool {
        self.nodes.get_mut(key).map(|n| n.consume_keyboard_token()).unwrap_or(false)
    }

    /// Traverse forward (Tab) from the current focus.
    pub fn traverse_forward(&mut self) -> Option<FocusNodeKey> {
        let current = self.primary_focus?;
        let scope = self.enclosing_scope(current)?;
        let policy = self.scopes.get(scope).map(|s| s.traversal_policy.clone()).unwrap_or(TraversalPolicy::WidgetOrder);

        if let Some(next) = policy.next(current, scope, self) {
            self.request_focus(next, false);
            return Some(next);
        }

        // At boundary — try parent scope
        let parent_scope = self.nodes.get(scope).and_then(|n| n.parent).and_then(|p| {
            if self.scopes.contains_key(p) { Some(p) } else { None }
        });

        if let Some(parent) = parent_scope {
            let parent_policy = self.scopes.get(parent).map(|s| s.traversal_policy.clone()).unwrap_or(TraversalPolicy::WidgetOrder);
            if let Some(next) = parent_policy.next(scope, parent, self) {
                self.request_focus(next, false);
                return Some(next);
            }
        }

        // Wrap around to first in root scope
        let root_policy = self.scopes.get(self.root_scope).map(|s| s.traversal_policy.clone()).unwrap_or(TraversalPolicy::WidgetOrder);
        if let Some(first) = root_policy.find_first(self.root_scope, self) {
            self.request_focus(first, false);
            return Some(first);
        }

        None
    }

    /// Traverse backward (Shift+Tab) from the current focus.
    pub fn traverse_backward(&mut self) -> Option<FocusNodeKey> {
        let current = self.primary_focus?;
        let scope = self.enclosing_scope(current)?;
        let policy = self.scopes.get(scope).map(|s| s.traversal_policy.clone()).unwrap_or(TraversalPolicy::WidgetOrder);

        if let Some(prev) = policy.previous(current, scope, self) {
            self.request_focus(prev, false);
            return Some(prev);
        }

        // At boundary — try parent scope
        let parent_scope = self.nodes.get(scope).and_then(|n| n.parent).and_then(|p| {
            if self.scopes.contains_key(p) { Some(p) } else { None }
        });

        if let Some(parent) = parent_scope {
            let parent_policy = self.scopes.get(parent).map(|s| s.traversal_policy.clone()).unwrap_or(TraversalPolicy::WidgetOrder);
            if let Some(prev) = parent_policy.previous(scope, parent, self) {
                self.request_focus(prev, false);
                return Some(prev);
            }
        }

        // Wrap around to last in root scope
        let root_policy = self.scopes.get(self.root_scope).map(|s| s.traversal_policy.clone()).unwrap_or(TraversalPolicy::WidgetOrder);
        if let Some(last) = root_policy.find_last(self.root_scope, self) {
            self.request_focus(last, false);
            return Some(last);
        }

        None
    }

    /// Set the element key for a focus node.
    pub fn set_element_key(&mut self, key: FocusNodeKey, element_key: Option<ElementKey>) {
        if let Some(node) = self.nodes.get_mut(key) {
            node.element_key = element_key;
        }
    }

    /// Set callbacks for a focus node.
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

    /// Set can_request_focus for a node.
    pub fn set_can_request_focus(&mut self, key: FocusNodeKey, value: bool) {
        if let Some(node) = self.nodes.get_mut(key) {
            node.can_request_focus = value;
        }
    }

    /// Set skip_traversal for a node.
    pub fn set_skip_traversal(&mut self, key: FocusNodeKey, value: bool) {
        if let Some(node) = self.nodes.get_mut(key) {
            node.skip_traversal = value;
        }
    }

    /// Set the traversal policy for a scope.
    pub fn set_traversal_policy(&mut self, key: FocusNodeKey, policy: TraversalPolicy) {
        if let Some(scope) = self.scopes.get_mut(key) {
            scope.traversal_policy = policy;
        }
    }

    // ---- Private helpers ----

    /// Update enclosing scopes' focused_child to point toward `key`.
    fn set_focused_child_chain(&mut self, key: FocusNodeKey) {
        let mut current = Some(key);
        while let Some(k) = current {
            let parent_key = self.nodes.get(k).and_then(|n| n.parent);
            if let Some(pk) = parent_key {
                if let Some(scope) = self.scopes.get_mut(pk) {
                    scope.push_focused_child(k);
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
```

- [ ] **Step 2: Update focus/mod.rs to export FocusManager**

Add `mod manager;` and `pub use manager::FocusManager;` to `vexo/src/retain/focus/mod.rs`.

- [ ] **Step 3: Update retain/mod.rs to export FocusManager**

Add `pub use focus::FocusManager;` to the exports in `vexo/src/retain/mod.rs`.

- [ ] **Step 4: Build and verify**

Run: `cargo build -p vexo 2>&1 | head -30`
Expected: BUILD SUCCEEDS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/focus/ vexo/src/retain/mod.rs
git commit -m "feat: add FocusManager with request_focus, unfocus, traversal, deferred callbacks"
```

---

## Task 4: FocusManager Unit Tests

**Files:**
- Create: `vexo/src/retain/focus/manager_tests.rs`
- Modify: `vexo/src/retain/focus/mod.rs`

- [ ] **Step 1: Write comprehensive unit tests**

```rust
// vexo/src/retain/focus/manager_tests.rs
use super::manager::FocusManager;
use super::scope::UnfocusDisposition;
use super::traversal::TraversalPolicy;

#[test]
fn test_create_node() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    assert!(!mgr.is_focused(node));
    assert!(mgr.can_request_focus(node));
}

#[test]
fn test_create_scope() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    assert!(mgr.is_scope(scope));
}

#[test]
fn test_request_focus_basic() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.request_focus(node, true);
    assert!(mgr.is_focused(node));
    assert_eq!(mgr.primary_focus(), Some(node));
}

#[test]
fn test_request_focus_replaces_previous() {
    let mut mgr = FocusManager::new();
    let a = mgr.create_node(None);
    let b = mgr.create_node(None);
    mgr.request_focus(a, true);
    mgr.request_focus(b, true);
    assert!(!mgr.is_focused(a));
    assert!(mgr.is_focused(b));
}

#[test]
fn test_request_focus_can_request_focus_false() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.set_can_request_focus(node, false);
    mgr.request_focus(node, true);
    assert!(!mgr.is_focused(node));
}

#[test]
fn test_unfocus_clear() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.request_focus(node, true);
    mgr.unfocus(UnfocusDisposition::Clear);
    assert!(mgr.primary_focus().is_none());
}

#[test]
fn test_unfocus_restore_previous() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    let a = mgr.create_node(Some(scope));
    let b = mgr.create_node(Some(scope));
    mgr.request_focus(a, true);
    mgr.request_focus(b, true);
    mgr.unfocus(UnfocusDisposition::RestorePrevious);
    // Should restore to a (the previous focused child)
    assert_eq!(mgr.primary_focus(), Some(a));
}

#[test]
fn test_focus_chain() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    let node = mgr.create_node(Some(scope));
    mgr.request_focus(node, true);
    let chain = mgr.focus_chain();
    assert!(chain.contains(&node));
    assert!(chain.contains(&scope));
}

#[test]
fn test_has_focus_ancestor() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    let node = mgr.create_node(Some(scope));
    mgr.request_focus(node, true);
    assert!(mgr.has_focus(node));
    assert!(mgr.has_focus(scope));
}

#[test]
fn test_keyboard_token_user_initiated() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.request_focus(node, true);
    assert!(mgr.consume_keyboard_token(node));
    // Token consumed, second call returns false
    assert!(!mgr.consume_keyboard_token(node));
}

#[test]
fn test_keyboard_token_programmatic() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.request_focus(node, false);
    assert!(!mgr.consume_keyboard_token(node));
}

#[test]
fn test_remove_node() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.request_focus(node, true);
    mgr.remove_node(node);
    assert!(mgr.primary_focus().is_none());
}

#[test]
fn test_remove_node_from_parent_children() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    let node = mgr.create_node(Some(scope));
    assert!(mgr.children(scope).contains(&node));
    mgr.remove_node(node);
    assert!(!mgr.children(scope).contains(&node));
}

#[test]
fn test_skip_traversal() {
    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);
    mgr.set_skip_traversal(node, true);
    assert!(mgr.skip_traversal(node));
}

#[test]
fn test_traverse_forward_widget_order() {
    let mut mgr = FocusManager::new();
    let a = mgr.create_node(None);
    let b = mgr.create_node(None);
    let c = mgr.create_node(None);
    mgr.request_focus(a, true);
    let next = mgr.traverse_forward();
    assert_eq!(next, Some(b));
    assert!(mgr.is_focused(b));
}

#[test]
fn test_traverse_forward_wraps_around() {
    let mut mgr = FocusManager::new();
    let a = mgr.create_node(None);
    let b = mgr.create_node(None);
    mgr.request_focus(b, true);
    let next = mgr.traverse_forward();
    assert_eq!(next, Some(a));
}

#[test]
fn test_traverse_backward_widget_order() {
    let mut mgr = FocusManager::new();
    let a = mgr.create_node(None);
    let b = mgr.create_node(None);
    mgr.request_focus(b, true);
    let prev = mgr.traverse_backward();
    assert_eq!(prev, Some(a));
}

#[test]
fn test_traverse_skips_non_focusable() {
    let mut mgr = FocusManager::new();
    let a = mgr.create_node(None);
    let b = mgr.create_node(None);
    mgr.set_can_request_focus(b, false);
    let c = mgr.create_node(None);
    mgr.request_focus(a, true);
    let next = mgr.traverse_forward();
    assert_eq!(next, Some(c)); // skips b
}

#[test]
fn test_traverse_skips_skip_traversal() {
    let mut mgr = FocusManager::new();
    let a = mgr.create_node(None);
    let b = mgr.create_node(None);
    mgr.set_skip_traversal(b, true);
    let c = mgr.create_node(None);
    mgr.request_focus(a, true);
    let next = mgr.traverse_forward();
    assert_eq!(next, Some(c)); // skips b
}

#[test]
fn test_scope_traversal_policy() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    mgr.set_traversal_policy(scope, TraversalPolicy::WidgetOrder);
    let a = mgr.create_node(Some(scope));
    let b = mgr.create_node(Some(scope));
    mgr.request_focus(a, true);
    let next = mgr.traverse_forward();
    assert_eq!(next, Some(b));
}

#[test]
fn test_deferred_callbacks() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let gained_count = Rc::new(RefCell::new(0));
    let lost_count = Rc::new(RefCell::new(0));

    let mut mgr = FocusManager::new();
    let a = mgr.create_node(None);
    let b = mgr.create_node(None);

    let g = gained_count.clone();
    let l = lost_count.clone();
    mgr.set_callbacks(a, Some(Box::new(move || *g.borrow_mut() += 1)), Some(Box::new(move || *l.borrow_mut() += 1)));

    let g2 = gained_count.clone();
    let l2 = lost_count.clone();
    mgr.set_callbacks(b, Some(Box::new(move || *g2.borrow_mut() += 1)), Some(Box::new(move || *l2.borrow_mut() += 1)));

    mgr.request_focus(a, true);
    // Callbacks not fired yet (deferred)
    assert_eq!(*gained_count.borrow(), 0);

    mgr.dispatch_focus_changes();
    assert_eq!(*gained_count.borrow(), 1); // a gained
    assert_eq!(*lost_count.borrow(), 0);

    mgr.request_focus(b, true);
    mgr.dispatch_focus_changes();
    assert_eq!(*gained_count.borrow(), 2); // a gained, b gained
    assert_eq!(*lost_count.borrow(), 1); // a lost
}

#[test]
fn test_enclosing_scope() {
    let mut mgr = FocusManager::new();
    let scope = mgr.create_scope(None);
    let node = mgr.create_node(Some(scope));
    assert_eq!(mgr.enclosing_scope(node), Some(scope));
}

#[test]
fn test_focused_element() {
    use slotmap::SlotMap;
    use crate::retain::id::ElementKey;

    let mut mgr = FocusManager::new();
    let node = mgr.create_node(None);

    let mut sm: SlotMap<ElementKey, ()> = SlotMap::with_key();
    let elem_key = sm.insert(());

    mgr.set_element_key(node, Some(elem_key));
    mgr.request_focus(node, true);
    assert_eq!(mgr.focused_element(), Some(elem_key));
}
```

- [ ] **Step 2: Add test module to focus/mod.rs**

Add at the bottom of `vexo/src/retain/focus/mod.rs`:
```rust
#[cfg(test)]
mod manager_tests;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p vexo focus::manager_tests 2>&1 | tail -30`
Expected: ALL TESTS PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/focus/
git commit -m "test: add FocusManager unit tests"
```

---

## Task 5: Focus and FocusScope Widgets

**Files:**
- Create: `vexo/src/retain/focus/widget.rs`
- Create: `vexo/src/retain/focus/element.rs`
- Modify: `vexo/src/retain/focus/mod.rs`
- Modify: `vexo/src/retain/widgets/mod.rs`

- [ ] **Step 1: Create Focus widget**

```rust
// vexo/src/retain/focus/widget.rs
use std::any::Any;

use crate::retain::key::WidgetKey;
use crate::retain::focus::element::FocusElement;
use crate::retain::widgets::Widget;

/// Widget that makes its child focusable.
///
/// Wraps a child widget and creates a focus node in the focus tree.
/// When the child is clicked, it requests focus. When focused,
/// keyboard events are dispatched to the child.
pub struct Focus {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    autofocus: bool,
    can_request_focus: bool,
    skip_traversal: bool,
}

impl Focus {
    pub fn new(child: impl Widget) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            autofocus: false,
            can_request_focus: true,
            skip_traversal: false,
        }
    }

    pub fn key(mut self, key: WidgetKey) -> Self {
        self.key = Some(key);
        self
    }

    pub fn autofocus(mut self, autofocus: bool) -> Self {
        self.autofocus = autofocus;
        self
    }

    pub fn can_request_focus(mut self, can: bool) -> Self {
        self.can_request_focus = can;
        self
    }

    pub fn skip_traversal(mut self, skip: bool) -> Self {
        self.skip_traversal = skip;
        self
    }

    pub fn autofocus_value(&self) -> bool {
        self.autofocus
    }

    pub fn can_request_focus_value(&self) -> bool {
        self.can_request_focus
    }

    pub fn skip_traversal_value(&self) -> bool {
        self.skip_traversal
    }
}

impl Widget for Focus {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn crate::retain::Element> {
        Box::new(FocusElement::new())
    }

    fn create_render_object(&self) -> Box<dyn crate::retain::RenderObject> {
        Box::new(crate::retain::ProxyRenderObject::new())
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Focus>()
            .map(|o| self.key == o.key)
            .unwrap_or(false)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            autofocus: self.autofocus,
            can_request_focus: self.can_request_focus,
            skip_traversal: self.skip_traversal,
        })
    }
}

/// Widget that creates a focus scope boundary.
///
/// Focus traversal (Tab/Shift+Tab) stays within a scope unless
/// explicitly broken. Each scope can have its own traversal policy.
pub struct FocusScope {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    traversal_policy: crate::retain::focus::TraversalPolicy,
}

impl FocusScope {
    pub fn new(child: impl Widget) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            traversal_policy: crate::retain::focus::TraversalPolicy::WidgetOrder,
        }
    }

    pub fn key(mut self, key: WidgetKey) -> Self {
        self.key = Some(key);
        self
    }

    pub fn policy(mut self, policy: crate::retain::focus::TraversalPolicy) -> Self {
        self.traversal_policy = policy;
        self
    }

    pub fn traversal_policy_value(&self) -> &crate::retain::focus::TraversalPolicy {
        &self.traversal_policy
    }
}

impl Widget for FocusScope {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn crate::retain::Element> {
        Box::new(FocusScopeElement::new())
    }

    fn create_render_object(&self) -> Box<dyn crate::retain::RenderObject> {
        Box::new(crate::retain::ProxyRenderObject::new())
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<FocusScope>()
            .map(|o| self.key == o.key)
            .unwrap_or(false)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            traversal_policy: self.traversal_policy.clone(),
        })
    }
}
```

- [ ] **Step 2: Create FocusElement and FocusScopeElement**

```rust
// vexo/src/retain/focus/element.rs
use std::any::Any;

use crate::input::{ButtonState, InputEvent};
use crate::retain::elements::render_object_element::RenderObjectElement;
use crate::retain::element_context::ElementContext;
use crate::retain::event_context::EventContext;
use crate::retain::focus::key::FocusNodeKey;
use crate::retain::focus::widget::{Focus, FocusScope};
use crate::retain::id::RenderObjectKey;
use crate::retain::key::WidgetKey;
use crate::retain::widgets::Widget;

/// Element for the Focus widget.
///
/// Creates a focus node in the focus tree on mount, removes it on unmount.
/// Handles pointer press by requesting focus. Passes all events to child.
pub struct FocusElement {
    id: Option<crate::retain::id::ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_node: Option<FocusNodeKey>,
}

impl FocusElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_node: None,
        }
    }

    fn get_child_widget(&self) -> Option<Box<dyn Widget>> {
        self.widget.as_ref()?.child().map(|w| w.clone_boxed())
    }
}

impl RenderObjectElement for FocusElement {
    fn widget(&self) -> Option<&Box<dyn Widget>> { self.widget.as_ref() }
    fn set_widget(&mut self, widget: &dyn Any) {
        if let Some(focus) = widget.downcast_ref::<Focus>() {
            self.widget = Some(focus.clone_boxed());
            self.key = focus.key().clone();
        }
    }
    fn render_object_id(&self) -> Option<RenderObjectKey> { self.render_object }
    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) { self.render_object = id; }
    fn stored_key(&self) -> Option<WidgetKey> { self.key.clone() }
    fn set_stored_key(&mut self, key: Option<WidgetKey>) { self.key = key; }
    fn element_id(&self) -> Option<crate::retain::id::ElementKey> { self.id }
    fn set_element_id(&mut self, id: Option<crate::retain::id::ElementKey>) { self.id = id; }
}

impl crate::retain::Element for FocusElement {
    fn mount(&mut self, context: &mut ElementContext) {
        self.mount_render_object(context);

        // Create focus node in the focus tree
        if let Some(focus_mgr) = context.focus_manager() {
            let parent_focus = context.parent_focus_node();
            let node_key = focus_mgr.create_node(parent_focus);
            focus_mgr.set_element_key(node_key, self.id);
            self.focus_node = Some(node_key);

            // Apply widget configuration
            if let Some(ref widget) = self.widget {
                if let Some(focus) = widget.as_any().downcast_ref::<Focus>() {
                    focus_mgr.set_can_request_focus(node_key, focus.can_request_focus_value());
                    focus_mgr.set_skip_traversal(node_key, focus.skip_traversal_value());

                    // Handle autofocus
                    if focus.autofocus_value() {
                        if let Some(scope) = focus_mgr.enclosing_scope(node_key) {
                            if focus_mgr.scopes_focused_child(scope).is_none() {
                                focus_mgr.request_focus(node_key, false);
                            }
                        }
                    }
                }
            }
        }

        // Inflate child
        if let Some(child) = self.get_child_widget() {
            context.inflate_child(None, child);
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Remove focus node from the focus tree
        if let Some(node_key) = self.focus_node.take() {
            if let Some(focus_mgr) = context.focus_manager() {
                focus_mgr.remove_node(node_key);
            }
        }
        self.unmount_render_object(context);
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast and store new widget
        if let Some(focus) = new_widget.downcast_ref::<Focus>() {
            self.widget = Some(focus.clone_boxed());
            self.key = focus.key().clone();

            // Update focus node configuration
            if let Some(node_key) = self.focus_node {
                if let Some(focus_mgr) = context.focus_manager() {
                    focus_mgr.set_can_request_focus(node_key, focus.can_request_focus_value());
                    focus_mgr.set_skip_traversal(node_key, focus.skip_traversal_value());
                }
            }
        }

        // Update render object
        if let Some(ref widget) = self.widget {
            self.update_render_object(widget.clone_boxed(), context);
        }

        // Reconcile single child
        let children = context.children.clone();
        let new_child = self.get_child_widget();

        if let Some(&child_id) = children.first() {
            if let Some(child_widget) = new_child {
                context.update_child(child_id, child_widget);
            } else {
                context.unmount_child(child_id);
            }
        } else if let Some(child_widget) = new_child {
            context.inflate_child(None, child_widget);
        }
    }

    fn on_event(&mut self, event: &InputEvent, context: &mut EventContext) -> Option<Box<dyn Any>> {
        // Request focus on pointer press inside bounds
        if let InputEvent::PointerButton { state: ButtonState::Pressed, .. } = event {
            if context.is_pointer_inside() {
                if let Some(node_key) = self.focus_node {
                    context.request_focus_via_manager(node_key, true);
                }
            }
        }
        None // Let event continue bubbling to child
    }

    fn render_object(&self) -> Option<RenderObjectKey> { self.render_object }
    fn widget_key(&self) -> Option<WidgetKey> { self.key.clone() }
    fn can_update(&self, widget: &dyn Any) -> bool {
        widget.downcast_ref::<Focus>()
            .map(|w| self.key == w.key())
            .unwrap_or(false)
    }

    fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
        self.insert_child_render_object(child_ro, context);
    }
}

/// Element for the FocusScope widget.
///
/// Creates a focus scope node in the focus tree on mount.
/// Does not handle events itself — just provides scope structure.
pub struct FocusScopeElement {
    id: Option<crate::retain::id::ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_node: Option<FocusNodeKey>,
}

impl FocusScopeElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_node: None,
        }
    }

    fn get_child_widget(&self) -> Option<Box<dyn Widget>> {
        self.widget.as_ref()?.child().map(|w| w.clone_boxed())
    }
}

impl RenderObjectElement for FocusScopeElement {
    fn widget(&self) -> Option<&Box<dyn Widget>> { self.widget.as_ref() }
    fn set_widget(&mut self, widget: &dyn Any) {
        if let Some(scope) = widget.downcast_ref::<FocusScope>() {
            self.widget = Some(scope.clone_boxed());
            self.key = scope.key().clone();
        }
    }
    fn render_object_id(&self) -> Option<RenderObjectKey> { self.render_object }
    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) { self.render_object = id; }
    fn stored_key(&self) -> Option<WidgetKey> { self.key.clone() }
    fn set_stored_key(&mut self, key: Option<WidgetKey>) { self.key = key; }
    fn element_id(&self) -> Option<crate::retain::id::ElementKey> { self.id }
    fn set_element_id(&mut self, id: Option<crate::retain::id::ElementKey>) { self.id = id; }
}

impl crate::retain::Element for FocusScopeElement {
    fn mount(&mut self, context: &mut ElementContext) {
        self.mount_render_object(context);

        // Create scope node in the focus tree
        if let Some(focus_mgr) = context.focus_manager() {
            let parent_focus = context.parent_focus_node();
            let node_key = focus_mgr.create_scope(parent_focus);
            focus_mgr.set_element_key(node_key, self.id);
            self.focus_node = Some(node_key);

            // Apply widget configuration
            if let Some(ref widget) = self.widget {
                if let Some(scope) = widget.as_any().downcast_ref::<FocusScope>() {
                    focus_mgr.set_traversal_policy(node_key, scope.traversal_policy_value().clone());
                }
            }
        }

        // Inflate child
        if let Some(child) = self.get_child_widget() {
            context.inflate_child(None, child);
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        if let Some(node_key) = self.focus_node.take() {
            if let Some(focus_mgr) = context.focus_manager() {
                focus_mgr.remove_node(node_key);
            }
        }
        self.unmount_render_object(context);
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Some(scope) = new_widget.downcast_ref::<FocusScope>() {
            self.widget = Some(scope.clone_boxed());
            self.key = scope.key().clone();

            if let Some(node_key) = self.focus_node {
                if let Some(focus_mgr) = context.focus_manager() {
                    focus_mgr.set_traversal_policy(node_key, scope.traversal_policy_value().clone());
                }
            }
        }

        if let Some(ref widget) = self.widget {
            self.update_render_object(widget.clone_boxed(), context);
        }

        let children = context.children.clone();
        let new_child = self.get_child_widget();

        if let Some(&child_id) = children.first() {
            if let Some(child_widget) = new_child {
                context.update_child(child_id, child_widget);
            } else {
                context.unmount_child(child_id);
            }
        } else if let Some(child_widget) = new_child {
            context.inflate_child(None, child_widget);
        }
    }

    fn render_object(&self) -> Option<RenderObjectKey> { self.render_object }
    fn widget_key(&self) -> Option<WidgetKey> { self.key.clone() }
    fn can_update(&self, widget: &dyn Any) -> bool {
        widget.downcast_ref::<FocusScope>()
            .map(|w| self.key == w.key())
            .unwrap_or(false)
    }

    fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
        self.insert_child_render_object(child_ro, context);
    }
}
```

- [ ] **Step 3: Update focus/mod.rs with element and widget exports**

Add `mod element;`, `mod widget;` and `pub use element::{FocusElement, FocusScopeElement};`, `pub use widget::{Focus, FocusScope};` to `vexo/src/retain/focus/mod.rs`.

- [ ] **Step 4: Update retain/mod.rs and widgets/mod.rs exports**

Add `Focus`, `FocusScope` to the widget re-exports in `vexo/src/retain/mod.rs`.

- [ ] **Step 5: Build and verify**

Run: `cargo build -p vexo 2>&1 | head -40`
Expected: May have compile errors due to missing methods on ElementContext/EventContext (focus_manager(), parent_focus_node(), request_focus_via_manager(), scopes_focused_child()). These will be added in Task 6. Fix any other errors first.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/retain/focus/ vexo/src/retain/mod.rs vexo/src/retain/widgets/mod.rs
git commit -m "feat: add Focus/FocusScope widgets and FocusElement/FocusScopeElement"
```

---

## Task 6: Integrate FocusManager into Pipeline and Contexts

**Files:**
- Modify: `vexo/src/retain/pipeline.rs` — Replace `focused_element` with `FocusManager`
- Modify: `vexo/src/retain/event_handler.rs` — Route through FocusManager, handle Tab
- Modify: `vexo/src/retain/event_context.rs` — Add FocusManager reference
- Modify: `vexo/src/retain/element_context.rs` — Add FocusManager reference
- Modify: `vexo/src/retain/build_owner.rs` — Remove `focused_element` field
- Modify: `vexo/src/retain/stateful_widget.rs` — Update BuildContext::is_focused()

This is the largest task. Each sub-step modifies a specific file.

- [ ] **Step 1: Add FocusManager to ThreeTreePipeline**

Replace `focused_element: Option<ElementKey>` with `focus_manager: FocusManager` in the pipeline struct. Update `new()`, remove `focused_element()`/`set_focus()` getters, add `focus_manager()`/`focus_manager_mut()` accessors. Update `sync_focus_to_build_owner()` to sync from `focus_manager.focused_element()`.

- [ ] **Step 2: Add focus_manager to ElementContext**

Add `focus_manager: &'a mut FocusManager` field to `ElementContext`. Add helper methods:
- `focus_manager(&mut self) -> Option<&mut FocusManager>` — returns Some (always available in retain mode)
- `parent_focus_node(&self) -> Option<FocusNodeKey>` — walks up element tree to find nearest element with a focus node key stored in StateStorage

- [ ] **Step 3: Add focus_manager to EventContext**

Add `focus_manager: Option<&'a mut FocusManager>` field to `EventContext`. Add method:
- `request_focus_via_manager(&mut self, node_key: FocusNodeKey, user_initiated: bool)` — calls `focus_manager.request_focus(node_key, user_initiated)`

Remove `focus_request` and `clear_focus_request` fields and their associated methods.

- [ ] **Step 4: Update EventHandler**

Update `handle_event()`, `handle_pointer_event()`, `handle_keyboard_event()` to:
- Accept `&mut FocusManager` instead of `&mut Option<ElementKey>`
- For pointer events: after hit test, if no hit on press, call `focus_manager.unfocus(UnfocusDisposition::Clear)`
- For pointer events: after each element's `on_event()`, no longer check `ctx.focus_request()`/`ctx.should_clear_focus()` — focus requests go directly to FocusManager
- For keyboard events: check for Tab/Shift+Tab and call `focus_manager.traverse_forward()`/`traverse_backward()`. For other keys, dispatch to `focus_manager.focused_element()`
- After event processing, call `focus_manager.dispatch_focus_changes()`

- [ ] **Step 5: Update BuildOwner**

Remove `focused_element: RefCell<Option<ElementKey>>` and its `focused_element()`/`set_focused_element()` methods. The pipeline will provide focus state via `FocusManager` directly.

- [ ] **Step 6: Update BuildContext::is_focused()**

Change `BuildContext::is_focused()` to read from `FocusManager` instead of `BuildOwner.focused_element()`. This requires `BuildContext` to have access to `FocusManager` (add a `focus_manager: &'a FocusManager` field or pass focus state as a parameter).

- [ ] **Step 7: Update all callers of the changed APIs**

Update `Reconciler`, `Layouter`, `Painter`, and test code to pass `FocusManager` references where needed. Fix all compile errors.

- [ ] **Step 8: Build and verify**

Run: `cargo build -p vexo 2>&1 | head -40`
Expected: BUILD SUCCEEDS

- [ ] **Step 9: Run existing tests**

Run: `cargo test -p vexo 2>&1 | tail -30`
Expected: ALL EXISTING TESTS PASS (some may need updates for the new API)

- [ ] **Step 10: Commit**

```bash
git add vexo/src/retain/
git commit -m "feat: integrate FocusManager into pipeline, event handler, and contexts"
```

---

## Task 7: Update StatefulElement Focus Behavior

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs`

- [ ] **Step 1: Remove direct focus request from StatefulElement::on_event()**

Currently `StatefulElement::on_event()` calls `context.request_focus(id)` on pointer press. With the new focus system, focus is managed by `FocusElement` wrappers. Remove the direct focus request from `StatefulElement::on_event()`.

- [ ] **Step 2: Update StatefulElement keyboard handling**

When a `StatefulElement` receives a keyboard event, it should check `focus_manager.is_focused()` using its associated `FocusNodeKey` (if any) instead of `context.is_focused(element_key)`.

- [ ] **Step 3: Build and verify**

Run: `cargo build -p vexo 2>&1 | head -20`
Expected: BUILD SUCCEEDS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "refactor: remove direct focus request from StatefulElement, rely on FocusElement"
```

---

## Task 8: Integration Tests

**Files:**
- Create: `vexo/src/retain/focus/integration_tests.rs`
- Modify: `vexo/src/retain/focus/mod.rs`

- [ ] **Step 1: Write integration tests**

Tests that exercise the focus system through the element tree:

1. **FocusElement mount/unmount** — create a pipeline with `Focus::new(Text::new("hello"))`, verify focus node exists in FocusManager after reconcile, removed after unmount
2. **FocusScopeElement** — create nested Focus/FocusScope, verify scope boundary respected during traversal
3. **Click-to-focus** — send pointer press event on a FocusElement, verify `focus_manager.primary_focus()` is set
4. **Click-outside-to-unfocus** — send pointer press event outside all focusable elements, verify focus cleared
5. **Tab navigation** — create multiple `Focus::new(Text::new(...))` children in a Column, focus the first, send Tab key, verify focus moves to the second
6. **Focus-dependent build** — create a `Focus::new(TextEdit::new(...))`, focus it, trigger rebuild, verify `BuildContext::is_focused()` returns true
7. **Autofocus** — create `Focus::new(Text::new(...)).autofocus(true)`, reconcile, verify focus is set after mount

- [ ] **Step 2: Add test module to focus/mod.rs**

```rust
#[cfg(test)]
mod integration_tests;
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -p vexo 2>&1 | tail -30`
Expected: ALL TESTS PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/focus/
git commit -m "test: add focus system integration tests"
```

---

## Task 9: Update Demo App and Final Verification

**Files:**
- Modify: `shared_app/src/lib.rs` — Wrap TextEdit with Focus in the demo app
- Modify: `vexo/src/retain/mod.rs` — Ensure all focus types are exported

- [ ] **Step 1: Update demo app to use Focus wrapper**

In the shared app's `view()` method, wrap any TextEdit widgets with `Focus::new(text_edit)` so they participate in the focus tree.

- [ ] **Step 2: Build desktop demo**

Run: `cargo build -p desktop_demo 2>&1 | head -20`
Expected: BUILD SUCCEEDS

- [ ] **Step 3: Run full test suite**

Run: `cargo test 2>&1 | tail -30`
Expected: ALL TESTS PASS

- [ ] **Step 4: Commit**

```bash
git add shared_app/src/lib.rs vexo/src/retain/
git commit -m "feat: update demo app to use Focus wrapper for TextEdit"
```

---

## Self-Review Checklist

- [ ] **Spec coverage:** Each section of the design spec maps to a task:
  - Section 1 (Core Data Model) → Tasks 1-3
  - Section 2 (Lifecycle) → Task 5
  - Section 3 (Focus Requests & Scope) → Tasks 3, 6
  - Section 4 (Traversal) → Tasks 2, 3
  - Section 5 (Callbacks & Keyboard Token) → Tasks 3, 4
  - Section 6 (Pipeline Integration) → Tasks 6, 7
  - Section 7 (Testing) → Tasks 4, 8
  - Section 8 (Module Structure) → Tasks 1-5
- [ ] **Placeholder scan:** No TBDs, TODOs, or "implement later" in code steps
- [ ] **Type consistency:** FocusNodeKey, FocusNodeData, FocusScopeData, FocusManager, TraversalPolicy, UnfocusDisposition used consistently across all tasks
