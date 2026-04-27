//! Element trait and registry.
//!
//! Elements are the middle tree in the three-tree architecture.
//! They bridge Widget (configuration) and RenderObject (layout/paint).

use std::any::Any;
use std::collections::HashMap;

use super::id::{ElementId, RenderObjectId};
use super::key::Key;
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
    ///
    /// The `new_widget` parameter contains the updated widget configuration.
    fn update(&mut self, new_widget: Box<dyn Widget>, context: &mut ElementContext);

    /// Called when element is removed from the tree.
    fn unmount(&mut self, context: &mut ElementContext);

    /// Visit children for traversal.
    fn visit_children(&self, visitor: &mut dyn FnMut(&dyn Element));

    /// Get associated render object (if any).
    fn render_object(&self) -> Option<RenderObjectId>;

    /// Get the widget key.
    fn widget_key(&self) -> Option<Key>;

    /// Check if this element can be updated with the given widget.
    fn can_update(&self, widget: &dyn Any) -> bool;
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

    /// Get the number of elements.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

impl Default for ElementRegistry {
    fn default() -> Self {
        Self::new()
    }
}