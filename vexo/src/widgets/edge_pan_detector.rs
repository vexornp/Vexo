//! EdgePanDetector — invisible modifier that detects leading-edge pan gestures.
//!
//! Wraps a child and fires `on_start` / `on_update(delta_x)` / `on_end(delta_x)`
//! when an `EdgePanRecognizer` wins the arena. When `enabled == false`, no
//! recognizer is registered and the widget is a pure pass-through wrapper
//! (stable widget type — no reconciler remount when toggling).
//!
//! Mirrors `GestureDetector`'s element/render-object plumbing. The render
//! object is pass-through (invisible, delegates layout to child).

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::{Bounds, Logical, Point, Size};
use crate::gestures::{ArenaEvent, EdgePanRecognizer, GestureArena, GestureRecognizer};
use crate::input::InputEvent;
use crate::layout::{Layout, LayoutNodeKey};

use super::super::elements::RenderObjectElement;
use super::super::focus::attachment::FocusAttachment;
use super::super::key::WidgetKey;
use super::super::{
    ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectKey,
};
use super::{Element, Widget};

pub struct EdgePanDetector {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    enabled: bool,
    on_start: Option<Rc<RefCell<dyn FnMut()>>>,
    on_update: Option<Rc<RefCell<dyn FnMut(f32)>>>,
    on_end: Option<Rc<RefCell<dyn FnMut(f32)>>>,
}

impl EdgePanDetector {
    pub fn new(child: impl Widget + 'static, enabled: bool) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            enabled,
            on_start: None,
            on_update: None,
            on_end: None,
        }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn on_start(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_start = Some(Rc::new(RefCell::new(callback)));
        self
    }

    pub fn on_update(mut self, callback: impl FnMut(f32) + 'static) -> Self {
        self.on_update = Some(Rc::new(RefCell::new(callback)));
        self
    }

    pub fn on_end(mut self, callback: impl FnMut(f32) + 'static) -> Self {
        self.on_end = Some(Rc::new(RefCell::new(callback)));
        self
    }

    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }
}

impl Clone for EdgePanDetector {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            enabled: self.enabled,
            on_start: self.on_start.clone(),
            on_update: self.on_update.clone(),
            on_end: self.on_end.clone(),
        }
    }
}

impl Widget for EdgePanDetector {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = EdgePanDetectorElement::new();
        elem.set_widget_from_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(EdgePanDetectorRenderObject::new())
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

pub struct EdgePanDetectorElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    enabled: bool,
    on_start: Option<Rc<RefCell<dyn FnMut()>>>,
    on_update: Option<Rc<RefCell<dyn FnMut(f32)>>>,
    on_end: Option<Rc<RefCell<dyn FnMut(f32)>>>,
    focus_attachment: Option<FocusAttachment>,
}

impl EdgePanDetectorElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            enabled: false,
            on_start: None,
            on_update: None,
            on_end: None,
            focus_attachment: None,
        }
    }

    fn set_widget_from_widget(&mut self, widget: &EdgePanDetector) {
        self.key = widget.key.clone();
        self.enabled = widget.enabled;
        self.on_start = widget.on_start.clone();
        self.on_update = widget.on_update.clone();
        self.on_end = widget.on_end.clone();
        self.widget = Some(widget.clone_boxed());
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for EdgePanDetectorElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for EdgePanDetectorElement {
    fn widget(&self) -> Option<&dyn Widget> {
        self.widget.as_deref()
    }

    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Some(epd) = widget.as_any().downcast_ref::<EdgePanDetector>() {
            self.key = epd.key.clone();
            self.enabled = epd.enabled;
            self.on_start = epd.on_start.clone();
            self.on_update = epd.on_update.clone();
            self.on_end = epd.on_end.clone();
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

impl Element for EdgePanDetectorElement {
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
        if let Some(child_widget) = self.get_child_widget() {
            context.inflate_child(None, child_widget.clone_boxed());
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

    fn register_gestures(&mut self, arena: &mut GestureArena, self_id: ElementKey) {
        if self.enabled {
            arena.add(Box::new(EdgePanRecognizer::new()), self_id);
        }
    }

    fn on_arena_winner_update(
        &mut self,
        recognizer: &dyn GestureRecognizer,
        event: &ArenaEvent,
        _ctx: &mut EventContext,
    ) {
        let Some(ep) = recognizer.as_any().downcast_ref::<EdgePanRecognizer>() else {
            return;
        };
        match event {
            ArenaEvent::Down { .. } => {
                if let Some(callback) = &self.on_start {
                    (callback.borrow_mut())();
                }
            }
            ArenaEvent::Move { .. } => {
                let delta_x = ep.total_delta_x();
                if let Some(callback) = &self.on_update {
                    (callback.borrow_mut())(delta_x);
                }
            }
            ArenaEvent::Up { .. } => {
                let delta_x = ep.total_delta_x();
                if let Some(callback) = &self.on_end {
                    (callback.borrow_mut())(delta_x);
                }
            }
            _ => {}
        }
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            if let Some(epd) = widget.as_any().downcast_ref::<EdgePanDetector>() {
                self.enabled = epd.enabled;
                self.on_start = epd.on_start.clone();
                self.on_update = epd.on_update.clone();
                self.on_end = epd.on_end.clone();
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

pub struct EdgePanDetectorRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl EdgePanDetectorRenderObject {
    pub fn new() -> Self {
        Self {
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }
}

impl Default for EdgePanDetectorRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for EdgePanDetectorRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
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
        vec![]
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
