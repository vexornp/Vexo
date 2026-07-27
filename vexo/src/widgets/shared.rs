//! `Shared` — a proxy widget that wraps `Rc<dyn Widget>` for cheap sharing.
//!
//! When a parent component rebuilds frequently (e.g., every keyboard animation
//! frame) but its child subtree hasn't changed, `Shared` avoids the expensive
//! deep-clone + reconciliation cascade by sharing the child via `Rc`. The
//! `SharedElement` compares `Rc` pointers on `update()`/`rebuild()` and skips
//! `update_child()` entirely when the pointer hasn't changed — turning an
//! O(n) subtree reconciliation into O(1).

use std::any::Any;
use std::rc::Rc;

use crate::element::Element;
use crate::element_context::ElementContext;
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::id::{ElementKey, RenderObjectKey};
use crate::key::WidgetKey;
use crate::render_object::RenderObject;
use crate::stateful_widget::ProxyRenderObject;
use crate::update_result::UpdateResult;
use crate::widgets::Widget;

/// Proxy widget wrapping `Rc<dyn Widget>`. `clone_boxed()` is O(1) (Rc clone).
///
/// This is a **crate-internal** primitive used by `Memo` and the built-in
/// `MediaQuery` family. App authors should use `Memo<T>` (the public API for
/// level-2 rebuild skipping) instead of `Shared` directly — `Memo` handles
/// `Rc` caching automatically, while bare `Shared` requires the caller to
/// manually cache the `Rc` across renders (easy to get wrong: a fresh
/// `Rc::new()` per render silently defeats the optimization).
#[derive(Clone)]
pub(crate) struct Shared {
    child: Rc<dyn Widget>,
}

impl Shared {
    pub(crate) fn new(child: Rc<dyn Widget>) -> Self {
        Self { child }
    }
}

impl Widget for Shared {
    fn key(&self) -> Option<WidgetKey> {
        None
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = SharedElement::new();
        elem.set_stored_key(None);
        elem.set_widget(Box::new(self.clone()));
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ProxyRenderObject::new())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update_render_object(&self, _ro: &mut dyn RenderObject) -> UpdateResult {
        UpdateResult::NONE
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }
}

pub(crate) struct SharedElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Shared>,
    focus_attachment: Option<FocusAttachment>,
    /// Pointer to the child widget from the last `update()`/`mount()`.
    /// Used to skip `update_child()` when the `Rc` pointer hasn't changed.
    child_ptr: Option<*const dyn Widget>,
}

impl SharedElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
            child_ptr: None,
        }
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref().map(|w| w.child.as_ref())
    }
}

impl Default for SharedElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for SharedElement {
    fn widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref().map(|w| w as &dyn Widget)
    }

    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Some(w) = widget.as_any().downcast_ref::<Shared>() {
            self.widget = Some(w.clone());
        }
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

impl Element for SharedElement {
    fn mount(&mut self, context: &mut ElementContext) {
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context
            .focus_manager()
            .create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        self.mount_render_object(context);

        // Extract child widget info without holding a borrow of self.
        let child_info = self.widget.as_ref().map(|w| {
            let child: &dyn Widget = w.child.as_ref();
            let ptr = child as *const dyn Widget;
            let boxed = child.clone_boxed();
            (ptr, boxed)
        });
        if let Some((ptr, boxed)) = child_info {
            self.child_ptr = Some(ptr);
            context.inflate_child(None, boxed);
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast and store the new widget.
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            if let Some(w) = widget.as_any().downcast_ref::<Shared>() {
                self.widget = Some(w.clone());
            }
        }

        // Update the render object (pass-through proxy — always NONE).
        if let Some(ro_id) = self.render_object {
            if let Some(ro) = context.get_render_object_mut(ro_id) {
                let _ = self
                    .widget
                    .as_ref()
                    .unwrap()
                    .update_render_object(ro.as_mut());
            }
        }

        // Compare child pointers — skip update_child if the Rc hasn't changed.
        let new_child_ptr = self.get_child_widget().map(|w| w as *const dyn Widget);
        if new_child_ptr != self.child_ptr {
            let old_child = context.children().first().copied();
            let child_boxed = self.get_child_widget().map(|w| w.clone_boxed());
            self.child_ptr = new_child_ptr;
            if let Some(child_widget) = child_boxed {
                match old_child {
                    Some(old_child_key) => {
                        context.update_child(old_child_key, child_widget);
                    }
                    None => {
                        context.inflate_child(None, child_widget);
                    }
                }
            } else if let Some(old_child_key) = old_child {
                context.unmount_child(old_child_key);
            }
        }

        if let Some(attachment) = self.focus_attachment.as_ref() {
            let new_parent_id = context.parent_focus_node_id();
            attachment.reparent_to(new_parent_id, context.focus_manager());
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        self.unmount_render_object(context);
        if let Some(old_child_key) = context.children().first().copied() {
            context.unmount_child(old_child_key);
        }
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

    fn can_update(&self, widget: &dyn Any) -> bool {
        widget.downcast_ref::<Shared>().is_some()
    }

    fn on_event(
        &mut self,
        _event: &crate::input::InputEvent,
        _context: &mut crate::EventContext,
        _state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update(new_widget, context);
    }

    fn child_mounted(
        &mut self,
        _slot: Option<usize>,
        child_ro: Option<RenderObjectKey>,
        context: &mut ElementContext,
    ) {
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
