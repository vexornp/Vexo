//! GestureDetector widget - invisible modifier that detects pointer gestures.
//!
//! This widget wraps a child and emits callbacks for press and release events.
//! It follows Flutter's GestureDetector pattern: invisible, no visual rendering,
//! and decouples gesture handling from visual widgets.
//!
//! # Architecture
//!
//! GestureDetector is a modifier widget (like DecoratedContainer):
//! - Wraps a single child widget
//! - Has its own element (GestureDetectorElement) for event handling
//! - Has a pass-through render object (invisible, delegates layout to child)
//!
//! # Usage
//!
//! ```ignore
//! // Tap pattern (press = click)
//! GestureDetector::new(Box::new(
//!     DecoratedContainer::new(Box::new(Text::new("Click Me")))
//!         .style(Style::new().background(Color::BLUE).corner_radius(4.0))
//! ))
//! .on_press(|| { log::info!("Clicked!"); })
//!
//! // Press and release pattern (drag-like)
//! GestureDetector::new(child)
//!     .on_press(|| { log::info!("Pressed!"); })
//!     .on_release(|| { log::info!("Released!"); })
//! ```

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::{Bounds, Logical, Point, Size};
use crate::input::{ButtonState, InputEvent};
use crate::layout::{Layout, LayoutNodeKey};

use super::{Element, Widget};
use super::super::key::WidgetKey;
use super::super::{
    ElementContext, ElementKey, EventContext,
    LayoutContext, LayoutResult, PaintContext, HitTestContext,
    RenderObject, RenderObjectKey,
};
use super::super::elements::RenderObjectElement;
use super::super::focus::attachment::FocusAttachment;

// ============================================================================
// GESTURE DETECTOR WIDGET
// ============================================================================

/// Invisible modifier widget that detects pointer gestures on its child.
///
/// GestureDetector wraps a child widget and provides callback-based event
/// handling for press and release events. It has no visual representation.
///
/// This follows Flutter's pattern where gesture detection is decoupled from
/// visual widgets. Any widget can become "tappable" by wrapping it in
/// GestureDetector.
pub struct GestureDetector {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    /// Callback invoked when pointer is pressed inside the child bounds.
    on_press: Option<Rc<RefCell<dyn FnMut()>>>,
    /// Callback invoked when pointer is released inside the child bounds.
    on_release: Option<Rc<RefCell<dyn FnMut()>>>,
}

impl GestureDetector {
    /// Create a new GestureDetector wrapping a child widget.
    pub fn new(child: Box<dyn Widget>) -> Self {
        Self {
            key: None,
            child,
            on_press: None,
            on_release: None,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the callback for press events (pointer button pressed inside bounds).
    pub fn on_press(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_press = Some(Rc::new(RefCell::new(callback)));
        self
    }

    /// Set the callback for release events (pointer button released inside bounds).
    pub fn on_release(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_release = Some(Rc::new(RefCell::new(callback)));
        self
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }
}

impl Clone for GestureDetector {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            on_press: self.on_press.clone(),
            on_release: self.on_release.clone(),
        }
    }
}

impl Widget for GestureDetector {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = GestureDetectorElement::new();
        elem.set_widget_from_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(GestureDetectorRenderObject::new())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

// ============================================================================
// GESTURE DETECTOR ELEMENT
// ============================================================================

/// Element for GestureDetector widget - handles press/release events.
///
/// This element:
/// - Owns a render object (pass-through, invisible)
/// - Has a single child element
/// - Handles pointer events via on_press/on_release callbacks
/// - Handles pointer events via on_press/on_release callbacks
pub struct GestureDetectorElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    on_press: Option<Rc<RefCell<dyn FnMut()>>>,
    on_release: Option<Rc<RefCell<dyn FnMut()>>>,
    focus_attachment: Option<FocusAttachment>,
}

impl GestureDetectorElement {
    /// Create a new GestureDetector element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            on_press: None,
            on_release: None,
            focus_attachment: None,
        }
    }

    /// Set widget data from a GestureDetector widget.
    fn set_widget_from_widget(&mut self, widget: &GestureDetector) {
        self.key = widget.key.clone();
        self.on_press = widget.on_press.clone();
        self.on_release = widget.on_release.clone();
        self.widget = Some(widget.clone_boxed());
    }

    /// Get the child widget from the stored widget.
    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for GestureDetectorElement {
    fn default() -> Self {
        Self::new()
    }
}

// Implement RenderObjectElement trait
impl RenderObjectElement for GestureDetectorElement {
    fn widget(&self) -> Option<&dyn Widget> {
        self.widget.as_deref()
    }

    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        // Update callbacks from the widget if it's a GestureDetector
        if let Some(gd) = widget.as_any().downcast_ref::<GestureDetector>() {
            self.key = gd.key.clone();
            self.on_press = gd.on_press.clone();
            self.on_release = gd.on_release.clone();
        }
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

// Implement Element trait
impl Element for GestureDetectorElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Create focus attachment BEFORE mounting child.
        // The child will look up this element's focus node as its parent
        // when it mounts, so it must exist before child mounting begins.
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context.focus_manager().create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        // Use RenderObjectElement's default mount for render object creation
        self.mount_render_object(context);

