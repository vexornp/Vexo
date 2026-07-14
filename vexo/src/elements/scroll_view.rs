//! ScrollViewElement - manages scroll state and handles input events.

use std::any::Any;
use std::sync::Arc;

use crate::element::Element;
use crate::element_context::ElementContext;
use crate::element_state::StateStorage;
use crate::elements::RenderObjectElement;
use crate::event_context::EventContext;
use crate::focus::attachment::FocusAttachment;
use crate::id::{ElementKey, RenderObjectKey};
use crate::input::{ButtonState, InputEvent, Key, NamedKey};
use crate::key::WidgetKey;
use crate::render_objects::ScrollViewRenderObject;
use crate::widgets::ScrollController;
use crate::widgets::Widget;

const LINE_HEIGHT: f32 = 40.0;

/// Wire a `ScrollController`'s dirty callback to the pipeline's mpsc channel.
///
/// Matches the `StatefulElement` dirty-callback pattern
/// (`stateful_widget.rs:567-570`): clones the `mpsc::Sender` directly into the
/// closure. `Sender: Send + Sync` since Rust 1.71, so no `Mutex` is needed.
fn wire_dirty_callback(ctrl: &ScrollController, context: &ElementContext) {
    let tx = context.dirty_sender.clone();
    let element_id = context.element_id;
    ctrl.set_dirty_callback(Arc::new(move || {
        let _ = tx.send(element_id);
    }));
}

pub struct ScrollViewElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
    scroll_offset: f32,
    content_height: f32,
    viewport_height: f32,
    controller: Option<ScrollController>,
}

