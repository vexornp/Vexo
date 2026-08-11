//! GestureDetector widget - invisible modifier that detects pointer gestures.
//!
//! This widget wraps a child and emits callbacks for press and release events.
//! It follows Flutter's GestureDetector pattern: invisible, no visual rendering,
//! and decouples gesture handling from visual widgets.
//!
//! # Architecture
//!
//! GestureDetector is a modifier widget (like DecoratedBox):
//! - Wraps a single child widget
//! - Has its own element (GestureDetectorElement) for event handling
//! - Has a pass-through render object (invisible, delegates layout to child)
//!
//! # Usage
//!
//! ```ignore
//! // Tap pattern (press = click)
//! GestureDetector::new(Box::new(
//!     DecoratedBox::with_style(
//!         Box::new(Text::new("Click Me")),
//!         Style::default().background(Color::BLUE).corner_radius(4.0),
//!     )
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
use crate::gestures::{ArenaEvent, GestureArena, GestureRecognizer, TapRecognizer};
use crate::input::{ButtonState, InputEvent};
use crate::layout::{Layout, LayoutNodeKey};

use super::super::elements::RenderObjectElement;
use super::super::focus::attachment::FocusAttachment;
use super::super::key::WidgetKey;
use super::super::{
    ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectKey,
};
use super::{Element, Widget};

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
    /// Callback invoked when a tap is recognized (pointer up, having won the
    /// arena). Arena-mediated — does NOT fire if a drag wins instead.
    on_tap: Option<Rc<RefCell<dyn FnMut()>>>,
    /// Callback invoked when the secondary (right) mouse button is pressed
    /// inside the child bounds. Receives the global cursor position and the
    /// element's global bounds (window-logical coordinates).
    on_secondary_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
    /// Callback invoked when a long-press is recognized (pointer held still
    /// for 500ms within slop). Arena-mediated — does NOT fire if a drag
    /// (scroll) wins instead. Receives the press position (where the finger
    /// went down) and the element's global bounds.
    on_long_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
}

impl GestureDetector {
    /// Create a new GestureDetector wrapping a child widget.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            on_press: None,
            on_release: None,
            on_tap: None,
            on_secondary_press: None,
            on_long_press: None,
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

    /// Set the callback for tap events (arena-mediated: fires on pointer-up
    /// after winning the arena). Use this for actions like navigation — it
    /// will NOT fire if a drag (scroll) wins the gesture instead.
    pub fn on_tap(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_tap = Some(Rc::new(RefCell::new(callback)));
        self
    }

    /// Set the callback for secondary (right-click) button press events.
    /// Receives the global cursor position (window-logical coordinates) and
    /// the element's global bounds (from `EventContext::bounds()`).
    /// When set, this takes precedence over `on_press` for Secondary presses.
    pub fn on_secondary_press(
        mut self,
        callback: impl FnMut(Point<Logical>, Bounds<Logical>) + 'static,
    ) -> Self {
        self.on_secondary_press = Some(Rc::new(RefCell::new(callback)));
        self
    }

