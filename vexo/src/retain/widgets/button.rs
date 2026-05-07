//! Button widget - clickable button for retain mode.
//!
//! This widget demonstrates event handling in retain mode with ELM-style typed messages.

use std::any::Any;

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position};
use crate::input::{ButtonState, InputEvent};
use crate::render::RenderCommand;

use super::{Element, Widget};
use super::super::key::{GlobalKey, Key, WidgetKey};
use super::super::{EventContext, RenderObject, LayoutContext, LayoutResult, PaintContext, HitTestContext, UpdateResult};
use crate::layout::{Layout, LayoutNodeId};

// ============================================================================
// BUTTON WIDGET
// ============================================================================

/// Button widget - clickable button with a label.
///
/// When clicked, emits a typed message `M` via the Element's on_event method.
///
/// # Type Parameter
///
/// `M` - The message type emitted when the button is clicked. Must be `Clone + Send + 'static`.
///
/// # Example
///
/// ```
/// use vexo::retain::Button;
///
/// #[derive(Clone)]
/// enum Message {
///     Increment,
///     Decrement,
/// }
///
/// let button = Button::new("Click Me").with_message(Message::Increment);
/// ```
pub struct Button<M: Clone + Send + 'static> {
    key: Option<WidgetKey>,
    label: String,
    /// The typed message to emit when clicked.
    message: Option<M>,
}

impl<M: Clone + Send + 'static> Button<M> {
    /// Create a new button with a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            key: None,
            label: label.into(),
            message: None,
        }
    }

    /// Set the key for this widget.
    ///
    /// Accepts both local keys (strings) and global keys.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the message to emit when clicked.
    pub fn with_message(mut self, message: M) -> Self {
        self.message = Some(message);
        self
    }

    /// Get the button label.
    pub fn label(&self) -> &str {
        &self.label
    }
}

impl<M: Clone + Send + 'static> Clone for Button<M> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            label: self.label.clone(),
            message: self.message.clone(),
        }
    }
}

impl<M: Clone + Send + 'static> Widget<M> for Button<M> {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ButtonElement::new(self.label.clone(), self.message.clone());
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ButtonRenderObject::new(&self.label))
    }

    fn clone_box(&self) -> Box<dyn Widget<M>> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        // Downcast to ButtonRenderObject and update properties
        if let Some(button_ro) = render_object.as_any_mut().downcast_mut::<ButtonRenderObject>() {
            if button_ro.set_label(&self.label) {
                // Label affects both layout (button size) and paint
                UpdateResult::LAYOUT | UpdateResult::PAINT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }
}

// ============================================================================
// BUTTON ELEMENT
// ============================================================================

use crate::retain::{ElementContext, ElementId, ElementRegistry, RenderObjectId};

/// Element for Button widget - handles click events.
///
/// Generic over the message type `M` to emit typed messages on click.
pub struct ButtonElement<M: Clone + Send + 'static> {
    id: Option<ElementId>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget<M>>>,
    label: String,
    message: Option<M>,
}

impl<M: Clone + Send + 'static> ButtonElement<M> {
    /// Create a new button element.
    pub fn new(label: impl Into<String>, message: Option<M>) -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            label: label.into(),
            message,
        }
    }

    /// Set the widget for this element.
    pub fn set_widget(&mut self, widget: &dyn Widget<M>) {
        self.widget = Some(widget.clone_box());
        self.key = widget.key();
    }

    /// Get the element ID.
    #[allow(dead_code)]
    pub fn id(&self) -> Option<ElementId> {
        self.id
    }
}

