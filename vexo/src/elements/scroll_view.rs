//! ScrollViewElement - manages scroll state and handles input events.

use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::animation::{AnimationTicker, MomentumSimulation};
use crate::element::Element;
use crate::element_context::ElementContext;
use crate::element_state::StateStorage;
use crate::elements::RenderObjectElement;
use crate::event_context::EventContext;
use crate::focus::attachment::FocusAttachment;
use crate::gestures::{
    ArenaEvent, GestureArena, GestureRecognizer, VelocityTracker, VerticalDragRecognizer,
};
use crate::id::{ElementKey, RenderObjectKey};
use crate::input::{ButtonState, InputEvent, Key, NamedKey};
use crate::key::WidgetKey;
use crate::render_objects::ScrollViewRenderObject;
use crate::widgets::ScrollController;
use crate::widgets::Widget;

const LINE_HEIGHT: f32 = 40.0;

/// Apply iOS-style rubber-band resistance to a scroll offset.
///
/// When `raw_new` is within `[0, max]`, it passes through unchanged.
/// When past an edge, the over-edge portion is scaled by decreasing
/// resistance: `resistance = 1 - overscroll / (overscroll + viewport)`.
/// Content asymptotically approaches one viewport past the edge but
/// can never exceed it.
fn apply_rubber_band(raw_new: f32, viewport: f32, max: f32) -> f32 {
    let (base, excess) = if raw_new < 0.0 {
        (0.0, raw_new)
    } else if raw_new > max {
        (max, raw_new - max)
    } else {
        (raw_new, 0.0)
    };

    let overscroll = excess.abs();
    let resistance = 1.0 - overscroll / (overscroll + viewport.max(1.0));
    let resisted_excess = excess.signum() * overscroll * resistance;

    base + resisted_excess
}

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
    /// Tracks the last y position from the drag recognizer, to compute
    /// per-move scroll deltas. Set when the drag recognizer wins.
    last_drag_y: f32,
    /// Windowed least-squares pointer-velocity estimate. Sampled on every
    /// drag Move; read on Up to seed the momentum simulation's v0.
    velocity_tracker: VelocityTracker,
    /// Exponential-decay fling simulation. Drives inertial scroll after the
    /// pointer lifts. Stepped in `rebuild_from_state` while `is_active()`.
    momentum: MomentumSimulation,
    /// Stashed copy of the pipeline's animation ticker. `EventContext` does
    /// not expose it, so we capture it in `mount` (which has ElementContext)
    /// for use in the Up arm when starting momentum.
    animation_ticker: Option<Arc<AnimationTicker>>,
    /// Timestamp of the most recent drag Move. Used to detect a stale lift
    /// (finger paused >100ms before release) — the VelocityTracker retains
    /// 2 samples across a pause, so without this guard a pause-then-lift
    /// would fling on the pre-pause velocity.
    last_move_time: Option<Instant>,
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
            last_drag_y: 0.0,
            velocity_tracker: VelocityTracker::new(),
            momentum: MomentumSimulation::new(),
            animation_ticker: None,
            last_move_time: None,
        }
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
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

        if (new_offset - self.scroll_offset).abs() < f32::EPSILON {
            return false;
        }
        self.scroll_offset = new_offset;

        if let Some(ctrl) = self.controller.as_ref() {
            ctrl.set_current_offset(new_offset);
        }

        if let Some(rr) = ctx.render_objects() {
            if let Some(ro_key) = self.render_object {
                if let Some(ro) = rr.get(ro_key) {
                    if let Some(svro) = ro.as_any().downcast_ref::<ScrollViewRenderObject>() {
                        svro.set_scroll_offset(new_offset);
                    }
                }
            }
        }

        if let Some(bo) = ctx.build_owner() {
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
        // Stash the animation ticker for the Up arm. EventContext (used by
        // on_arena_winner_update) does not expose animation_ticker, so we
        // capture it here at mount time when ElementContext is available.
        self.animation_ticker = Some(context.animation_ticker.clone());
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
        // Stop any in-flight fling so its ticker callback is unregistered
        // before the element (and its render object) goes away. Without
        // this, the ticker would keep firing the dirty callback for a
        // dead element_id.
        self.momentum.stop();
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
            .downcast_ref::<crate::widgets::scroll_view::ScrollView>()
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
                position,
                ..
            } => {
                if context.bounds().contains(position) {
                    self.momentum.stop();
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

    fn register_gestures(&mut self, arena: &mut GestureArena, self_id: ElementKey) {
        arena.add(Box::new(VerticalDragRecognizer::new()), self_id);
    }

    fn on_arena_winner_update(
        &mut self,
        recognizer: &dyn GestureRecognizer,
        event: &ArenaEvent,
        ctx: &mut EventContext,
    ) {
        // Downcast to read the drag recognizer's position.
        let Some(drag) = recognizer.as_any().downcast_ref::<VerticalDragRecognizer>() else {
            return;
        };

        match event {
            ArenaEvent::Move { position } => {
                // Sample the pointer position into the velocity tracker FIRST,
                // so the timestamp reflects when the pointer was here, not
                // after the delta math below. The tracker keeps a 100ms window
                // of samples for least-squares velocity estimation on Up.
                let now = Instant::now();
                self.velocity_tracker.add(now, position.y);
                self.last_move_time = Some(now);
                // Compute scroll delta from the previous tracked position to
                // the current event position. We use `event.position` (not
                // `drag.last_position()`) because once the arena closes the
                // recognizer is no longer fed Move events, so its
                // `last_position` would be stale.
                let delta = self.last_drag_y - position.y;
                self.last_drag_y = position.y;
                let raw_new = self.scroll_offset + delta;
                let new_offset =
                    apply_rubber_band(raw_new, self.viewport_height, self.max_scroll());
                self.apply_scroll_offset(new_offset, ctx);
            }
            ArenaEvent::Down { .. } => {
                // Stop any in-flight fling BEFORE clearing the tracker, so a
                // new drag's samples can't race with an old fling's dirty
                // callback. This is one of the six termination conditions for
                // momentum: a fresh touch-down cancels inertia.
                self.momentum.stop();
                self.velocity_tracker.clear();
                self.last_move_time = None;
                // Drag just won (on the move that crossed slop). Initialize
                // last_drag_y from the recognizer's DOWN position so the
                // first Move delta captures the full movement from press-down
                // to current — matching Flutter's scroll-keeps-up-with-finger
                // behavior. (The event_handler only calls Down on the FIRST
                // winning move, so this runs once per drag.)
                self.last_drag_y = drag.down_position().y;
            }
            ArenaEvent::Up { .. } => {
                // Skip momentum if the last move was stale (pause-then-lift).
                // VelocityTracker retains 2 samples even across a pause, so
                // velocity() would return the pre-pause velocity. Guard
                // against that here.
                // Must match VelocityTracker::WINDOW (100ms). The tracker's len > 2
                // eviction guard retains stale samples across a pause; this check
                // ensures we don't read pre-pause velocity after a pause-then-lift.
                let is_stale = self
                    .last_move_time
                    .map(|t| Instant::now().duration_since(t) > Duration::from_millis(100))
                    .unwrap_or(true);
                if is_stale {
                    return;
                }
                // Sign-flip: the tracker returns pointer-space dy/dt (y-down).
                // The existing Move handler does `delta = last_drag_y -
                // position.y` (negates pointer delta) before applying to
                // scroll_offset, so an upward finger motion (dy/dt < 0)
                // produces positive offset delta. To scroll the same direction
                // after release, negate the tracker velocity so positive v0 =
                // offset increases = scrolls toward bottom.
                let v = -self.velocity_tracker.velocity();
                const V_MIN_FLING: f32 = 50.0;
                if v.abs() < V_MIN_FLING {
                    return;
                }
                let Some(element_id) = self.id else {
                    return;
                };
                let Some(tx) = ctx.dirty_sender().cloned() else {
                    return;
                };
                let Some(ticker) = self.animation_ticker.clone() else {
                    return;
                };
                self.momentum.start(
                    self.scroll_offset,
                    v,
                    Instant::now(),
                    tx,
                    element_id,
                    ticker,
                );
            }
            ArenaEvent::Cancel => {
                // Drag cancelled. No cleanup needed.
            }
        }
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
            // Programmatic jump (jump_to / jump_to_bottom) cancels any
            // in-flight fling — the user's intent overrides inertia.
            self.momentum.stop();
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

        if self.momentum.is_active() {
            let now = Instant::now();
            match self.momentum.advance(now) {
                Some(physics_offset) => {
                    let clamped = physics_offset.clamp(0.0, self.max_scroll());
                    let hit_edge = (clamped - physics_offset).abs() > f32::EPSILON;
                    if hit_edge {
                        self.momentum.stop();
                    }
                    if let Some(ro_key) = self.render_object {
                        if let Some(svro) = context
                            .render_objects
                            .get(ro_key)
                            .and_then(|ro| ro.as_any().downcast_ref::<ScrollViewRenderObject>())
                        {
                            self.viewport_height = svro.viewport_size().height;
                            self.content_height = svro.content_size().height;
                            svro.set_scroll_offset(clamped);
                        }
                    }
                    self.scroll_offset = clamped;
                    if let Some(ctrl) = self.controller.as_ref() {
                        ctrl.set_current_offset(clamped);
                    }
                    // The next frame's tick fires the dirty callback (registered
                    // in momentum.start), which sends element_id through the
                    // mpsc channel, which drain_dirty_to_build_owner picks up
                    // to schedule the next rebuild_from_state. No explicit
                    // mark_needs_build here.
                }
                None => {
                    self.momentum.stop();
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

    fn test_clipboard() -> std::sync::Arc<dyn crate::platform::Clipboard> {
        std::sync::Arc::new(crate::platform::stub_clipboard::StubClipboard)
    }

    #[test]
    fn test_rubber_band_no_resistance_in_bounds() {
        assert_eq!(apply_rubber_band(50.0, 400.0, 1000.0), 50.0);
    }

    #[test]
    fn test_rubber_band_no_resistance_at_exact_edge() {
        assert_eq!(apply_rubber_band(0.0, 400.0, 1000.0), 0.0);
        assert_eq!(apply_rubber_band(1000.0, 400.0, 1000.0), 1000.0);
    }

    #[test]
    fn test_rubber_band_shrinks_past_top() {
        let result = apply_rubber_band(-100.0, 400.0, 1000.0);
        assert!(
            result < 0.0,
            "should be past top (negative); got {}",
            result
        );
        assert!(
            result > -100.0,
            "should be resisted (less negative than raw); got {}",
            result
        );
        assert!(
            result > -400.0,
            "should not exceed viewport past edge; got {}",
            result
        );
    }

    #[test]
    fn test_rubber_band_shrinks_past_bottom() {
        let result = apply_rubber_band(1100.0, 400.0, 1000.0);
        assert!(result > 1000.0, "should be past bottom; got {}", result);
        assert!(
            result < 1100.0,
            "should be resisted (less than raw); got {}",
            result
        );
        assert!(
            result < 1400.0,
            "should not exceed viewport past edge; got {}",
            result
        );
    }

    #[test]
    fn test_rubber_band_asymptotic_at_viewport() {
        let result = apply_rubber_band(-10000.0, 400.0, 1000.0);
        assert!(
            result > -400.0,
            "content can never be dragged more than ~viewport past edge; got {}",
            result
        );
    }

    #[test]
    fn test_rubber_band_symmetric_top_bottom() {
        let top_result = apply_rubber_band(-100.0, 400.0, 1000.0);
        let bottom_result = apply_rubber_band(1100.0, 400.0, 1000.0);
        let top_excess = top_result.abs();
        let bottom_excess = (bottom_result - 1000.0).abs();
        assert!(
            (top_excess - bottom_excess).abs() < 0.01,
            "top and bottom excess should be symmetric; got top={} bottom={}",
            top_excess,
            bottom_excess
        );
    }

    #[test]
    fn test_rubber_band_zero_viewport_guarded() {
        // Should not panic on div-by-zero.
        let result = apply_rubber_band(-100.0, 0.0, 1000.0);
        assert!(result < 0.0, "should still be past top; got {}", result);
        assert!(
            result >= -100.0,
            "should not move more than raw; got {}",
            result
        );
    }

    #[test]
    fn test_drag_past_top_goes_negative() {
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::sync::Arc;

        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
        for _ in 0..200 {
            col = col.push(crate::Text::new("row"));
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
        // Press at (200, 300) inside the viewport.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        // Drag DOWN 200px (past slop, past top edge). Finger moves down →
        // scroll toward top → offset goes negative (overscroll).
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 500.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 500.0),
            &move_evt,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert!(
            ctrl.current_offset() < 0.0,
            "drag past top should produce negative offset (overscroll); got {}",
            ctrl.current_offset()
        );
    }

    #[test]
    fn test_drag_past_top_resists() {
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::sync::Arc;

        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
        for _ in 0..200 {
            col = col.push(crate::Text::new("row"));
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
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        // Drag DOWN 2000px (way past top). Without resistance, offset would
        // be -2000. With rubber-band, it should be much less (asymptote at
        // ~viewport=600).
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 2300.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 2300.0),
            &move_evt,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let offset = ctrl.current_offset();
        assert!(offset < 0.0, "should be past top; got {}", offset);
        assert!(
            offset > -600.0,
            "should not exceed ~viewport past edge (rubber-band); got {}",
            offset
        );
    }

    /// Standard test harness for momentum tests: 200-row scroll view, 400×600
    /// viewport, wired to a fresh ticker + pipeline. Returns the controller,
    /// ticker, pipeline, and font system — callers drive events through the
    /// pipeline and pump via `(ticker.tick(), pipeline.drain_dirty_to_build_owner(),
    /// pipeline.perform_rebuilds())`.
    fn setup_scroll_view(
        ctrl: &crate::widgets::ScrollController,
    ) -> (
        std::sync::Arc<crate::animation::AnimationTicker>,
        crate::ThreeTreePipeline,
        glyphon::FontSystem,
    ) {
        use crate::animation::AnimationTicker;
        use crate::widgets::ScrollView;
        use crate::{Layout, MultiChild};
        use std::sync::Arc;

        let mut col = MultiChild::empty(Layout::column());
        for _ in 0..200 {
            col = col.push(crate::Text::new("row"));
        }
        let sv = ScrollView::new(col.boxed()).controller(ctrl.clone());
        let ticker = Arc::new(AnimationTicker::new());
        let mut pipeline = crate::ThreeTreePipeline::new(ticker.clone());
        pipeline.reconcile(Box::new(sv));
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        (ticker, pipeline, font_system)
    }

    /// Dispatch a pointer event through the pipeline. Reduces per-test boilerplate.
    fn dispatch(
        pipeline: &mut crate::ThreeTreePipeline,
        font_system: &mut glyphon::FontSystem,
        position: crate::core::Point<crate::core::Logical>,
        event: &crate::input::InputEvent,
    ) {
        use crate::core::ScaleSource;
        use crate::input::Modifiers;
        pipeline.handle_event(
            position,
            event,
            Modifiers::default(),
            font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
    }

    /// Pump the ticker + pipeline one frame. Each pump fires the dirty callback
    /// → drains to build owner → performs rebuilds (which step momentum).
    fn pump(ticker: &crate::animation::AnimationTicker, pipeline: &mut crate::ThreeTreePipeline) {
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
    }

    /// Compute the exact max_scroll for the scroll view's render object by
    /// querying the pipeline's render object registry. Used by edge-clamp
    /// tests to assert exact boundary values rather than behavioral bounds.
    fn max_scroll_of(pipeline: &crate::ThreeTreePipeline) -> f32 {
        let ro_registry = pipeline.render_objects();
        // The scroll view's render object is the first ScrollViewRenderObject
        // in the registry (there's only one per test harness).
        for key in ro_registry.keys() {
            if let Some(ro) = ro_registry.get(key) {
                if let Some(svro) = ro.as_any().downcast_ref::<ScrollViewRenderObject>() {
                    let content = svro.content_size().height;
                    let viewport = svro.viewport_size().height;
                    return (content - viewport).max(0.0);
                }
            }
        }
        panic!("no ScrollViewRenderObject found in registry");
    }

    #[test]
    fn test_scroll_controller_wired_on_mount_via_pipeline() {
        use crate::animation::AnimationTicker;
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::sync::Arc;

        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
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

    #[test]
    fn test_drag_in_tappable_row_scrolls_not_navigates() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;

        // Build a scroll view of tappable rows (GestureDetector.on_tap).
        let tap_count = Rc::new(Cell::new(0u32));
        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
        for _ in 0..200 {
            let tc = tap_count.clone();
            col = col.push(
                crate::Text::new("row")
                    .boxed()
                    .on_tap(move || tc.set(tc.get() + 1)),
            );
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

        // Press at (200, 300) inside the viewport.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        // Drag UP 50px (past slop) → should scroll toward bottom, NOT tap.
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 250.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 250.0),
            &move_evt,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 250.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(200.0, 250.0),
            &release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(
            ctrl.current_offset() > 0.0,
            "drag should scroll; got offset={}",
            ctrl.current_offset()
        );
        assert_eq!(tap_count.get(), 0, "drag should NOT fire on_tap (navigate)");
    }

    #[test]
    fn test_tap_in_tappable_row_navigates_not_scrolls() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;

        let tap_count = Rc::new(Cell::new(0u32));
        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
        for _ in 0..200 {
            let tc = tap_count.clone();
            col = col.push(
                crate::Text::new("row")
                    .boxed()
                    .on_tap(move || tc.set(tc.get() + 1)),
            );
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

        // Press + Release with no move past slop → tap fires, no scroll.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert_eq!(tap_count.get(), 1, "tap should fire on_tap once");
        assert_eq!(ctrl.current_offset(), 0.0, "tap should NOT scroll");
    }

    #[test]
    fn test_drag_clamps_at_top_with_arena() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::sync::Arc;

        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
        for _ in 0..200 {
            col = col.push(crate::Text::new("row"));
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
        // Drag DOWN 1000px from offset 0 → clamp at 0.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 1300.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 1300.0),
            &move_evt,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        // With bounce enabled, dragging past top produces overscroll (negative
        // offset) rather than clamping at 0. The rubber-band resistance keeps
        // it bounded (~viewport past edge).
        let offset = ctrl.current_offset();
        assert!(offset <= 0.0, "should be at or past top; got {}", offset);
        assert!(
            offset > -600.0,
            "should not exceed ~viewport past edge; got {}",
            offset
        );
    }

    #[test]
    fn test_on_press_fires_on_down_regardless_of_drag_win() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;

        let press_count = Rc::new(Cell::new(0u32));
        let tap_count = Rc::new(Cell::new(0u32));
        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
        for _ in 0..200 {
            let pc = press_count.clone();
            let tc = tap_count.clone();
            col = col.push(
                crate::Text::new("row")
                    .boxed()
                    .on_press(move || pc.set(pc.get() + 1))
                    .on_tap(move || tc.set(tc.get() + 1)),
            );
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

        // Press → on_press fires immediately.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(press_count.get(), 1, "on_press fires on press-down");

        // Drag past slop → drag wins, tap rejected.
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 250.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 250.0),
            &move_evt,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 250.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(200.0, 250.0),
            &release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(press_count.get(), 1, "on_press stays at 1 (no double-fire)");
        assert_eq!(tap_count.get(), 0, "on_tap does NOT fire (drag won)");
        assert!(ctrl.current_offset() > 0.0, "drag scrolled");
    }

    #[test]
    fn test_tap_outside_scroll_view_unchanged() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::ThreeTreePipeline;
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;

        let tap_count = Rc::new(Cell::new(0u32));
        let tc = tap_count.clone();
        let widget = crate::Text::new("tap me")
            .boxed()
            .on_tap(move || tc.set(tc.get() + 1));

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(widget));
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(
            crate::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let press = InputEvent::PointerButton {
            position: Point::new(50.0, 20.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(50.0, 20.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let release = InputEvent::PointerButton {
            position: Point::new(50.0, 20.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(50.0, 20.0),
            &release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(tap_count.get(), 1, "tap fires outside scroll view");
    }

    #[test]
    fn test_mouse_wheel_still_works() {
        use crate::animation::AnimationTicker;
        use crate::core::ScaleSource;
        use crate::core::{Point, Size};
        use crate::input::{InputEvent, Modifiers};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::sync::Arc;

        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
        for i in 0..200 {
            col = col.push(crate::Text::new(format!("line {}", i)));
        }
        let sv = ScrollView::new(col.boxed()).controller(ctrl.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(sv));
        let mut engine = crate::layout::TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);
        let event = InputEvent::Scroll {
            position: Point::new(200.0, 300.0),
            delta: Point::new(0.0, -100.0), // scroll down 100px (negative y = down per winit/codebase convention)
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &event,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(
            ctrl.current_offset(),
            100.0,
            "mouse wheel still scrolls; existing path unchanged"
        );
    }

    #[test]
    fn test_multi_move_drag_accumulates_scroll() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::sync::Arc;

        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
        for _ in 0..200 {
            col = col.push(crate::Text::new("row"));
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

        // Press at (200, 300).
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        // Move 1: drag up 25px (crosses slop, drag wins).
        let move1 = InputEvent::PointerMoved {
            position: Point::new(200.0, 275.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 275.0),
            &move1,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let offset_after_move1 = ctrl.current_offset();
        assert!(
            offset_after_move1 > 0.0,
            "first move should scroll; got offset={}",
            offset_after_move1
        );
        // Move 2: drag up another 25px. This exercises Bug 2's fix: the arena
        // is already closed (drag won on move 1), so the recognizer is NOT fed
        // again. The scroll delta must come from event.position, not the stale
        // recognizer state. Without the fix, last_drag_y would be reset and the
        // delta would be 0 (or bogus).
        let move2 = InputEvent::PointerMoved {
            position: Point::new(200.0, 250.0),
        };
        pipeline.handle_event(
            Point::new(200.0, 250.0),
            &move2,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let offset_after_move2 = ctrl.current_offset();
        assert!(
            offset_after_move2 > offset_after_move1,
            "second move should scroll further; got offset={} after move1, {} after move2",
            offset_after_move1,
            offset_after_move2
        );
        // The total scroll should be roughly 50px (25 + 25), allowing for slop
        // adjustment on the first move.
        let total_delta = offset_after_move2 - offset_after_move1;
        assert!(
            total_delta > 0.0,
            "second move contributed positive scroll; got delta={}",
            total_delta
        );
    }

    #[test]
    fn test_cancel_on_blur_drops_arena() {
        use crate::animation::AnimationTicker;
        use crate::core::Point;
        use crate::core::ScaleSource;
        use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
        use crate::widgets::{ScrollController, ScrollView};
        use crate::ThreeTreePipeline;
        use crate::{Layout, MultiChild};
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;

        let tap_count = Rc::new(Cell::new(0u32));
        let ctrl = ScrollController::new();
        let mut col = MultiChild::empty(Layout::column());
        for _ in 0..200 {
            let tc = tap_count.clone();
            col = col.push(
                crate::Text::new("row")
                    .boxed()
                    .on_tap(move || tc.set(tc.get() + 1)),
            );
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

        // Press at (200, 300) inside the viewport — creates an arena.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        // Simulate window unfocus mid-press — cancels the arena.
        pipeline.cancel_current_gesture();

        // Release (would have completed the gesture) — should NOT fire tap
        // because the arena was cancelled.
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(
            tap_count.get(),
            0,
            "cancelled arena should not fire on_tap on subsequent release"
        );

        // Press again at a different location — should create a FRESH arena
        // and a normal tap should fire on release.
        let press2 = InputEvent::PointerButton {
            position: Point::new(200.0, 350.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 350.0),
            &press2,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        let release2 = InputEvent::PointerButton {
            position: Point::new(200.0, 350.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(200.0, 350.0),
            &release2,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert_eq!(
            tap_count.get(),
            1,
            "fresh arena after cancel should allow normal tap"
        );
    }

    #[test]
    fn test_fling_scrolls_after_release() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Press at (200, 300).
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 300.0),
            &press,
        );
        // Three fast upward moves over ~60ms (synthetic — real time will vary,
        // but VelocityTracker uses Instant::now() so we can't fake timestamps here).
        // We rely on the moves being fast enough in wall-clock to exceed V_MIN_FLING.
        for &y in &[290.0, 270.0, 240.0] {
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, y),
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, y), &mv);
        }
        let offset_at_release = ctrl.current_offset();
        assert!(
            offset_at_release > 0.0,
            "drag should have scrolled; got {}",
            offset_at_release
        );

        // Release.
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 240.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 240.0),
            &release,
        );

        // Pump once and verify momentum actually engaged (guards against
        // wall-clock slowness silently making the test tautological).
        pump(&ticker, &mut pipeline);
        assert!(
            ticker.has_active(),
            "momentum should have engaged after release; if this fails, the synthetic moves were too slow (wall-clock)"
        );
        for _ in 0..29 {
            pump(&ticker, &mut pipeline);
        }

        assert!(
            ctrl.current_offset() > offset_at_release,
            "momentum should have scrolled further after release; got {} after release, {} after pump",
            offset_at_release,
            ctrl.current_offset()
        );
    }

    #[test]
    fn test_pause_then_lift_no_momentum() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;
        use std::thread;
        use std::time::Duration;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Press at (200, 400).
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 400.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 400.0),
            &press,
        );
        // Three fast upward moves to build up velocity.
        for &y in &[350.0, 250.0, 150.0] {
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, y),
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, y), &mv);
        }
        let offset_at_release = ctrl.current_offset();
        assert!(
            offset_at_release > 0.0,
            "drag should have scrolled; got {}",
            offset_at_release
        );

        // Pause with finger still down: 200ms with no movement. This exceeds
        // the 100ms staleness guard in the Up arm, so the pre-pause velocity
        // must NOT seed a fling on release.
        thread::sleep(Duration::from_millis(200));

        // Release at the last move position.
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 150.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 150.0),
            &release,
        );

        // Pump the ticker + pipeline. If the staleness guard works, momentum
        // never starts and the offset stays frozen at offset_at_release.
        for _ in 0..10 {
            pump(&ticker, &mut pipeline);
        }

        assert_eq!(
            ctrl.current_offset(),
            offset_at_release,
            "pause-then-lift should NOT engage momentum; got {} before release, {} after pump",
            offset_at_release,
            ctrl.current_offset()
        );
    }

    #[test]
    fn test_fling_clamps_at_bottom_edge() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);
        let max_scroll = max_scroll_of(&pipeline);

        // Pre-scroll near the bottom so the fling only needs to cover a small
        // distance to reach the edge. This makes the test deterministic —
        // the fling's momentum displacement (v0·τ) easily covers the remaining
        // gap regardless of wall-clock timing.
        let target = (max_scroll - 500.0).max(0.0);
        ctrl.jump_to(target);
        for _ in 0..5 {
            pump(&ticker, &mut pipeline);
        }
        assert!(
            ctrl.current_offset() >= target - 1.0,
            "pre-scroll should have reached {}; got {}",
            target,
            ctrl.current_offset()
        );

        // Fling upward from the middle of the viewport.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 400.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 400.0),
            &press,
        );
        for &y in &[300.0, 200.0, 100.0] {
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, y),
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, y), &mv);
        }
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 100.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 100.0),
            &release,
        );

        // Pump long enough for the fling to hit the edge and clamp.
        for _ in 0..120 {
            pump(&ticker, &mut pipeline);
        }

        // Exact assertion: the fling should have clamped at max_scroll.
        assert_eq!(
            ctrl.current_offset(),
            max_scroll,
            "fling should clamp exactly at max_scroll ({})",
            max_scroll
        );
    }

    #[test]
    fn test_fling_clamps_at_top_edge() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);
        let max_scroll = max_scroll_of(&pipeline);

        // Pre-scroll to a known offset near the top so the fling only needs
        // to cover a small distance to reach offset 0. This makes the test
        // deterministic regardless of wall-clock timing.
        let start_offset = 500.0_f32.min(max_scroll);
        ctrl.jump_to(start_offset);
        for _ in 0..5 {
            pump(&ticker, &mut pipeline);
        }
        assert!(
            ctrl.current_offset() >= start_offset - 1.0,
            "pre-scroll should have reached {}; got {}",
            start_offset,
            ctrl.current_offset()
        );

        // Fling DOWNWARD (toward top): press at y=200, moves to y=300, 400, 500
        // (finger moves down → scroll toward top, offset decreases).
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 200.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 200.0),
            &press,
        );
        for &y in &[300.0, 400.0, 500.0] {
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, y),
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, y), &mv);
        }
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 500.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 500.0),
            &release,
        );

        // Pump long enough for the fling to hit the top edge and clamp.
        for _ in 0..120 {
            pump(&ticker, &mut pipeline);
        }

        // Exact assertion: the fling should have clamped at 0.0 (top edge).
        assert_eq!(
            ctrl.current_offset(),
            0.0,
            "downward fling should clamp exactly at top edge (0.0)"
        );
    }

    #[test]
    fn test_touch_down_stops_in_flight_momentum() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Start a fling.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 400.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 400.0),
            &press,
        );
        for &y in &[350.0, 250.0, 100.0] {
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, y),
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, y), &mv);
        }
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 100.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 100.0),
            &release,
        );

        // Pump once to let momentum start.
        pump(&ticker, &mut pipeline);
        assert!(
            ticker.has_active(),
            "momentum should have engaged after release; if this fails, the synthetic moves were too slow (wall-clock)"
        );
        let offset_mid_fling = ctrl.current_offset();

        // New touch Down — should stop momentum.
        let press2 = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 300.0),
            &press2,
        );

        // Pump several more times — offset should NOT change (momentum stopped).
        for _ in 0..10 {
            pump(&ticker, &mut pipeline);
        }
        assert_eq!(
            ctrl.current_offset(),
            offset_mid_fling,
            "touch Down should have stopped momentum; offset should be frozen"
        );
    }

    #[test]
    fn test_jump_to_cancels_momentum() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Start a fling.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 400.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 400.0),
            &press,
        );
        for &y in &[350.0, 250.0, 100.0] {
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, y),
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, y), &mv);
        }
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 100.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 100.0),
            &release,
        );

        // Pump once to let momentum start.
        pump(&ticker, &mut pipeline);
        assert!(
            ticker.has_active(),
            "momentum should have engaged after release; if this fails, the synthetic moves were too slow (wall-clock)"
        );

        // Immediately jump to a specific offset.
        ctrl.jump_to(50.0);

        // Pump — the jump should win, momentum should be cancelled.
        for _ in 0..10 {
            pump(&ticker, &mut pipeline);
        }

        assert_eq!(
            ctrl.current_offset(),
            50.0,
            "jump_to should have cancelled momentum and applied; got {}",
            ctrl.current_offset()
        );
    }

    #[test]
    fn test_slow_drag_no_momentum() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;
        use std::thread;
        use std::time::Duration;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Press.
        let press = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 300.0),
            &press,
        );
        // Slow moves: 10px each, 250ms apart → 40 px/s. Below V_MIN_FLING (50).
        for &y in &[290.0, 280.0, 270.0] {
            thread::sleep(Duration::from_millis(250));
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, y),
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, y), &mv);
        }
        let offset_at_release = ctrl.current_offset();
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 270.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 270.0),
            &release,
        );

        // Pump — no momentum should engage.
        for _ in 0..10 {
            pump(&ticker, &mut pipeline);
        }

        assert_eq!(
            ctrl.current_offset(),
            offset_at_release,
            "slow drag (below V_MIN_FLING) should not engage momentum; got {} before release, {} after pump",
            offset_at_release,
            ctrl.current_offset()
        );
    }
}
