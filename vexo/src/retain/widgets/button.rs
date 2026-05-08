//! Button widget - clickable button for retain mode.
//!
//! This widget demonstrates event handling in retain mode with callbacks.

use std::any::Any;
use std::rc::Rc;
use std::cell::RefCell;

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position};
use crate::input::{ButtonState, InputEvent};
use crate::render::RenderCommand;

use super::{Element, Widget};
use super::super::key::WidgetKey;
use super::super::{EventContext, RenderObject, LayoutContext, LayoutResult, PaintContext, HitTestContext, UpdateResult};
use crate::layout::{Layout, LayoutNodeId};

// ============================================================================
// BUTTON WIDGET
// ============================================================================

/// Button widget - clickable button with a label.
///
/// When clicked, calls the `on_press` callback if set.
pub struct Button {
    key: Option<WidgetKey>,
    label: String,
    /// Callback invoked when button is pressed.
    /// Uses Rc<RefCell> for Clone support.
    on_press: Option<Rc<RefCell<dyn FnMut()>>>,
}

impl Button {
    /// Create a new button with a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            key: None,
            label: label.into(),
            on_press: None,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the callback for press events.
    pub fn on_press(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_press = Some(Rc::new(RefCell::new(callback)));
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
            // Rc<RefCell> is Clone, so callbacks survive widget rebuilds
            on_press: self.on_press.clone(),
        }
    }
}

impl Widget for Button {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        // Create element with label and callback
        let elem = ButtonElement::new(
            self.label.clone(),
            self.key.clone(),
            self.on_press.clone(),
        );
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ButtonRenderObject::new(&self.label))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(button_ro) = render_object.as_any_mut().downcast_mut::<ButtonRenderObject>() {
            if button_ro.set_label(&self.label) {
                UpdateResult::LAYOUT | UpdateResult::PAINT
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

// ============================================================================
// BUTTON ELEMENT
// ============================================================================

use crate::retain::{ElementContext, ElementId, ElementRegistry, RenderObjectId};

/// Element for Button widget - handles click events.
pub struct ButtonElement {
    id: Option<ElementId>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectId>,
    label: String,
    on_press: Option<Rc<RefCell<dyn FnMut()>>>,
}

impl ButtonElement {
    /// Create a new button element.
    pub fn new(label: String, key: Option<WidgetKey>, on_press: Option<Rc<RefCell<dyn FnMut()>>>) -> Self {
        Self {
            id: None,
            key,
            render_object: None,
            label,
            on_press,
        }
    }

    /// Get the element ID.
    #[allow(dead_code)]
    pub fn id(&self) -> Option<ElementId> {
        self.id
    }
}

impl Element for ButtonElement {
    fn mount(&mut self, context: &mut ElementContext) {
        self.id = Some(context.element_id);

        // Create render object
        let render_obj = Box::new(ButtonRenderObject::new(&self.label));
        if let Some(ro_id) = context.create_render_object(render_obj, context.element_id) {
            self.render_object = Some(ro_id);
            context.render_object = Some(ro_id);

            context.mark_needs_layout(ro_id);
            context.mark_needs_paint(ro_id);
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Button>() {
            self.label = widget.label.clone();
            self.key = widget.key.clone();
            self.on_press = widget.on_press.clone();

            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    if let Some(button_ro) = ro.as_any_mut().downcast_mut::<ButtonRenderObject>() {
                        let result = if button_ro.set_label(&self.label) {
                            UpdateResult::LAYOUT | UpdateResult::PAINT
                        } else {
                            UpdateResult::NONE
                        };

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

    fn unmount(&mut self, context: &mut ElementContext) {
        if let Some(ro) = self.render_object {
            context.remove_render_object(ro);
            context.dirty.mark_needs_paint(ro);
        }
        if let Some(id) = self.id {
            context.remove_state(id);
        }
    }

    fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {}

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
        if let InputEvent::PointerButton { state, .. } = event {
            if *state == ButtonState::Pressed && context.is_pointer_inside() {
                // Invoke callback if set
                if let Some(callback) = &self.on_press {
                    (callback.borrow_mut())();
                }
                // Button was clicked - return a marker
                return Some(Box::new(()));
            }
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
    use super::super::Key;

    #[test]
    fn test_button_widget_creation() {
        let widget = Button::new("Click Me");
        assert_eq!(widget.label(), "Click Me");
    }

    #[test]
    fn test_button_widget_with_key() {
        let widget = Button::new("Click Me").with_key("my-button");
        assert_eq!(widget.key(), Some(WidgetKey::Local(Key::new("my-button"))));
    }

    #[test]
    fn test_button_widget_with_callback() {
        use std::cell::Cell;
        use std::rc::Rc;

        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();

        let widget = Button::new("Click Me").on_press(move || {
            called_clone.set(true);
        });

        assert!(widget.on_press.is_some());
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
