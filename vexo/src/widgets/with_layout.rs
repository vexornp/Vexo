//! WithLayout widget - a single-child wrapper that applies layout properties.
//!
//! This widget is purely for layout -- no visual decoration, no painting.
//! It applies a `Layout` to its child and creates a Taffy layout node.
//!
//! This is the Vexo equivalent of inline styles on a child element in CSS.

use std::any::Any;

use crate::core::{Bounds, Logical, Size};
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::layout::{AlignItems, FlexDirection, Layout};
use crate::render_objects::ContainerRenderObject;
use crate::{
    Element, ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectKey, UpdateResult, Widget, WidgetKey,
};

// ============================================================================
// WithLayoutElement
// ============================================================================

/// Element for WithLayout widget.
///
/// Manages a single child element and updates the render object
/// when layout properties change.
pub struct WithLayoutElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl WithLayoutElement {
    /// Create a new WithLayout element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
        }
    }

    /// Set the widget for this element.
    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(widget.clone_boxed());
        self.key = widget.key();
    }

    /// Get the child widget from the stored widget.
    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for WithLayoutElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for WithLayoutElement {
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

impl Element for WithLayoutElement {
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

                    if result.contains(UpdateResult::LAYOUT) {
                        context.mark_needs_layout(ro_id);
                    }
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
// WithLayout Widget
// ============================================================================

/// A widget that applies layout properties to a child without visual decoration.
///
/// This is the Vexo equivalent of inline styles on a child element in CSS.
/// Use it to add padding, margin, sizing, flex, or positioning to any widget
/// without introducing visual elements.
///
/// # Example
///
/// ```ignore
/// // Add padding and center a text widget
/// WithLayout::new(
///     Text::new("Hello"),
///     Layout::default()
///         .padding(16.0)
///         .align_self(AlignSelf::Center),
/// )
///
/// // Fixed-size container
/// WithLayout::new(
///     Text::new("Fixed"),
///     Layout::fixed(200.0, 100.0),
/// )
/// ```
pub struct WithLayout {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    layout: Layout,
}

impl WithLayout {
    /// Create a new WithLayout wrapper with a child and layout properties.
    ///
    /// Defaults to `FlexDirection::Column` with `AlignItems::Stretch` so the
    /// wrapper acts as a transparent passthrough for width constraints: the
    /// child stretches to fill the wrapper's cross-axis (width) size. This
    /// ensures that when `WithLayout` is used to apply `flex_grow`/`padding`
    /// etc. to a `Box<dyn Widget>`, definite width constraints propagate
    /// down to the child (enabling text wrapping, for example).
    pub fn new(child: impl Widget + 'static, layout: Layout) -> Self {
        let layout = Layout {
            flex_direction: Some(layout.flex_direction.unwrap_or(FlexDirection::Column)),
            align_items: Some(layout.align_items.unwrap_or(AlignItems::Stretch)),
            ..layout
        };
        Self {
            key: None,
            child: Box::new(child),
            layout,
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the layout.
    pub fn layout_ref(&self) -> &Layout {
        &self.layout
    }
}

impl Clone for WithLayout {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for WithLayout {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = WithLayoutElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new(self.layout.clone()))
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
            .downcast_mut::<ContainerRenderObject>()
        {
            if ro.set_layout(self.layout.clone()) {
                UpdateResult::LAYOUT
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
    use crate::layout::{AlignItems, AlignSelf, FlexDirection};
    use crate::{GlobalKey, Key, Text};

    #[test]
    fn test_with_layout_creation() {
        let w = WithLayout::new(Text::new("Hello"), Layout::default());
        assert!(w.key().is_none());
    }

    #[test]
    fn test_with_layout_with_key() {
        let w = WithLayout::new(Text::new("Hello"), Layout::default()).with_key("my-layout");
        assert_eq!(w.key(), Some(WidgetKey::Local(Key::new("my-layout"))));
    }

    #[test]
    fn test_with_layout_with_global_key() {
        let global_key = GlobalKey::new();
        let w = WithLayout::new(Text::new("Hello"), Layout::default()).with_key(global_key.clone());
        assert_eq!(w.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_with_layout_render_object_creation() {
        let layout = Layout::default().padding(10.0).flex_grow(1.0);
        let w = WithLayout::new(Text::new("Hello"), layout);
        let ro = w.create_render_object();
        assert!(ro
            .as_any()
            .downcast_ref::<ContainerRenderObject>()
            .is_some());
    }

    #[test]
    fn test_with_layout_render_object_set_layout() {
        let layout1 = Layout::default().padding(10.0);
        let mut ro = ContainerRenderObject::new(layout1);

        // Same layout = no change
        assert!(!ro.set_layout(Layout::default().padding(10.0)));

        // Different layout = change
        assert!(ro.set_layout(Layout::default().padding(20.0)));
    }

    #[test]
    fn test_with_layout_render_object_paint_empty() {
        let mut ro = ContainerRenderObject::new(Layout::default());
        ro.set_computed_bounds(Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0)));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // No visual output (default style has no decorations)
        assert_eq!(cmds.len(), 0);
    }

    #[test]
    fn test_with_layout_update_render_object() {
        let layout1 = Layout::default().padding(10.0);
        let layout2 = Layout::default().padding(20.0);

        let widget1 = WithLayout::new(Text::new("Hello"), layout1);
        let widget2 = WithLayout::new(Text::new("Hello"), layout2);
        // Use the same Column+Stretch defaults that WithLayout::new applies
        // so the initial render object matches widget1's layout exactly.
        let mut ro = ContainerRenderObject::new(
            Layout::default()
                .flex_direction(FlexDirection::Column)
                .align(AlignItems::Stretch)
                .padding(10.0),
        );

        // Same layout = NONE
        let result = widget1.update_render_object(&mut ro);
        assert_eq!(result, UpdateResult::NONE);

        // Different layout = LAYOUT (framework cascades to paint automatically)
        let result = widget2.update_render_object(&mut ro);
        assert!(result.contains(UpdateResult::LAYOUT));
    }

    #[test]
    fn test_with_layout_child() {
        let w = WithLayout::new(Text::new("Hello"), Layout::default());
        assert!(w.child().is_some());
    }

    #[test]
    fn test_with_layout_clone() {
        let w = WithLayout::new(Text::new("Hello"), Layout::default().padding(10.0));
        let cloned = w.clone();
        assert!(cloned.child().is_some());
    }

    #[test]
    fn test_with_layout_doc_example_compiles() {
        // Mirrors the updated doc example — verifies the explicit
        // constructor form compiles and produces a widget with the
        // expected layout.
        let w = WithLayout::new(
            Text::new("Hello"),
            Layout::default()
                .padding(16.0)
                .align_self(AlignSelf::Center),
        );
        assert!(w.child().is_some());
        assert!(w.layout_ref().padding.is_some());
        assert_eq!(w.layout_ref().align_self, Some(AlignSelf::Center));
    }

    #[test]
    fn test_with_layout_element_default() {
        let element = WithLayoutElement::default();
        assert!(element.element_id().is_none());
        assert!(element.render_object_id().is_none());
    }

    #[test]
    fn test_with_layout_gap_preserves_padding() {
        let w = WithLayout::new(Text::new("Hello"), Layout::default().padding(10.0).gap(4.0));
        assert!(w.layout_ref().padding.is_some());
        assert!(w.layout_ref().gap.is_some());
    }
}
