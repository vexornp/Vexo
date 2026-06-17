//! Decorated container widget - applies visual decorations to a child.
//!
//! This widget bundles multiple decorations (background, border, corner radius)
//! into a single element and render object, reducing overhead compared to
//! chaining multiple modifier widgets.

use std::any::Any;

use crate::core::{Color, Logical, Size};
#[allow(unused_imports)]
use crate::core::Bounds;
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
#[allow(unused_imports)]
use crate::layout::{
    AlignContent, AlignItems, AlignSelf, Dimension, EdgeInsets, FlexDirection, FlexWrap,
    Inset, JustifyContent, Layout, Overflow,
};
use crate::layout_builder_methods;
use crate::render_objects::ContainerRenderObject;
use crate::style::Style;
#[allow(unused_imports)]
use crate::{
    Element, ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectKey, UpdateResult, Widget, WidgetKey,
};

// ============================================================================
// DecoratedContainerElement
// ============================================================================

/// Element for DecoratedContainer widget.
///
/// Manages a single child element and updates the render object
/// when style changes.
pub struct DecoratedContainerElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl DecoratedContainerElement {
    /// Create a new decorated container element.
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

    /// Get the element ID.
    #[allow(dead_code)]
    pub fn id(&self) -> Option<ElementKey> {
        self.id
    }

    /// Get the child widget from the stored widget.
    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for DecoratedContainerElement {
    fn default() -> Self {
        Self::new()
    }
}

// Implement RenderObjectElement trait
impl RenderObjectElement for DecoratedContainerElement {
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

// Implement Element trait using the new traits
impl Element for DecoratedContainerElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Create focus attachment BEFORE mounting child.
        // The child will look up this element's focus node as its parent
        // when it mounts, so it must exist before child mounting begins.
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

        // Mount single child via child_ops (emit Inflate command)
        // The pipeline will execute it after mount() returns,
        // then call child_mounted() to link the child's render object.
        if let Some(widget) = &self.widget {
            if let Some(child_widget) = widget.child() {
                context.inflate_child(None, child_widget.clone_boxed());
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Use RenderObjectElement's default update for render object updates
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Use RenderObjectElement's default unmount for render object removal
        self.unmount_render_object(context);

        // Detach focus node from the focus tree
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

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
    }

    fn on_event(
        &mut self,
        _event: &InputEvent,
        _context: &mut EventContext,
        _state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        // DecoratedContainer doesn't handle events itself
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast and store the new widget
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.widget = Some(*widget);

            // Update the render object with new properties
            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self
                        .widget
                        .as_ref()
                        .unwrap()
                        .update_render_object(ro.as_mut());

                    // Only mark dirty based on what actually changed
                    if result.contains(UpdateResult::LAYOUT) {
                        context.mark_needs_layout(ro_id);
                    }
                    if result.contains(UpdateResult::PAINT) {
                        context.mark_needs_paint(ro_id);
                    }
                }
            }

