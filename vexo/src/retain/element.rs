//! Element trait and registry.
//!
//! Elements are the middle tree in the three-tree architecture.
//! They bridge Widget (configuration) and RenderObject (layout/paint).

use std::any::Any;
use std::collections::HashMap;

use super::id::{ElementId, RenderObjectId};
use super::key::WidgetKey;
use super::element_context::ElementContext;

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
    ///
    /// The `new_widget` parameter contains the updated widget configuration.
    /// Note: The widget is type-erased as `Box<dyn Any>` to allow the Element trait
    /// to be object-safe while still supporting generic `Widget<M>` implementations.
    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext);

    /// Called when element is removed from the tree.
    fn unmount(&mut self, context: &mut ElementContext);

    /// Visit children for traversal.
    ///
    /// The registry parameter provides access to look up child elements by ID.
    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element));

    /// Get associated render object (if any).
    fn render_object(&self) -> Option<RenderObjectId>;

    /// Get the widget key (local or global).
    fn widget_key(&self) -> Option<WidgetKey>;

    /// Check if this element can be updated with the given widget.
    fn can_update(&self, widget: &dyn Any) -> bool;

    /// Handle an input event.
    ///
    /// Returns `Some(message)` if the event was handled and produces a message.
    /// The message is type-erased as `Box<dyn Any>` and will be downcast
    /// by `WindowState` to the application's message type.
    ///
    /// Default implementation returns `None` (no interaction).
    fn on_event(
        &mut self,
        _event: &crate::input::InputEvent,
        _context: &mut super::EventContext,
    ) -> Option<Box<dyn Any>> {
        None
    }

    /// Add a child element ID.
    ///
    /// Called by the pipeline during mount to link children.
    /// Default implementation does nothing (for leaf elements).
    fn add_child(&mut self, _child_id: ElementId) {
        // Default: no-op for leaf elements
    }

    /// Rebuild this element with a new widget.
    ///
    /// Called by BuildOwner during perform_rebuilds(). The element should:
    /// 1. Update its widget configuration
    /// 2. Reconcile its children (if any)
    /// 3. Mark render objects dirty
    ///
    /// This is the per-element equivalent of the pipeline's reconcile.
    /// Container and modifier elements override this to reconcile children.
    /// Leaf elements use the default (no children to reconcile).
    fn rebuild(
        &mut self,
        new_widget: Box<dyn Any>,
        context: &mut ElementContext,
    ) {
        // Default: just update, no children to reconcile
        self.update(new_widget, context);
    }

    /// Check if this element has children that need reconciliation.
    ///
    /// Returns true for containers and modifiers, false for leaves.
    fn has_children(&self) -> bool {
        false
    }
}

/// Central registry for all live elements.
///
/// Manages elements and their tree structure (parent/child relationships).
pub struct ElementRegistry {
    elements: HashMap<ElementId, Box<dyn Element>>,
    parent_map: HashMap<ElementId, Option<ElementId>>,
    children_map: HashMap<ElementId, Vec<ElementId>>,
    root: Option<ElementId>,
}

