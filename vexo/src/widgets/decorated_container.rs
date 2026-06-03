//! Decorated container widget - applies visual decorations to a child.
//!
//! This widget bundles multiple decorations (background, border, corner radius)
//! into a single element and render object, reducing overhead compared to
//! chaining multiple modifier widgets.

use std::any::Any;

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position, Size};
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::layout::{Layout, LayoutNodeKey};
use crate::render::RenderCommand;
use crate::style::Style;
use crate::{
    Element, ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectKey, UpdateResult, Widget, WidgetKey,
};

// ============================================================================
// DecoratedContainerRenderObject
// ============================================================================

/// Render object for DecoratedContainer - handles all decorations in a single pass.
///
/// This render object paints background, border, and corner radius together,
/// avoiding the overhead of multiple nested render objects.
pub struct DecoratedContainerRenderObject {
    /// Current style configuration.
    style: Style,

    /// Child render object ID.
    child: Option<RenderObjectKey>,

    /// Computed bounds from layout.
    computed_bounds: Option<Bounds<Logical>>,

    /// Layout node in Taffy.
    layout_node: Option<LayoutNodeKey>,
}

impl DecoratedContainerRenderObject {
    /// Create a new decorated container render object with the given style.
    pub fn new(style: Style) -> Self {
        Self {
            style,
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }

    /// Set the style configuration.
    ///
    /// Returns true if the style changed.
    pub fn set_style(&mut self, style: Style) -> bool {
        if self.style != style {
            self.style = style;
            true
        } else {
            false
        }
    }

    /// Get the current style.
    #[allow(dead_code)]
    pub fn style(&self) -> &Style {
        &self.style
    }
}

impl RenderObject for DecoratedContainerRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // DecoratedContainer needs its own layout node to have a position in the parent container.
        // It wraps the child in a container-like node so the parent can position this container,
        // and the child is positioned relative to this container.
        let mut layout = Layout::default();

        // Apply padding from style if set
        if let Some(padding) = self.style.padding {
            layout = layout.padding(padding);
        }

        // Create a container node that holds the child
        let node = ctx.engine().create_container(&layout, child_nodes);
        self.layout_node = Some(node);

        LayoutResult {
            node,
            size: Size::zero(),
        }
    }

    fn apply_layout(&mut self, ctx: &LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        let bounds = match &self.computed_bounds {
            Some(b) => b,
            None => return vec![],
        };

        let mut commands = Vec::new();
        let pos: Position<Logical, Absolute> = ctx.absolute_position();

        let absolute_bounds = Bounds::new(
            pos.x,
            pos.y,
            pos.x + bounds.width(),
            pos.y + bounds.height(),
        );

        // 1. Push corner radius if set (affects all subsequent rects)
        if let Some(ref cr) = self.style.corner_radius {
            commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
        }

        // 2. Draw background first (behind child)
        if let Some(bg_color) = self.style.background {
            commands.push(RenderCommand::rect(absolute_bounds, bg_color));
        }

        // 3. Draw border on top (after background)
        if let Some(ref border) = self.style.border {
            commands.push(RenderCommand::rect_with_border(
                absolute_bounds,
                Color::TRANSPARENT,
                border.color,
                border.width,
            ));
        }

        // 4. Pop corner radius
        if self.style.corner_radius.is_some() {
            commands.push(RenderCommand::PopCornerRadius);
        }

        commands
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

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
}

impl DecoratedContainer {
    /// Create a new decorated container with a child.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            style: Style::default(),
        }
    }

    /// Set the style for this container.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
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

impl Clone for DecoratedContainer {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            style: self.style.clone(),
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
        Box::new(DecoratedContainerRenderObject::new(self.style.clone()))
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
            .downcast_mut::<DecoratedContainerRenderObject>()
        {
            if container_ro.set_style(self.style.clone()) {
                // Style changes (background, border, corner_radius) are visual-only
                // They don't affect layout, only paint
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
            .downcast_ref::<DecoratedContainerRenderObject>()
            .is_some());
    }

    #[test]
    fn test_decorated_container_render_object_paint() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);

        let mut ro = DecoratedContainerRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Should have 2 commands (background + border)
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn test_decorated_container_render_object_paint_with_corner_radius() {
        let style = Style::new().background(Color::RED).corner_radius(8.0);

        let mut ro = DecoratedContainerRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Should have 3 commands (push radius + background + pop radius)
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn test_decorated_container_render_object_paint_empty() {
        let style = Style::new(); // No decorations

        let mut ro = DecoratedContainerRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Should have 0 commands (no decorations)
        assert_eq!(cmds.len(), 0);
    }

    #[test]
    fn test_decorated_container_render_object_set_style() {
        let style1 = Style::new().background(Color::RED);
        let mut ro = DecoratedContainerRenderObject::new(style1);

        assert_eq!(ro.style().background, Some(Color::RED));

        let style2 = Style::new().background(Color::BLUE);
        ro.set_style(style2);

        assert_eq!(ro.style().background, Some(Color::BLUE));
    }

    #[test]
    fn test_decorated_container_element_default() {
        let element = DecoratedContainerElement::default();

        assert!(element.id().is_none());
        assert!(element.render_object_id().is_none());
    }
}
