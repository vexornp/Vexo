//! Container element implementation.
//!
//! ContainerElement is an element with children.
//! Used by container widgets like Column, Row, etc.

use std::any::Any;

use crate::retain::{Element, ElementContext, Key, RenderObjectId};

/// A container element with children.
///
/// This is an element implementation for container widgets.
/// It holds references to child elements.
pub struct ContainerElement {
    render_object: Option<RenderObjectId>,
    key: Option<Key>,
}

impl ContainerElement {
    /// Create a new container element.
    pub fn new() -> Self {
        Self {
            render_object: None,
            key: None,
        }
    }
}

impl Default for ContainerElement {
    fn default() -> Self {
        Self::new()
    }
}

impl Element for ContainerElement {
    fn mount(&mut self, _context: &mut ElementContext) {
        // Container elements don't need special mount logic yet
    }

    fn update(&mut self, _context: &mut ElementContext) {
        // Container elements don't need special update logic yet
    }

    fn unmount(&mut self, _context: &mut ElementContext) {
        // Container elements don't need special unmount logic yet
    }

    fn visit_children(&self, _visitor: &mut dyn FnMut(&dyn Element)) {
        // Container elements will visit children when implemented
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
