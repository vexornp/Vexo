//! Button widget - clickable button for retain mode.
//!
//! This widget demonstrates event handling in retain mode.

use std::any::Any;

use crate::core::{Bounds, Color, Logical, Point};
use crate::input::{ButtonState, InputEvent};
use crate::render::RenderCommand;

use super::{Element, Key, Widget};
use super::super::{EventContext, RenderObject, LayoutContext, LayoutResult, PaintContext, HitTestContext};
use crate::layout::{Layout, LayoutNodeId};

// ============================================================================
// BUTTON WIDGET
// ============================================================================

/// Button widget - clickable button with a label.
///
/// When clicked, emits a message via the Element's on_event method.
pub struct Button {
    key: Option<Key>,
    label: String,
    /// The message to emit when clicked (stored as Any for type erasure).
    message: Option<Box<dyn Any + Send>>,
}

impl Button {
    /// Create a new button with a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            key: None,
            label: label.into(),
            message: None,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the message to emit when clicked.
    pub fn with_message<M: Any + Clone + Send>(mut self, message: M) -> Self {
        self.message = Some(Box::new(message));
        self
    }

    /// Get the button label.
    pub fn label(&self) -> &str {
        &self.label
    }
}

impl Clone for Button {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            label: self.label.clone(),
            message: None, // Can't clone Box<dyn Any>
        }
    }
}

impl Widget for Button {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = ButtonElement::new(self.label.clone());
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ButtonRenderObject::new(&self.label))
    }

    fn clone_box(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ============================================================================
// BUTTON ELEMENT
// ============================================================================

use crate::retain::{ElementContext, ElementId, ElementRegistry, RenderObjectId};

/// Element for Button widget - handles click events.
pub struct ButtonElement {
    id: Option<ElementId>,
    key: Option<Key>,
    render_object: Option<RenderObjectId>,
    widget: Option<Box<dyn Widget>>,
    label: String,
}

impl ButtonElement {
    /// Create a new button element.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            label: label.into(),
        }
    }

    /// Set the widget for this element.
    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(widget.clone_box());
        self.key = widget.key();
    }

    /// Get the element ID.
    #[allow(dead_code)]
    pub fn id(&self) -> Option<ElementId> {
        self.id
    }
}

impl Element for ButtonElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Use the element ID from context (pre-allocated by pipeline)
        self.id = context.element_id;

        // Create render object if widget is set
        if let (Some(widget), Some(id)) = (&self.widget, self.id) {
            let render_obj = widget.create_render_object();
            if let Some(ro_id) = context.create_render_object(render_obj, id) {
                self.render_object = Some(ro_id);
                context.render_object = Some(ro_id);

                // Mark the new render object as needing layout and paint
                context.mark_needs_layout(ro_id);
                context.mark_needs_paint(ro_id);
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Widget>, context: &mut ElementContext) {
        // Store the new widget configuration
        self.widget = Some(new_widget);

        if let Some(ro) = self.render_object {
            context.mark_needs_layout(ro);
            context.mark_needs_paint(ro);
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

    fn widget_key(&self) -> Option<Key> {
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
                        eprintln!("ButtonElement::on_event: button clicked, label={}", self.label);
                        // Return a click message with the button label
                        return Some(Box::new(format!("button:{}", self.label)));
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
}

impl RenderObject for ButtonRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeId]) -> LayoutResult {
        // Create a leaf node with button-like sizing
        let layout = Layout {
            width: Some(crate::layout::Dimension::Length(100.0)),
            height: Some(crate::layout::Dimension::Length(40.0)),
            ..Layout::default()
        };
        let node = ctx.engine().create_leaf(&layout);
        self.layout_node = Some(node);
        LayoutResult {
            node,
            size: crate::core::Size::new(100.0, 40.0),
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

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        let bounds = match &self.computed_bounds {
            Some(b) => b,
            None => return Vec::new(),
        };

        let mut commands = Vec::new();

        // Draw button background (light gray with rounded corners)
        commands.push(RenderCommand::rounded_rect(
            *bounds,
            Color::rgb(0.9, 0.9, 0.9),
            4.0,
        ));

        // Draw button border (darker gray)
        commands.push(RenderCommand::rect_with_border(
            *bounds,
            Color::TRANSPARENT,
            Color::rgb(0.6, 0.6, 0.6),
            1.0,
        ));

        // Draw button label (centered)
        let text_x = bounds.left + bounds.width() / 2.0 - (self.label.len() as f32 * 4.0);
        let text_y = bounds.top + 10.0;
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

    #[test]
    fn test_button_widget_creation() {
        let widget = Button::new("Click Me");
        assert_eq!(widget.label(), "Click Me");
    }

    #[test]
    fn test_button_widget_with_key() {
        let widget = Button::new("Click Me").with_key("my-button");
        assert_eq!(widget.key(), Some(Key::new("my-button")));
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
