//! Leaf element implementation.
//!
//! LeafElement is the simplest element with no children.
//! Used by leaf widgets like Text, Image, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementId, ElementRegistry, Key, RenderObjectId, Widget};

/// Element for leaf widgets (no children).
pub struct LeafElement {
    id: Option<ElementId>,
    key: Option<Key>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget>>,
}

impl LeafElement {
    /// Create a new leaf element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
        }
    }

    /// Create with a key.
    pub fn with_key(key: Option<Key>) -> Self {
        Self {
            id: None,
            key,
            render_object: None,
            widget: None,
        }
    }

    /// Set the widget for this element.
    ///
    /// Must be called before mount to create the render object.
    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(widget.clone_box());
        self.key = widget.key();
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
    fn mount(&mut self, context: &mut ElementContext) {
        self.id = Some(ElementId::new());

        // Create render object if widget is set
        if let (Some(widget), Some(id)) = (&self.widget, self.id) {
            let render_obj = widget.create_render_object();
            if let Some(ro_id) = context.create_render_object(render_obj, id) {
                self.render_object = Some(ro_id);
                context.render_object = Some(ro_id);

                // Mark the new render object as needing layout and paint
                context.mark_needs_layout(ro_id);
                context.mark_needs_paint(ro_id);
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Widget>, context: &mut ElementContext) {
        // Store the new widget configuration
        self.widget = Some(new_widget);

        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
            context.mark_needs_paint(ro);
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Remove render object from registry
        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
            context.dirty.mark_needs_paint(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {
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

    fn on_event(
        &mut self,
        _event: &crate::input::InputEvent,
        _context: &mut crate::retain::EventContext,
    ) -> Option<Box<dyn Any>> {
        // Leaf elements (like Text) don't handle events by default
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{DirtyTracking, StateStorage, RenderObjectRegistry, Text};

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
    fn test_leaf_element_mount_creates_render_object() {
        // This test verifies that mounting a leaf element with a widget
        // creates a render object in the registry.
        let mut element = LeafElement::new();
        let widget = Text::new("Hello");
        element.set_widget(&widget);

        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let mut context = ElementContext::new_with_registry(None, &mut state, &mut dirty, &mut render_objects);

        element.mount(&mut context);

        // After mount, the element should have a render object ID
        assert!(element.render_object().is_some());

        // The registry should contain the render object
        let ro_id = element.render_object().unwrap();
        assert!(render_objects.get(ro_id).is_some());
    }

    #[test]
    fn test_leaf_element_unmount_removes_render_object() {
        // This test verifies that unmounting a leaf element removes
        // the render object from the registry.
        let mut element = LeafElement::new();
        let widget = Text::new("Hello");
        element.set_widget(&widget);

        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let mut context = ElementContext::new_with_registry(None, &mut state, &mut dirty, &mut render_objects);

        element.mount(&mut context);
        let ro_id = element.render_object().unwrap();

        // Now unmount
        element.unmount(&mut context);

        // The render object should be removed from the registry
        assert!(render_objects.get(ro_id).is_none());
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
        use crate::retain::element::ElementRegistry;

        let element = LeafElement::new();
        let registry = ElementRegistry::new();
        let mut count = 0;

        element.visit_children(&registry, &mut |_child| {
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
