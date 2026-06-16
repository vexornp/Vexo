//! Focus widget and FocusElement for the retain-mode system.
//!
//! `Focus` is a single-child wrapper widget that creates a focus node in
//! the FocusManager's focus tree when its element mounts. It supports an
//! `on_focus_change` callback that fires when the node or a descendant
//! gains or loses primary focus.

use std::any::Any;
use std::sync::Arc;

use crate::element::Element;
use crate::element_context::ElementContext;
use crate::element_state::StateStorage;
use crate::elements::RenderObjectElement;
use crate::event_context::EventContext;
use crate::focus::attachment::FocusAttachment;
use crate::id::{ElementKey, RenderObjectKey};
use crate::input::InputEvent;
use crate::key::WidgetKey;
use crate::render_object::RenderObject;
use crate::widgets::Widget;
use crate::UpdateResult;

// ============================================================================
// Focus Widget
// ============================================================================

/// A widget that wraps a child and makes it focusable.
///
/// When the corresponding element mounts, a focus node is registered in the
/// FocusManager's focus tree. If `autofocus` is set, the node requests focus
/// during mount.
///
/// The `on_focus_change` callback fires when this node or a descendant
/// gains/loses primary focus, matching Flutter's `Focus.onFocusChange`
/// behavior. Called with `true` when focus is gained, `false` when lost.
///
/// Focus is a proxy widget — it delegates rendering entirely to its child.
pub struct Focus {
    child: Box<dyn Widget>,
    autofocus: bool,
    on_focus_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl Focus {
    /// Create a new Focus widget wrapping the given child.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            autofocus: false,
            on_focus_change: None,
        }
    }

    /// Set whether this focus node should automatically request focus on mount.
    pub fn autofocus(mut self, autofocus: bool) -> Self {
        self.autofocus = autofocus;
        self
    }

    /// Set a callback that fires when this node or a descendant gains/loses focus.
    ///
    /// Called with `true` when focus is gained, `false` when lost.
    /// The callback can trigger a rebuild by calling a dirty callback
    /// captured from a StatefulWidget's `State::init()`.
    pub fn on_focus_change(mut self, callback: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_focus_change = Some(Arc::new(callback));
        self
    }
}

impl Clone for Focus {
    fn clone(&self) -> Self {
        Self {
            child: self.child.clone_boxed(),
            autofocus: self.autofocus,
            on_focus_change: self.on_focus_change.clone(),
        }
    }
}

impl Widget for Focus {
    fn key(&self) -> Option<WidgetKey> {
        self.child.key()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = FocusElement::new();
        elem.set_widget(self.clone_boxed());
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        self.child.create_render_object()
    }

    fn can_update(&self, other: &dyn Widget) -> bool {
        other.as_any().downcast_ref::<Focus>().is_some()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        std::slice::from_ref(&self.child)
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        self.child.update_render_object(render_object)
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

// ============================================================================
// FocusElement
// ============================================================================

/// Element for the Focus widget.
///
/// Manages the focus node lifecycle and `on_focus_change` callback:
/// - Sets the callback on the focus node during mount
/// - Clears the callback during unmount
/// - Handles autofocus on mount
/// - Re-reads the callback during rebuild if the widget changed
pub struct FocusElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl FocusElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
        }
    }

    fn get_focus_widget(&self) -> Option<&Focus> {
        self.widget.as_ref()?.as_any().downcast_ref::<Focus>()
    }

    fn mount_children(&mut self, context: &mut ElementContext) {
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

    fn reconcile_children(&mut self, context: &mut ElementContext) {
        if let Some(widget) = &self.widget {
            let new_child_widgets: Vec<Box<dyn Widget>> = widget.children()
                .iter()
                .map(|c| c.clone_boxed())
                .collect();
            let old_children = context.children().to_vec();
            let old_len = old_children.len();
            let new_len = new_child_widgets.len();

            for (i, new_child_widget) in new_child_widgets.into_iter().enumerate() {
                if i < old_len {
                    context.update_child(old_children[i], new_child_widget);
                } else {
                    context.inflate_child(Some(i), new_child_widget);
                }
            }
            for i in (new_len..old_len).rev() {
                context.unmount_child(old_children[i]);
            }
        }
    }

    fn sync_callback(&self, context: &mut ElementContext) {
        if let Some(attachment) = &self.focus_attachment {
            if let Some(focus) = self.get_focus_widget() {
                if let Some(ref callback) = focus.on_focus_change {
                    attachment.set_on_focus_change(callback.clone(), context.focus_manager());
                } else {
                    attachment.clear_on_focus_change(context.focus_manager());
                }
            }
        }
    }
}

impl Default for FocusElement {
    fn default() -> Self { Self::new() }
}