        // Mount single child via child_ops (emit Inflate command)
        // The pipeline will execute it after mount() returns,
        // then call child_mounted() to link the child's render object.
        if let Some(child_widget) = self.get_child_widget() {
            context.inflate_child(None, child_widget.clone_boxed());
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Use RenderObjectElement's default update for render object updates
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Use RenderObjectElement's default unmount for render object removal
        self.unmount_render_object(context);
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
        event: &InputEvent,
        context: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        if let InputEvent::PointerButton { state, .. } = event {
            if context.is_pointer_inside() {
                match state {
                    ButtonState::Pressed => {
                        if let Some(callback) = &self.on_press {
                            (callback.borrow_mut())();
                        }
                        return Some(Box::new(()));
                    }
                    ButtonState::Released => {
                        if let Some(callback) = &self.on_release {
                            (callback.borrow_mut())();
                        }
                        return Some(Box::new(()));
                    }
                }
            }
        }
        None
    }

    fn rebuild(
        &mut self,
        new_widget: Box<dyn Any>,
        context: &mut ElementContext,
    ) {
        // Downcast and store the new widget
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            // Update callbacks from the new widget
            if let Some(gd) = widget.as_any().downcast_ref::<GestureDetector>() {
                self.on_press = gd.on_press.clone();
                self.on_release = gd.on_release.clone();
            }
            self.widget = Some(*widget);

            // Reconcile single child via child_ops
            if let Some(child_widget) = self.get_child_widget() {
                let old_child = context.children().first().copied();
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
            } else if let Some(old_child_key) = context.children().first().copied() {
                // No new child widget - unmount the old child
                context.unmount_child(old_child_key);
            }
        }
    }

    fn child_mounted(&mut self, _slot: Option<usize>, child_ro: Option<RenderObjectKey>, context: &mut ElementContext) {
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
// GESTURE DETECTOR RENDER OBJECT
// ============================================================================

/// Pass-through render object for GestureDetector - invisible.
///
/// This render object:
/// - Delegates layout to its child (pass-through)
/// - Generates no paint commands (invisible)
/// - Hit tests using its computed bounds (for event routing)
pub struct GestureDetectorRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl GestureDetectorRenderObject {
    /// Create a new GestureDetector render object.
    pub fn new() -> Self {
        Self {
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }
}

impl Default for GestureDetectorRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for GestureDetectorRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        // Pass-through: create a container node wrapping the child
        let layout = Layout::default();
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

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        // Invisible - no paint commands
        vec![]
    }

    fn hit_test(&self, position: Point<crate::core::Logical>, _ctx: &HitTestContext) -> bool {
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
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Key;
    use crate::retain::Text;
    use std::cell::Cell;
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_gesture_detector_creation() {
        let gd = GestureDetector::new(Box::new(Text::new("Click Me")));
        assert!(gd.key().is_none());
    }

    #[test]
    fn test_gesture_detector_with_key() {
        let gd = GestureDetector::new(Box::new(Text::new("Click Me")))
            .with_key("my-gesture");
        assert_eq!(gd.key(), Some(WidgetKey::Local(Key::new("my-gesture"))));
    }

    #[test]
    fn test_gesture_detector_with_callbacks() {
        let press_called = Rc::new(Cell::new(false));
        let release_called = Rc::new(Cell::new(false));
        let press_clone = press_called.clone();
        let release_clone = release_called.clone();

        let gd = GestureDetector::new(Box::new(Text::new("Click Me")))
            .on_press(move || press_clone.set(true))
            .on_release(move || release_clone.set(true));

        assert!(gd.on_press.is_some());
        assert!(gd.on_release.is_some());
    }

    #[test]
    fn test_gesture_detector_element_event_press() {
        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();

        let mut elem = GestureDetectorElement::new();
        elem.on_press = Some(Rc::new(RefCell::new(move || called_clone.set(true))));

        let mut state = crate::retain::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let mut ctx = EventContext::new(
            Point::new(50.0, 25.0),
            None,
            bounds,
            crate::input::Modifiers::default(),
            &mut state,
            &mut font_system,
        );

        let event = InputEvent::PointerButton {
            position: Point::new(50.0, 25.0),
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx);
        assert!(result.is_some());
        assert!(called.get());
    }

    #[test]
    fn test_gesture_detector_element_event_release() {
        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();

        let mut elem = GestureDetectorElement::new();
        elem.on_release = Some(Rc::new(RefCell::new(move || called_clone.set(true))));

        let mut state = crate::retain::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let mut ctx = EventContext::new(
            Point::new(50.0, 25.0),
            None,
            bounds,
            crate::input::Modifiers::default(),
            &mut state,
            &mut font_system,
        );

        let event = InputEvent::PointerButton {
            position: Point::new(50.0, 25.0),
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Released,
        };

        let result = elem.on_event(&event, &mut ctx);
        assert!(result.is_some());
        assert!(called.get());
    }

    #[test]
    fn test_gesture_detector_element_event_outside_bounds() {
        let mut elem = GestureDetectorElement::new();
        elem.on_press = Some(Rc::new(RefCell::new(|| {})));

        let mut state = crate::retain::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let mut ctx = EventContext::new(
            Point::new(200.0, 200.0), // Outside bounds
            None,
            bounds,
            crate::input::Modifiers::default(),
            &mut state,
            &mut font_system,
        );

        let event = InputEvent::PointerButton {
            position: Point::new(200.0, 200.0),
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx);
        assert!(result.is_none());
    }

    #[test]
    fn test_gesture_detector_render_object_paint_empty() {
        let ro = GestureDetectorRenderObject::new();
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_gesture_detector_clone() {
        let gd = GestureDetector::new(Box::new(Text::new("Hello")))
            .on_press(|| {});

        let cloned = gd.clone();
        assert!(cloned.on_press.is_some());
    }
}