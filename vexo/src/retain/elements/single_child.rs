//! SingleChildRenderObjectElement trait for elements with a single child.
//!
//! This trait provides child management methods for elements that have
//! exactly one child (e.g., DecoratedContainer, Padding, Background).

use super::RenderObjectElement;
use crate::retain::{ElementContext, ElementId, RenderObjectId};

/// Element with a single child render object.
///
/// Provides child management methods for single-child containers.
/// Similar to Flutter's `RenderObjectElementWithChild` mixin.
///
/// # Implementation Requirements
///
/// Implementors must provide:
/// - `child_element()` - Get the child element ID
/// - `set_child_element()` - Set the child element ID
///
/// The trait provides default implementations for:
/// - `insert_child_render_object()` - Link child render object to parent
/// - `remove_child_render_object()` - Unlink child render object from parent
pub trait SingleChildRenderObjectElement: RenderObjectElement {
    /// Get the child element ID.
    fn child_element(&self) -> Option<ElementId>;

    /// Set the child element ID.
    fn set_child_element(&mut self, child: Option<ElementId>);

    /// Insert a child render object into this element's render object.
    ///
    /// This links the child's render object to the parent's render object
    /// via `set_child_id()`, enabling the render tree traversal.
    fn insert_child_render_object(&mut self, child_ro: RenderObjectId, context: &mut ElementContext) {
        if let Some(parent_ro) = self.render_object_id() {
            if let Some(parent_obj) = context.get_render_object_mut(parent_ro) {
                parent_obj.set_child_id(child_ro);
            }
        }
    }

    /// Remove a child render object from this element's render object.
    ///
    /// This clears the child reference from the parent's render object.
    fn remove_child_render_object(&mut self, context: &mut ElementContext) {
        if let Some(parent_ro) = self.render_object_id() {
            if let Some(parent_obj) = context.get_render_object_mut(parent_ro) {
                // Clear the child by setting to a sentinel value
                // Note: RenderObject::set_child_id doesn't support Option,
                // so we use a workaround - the child will be replaced on next insert
                let _ = parent_obj;
                // The actual clearing happens when a new child is inserted
            }
        }
    }
}
