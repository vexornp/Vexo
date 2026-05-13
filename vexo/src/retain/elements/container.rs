//! ContainerElement implementation for multi-child widgets.
//!
//! ContainerElement is an element with multiple children.
//! Used by container widgets like Column, Row, etc.
//!
//! This element owns a render object and manages its lifecycle through
//! the RenderObjectElement and MultiChildRenderObjectElement traits.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementKey, ElementRegistry, RenderObjectKey, Widget, UpdateResult};
use crate::retain::elements::{RenderObjectElement, MultiChildRenderObjectElement};
use crate::retain::key::WidgetKey;

/// Element for container widgets (multiple children).
///
/// This element:
/// - Owns a render object
/// - Has multiple children
/// - Manages render object lifecycle via RenderObjectElement trait
/// - Manages children via MultiChildRenderObjectElement trait
pub struct ContainerElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    children: Vec<ElementKey>,
    render_object: Option<RenderObjectKey>,
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
    pub fn id(&self) -> Option<ElementKey> {
        self.id
    }

    /// Get the children.
    pub fn children(&self) -> &[ElementKey] {
        &self.children
    }
}

impl Default for ContainerElement {
    fn default() -> Self {
        Self::new()
    }
}

// Implement RenderObjectElement trait
impl RenderObjectElement for ContainerElement {
    fn widget(&self) -> Option<&dyn Widget> {
        self.widget.as_deref()
    }

    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        self.widget = Some(widget);
    }

    fn render_object_id(&self) -> Option<RenderObjectKey> {
        self.render_object
    }

    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) {
        self.render_object = id;
    }

    fn stored_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn set_stored_key(&mut self, key: Option<WidgetKey>) {
        self.key = key;
    }

    fn element_id(&self) -> Option<ElementKey> {
        self.id
    }

    fn set_element_id(&mut self, id: Option<ElementKey>) {
        self.id = id;
    }
}

// Implement MultiChildRenderObjectElement trait
impl MultiChildRenderObjectElement for ContainerElement {
    fn child_elements(&self) -> &[ElementKey] {
        &self.children
    }

    fn set_child_elements(&mut self, children: Vec<ElementKey>) {
        self.children = children;
    }

    fn add_child_element(&mut self, child: ElementKey) {
        self.children.push(child);
    }
}

// Implement Element trait
impl Element for ContainerElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Use RenderObjectElement's default mount for render object creation
        self.mount_render_object(context);

        // Mount children - this element manages its own children during mount
        if let Some(widget) = &self.widget {
            // Get child widgets from the widget
            let child_widgets: Vec<Box<dyn Widget>> = widget.children()
                .iter()
                .map(|c| c.clone_boxed())
                .collect();

            // Mount each child and collect their IDs
            let mut child_render_objects = Vec::new();
            for child_widget in child_widgets {
                if let Some(child_id) = context.inflate_widget(child_widget) {
                    self.children.push(child_id);

                    // Track child render objects for linking
                    if let Some(registry) = &context.element_registry {
                        if let Some(child_ro) = registry.get(child_id).and_then(|el| el.render_object()) {
                            child_render_objects.push(child_ro);
                        }
                    }
                }
            }

            // Link child render objects to this container's render object
            for child_ro in &child_render_objects {
                self.insert_child_render_object(*child_ro, context);
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Use RenderObjectElement's default update for render object updates
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Use RenderObjectElement's default unmount for render object removal
        self.unmount_render_object(context);
        // Children are unmounted by the registry
    }

    fn visit_children(&self, registry: &ElementRegistry, visitor: &mut dyn FnMut(&dyn Element)) {
        for &child_id in &self.children {
            if let Some(child) = registry.get(child_id) {
                visitor(child);
            }
        }
    }

    fn render_object(&self) -> Option<RenderObjectKey> {
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

    fn add_child(&mut self, child_id: ElementKey) {
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

            // Reconcile children using update_child from the Element trait
            let mut updated_children = Vec::new();
            for (i, new_child_widget) in new_child_widgets.into_iter().enumerate() {
                let old_child = self.children.get(i).copied();
                let new_child = self.update_child(old_child, Some(new_child_widget), Some(i), context);
                if let Some(child_id) = new_child {
                    updated_children.push(child_id);
                }
            }

            // Collect children to unmount (those that weren't matched)
            let children_to_unmount: Vec<ElementKey> = self.children.iter()
                .skip(updated_children.len())
                .copied()
                .collect();

            // Unmount remaining old children that weren't matched
            for old_child in children_to_unmount {
                self.update_child(Some(old_child), None, None, context);
            }

            self.children = updated_children;
        }
    }

    fn has_children(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    fn make_element_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    use super::*;
    use std::sync::mpsc;
    use crate::retain::{DirtyTracking, StateStorage, RenderObjectRegistry, Column, Text, BuildOwner};

    #[test]
    fn test_container_element_mount() {
        let mut element = ContainerElement::new();
        let mut state = StateStorage::new();
        let mut dirty = DirtyTracking::new();
        let mut context = ElementContext::new(
            make_element_key(),
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
            make_element_key(),
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
        let mut element_registry = ElementRegistry::new();
        let build_owner = BuildOwner::new();
        let (dirty_sender, _) = mpsc::channel();
        let mut context = ElementContext::full(
            make_element_key(),
            None,
            &mut state,
            &mut dirty,
            &mut render_objects,
            &mut element_registry,
            &build_owner,
            &dirty_sender,
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
        let mut element_registry = ElementRegistry::new();
        let build_owner = BuildOwner::new();
        let (dirty_sender, _) = mpsc::channel();
        let mut context = ElementContext::full(
            make_element_key(),
            None,
            &mut state,
            &mut dirty,
            &mut render_objects,
            &mut element_registry,
            &build_owner,
            &dirty_sender,
        );

        element.mount(&mut context);
        let ro_id = element.render_object().unwrap();

        // Now unmount
        element.unmount(&mut context);

        // The render object should be removed from the registry
        assert!(render_objects.get(ro_id).is_none());
    }
}
