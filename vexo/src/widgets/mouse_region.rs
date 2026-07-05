//! MouseRegion widget - invisible modifier that declares cursor intent and hover callbacks.
//!
//! This widget wraps a child and declares a mouse cursor (via MouseTrackerAnnotation)
//! that the MouseTracker resolves during hit testing. It follows Flutter's MouseRegion
//! pattern: invisible, no visual rendering, decouples cursor/hover from visual widgets.
//!
//! # Architecture
//!
//! MouseRegion is a modifier widget (like GestureDetector):
//! - Wraps a single child widget
//! - Has its own element (MouseRegionElement) for lifecycle
//! - Has a pass-through render object (invisible, delegates layout to child)
//! - Registers a MouseTrackerAnnotation on the render object during mount
//!
//! # Usage
//!
//! ```ignore
//! MouseRegion::new(Box::new(TextEditContent::new(...)))
//!     .cursor(MouseCursor::System(SystemCursorKind::Text))
//!     .on_enter(|| { log::info!("Mouse entered!"); })
//!     .on_exit(|| { log::info!("Mouse exited!"); })
//! ```

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::{Bounds, Logical, Point, Size};
use crate::input::{MouseCursor, MouseTrackerAnnotation};
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey};

use super::super::elements::RenderObjectElement;
use super::super::focus::attachment::FocusAttachment;
use super::super::key::WidgetKey;
use super::super::{
    ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectKey,
};
use super::{Element, Widget};

// ============================================================================
// MOUSE REGION WIDGET
// ============================================================================

/// Invisible modifier widget that declares cursor intent on its child.
///
/// MouseRegion wraps a child widget and provides cursor declaration and
/// hover callbacks. It has no visual representation — only the annotation
/// (registered on the render object) affects cursor resolution.
pub struct MouseRegion {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    cursor: MouseCursor,
    on_enter: Option<Rc<RefCell<dyn FnMut()>>>,
    on_exit: Option<Rc<RefCell<dyn FnMut()>>>,
    opaque: bool,
}

impl MouseRegion {
    /// Create a new MouseRegion wrapping a child widget with Defer cursor.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            cursor: MouseCursor::Defer,
            on_enter: None,
            on_exit: None,
            opaque: true,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the cursor intent (e.g., System(SystemCursorKind::Pointer) for clickable, Text for editable).
    pub fn cursor(mut self, cursor: MouseCursor) -> Self {
        self.cursor = cursor;
        self
    }

    /// Set the callback for hover enter events (pointer enters the child bounds).
    pub fn on_enter(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_enter = Some(Rc::new(RefCell::new(callback)));
        self
    }

    /// Set the callback for hover exit events (pointer leaves the child bounds).
    pub fn on_exit(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_exit = Some(Rc::new(RefCell::new(callback)));
        self
    }

    /// Set whether this region is opaque in hit testing (default: true).
    /// When opaque, it absorbs cursor resolution from descendants behind it.
    pub fn opaque(mut self, opaque: bool) -> Self {
        self.opaque = opaque;
        self
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }

    /// Build the annotation from current configuration.
    fn build_annotation(&self) -> MouseTrackerAnnotation {
        MouseTrackerAnnotation::new(self.cursor)
            .with_on_enter(
                self.on_enter
                    .clone()
                    .unwrap_or_else(|| Rc::new(RefCell::new(|| {}))),
            )
            .with_on_exit(
                self.on_exit
                    .clone()
                    .unwrap_or_else(|| Rc::new(RefCell::new(|| {}))),
            )
            .with_opaque(self.opaque)
    }
}

impl Clone for MouseRegion {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            cursor: self.cursor,
            on_enter: self.on_enter.clone(),
            on_exit: self.on_exit.clone(),
            opaque: self.opaque,
        }
    }
}

impl Widget for MouseRegion {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = MouseRegionElement::new();
        elem.set_widget_from_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(MouseRegionRenderObject::new())
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
// MOUSE REGION ELEMENT
// ============================================================================

/// Element for MouseRegion widget - registers annotation on mount.
pub struct MouseRegionElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    cursor: MouseCursor,
    on_enter: Option<Rc<RefCell<dyn FnMut()>>>,
    on_exit: Option<Rc<RefCell<dyn FnMut()>>>,
    opaque: bool,
    focus_attachment: Option<FocusAttachment>,
}

