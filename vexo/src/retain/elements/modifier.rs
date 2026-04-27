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
}

impl ModifierElement {
    /// Create a new modifier element.
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
