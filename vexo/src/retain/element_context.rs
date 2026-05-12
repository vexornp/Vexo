//! Context passed to Elements during operations.

use std::sync::mpsc;

use super::id::{ElementId, RenderObjectId};
use super::state::StateStorage;
use super::dirty::DirtyTracking;
use super::render_object::{RenderObjectRegistry, RenderObject};
use super::element::ElementRegistry;
use super::build_owner::BuildOwner;

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

    /// Build owner for marking elements dirty and accessing global keys.
    /// Uses shared reference because BuildOwner uses interior mutability (RefCell)
    /// for both dirty tracking and global key registry.
    pub build_owner: Option<&'a BuildOwner>,

    /// Channel sender for dirty element signals from StatefulMutable callbacks.
    ///
    /// When a `StatefulMutable::set()` fires its dirty callback, it sends
    /// the element ID through this channel instead of directly calling
    /// `mark_needs_build()`. The pipeline drains the channel and calls
    /// `mark_needs_build()` itself, eliminating the need for raw pointers.
    pub dirty_sender: Option<&'a mpsc::Sender<ElementId>>,
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
            build_owner: None,
            dirty_sender: None,
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
            build_owner: None,
            dirty_sender: None,
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
        build_owner: &'a BuildOwner,
        dirty_sender: &'a mpsc::Sender<ElementId>,
    ) -> Self {
        Self {
            parent,
            element_id,
            render_object: None,
            state,
            dirty,
            render_objects: Some(render_objects),
            element_registry: Some(element_registry),
            build_owner: Some(build_owner),
            dirty_sender: Some(dirty_sender),
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

    /// Get a mutable reference to a render object by ID.
    ///
    /// Returns None if the ID is not valid or no registry is available.
    /// Used during Element::update() to update render object properties.
    pub fn get_render_object_mut(&mut self, id: RenderObjectId) -> Option<&mut Box<dyn RenderObject>> {
        self.render_objects.as_mut().and_then(|registry| registry.get_mut(id))
    }

    /// Get the build owner reference, if set.
    ///
    /// Returns a copy of the shared BuildOwner reference with the original
    /// lifetime `'a`, not tied to the borrow of `self`. This allows the
    /// caller to use the reference without holding an immutable borrow on
    /// the ElementContext, which is needed to avoid borrow conflicts when
    /// other fields are mutably borrowed.
    pub fn get_build_owner(&self) -> Option<&'a BuildOwner> {
        self.build_owner
    }

    /// Mark an element as needing rebuild.
    pub fn mark_needs_build(&mut self, element_id: ElementId) {
        if let Some(build_owner) = &self.build_owner {
            build_owner.mark_needs_build(element_id);
        }
    }

    /// Register a global key for this element.
    ///
    /// Called during mount() for elements with GlobalKey.
    /// Returns an error if the key is already registered to another element.
    pub fn register_global_key(&mut self, key: super::key::GlobalKey, element_id: ElementId) -> Result<(), super::global_key_registry::GlobalKeyError> {
        if let Some(build_owner) = &self.build_owner {
            build_owner.global_keys_mut().register(key, element_id)
        } else {
            Ok(())
        }
    }

    /// Unregister a global key for this element.
    ///
    /// Called during unmount() for elements with GlobalKey.
    pub fn unregister_global_key(&mut self, element_id: ElementId) {
        if let Some(build_owner) = &self.build_owner {
            build_owner.global_keys_mut().unregister_element(element_id);
        }
    }

    /// Inflate a widget into an element tree.
    ///
    /// Convenience method that delegates to ElementRegistry::inflate_widget().
    /// This recursively mounts all children and links render objects.
    ///
    /// Returns the ID of the inflated element, or None if registries are not available.
    pub fn inflate_widget(&mut self, widget: Box<dyn super::Widget>) -> Option<ElementId> {
        let element_registry = self.element_registry.take()?;
        let render_objects = self.render_objects.take()?;
        let build_owner = self.build_owner?;
        let dirty_sender = self.dirty_sender?;

        let id = element_registry.inflate_widget(
            widget,
            Some(self.element_id),
            self.state,
            self.dirty,
            render_objects,
            build_owner,
            dirty_sender,
        );

        self.element_registry = Some(element_registry);
        self.render_objects = Some(render_objects);
        self.build_owner = Some(build_owner);
        self.dirty_sender = Some(dirty_sender);
        Some(id)
    }

    /// Update or mount a child element.
    ///
    /// Convenience method that delegates to ElementRegistry::update_child().
    /// If the child exists and can update, it updates it; otherwise mounts a new tree.
    ///
    /// Returns the ID of the child element, or None if registries are not available.
    pub fn update_child(
        &mut self,
        child_id: Option<ElementId>,
        widget: Box<dyn super::Widget>,
    ) -> Option<ElementId> {
        let element_registry = self.element_registry.take()?;
        let render_objects = self.render_objects.take()?;
        let build_owner = self.build_owner?;
        let dirty_sender = self.dirty_sender?;

        let id = element_registry.update_child(
            child_id,
            widget,
            self.element_id,
            self.state,
            self.dirty,
            render_objects,
            build_owner,
            dirty_sender,
        );

        self.element_registry = Some(element_registry);
        self.render_objects = Some(render_objects);
        self.build_owner = Some(build_owner);
        self.dirty_sender = Some(dirty_sender);
        Some(id)
    }
}