impl MouseRegionElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            cursor: MouseCursor::Defer,
            on_enter: None,
            on_exit: None,
            opaque: true,
            focus_attachment: None,
        }
    }

    fn set_widget_from_widget(&mut self, widget: &MouseRegion) {
        self.key = widget.key.clone();
        self.cursor = widget.cursor;
        self.on_enter = widget.on_enter.clone();
        self.on_exit = widget.on_exit.clone();
        self.opaque = widget.opaque;
        self.widget = Some(widget.clone_boxed());
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }

    /// Register the annotation on the render object in the registry.
    fn register_annotation(&self, context: &mut ElementContext) {
        if let Some(ro_key) = self.render_object {
            let annotation = MouseTrackerAnnotation::new(self.cursor)
                .with_on_enter(
                    self.on_enter
                        .clone()
                        .unwrap_or_else(|| Rc::new(RefCell::new(|| {}))),
                )
                .with_on_exit(
                    self.on_exit
                        .clone()
                        .unwrap_or_else(|| Rc::new(RefCell::new(|| {}))),
                )
                .with_opaque(self.opaque);
            context
                .render_objects
                .set_cursor_annotation(ro_key, annotation);
        }
    }
}

impl Default for MouseRegionElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for MouseRegionElement {
    fn widget(&self) -> Option<&dyn Widget> {
        self.widget.as_deref()
    }

    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Some(mr) = widget.as_any().downcast_ref::<MouseRegion>() {
            self.key = mr.key.clone();
            self.cursor = mr.cursor;
            self.on_enter = mr.on_enter.clone();
            self.on_exit = mr.on_exit.clone();
            self.opaque = mr.opaque;
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

impl Element for MouseRegionElement {
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

        // Register the annotation on the render object after mount
        self.register_annotation(context);

        if let Some(child_widget) = self.get_child_widget() {
            context.inflate_child(None, child_widget.clone_boxed());
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update_render_object(new_widget, context);

        // Re-register annotation if cursor/opaque changed
        self.register_annotation(context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        // Remove the annotation before unmounting the render object
        if let Some(ro_key) = self.render_object {
            context.render_objects.remove_cursor_annotation(ro_key);
        }

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
        _event: &crate::input::InputEvent,
        _context: &mut EventContext,
        _state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        // MouseRegion doesn't handle pointer events directly —
        // cursor resolution and hover dispatch happen in the pipeline.
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            if let Some(mr) = widget.as_any().downcast_ref::<MouseRegion>() {
                self.cursor = mr.cursor;
                self.on_enter = mr.on_enter.clone();
                self.on_exit = mr.on_exit.clone();
                self.opaque = mr.opaque;
            }
            self.widget = Some(*widget);

            if let Some(child_widget) = self.get_child_widget() {
                let old_child = context.children().first().copied();
                match old_child {
                    Some(old_child_key) => {
                        context.update_child(old_child_key, child_widget.clone_boxed());
                    }
                    None => {
                        context.inflate_child(None, child_widget.clone_boxed());
                    }
                }
            } else if let Some(old_child_key) = context.children().first().copied() {
                context.unmount_child(old_child_key);
            }
        }

        // Re-register annotation after rebuild
        self.register_annotation(context);

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
// MOUSE REGION RENDER OBJECT
// ============================================================================

/// Pass-through render object for MouseRegion - invisible.
///
/// Same as GestureDetectorRenderObject: delegates layout to child,
/// generates no paint commands, hit tests using computed bounds.
/// The annotation lives on the registry, not on this render object.
pub struct MouseRegionRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl MouseRegionRenderObject {
    pub fn new() -> Self {
        Self {
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }
}

impl Default for MouseRegionRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for MouseRegionRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch);
        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &layout);
                ctx.engine().set_children(existing, child_nodes);
                LayoutResult {
                    node: existing,
                    size: Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_container(&layout, child_nodes);
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

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
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
