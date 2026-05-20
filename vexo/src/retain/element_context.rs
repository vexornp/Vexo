//! Element context for lifecycle methods.
//!
//! Provides context during element mount, update, and unmount.

use std::sync::mpsc;

use crate::retain::build_owner::BuildOwner;
use crate::retain::child_ops::ChildOps;
use crate::retain::dirty::DirtyTracking;
use crate::retain::focus::FocusManager;
use crate::retain::id::{ElementKey, RenderObjectKey};
use crate::retain::key::GlobalKey;
use crate::retain::render_object::{RenderObject, RenderObjectRegistry};
use crate::retain::state::StateStorage;

/// Context passed to element lifecycle methods.
///
/// Elements use `child_ops` to request child tree operations instead of
/// directly accessing the ElementRegistry. The pipeline executes the
/// operations after the element method returns.
pub struct ElementContext<'a> {
    pub element_id: ElementKey,
    pub parent: Option<ElementKey>,
    pub children: Vec<ElementKey>,
    pub state: &'a mut StateStorage,
    pub dirty: &'a mut DirtyTracking,
    pub render_objects: &'a mut RenderObjectRegistry,
    pub build_owner: &'a BuildOwner,
    pub dirty_sender: &'a mpsc::Sender<ElementKey>,
    pub child_ops: &'a mut ChildOps,
    pub focus_manager: &'a mut FocusManager,
}

impl<'a> ElementContext<'a> {
    pub fn new(
        element_id: ElementKey,
        parent: Option<ElementKey>,
        children: Vec<ElementKey>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
        render_objects: &'a mut RenderObjectRegistry,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a mpsc::Sender<ElementKey>,
        child_ops: &'a mut ChildOps,
        focus_manager: &'a mut FocusManager,
    ) -> Self {
        Self {
            element_id,
            parent,
            children,
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
            focus_manager,
        }
    }

    /// Get the children of this element.
    ///
    /// Set by the reconciler before calling element lifecycle methods.
    /// Elements use this instead of storing children internally.
    pub fn children(&self) -> &[ElementKey] {
        &self.children
    }

    // -- Child operations (emit commands, pipeline executes later) --

    /// Request inflation of a new child element.
    pub fn inflate_child(&mut self, slot: Option<usize>, widget: Box<dyn crate::retain::widgets::Widget>) {
        self.child_ops.inflate(slot, widget, self.element_id);
    }

    /// Request update of an existing child element.
    pub fn update_child(&mut self, child: ElementKey, widget: Box<dyn crate::retain::widgets::Widget>) {
        self.child_ops.update(child, widget);
    }

    /// Request unmount of a child element.
    pub fn unmount_child(&mut self, child: ElementKey) {
        self.child_ops.unmount(child);
    }

    // -- Dirty tracking --

    /// Mark a render object as needing layout.
    pub fn mark_needs_layout(&mut self, key: RenderObjectKey) {
        self.dirty.mark_needs_layout(key);
    }

    /// Mark a render object as needing paint.
    pub fn mark_needs_paint(&mut self, key: RenderObjectKey) {
        self.dirty.mark_needs_paint(key);
    }

    /// Mark this element as needing rebuild.
    pub fn mark_dirty(&mut self) {
        let _ = self.dirty_sender.send(self.element_id);
    }

    /// Mark an element as needing build via the BuildOwner.
    pub fn mark_needs_build(&mut self, element_id: ElementKey) {
        self.build_owner.mark_needs_build(element_id);
    }

    // -- State storage --

    /// Get state for this element.
    pub fn get_state<T: 'static>(&self, id: ElementKey) -> Option<&T> {
        self.state.get::<T>(id)
    }

    /// Get mutable state for this element.
    pub fn get_state_mut<T: 'static>(&mut self, id: ElementKey) -> Option<&mut T> {
        self.state.get_mut::<T>(id)
    }

    /// Insert state for this element.
    pub fn insert_state<T: 'static>(&mut self, id: ElementKey, state: T) {
        self.state.insert(id, state);
    }

    /// Remove state for this element.
    pub fn remove_state(&mut self, id: ElementKey) {
        self.state.remove(id);
    }

    /// Get or create state for this element.
    pub fn get_or_create_state<S: 'static + Clone + Send>(&mut self, initial: S) -> S {
        if let Some(existing) = self.state.get::<S>(self.element_id) {
            existing.clone()
        } else {
            self.state.insert(self.element_id, initial.clone());
            initial
        }
    }

    // -- Render object registry --

    /// Create a render object in the registry.
    pub fn create_render_object(&mut self, object: Box<dyn RenderObject>, owner: ElementKey) -> Option<RenderObjectKey> {
        Some(self.render_objects.create(object, owner))
    }

    /// Remove a render object from the registry.
    pub fn remove_render_object(&mut self, key: RenderObjectKey) {
        self.render_objects.remove(key);
    }

    /// Get a mutable reference to a render object by key.
    pub fn get_render_object_mut(&mut self, key: RenderObjectKey) -> Option<&mut Box<dyn RenderObject>> {
        self.render_objects.get_mut(key)
    }

    /// Set the child render object on a parent render object.
    pub fn set_render_object_child(&mut self, parent: RenderObjectKey, child: RenderObjectKey) {
        self.render_objects.set_child(parent, child);
    }

    // -- Global key registry --

    /// Register a global key for this element.
    pub fn register_global_key(&mut self, key: GlobalKey, element_id: ElementKey) -> Result<(), crate::retain::global_key_registry::GlobalKeyError> {
        self.build_owner.global_keys_mut().register(key, element_id)
    }

    /// Unregister a global key for this element.
    pub fn unregister_global_key(&mut self, element_id: ElementKey) {
        self.build_owner.global_keys_mut().unregister_element(element_id);
    }

    // -- Focus management --

    /// Get a mutable reference to the focus manager.
    pub fn focus_manager(&mut self) -> &mut FocusManager {
        self.focus_manager
    }

    /// Walk up the element tree to find the nearest parent FocusNodeKey.
    ///
    /// Each FocusElement/FocusScopeElement stores its FocusNodeKey in
    /// StateStorage keyed by its ElementKey. This method walks up via
    /// `self.parent` and checks each ancestor for a stored FocusNodeKey.
    pub fn parent_focus_node(&self) -> Option<super::focus::FocusNodeKey> {
        let current = self.parent;
        while let Some(ancestor_key) = current {
            // Check if this ancestor has a stored FocusNodeKey.
            if let Some(node_key) = self.state.get::<super::focus::FocusNodeKey>(ancestor_key) {
                return Some(*node_key);
            }
            // We can't walk further without the element registry.
            // For now, stop at the immediate parent. The Reconciler will
            // provide the correct parent chain when constructing the context.
            break;
        }
        None
    }
}