impl ScrollViewElement {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
            scroll_offset: 0.0,
            content_height: 0.0,
            viewport_height: 0.0,
            controller: None,
        }
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    fn clamp_offset(&self, offset: f32) -> f32 {
        offset.clamp(0.0, self.max_scroll())
    }

    fn apply_scroll_offset(&mut self, new_offset: f32, ctx: &EventContext) -> bool {
        if let Some(rr) = ctx.render_objects() {
            if let Some(ro_key) = self.render_object {
                if let Some(ro) = rr.get(ro_key) {
                    if let Some(svro) = ro.as_any().downcast_ref::<ScrollViewRenderObject>() {
                        self.viewport_height = svro.viewport_size().height;
                        self.content_height = svro.content_size().height;
                    }
                }
            }
        }

        let clamped = self.clamp_offset(new_offset);
        if (clamped - self.scroll_offset).abs() < f32::EPSILON {
            return false;
        }
        self.scroll_offset = clamped;

        if let Some(ctrl) = self.controller.as_ref() {
            ctrl.set_current_offset(clamped);
        }

        if let Some(rr) = ctx.render_objects() {
            if let Some(ro_key) = self.render_object {
                if let Some(ro) = rr.get(ro_key) {
                    if let Some(svro) = ro.as_any().downcast_ref::<ScrollViewRenderObject>() {
                        svro.set_scroll_offset(clamped);
                    }
                }
            }
        }

        if let Some(bo) = ctx.build_owner {
            bo.mark_needs_build(ctx.element_id());
        }
        true
    }

    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for ScrollViewElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for ScrollViewElement {
    fn widget(&self) -> Option<&dyn Widget> {
        self.widget.as_deref()
    }
    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        if let Some(sv) = widget
            .as_any()
            .downcast_ref::<crate::widgets::scroll_view::ScrollView>()
        {
            self.key = sv.key().clone();
            self.controller = sv.controller_ref().cloned();
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

impl Element for ScrollViewElement {
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

        if let Some(ctrl) = self.controller.as_ref() {
            wire_dirty_callback(ctrl, context);
        }

        if let Some(child_widget) = self.get_child_widget() {
            context.inflate_child(None, child_widget.clone_boxed());
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Capture the old controller before set_widget replaces it, so we can
        // detect a controller swap and re-wire the dirty callback. Mirrors
        // TextEditState::on_update (text_edit.rs:320-329) which compares
        // controllers via Rc::ptr_eq.
        let old_controller = self.controller.clone();
        self.update_render_object(new_widget, context);
        match (&old_controller, &self.controller) {
            (Some(old), Some(new)) if !old.is_same_instance(new) => {
                old.clear_dirty_callback();
                wire_dirty_callback(new, context);
            }
            (Some(old), None) => {
                old.clear_dirty_callback();
            }
            (None, Some(new)) => {
                wire_dirty_callback(new, context);
            }
            _ => {}
        }
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        if let Some(ctrl) = self.controller.as_ref() {
            ctrl.clear_dirty_callback();
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
        widget
            .downcast_ref::<Box<dyn Widget>>()
            .and_then(|w| {
                w.as_any()
                    .downcast_ref::<crate::widgets::scroll_view::ScrollView>()
            })
            .is_some()
    }

    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut EventContext,
        _state: &mut StateStorage,
    ) -> Option<Box<dyn Any>> {
        match event {
            InputEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } => {
                if context.is_pointer_inside() {
                    context.request_focus(context.element_id());
                    return Some(Box::new(()));
                }
            }

            InputEvent::Scroll { delta, .. } => {
                let new_offset = self.scroll_offset - delta.y;
                self.apply_scroll_offset(new_offset, context);
                return Some(Box::new(()));
            }

            InputEvent::Keyboard {
                key,
                state: ButtonState::Pressed,
                ..
            } => {
                let delta = match key {
                    Key::Named(NamedKey::ArrowUp) => Some(-LINE_HEIGHT),
                    Key::Named(NamedKey::ArrowDown) => Some(LINE_HEIGHT),
                    Key::Named(NamedKey::PageUp) => Some(-self.viewport_height),
                    Key::Named(NamedKey::PageDown) => Some(self.viewport_height),
                    Key::Named(NamedKey::Home) => Some(-self.scroll_offset),
                    Key::Named(NamedKey::End) => Some(self.max_scroll() - self.scroll_offset),
                    _ => None,
                };
                if let Some(d) = delta {
                    self.apply_scroll_offset(self.scroll_offset + d, context);
                    return Some(Box::new(()));
                }
            }

            _ => {}
        }
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            if let Some(sv) = widget
                .as_any()
                .downcast_ref::<crate::widgets::scroll_view::ScrollView>()
            {
                self.key = sv.key().clone();
            }
            self.widget = Some(*widget);

            if let Some(child_widget) = self.get_child_widget() {
                let old_child = context.children().first().copied();
                match old_child {
                    Some(old_child_key) => {
                        context.update_child(old_child_key, child_widget.clone_boxed())
                    }
                    None => context.inflate_child(None, child_widget.clone_boxed()),
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

    fn rebuild_from_state(&mut self, context: &mut ElementContext) {
        // Deferred-apply: consume any pending target offset from the controller
        // (set by jump_to_bottom / jump_to). The controller's dirty callback
        // sent this element's ID through the pipeline's mpsc channel, which
        // the pipeline drained into the BuildOwner, scheduling this rebuild.
        // Here we have safe `&mut RenderObjectRegistry` access — no raw
        // pointers needed.
        let pending = self
            .controller
            .as_ref()
            .and_then(|ctrl| ctrl.take_target_offset());

        if let Some(target) = pending {
            if let Some(ro_key) = self.render_object {
                if let Some(svro) = context
                    .render_objects
                    .get(ro_key)
                    .and_then(|ro| ro.as_any().downcast_ref::<ScrollViewRenderObject>())
                {
                    self.viewport_height = svro.viewport_size().height;
                    self.content_height = svro.content_size().height;
                    let max = self.max_scroll();
                    let clamped = if target.is_infinite() {
                        max
                    } else {
                        target.clamp(0.0, max)
                    };
                    svro.set_scroll_offset(clamped);
                    self.scroll_offset = clamped;
                    if let Some(ctrl) = self.controller.as_ref() {
                        ctrl.set_current_offset(clamped);
                    }
                }
            }
        }

        if let Some(ro_key) = self.render_object {
            context.mark_needs_paint(ro_key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_offset_at_zero() {
        let elem = ScrollViewElement::new();
        assert_eq!(elem.clamp_offset(-10.0), 0.0);
    }

    #[test]
    fn test_clamp_offset_at_max() {
        let mut elem = ScrollViewElement::new();
        elem.content_height = 500.0;
        elem.viewport_height = 100.0;
        assert_eq!(elem.clamp_offset(450.0), 400.0);
    }

    #[test]
    fn test_no_scroll_when_content_fits() {
        let mut elem = ScrollViewElement::new();
        elem.content_height = 300.0;
        elem.viewport_height = 500.0;
        assert_eq!(elem.max_scroll(), 0.0);
        assert_eq!(elem.clamp_offset(100.0), 0.0);
    }

    #[test]
    fn test_scroll_controller_wired_on_mount_via_pipeline() {
        use crate::animation::AnimationTicker;
        use crate::widgets::{ScrollController, ScrollView};
        use crate::Flex;
        use crate::ThreeTreePipeline;
        use std::sync::Arc;

        let ctrl = ScrollController::new();
        let mut col = Flex::column();
        for i in 0..200 {
            col = col.push(crate::Text::new(format!("line {}", i)));
        }
        let sv = ScrollView::new(col.boxed()).controller(ctrl.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(sv));
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // jump_to_bottom defers: stores target + fires dirty callback (sends
        // element_id through the pipeline's mpsc channel). The offset is NOT
        // applied yet — current_offset() still reads 0.0.
        ctrl.jump_to_bottom();
        assert_eq!(
            ctrl.current_offset(),
            0.0,
            "offset not applied until pipeline pumps rebuild"
        );

        // Pump: drain dirty channel into BuildOwner, then run rebuilds.
        // rebuild_from_state consumes the pending target, computes max_scroll
        // live from the render object, clamps, and applies via set_scroll_offset.
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();

        assert!(
            ctrl.current_offset() > 0.0,
            "after pump, deferred jump_to_bottom applied"
        );
    }
}
