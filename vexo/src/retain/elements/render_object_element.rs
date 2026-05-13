//! RenderObjectElement trait for elements that own render objects.
//!
//! This trait provides default implementations for render object lifecycle:
//! - Creation in mount()
//! - Update in update()
//! - Removal in unmount()
//!
//! Elements that own render objects should implement this trait to eliminate
//! code duplication and follow Flutter's architecture.

use std::any::Any;

use crate::retain::{Element, ElementContext, ElementId, RenderObjectKey, Widget, UpdateResult};
use crate::retain::key::WidgetKey;

/// Element that owns and manages a RenderObject.
///
/// Provides default implementations for render object lifecycle:
/// - Creation in mount()
/// - Update in update()
/// - Removal in unmount()
///
/// # Implementation Requirements
///
/// Implementors must provide:
/// - `widget()` - Get the widget configuration
/// - `set_widget()` - Store a new widget
/// - `render_object_id()` - Get the owned render object ID
/// - `set_render_object_id()` - Store the render object ID
/// - `stored_key()` - Get the widget key
/// - `set_stored_key()` - Store the widget key
/// - `element_id()` - Get the element ID
///
/// # Example
///
/// ```ignore
/// pub struct MyLeafElement {
///     id: Option<ElementId>,
///     key: Option<WidgetKey>,
///     render_object: Option<RenderObjectKey>,
///     widget: Option<Box<dyn Widget>>,
/// }
///
/// impl RenderObjectElement for MyLeafElement {
///     fn widget(&self) -> Option<&dyn Widget> {
///         self.widget.as_deref()
///     }
///
///     fn set_widget(&mut self, widget: Box<dyn Widget>) {
///         self.widget = Some(widget);
///     }
///
///     fn render_object_id(&self) -> Option<RenderObjectKey> {
///         self.render_object
///     }
///
///     fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) {
///         self.render_object = id;
///     }
///
///     fn stored_key(&self) -> Option<WidgetKey> {
///         self.key.clone()
///     }
///
///     fn set_stored_key(&mut self, key: Option<WidgetKey>) {
///         self.key = key;
///     }
///
///     fn element_id(&self) -> Option<ElementId> {
///         self.id
///     }
/// }
/// ```
pub trait RenderObjectElement: Element {
    /// Get the widget as a reference for render object operations.
    fn widget(&self) -> Option<&dyn Widget>;

    /// Store the widget after update.
    fn set_widget(&mut self, widget: Box<dyn Widget>);

    /// Get the stored render object ID.
    fn render_object_id(&self) -> Option<RenderObjectKey>;

    /// Set the render object ID after creation.
    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>);

    /// Get the stored key.
    fn stored_key(&self) -> Option<WidgetKey>;

    /// Set the key.
    fn set_stored_key(&mut self, key: Option<WidgetKey>);

    /// Get the element ID.
    fn element_id(&self) -> Option<ElementId>;

    /// Set the element ID.
    fn set_element_id(&mut self, id: Option<ElementId>);

    /// Default mount implementation for render object creation.
    ///
    /// This method:
    /// 1. Stores the element ID from context
    /// 2. Registers global key if present
    /// 3. Creates the render object from the widget
    /// 4. Marks the render object as needing layout and paint
    ///
    /// Elements should call this in their `mount()` implementation.
    fn mount_render_object(&mut self, context: &mut ElementContext) {
        // Store element ID from context
        self.set_element_id(Some(context.element_id));

        // Register global key if present
        if let Some(WidgetKey::Global(key)) = &self.stored_key() {
            let _ = context.register_global_key(key.clone(), context.element_id);
        }

        // Create render object from widget
        if let Some(widget) = self.widget() {
            let render_obj = widget.create_render_object();
            if let Some(ro_id) = context.create_render_object(render_obj, context.element_id) {
                self.set_render_object_id(Some(ro_id));
                context.render_object = Some(ro_id);
                context.mark_needs_layout(ro_id);
                context.mark_needs_paint(ro_id);
            }
        }
    }

    /// Default update implementation for render object updates.
    ///
    /// This method:
    /// 1. Downcasts the new widget and stores it
    /// 2. Updates the render object with new properties
    /// 3. Marks dirty based on UpdateResult
    ///
    /// Elements should call this in their `update()` implementation.
    fn update_render_object(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast and store the widget
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.set_widget(*widget);

            // Update the render object with new properties
            if let Some(ro_id) = self.render_object_id() {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    if let Some(widget) = self.widget() {
                        let result = widget.update_render_object(ro.as_mut());

                        // Only mark dirty based on what actually changed
                        if result.contains(UpdateResult::LAYOUT) {
                            context.mark_needs_layout(ro_id);
                        }
                        if result.contains(UpdateResult::PAINT) {
                            context.mark_needs_paint(ro_id);
                        }
                    }
                }
            }
        }
    }

    /// Default unmount implementation for render object removal.
    ///
    /// This method:
    /// 1. Unregisters global key if present
    /// 2. Removes the render object from the registry
    /// 3. Removes element state
    ///
    /// Elements should call this in their `unmount()` implementation.
    fn unmount_render_object(&mut self, context: &mut ElementContext) {
        // Unregister global key if present
        if let Some(WidgetKey::Global(_)) = &self.stored_key() {
            if let Some(id) = self.element_id() {
                context.unregister_global_key(id);
            }
        }

        // Remove render object from registry
        if let Some(ro) = self.render_object_id() {
            context.remove_render_object(ro);
            context.dirty.mark_needs_paint(ro);
        }

        // Remove element state
        if let Some(id) = self.element_id() {
            context.remove_state(id);
        }
    }
}
