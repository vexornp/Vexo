//! FocusElement and FocusScopeElement for the focus system.
//!
//! These elements manage focus node lifecycle in the FocusManager:
//!
//! - **FocusElement**: Creates a regular focus node on mount, sets config
//!   (can_request_focus, skip_traversal), handles autofocus, and requests
//!   focus on pointer press. Removes the node on unmount.
//!
//! - **FocusScopeElement**: Creates a scope node on mount, sets traversal
//!   policy. No event handling - just scope structure. Removes the node
//!   on unmount.
//!
//! Both elements follow the single-child wrapper pattern (like
//! DecoratedContainerElement, GestureDetectorElement): they own a
//! ProxyRenderObject, inflate/reconcile a single child, and link the
//! child's render object via child_mounted().

use std::any::Any;

use crate::input::{ButtonState, InputEvent};
use crate::retain::{
    Element, ElementContext, ElementKey, EventContext,
    RenderObjectKey, Widget, WidgetKey, UpdateResult,
};
use crate::retain::elements::RenderObjectElement;
use crate::retain::focus::{FocusNodeKey, TraversalPolicy};
use crate::retain::focus::widget::{Focus, FocusScope};

// ============================================================================
// FOCUS ELEMENT
// ============================================================================

/// Element for the Focus widget.
///
/// Creates a focus node in the FocusManager on mount, configures it
/// with can_request_focus and skip_traversal from the widget, and
/// handles autofocus. On pointer press inside bounds, requests focus
/// via FocusManager (user_initiated = true).
///
/// Removes the focus node on unmount.
pub struct FocusElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_node: Option<FocusNodeKey>,
}

impl FocusElement {
    /// Create a new FocusElement.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_node: None,
        }
    }

    /// Get the child widget from the stored widget.
    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }

    /// Get the Focus widget configuration (if stored widget is a Focus).
    fn get_focus_widget(&self) -> Option<&Focus> {
        self.widget.as_ref()?.as_any().downcast_ref::<Focus>()
    }
}

impl Default for FocusElement {
    fn default() -> Self {
        Self::new()
    }
}

// Implement RenderObjectElement trait
impl RenderObjectElement for FocusElement {
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
impl Element for FocusElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Use RenderObjectElement's default mount for render object creation
        self.mount_render_object(context);

        // Create focus node via FocusManager
        let parent_focus = context.parent_focus_node();
        let focus_node = context.focus_manager().create_node(parent_focus);
        self.focus_node = Some(focus_node);

        // Set element key on the focus node so FocusManager can map
        // focus nodes back to elements
        let element_id = context.element_id;
        context.focus_manager().set_element_key(focus_node, Some(element_id));

        // Store the FocusNodeKey in StateStorage so children can find it
        // via parent_focus_node().
        context.insert_state(context.element_id, focus_node);

        // Apply widget configuration to the focus node
        if let Some(focus_widget) = self.get_focus_widget() {
            context.focus_manager().set_can_request_focus(
                focus_node,
                focus_widget.can_request_focus_value(),
            );
            context.focus_manager().set_skip_traversal(
                focus_node,
                focus_widget.skip_traversal_value(),
            );

            // Handle autofocus
            if focus_widget.autofocus_value() {
                context.focus_manager().request_focus(focus_node, true);
            }
        }

        // Inflate child widget
        if let Some(child_widget) = self.get_child_widget() {
            context.inflate_child(None, child_widget.clone_boxed());
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Use RenderObjectElement's default update for render object updates
        self.update_render_object(new_widget, context);

        // Update focus node configuration from new widget
        if let Some(focus_node) = self.focus_node {
            if let Some(focus_widget) = self.get_focus_widget() {
                context.focus_manager().set_can_request_focus(
                    focus_node,
                    focus_widget.can_request_focus_value(),
                );
                context.focus_manager().set_skip_traversal(
                    focus_node,
                    focus_widget.skip_traversal_value(),
                );
            }
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Remove focus node from FocusManager
        if let Some(focus_node) = self.focus_node {
            context.focus_manager().remove_node(focus_node);
            self.focus_node = None;
        }

        // Use RenderObjectElement's default unmount for render object removal
        // (which also removes state from StateStorage)
        self.unmount_render_object(context);
    }

    fn render_object(&self) -> Option<RenderObjectKey> {
        self.render_object
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn can_update(&self, widget: &dyn Any) -> bool {
        widget.downcast_ref::<Focus>().is_some()
    }

    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        // On pointer press inside bounds, request focus via EventContext
        if let InputEvent::PointerButton {
            state: ButtonState::Pressed,
            ..
        } = event
        {
            if context.is_pointer_inside() {
                if let Some(id) = self.id {
                    // Request focus for this element (user_initiated = true)
                    // (pointer press is a user action)
                    context.request_focus(id);
                    return Some(Box::new(()));
                }
            }
        }

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

            // Update focus node configuration from new widget
            if let Some(focus_node) = self.focus_node {
                if let Some(focus_widget) = self.get_focus_widget() {
                    context.focus_manager().set_can_request_focus(
                        focus_node,
                        focus_widget.can_request_focus_value(),
                    );
                    context.focus_manager().set_skip_traversal(
                        focus_node,
                        focus_widget.skip_traversal_value(),
                    );
                }
            }

            // Reconcile single child via child_ops
            if let Some(child_widget) = self.get_child_widget() {
                let old_child = context.children().first().copied();
                match old_child {
                    Some(old_child_key) => {
                        // Update existing child
                        context.update_child(old_child_key, child_widget.clone_boxed());
                    }
                    None => {
                        // Inflate new child
                        context.inflate_child(None, child_widget.clone_boxed());
                    }
                }
            } else if let Some(old_child_key) = context.children().first().copied() {
                // No new child widget - unmount the old child
                context.unmount_child(old_child_key);
            }
        }
    }

    fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
        // Link the child's render object to our ProxyRenderObject
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
    }
}

// ============================================================================
// FOCUS SCOPE ELEMENT
// ============================================================================

/// Element for the FocusScope widget.
///
/// Creates a scope node in the FocusManager on mount, sets traversal
/// policy from the widget configuration. No event handling - just
/// scope structure for focus traversal.
///
/// Removes the scope node on unmount.
pub struct FocusScopeElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_node: Option<FocusNodeKey>,
}

impl FocusScopeElement {
    /// Create a new FocusScopeElement.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_node: None,
        }
    }

    /// Get the child widget from the stored widget.
    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }

    /// Get the FocusScope widget configuration (if stored widget is a FocusScope).
    fn get_scope_widget(&self) -> Option<&FocusScope> {
        self.widget.as_ref()?.as_any().downcast_ref::<FocusScope>()
    }
}

impl Default for FocusScopeElement {
    fn default() -> Self {
        Self::new()
    }
}

// Implement RenderObjectElement trait
impl RenderObjectElement for FocusScopeElement {
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
impl Element for FocusScopeElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Use RenderObjectElement's default mount for render object creation
        self.mount_render_object(context);

        // Create scope node via FocusManager
        let parent_focus = context.parent_focus_node();
        let focus_node = context.focus_manager().create_scope(parent_focus);
        self.focus_node = Some(focus_node);

        // Set element key on the scope node
        let element_id = context.element_id;
        context.focus_manager().set_element_key(focus_node, Some(element_id));

        // Store the FocusNodeKey in StateStorage so children can find it
        // via parent_focus_node().
        context.insert_state(context.element_id, focus_node);

        // Apply widget configuration to the scope node
        if let Some(scope_widget) = self.get_scope_widget() {
            context.focus_manager().set_traversal_policy(
                focus_node,
                scope_widget.traversal_policy_value().clone(),
            );
        }

        // Inflate child widget
        if let Some(child_widget) = self.get_child_widget() {
            context.inflate_child(None, child_widget.clone_boxed());
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Use RenderObjectElement's default update for render object updates
        self.update_render_object(new_widget, context);

        // Update scope node configuration from new widget
        if let Some(focus_node) = self.focus_node {
            if let Some(scope_widget) = self.get_scope_widget() {
                context.focus_manager().set_traversal_policy(
                    focus_node,
                    scope_widget.traversal_policy_value().clone(),
                );
            }
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Remove scope node from FocusManager
        if let Some(focus_node) = self.focus_node {
            context.focus_manager().remove_node(focus_node);
            self.focus_node = None;
        }

        // Remove the FocusNodeKey from StateStorage.
        context.remove_state(context.element_id);

        // Use RenderObjectElement's default unmount for render object removal
        self.unmount_render_object(context);
    }

    fn render_object(&self) -> Option<RenderObjectKey> {
        self.render_object
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn can_update(&self, widget: &dyn Any) -> bool {
        widget.downcast_ref::<FocusScope>().is_some()
    }

    fn on_event(
        &mut self,
        _event: &InputEvent,
        _context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        // FocusScope does not handle events - it only provides scope structure
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

            // Update scope node configuration from new widget
            if let Some(focus_node) = self.focus_node {
                if let Some(scope_widget) = self.get_scope_widget() {
                    context.focus_manager().set_traversal_policy(
                        focus_node,
                        scope_widget.traversal_policy_value().clone(),
                    );
                }
            }

            // Reconcile single child via child_ops
            if let Some(child_widget) = self.get_child_widget() {
                let old_child = context.children().first().copied();
                match old_child {
                    Some(old_child_key) => {
                        // Update existing child
                        context.update_child(old_child_key, child_widget.clone_boxed());
                    }
                    None => {
                        // Inflate new child
                        context.inflate_child(None, child_widget.clone_boxed());
                    }
                }
            } else if let Some(old_child_key) = context.children().first().copied() {
                // No new child widget - unmount the old child
                context.unmount_child(old_child_key);
            }
        }
    }

    fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
        // Link the child's render object to our ProxyRenderObject
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
    }
}
