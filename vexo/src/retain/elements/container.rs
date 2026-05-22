//! ContainerElement implementation for multi-child widgets.
//!
//! ContainerElement is an element with multiple children.
//! Used by container widgets like Column, Row, etc.
//!
//! This element owns a render object and manages its lifecycle through
//! the RenderObjectElement trait.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementKey, RenderObjectKey, Widget, UpdateResult};
use crate::retain::elements::RenderObjectElement;
use crate::retain::key::WidgetKey;
use crate::retain::focus::attachment::FocusAttachment;

/// Element for container widgets (multiple children).
///
/// This element:
/// - Owns a render object
/// - Has multiple children
/// - Manages render object lifecycle via RenderObjectElement trait
pub struct ContainerElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl ContainerElement {
    /// Create a new container element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
        }
    }

    /// Create with a key.
    pub fn with_key(key: Option<WidgetKey>) -> Self {
        Self {
            id: None,
            key,
            render_object: None,
            widget: None,
            focus_attachment: None,
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

// Implement Element trait
impl Element for ContainerElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Create focus attachment BEFORE mounting children.
        // Children will look up this element's focus node as their parent
        // when they mount, so it must exist before child mounting begins.
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context.focus_manager().create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        // Use RenderObjectElement's default mount for render object creation
        self.mount_render_object(context);

        // Mount children via child_ops (emit Inflate commands)
        // The pipeline will execute them after mount() returns,
        // then call child_mounted() to link each child's render object.
        if let Some(widget) = &self.widget {
            let child_widgets: Vec<Box<dyn Widget>> = widget.children()
                .iter()
                .map(|c| c.clone_boxed())
                .collect();

            for (i, child_widget) in child_widgets.into_iter().enumerate() {
                context.inflate_child(Some(i), child_widget);
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

        // Detach focus node from the focus tree AFTER render object cleanup.
        // Children are already unmounted by the reconciler before this method
        // is called, so their focus nodes have already been detached.
        if let Some(mut attachment) = self.focus_attachment.take() {
            attachment.detach(context.focus_manager());
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

            // Reconcile children via child_ops commands:
            // - Update existing children at matching positions
            // - Inflate new children for positions beyond old count
            // - Unmount excess old children
            let old_children = context.children().to_vec();
            let old_len = old_children.len();
            let new_len = new_child_widgets.len();

            for (i, new_child_widget) in new_child_widgets.into_iter().enumerate() {
                if i < old_len {
                    // Update existing child
                    context.update_child(old_children[i], new_child_widget);
                } else {
                    // Inflate new child
                    context.inflate_child(Some(i), new_child_widget);
                }
            }

            // Unmount excess children (in reverse order to preserve indices)
            for i in (new_len..old_len).rev() {
                context.unmount_child(old_children[i]);
            }
        }

        // Reparent focus node if parent changed
        if let Some(attachment) = self.focus_attachment.as_ref() {
            let new_parent_id = context.parent_focus_node_id();
            attachment.reparent_to(new_parent_id, context.focus_manager());
        }
    }

    fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
        // Link the child's render object to our render object
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
    }

    fn focus_attachment(&self) -> &Option<FocusAttachment> {
        &self.focus_attachment
    }

    fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment> {
        &mut self.focus_attachment
    }
}