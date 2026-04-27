//! Container element implementation.
//!
//! ContainerElement is an element with children.
//! Used by container widgets like Column, Row, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementId, Key, RenderObjectId};

/// Element for container widgets (with children).
pub struct ContainerElement {
    id: Option<ElementId>,
    key: Option<Key>,
    render_object: Option<RenderObjectId>,
}

impl ContainerElement {
    /// Create a new container element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
        }
    }

    /// Create with a key.
    pub fn with_key(key: Option<Key>) -> Self {
        Self {
            id: None,
            key,
            render_object: None,
        }
    }

    /// Get the element ID.
    pub fn id(&self) -> Option<ElementId> {
        self.id
    }
}

impl Default for ContainerElement {
    fn default() -> Self {
        Self::new()
    }
}

impl Element for ContainerElement {
    fn mount(&mut self, _context: &mut ElementContext) {
        self.id = Some(ElementId::new());
    }

    fn update(&mut self, context: &mut ElementContext) {
        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
            context.mark_needs_paint(ro);
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        if let Some(ro) = self.render_object {
            context.dirty.mark_needs_paint(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, _visitor: &mut dyn FnMut(&dyn Element)) {
        // TODO: Container elements will visit children when implemented
    }

    fn render_object(&self) -> Option<RenderObjectId> {
        self.render_object
    }

    fn widget_key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
    }
}
