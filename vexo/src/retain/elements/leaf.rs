//! Leaf element implementation.
//!
//! LeafElement is the simplest element with no children.
//! Used by leaf widgets like Text, Image, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementId, Key, RenderObjectId};

/// Element for leaf widgets (no children).
pub struct LeafElement {
    id: Option<ElementId>,
    key: Option<Key>,
    render_object: Option<RenderObjectId>,
}

impl LeafElement {
    /// Create a new leaf element.
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

impl Default for LeafElement {
    fn default() -> Self {
        Self::new()
    }
}

impl Element for LeafElement {
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
        // Leaf elements have no children
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
    fn test_leaf_element_mount() {
        let mut element = LeafElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut context = ElementContext::new(None, &mut state, &mut dirty);

        element.mount(&mut context);

        assert!(element.id().is_some());
    }

    #[test]
    fn test_leaf_element_unmount() {
        let mut element = LeafElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut context = ElementContext::new(None, &mut state, &mut dirty);

        element.mount(&mut context);
        element.unmount(&mut context);

        // Element should be cleaned up
        // The id is still set (we don't clear it), but state should be removed
    }

    #[test]
    fn test_leaf_element_with_key() {
        let key = Key::new("test-key");
        let element = LeafElement::with_key(Some(key.clone()));

        assert_eq!(element.widget_key(), Some(key));
    }

    #[test]
    fn test_leaf_element_default() {
        let element = LeafElement::default();

        assert!(element.id().is_none());
        assert!(element.widget_key().is_none());
        assert!(element.render_object().is_none());
    }

    #[test]
    fn test_leaf_element_no_children() {
        let element = LeafElement::new();
        let mut count = 0;

        element.visit_children(&mut |_child| {
            count += 1;
        });

        assert_eq!(count, 0);
    }

    #[test]
    fn test_leaf_element_can_update() {
        let element = LeafElement::new();

        // can_update should return true for any widget
        assert!(element.can_update(&"any widget" as &dyn Any));
    }
}
