//! Modifier element implementation.
//!
//! ModifierElement is an element that wraps a single child.
//! Used by modifier widgets like Padding, Background, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementId, Key, RenderObjectId, Widget};

/// Element for modifier widgets (wraps single child).
pub struct ModifierElement {
    id: Option<ElementId>,
    key: Option<Key>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget>>,
    child_element: Option<ElementId>,
}

impl ModifierElement {
    /// Create a new modifier element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            child_element: None,
        }
    }

    /// Create with a key.
    pub fn with_key(key: Option<Key>) -> Self {
        Self {
            id: None,
            key,
            render_object: None,
            widget: None,
            child_element: None,
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

    /// Get the child element ID.
    pub fn child_element(&self) -> Option<ElementId> {
        self.child_element
    }

    /// Try to get the child widget from the stored widget.
    ///
    /// This attempts to downcast the widget to Background to get its child.
    fn get_child_widget(&self) -> Option<Box<dyn Widget>> {
        // For now, we check if the widget is a Background and get its child
        // In a more generic system, we'd have a ChildWidget trait
        let widget = self.widget.as_ref()?;
        let any = widget.as_any();

        // Try to downcast to Background
        if let Some(bg) = any.downcast_ref::<crate::retain::widgets::Background>() {
            Some(bg.child().clone_box())
        } else {
            None
        }
    }
}

impl Default for ModifierElement {
    fn default() -> Self {
        Self::new()
    }
}

impl Element for ModifierElement {
    fn mount(&mut self, context: &mut ElementContext) {
        self.id = Some(ElementId::new());

        // Create render object if widget is set
        if let (Some(widget), Some(id)) = (&self.widget, self.id) {
            let render_obj = widget.create_render_object();
            if let Some(ro_id) = context.create_render_object(render_obj, id) {
                self.render_object = Some(ro_id);
                context.render_object = Some(ro_id);
            }
        }

        // Create and mount child element if widget has a child
        if let (Some(child_widget), Some(parent_id)) = (self.get_child_widget(), self.id) {
            let mut child_element = child_widget.create_element();
            child_element.mount(context);

            // Store the child element in the registry if available
            if let Some(child_id) = context.mount_child_element(child_element, parent_id) {
                self.child_element = Some(child_id);
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

    fn visit_children(&self, _visitor: &mut dyn FnMut(&dyn Element)) {
        // TODO: Modifier elements will visit their single child when implemented
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
    use crate::retain::{Background, DirtyTracking, RenderObjectRegistry, StateStorage, Text};
    use crate::core::Color;

    #[test]
    fn test_modifier_element_mount() {
        let mut element = ModifierElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut context = ElementContext::new(None, &mut state, &mut dirty);

        element.mount(&mut context);

        assert!(element.id().is_some());
    }

    #[test]
    fn test_modifier_element_mount_creates_render_object() {
        let mut element = ModifierElement::new();
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);
        element.set_widget(&bg);

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
    fn test_modifier_element_creates_child_element() {
        use crate::retain::element::ElementRegistry;

        let mut element = ModifierElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let mut element_registry = ElementRegistry::new();
        let mut context = ElementContext::new_full(
            None,
            &mut state,
            &mut dirty,
            &mut render_objects,
            &mut element_registry,
        );

        // Create a Background widget with a Text child
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);
        element.set_widget(&bg);
        element.mount(&mut context);

        // Should have created an element ID
        assert!(element.id().is_some());

        // Should have created a render object
        assert!(element.render_object().is_some());

        // Should have created and stored a child element
        assert!(element.child_element().is_some());
    }

    #[test]
    fn test_modifier_element_get_child_widget() {
        let mut element = ModifierElement::new();

        // Without a widget, should return None
        assert!(element.get_child_widget().is_none());

        // With a Background widget that has a child
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);
        element.set_widget(&bg);

        // Should now return the child widget
        let child_widget = element.get_child_widget();
        assert!(child_widget.is_some());
    }

    #[test]
    fn test_modifier_element_unmount_removes_render_object() {
        let mut element = ModifierElement::new();
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);
        element.set_widget(&bg);

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
    fn test_modifier_element_default() {
        let element = ModifierElement::default();

        assert!(element.id().is_none());
        assert!(element.widget_key().is_none());
        assert!(element.render_object().is_none());
        assert!(element.child_element().is_none());
    }

    #[test]
    fn test_modifier_element_no_children_visited() {
        let element = ModifierElement::new();
        let mut count = 0;

        element.visit_children(&mut |_child| {
            count += 1;
        });

        // Currently no children are visited (TODO)
        assert_eq!(count, 0);
    }

    #[test]
    fn test_modifier_element_can_update() {
        let element = ModifierElement::new();

        // can_update should return true for any widget
        assert!(element.can_update(&"any widget" as &dyn Any));
    }
}
