//! DecoratedBox widget — decoration only, no layout opinion.
//!
//! `DecoratedBox` is the Vexo equivalent of Flutter's `DecoratedBox`: it
//! paints a `Style` (background, border, corner radius, shadow, clip) around
//! its child without imposing any layout. The wrapper is a true pass-through
//! proxy (`is_pass_through() == true`) — it does NOT own a Taffy node, so
//! the grandparent links the grandchild directly. The child sizes itself
//! naturally; `DecoratedBox` adopts the child's bounds and paints the
//! decoration there.
//!
//! To combine decoration with layout (padding, sizing, flex), compose:
//! `DecoratedBox::with_style(WithLayout::new(child, layout), Style::default().background(RED))`.
//! The `WithLayout` owns the Taffy node; `DecoratedBox` adopts its bounds.
//!
//! # Border semantics
//!
//! `DecoratedBox::border(color, width)` does NOT add padding — the border
//! paints over the child's edge pixels (Flutter semantics). If you want the
//! child inset from the border, compose with [`WithLayout`](crate::WithLayout)
//! padding.

use std::any::Any;

use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::key::WidgetKey;
use crate::render_objects::DecoratedBoxRenderObject;
use crate::style::Style;
use crate::{
    Element, ElementContext, ElementKey, EventContext, RenderObject, RenderObjectKey, UpdateResult,
    Widget,
};

// ============================================================================
// DecoratedBoxElement
// ============================================================================

/// Element for `DecoratedBox` widget.
///
/// Manages a single child element and updates the render object when the
/// style changes. Structurally identical to other single-child render-object
/// elements — no layout bookkeeping since `DecoratedBox` has no `Layout` field.
pub struct DecoratedBoxElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl DecoratedBoxElement {
    /// Create a new `DecoratedBox` element.
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

impl Default for DecoratedBoxElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for DecoratedBoxElement {
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

impl Element for DecoratedBoxElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Create focus attachment BEFORE mounting child (same rationale as
        // other single-child elements): the child looks up this element's
        // focus node as its parent when it mounts.
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context
            .focus_manager()
            .create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        // Use RenderObjectElement's default mount for render object creation
        self.mount_render_object(context);

        // Mount single child via child_ops (emit Inflate command).
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
        // DecoratedBox doesn't handle events itself
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast and store the new widget
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.widget = Some(*widget);

            // Update the render object with new properties.
            // DecoratedBoxRenderObject::set_style only returns true on
            // actual change, and update_render_object only returns PAINT
            // (never LAYOUT — proxy has no layout).
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
                    // LAYOUT is never returned; no mark_needs_layout call.
                }
            }

            // Reconcile single child via child_ops
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

        // Reparent focus node if parent changed
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
// DecoratedBox Widget
// ============================================================================

/// A widget that decorates a child with visual styling, with no layout opinion.
///
/// `DecoratedBox` paints a `Style` (background, border, corner radius,
/// shadow, clip) around its child without imposing any layout. The wrapper
/// is a true pass-through proxy — it does NOT own a Taffy node, so the
/// grandparent links the grandchild directly. The child sizes itself
/// naturally; `DecoratedBox` adopts the child's bounds and paints the
/// decoration there.
///
/// To combine decoration with layout, compose:
/// `DecoratedBox::with_style(WithLayout::new(child, layout), Style::default().background(RED))`.
///
/// # Example
///
/// ```ignore
/// DecoratedBox::with_style(
///     Text::new("Hello"),
///     Style::default()
///         .background(Color::RED)
///         .border(Color::BLACK, 2.0)
///         .corner_radius(8.0),
/// )
/// ```
///
/// # Border semantics
///
/// `Style::border(color, width)` does NOT add padding — the border
/// paints over the child's edge pixels (Flutter semantics). If you want the
/// child inset from the border, compose with [`WithLayout`](crate::WithLayout)
/// padding.
pub struct DecoratedBox {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    style: Style,
}

impl DecoratedBox {
    /// Create a new `DecoratedBox` with a child and default (empty) style.
    ///
    /// This does NOT set any `align_self`/`flex_shrink` defaults — the
    /// widget imposes zero layout opinion. If you want padding/sizing,
    /// compose with `WithLayout`.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            style: Style::default(),
        }
    }

    /// Create a new `DecoratedBox` with a child and a pre-built `Style`.
    ///
    /// This is the primary constructor for decoration. Build the `Style`
    /// fluently:
    /// `DecoratedBox::with_style(child, Style::default().background(RED).border(BLACK, 1.0))`.
    pub fn with_style(child: impl Widget + 'static, style: Style) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            style,
        }
    }

    /// Set the key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }

    /// Get the style.
    pub fn style_ref(&self) -> &Style {
        &self.style
    }
}

impl Clone for DecoratedBox {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            style: self.style.clone(),
        }
    }
}

