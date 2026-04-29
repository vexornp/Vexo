//! Container element implementation.
//!
//! ContainerElement is an element with children.
//! Used by container widgets like Column, Row, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementId, ElementRegistry, Key, RenderObjectId, Widget};

/// Element for container widgets (multiple children).
pub struct ContainerElement {
    id: Option<ElementId>,
    key: Option<Key>,
    children: Vec<ElementId>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget>>,
}

impl ContainerElement {
    /// Create a new container element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            children: Vec::new(),
            render_object: None,
            widget: None,
        }
    }

    /// Create with a key.
    pub fn with_key(key: Option<Key>) -> Self {
        Self {
            id: None,
            key,
            children: Vec::new(),
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
    fn mount(&mut self, context: &mut ElementContext) {
        // Use the element ID from context - single source of truth
        self.id = Some(context.element_id);

        // Create render object if widget is set
        if let Some(widget) = &self.widget {
            let render_obj = widget.create_render_object();
            if let Some(ro_id) = context.create_render_object(render_obj, context.element_id) {
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
        // Children are unmounted by the registry
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        for &child_id in &self.children {
            if let Some(child) = registry.get(child_id) {
                visitor(child);
            }
        }
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
        // Container elements don't handle events themselves
        // Hit testing finds the specific child element
        None
    }

    fn add_child(&mut self, child_id: ElementId) {
        self.children.push(child_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{DirtyTracking, StateStorage, RenderObjectRegistry, Column, Text};

    #[test]
    fn test_container_element_mount() {
        let mut element = ContainerElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut context = ElementContext::new(
            ElementId::new(),
            None,
            &mut state,
            &mut dirty,
        );

        element.mount(&mut context);

        assert!(element.id().is_some());
    }

    #[test]
    fn test_container_element_children() {
        use crate::retain::element::ElementRegistry;

        let mut element = ContainerElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut context = ElementContext::new(
            ElementId::new(),
            None,
            &mut state,
            &mut dirty,
        );

        element.mount(&mut context);

        let registry = ElementRegistry::new();
        let mut count = 0;
        element.visit_children(&registry, &mut |_| count += 1);

        assert_eq!(count, 0);
    }

    #[test]
    fn test_container_element_mount_creates_render_object() {
        // This test verifies that mounting a container element with a widget
        // creates a render object in the registry.
        let mut element = ContainerElement::new();
        let widget = Column::new().push(Text::new("Hello"));
        element.set_widget(&widget);

        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let mut context = ElementContext::with_registry(
            ElementId::new(),
            None,
            &mut state,
            &mut dirty,
            &mut render_objects,
        );

        element.mount(&mut context);

        // After mount, the element should have a render object ID
        assert!(element.render_object().is_some());

        // The registry should contain the render object
        let ro_id = element.render_object().unwrap();
        assert!(render_objects.get(ro_id).is_some());
    }

    #[test]
    fn test_container_element_unmount_removes_render_object() {
        // This test verifies that unmounting a container element removes
        // the render object from the registry.
        let mut element = ContainerElement::new();
        let widget = Column::new().push(Text::new("Hello"));
        element.set_widget(&widget);

        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let mut context = ElementContext::with_registry(
            ElementId::new(),
            None,
            &mut state,
            &mut dirty,
            &mut render_objects,
        );

        element.mount(&mut context);
        let ro_id = element.render_object().unwrap();

        // Now unmount
        element.unmount(&mut context);

        // The render object should be removed from the registry
        assert!(render_objects.get(ro_id).is_none());
    }
}