    /// Set the callback for long-press events (arena-mediated: fires after
    /// the pointer is held still for 500ms within slop). Receives the press
    /// position (where the finger went down, in window-logical coordinates)
    /// and the element's global bounds. Use this for actions like showing a
    /// context menu on mobile — it will NOT fire if a drag (scroll) wins the
    /// gesture instead.
    pub fn on_long_press(
        mut self,
        callback: impl FnMut(Point<Logical>, Bounds<Logical>) + 'static,
    ) -> Self {
        self.on_long_press = Some(Rc::new(RefCell::new(callback)));
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
            on_tap: self.on_tap.clone(),
            on_secondary_press: self.on_secondary_press.clone(),
            on_long_press: self.on_long_press.clone(),
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
    on_tap: Option<Rc<RefCell<dyn FnMut()>>>,
    on_secondary_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
    on_long_press: Option<Rc<RefCell<dyn FnMut(Point<Logical>, Bounds<Logical>)>>>,
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
            on_tap: None,
            on_secondary_press: None,
            on_long_press: None,
            focus_attachment: None,
        }
    }

    /// Set widget data from a GestureDetector widget.
    fn set_widget_from_widget(&mut self, widget: &GestureDetector) {
        self.key = widget.key.clone();
        self.on_press = widget.on_press.clone();
        self.on_release = widget.on_release.clone();
        self.on_tap = widget.on_tap.clone();
        self.on_secondary_press = widget.on_secondary_press.clone();
        self.on_long_press = widget.on_long_press.clone();
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
            self.on_tap = gd.on_tap.clone();
            self.on_secondary_press = gd.on_secondary_press.clone();
            self.on_long_press = gd.on_long_press.clone();
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

    fn can_update(&self, widget: &dyn Any) -> bool {
        self.widget
            .as_ref()
            .map(|old| old.as_any().type_id() == widget.type_id())
            .unwrap_or(false)
    }

    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
        _state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        // Claim semantics: this method returns Some only when a callback
        // actually fired. A callback-less GestureDetector (e.g. a future
        // hit-test-only wrapper) returns None so the event bubbles to
        // ancestors. Previously every in-bounds press/release returned Some
        // unconditionally, swallowing events from parents.
        if let InputEvent::PointerButton {
            state,
            position,
            button,
        } = event
        {
            if context.bounds().contains(position) {
                match state {
                    ButtonState::Pressed => {
                        // Secondary (right-click) with on_secondary_press set:
                        // fire it with position + global bounds, claim the
                        // event, skip on_press.
                        if *button == crate::input::PointerButton::Secondary {
                            if let Some(callback) = &self.on_secondary_press {
                                (callback.borrow_mut())(*position, context.bounds());
                                return Some(Box::new(()));
                            }
                            // Fall through to on_press for Secondary when
                            // on_secondary_press is not set (backward-compat:
                            // dismiss barrier closes on any button).
                        }
                        if let Some(callback) = &self.on_press {
                            (callback.borrow_mut())();
                            return Some(Box::new(()));
                        }
                    }
                    ButtonState::Released => {
                        if let Some(callback) = &self.on_release {
                            (callback.borrow_mut())();
                            return Some(Box::new(()));
                        }
                    }
                }
            }
        }
        None
    }

    fn register_gestures(&mut self, arena: &mut GestureArena, self_id: ElementKey) {
        // Only register a tap recognizer if there's an on_tap callback.
        // (on_press/on_release fire immediately via on_event and don't need
        // the arena — they're press-down feedback, not actions.)
        if self.on_tap.is_some() {
            arena.add(Box::new(TapRecognizer::new()), self_id);
        }
        if self.on_long_press.is_some() {
            arena.add(
                Box::new(crate::gestures::LongPressRecognizer::new()),
                self_id,
            );
        }
    }

    fn on_arena_winner_update(
        &mut self,
        recognizer: &dyn GestureRecognizer,
        event: &ArenaEvent,
        ctx: &mut EventContext,
    ) {
        match event {
            ArenaEvent::Up { .. } => {
                // Fire on_tap when the tap recognizer wins (on Up).
                if recognizer.accepted() {
                    if let Some(callback) = &self.on_tap {
                        (callback.borrow_mut())();
                    }
                }
            }
            ArenaEvent::Tick { .. } => {
                // Long-press fires at 500ms while the finger is still down.
                // Position comes from the recognizer's `down_position()`
                // (the press location). Bounds come from the EventContext,
                // which the pipeline's tick_arena dispatch builds from
                // render_objects.bounds_for_element(winner_id).
                if recognizer.accepted() {
                    if let Some(callback) = &self.on_long_press {
                        if let Some(lp) = recognizer
                            .as_any()
                            .downcast_ref::<crate::gestures::LongPressRecognizer>()
                        {
                            (callback.borrow_mut())(lp.down_position(), ctx.bounds());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast and store the new widget
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            // Update callbacks from the new widget
            if let Some(gd) = widget.as_any().downcast_ref::<GestureDetector>() {
                self.on_press = gd.on_press.clone();
                self.on_release = gd.on_release.clone();
                self.on_tap = gd.on_tap.clone();
                self.on_secondary_press = gd.on_secondary_press.clone();
                self.on_long_press = gd.on_long_press.clone();
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
    /// Create a new pass-through GestureDetector render object.
    ///
    /// Pass-through: delegates layout to its child, generates no paint
    /// commands, hit tests using the child's computed bounds (adopted
    /// via `apply_layout`). Mirrors `DecoratedBoxRenderObject`.
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
        // Pass-through: return the child's node directly. No intervening
        // container — the grandparent links the grandchild's Taffy node.
        // Mirrors `DecoratedBoxRenderObject::layout`.
        match child_nodes.first() {
            Some(&child_node) => {
                self.layout_node = Some(child_node);
                LayoutResult {
                    node: child_node,
                    size: Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_leaf(&Layout::default());
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::zero(),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        true
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

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
        }
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
    use super::super::Key;
    use super::*;
    fn test_clipboard() -> std::sync::Arc<dyn crate::platform::Clipboard> {
        std::sync::Arc::new(crate::platform::stub_clipboard::StubClipboard)
    }

    use crate::Text;
    use std::cell::Cell;
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_gesture_detector_creation() {
        let gd = GestureDetector::new(Text::new("Click Me"));
        assert!(gd.key().is_none());
    }

    #[test]
    fn test_gesture_detector_with_key() {
        let gd = GestureDetector::new(Text::new("Click Me")).with_key("my-gesture");
        assert_eq!(gd.key(), Some(WidgetKey::Local(Key::new("my-gesture"))));
    }

    #[test]
    fn test_gesture_detector_with_callbacks() {
        let press_called = Rc::new(Cell::new(false));
        let release_called = Rc::new(Cell::new(false));
        let press_clone = press_called.clone();
        let release_clone = release_called.clone();

        let gd = GestureDetector::new(Text::new("Click Me"))
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

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(50.0, 25.0),
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
        assert!(result.is_some());
        assert!(called.get());
    }

    #[test]
    fn test_gesture_detector_element_event_release() {
        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();

        let mut elem = GestureDetectorElement::new();
        elem.on_release = Some(Rc::new(RefCell::new(move || called_clone.set(true))));

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(50.0, 25.0),
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Released,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
        assert!(result.is_some());
        assert!(called.get());
    }

    #[test]
    fn test_gesture_detector_element_event_outside_bounds() {
        let mut elem = GestureDetectorElement::new();
        elem.on_press = Some(Rc::new(RefCell::new(|| {})));

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(200.0, 200.0), // Outside bounds
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(200.0, 200.0),
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
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
        let gd = GestureDetector::new(Text::new("Hello")).on_press(|| {});

        let cloned = gd.clone();
        assert!(cloned.on_press.is_some());
    }

    #[test]
    fn test_gesture_detector_render_object_is_pass_through() {
        let widget = GestureDetector::new(Text::new("Hello"));
        let ro = widget.create_render_object();
        assert!(
            ro.is_pass_through(),
            "GestureDetector's render object must be pass-through"
        );
    }

    #[test]
    fn test_gesture_detector_layout_returns_child_node() {
        use crate::layout::TaffyLayoutEngine;
        let mut ro = GestureDetectorRenderObject::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        // Create a child Taffy node the way the pipeline would: by calling
        // engine.create_leaf and passing the key as a child_nodes entry.
        let child_node = ctx
            .engine()
            .create_leaf(&Layout::default().width(50.0).height(50.0));
        let result = ro.layout(&mut ctx, &[child_node]);

        assert_eq!(
            result.node, child_node,
            "layout() must return the child's node (pass-through)"
        );
        assert_eq!(
            ro.layout_node(),
            Some(child_node),
            "layout_node() must return the child's node after layout()"
        );
    }

    #[test]
    fn test_gesture_detector_layout_no_child_creates_throwaway_node() {
        use crate::layout::TaffyLayoutEngine;
        let mut ro = GestureDetectorRenderObject::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = ro.layout(&mut ctx, &[]);

        // Should not panic; should return some node and store it.
        assert!(ro.layout_node().is_some());
        assert_eq!(ro.layout_node(), Some(result.node));
    }

    #[test]
    fn test_on_secondary_press_fires_with_position() {
        let captured_pos = Rc::new(Cell::new(Point::<Logical>::new(-1.0, -1.0)));
        let captured_bounds = Rc::new(Cell::new(Bounds::<Logical>::new(0.0, 0.0, 0.0, 0.0)));
        let pos_clone = captured_pos.clone();
        let bounds_clone = captured_bounds.clone();

        let mut elem = GestureDetectorElement::new();
        elem.on_secondary_press = Some(Rc::new(RefCell::new(
            move |pos: Point<Logical>, bounds: Bounds<Logical>| {
                pos_clone.set(pos);
                bounds_clone.set(bounds);
            },
        )));

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        // Non-zero origin so a mistakenly-defaulted bounds would fail the
        // assertion. Event position (42, 17) is inside these bounds.
        let test_bounds = Bounds::from_xywh(5.0, 5.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            test_bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(42.0, 17.0),
            button: crate::input::PointerButton::Secondary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
        assert!(
            result.is_some(),
            "on_secondary_press should claim the event"
        );
        assert_eq!(captured_pos.get(), Point::new(42.0, 17.0));
        assert_eq!(
            captured_bounds.get(),
            test_bounds,
            "callback should receive the element's global bounds from context.bounds()"
        );
    }

    #[test]
    fn test_on_secondary_press_does_not_fire_on_primary() {
        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();

        let mut elem = GestureDetectorElement::new();
        elem.on_secondary_press = Some(Rc::new(RefCell::new(move |_pos, _bounds| {
            called_clone.set(true);
        })));

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(50.0, 25.0),
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
        assert!(!called.get(), "on_secondary_press must not fire on Primary");
        assert!(
            result.is_none(),
            "Primary with no on_press should not claim"
        );
    }

    #[test]
    fn test_secondary_press_skips_on_press_when_both_set() {
        let secondary_called = Rc::new(Cell::new(false));
        let press_called = Rc::new(Cell::new(false));
        let sec_clone = secondary_called.clone();
        let press_clone = press_called.clone();

        let mut elem = GestureDetectorElement::new();
        elem.on_secondary_press = Some(Rc::new(RefCell::new(move |_pos, _bounds| {
            sec_clone.set(true);
        })));
        elem.on_press = Some(Rc::new(RefCell::new(move || {
            press_clone.set(true);
        })));

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(50.0, 25.0),
            button: crate::input::PointerButton::Secondary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
        assert!(secondary_called.get(), "on_secondary_press should fire");
        assert!(!press_called.get(), "on_press should be skipped");
        assert!(result.is_some());
    }

    #[test]
    fn test_secondary_press_falls_through_to_on_press_when_not_set() {
        let press_called = Rc::new(Cell::new(false));
        let press_clone = press_called.clone();

        let mut elem = GestureDetectorElement::new();
        // No on_secondary_press set — only on_press.
        elem.on_press = Some(Rc::new(RefCell::new(move || {
            press_clone.set(true);
        })));

        let mut state = crate::StateStorage::new();
        let mut font_system = create_test_font_system();
        let bounds = Bounds::from_xywh(0.0, 0.0, 100.0, 50.0);
        let element_id = {
            let mut sm: slotmap::SlotMap<crate::id::ElementKey, ()> = slotmap::SlotMap::with_key();
            sm.insert(())
        };
        let mut ctx = EventContext::new(
            element_id,
            Point::new(50.0, 25.0),
            bounds,
            crate::input::Modifiers::default(),
            &mut font_system,
            None,
            test_clipboard(),
        );

        let event = InputEvent::PointerButton {
            position: Point::new(50.0, 25.0),
            button: crate::input::PointerButton::Secondary,
            state: ButtonState::Pressed,
        };

        let result = elem.on_event(&event, &mut ctx, &mut state);
        assert!(press_called.get(), "on_press should fire as fall-through");
        assert!(result.is_some());
    }

    #[test]
    fn test_widget_trait_on_secondary_press() {
        use crate::core::{Bounds, Logical};
        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();

        // Use the Widget trait method on a Text widget.
        let widget: Box<dyn Widget> = Text::new("Right-click me").on_secondary_press(
            move |_pos: Point<Logical>, _bounds: Bounds<Logical>| {
                called_clone.set(true);
            },
        );

        // Verify it wrapped in a GestureDetector.
        assert!(widget.as_any().downcast_ref::<GestureDetector>().is_some());
    }

    #[test]
    fn test_on_long_press_fires_after_500ms_via_tick_arena() {
        use crate::animation::AnimationTicker;
        use crate::core::ScaleSource;
        use crate::layout::TaffyLayoutEngine;
        use crate::pipeline::ThreeTreePipeline;
        use std::time::{Duration, Instant};

        let pressed = Rc::new(Cell::new(false));
        let press_pos = Rc::new(Cell::new(Point::new(0.0, 0.0)));
        let press_bounds = Rc::new(Cell::new(Bounds::new(0.0, 0.0, 0.0, 0.0)));
        let pressed_clone = pressed.clone();
        let pos_clone = press_pos.clone();
        let bounds_clone = press_bounds.clone();

        // A small tappable area. Layout gives it non-zero bounds, but
        // `tick_arena` does not hit-test on Tick, so the callback's `bounds`
        // argument will still be `Bounds::default()` (asserted below).
        let widget: Box<dyn Widget> = crate::DecoratedBox::with_style(
            crate::Text::new("Hold me"),
            crate::Style::default().background(crate::Color::WHITE),
        )
        .on_long_press(move |pos: Point<Logical>, bounds: Bounds<Logical>| {
            pressed_clone.set(true);
            pos_clone.set(pos);
            bounds_clone.set(bounds);
        });

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(widget);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Primary press inside the bubble.
        let press = InputEvent::PointerButton {
            position: Point::new(50.0, 30.0),
            button: crate::input::PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        let clipboard = test_clipboard();
        pipeline.handle_event(
            Point::new(50.0, 30.0),
            &press,
            crate::input::Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &clipboard,
        );

        // The LongPressRecognizer defers `down_time` from `Down` to the first
        // `Tick` (see gestures/long_press.rs). So the first Tick — sent at
        // `start` — records down_time and stays Pending (0ms < 500ms). Long-
        // press must NOT have fired yet.
        let start = Instant::now();
        pipeline.tick_arena(start, &mut font_system, &clipboard);
        pipeline.perform_rebuilds();
        assert!(!pressed.get(), "long-press must not fire before 500ms");

        // Second Tick at start + 500ms: elapsed >= 500ms → recognizer accepts
        // → arena resolves → on_arena_winner_update fires on_long_press with
        // the recognizer's down_position and `Bounds::default()` (tick_arena
        // does not hit-test on Tick — see pipeline.rs).
        pipeline.tick_arena(
            start + Duration::from_millis(500),
            &mut font_system,
            &clipboard,
        );
        pipeline.perform_rebuilds();

        assert!(pressed.get(), "long-press callback should fire after 500ms");
        assert_eq!(
            press_pos.get(),
            Point::new(50.0, 30.0),
            "position should be the press location"
        );
        // Bounds: `tick_arena` does not perform a hit-test on Tick, so it
        // dispatches with `Bounds::default()` (see pipeline.rs `tick_arena`).
        // The callback still receives the channel — verify it's the default
        // so a future change to pass real bounds trips this test deliberately.
        assert_eq!(
            press_bounds.get(),
            Bounds::default(),
            "bounds should be Bounds::default() — tick_arena does not hit-test on Tick"
        );
    }
}
