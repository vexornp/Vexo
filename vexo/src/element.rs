//! Element trait and registry.
//!
//! Elements are the middle tree in the three-tree architecture.
//! They bridge Widget (configuration) and RenderObject (layout/paint).

use std::any::Any;
use slotmap::{SlotMap, SecondaryMap};

use super::id::ElementKey;
use super::key::WidgetKey;
use super::element_context::ElementContext;
use super::focus::attachment::FocusAttachment;
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

    /// Check if this element can be updated with the given widget.
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
    fn rebuild(
        &mut self,
        new_widget: Box<dyn Any>,
        context: &mut ElementContext,
    ) {
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
    fn child_mounted(&mut self, _slot: Option<usize>, _child_ro: Option<super::id::RenderObjectKey>, _context: &mut ElementContext) {}

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
}

/// Central registry for all live elements using generational keys.
///
/// Elements are stored in a SlotMap with generational keys for ABA protection.
/// Parent-child relationships are tracked via SecondaryMaps.
pub struct ElementRegistry {
    slots: SlotMap<ElementKey, Box<dyn Element>>,
    parent_map: SecondaryMap<ElementKey, Option<ElementKey>>,
    children_map: SecondaryMap<ElementKey, Vec<ElementKey>>,
    root: Option<ElementKey>,
}

impl ElementRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            slots: SlotMap::with_key(),
            parent_map: SecondaryMap::new(),
            children_map: SecondaryMap::new(),
            root: None,
        }
    }

    /// Insert an element into the registry and set up parent metadata.
    /// Does NOT call element.mount() — the pipeline handles lifecycle.
    /// Does NOT add to parent's children list — the pipeline calls add_child() separately.
    pub fn insert(
        &mut self,
        element: Box<dyn Element>,
        parent: Option<ElementKey>,
    ) -> ElementKey {
        let key = self.slots.insert(element);
        self.parent_map.insert(key, parent);
        if parent.is_none() {
            self.root = Some(key);
        }
        key
    }

    /// Add a child to a parent's children list at the given slot position.
    /// Called by the reconciler after executing a ChildOp::Inflate.
    pub fn add_child(&mut self, parent: ElementKey, child: ElementKey, slot: Option<usize>) {
        let children = self.children_map.entry(parent).expect("entry for existing parent key").or_default();
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
        let element = self.slots.get_mut(key)?;
        Some(f(element, context))
    }

    /// Unmount an element and all its descendants.
    ///
    /// Uses `SlotMap::remove()` which bumps the generation, invalidating
    /// any stale keys held elsewhere (ABA protection).
    pub fn unmount(&mut self, key: ElementKey) {
        // Recursively unmount children first
        let children: Vec<ElementKey> = self.children_map.get(key).cloned().unwrap_or_default();
        for child in children {
            self.unmount(child);
        }

        // Remove from parent's children list
        if let Some(Some(parent)) = self.parent_map.get(key) {
            if let Some(siblings) = self.children_map.get_mut(*parent) {
                siblings.retain(|&s| s != key);
            }
        }

        // True removal — bumps generation, invalidates stale keys
        self.slots.remove(key);
        self.parent_map.remove(key);
        self.children_map.remove(key);
    }

    /// Get an element by key.
    pub fn get(&self, key: ElementKey) -> Option<&dyn Element> {
        self.slots.get(key).map(|b| b.as_ref())
    }

    /// Get a mutable element by key.
    pub fn get_mut(&mut self, key: ElementKey) -> Option<&mut Box<dyn Element>> {
        self.slots.get_mut(key)
    }

    /// Check if an element exists in the registry.
    pub fn contains(&self, key: ElementKey) -> bool {
        self.slots.contains_key(key)
    }

    /// Get the parent of an element.
    pub fn parent(&self, key: ElementKey) -> Option<ElementKey> {
        self.parent_map.get(key).and_then(|p| *p)
    }

    /// Get the children of an element.
    pub fn children(&self, key: ElementKey) -> &[ElementKey] {
        self.children_map.get(key).map(|v| v.as_slice()).unwrap_or_default()
    }

    /// Set the children of an element.
    pub fn set_children(&mut self, key: ElementKey, children: Vec<ElementKey>) {
        self.children_map.insert(key, children);
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
        while let Some(Some(parent)) = self.parent_map.get(current) {
            depth += 1;
            current = *parent;
        }
        depth
    }
}

impl Default for ElementRegistry {
    fn default() -> Self {
        Self::new()
    }
}