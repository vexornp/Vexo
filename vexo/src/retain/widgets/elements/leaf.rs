//! Leaf element implementation.
//!
//! LeafElement is the simplest element with no children.
//! Used by leaf widgets like Text, Image, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, Key, RenderObjectId};

/// A leaf element with no children.
///
/// This is a minimal element implementation for leaf widgets.
/// It holds no state and has no children.
pub struct LeafElement {
    render_object: Option<RenderObjectId>,
    key: Option<Key>,
}

impl LeafElement {
    /// Create a new leaf element.
    pub fn new() -> Self {
        Self {
            render_object: None,
            key: None,
        }
    }
}

impl Default for LeafElement {
    fn default() -> Self {
        Self::new()
    }
}

impl Element for LeafElement {
    fn mount(&mut self, _context: &mut ElementContext) {
        // Leaf elements don't need special mount logic
    }

    fn update(&mut self, _context: &mut ElementContext) {
        // Leaf elements don't need special update logic
    }

    fn unmount(&mut self, _context: &mut ElementContext) {
        // Leaf elements don't need special unmount logic
    }

    fn visit_children(&self, _visitor: &mut dyn FnMut(&dyn Element)) {
        // Leaf elements have no children
    }

    fn render_object(&self) -> Option<RenderObjectId> {
        self.render_object
    }

    fn widget_key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn can_update(&self, _widget: &dyn Any) -> bool {
        // Default: allow updates
        true
    }
}