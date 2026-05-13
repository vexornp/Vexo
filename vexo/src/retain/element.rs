//! Element trait and registry.
//!
//! Elements are the middle tree in the three-tree architecture.
//! They bridge Widget (configuration) and RenderObject (layout/paint).

use std::any::Any;
use std::sync::mpsc;

use slotmap::{SlotMap, SecondaryMap};

use super::id::ElementKey;
use super::key::WidgetKey;
use super::element_context::ElementContext;
use super::widgets::Widget;

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

    /// Visit children for traversal.
    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element));

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
    ) -> Option<Box<dyn Any>> {
        None
    }

    /// Add a child element key.
    fn add_child(&mut self, _child_key: ElementKey) {}

    /// Rebuild this element with a new widget.
    fn rebuild(
        &mut self,
        new_widget: Box<dyn Any>,
        context: &mut ElementContext,
    ) {
        self.update(new_widget, context);
    }

    /// Check if this element has children that need reconciliation.
    fn has_children(&self) -> bool {
        false
    }

    /// Update the given child with a new widget configuration.
    fn update_child(
        &mut self,
        child: Option<ElementKey>,
        new_widget: Option<Box<dyn Widget>>,
        _slot: Option<usize>,
        context: &mut ElementContext,
    ) -> Option<ElementKey> {
        match (child, new_widget) {
            (None, None) => None,
            (Some(child_key), None) => {
                if let Some(registry) = context.element_registry.as_mut() {
                    registry.unmount(child_key);
                }
                None
            }
            (None, Some(widget)) => {
                context.inflate_widget(widget)
            }
            (Some(child_key), Some(widget)) => {
                context.update_child(Some(child_key), widget)
            }
        }
    }

    /// Rebuild this element from its current state (without a new widget).
    fn rebuild_from_state(&mut self, _context: &mut ElementContext) {}
}

/// Central registry for all live elements using generational keys.
///
/// Uses `SlotMap<ElementKey, Option<Box<dyn Element>>>` so that the
/// `remove()/insert()` pattern (used during rebuilds to avoid borrow
/// conflicts) works by vacating a slot to `None` and restoring it to
/// `Some`. True unmount calls `SlotMap::remove()` which bumps the
/// generation, invalidating stale keys (ABA protection).
pub struct ElementRegistry {
    /// Primary element storage. `None` slots are temporarily vacated
    /// during rebuilds; `SlotMap::remove()` is used for true unmount.
    slots: SlotMap<ElementKey, Option<Box<dyn Element>>>,
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

    /// Mount a new element.
    ///
    /// Returns the key of the newly created element.
    pub fn mount(&mut self, element: Box<dyn Element>, parent: Option<ElementKey>) -> ElementKey {
        let key = self.slots.insert(Some(element));
        self.parent_map.insert(key, parent);

        if let Some(p) = parent {
            self.children_map.entry(p).expect("entry for existing parent key").or_insert_with(Vec::new).push(key);
        } else {
            self.root = Some(key);
        }

        key
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
        self.slots.get(key).and_then(|opt| opt.as_ref().map(|b| b.as_ref()))
    }

    /// Get a mutable element by key.
    pub fn get_mut(&mut self, key: ElementKey) -> Option<&mut Box<dyn Element>> {
        self.slots.get_mut(key).and_then(|opt| opt.as_mut())
    }

