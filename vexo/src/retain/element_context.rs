//! Context passed to Elements during operations.

use super::id::{ElementId, RenderObjectId};
use super::state::StateStorage;
use super::dirty::DirtyTracking;
use super::render_object::{RenderObjectRegistry, RenderObject};
use super::element::ElementRegistry;

/// Context provided to element lifecycle methods.
pub struct ElementContext<'a> {
    /// The parent element (None for root).
    pub parent: Option<ElementId>,

    /// The element ID for this element.
    /// This is the single source of truth - always provided by the pipeline.
    pub element_id: ElementId,

    /// The render object created for this element (set during mount).
    pub render_object: Option<RenderObjectId>,

    /// State storage for this element.
    pub state: &'a mut StateStorage,

    /// Dirty tracking for layout/paint.
    pub dirty: &'a mut DirtyTracking,

    /// Render object registry.
    pub render_objects: Option<&'a mut RenderObjectRegistry>,

    /// Element registry for mounting child elements.
    pub element_registry: Option<&'a mut ElementRegistry>,
}

impl<'a> ElementContext<'a> {
    /// Create a new element context with a specific element ID.
    ///
    /// This is the primary constructor - the element ID must be provided.
    /// The pipeline generates the ID and passes it here.
    pub fn new(
        element_id: ElementId,
        parent: Option<ElementId>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
    ) -> Self {
        Self {
            parent,
            element_id,
            render_object: None,
            state,
            dirty,
            render_objects: None,
            element_registry: None,
        }
    }

    /// Create a new element context with render object registry.
    pub fn with_registry(
        element_id: ElementId,
        parent: Option<ElementId>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
        render_objects: &'a mut RenderObjectRegistry,
    ) -> Self {
        Self {
            parent,
            element_id,
            render_object: None,
            state,
            dirty,
            render_objects: Some(render_objects),
            element_registry: None,
        }
    }

    /// Create a new element context with all registries.
    pub fn full(
        element_id: ElementId,
        parent: Option<ElementId>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
        render_objects: &'a mut RenderObjectRegistry,
        element_registry: &'a mut ElementRegistry,
    ) -> Self {
        Self {
            parent,
            element_id,
            render_object: None,
            state,
            dirty,
            render_objects: Some(render_objects),
            element_registry: Some(element_registry),
        }
    }

    /// Mark a render object as needing layout.
    pub fn mark_needs_layout(&mut self, id: RenderObjectId) {
        self.dirty.mark_needs_layout(id);
    }

    /// Mark a render object as needing paint.
    pub fn mark_needs_paint(&mut self, id: RenderObjectId) {
        self.dirty.mark_needs_paint(id);
    }

    /// Get state for this element.
    pub fn get_state<T: 'static>(&self, id: ElementId) -> Option<&T> {
        self.state.get::<T>(id)
    }

    /// Get mutable state for this element.
    pub fn get_state_mut<T: 'static>(&mut self, id: ElementId) -> Option<&mut T> {
        self.state.get_mut::<T>(id)
    }

    /// Insert state for this element.
    pub fn insert_state<T: 'static>(&mut self, id: ElementId, state: T) {
        self.state.insert(id, state);
    }

    /// Remove state for this element.
    pub fn remove_state(&mut self, id: ElementId) {
        self.state.remove(id);
    }

    /// Create a render object in the registry.
    ///
    /// Returns the ID of the created render object, or None if no registry is available.
    pub fn create_render_object(&mut self, object: Box<dyn RenderObject>, owner: ElementId) -> Option<RenderObjectId> {
        self.render_objects.as_mut().map(|registry| registry.create(object, owner))
    }

    /// Remove a render object from the registry.
    pub fn remove_render_object(&mut self, id: RenderObjectId) {
        if let Some(registry) = &mut self.render_objects {
            registry.remove(id);
        }
    }

    /// Mount a child element in the registry.
    ///
    /// Returns the ID of the mounted element, or None if no registry is available.
    pub fn mount_child_element(&mut self, element: Box<dyn super::Element>, parent: ElementId) -> Option<ElementId> {
        self.element_registry.as_mut().map(|registry| registry.mount(element, Some(parent)))
    }

    /// Unmount a child element from the registry.
    ///
    /// Returns true if the element was unmounted, false if no registry or element found.
    pub fn unmount_child_element(&mut self, child_id: ElementId) -> bool {
        if let Some(registry) = self.element_registry.as_mut() {
            if registry.contains(child_id) {
                registry.unmount(child_id);
                return true;
            }
        }
        false
    }

    /// Set the child render object on a parent render object.
    ///
    /// Used by modifier elements to link their render object to their child's.
    pub fn set_render_object_child(&mut self, parent: RenderObjectId, child: RenderObjectId) {
        if let Some(registry) = &mut self.render_objects {
            registry.set_child(parent, child);
        }
    }
}