impl ElementRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            parent_map: HashMap::new(),
            children_map: HashMap::new(),
            root: None,
        }
    }

    /// Mount a new element.
    ///
    /// Returns the ID of the newly created element.
    pub fn mount(&mut self, element: Box<dyn Element>, parent: Option<ElementId>) -> ElementId {
        let id = ElementId::new();

        self.elements.insert(id, element);
        self.parent_map.insert(id, parent);

        if let Some(p) = parent {
            self.children_map.entry(p).or_default().push(id);
        } else {
            self.root = Some(id);
        }

        id
    }

    /// Mount a new element with a pre-allocated ID.
    ///
    /// This is used when the element ID needs to be known before mount()
    /// is called (e.g., for creating render objects during mount).
    pub fn mount_with_id(&mut self, element: Box<dyn Element>, parent: Option<ElementId>, id: ElementId) {
        self.elements.insert(id, element);
        self.parent_map.insert(id, parent);

        if let Some(p) = parent {
            self.children_map.entry(p).or_default().push(id);
        } else {
            self.root = Some(id);
        }
    }

    /// Unmount an element and all its descendants.
    pub fn unmount(&mut self, id: ElementId) {
        // Recursively unmount children first
        let children: Vec<ElementId> = self.children_map.get(&id).cloned().unwrap_or_default();
        for child in children {
            self.unmount(child);
        }

        // Remove from parent's children list
        if let Some(Some(parent)) = self.parent_map.get(&id) {
            if let Some(siblings) = self.children_map.get_mut(parent) {
                siblings.retain(|&s| s != id);
            }
        }

        // Remove the element
        self.elements.remove(&id);
        self.parent_map.remove(&id);
        self.children_map.remove(&id);
    }

    /// Get an element by ID.
    pub fn get(&self, id: ElementId) -> Option<&dyn Element> {
        self.elements.get(&id).map(|b| b.as_ref())
    }

    /// Get a mutable element by ID.
    pub fn get_mut(&mut self, id: ElementId) -> Option<&mut (dyn Element + '_)> {
        let boxed = self.elements.get_mut(&id)?;
        Some(boxed.as_mut())
    }

    /// Check if an element exists.
    pub fn contains(&self, id: ElementId) -> bool {
        self.elements.contains_key(&id)
    }

    /// Get the parent of an element.
    pub fn parent(&self, id: ElementId) -> Option<ElementId> {
        self.parent_map.get(&id).and_then(|p| *p)
    }

    /// Get the children of an element.
    pub fn children(&self, id: ElementId) -> &[ElementId] {
        self.children_map.get(&id).map(|v| v.as_slice()).unwrap_or_default()
    }

    /// Set the children of an element.
    pub fn set_children(&mut self, id: ElementId, children: Vec<ElementId>) {
        self.children_map.insert(id, children);
    }

    /// Get the root element ID.
    pub fn root(&self) -> Option<ElementId> {
        self.root
    }

    /// Set the root element ID.
    pub fn set_root(&mut self, id: ElementId) {
        self.root = Some(id);
    }

    /// Get the number of elements.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Update an element with a new widget.
    ///
    /// Returns true if the element was found and updated, false otherwise.
    pub fn update_element(&mut self, id: ElementId, widget: Box<dyn Any>, context: &mut ElementContext) -> bool {
        if let Some(element) = self.elements.get_mut(&id) {
            element.update(widget, context);
            return true;
        }
        false
    }

    /// Mount a new element from an element box with full lifecycle.
    ///
    /// This is the canonical way to mount an element. It encapsulates the entire
    /// mount pattern:
    /// 1. Generate a new ElementId (single source of truth)
    /// 2. Create the ElementContext with the generated ID
    /// 3. Call mount() on the element
    /// 4. Register the element in the registry
    ///
    /// This ensures the mount pattern is always followed correctly.
    ///
    /// # Arguments
    ///
    /// * `element` - The element to mount (already created from a widget)
    /// * `parent` - The parent element ID (None for root)
    /// * `state` - State storage for elements
    /// * `dirty` - Dirty tracking for layout/paint
    /// * `render_objects` - Render object registry
    ///
    /// # Returns
    ///
    /// The ID of the newly mounted element.
    pub fn mount_element(
        &mut self,
        element: Box<dyn Element>,
        parent: Option<ElementId>,
        state: &mut super::state::StateStorage,
        dirty: &mut super::dirty::DirtyTracking,
        render_objects: &mut super::render_object::RenderObjectRegistry,
    ) -> ElementId {
        self.mount_element_with_global_keys(
            element,
            parent,
            state,
            dirty,
            render_objects,
            None,
        )
    }

    /// Mount a new element from an element box with full lifecycle and global keys.
    ///
    /// This is the canonical way to mount an element. It encapsulates the entire
    /// mount pattern:
    /// 1. Generate a new ElementId (single source of truth)
    /// 2. Create the ElementContext with the generated ID
    /// 3. Call mount() on the element
    /// 4. Register the element in the registry
    ///
    /// This ensures the mount pattern is always followed correctly.
    ///
    /// # Arguments
    ///
    /// * `element` - The element to mount (already created from a widget)
    /// * `parent` - The parent element ID (None for root)
    /// * `state` - State storage for elements
    /// * `dirty` - Dirty tracking for layout/paint
    /// * `render_objects` - Render object registry
    /// * `global_keys` - Optional global key registry for GlobalKey registration
    ///
    /// # Returns
    ///
    /// The ID of the newly mounted element.
    pub fn mount_element_with_global_keys(
        &mut self,
        mut element: Box<dyn Element>,
        parent: Option<ElementId>,
        state: &mut super::state::StateStorage,
        dirty: &mut super::dirty::DirtyTracking,
        render_objects: &mut super::render_object::RenderObjectRegistry,
        global_keys: Option<&mut super::global_key_registry::GlobalKeyRegistry>,
    ) -> ElementId {
        // 1. Generate element ID - single source of truth
        let element_id = ElementId::new();

        // 2. Create context with the element ID
        let mut ctx = ElementContext::full(
            element_id,
            parent,
            state,
            dirty,
            render_objects,
            self,
        );
        ctx.global_key_registry = global_keys;

        // 3. Call mount lifecycle
        element.mount(&mut ctx);

        // 4. Register element with the same ID
        self.mount_with_id(element, parent, element_id);

        element_id
    }
}

impl Default for ElementRegistry {
    fn default() -> Self {
        Self::new()
    }
}