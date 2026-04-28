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
    /// Create a new element context.
    pub fn new(
        parent: Option<ElementId>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
    ) -> Self {
        Self {
            parent,
            render_object: None,
            state,
            dirty,
            render_objects: None,
            element_registry: None,
        }
    }

    /// Create a new element context with render object registry.
    pub fn new_with_registry(
        parent: Option<ElementId>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
        render_objects: &'a mut RenderObjectRegistry,
    ) -> Self {
        Self {
            parent,
            render_object: None,
            state,
            dirty,
            render_objects: Some(render_objects),
            element_registry: None,
        }
    }

    /// Create a new element context with all registries.
    pub fn new_full(
        parent: Option<ElementId>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
        render_objects: &'a mut RenderObjectRegistry,
        element_registry: &'a mut ElementRegistry,
    ) -> Self {
        Self {
            parent,
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
}