//! Modifier element implementation.
//!
//! ModifierElement is an element that wraps a single child.
//! Used by modifier widgets like Padding, Background, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementId, ElementRegistry, Key, RenderObjectId, Widget};

/// Element for modifier widgets (wraps single child).
///
/// Generic over the message type `M` to support ELM-style typed messages.
/// For non-interactive widgets, `M = ()`.
pub struct ModifierElement<M: Clone + Send + 'static = ()> {
    id: Option<ElementId>,
    key: Option<Key>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget<M>>>,
    child_element: Option<ElementId>,
}

impl<M: Clone + Send + 'static> ModifierElement<M> {
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
    pub fn set_widget(&mut self, widget: &dyn Widget<M>) {
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
    /// This uses the `child()` method on the Widget trait, which modifier widgets
    /// like Background, Padding, and Border override to return their child.
    fn get_child_widget(&self) -> Option<&dyn Widget<M>> {
        self.widget.as_ref()?.child()
    }
}

impl<M: Clone + Send + 'static> Default for ModifierElement<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Clone + Send + 'static> Element for ModifierElement<M> {
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

        // Note: Child mounting is handled by the pipeline, not by the element itself
        // The pipeline calls add_child() after mounting the child element
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // The widget is passed as Box<dyn Widget<M>> but type-erased to Box<dyn Any>
        // We need to downcast it back
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget<M>>>() {
            self.widget = Some(*widget);

            // Update the render object with new properties from the widget
            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    self.widget.as_ref().unwrap().update_render_object(ro.as_mut());
                }
            }
        }

        // Mark render objects dirty
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

    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        if let Some(child_id) = self.child_element {
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
        // Modifier elements don't handle events themselves
        // The hit test already found the correct target
        None
    }

    fn add_child(&mut self, child_id: ElementId) {
        self.child_element = Some(child_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::{Background, DirtyTracking, RenderObjectRegistry, StateStorage, Text};
    use crate::core::Color;

    #[test]
    fn test_modifier_element_mount() {
        let mut element: ModifierElement<()> = ModifierElement::new();
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
    fn test_modifier_element_mount_creates_render_object() {
        let mut element: ModifierElement<()> = ModifierElement::new();
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);
        element.set_widget(&bg);

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
    fn test_modifier_element_default() {
        let element: ModifierElement<()> = ModifierElement::default();

        assert!(element.id().is_none());
        assert!(element.widget_key().is_none());
        assert!(element.render_object().is_none());
        assert!(element.child_element().is_none());
    }

    #[test]
    fn test_modifier_element_can_update() {
        let element: ModifierElement<()> = ModifierElement::new();

        // can_update should return true for any widget
        assert!(element.can_update(&"any widget" as &dyn Any));
    }
}