            // Reconcile single child via child_ops
            let old_child = context.children().first().copied();
            if let Some(child_widget) = self.get_child_widget() {
                match old_child {
                    Some(old_child_key) => {
                        // Update existing child
                        context.update_child(old_child_key, child_widget.clone_boxed());
                    }
                    None => {
                        // Inflate new child
                        context.inflate_child(None, child_widget.clone_boxed());
                    }
                }
            } else if let Some(old_child_key) = old_child {
                // No new child widget - unmount the old child
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
        // Link the child's render object to our render object
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
// DecoratedContainer Widget
// ============================================================================

/// A widget that decorates a child with visual styling.
///
/// Creates a single element and render object regardless of how many
/// decorations are applied. This is more efficient than chaining
/// multiple modifier widgets (Background, Border, CornerRadius).
///
/// # Performance
///
/// | Approach | Elements | Render Objects |
/// |----------|----------|----------------|
/// | Chained modifiers | N | N |
/// | DecoratedContainer | 1 | 1 |
///
/// # Example
///
/// ```ignore
/// DecoratedContainer::new(Text::new("Hello").boxed())
///     .style(Style::new()
///         .background(Color::RED)
///         .border(Color::BLACK, 2.0)
///         .corner_radius(8.0))
/// ```
pub struct DecoratedContainer {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    style: Style,
    layout: Layout,
}

impl DecoratedContainer {
    /// Create a new decorated container with a child.
    pub fn new(child: impl Widget + 'static) -> Self {
        use crate::layout::AlignSelf;
        Self {
            key: None,
            child: Box::new(child),
            style: Style::default(),
            layout: Layout::default().align_self(AlignSelf::Start).flex_shrink(0.0),
        }
    }

    /// Set the style for this container.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the layout properties for this container.
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Set the key for this container.
    ///
    /// Accepts both local keys (strings) and global keys.
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

impl DecoratedContainer {
    layout_builder_methods!();

    // Style property builder methods (set individual style properties, preserving others)
    pub fn background(mut self, color: Color) -> Self {
        self.style = self.style.background(color);
        self
    }

    pub fn border(mut self, color: Color, width: f32) -> Self {
        self.style = self.style.border(color, width);
        // Add padding equal to border width so the child is inset from the border.
        // Without this, the child occupies the same area as the border and
        // opaque content paints over the border pixels.
        let existing = self.layout.padding.unwrap_or_default();
        self.layout.padding = Some(EdgeInsets {
            left: existing.left + width,
            right: existing.right + width,
            top: existing.top + width,
            bottom: existing.bottom + width,
        });
        self
    }

    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.style = self.style.corner_radius(radius);
        self
    }

    pub fn clip(mut self) -> Self {
        self.style = self.style.clip();
        self
    }
}

impl Clone for DecoratedContainer {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            style: self.style.clone(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for DecoratedContainer {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = DecoratedContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new_with_style(
            self.layout.clone(),
            self.style.clone(),
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(container_ro) = render_object
            .as_any_mut()
            .downcast_mut::<ContainerRenderObject>()
        {
            let layout_changed = container_ro.set_layout(self.layout.clone());
            let style_changed = container_ro.set_style(self.style.clone());

            if layout_changed {
                UpdateResult::LAYOUT
            } else if style_changed {
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
    fn test_decorated_container_creation() {
        let container = DecoratedContainer::new(Text::new("Hello"));
        assert!(container.key().is_none());
    }

    #[test]
    fn test_decorated_container_with_key() {
        let container = DecoratedContainer::new(Text::new("Hello")).with_key("my-container");
        assert_eq!(
            container.key(),
            Some(WidgetKey::Local(Key::new("my-container")))
        );
    }

    #[test]
    fn test_decorated_container_with_global_key() {
        let global_key = GlobalKey::new();
        let container = DecoratedContainer::new(Text::new("Hello")).with_key(global_key.clone());
        assert_eq!(container.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_decorated_container_with_style() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);

        let container = DecoratedContainer::new(Text::new("Hello")).style(style);
        assert_eq!(container.style_ref().background, Some(Color::RED));
    }

    #[test]
    fn test_decorated_container_render_object_creation() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);

        let container = DecoratedContainer::new(Text::new("Hello")).style(style);
        let ro = container.create_render_object();
        assert!(ro
            .as_any()
            .downcast_ref::<ContainerRenderObject>()
            .is_some());
    }

    #[test]
    fn test_decorated_container_render_object_paint() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);

        let mut ro = ContainerRenderObject::new_with_style(Layout::default(), style);
        ro.set_computed_bounds(Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0)));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Should have 2 commands (background + border)
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn test_decorated_container_render_object_paint_with_corner_radius() {
        let style = Style::new().background(Color::RED).corner_radius(8.0);

        let mut ro = ContainerRenderObject::new_with_style(Layout::default(), style);
        ro.set_computed_bounds(Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0)));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Should have 3 commands (push radius + background + pop radius)
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn test_decorated_container_render_object_paint_empty() {
        let style = Style::new(); // No decorations

        let mut ro = ContainerRenderObject::new_with_style(Layout::default(), style);
        ro.set_computed_bounds(Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0)));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Should have 0 commands (no decorations)
        assert_eq!(cmds.len(), 0);
    }

    #[test]
    fn test_decorated_container_render_object_set_style() {
        let style1 = Style::new().background(Color::RED);
        let mut ro = ContainerRenderObject::new_with_style(Layout::default(), style1);

        // Verify initial style via paint output
        ro.set_computed_bounds(Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0)));
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert_eq!(cmds.len(), 1); // background only

        let style2 = Style::new().background(Color::BLUE);
        ro.set_style(style2);

        let mut commands2 = Vec::new();
        let mut ctx2 = PaintContext::new(&mut commands2);
        let cmds2 = ro.paint(&mut ctx2);
        assert_eq!(cmds2.len(), 1); // still background only, different color
    }

    #[test]
    fn test_decorated_container_element_default() {
        let element = DecoratedContainerElement::default();

        assert!(element.id().is_none());
        assert!(element.render_object_id().is_none());
    }

    #[test]
    fn test_decorated_container_padding_preserves_default() {
        let dc = DecoratedContainer::new(Text::new("Hello"))
            .padding(8.0);
        assert!(dc.layout.padding.is_some());
    }

    #[test]
    fn test_decorated_container_background_preserves_padding() {
        let dc = DecoratedContainer::new(Text::new("Hello"))
            .padding(8.0)
            .background(Color::RED);
        assert!(dc.layout.padding.is_some());
        assert_eq!(dc.style.background, Some(Color::RED));
    }

    #[test]
    fn test_decorated_container_style_properties_chain() {
        let dc = DecoratedContainer::new(Text::new("Hello"))
            .background(Color::RED)
            .border(Color::BLACK, 2.0)
            .corner_radius(8.0)
            .clip();
        assert_eq!(dc.style.background, Some(Color::RED));
        assert_eq!(dc.style.border.as_ref().unwrap().width, 2.0);
        assert_eq!(dc.style.corner_radius.as_ref().unwrap().radius, 8.0);
        assert!(dc.style.clip);
    }

    #[test]
    fn test_decorated_container_style_replaces_everything() {
        let dc = DecoratedContainer::new(Text::new("Hello"))
            .background(Color::RED)
            .style(Style::new().border(Color::BLACK, 1.0));
        // .style() replaces the entire Style, so background is lost
        assert_eq!(dc.style.background, None);
        assert!(dc.style.border.is_some());
    }
}
