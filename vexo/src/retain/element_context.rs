use std::sync::mpsc;

use crate::retain::build_owner::BuildOwner;
use crate::retain::child_ops::ChildOps;
use crate::retain::dirty::DirtyTracking;
use crate::retain::id::{ElementKey, RenderObjectKey};
use crate::retain::render_object::RenderObjectRegistry;
use crate::retain::state::StateStorage;

/// Context passed to element lifecycle methods.
///
/// Elements use `child_ops` to request child tree operations instead of
/// directly accessing the ElementRegistry. The pipeline executes the
/// operations after the element method returns.
pub struct ElementContext<'a> {
    pub element_id: ElementKey,
    pub parent: Option<ElementKey>,
    pub render_object: Option<RenderObjectKey>,
    pub state: &'a mut StateStorage,
    pub dirty: &'a mut DirtyTracking,
    pub render_objects: &'a mut RenderObjectRegistry,
    pub build_owner: &'a BuildOwner,
    pub dirty_sender: &'a mpsc::Sender<ElementKey>,
    pub child_ops: &'a mut ChildOps,
}

impl<'a> ElementContext<'a> {
    pub fn new(
        element_id: ElementKey,
        parent: Option<ElementKey>,
        render_object: Option<RenderObjectKey>,
        state: &'a mut StateStorage,
        dirty: &'a mut DirtyTracking,
        render_objects: &'a mut RenderObjectRegistry,
        build_owner: &'a BuildOwner,
        dirty_sender: &'a mpsc::Sender<ElementKey>,
        child_ops: &'a mut ChildOps,
    ) -> Self {
        Self {
            element_id,
            parent,
            render_object,
            state,
            dirty,
            render_objects,
            build_owner,
            dirty_sender,
            child_ops,
        }
    }

    /// Request inflation of a new child element.
    pub fn inflate_child(&mut self, slot: Option<usize>, widget: Box<dyn crate::retain::widgets::Widget>) {
        self.child_ops.inflate(slot, widget, self.element_id);
    }

    /// Request update of an existing child element.
    pub fn update_child(&mut self, child: ElementKey, widget: Box<dyn crate::retain::widgets::Widget>) {
        self.child_ops.update(child, widget);
    }

    /// Request unmount of a child element.
    pub fn unmount_child(&mut self, child: ElementKey) {
        self.child_ops.unmount(child);
    }

    /// Mark this element as needing rebuild.
    pub fn mark_dirty(&mut self) {
        let _ = self.dirty_sender.send(self.element_id);
    }

    /// Get or create state for this element.
    pub fn get_or_create_state<S: 'static + Clone + Send>(&mut self, initial: S) -> S {
        if let Some(existing) = self.state.get::<S>(self.element_id) {
            existing.clone()
        } else {
            self.state.insert(self.element_id, initial.clone());
            initial
        }
    }

    /// Remove render object associated with this element.
    pub fn remove_render_object(&mut self, render_object_id: RenderObjectKey) {
        self.render_objects.remove(render_object_id);
    }
}