impl RenderObjectElement for FocusElement {
    fn widget(&self) -> Option<&dyn Widget> { self.widget.as_deref() }
    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Some(focus) = widget.as_any().downcast_ref::<Focus>() {
            self.key = focus.key();
        }
        self.widget = Some(widget);
    }
    fn render_object_id(&self) -> Option<RenderObjectKey> { self.render_object }
    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) { self.render_object = id; }
    fn stored_key(&self) -> Option<WidgetKey> { self.key.clone() }
    fn set_stored_key(&mut self, key: Option<WidgetKey>) { self.key = key; }
    fn element_id(&self) -> Option<ElementKey> { self.id }
    fn set_element_id(&mut self, id: Option<ElementKey>) { self.id = id; }
}

impl Element for FocusElement {
    fn mount(&mut self, context: &mut ElementContext) {
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context.focus_manager().create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        // Set on_focus_change callback on the focus node
        self.sync_callback(context);

        self.mount_render_object(context);

        // Handle autofocus
        if let Some(focus) = self.get_focus_widget() {
            if focus.autofocus {
                if let Some(node_id) = node_id {
                    context.focus_manager().request_focus(node_id);
                }
            }
        }

        self.mount_children(context);
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        self.unmount_render_object(context);

        // Clear the on_focus_change callback before detaching
        if let Some(attachment) = &self.focus_attachment {
            attachment.clear_on_focus_change(context.focus_manager());
        }

        if let Some(mut attachment) = self.focus_attachment.take() {
            attachment.detach(context.focus_manager());
        }
    }

    fn render_object(&self) -> Option<RenderObjectKey> { self.render_object }

    fn widget_key(&self) -> Option<WidgetKey> { self.key.clone() }

    fn can_update(&self, widget: &dyn Any) -> bool {
        widget.downcast_ref::<Focus>().is_some()
    }

    fn on_event(
        &mut self,
        _event: &InputEvent,
        _context: &mut EventContext,
        _state: &mut StateStorage,
    ) -> Option<Box<dyn Any>> {
        None
    }

    fn rebuild(
        &mut self,
        new_widget: Box<dyn Any>,
        context: &mut ElementContext,
    ) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            if let Some(focus) = widget.as_any().downcast_ref::<Focus>() {
                self.key = focus.key();
            }
            self.widget = Some(*widget);

            // Update the on_focus_change callback if the widget changed
            self.sync_callback(context);

            // Update the render object with new properties
            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self.widget.as_ref().unwrap().update_render_object(ro.as_mut());
                    if result.contains(UpdateResult::LAYOUT) {
                        context.mark_needs_layout(ro_id);
                    }
                    if result.contains(UpdateResult::PAINT) {
                        context.mark_needs_paint(ro_id);
                    }
                }
            }

            self.reconcile_children(context);
        }

        // Reparent focus node if parent changed
        if let Some(attachment) = self.focus_attachment.as_ref() {
            let new_parent_id = context.parent_focus_node_id();
            attachment.reparent_to(new_parent_id, context.focus_manager());
        }
    }

    fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
    }

    fn focus_attachment(&self) -> &Option<FocusAttachment> { &self.focus_attachment }
    fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment> { &mut self.focus_attachment }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[test]
    fn test_focus_new() {
        let focus = Focus::new(Text::new("Hello"));
        assert!(!focus.autofocus);
        assert!(focus.on_focus_change.is_none());
        assert!(focus.child().is_some());
    }

    #[test]
    fn test_focus_autofocus() {
        let focus = Focus::new(Text::new("Hello")).autofocus(true);
        assert!(focus.autofocus);
    }

    #[test]
    fn test_focus_on_focus_change() {
        let focus = Focus::new(Text::new("Hello"))
            .on_focus_change(|_focused| {});
        assert!(focus.on_focus_change.is_some());
    }

    #[test]
    fn test_focus_key_delegates_to_child() {
        let focus = Focus::new(
            Text::new("Hello").with_key("my-key")
        );
        assert!(focus.key().is_some());
    }

    #[test]
    fn test_focus_child_returns_child() {
        let focus = Focus::new(Text::new("Hello"));
        let child = focus.child().unwrap();
        assert!(child.as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn test_focus_can_update_same_type() {
        let f1 = Focus::new(Text::new("Hello"));
        let f2 = Focus::new(Text::new("World"));
        assert!(f1.can_update(&f2));
    }

    #[test]
    fn test_focus_clone() {
        let focus = Focus::new(Text::new("Hello")).autofocus(true);
        let cloned = focus.clone();
        assert!(cloned.autofocus);
        assert!(cloned.child().is_some());
    }

    #[test]
    fn test_focus_children_returns_child_as_slice() {
        let focus = Focus::new(Text::new("Hello"));
        let children = focus.children();
        assert_eq!(children.len(), 1);
        assert!(children[0].as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn test_focus_element_can_update_same_type() {
        let elem = FocusElement::new();
        let focus_widget = Focus::new(Text::new("Hello"));
        assert!(elem.can_update(focus_widget.as_any()));
    }
}