impl<M: Clone + Send + 'static> Element for ButtonElement<M> {
    fn mount(&mut self, context: &mut ElementContext) {
        // Use the element ID from context - single source of truth
        self.id = Some(context.element_id);

        // Create render object if widget is set
        if let Some(widget) = &self.widget {
            let render_obj = widget.create_render_object();
            if let Some(ro_id) = context.create_render_object(render_obj, context.element_id) {
                self.render_object = Some(ro_id);
                context.render_object = Some(ro_id);

                // Mark the new render object as needing layout and paint
                context.mark_needs_layout(ro_id);
                context.mark_needs_paint(ro_id);
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // The widget is passed as Box<dyn Widget<M>> but type-erased to Box<dyn Any>
        // We need to downcast it back
        // Note: This is safe because we know the pipeline only passes widgets of the correct type
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget<M>>>() {
            self.widget = Some(*widget);

            // Update the render object with new properties from the widget
            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self.widget.as_ref().unwrap().update_render_object(ro.as_mut());

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

    fn unmount(&mut self, context: &mut ElementContext) {
        // Remove render object from registry
        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
            context.dirty.mark_needs_paint(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {
        // Button has no children
    }

    fn render_object(&self) -> Option<RenderObjectId> {
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
        event: &InputEvent,
        context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        // Handle click events
        match event {
            InputEvent::PointerButton { state, .. } => {
                if *state == ButtonState::Pressed {
                    // Check if click is inside our bounds
                    if context.is_pointer_inside() {
                        // Emit the typed message if set
                        if let Some(msg) = &self.message {
                            return Some(Box::new(msg.clone()));
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }
}

// ============================================================================
// BUTTON RENDER OBJECT
// ============================================================================

/// Render object for Button - renders the button visuals.
pub struct ButtonRenderObject {
    label: String,
    layout_node: Option<LayoutNodeId>,
    computed_bounds: Option<Bounds<Logical>>,
}

impl ButtonRenderObject {
    /// Create a new button render object.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            layout_node: None,
            computed_bounds: None,
        }
    }

    /// Get the computed bounds.
    #[allow(dead_code)]
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    /// Set the button label.
    ///
    /// Returns true if the label changed.
    pub fn set_label(&mut self, label: &str) -> bool {
        if self.label != label {
            self.label = label.to_string();
            true
        } else {
            false
        }
    }
}

impl RenderObject for ButtonRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeId]) -> LayoutResult {
        // Calculate button width based on label length
        // Approximate: each character is ~8px wide at 16px font size, plus padding
        let text_width = self.label.len() as f32 * 8.0;
        let button_width = (text_width + 24.0).max(80.0); // Minimum 80px, with 12px padding on each side
        let button_height = 40.0;

        let layout = Layout {
            width: Some(crate::layout::Dimension::Length(button_width)),
            height: Some(crate::layout::Dimension::Length(button_height)),
            ..Layout::default()
        };
        let node = ctx.engine().create_leaf(&layout);
        self.layout_node = Some(node);
        LayoutResult {
            node,
            size: crate::core::Size::new(button_width, button_height),
        }
    }

    fn apply_layout(&mut self, ctx: &LayoutContext) {
        // Read computed layout from engine
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        let bounds = match &self.computed_bounds {
            Some(b) => b,
            None => return Vec::new(),
        };

        // Get the absolute position where this button should be painted.
        // The context already calculated the absolute position from the
        // parent chain, so we just use it directly.
        let pos: Position<Logical, Absolute> = ctx.absolute_position();

        // Create absolute bounds at the correct position with our size
        let absolute_bounds = Bounds::new(
            pos.x,
            pos.y,
            pos.x + bounds.width(),
            pos.y + bounds.height(),
        );

        let mut commands = Vec::new();

        // Draw button background (light gray with rounded corners)
        commands.push(RenderCommand::rounded_rect(
            absolute_bounds,
            Color::rgb(0.9, 0.9, 0.9),
            4.0,
        ));

        // Draw button border (darker gray)
        commands.push(RenderCommand::rect_with_border(
            absolute_bounds,
            Color::TRANSPARENT,
            Color::rgb(0.6, 0.6, 0.6),
            1.0,
        ));

        // Draw button label (centered)
        // Approximate text width: ~8px per character at 16px font
        let text_width = self.label.len() as f32 * 8.0;
        let text_x = absolute_bounds.left + (absolute_bounds.width() - text_width) / 2.0;
        let text_y = absolute_bounds.top + (absolute_bounds.height() - 16.0) / 2.0;
        commands.push(RenderCommand::text(
            self.label.clone(),
            Point::new(text_x, text_y),
            16.0,
            Color::BLACK,
        ));

        commands
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn layout_node(&self) -> Option<LayoutNodeId> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    enum TestMessage {
        Clicked,
        Other,
    }

    #[test]
    fn test_button_widget_creation() {
        let widget: Button<TestMessage> = Button::new("Click Me");
        assert_eq!(widget.label(), "Click Me");
    }

    #[test]
    fn test_button_widget_with_key() {
        let widget: Button<TestMessage> = Button::new("Click Me").with_key("my-button");
        assert_eq!(widget.key(), Some(WidgetKey::Local(Key::new("my-button"))));
    }

    #[test]
    fn test_button_widget_with_global_key() {
        let global_key = GlobalKey::new();
        let widget: Button<TestMessage> = Button::new("Click Me").with_key(global_key.clone());
        assert_eq!(widget.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_button_widget_with_message() {
        let widget: Button<TestMessage> = Button::new("Click Me").with_message(TestMessage::Clicked);
        assert_eq!(widget.message, Some(TestMessage::Clicked));
    }

    #[test]
    fn test_button_render_object_layout() {
        use crate::layout::TaffyLayoutEngine;
        use std::sync::Arc;

        let mut obj = ButtonRenderObject::new("Click Me");
        let mut engine = TaffyLayoutEngine::new();
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        let mut font_system = glyphon::FontSystem::new_with_fonts([binary]);
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = obj.layout(&mut ctx, &[]);
        assert!(obj.layout_node.is_some());
        let _ = result;
    }
}
