//! Element trait and registry.
//!
//! Elements are the middle tree in the three-tree architecture.
//! They bridge Widget (configuration) and RenderObject (layout/paint).

use slotmap::SlotMap;
use std::any::Any;

use super::element_context::ElementContext;
use super::focus::attachment::FocusAttachment;
use super::id::ElementKey;
use super::key::WidgetKey;
use crate::gestures::{ArenaEvent, GestureArena, GestureRecognizer};
/// Persistent element with state and lifecycle.
///
/// Elements represent the "live" state of the UI tree. They:
/// - Have lifecycle methods (mount, update, unmount)
/// - Hold state (via StateStorage)
/// - Track parent/child relationships
/// - Connect to RenderObjects
pub trait Element {
    /// Called when element is added to the tree.
    fn mount(&mut self, context: &mut ElementContext);

    /// Called when widget configuration changes.
    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext);

    /// Called when element is removed from the tree.
    fn unmount(&mut self, context: &mut ElementContext);

    /// Get associated render object (if any).
    fn render_object(&self) -> Option<super::id::RenderObjectKey>;

    /// Get the widget key (local or global).
    fn widget_key(&self) -> Option<WidgetKey>;

    /// Check if this element can be updated with the given widget (reused in
    /// place) instead of being unmounted and remounted.
    ///
    /// This is the **authoritative** reconciliation check — the reconciler
    /// calls this (not any `Widget`-side method) on every `ChildOp::Update`
    /// and at the root. Implementations decide their own policy:
    ///
    /// - Stateless pass-through elements (`ClipRRect`, `Opacity`, `Positioned`,
    ///   `Offstage`, `Container`, `Leaf`, `DecoratedBox`, `Focus`, `ScrollView`)
    ///   check **type only**. They have no state derived from widget fields, so
    ///   a remount on key change would be wasted work; `rebuild()` fully syncs
    ///   props and the child subtree. Note: this means widget keys are
    ///   **ineffective in single-child slots** — keys only drive sibling
    ///   reconciliation in multi-child containers. Production code relies on
    ///   this (see `shared_app/src/chats/chat_screen.rs` and
    ///   `docs/rebuild-skipping-patterns.md`).
    /// - `StatefulElement` checks **type AND key**, because its `State` is
    ///   often derived from widget fields in `on_mount`; without a remount on
    ///   key change, that derivation never re-runs and `State` goes stale.
    ///
    /// Returns `false` to force `replace_element` (unmount old + mount new).
    fn can_update(&self, widget: &dyn Any) -> bool;

    /// Handle an input event.
    fn on_event(
        &mut self,
        _event: &crate::input::InputEvent,
        _context: &mut super::EventContext,
        _state: &mut super::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        None
    }

    /// Rebuild this element with a new widget.
    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update(new_widget, context);
    }

    /// Called by the reconciler after a ChildOp::Inflate is executed,
    /// to link the child's render object into the parent's render object tree.
    ///
    /// Elements that own render objects and have children should override this
    /// to connect the child's render object to their own.
    ///
    /// The `child_ro` parameter is the child's render object key (if any).
    /// The `slot` parameter indicates the position for multi-child elements.
    fn child_mounted(
        &mut self,
        _slot: Option<usize>,
        _child_ro: Option<super::id::RenderObjectKey>,
        _context: &mut ElementContext,
    ) {
    }

    /// Rebuild this element from its current state (without a new widget).
    fn rebuild_from_state(&mut self, _context: &mut ElementContext) {}

    /// Advance animations before rebuild.
    ///
    /// Called by the reconciler before `rebuild_from_state` on each frame.
    /// StatefulElement overrides this to call `State::animate(now)`, giving
    /// the state a chance to advance any AnimationControllers.
    fn animate(&mut self, _now: std::time::Instant, _context: &mut ElementContext) {}

    /// Get the focus attachment for this element.
    fn focus_attachment(&self) -> &Option<FocusAttachment>;

    /// Get mutable access to the focus attachment for this element.
    fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment>;

    /// Register gesture recognizers into the arena for this pointer press.
    ///
    /// Called once on pointer press for every element in the hit-test path
    /// (deepest first). Default: no-op. Override to add recognizers.
    fn register_gestures(&mut self, _arena: &mut GestureArena, _self_id: ElementKey) {}

    /// Called on each subsequent Move/Up event **only for the winning element**.
    ///
    /// The element downcasts the recognizer to read its state and apply
    /// effects (e.g. ScrollView reads the drag recognizer's position delta).
    /// Default: no-op.
    fn on_arena_winner_update(
        &mut self,
        _recognizer: &dyn GestureRecognizer,
        _event: &ArenaEvent,
        _ctx: &mut super::EventContext,
    ) {
    }
}

/// Per-element metadata stored alongside the element itself.
///
/// Co-locating `element`, `parent`, and `children` in one entry keeps
/// identity and topology in sync structurally: `unmount` removes a single
/// slot and all three fields die atomically, so there is no parallel set
/// of SecondaryMaps that can drift out of sync.
struct ElementEntry {
    element: Box<dyn Element>,
    parent: Option<ElementKey>,
    children: Vec<ElementKey>,
}

/// Central registry for all live elements using generational keys.
///
/// Elements are stored in a SlotMap with generational keys for ABA protection.
/// Parent-child relationships are co-located with each element in `ElementEntry`.
pub struct ElementRegistry {
    slots: SlotMap<ElementKey, ElementEntry>,
    root: Option<ElementKey>,
}

