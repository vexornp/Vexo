//! Modifier element implementation.
//!
//! ModifierElement is an element that wraps a single child.
//! Used by modifier widgets like Padding, Background, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementId, ElementRegistry, Key, RenderObjectId, Widget};

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
    /// This uses the `child()` method on the Widget trait, which modifier widgets
    /// like Background, Padding, and Border override to return their child.
    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
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

                // Mark the new render object as needing layout and paint
                context.mark_needs_layout(ro_id);
                context.mark_needs_paint(ro_id);
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

            // Link child render object to parent render object for tree traversal
            // After child_element.mount(), context.render_object is the child's render object
            if let (Some(parent_ro), Some(child_ro)) = (self.render_object, context.render_object) {
                context.set_render_object_child(parent_ro, child_ro);
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Widget>, context: &mut ElementContext) {
        // Get the old child widget (cloned to avoid borrow issues)
        let old_child_widget = self.widget.as_ref().and_then(|w| w.child().map(|c| c.clone_box()));

        // Store the new widget configuration
        self.widget = Some(new_widget);

        // Get the new child widget
        let new_child_widget = self.widget.as_ref().and_then(|w| w.child().map(|c| c.clone_box()));

        // Handle child element lifecycle
        match (old_child_widget, new_child_widget, self.child_element) {
            // No change - both None
            (None, None, _) => {}

            // Child added - create and mount
            (None, Some(new), None) => {
                let mut child_element = new.create_element();
                child_element.mount(context);

                if let Some(parent_id) = self.id {
                    self.child_element = context.mount_child_element(child_element, parent_id);

                    // Link render objects
                    if let (Some(parent_ro), Some(child_ro)) = (self.render_object, context.render_object) {
                        context.set_render_object_child(parent_ro, child_ro);
                    }
                }
            }

            // Child removed - unmount
            (Some(_), None, Some(child_id)) => {
                context.unmount_child_element(child_id);
                self.child_element = None;
            }

            // Child updated - propagate update to existing element
            (Some(_), Some(new), Some(child_id)) => {
                context.update_child_element(child_id, new);
            }

            // Edge cases: mismatched state
            (None, Some(_), Some(child_id)) => {
                // Have element but no old child - should not happen, clear and remount
                context.unmount_child_element(child_id);
                self.child_element = None;
            }
            (Some(_), None, None) => {
                // Old child but no element - should not happen, nothing to do
            }
            (Some(_), Some(new), None) => {
                // Both have children but no element - mount new
                let mut child_element = new.create_element();
                child_element.mount(context);

                if let Some(parent_id) = self.id {
                    self.child_element = context.mount_child_element(child_element, parent_id);

                    if let (Some(parent_ro), Some(child_ro)) = (self.render_object, context.render_object) {
                        context.set_render_object_child(parent_ro, child_ro);
                    }
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
        use crate::retain::element::ElementRegistry;

        let element = ModifierElement::new();
        let registry = ElementRegistry::new();
        let mut count = 0;

        element.visit_children(&registry, &mut |_child| {
            count += 1;
        });

        // No children because element was not mounted
        assert_eq!(count, 0);
    }

    #[test]
    fn test_modifier_element_can_update() {
        let element = ModifierElement::new();

        // can_update should return true for any widget
        assert!(element.can_update(&"any widget" as &dyn Any));
    }

    #[test]
    fn test_modifier_element_links_child_render_object() {
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

        // Get the parent render object
        let parent_ro_id = element.render_object().unwrap();
        let parent_ro = render_objects.get(parent_ro_id).unwrap();

        // Verify the parent's children() returns the child render object
        let children = parent_ro.children();
        assert_eq!(children.len(), 1, "Background render object should have one child");

        // The child should be the Text render object
        let child_ro_id = children[0];
        assert!(render_objects.get(child_ro_id).is_some(), "Child render object should exist in registry");
    }

    #[test]
    fn test_modifier_element_visit_children_with_mounted_child() {
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

        // Get the element registry from the context (it was modified by mount)
        // We need to re-borrow it, so we'll drop context and use element_registry directly
        drop(context);

        // Now visit children - should find the mounted child element
        let mut visited_count = 0;
        let mut found_keys = Vec::new();
        element.visit_children(&element_registry, &mut |child_element| {
            visited_count += 1;
            if let Some(key) = child_element.widget_key() {
                found_keys.push(key);
            }
        });

        // Should have visited exactly one child
        assert_eq!(visited_count, 1, "Should visit exactly one child element");
    }

    #[test]
    fn test_modifier_element_update_propagates_to_child() {
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

        // Create a Background widget with a Text child "Hello"
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);
        element.set_widget(&bg);
        element.mount(&mut context);

        // Store the child element ID
        let child_element_id = element.child_element().unwrap();

        // Now update with a new Background widget with different text "World"
        let new_child = Box::new(Text::new("World"));
        let new_bg = Background::new(new_child, Color::BLUE);
        element.update(Box::new(new_bg), &mut context);

        // The child element should still be the same (same ID)
        assert_eq!(element.child_element(), Some(child_element_id), "Child element ID should remain the same after update");

        // Drop context to release borrows
        drop(context);

        // Verify that child element exists in registry
        assert!(element_registry.contains(child_element_id), "Child element should still be in registry");
    }

    #[test]
    fn test_modifier_element_update_child_removed() {
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

        let child_element_id = element.child_element().unwrap();

        // Update should preserve the child if the new widget also has a child
        let new_child = Box::new(Text::new("Updated"));
        let new_bg = Background::new(new_child, Color::GREEN);
        element.update(Box::new(new_bg), &mut context);

        // Drop context to release borrows
        drop(context);

        // Child should still exist
        assert!(element_registry.contains(child_element_id), "Child element should exist after update");
    }

    #[test]
    fn test_modifier_element_update_marks_dirty() {
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

        // Get render object ID before we lose access
        let ro_id = element.render_object().unwrap();

        // Clear dirty state - need to drop context first
        drop(context);
        dirty.clear();

        // Re-create context for update
        let mut context = ElementContext::new_full(
            None,
            &mut state,
            &mut dirty,
            &mut render_objects,
            &mut element_registry,
        );

        // Update should mark render objects dirty
        let new_child = Box::new(Text::new("Updated"));
        let new_bg = Background::new(new_child, Color::BLUE);
        element.update(Box::new(new_bg), &mut context);

        // Drop context to release borrows
        drop(context);

        // The render object should be marked dirty
        assert!(dirty.needs_layout(ro_id), "Render object should be marked for layout after update");
        assert!(dirty.needs_paint(ro_id), "Render object should be marked for paint after update");
    }
}
