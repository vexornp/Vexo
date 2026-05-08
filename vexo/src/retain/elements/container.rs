//! Container element implementation.
//!
//! ContainerElement is an element with children.
//! Used by container widgets like Column, Row, etc.

use std::any::Any;
use std::collections::{HashMap, HashSet};

use crate::retain::{Element, ElementContext, ElementId, ElementRegistry, RenderObjectId, Widget, UpdateResult};
use crate::retain::key::{Key, WidgetKey};

/// Element for container widgets (multiple children).
pub struct ContainerElement {
    id: Option<ElementId>,
    key: Option<WidgetKey>,
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
    pub fn with_key(key: Option<WidgetKey>) -> Self {
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
        self.widget = Some(widget.clone_boxed());
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

    /// Reconcile children with new widgets.
    ///
    /// This is the Flutter-style per-element reconciliation.
    fn reconcile_children_internal(
        &mut self,
        registry: &mut ElementRegistry,
        context: &mut ElementContext,
        new_child_widgets: Vec<Box<dyn Widget>>,
    ) {
        let existing_children = self.children.clone();
        let mut new_children = Vec::new();
        let mut matched = HashSet::new();

        // Build key map for existing children (local keys only)
        let key_map: HashMap<Key, ElementId> = existing_children
            .iter()
            .filter_map(|&id| {
                registry.get(id)
                    .and_then(|el| match el.widget_key() {
                        Some(WidgetKey::Local(k)) => Some((k, id)),
                        _ => None,
                    })
            })
            .collect();

        // Match new widgets to existing elements
        for (index, child_widget) in new_child_widgets.into_iter().enumerate() {
            let element_id = match child_widget.key() {
                Some(WidgetKey::Local(key)) => {
                    // Local key: look up in map
                    if let Some(&id) = key_map.get(&key) {
                        if !matched.contains(&id) {
                            matched.insert(id);
                            Some(id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                Some(WidgetKey::Global(_)) => {
                    // Global keys are handled by the pipeline's global registry
                    // For now, fall back to position-based matching
                    if let Some(&id) = existing_children.get(index) {
                        if !matched.contains(&id) {
                            matched.insert(id);
                            Some(id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                None => {
                    // Non-keyed: match by position
                    if let Some(&id) = existing_children.get(index) {
                        if !matched.contains(&id) {
                            matched.insert(id);
                            Some(id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            };

            if let Some(child_id) = element_id {
                // Update existing child
                let widget_any = Box::new(child_widget.clone_boxed());
                if let Some(child_element) = registry.get_mut(child_id) {
                    child_element.rebuild(widget_any, context);
                }
                new_children.push(child_id);
            } else {
                // Mount new child - need to use the pipeline's mount logic
                // For now, we'll skip this and let the full reconcile handle it
                // This is a limitation that will be addressed in Task 5
            }
        }

        // Unmount unmatched children
        for child_id in existing_children {
            if !matched.contains(&child_id) {
                registry.unmount(child_id);
            }
        }

        self.children = new_children;
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

        // Register global key if present
        if let Some(WidgetKey::Global(key)) = &self.key {
            let _ = context.register_global_key(key.clone(), context.element_id);
        }

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

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // The widget is passed as Box<dyn Widget> but type-erased to Box<dyn Any>
        // We need to downcast it back
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.widget = Some(*widget);

            // Update the render object with new properties from the widget
            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self.widget.as_ref().unwrap().update_render_object(ro.as_mut());

                    // Only mark dirty based on what actually changed
                    if result.contains(UpdateResult::LAYOUT) {
                        context.mark_needs_layout(ro_id);
                    }
                    if result.contains(UpdateResult::PAINT) {
                        context.mark_needs_paint(ro_id);
                    }
                }
            }
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Unregister global key if present
        if let Some(WidgetKey::Global(_)) = &self.key {
            if let Some(id) = self.id {
                context.unregister_global_key(id);
            }
        }

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

    fn widget_key(&self) -> Option<WidgetKey> {
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

    fn rebuild(
        &mut self,
        new_widget: Box<dyn Any>,
        context: &mut ElementContext,
    ) {
        // Downcast and store the new widget
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.widget = Some(*widget);

            // Update the render object with new properties
            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self.widget.as_ref().unwrap().update_render_object(ro.as_mut());

                    // Only mark dirty based on what actually changed
                    if result.contains(UpdateResult::LAYOUT) {
                        context.mark_needs_layout(ro_id);
                    }
                    if result.contains(UpdateResult::PAINT) {
                        context.mark_needs_paint(ro_id);
                    }
                }
            }

            // Get new child widgets
            let new_child_widgets: Vec<Box<dyn Widget>> = self.widget.as_ref()
                .map(|w| w.children().iter().map(|c| c.clone_boxed()).collect())
                .unwrap_or_default();

            // Reconcile children - extract the registry to avoid double borrow
            // We need to temporarily take the element_registry to avoid borrowing conflicts
            let element_registry = context.element_registry.take();
            if let Some(mut registry) = element_registry {
                self.reconcile_children_internal(
                    &mut registry,
                    context,
                    new_child_widgets,
                );
                // Restore the registry
                context.element_registry = Some(registry);
            }
        }
    }

    fn has_children(&self) -> bool {
        true
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
