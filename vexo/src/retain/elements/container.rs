//! Container element implementation.
//!
//! ContainerElement is an element with children.
//! Used by container widgets like Column, Row, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementId, Key, RenderObjectId};

/// Element for container widgets (multiple children).
pub struct ContainerElement {
    id: Option<ElementId>,
    key: Option<Key>,
    children: Vec<ElementId>,
    render_object: Option<RenderObjectId>,
}

impl ContainerElement {
    /// Create a new container element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            children: Vec::new(),
            render_object: None,
        }
    }

    /// Create with a key.
    pub fn with_key(key: Option<Key>) -> Self {
        Self {
            id: None,
            key,
            children: Vec::new(),
            render_object: None,
        }
    }

    /// Get the element ID.
    pub fn id(&self) -> Option<ElementId> {
        self.id
    }

    /// Get the children.
    pub fn children(&self) -> &[ElementId] {
        &self.children
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
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Children are unmounted by the registry
        if let Some(ro) = self.render_object {
            context.dirty.mark_needs_paint(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, visitor: &mut dyn FnMut(&dyn Element)) {
        // Note: This requires access to the registry, which we don't have here.
        // In a full implementation, this would be handled differently.
        let _ = visitor;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{DirtyTracking, StateStorage};

    #[test]
    fn test_container_element_mount() {
        let mut element = ContainerElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut context = ElementContext::new(None, &mut state, &mut dirty);

        element.mount(&mut context);

        assert!(element.id().is_some());
    }

    #[test]
    fn test_container_element_children() {
        let mut element = ContainerElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut context = ElementContext::new(None, &mut state, &mut dirty);

        element.mount(&mut context);

        let mut count = 0;
        element.visit_children(&mut |_| count += 1);

        assert_eq!(count, 0);
    }
}
