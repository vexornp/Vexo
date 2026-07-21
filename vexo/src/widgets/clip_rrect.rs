//! ClipRRect widget — clips its child to a rounded rectangle.
//!
//! This widget clips its single child subtree to a rounded rectangle.
//! The clip is applied at paint time via `PushClipRRect`/`PopClipRRect`
//! render commands, which the GPU backend enforces as an SDF mask in
//! the fragment shader.
//!
//! This is the Vexo equivalent of Flutter's `ClipRRect`.
//!
//! # Example
//!
//! ```ignore
//! ClipRRect::new(8.0, DecoratedBox::with_style(
//!     Text::new("Clipped!"),
//!     Style::default().background(Color::RED),
//! ))
//! ```

use std::any::Any;

use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::key::WidgetKey;
use crate::render_objects::ClipRRectRenderObject;
use crate::{
    Element, ElementContext, ElementKey, EventContext, RenderObject, RenderObjectKey, UpdateResult,
    Widget,
};

// ============================================================================
// ClipRRectElement
// ============================================================================

pub struct ClipRRectElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl ClipRRectElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
        }
    }

    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(widget.clone_boxed());
        self.key = widget.key();
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for ClipRRectElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for ClipRRectElement {
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

impl Element for ClipRRectElement {
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

        if let Some(widget) = &self.widget {
            if let Some(child_widget) = widget.child() {
                context.inflate_child(None, child_widget.clone_boxed());
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        self.unmount_render_object(context);
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
        self.widget
            .as_ref()
            .map(|old| old.as_any().type_id() == widget.type_id())
            .unwrap_or(false)
    }

    fn on_event(
        &mut self,
        _event: &InputEvent,
        _context: &mut EventContext,
        _state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.widget = Some(*widget);

            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self
                        .widget
                        .as_ref()
                        .unwrap()
                        .update_render_object(ro.as_mut());

                    if result.contains(UpdateResult::PAINT) {
                        context.mark_needs_paint(ro_id);
                    }
                }
            }

            let old_child = context.children().first().copied();
            if let Some(child_widget) = self.get_child_widget() {
                match old_child {
                    Some(old_child_key) => {
                        context.update_child(old_child_key, child_widget.clone_boxed());
                    }
                    None => {
                        context.inflate_child(None, child_widget.clone_boxed());
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

// ============================================================================
// ClipRRect Widget
// ============================================================================

/// A widget that clips its child to a rounded rectangle.
///
/// The clip is applied at paint time via the GPU fragment shader (SDF
/// mask). Layout is pass-through — the child sizes itself naturally.
///
/// # Example
///
/// ```ignore
/// ClipRRect::new(8.0, DecoratedBox::with_style(
///     Text::new("Clipped!"),
///     Style::default().background(Color::RED),
/// ))
/// ```
pub struct ClipRRect {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    radius: f32,
}

impl ClipRRect {
    /// Create a new ClipRRect with the given corner radius and child.
    ///
    /// A radius of 0.0 means "rectangular clip" (degenerates to the
    /// existing PushClip path). Negative radius is clamped to 0.0.
    pub fn new(radius: f32, child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            radius: radius.max(0.0),
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }

    /// Get the corner radius.
    pub fn radius(&self) -> f32 {
        self.radius
    }
}

impl Clone for ClipRRect {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            radius: self.radius,
        }
    }
}

impl Widget for ClipRRect {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ClipRRectElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ClipRRectRenderObject::new(self.radius))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(ro) = render_object
            .as_any_mut()
            .downcast_mut::<ClipRRectRenderObject>()
        {
            if ro.set_radius(self.radius) {
                UpdateResult::PAINT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GlobalKey, Key, Text};

    #[test]
    fn test_clip_rrect_creation() {
        let w = ClipRRect::new(8.0, Text::new("Hi"));
        assert!(w.key().is_none());
        assert_eq!(w.radius(), 8.0);
    }

    #[test]
    fn test_clip_rrect_with_key_local() {
        let w = ClipRRect::new(8.0, Text::new("Hi")).with_key("my-clip");
        assert_eq!(w.key(), Some(WidgetKey::Local(Key::new("my-clip"))));
    }

    #[test]
    fn test_clip_rrect_with_key_global() {
        let gk = GlobalKey::new();
        let w = ClipRRect::new(8.0, Text::new("Hi")).with_key(gk.clone());
        assert_eq!(w.key(), Some(WidgetKey::Global(gk)));
    }

    #[test]
    fn test_clip_rrect_negative_radius_clamped() {
        let w = ClipRRect::new(-5.0, Text::new("Hi"));
        assert_eq!(w.radius(), 0.0);
    }

    #[test]
    fn test_clip_rrect_clone_preserves_fields() {
        let w = ClipRRect::new(12.0, Text::new("Hi")).with_key("clipped");
        let cloned = w.clone();
        assert_eq!(cloned.key(), w.key());
        assert_eq!(cloned.radius(), w.radius());
    }

    #[test]
    fn test_clip_rrect_render_object_is_pass_through() {
        let w = ClipRRect::new(8.0, Text::new("Hi"));
        let ro = w.create_render_object();
        assert!(ro.is_pass_through());
    }

    #[test]
    fn test_clip_rrect_update_render_object_paint_only() {
        let w1 = ClipRRect::new(8.0, Text::new("Hi"));
        let mut ro = w1.create_render_object();
        assert_eq!(w1.update_render_object(ro.as_mut()), UpdateResult::NONE);

        let w2 = ClipRRect::new(12.0, Text::new("Hi"));
        let result = w2.update_render_object(ro.as_mut());
        assert!(result.contains(UpdateResult::PAINT));
        assert!(!result.contains(UpdateResult::LAYOUT));
    }

    #[test]
    fn test_clip_rrect_can_update_same_type() {
        let w1 = ClipRRect::new(8.0, Text::new("Hi"));
        let w2 = ClipRRect::new(12.0, Text::new("Hi"));
        let mut elem = ClipRRectElement::new();
        elem.set_widget(&w1);
        assert!(elem.can_update(w2.as_any()));
    }
}
