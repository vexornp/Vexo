//! MultiChildRenderObjectElement trait for elements with multiple children.
//!
//! This trait provides child management methods for elements that have
//! multiple children (e.g., Column, Row, Stack).

use super::RenderObjectElement;
use crate::retain::{ElementContext, ElementId, RenderObjectKey};

/// Element with multiple child render objects.
///
/// Provides child management methods for multi-child containers.
/// Similar to Flutter's `MultiChildRenderObjectElement` class.
///
/// # Implementation Requirements
///
/// Implementors must provide:
/// - `child_elements()` - Get the child element IDs
/// - `set_child_elements()` - Set the child element IDs
/// - `add_child_element()` - Add a child element ID
///
/// The trait provides default implementations for:
/// - `insert_child_render_object()` - Link child render object to parent
/// - `remove_child_render_object()` - Unlink child render object from parent
/// - `clear_child_render_objects()` - Clear all children from parent
pub trait MultiChildRenderObjectElement: RenderObjectElement {
    /// Get the child element IDs.
    fn child_elements(&self) -> &[ElementId];

    /// Set the child element IDs.
    fn set_child_elements(&mut self, children: Vec<ElementId>);

    /// Add a child element ID.
    fn add_child_element(&mut self, child: ElementId);

    /// Insert a child render object into this element's render object.
    ///
    /// This links the child's render object to the parent's render object
    /// via `add_child()`, enabling the render tree traversal.
    fn insert_child_render_object(&mut self, child_ro: RenderObjectKey, context: &mut ElementContext) {
        if let Some(parent_ro) = self.render_object_id() {
            if let Some(parent_obj) = context.get_render_object_mut(parent_ro) {
                parent_obj.add_child(child_ro);
            }
        }
    }

    /// Clear all child render objects from this element's render object.
    ///
    /// This removes all children from the parent's render object.
    fn clear_child_render_objects(&mut self, context: &mut ElementContext) {
        if let Some(parent_ro) = self.render_object_id() {
            if let Some(parent_obj) = context.get_render_object_mut(parent_ro) {
                parent_obj.clear_children();
            }
        }
    }
}