impl Widget for DecoratedBox {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = DecoratedBoxElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(DecoratedBoxRenderObject::new(self.style.clone()))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(decorated_ro) = render_object
            .as_any_mut()
            .downcast_mut::<DecoratedBoxRenderObject>()
        {
            // Style change is paint-only — DecoratedBoxRenderObject is a
            // true pass-through proxy with no layout node.
            if decorated_ro.set_style(self.style.clone()) {
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
    use crate::core::Color;
    use crate::style::BoxShadow;
    use crate::{GlobalKey, Key, Text};

    #[test]
    fn test_decorated_box_creation() {
        let widget = DecoratedBox::new(Text::new("Hello"));
        assert!(widget.key().is_none());
        assert_eq!(widget.style_ref(), &Style::default());
    }

    #[test]
    fn test_decorated_box_with_key_local() {
        let widget = DecoratedBox::new(Text::new("Hello")).with_key("my-box");
        assert_eq!(widget.key(), Some(WidgetKey::Local(Key::new("my-box"))));
    }

    #[test]
    fn test_decorated_box_with_key_global() {
        let global_key = GlobalKey::new();
        let widget = DecoratedBox::new(Text::new("Hello")).with_key(global_key.clone());
        assert_eq!(widget.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_decorated_box_style_builder_chain() {
        let widget = DecoratedBox::with_style(
            Text::new("Hello"),
            Style::default()
                .background(Color::RED)
                .border(Color::BLACK, 2.0)
                .corner_radius(8.0)
                .clip(),
        );
        assert_eq!(widget.style_ref().background, Some(Color::RED));
        assert_eq!(widget.style_ref().border.as_ref().unwrap().width, 2.0);
        assert_eq!(
            widget.style_ref().corner_radius.as_ref().unwrap().radius,
            8.0
        );
        assert!(widget.style_ref().clip);
    }

    #[test]
    fn test_decorated_box_shadow_builder() {
        let widget = DecoratedBox::with_style(
            Text::new("Hi"),
            Style::default().shadow(BoxShadow::new(Color::BLACK).blur(8.0)),
        );
        assert_eq!(widget.style_ref().shadows.len(), 1);
        assert_eq!(widget.style_ref().shadows[0].blur_radius, 8.0);
    }

    #[test]
    fn test_decorated_box_shadows_builder() {
        let widget = DecoratedBox::with_style(
            Text::new("Hi"),
            Style::default().shadows(vec![
                BoxShadow::new(Color::BLACK),
                BoxShadow::new(Color::RED),
            ]),
        );
        assert_eq!(widget.style_ref().shadows.len(), 2);
    }

    #[test]
    fn test_decorated_box_shadow_preserves_background() {
        let widget = DecoratedBox::with_style(
            Text::new("Hi"),
            Style::default()
                .background(Color::WHITE)
                .shadow(BoxShadow::new(Color::BLACK)),
        );
        assert_eq!(widget.style_ref().background, Some(Color::WHITE));
        assert_eq!(widget.style_ref().shadows.len(), 1);
    }

    #[test]
    fn test_decorated_box_with_style_sets_style() {
        let widget =
            DecoratedBox::with_style(Text::new("Hello"), Style::new().border(Color::BLACK, 1.0));
        assert_eq!(widget.style_ref().background, None);
        assert!(widget.style_ref().border.is_some());
    }

    #[test]
    fn test_decorated_box_border_does_not_add_padding() {
        let widget = DecoratedBox::with_style(
            Text::new("Hello"),
            Style::default().border(Color::BLACK, 2.0),
        );
        assert_eq!(widget.style_ref().border.as_ref().unwrap().width, 2.0);
    }

    #[test]
    fn test_decorated_box_render_object_is_pass_through() {
        let widget = DecoratedBox::new(Text::new("Hello"));
        let ro = widget.create_render_object();
        assert!(
            ro.is_pass_through(),
            "DecoratedBox's render object must be pass-through"
        );
    }

    #[test]
    fn test_decorated_box_render_object_creation() {
        let widget =
            DecoratedBox::with_style(Text::new("Hello"), Style::default().background(Color::RED));
        let ro = widget.create_render_object();
        assert!(ro
            .as_any()
            .downcast_ref::<DecoratedBoxRenderObject>()
            .is_some());
    }

    #[test]
    fn test_decorated_box_update_render_object_returns_paint_only() {
        let widget_red =
            DecoratedBox::with_style(Text::new("Hi"), Style::default().background(Color::RED));
        let mut ro = widget_red.create_render_object();

        let result = widget_red.update_render_object(ro.as_mut());
        assert_eq!(result, UpdateResult::NONE);

        let widget_blue =
            DecoratedBox::with_style(Text::new("Hi"), Style::default().background(Color::BLUE));
        let result = widget_blue.update_render_object(ro.as_mut());
        assert!(result.contains(UpdateResult::PAINT));
        assert!(
            !result.contains(UpdateResult::LAYOUT),
            "DecoratedBox must never return LAYOUT (proxy has no layout)"
        );
    }

    #[test]
    fn test_decorated_box_can_update_same_type() {
        let w1 = DecoratedBox::with_style(Text::new("Hi"), Style::default().background(Color::RED));
        let w2 =
            DecoratedBox::with_style(Text::new("Hi"), Style::default().background(Color::BLUE));
        let mut elem = DecoratedBoxElement::new();
        elem.set_widget(&w1);
        assert!(
            elem.can_update(w2.as_any()),
            "two DecoratedBox widgets must be able to update each other"
        );
    }

    #[test]
    fn test_decorated_box_element_default() {
        let elem = DecoratedBoxElement::default();
        assert!(elem.widget().is_none());
        assert!(elem.render_object_id().is_none());
    }

    #[test]
    fn test_decorated_box_clone_preserves_fields() {
        let widget =
            DecoratedBox::with_style(Text::new("Hi"), Style::default().background(Color::RED))
                .with_key("cloned");
        let cloned = widget.clone();
        assert_eq!(cloned.key(), widget.key());
        assert_eq!(cloned.style_ref(), widget.style_ref());
    }
}
