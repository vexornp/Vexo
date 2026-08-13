//! ScrollViewElement - manages scroll state and handles input events.

use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::animation::simulation::{Simulation, SpringSimulation as SpringMath};
use crate::animation::{AnimationTicker, FrictionSimulation, TickHandle};
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
use crate::widgets::scroll_view::ScrollPhysics;
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

/// Inverse of `apply_rubber_band`: given a rubber-banded (displayed) offset,
/// recover the unresisted offset that produces it. Used to initialize the
/// drag's unresisted tracking offset from the current displayed offset when
/// a drag starts — including from overscroll (e.g. after interrupting a
/// spring), so the first drag delta doesn't cause a visual jump.
fn invert_rubber_band(displayed: f32, viewport: f32, max: f32) -> f32 {
    let v = viewport.max(1.0);
    if displayed < 0.0 {
        displayed * v / (v + displayed)
    } else if displayed > max {
        let e_d = displayed - max;
        max + e_d * v / (v - e_d)
    } else {
        displayed
    }
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

/// Active physics drive for scroll. One sim active at a time; starting one
/// stops the other (preserves the old momentum/spring mutual-exclusion).
enum ScrollDrive {
    Idle,
    Fling {
        sim: FrictionSimulation,
        start: Instant,
    },
    Bounce {
        sim: SpringMath,
        start: Instant,
        /// Rest/target offset for snap-to-rest on settle. The new analytic
        /// `SpringSimulation` doesn't expose its `to` field, so the element
        /// stashes it here (mirrors the old `self.spring.rest()` accessor).
        rest: f32,
    },
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
    /// Unresisted scroll offset during a drag — accumulates the full finger
    /// delta 1:1. The displayed offset is `apply_rubber_band(unresisted)`.
    /// Tracking the unresisted value separately (rather than rubber-banding
    /// the accumulated `scroll_offset + delta`) ensures that dragging back
    /// past an edge exactly reverses the rubber-band, instead of jumping
    /// forward — the old approach double-resisted outward movement but let
    /// return movement through at 1:1, so the content overshot its expected
    /// position mid-drag (looked like a "bounce back" with the finger still
    /// down).
    drag_unresisted_offset: f32,
    /// Windowed least-squares pointer-velocity estimate. Sampled on every
    /// drag Move; read on Up to seed the momentum simulation's v0.
    velocity_tracker: VelocityTracker,
    /// Active scroll physics drive. `Idle` when at rest. `Fling`/`Bounce`
    /// source math from the new pure-math sims; the ticker/dirty plumbing
    /// stays here (ScrollView can't use AnimationController::animate_with
    /// because it operates in px and needs mid-flight velocity handoff).
    drive: ScrollDrive,
    /// Stashed physics config (from the widget). Replaces the old module-level
    /// `const STIFFNESS`/`TAU`/etc.
    physics: ScrollPhysics,
    /// Stashed copy of the pipeline's animation ticker. `EventContext` does
    /// not expose it, so we capture it in `mount` (which has ElementContext)
    /// for use in the Up arm when starting momentum.
    animation_ticker: Option<Arc<AnimationTicker>>,
    /// Ticker registration handle for the currently-active drive. The old
    /// `MomentumSimulation`/`SpringSimulation` each held their own handle;
    /// now one shared handle lives on the element.
    tick_handle: Option<TickHandle>,
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
            drag_unresisted_offset: 0.0,
            velocity_tracker: VelocityTracker::new(),
            drive: ScrollDrive::Idle,
            physics: ScrollPhysics::default(),
            animation_ticker: None,
            tick_handle: None,
            last_move_time: None,
        }
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    /// Refresh `viewport_height` / `content_height` from the render object.
    /// Must be called before `max_scroll()` is used for clamping in paths
    /// that don't go through `apply_scroll_offset` first (e.g. the wheel
    /// and keyboard arms clamp before applying).
    fn refresh_sizes(&mut self, ctx: &EventContext) {
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
    }

    fn apply_scroll_offset(&mut self, new_offset: f32, ctx: &EventContext) -> bool {
        self.refresh_sizes(ctx);

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

    /// Register a ticker callback that sends this element's id through the
    /// dirty channel. Returns the handle. Shared by the Fling/Bounce start
    /// paths — replaces the old sims' built-in `start(...tx, element_id, ticker)`.
    fn register_ticker(
        &mut self,
        tx: std::sync::mpsc::Sender<ElementKey>,
        ticker: Arc<AnimationTicker>,
    ) -> Option<TickHandle> {
        let element_id = self.id?;
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = tx.send(element_id);
        });
        Some(ticker.register(cb))
    }

    /// Unregister the active ticker callback (if any). Mirrors the old
    /// `MomentumSimulation::stop` / `SpringSimulation::stop` cleanup.
    fn unregister_ticker(&mut self) {
        if let (Some(ticker), Some(handle)) =
            (self.animation_ticker.clone(), self.tick_handle.take())
        {
            ticker.unregister(handle);
        }
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

/// Safety net mirroring the retired `MomentumSimulation`/`SpringSimulation`
/// `Drop` impls: if the element is dropped while a drive is still active
/// (e.g. tree torn down mid-fling without `unmount`), unregister the ticker
/// callback so it can't fire for a dead `element_id`. No-op when idle.
impl Drop for ScrollViewElement {
    fn drop(&mut self) {
        if self.tick_handle.is_some() {
            self.unregister_ticker();
        }
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
            self.physics = sv.physics_ref();
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
        self.unregister_ticker();
        self.drive = ScrollDrive::Idle;
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
                    self.unregister_ticker();
                    self.drive = ScrollDrive::Idle;
                    context.request_focus(context.element_id());
                    return Some(Box::new(()));
                }
            }

            InputEvent::Scroll { delta, .. } => {
                self.unregister_ticker();
                self.drive = ScrollDrive::Idle;
                self.refresh_sizes(context);
                let new_offset = (self.scroll_offset - delta.y).clamp(0.0, self.max_scroll());
                self.apply_scroll_offset(new_offset, context);
                return Some(Box::new(()));
            }

            InputEvent::Keyboard {
                key,
                state: ButtonState::Pressed,
                ..
            } => {
                self.unregister_ticker();
                self.drive = ScrollDrive::Idle;
                self.refresh_sizes(context);
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
                    let new_offset = (self.scroll_offset + d).clamp(0.0, self.max_scroll());
                    self.apply_scroll_offset(new_offset, context);
                    return Some(Box::new(()));
                }
            }

            _ => {}
        }
        None
    }

    fn register_gestures(&mut self, arena: &mut GestureArena, self_id: ElementKey) {
        // Stop any in-flight momentum/spring on press. register_gestures is
        // called on EVERY element in the hit path during the press phase,
        // BEFORE on_event bubbling. This matters because child GestureDetectors
        // return Some(()) on press (stopping propagation), which would prevent
        // the ScrollView's on_event Pressed handler from firing. Without this,
        // the spring would keep advancing during drag — content bouncing back
        // while the finger is still down.
        self.unregister_ticker();
        self.drive = ScrollDrive::Idle;
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
                // Sync viewport/content sizes from the render object BEFORE
                // the rubber-band calc below. Without this, the first Move
                // after mount reads stale viewport_height=0, which makes
                // apply_rubber_band's denominator collapse to 1.0 (the
                // `viewport.max(1.0)` guard) and over-compress overscroll to
                // ~1px — small enough to fall inside SpringSimulation's
                // X_SETTLE threshold so a release-in-overscroll spring would
                // settle on its first advance. (apply_scroll_offset also
                // calls refresh_sizes, but that runs AFTER the rubber-band
                // computation that needs the correct viewport.)
                self.refresh_sizes(ctx);
                // Sample the pointer position into the velocity tracker FIRST,
                // so the timestamp reflects when the pointer was here, not
                // after the delta math below. The tracker keeps a 100ms window
                // of samples for least-squares velocity estimation on Up.
                let now = Instant::now();
                self.velocity_tracker.add(now, position.y);
                self.last_move_time = Some(now);
                // Accumulate the full finger delta into the unresisted offset
                // (1:1 tracking), then rubber-band it to get the displayed
                // offset. This ensures dragging back past an edge exactly
                // reverses the rubber-band instead of jumping forward.
                let delta = self.last_drag_y - position.y;
                self.last_drag_y = position.y;
                self.drag_unresisted_offset += delta;
                let new_offset = apply_rubber_band(
                    self.drag_unresisted_offset,
                    self.viewport_height,
                    self.max_scroll(),
                );
                self.apply_scroll_offset(new_offset, ctx);
            }
            ArenaEvent::Down { .. } => {
                // Stop any in-flight fling BEFORE clearing the tracker, so a
                // new drag's samples can't race with an old fling's dirty
                // callback. This is one of the six termination conditions for
                // momentum: a fresh touch-down cancels inertia.
                self.unregister_ticker();
                self.drive = ScrollDrive::Idle;
                self.velocity_tracker.clear();
                self.last_move_time = None;
                // Initialize the unresisted tracking offset from the current
                // DISPLAYED offset by inverting the rubber-band. This handles
                // both the common case (offset in bounds → identity) and the
                // edge case of starting a drag from overscroll (e.g.
                // interrupting a spring mid-bounce) without a visual jump.
                self.refresh_sizes(ctx);
                self.drag_unresisted_offset =
                    invert_rubber_band(self.scroll_offset, self.viewport_height, self.max_scroll());
                // Drag just won (on the move that crossed slop). Initialize
                // last_drag_y from the recognizer's DOWN position so the
                // first Move delta captures the full movement from press-down
                // to current — matching Flutter's scroll-keeps-up-with-finger
                // behavior. (The event_handler only calls Down on the FIRST
                // winning move, so this runs once per drag.)
                self.last_drag_y = drag.down_position().y;
            }
            ArenaEvent::Up { .. } => {
                // Sign-flip: the tracker returns pointer-space dy/dt (y-down).
                // The Move handler does `delta = last_drag_y - position.y`
                // (negates pointer delta), so negate tracker velocity so
                // positive v0 = offset increases = scrolls toward bottom.
                let v = -self.velocity_tracker.velocity();
                let max = self.max_scroll();

                if self.scroll_offset < 0.0 {
                    // Released past top → bounce back to 0. Always start the
                    // spring, even with zero velocity — a critically-damped
                    // spring still pulls content back to the edge.
                    let now = Instant::now();
                    let Some(element_id) = self.id else {
                        return;
                    };
                    let Some(tx) = ctx.dirty_sender().cloned() else {
                        return;
                    };
                    let Some(ticker) = self.animation_ticker.clone() else {
                        return;
                    };
                    self.unregister_ticker();
                    let sim = SpringMath::with_tolerance(
                        self.physics.spring,
                        self.scroll_offset as f64,
                        0.0,
                        v as f64,
                        self.physics.settle,
                    );
                    self.drive = ScrollDrive::Bounce {
                        sim,
                        start: now,
                        rest: 0.0,
                    };
                    self.tick_handle = self.register_ticker(tx.clone(), ticker);
                    let _ = tx.send(element_id);
                    log::debug!(
                        "[scroll] release past top → spring: offset={}, v={}",
                        self.scroll_offset,
                        v
                    );
                } else if self.scroll_offset > max {
                    // Released past bottom → bounce back to max.
                    let now = Instant::now();
                    let Some(element_id) = self.id else {
                        return;
                    };
                    let Some(tx) = ctx.dirty_sender().cloned() else {
                        return;
                    };
                    let Some(ticker) = self.animation_ticker.clone() else {
                        return;
                    };
                    self.unregister_ticker();
                    let sim = SpringMath::with_tolerance(
                        self.physics.spring,
                        self.scroll_offset as f64,
                        max as f64,
                        v as f64,
                        self.physics.settle,
                    );
                    self.drive = ScrollDrive::Bounce {
                        sim,
                        start: now,
                        rest: max,
                    };
                    self.tick_handle = self.register_ticker(tx.clone(), ticker);
                    let _ = tx.send(element_id);
                    log::debug!(
                        "[scroll] release past bottom → spring: offset={}, v={}, max={}",
                        self.scroll_offset,
                        v,
                        max
                    );
                } else {
                    // Released in-bounds — existing fling behavior, gated by
                    // staleness + minimum velocity. The staleness guard lives
                    // HERE (not at the top of the Up arm) because releasing
                    // in overscroll should always start the spring, even if
                    // the last move was stale.
                    let is_stale = self
                        .last_move_time
                        .map(|t| Instant::now().duration_since(t) > Duration::from_millis(100))
                        .unwrap_or(true);
                    if is_stale {
                        return;
                    }
                    if v.abs() < self.physics.fling_min_velocity {
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
                    self.unregister_ticker();
                    let now = Instant::now();
                    let sim = FrictionSimulation::with_tolerance(
                        self.scroll_offset as f64,
                        v as f64,
                        self.physics.friction,
                        self.physics.settle,
                    );
                    self.drive = ScrollDrive::Fling { sim, start: now };
                    self.tick_handle = self.register_ticker(tx.clone(), ticker);
                    let _ = tx.send(element_id);
                }
            }
            ArenaEvent::Cancel => {
                // Drag cancelled. No cleanup needed.
            }
            ArenaEvent::Tick { .. } => {
                // Scroll is purely event-driven; ignore the clock tick.
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
            self.unregister_ticker();
            self.drive = ScrollDrive::Idle;
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

        // Single-drive dispatch (replaces the old two-block
        // `if momentum.is_active()` / `if spring.is_active()` structure).
        // The `ScrollDrive` enum enforces the old mutual-exclusion invariant
        // structurally — only one sim is ever active — so the debug_assert
        // guarding "both active" is no longer needed.
        //
        // `max_scroll` is captured BEFORE the match because the Fling arm
        // mutably borrows `sim` from `&mut self.drive` (can't also borrow
        // `&self` for `max_scroll()`). This matches the old code, which
        // called `self.max_scroll()` before refreshing sizes from the render
        // object — the possibly-stale value is the intended behavior.
        let max_scroll = self.max_scroll();
        match &mut self.drive {
            ScrollDrive::Idle => {}
            ScrollDrive::Fling { sim, start } => {
                let now = Instant::now();
                let t = now.saturating_duration_since(*start).as_secs_f64();
                if sim.is_done(t) {
                    self.unregister_ticker();
                    self.drive = ScrollDrive::Idle;
                } else {
                    let physics_offset = sim.x(t) as f32;
                    // Clamp to scroll bounds; on edge hit, hand off remaining
                    // velocity to a spring (inlined from the removed clamp_offset method).
                    let clamped = physics_offset.clamp(0.0, max_scroll);
                    let hit_edge = (clamped - physics_offset).abs() > f32::EPSILON;
                    if hit_edge {
                        // Fling hit an edge — hand off remaining velocity to
                        // a spring for one bounded overshoot + settle.
                        let v = sim.dx(t) as f32;
                        let rest = if physics_offset < 0.0 {
                            0.0
                        } else {
                            max_scroll
                        };
                        self.unregister_ticker();
                        if let (Some(element_id), Some(ticker)) =
                            (self.id, self.animation_ticker.clone())
                        {
                            let now = Instant::now();
                            let tx = context.dirty_sender.clone();
                            let new_sim = SpringMath::with_tolerance(
                                self.physics.spring,
                                clamped as f64,
                                rest as f64,
                                v as f64,
                                self.physics.settle,
                            );
                            self.drive = ScrollDrive::Bounce {
                                sim: new_sim,
                                start: now,
                                rest,
                            };
                            self.tick_handle = self.register_ticker(tx.clone(), ticker);
                            let _ = tx.send(element_id);
                            log::debug!(
                                "[scroll] fling hit edge → spring: clamped={}, v={}, rest={}",
                                clamped,
                                v,
                                rest
                            );
                        } else {
                            // No id/ticker — can't start the spring handoff.
                            self.drive = ScrollDrive::Idle;
                        }
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
                    // above), which sends element_id through the mpsc channel,
                    // which drain_dirty_to_build_owner picks up to schedule the
                    // next rebuild_from_state. No explicit mark_needs_build here.
                }
            }
            ScrollDrive::Bounce { sim, start, rest } => {
                let now = Instant::now();
                let t = now.saturating_duration_since(*start).as_secs_f64();
                if sim.is_done(t) {
                    // Settled — snap exactly to rest and stop.
                    let rest = *rest;
                    self.unregister_ticker();
                    self.drive = ScrollDrive::Idle;
                    if let Some(ro_key) = self.render_object {
                        if let Some(svro) = context
                            .render_objects
                            .get(ro_key)
                            .and_then(|ro| ro.as_any().downcast_ref::<ScrollViewRenderObject>())
                        {
                            self.viewport_height = svro.viewport_size().height;
                            self.content_height = svro.content_size().height;
                            svro.set_scroll_offset(rest);
                        }
                    }
                    self.scroll_offset = rest;
                    if let Some(ctrl) = self.controller.as_ref() {
                        ctrl.set_current_offset(rest);
                    }
                } else {
                    let physics_offset = sim.x(t) as f32;
                    if let Some(ro_key) = self.render_object {
                        if let Some(svro) = context
                            .render_objects
                            .get(ro_key)
                            .and_then(|ro| ro.as_any().downcast_ref::<ScrollViewRenderObject>())
                        {
                            self.viewport_height = svro.viewport_size().height;
                            self.content_height = svro.content_size().height;
                            svro.set_scroll_offset(physics_offset);
                        }
                    }
                    self.scroll_offset = physics_offset;
                    if let Some(ctrl) = self.controller.as_ref() {
                        ctrl.set_current_offset(physics_offset);
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
    fn test_invert_rubber_band_identity_in_bounds() {
        assert_eq!(invert_rubber_band(50.0, 400.0, 1000.0), 50.0);
        assert_eq!(invert_rubber_band(0.0, 400.0, 1000.0), 0.0);
        assert_eq!(invert_rubber_band(1000.0, 400.0, 1000.0), 1000.0);
    }

    #[test]
    fn test_invert_rubber_band_roundtrips_past_top() {
        for &raw in &[-10.0, -50.0, -100.0, -300.0, -399.0] {
            let displayed = apply_rubber_band(raw, 400.0, 1000.0);
            let recovered = invert_rubber_band(displayed, 400.0, 1000.0);
            assert!(
                (recovered - raw).abs() < 0.01,
                "roundtrip failed: raw={} displayed={} recovered={}",
                raw,
                displayed,
                recovered
            );
        }
    }

    #[test]
    fn test_invert_rubber_band_roundtrips_past_bottom() {
        for &raw in &[1010.0, 1050.0, 1100.0, 1300.0, 1399.0] {
            let displayed = apply_rubber_band(raw, 400.0, 1000.0);
            let recovered = invert_rubber_band(displayed, 400.0, 1000.0);
            assert!(
                (recovered - raw).abs() < 0.01,
                "roundtrip failed: raw={} displayed={} recovered={}",
                raw,
                displayed,
                recovered
            );
        }
    }

    #[test]
    fn test_invert_rubber_band_zero_viewport_guarded() {
        let displayed = apply_rubber_band(-100.0, 0.0, 1000.0);
        let recovered = invert_rubber_band(displayed, 0.0, 1000.0);
        assert!(recovered <= 0.0, "should be past top; got {}", recovered);
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

    #[test]
    fn test_drag_past_edge_and_back_returns_to_start() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Press at y=300.
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

        // Drag DOWN 200px (past top edge → overscroll with rubber-band).
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 500.0),
            &InputEvent::PointerMoved {
                position: Point::new(200.0, 500.0),
            },
        );
        let overscrolled = ctrl.current_offset();
        assert!(
            overscrolled < 0.0,
            "should be in overscroll after dragging past top; got {}",
            overscrolled
        );

        // Drag back UP 200px (finger returns to the press position).
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 300.0),
            &InputEvent::PointerMoved {
                position: Point::new(200.0, 300.0),
            },
        );
        let after_return = ctrl.current_offset();

        // The finger returned to its starting position, so the offset should
        // be back at ~0 (the top edge). Before the fix, the rubber-band
        // compounded on the accumulated (already-resisted) offset, so the
        // return delta was applied 1:1 while the outward delta was resisted —
        // the content jumped forward past 0 instead of returning.
        assert!(
            after_return.abs() < 1.0,
            "dragging past edge and back should return to start (~0), not jump forward; got {}",
            after_return
        );

        // Suppress unused warning for ticker in case setup changes.
        drop(ticker);
    }

    #[test]
    fn test_drag_past_bottom_edge_and_back_returns_to_start() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);
        let max_scroll = max_scroll_of(&pipeline);

        // Pre-scroll to the bottom edge.
        ctrl.jump_to(max_scroll);
        for _ in 0..5 {
            pump(&ticker, &mut pipeline);
        }
        assert_eq!(ctrl.current_offset(), max_scroll);

        // Press + drag UP 200px (past bottom edge → overscroll).
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
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 200.0),
            &InputEvent::PointerMoved {
                position: Point::new(200.0, 200.0),
            },
        );
        let overscrolled = ctrl.current_offset();
        assert!(
            overscrolled > max_scroll,
            "should be in overscroll past bottom; got {}",
            overscrolled
        );

        // Drag back DOWN 200px (finger returns to press position).
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 400.0),
            &InputEvent::PointerMoved {
                position: Point::new(200.0, 400.0),
            },
        );
        let after_return = ctrl.current_offset();
        assert!(
            (after_return - max_scroll).abs() < 1.0,
            "dragging past bottom edge and back should return to max_scroll, not jump; got {} (max={})",
            after_return,
            max_scroll
        );
    }

    #[test]
    fn test_release_past_top_starts_spring() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Press + drag down past top (overscroll).
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
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 500.0),
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 500.0),
            &move_evt,
        );
        assert!(ctrl.current_offset() < 0.0, "should be in overscroll");

        // Release.
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

        // Spring should be active (ticker has registrations).
        pump(&ticker, &mut pipeline);
        assert!(
            ticker.has_active(),
            "spring should be active after releasing in overscroll"
        );
    }

    #[test]
    fn test_release_in_bounds_starts_momentum_not_spring() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Press + fast drag up (in-bounds, builds velocity).
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
        for &y in &[350.0, 250.0, 150.0] {
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, y),
            };
            dispatch(&mut pipeline, &mut font_system, Point::new(200.0, y), &mv);
        }
        assert!(ctrl.current_offset() > 0.0, "should have scrolled");

        // Release in-bounds.
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

        // Momentum should be active (not spring — this is the existing fling path).
        pump(&ticker, &mut pipeline);
        assert!(
            ticker.has_active(),
            "momentum should be active after in-bounds release with velocity"
        );
    }

    #[test]
    fn test_spring_settles_to_top_edge() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Press + drag down past top.
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
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 500.0),
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 500.0),
            &move_evt,
        );

        // Release.
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

        // Pump until spring settles (ticker goes quiet). Each pump advances
        // the spring by `frame_dt` worth of physics (measured via Instant::now()
        // inside SpringSimulation::advance). Without real wall-clock time
        // between pumps, 2000 pumps cover only ~5ms of physics — far short of
        // the ~400ms a critically-damped spring needs to settle from -150px.
        // The 2ms sleep lets each pump's advance see ~2ms of elapsed time, so
        // the spring settles after ~200 pumps (~0.4s).
        for _ in 0..2000 {
            pump(&ticker, &mut pipeline);
            if !ticker.has_active() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!ticker.has_active(), "spring should have settled");
        assert_eq!(
            ctrl.current_offset(),
            0.0,
            "spring should settle exactly at top edge (0.0); got {}",
            ctrl.current_offset()
        );
    }

    #[test]
    fn test_fling_into_bottom_edge_starts_spring() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);
        let max_scroll = max_scroll_of(&pipeline);

        // Pre-scroll near the bottom so the fling hits the edge quickly.
        let target = (max_scroll - 500.0).max(0.0);
        ctrl.jump_to(target);
        for _ in 0..5 {
            pump(&ticker, &mut pipeline);
        }

        // Fling upward (toward bottom edge).
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

        // Pump enough for the fling to hit the edge and hand off to spring.
        for _ in 0..10 {
            pump(&ticker, &mut pipeline);
        }
        // After hitting the edge, momentum stops and spring starts.
        // The spring is active (ticker.has_active() is true).
        assert!(
            ticker.has_active(),
            "spring should be active after fling hits bottom edge"
        );

        // Pump until spring settles.
        for _ in 0..2000 {
            pump(&ticker, &mut pipeline);
            if !ticker.has_active() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!ticker.has_active(), "spring should have settled");
        assert_eq!(
            ctrl.current_offset(),
            max_scroll,
            "spring should settle exactly at bottom edge; got {}",
            ctrl.current_offset()
        );
    }

    #[test]
    fn test_wheel_clamps_at_bottom_edge() {
        use crate::core::ScaleSource;
        use crate::core::{Point, Size};
        use crate::input::{InputEvent, Modifiers};
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
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Scroll to the very bottom via jump_to.
        let max_scroll = max_scroll_of(&pipeline);
        ctrl.jump_to(max_scroll);
        for _ in 0..5 {
            pipeline.drain_dirty_to_build_owner();
            pipeline.perform_rebuilds();
        }
        assert_eq!(ctrl.current_offset(), max_scroll);

        // Wheel down 1000px past the bottom edge.
        let event = InputEvent::Scroll {
            position: Point::new(200.0, 300.0),
            delta: Point::new(0.0, -1000.0),
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
            max_scroll,
            "wheel past bottom should clamp at max_scroll, not overscroll; got {}",
            ctrl.current_offset()
        );
    }

    #[test]
    fn test_wheel_clamps_at_top_edge() {
        use crate::core::ScaleSource;
        use crate::core::{Point, Size};
        use crate::input::{InputEvent, Modifiers};
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
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Start at top (offset 0). Wheel UP 1000px (toward top, past edge).
        // delta.y positive = scroll up (toward top) per codebase convention.
        let event = InputEvent::Scroll {
            position: Point::new(200.0, 300.0),
            delta: Point::new(0.0, 1000.0),
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
            0.0,
            "wheel past top should clamp at 0, not overscroll; got {}",
            ctrl.current_offset()
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

    /// Like `setup_scroll_view` but injects custom `ScrollPhysics`. Used by
    /// `stiffer_physics_settles_faster_than_default` to prove the config
    /// surface drives the bounce-back sim (ROADMAP §9 ScrollPhysics gap).
    fn setup_scroll_view_with_physics(
        ctrl: &crate::widgets::ScrollController,
        physics: crate::widgets::scroll_view::ScrollPhysics,
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
        let sv = ScrollView::new(col.boxed())
            .controller(ctrl.clone())
            .physics(physics);
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
    fn test_drag_overscrolls_at_top_with_arena() {
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

        // Pump for the fling to hit the edge and the spring to settle.
        // The fling hands off to a spring on edge-hit; the spring overshoots
        // once then settles back to the edge. Pump until the ticker goes quiet.
        // The 2ms sleep lets each pump's spring advance see real wall-clock
        // time (SpringSimulation uses Instant::now() for frame_dt) — without
        // it, instantaneous pumps cover only ~ms of physics and the spring
        // can't settle. Same pattern as test_spring_settles_to_top_edge.
        for _ in 0..5000 {
            pump(&ticker, &mut pipeline);
            if !ticker.has_active() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        // After spring settle, offset snaps exactly to max_scroll (the rest).
        assert_eq!(
            ctrl.current_offset(),
            max_scroll,
            "fling should settle at max_scroll ({}) after bounce; got {}",
            max_scroll,
            ctrl.current_offset()
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

        // Pump for the fling to hit the top edge and the spring to settle.
        // 2ms sleep per pump gives the spring real wall-clock time to settle
        // (same pattern as test_spring_settles_to_top_edge).
        for _ in 0..5000 {
            pump(&ticker, &mut pipeline);
            if !ticker.has_active() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }

        // After spring settle, offset snaps exactly to 0.0 (the rest).
        assert_eq!(
            ctrl.current_offset(),
            0.0,
            "downward fling should settle at top edge (0.0) after bounce; got {}",
            ctrl.current_offset()
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

    #[test]
    fn test_press_during_bounce_stops_spring() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Drag past top to create overscroll.
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
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 500.0),
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 500.0),
            &move_evt,
        );

        // Release → spring starts.
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
        pump(&ticker, &mut pipeline);
        assert!(
            ticker.has_active(),
            "spring should be active after release in overscroll"
        );

        // Press mid-bounce → should stop the spring.
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
        assert!(
            !ticker.has_active(),
            "press during bounce should stop spring"
        );
    }

    #[test]
    fn test_wheel_during_bounce_stops_spring() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Drag past top + release → spring starts.
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
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 500.0),
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 500.0),
            &move_evt,
        );
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
        pump(&ticker, &mut pipeline);
        assert!(ticker.has_active(), "spring should be active");

        // Wheel mid-bounce → should stop the spring.
        let wheel = InputEvent::Scroll {
            position: Point::new(200.0, 300.0),
            delta: Point::new(0.0, -100.0),
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 300.0),
            &wheel,
        );
        assert!(
            !ticker.has_active(),
            "wheel during bounce should stop spring"
        );
    }

    #[test]
    fn test_jump_to_during_bounce_stops_spring() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Drag past top + release → spring starts.
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
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 500.0),
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 500.0),
            &move_evt,
        );
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
        pump(&ticker, &mut pipeline);
        assert!(ticker.has_active(), "spring should be active");

        // jump_to mid-bounce → should stop the spring.
        ctrl.jump_to(0.0);
        pump(&ticker, &mut pipeline);
        assert!(
            !ticker.has_active(),
            "jump_to during bounce should stop spring"
        );
    }

    #[test]
    fn test_unmount_stops_spring() {
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::ScrollController;

        let ctrl = ScrollController::new();
        let (ticker, mut pipeline, mut font_system) = setup_scroll_view(&ctrl);

        // Drag past top + release → spring starts.
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
        let move_evt = InputEvent::PointerMoved {
            position: Point::new(200.0, 500.0),
        };
        dispatch(
            &mut pipeline,
            &mut font_system,
            Point::new(200.0, 500.0),
            &move_evt,
        );
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
        pump(&ticker, &mut pipeline);
        assert!(ticker.has_active(), "spring should be active");

        // Drop the pipeline → element dropped → SpringSimulation::Drop → stop() → unregister ticker.
        drop(pipeline);
        assert!(
            !ticker.has_active(),
            "unmount should stop spring and unregister ticker handle"
        );
    }

    #[test]
    fn test_press_stops_spring_even_with_child_gesture_detector() {
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
            col = col.push(crate::Text::new("row").boxed().on_press(|| ()));
        }
        let sv = ScrollView::new(col.boxed()).controller(ctrl.clone());
        let ticker = Arc::new(AnimationTicker::new());
        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
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
        let release = InputEvent::PointerButton {
            position: Point::new(200.0, 500.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            Point::new(200.0, 500.0),
            &release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        assert!(
            ticker.has_active(),
            "spring should be active after release in overscroll"
        );

        let press2 = InputEvent::PointerButton {
            position: Point::new(200.0, 300.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::new(200.0, 300.0),
            &press2,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        assert!(
            !ticker.has_active(),
            "press should stop spring even when child GestureDetector blocks on_event propagation"
        );
    }

    #[test]
    fn stiffer_physics_settles_faster_than_default() {
        // A stiffer spring (k=2000 vs default 340) should settle in fewer
        // pumps. Proves the ScrollPhysics config surface actually drives the
        // bounce-back sim (ROADMAP §9 ScrollPhysics gap).
        use crate::animation::SpringDescription;
        use crate::core::Point;
        use crate::input::{ButtonState, InputEvent, PointerButton};
        use crate::widgets::scroll_view::ScrollPhysics;
        use crate::widgets::ScrollController;

        // Drag past top edge + release, then count pumps until settled.
        // Returns the pump count. Modeled on test_spring_settles_to_top_edge.
        fn settle_pump_count(physics: ScrollPhysics) -> usize {
            let ctrl = ScrollController::new();
            let (ticker, mut pipeline, mut font_system) =
                setup_scroll_view_with_physics(&ctrl, physics);

            // Press + drag down past top (overscroll).
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
            let mv = InputEvent::PointerMoved {
                position: Point::new(200.0, 500.0),
            };
            dispatch(
                &mut pipeline,
                &mut font_system,
                Point::new(200.0, 500.0),
                &mv,
            );
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

            // Pump until spring settles. The 2ms sleep lets each pump's
            // advance see ~2ms of elapsed wall-clock time (the sim uses
            // Instant::now()), so the spring settles after ~200 pumps for
            // the default k=340.
            let mut pumps = 0;
            for _ in 0..5000 {
                pump(&ticker, &mut pipeline);
                pumps += 1;
                if !ticker.has_active() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert!(!ticker.has_active(), "spring should have settled");
            pumps
        }

        let default_pumps = settle_pump_count(ScrollPhysics::default());
        let stiff_pumps = settle_pump_count(ScrollPhysics {
            spring: SpringDescription::ios(2000.0, 1.0), // ~6× stiffer
            ..ScrollPhysics::default()
        });
        assert!(
            stiff_pumps < default_pumps,
            "stiffer spring (k=2000) should settle faster than default (k=340); \
             got stiff={} pumps, default={} pumps",
            stiff_pumps,
            default_pumps
        );
    }
}