impl ElementRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            slots: SlotMap::with_key(),
            root: None,
        }
    }

    /// Insert an element into the registry and set up parent metadata.
    /// Does NOT call element.mount() — the pipeline handles lifecycle.
    /// Does NOT add to parent's children list — the pipeline calls add_child() separately.
    pub fn insert(&mut self, element: Box<dyn Element>, parent: Option<ElementKey>) -> ElementKey {
        let key = self.slots.insert(ElementEntry {
            element,
            parent,
            children: Vec::new(),
        });
        if parent.is_none() {
            self.root = Some(key);
        }
        key
    }

    /// Add a child to a parent's children list at the given slot position.
    /// Called by the reconciler after executing a ChildOp::Inflate.
    pub fn add_child(&mut self, parent: ElementKey, child: ElementKey, slot: Option<usize>) {
        let entry = match self.slots.get_mut(parent) {
            Some(e) => e,
            None => return,
        };
        let children = &mut entry.children;
        if let Some(idx) = slot {
            if idx >= children.len() {
                children.resize(idx + 1, child);
            } else {
                children[idx] = child;
            }
        } else {
            children.push(child);
        }
    }

    /// Replace the child at the given slot with a new child key.
    ///
    /// Called by the reconciler during `replace_element` to swap an old element
    /// key for a new one at the same position, BEFORE unmounting the old element.
    /// This avoids the slot-shift corruption that would occur if the old element
    /// were unmounted first (which removes it from the children list, shifting
    /// subsequent elements left and invalidating the slot index).
    ///
    /// Also sets the new child's parent.
    pub fn replace_child_at(&mut self, parent: ElementKey, slot: usize, new_child: ElementKey) {
        if let Some(entry) = self.slots.get_mut(parent) {
            let siblings = &mut entry.children;
            if slot < siblings.len() {
                siblings[slot] = new_child;
            } else {
                siblings.resize(slot + 1, new_child);
            }
        }
        if let Some(entry) = self.slots.get_mut(new_child) {
            entry.parent = Some(parent);
        }
    }

    /// Call a closure with mutable access to an element and an external context.
    ///
    /// This replaces the vacate/restore pattern. The element is accessed via
    /// SlotMap::get_mut(), and the context is a separate parameter — Rust can
    /// prove they're disjoint because they're different arguments.
    ///
    /// Returns None if the key is invalid or the slot is empty.
    pub fn with_element<C, R>(
        &mut self,
        key: ElementKey,
        context: &mut C,
        f: impl FnOnce(&mut Box<dyn Element>, &mut C) -> R,
    ) -> Option<R> {
        let entry = self.slots.get_mut(key)?;
        Some(f(&mut entry.element, context))
    }

    /// Unmount an element and all its descendants.
    ///
    /// Uses `SlotMap::remove()` which bumps the generation, invalidating
    /// any stale keys held elsewhere (ABA protection).
    pub fn unmount(&mut self, key: ElementKey) {
        // Recursively unmount children first. Clone the children list so we
        // can borrow `self` mutably for the recursive call without holding
        // a borrow on the slot.
        let children: Vec<ElementKey> = self
            .slots
            .get(key)
            .map(|e| e.children.clone())
            .unwrap_or_default();
        for child in children {
            self.unmount(child);
        }

        // Remove from parent's children list.
        let parent = self.slots.get(key).and_then(|e| e.parent);
        if let Some(parent) = parent {
            if let Some(entry) = self.slots.get_mut(parent) {
                entry.children.retain(|&s| s != key);
            }
        }

        // True removal — bumps generation, invalidates stale keys. Frees
        // element, parent, and children atomically.
        self.slots.remove(key);
    }

    /// Get an element by key.
    pub fn get(&self, key: ElementKey) -> Option<&dyn Element> {
        self.slots.get(key).map(|e| e.element.as_ref())
    }

    /// Get a mutable element by key.
    pub fn get_mut(&mut self, key: ElementKey) -> Option<&mut Box<dyn Element>> {
        self.slots.get_mut(key).map(|e| &mut e.element)
    }

    /// Check if an element exists in the registry.
    pub fn contains(&self, key: ElementKey) -> bool {
        self.slots.contains_key(key)
    }

    /// Get the parent of an element.
    pub fn parent(&self, key: ElementKey) -> Option<ElementKey> {
        self.slots.get(key).and_then(|e| e.parent)
    }

    /// Get the children of an element.
    pub fn children(&self, key: ElementKey) -> &[ElementKey] {
        self.slots
            .get(key)
            .map(|e| e.children.as_slice())
            .unwrap_or_default()
    }

    /// Set the children of an element.
    pub fn set_children(&mut self, key: ElementKey, children: Vec<ElementKey>) {
        if let Some(entry) = self.slots.get_mut(key) {
            entry.children = children;
        }
    }

    /// Get the root element key.
    pub fn root(&self) -> Option<ElementKey> {
        self.root
    }

    /// Set the root element key.
    pub fn set_root(&mut self, key: ElementKey) {
        self.root = Some(key);
    }

    /// Get the number of elements.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Compute the depth of an element in the tree.
    pub fn depth(&self, key: ElementKey) -> usize {
        let mut depth = 0;
        let mut current = key;
        while let Some(parent) = self.slots.get(current).and_then(|e| e.parent) {
            depth += 1;
            current = parent;
        }
        depth
    }
}

impl Default for ElementRegistry {
    fn default() -> Self {
        Self::new()
    }
}