    /// Check if an element exists (slot is occupied, not vacated or removed).
    pub fn contains(&self, key: ElementKey) -> bool {
        self.slots.get(key).map_or(false, |opt| opt.is_some())
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

    /// Temporarily vacate an element slot (set to None).
    ///
    /// Used by `perform_rebuilds()` to avoid borrow conflicts while
    /// creating an `ElementContext` that needs `&mut ElementRegistry`.
    /// The key remains valid — call `restore()` to put the element back.
    pub fn vacate(&mut self, key: ElementKey) -> Option<Box<dyn Element>> {
        let slot = self.slots.get_mut(key)?;
        slot.take()
    }

    /// Restore an element to a previously vacated slot.
    ///
    /// Used after `vacate()` to put the element back.
    pub fn restore(&mut self, key: ElementKey, element: Box<dyn Element>) {
        if let Some(slot) = self.slots.get_mut(key) {
            *slot = Some(element);
        }
    }

    /// Update an element with a new widget.
    pub fn update_element(&mut self, key: ElementKey, widget: Box<dyn Any>, context: &mut ElementContext) -> bool {
        if let Some(element) = self.get_mut(key) {
            element.update(widget, context);
            return true;
        }
        false
    }

    /// Mount a new element with full lifecycle.
    pub fn mount_element(
        &mut self,
        element: Box<dyn Element>,
        parent: Option<ElementKey>,
        state: &mut super::state::StateStorage,
        dirty: &mut super::dirty::DirtyTracking,
        render_objects: &mut super::render_object::RenderObjectRegistry,
        build_owner: &super::build_owner::BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
    ) -> ElementKey {
        // 1. Insert element into slotmap (generates key)
        let element_key = self.slots.insert(Some(element));
        self.parent_map.insert(element_key, parent);

        if let Some(p) = parent {
            self.children_map.entry(p).expect("entry for existing parent key").or_insert_with(Vec::new).push(element_key);
        } else {
            self.root = Some(element_key);
        }

        // 2. Vacate the slot to get the element, avoiding double &mut borrow
        let mut element = self.slots.get_mut(element_key).unwrap().take().unwrap();

        // 3. Create context — self is not borrowed by ctx since we vacated first
        // We need a temporary registry proxy that doesn't actually borrow self
        // Use a simple approach: mount with just the element_key context
        // then restore the element
        {
            let mut ctx = ElementContext::full(
                element_key,
                parent,
                state,
                dirty,
                render_objects,
                self,
                build_owner,
                dirty_sender,
            );

            element.mount(&mut ctx);
        }

        // 4. Restore the element to its slot
        *self.slots.get_mut(element_key).unwrap() = Some(element);

        element_key
    }

    /// Inflate a widget into an element tree.
    pub fn inflate_widget(
        &mut self,
        widget: Box<dyn Widget>,
        parent: Option<ElementKey>,
        state: &mut super::state::StateStorage,
        dirty: &mut super::dirty::DirtyTracking,
        render_objects: &mut super::render_object::RenderObjectRegistry,
        build_owner: &super::build_owner::BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
    ) -> ElementKey {
        let element = widget.create_element();

        let element_key = self.mount_element(
            element,
            parent,
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
        );

        let render_object_id = self.get(element_key)
            .and_then(|el| el.render_object());

        if parent.is_none() {
            if let Some(ro_id) = render_object_id {
                render_objects.set_root(ro_id);
            }
        }

        element_key
    }

    /// Update or mount a child element.
    pub fn update_child(
        &mut self,
        child_key: Option<ElementKey>,
        new_widget: Box<dyn Widget>,
        parent: ElementKey,
        state: &mut super::state::StateStorage,
        dirty: &mut super::dirty::DirtyTracking,
        render_objects: &mut super::render_object::RenderObjectRegistry,
        build_owner: &super::build_owner::BuildOwner,
        dirty_sender: &mpsc::Sender<ElementKey>,
    ) -> ElementKey {
        let can_update_existing = child_key
            .filter(|&k| self.contains(k))
            .map(|k| {
                self.get(k)
                    .map(|el| el.can_update(new_widget.as_any()))
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if can_update_existing {
            let key = child_key.unwrap();
            let widget_any: Box<dyn Any> = Box::new(new_widget.clone_boxed());

            // Vacate the slot, rebuild, then restore
            if let Some(mut element) = self.vacate(key) {
                let mut ctx = ElementContext::full(
                    key,
                    Some(parent),
                    state,
                    dirty,
                    render_objects,
                    self,
                    build_owner,
                    dirty_sender,
                );

                element.rebuild(widget_any, &mut ctx);

                self.restore(key, element);
            }
            return key;
        }

        self.inflate_widget(new_widget, Some(parent), state, dirty, render_objects, build_owner, dirty_sender)
    }
}

impl Default for ElementRegistry {
    fn default() -> Self {
        Self::new()
    }
}