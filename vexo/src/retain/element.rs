//! Element trait and registry.
//!
//! Elements are the middle tree in the three-tree architecture.
//! They bridge Widget (configuration) and RenderObject (layout/paint).

use std::any::Any;
use std::collections::HashMap;

use super::id::{ElementId, RenderObjectId};
use super::key::WidgetKey;
use super::element_context::ElementContext;
use super::global_key_registry::GlobalKeyRegistry;
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

    /// Check if this element manages its own children internally.
    ///
    /// Returns true for elements like StatefulElement that handle their own
    /// child reconciliation (e.g., through build()). The pipeline should
    /// NOT reconcile children for such elements - they do it themselves.
    ///
    /// Returns false for elements whose children are managed by the pipeline
    /// (ContainerElement, LeafElement, etc.).
    fn manages_own_children(&self) -> bool {
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

    /// Inflate a widget into an element tree.
    ///
    /// This is the Flutter-style inflateWidget() equivalent.
    /// Creates an element from the widget, mounts it, and recursively
    /// mounts all children, linking render objects.
    ///
    /// This is the canonical way to create a full element tree from a widget.
    /// Use this instead of `mount_element()` when you need the entire tree
    /// (including children) to be mounted.
    ///
    /// # Arguments
    ///
    /// * `widget` - The widget to inflate into an element tree
    /// * `parent` - The parent element ID (None for root)
    /// * `state` - State storage for elements
    /// * `dirty` - Dirty tracking for layout/paint
    /// * `render_objects` - Render object registry
    /// * `global_keys` - Optional global key registry for GlobalKey registration
    ///
    /// # Returns
    ///
    /// The ID of the root element of the inflated tree.
    pub fn inflate_widget(
        &mut self,
        widget: Box<dyn Widget>,
        parent: Option<ElementId>,
        state: &mut super::state::StateStorage,
        dirty: &mut super::dirty::DirtyTracking,
        render_objects: &mut super::render_object::RenderObjectRegistry,
        global_keys: Option<&mut GlobalKeyRegistry>,
    ) -> ElementId {
        // 1. Create element from widget
        let element = widget.create_element();

        // 2. Mount the element (calls mount() lifecycle)
        let element_id = self.mount_element_with_global_keys(
            element,
            parent,
            state,
            dirty,
            render_objects,
            global_keys,
        );

        // 3. Get the render object for linking
        let render_object_id = self.get(element_id)
            .and_then(|el| el.render_object());

        // 4. Set as root if no parent
        if parent.is_none() {
            if let Some(ro_id) = render_object_id {
                render_objects.set_root(ro_id);
            }
        }

        // 5. Check if this element manages its own children
        // If so, skip the recursive child mounting - the element handles it internally
        let manages_own = self.get(element_id)
            .map(|el| el.manages_own_children())
            .unwrap_or(false);

        if manages_own {
            log::debug!(
                "[RetainMode] inflate_widget - element {:?} manages own children, skipping recursive mount",
                element_id
            );
            return element_id;
        }

        // 6. Mount children recursively (single-child case - modifiers like Background, Border)
        if let Some(child_widget) = widget.child() {
            let child_id = self.inflate_widget(
                child_widget.clone_boxed(),
                Some(element_id),
                state,
                dirty,
                render_objects,
                None, // global_keys only needed for root
            );

            // Link child render object to parent
            if let (Some(parent_ro), Some(child_ro)) = (
                render_object_id,
                self.get(child_id).and_then(|el| el.render_object()),
            ) {
                if let Some(parent_obj) = render_objects.get_mut(parent_ro) {
                    parent_obj.set_child_id(child_ro);
                }
            }

            // Update element's children list
            if let Some(elem) = self.get_mut(element_id) {
                elem.add_child(child_id);
            }
        }

        // 6. Mount children recursively (multi-child case - containers like Column, Row)
        let children: Vec<Box<dyn Widget>> = widget.children()
            .iter()
            .map(|c| c.clone_boxed())
            .collect();

        if !children.is_empty() {
            let mut child_render_objects = Vec::new();
            let mut child_element_ids = Vec::new();

            for child_widget in children {
                let child_id = self.inflate_widget(
                    child_widget,
                    Some(element_id),
                    state,
                    dirty,
                    render_objects,
                    None,
                );
                child_element_ids.push(child_id);

                if let Some(child_ro) = self.get(child_id).and_then(|el| el.render_object()) {
                    child_render_objects.push(child_ro);
                }
            }

            // Link child render objects to parent container
            if let Some(parent_ro) = render_object_id {
                if let Some(parent_obj) = render_objects.get_mut(parent_ro) {
                    for child_ro in &child_render_objects {
                        parent_obj.add_child(*child_ro);
                    }
                }
            }

            // Update element's children list
            if let Some(elem) = self.get_mut(element_id) {
                for child_id in child_element_ids {
                    elem.add_child(child_id);
                }
            }
        }

        element_id
    }

    /// Update or mount a child element.
    ///
    /// This is the Flutter-style updateChild() equivalent.
    /// If child_id exists and can update with the new widget, calls update().
    /// Otherwise, inflates a new element tree.
    ///
    /// # Arguments
    ///
    /// * `child_id` - The existing child element ID (None to always mount new)
    /// * `new_widget` - The new widget for the child
    /// * `parent` - The parent element ID
    /// * `state` - State storage for elements
    /// * `dirty` - Dirty tracking for layout/paint
    /// * `render_objects` - Render object registry
    /// * `global_keys` - Optional global key registry
    ///
    /// # Returns
    ///
    /// The element ID of the updated or newly mounted child.
    pub fn update_child(
        &mut self,
        child_id: Option<ElementId>,
        new_widget: Box<dyn Widget>,
        parent: ElementId,
        state: &mut super::state::StateStorage,
        dirty: &mut super::dirty::DirtyTracking,
        render_objects: &mut super::render_object::RenderObjectRegistry,
        global_keys: Option<&mut GlobalKeyRegistry>,
    ) -> ElementId {
        // Check if we can update an existing child
        let can_update_existing = child_id
            .filter(|&id| self.contains(id))
            .map(|id| {
                self.get(id)
                    .map(|el| el.can_update(new_widget.as_any()))
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if can_update_existing {
            let id = child_id.unwrap();
            // Update existing element - use the existing update_element pattern
            // but we need to handle the context creation differently
            let widget_any: Box<dyn Any> = Box::new(new_widget.clone_boxed());

            // Create a minimal context for the update
            // We need to temporarily move the element out, update it, and put it back
            if let Some(mut element) = self.elements.remove(&id) {
                let mut ctx = ElementContext::full(
                    id,
                    Some(parent),
                    state,
                    dirty,
                    render_objects,
                    self,
                );
                ctx.global_key_registry = global_keys;

                element.update(widget_any, &mut ctx);

                // Put the element back
                self.elements.insert(id, element);
            }
            return id;
        }

        // Mount new element tree
        self.inflate_widget(new_widget, Some(parent), state, dirty, render_objects, global_keys)
    }
}

impl Default for ElementRegistry {
    fn default() -> Self {
        Self::new()
    }
}