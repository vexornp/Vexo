# Swipe-Right-to-Pop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add iOS-style swipe-right-from-leading-edge-to-pop to `NavigationStackView`, where the page follows the finger and a velocity-aware spring commits or cancels on release.

**Architecture:** A new `EdgePanRecognizer` (leading-edge-gated horizontal drag) is consumed by a new `EdgePanDetector` widget that wraps the nav stack's output. `NavigationStackViewState` holds an `InteractivePop` state behind an `Rc<RefCell<...>>` (shared with the gesture closures, mirroring `ContextMenuState`'s pattern). The finger drives `AnimationController::set_value` directly; on release, `animate_with(SpringSimulation)` carries the gesture velocity to commit (1.0) or cancel (0.0). The interactive render branch reuses the existing `base_fx_alpha` + `default_mobile_transition` with finger-driven `eased`.

**Tech Stack:** Rust, vexo framework (gestures/animation/widgets), vexo_uikit (navigation), Taffy layout, wgpu rendering.

## Global Constraints

- Edge width: `EDGE_WIDTH = 20.0` logical pt (leading edge only)
- Horizontal drag slop: `HORIZONTAL_DRAG_SLOP = 18.0` pt (matches existing `VERTICAL_DRAG_SLOP`)
- Flick threshold: `FLICK_THRESHOLD = 0.5` progress/sec
- Spring: `SpringDescription::ios(340.0, 1.0)` (matches existing nav/context-menu springs)
- Mobile-only; disabled at root, during transitions, and on desktop
- Must not break existing `should_rebuild` level-3 optimization on `NavigationStackView`
- Follow existing patterns: `VerticalDragRecognizer` (recognizer template), `GestureDetector` (widget/element template), `ContextMenuState` (shared-cell animation template)
- No comments in code unless asked (per CLAUDE.md) — except module-level `//!` docs matching existing style
- Run `cargo build` after each task's edits; `cargo test` after each task's tests pass

---

## File Structure

| File | Responsibility |
|---|---|
| `vexo/src/gestures/edge_pan.rs` | **New** — `EdgePanRecognizer`: leading-edge-gated horizontal drag state machine |
| `vexo/src/gestures/mod.rs` | Add `edge_pan` module + `EDGE_WIDTH`/`HORIZONTAL_DRAG_SLOP` constants + re-export |
| `vexo/src/widgets/edge_pan_detector.rs` | **New** — `EdgePanDetector` widget + element + pass-through render object |
| `vexo/src/widgets/mod.rs` | Re-export `EdgePanDetector` |
| `vexo/src/animation/controller.rs` | Add `set_value` method + unit tests |
| `vexo/src/event_handler.rs` | Extend `is_drag_winner` check to include `EdgePanRecognizer` |
| `vexo_uikit/src/navigation.rs` | `InteractivePop` state, render branch, `EdgePanDetector` wiring, lifecycle hooks, `NavigationController` begin/commit/cancel API, constants |
| `vexo_uikit/tests/navigation_interactive_pop_tests.rs` | **New** — integration tests |

---

### Task 1: `AnimationController::set_value`

**Files:**
- Modify: `vexo/src/animation/controller.rs`

**Interfaces:**
- Produces: `pub fn set_value(&mut self, v: f64)` — sets value clamped to 0..1, stops any active drive, fires dirty callback. After call: `is_animating() == false`, `direction() == Stopped`, `value() == v.clamp(0,1)`.

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)]` module in `vexo/src/animation/controller.rs` (after the existing tests, before the closing `}`):

```rust
    #[test]
    fn set_value_sets_value_and_stops_drive() {
        let mut ctrl = AnimationController::new(Duration::from_millis(100));
        ctrl.forward();
        assert!(ctrl.is_animating());
        ctrl.set_value(0.42);
        assert!(!ctrl.is_animating(), "set_value must stop the drive");
        assert_eq!(ctrl.direction(), AnimationDirection::Stopped);
        assert!((ctrl.value() - 0.42).abs() < 1e-9, "value must be 0.42");
    }

    #[test]
    fn set_value_clamps_to_0_1() {
        let mut ctrl = AnimationController::new(Duration::from_millis(100));
        ctrl.set_value(-0.5);
        assert_eq!(ctrl.value(), 0.0, "negative clamps to 0");
        ctrl.set_value(1.5);
        assert_eq!(ctrl.value(), 1.0, ">1 clamps to 1");
    }

    #[test]
    fn set_value_fires_dirty_callback() {
        use std::sync::{Arc, Mutex};
        let count = Arc::new(Mutex::new(0u32));
        let cb_count = count.clone();
        let mut ctrl = AnimationController::new(Duration::from_millis(100));
        ctrl.set_dirty_callback(Arc::new(move || {
            *cb_count.lock().unwrap() += 1;
        }));
        ctrl.set_value(0.5);
        assert_eq!(*count.lock().unwrap(), 1, "set_value must fire dirty once");
    }

    #[test]
    fn set_value_cancels_prior_simulation() {
        let mut ctrl = AnimationController::new(Duration::from_millis(100));
        ctrl.animate_with(Box::new(critical_spring_sim(0.0, 1.0, 0.0)));
        assert!(ctrl.is_animating());
        ctrl.set_value(0.3);
        assert!(!ctrl.is_animating(), "set_value must cancel the simulation");
        assert!((ctrl.value() - 0.3).abs() < 1e-9);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib animation::controller::tests::set_value -- --nocapture`
Expected: FAIL — `set_value` method not found (compile error).

- [ ] **Step 3: Implement `set_value`**

Add this method to the `impl AnimationController` block in `vexo/src/animation/controller.rs`, immediately after the existing `stop()` method (after line 107):

```rust
    /// Set the controller's value directly, stopping any active drive.
    ///
    /// Used by gesture-driven animations (e.g. swipe-to-pop) where the finger
    /// controls progress: each pointer Move calls `set_value(progress)` so the
    /// rendered transition tracks the finger 1:1. On release, the caller starts
    /// a spring via `animate_with` to settle to 0.0 or 1.0.
    ///
    /// After this call: `is_animating() == false`, `direction() == Stopped`,
    /// `value() == v.clamp(0.0, 1.0)`. The value is clamped so a finger
    /// briefly overshooting the content width can't push progress past 1.0.
    pub fn set_value(&mut self, v: f64) {
        self.unregister_from_ticker();
        self.drive = Drive::Stopped;
        self.value = v.clamp(0.0, 1.0);
        if let Some(cb) = &self.dirty_callback {
            cb();
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib animation::controller::tests::set_value -- --nocapture`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add vexo/src/animation/controller.rs
git commit -m "feat(animation): add AnimationController::set_value for gesture-driven progress

Lets a finger drive the controller directly (0..1), stopping any active
drive. Foundation for swipe-to-pop's drag phase."
```

---

### Task 2: `EdgePanRecognizer`

**Files:**
- Create: `vexo/src/gestures/edge_pan.rs`
- Modify: `vexo/src/gestures/mod.rs`

**Interfaces:**
- Produces: `pub struct EdgePanRecognizer` implementing `GestureRecognizer`
- Produces: accessors `down_position()`, `last_position()`, `total_delta_x()` (net signed horizontal displacement from down)
- Produces: constants `EDGE_WIDTH = 20.0`, `HORIZONTAL_DRAG_SLOP = 18.0` (in `mod.rs`)

- [ ] **Step 1: Write the failing tests**

Create `vexo/src/gestures/edge_pan.rs` with the full implementation AND tests (the tests reference `super::*` so the struct must exist — write the struct + tests together, then verify tests pass). First, write the test module at the bottom of the new file:

```rust
//! EdgePanRecognizer — recognizes a horizontal drag starting from the
//! leading (left) screen edge.
//!
//! Mirrors `VerticalDragRecognizer`'s slop/accept/reject model, with two
//! additions:
//! 1. The initial `Down` must land within `EDGE_WIDTH` of the left edge;
//!    otherwise the recognizer rejects immediately (a non-edge drag never
//!    competes, so a future horizontal-scroll recognizer isn't starved).
//! 2. Only rightward movement (positive Δx) accepts — a leftward drag from
//!    the edge rejects, letting content (e.g. a scroll view) handle it.
//!
//! `total_delta_x` is the NET signed displacement (`last.x - down.x`), not
//! cumulative magnitude. This is what swipe-to-pop needs for finger-tracking
//! progress, and a finger that jitters in place without net rightward
//! movement shouldn't start a pop.

use crate::core::{Logical, Point};

use std::any::Any;

use super::arena_event::ArenaEvent;
use super::recognizer::{ArenaContext, GestureRecognizer, RecognizerResolution};
use super::{EDGE_WIDTH, HORIZONTAL_DRAG_SLOP};

pub struct EdgePanRecognizer {
    resolution: RecognizerResolution,
    down_position: Point<Logical>,
    last_position: Point<Logical>,
}

impl EdgePanRecognizer {
    pub fn new() -> Self {
        Self {
            resolution: RecognizerResolution::Pending,
            down_position: Point::zero(),
            last_position: Point::zero(),
        }
    }

    /// Net signed horizontal displacement from the down position.
    /// Positive = rightward (the swipe-to-pop direction). Read by
    /// `EdgePanDetectorElement` to drive `on_update(total_delta_x)`.
    pub fn total_delta_x(&self) -> f32 {
        self.last_position.x - self.down_position.x
    }

    pub fn down_position(&self) -> Point<Logical> {
        self.down_position
    }

    pub fn last_position(&self) -> Point<Logical> {
        self.last_position
    }
}

impl Default for EdgePanRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer for EdgePanRecognizer {
    fn handle_event(&mut self, event: &ArenaEvent, ctx: &ArenaContext) {
        if self.rejected() {
            return;
        }
        match event {
            ArenaEvent::Down { .. } => {
                if ctx.down_position.x <= EDGE_WIDTH {
                    self.down_position = ctx.down_position;
                    self.last_position = ctx.down_position;
                    self.resolution = RecognizerResolution::Pending;
                } else {
                    self.resolution = RecognizerResolution::Rejected;
                }
            }
            ArenaEvent::Move { .. } => {
                self.last_position = ctx.current_position;
                if self.resolution == RecognizerResolution::Pending {
                    let dx = self.total_delta_x();
                    let abs_dy = (ctx.current_position.y - self.down_position.y).abs();
                    if dx > HORIZONTAL_DRAG_SLOP && dx > abs_dy {
                        self.resolution = RecognizerResolution::Accepted;
                    } else if abs_dy > HORIZONTAL_DRAG_SLOP && abs_dy > dx {
                        self.resolution = RecognizerResolution::Rejected;
                    }
                }
            }
            ArenaEvent::Up { .. } => {
                if self.resolution == RecognizerResolution::Pending {
                    self.resolution = RecognizerResolution::Rejected;
                }
            }
            ArenaEvent::Cancel => {
                self.resolution = RecognizerResolution::Rejected;
            }
            ArenaEvent::Tick { .. } => {}
        }
    }

    fn resolution(&self) -> RecognizerResolution {
        self.resolution
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(down: Point<Logical>, current: Point<Logical>) -> ArenaContext {
        ArenaContext {
            down_position: down,
            current_position: current,
        }
    }

    #[test]
    fn down_in_edge_zone_stays_pending() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        assert_eq!(r.resolution(), RecognizerResolution::Pending);
    }

    #[test]
    fn down_outside_edge_zone_rejects_immediately() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(50.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn rightward_move_past_slop_accepts() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move { position: Point::new(40.0, 52.0) },
            &ctx(down, Point::new(40.0, 52.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
        assert_eq!(r.total_delta_x(), 30.0);
    }

    #[test]
    fn leftward_move_does_not_accept() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move { position: Point::new(-20.0, 50.0) },
            &ctx(down, Point::new(-20.0, 50.0)),
        );
        assert_eq!(
            r.resolution(),
            RecognizerResolution::Pending,
            "leftward drag from edge must not accept"
        );
    }

    #[test]
    fn vertical_dominant_move_rejects() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move { position: Point::new(15.0, 100.0) },
            &ctx(down, Point::new(15.0, 100.0)),
        );
        assert_eq!(
            r.resolution(),
            RecognizerResolution::Rejected,
            "vertical-dominant movement must reject so vertical scroll can win"
        );
    }

    #[test]
    fn up_without_slop_rejects() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Up { position: down }, &ctx(down, down));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }

    #[test]
    fn stays_accepted_after_slop() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(
            &ArenaEvent::Move { position: Point::new(50.0, 50.0) },
            &ctx(down, Point::new(50.0, 50.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
        r.handle_event(
            &ArenaEvent::Move { position: Point::new(30.0, 50.0) },
            &ctx(down, Point::new(30.0, 50.0)),
        );
        assert_eq!(r.resolution(), RecognizerResolution::Accepted);
        assert_eq!(r.total_delta_x(), 20.0, "net displacement tracks last position");
    }

    #[test]
    fn cancel_rejects() {
        let mut r = EdgePanRecognizer::new();
        let down = Point::new(10.0, 50.0);
        r.handle_event(&ArenaEvent::Down { position: down }, &ctx(down, down));
        r.handle_event(&ArenaEvent::Cancel, &ctx(down, down));
        assert_eq!(r.resolution(), RecognizerResolution::Rejected);
    }
}
```

- [ ] **Step 2: Register the module + constants**

In `vexo/src/gestures/mod.rs`:

Add `pub mod edge_pan;` to the module list (after `pub mod arena_event;` line). Add the re-export `pub use edge_pan::EdgePanRecognizer;` (after the `pub use vertical_drag::VerticalDragRecognizer;` line). Add the constants (after the `LONG_PRESS_SLOP` constant):

```rust
/// Distance from the leading (left) screen edge within which an edge-pan
/// gesture may begin. Matches iOS `UIScreenEdgePanGestureRecognizer`'s
/// default edge zone.
pub(crate) const EDGE_WIDTH: f32 = 20.0;

/// Cumulative horizontal movement threshold beyond which an edge-pan is
/// recognized. Matches `VERTICAL_DRAG_SLOP` for consistent feel.
pub(crate) const HORIZONTAL_DRAG_SLOP: f32 = 18.0;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p vexo --lib gestures::edge_pan -- --nocapture`
Expected: PASS (8 tests).

- [ ] **Step 4: Commit**

```bash
git add vexo/src/gestures/edge_pan.rs vexo/src/gestures/mod.rs
git commit -m "feat(gestures): add EdgePanRecognizer for leading-edge horizontal drag

Leading-edge-gated (20pt) horizontal drag recognizer. Only rightward
movement past slop accepts; vertical-dominant movement rejects so scroll
wins. Foundation for swipe-to-pop."
```

---

### Task 3: `EdgePanDetector` widget + element + render object

**Files:**
- Create: `vexo/src/widgets/edge_pan_detector.rs`
- Modify: `vexo/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `EdgePanRecognizer` (Task 2), `GestureRecognizer` trait, `ArenaEvent`, `GestureArena`
- Produces: `pub struct EdgePanDetector` with builder `EdgePanDetector::new(child, enabled).on_start(FnMut()).on_update(FnMut(f32)).on_end(FnMut(f32))`

- [ ] **Step 1: Create the widget file**

Create `vexo/src/widgets/edge_pan_detector.rs`. This mirrors `gesture_detector.rs`'s structure (widget + element + pass-through render object) but with pan callbacks and an `enabled` flag. Full file:

```rust
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
use crate::gestures::{ArenaEvent, GestureArena, GestureRecognizer, EdgePanRecognizer};
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
```

- [ ] **Step 2: Register the module + re-export**

In `vexo/src/widgets/mod.rs`, add `pub mod edge_pan_detector;` (next to the existing `pub mod gesture_detector;` line) and `pub use edge_pan_detector::EdgePanDetector;` (next to the existing `GestureDetector` re-export). First read the file to find the exact lines:

Run: `grep -n "gesture_detector\|GestureDetector" vexo/src/widgets/mod.rs`

Add the `edge_pan_detector` module declaration and re-export alongside the `gesture_detector` ones.

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build -p vexo`
Expected: compiles with no errors.

- [ ] **Step 4: Write the integration test (pipeline-level, verifies callbacks fire)**

Create `vexo/src/widgets/edge_pan_detector.rs` already has no test module — add the test as a separate integration test in `vexo/tests/edge_pan_detector.rs`:

```rust
//! Integration test: EdgePanDetector fires on_start/on_update/on_end when an
//! edge-pan gesture wins the arena, and registers no recognizer when disabled.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vexo::animation::AnimationTicker;
use vexo::core::{Logical, Point, ScaleSource, Size};
use vexo::input::{ButtonState, InputEvent, PointerButton};
use vexo::layout::TaffyLayoutEngine;
use vexo::pipeline::ThreeTreePipeline;
use vexo::{EdgePanDetector, EdgePanRecognizer, Widget};
use vexo::{DecoratedBox, Style, Color, Text};

fn create_test_font_system() -> glyphon::FontSystem {
    let font_data = vexo::resource::file::FONT.to_vec();
    let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
    glyphon::FontSystem::new_with_fonts([binary])
}

#[test]
fn edge_pan_detector_fires_start_update_end_when_enabled() {
    let started = Rc::new(Cell::new(false));
    let last_delta = Rc::new(Cell::new(0.0_f32));
    let ended = Rc::new(Cell::new(false));
    let end_delta = Rc::new(Cell::new(0.0_f32));
    let s = started.clone();
    let u = last_delta.clone();
    let e = ended.clone();
    let ed = end_delta.clone();

    let widget: Box<dyn Widget> = Box::new(
        EdgePanDetector::new(
            DecoratedBox::with_style(
                Text::new("Swipe me"),
                Style::default().background(Color::WHITE),
            ),
            true,
        )
        .on_start(move || s.set(true))
        .on_update(move |dx| u.set(dx))
        .on_end(move |dx| {
            e.set(true);
            ed.set(dx);
        }),
    );

    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(widget);
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    let clipboard: Arc<dyn vexo::platform::Clipboard> =
        Arc::new(vexo::platform::stub_clipboard::StubClipboard);

    // Press within the 20pt edge zone.
    let press = InputEvent::PointerButton {
        position: Point::new(10.0, 100.0),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    pipeline.handle_event(
        Point::new(10.0, 100.0),
        &press,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &clipboard,
    );

    // Move rightward past slop — triggers Down+Move on the winning recognizer.
    let mv = InputEvent::PointerMoved {
        position: Point::new(80.0, 102.0),
    };
    pipeline.handle_event(
        Point::new(80.0, 102.0),
        &mv,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &clipboard,
    );
    pipeline.perform_rebuilds();

    assert!(started.get(), "on_start must fire when recognizer wins");
    assert!(
        last_delta.get() > 0.0,
        "on_update must fire with positive delta_x, got {}",
        last_delta.get()
    );

    // Release — triggers on_end.
    let release = InputEvent::PointerButton {
        position: Point::new(80.0, 102.0),
        button: PointerButton::Primary,
        state: ButtonState::Released,
    };
    pipeline.handle_event(
        Point::new(80.0, 102.0),
        &release,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &clipboard,
    );
    pipeline.perform_rebuilds();

    assert!(ended.get(), "on_end must fire on release");
    assert!(end_delta.get() > 0.0);
}

#[test]
fn edge_pan_detector_disabled_does_not_fire() {
    let started = Rc::new(Cell::new(false));
    let s = started.clone();

    let widget: Box<dyn Widget> = Box::new(
        EdgePanDetector::new(
            DecoratedBox::with_style(
                Text::new("No swipe"),
                Style::default().background(Color::WHITE),
            ),
            false,
        )
        .on_start(move || s.set(true)),
    );

    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(widget);
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

    let clipboard: Arc<dyn vexo::platform::Clipboard> =
        Arc::new(vexo::platform::stub_clipboard::StubClipboard);

    let press = InputEvent::PointerButton {
        position: Point::new(10.0, 100.0),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    pipeline.handle_event(
        Point::new(10.0, 100.0),
        &press,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &clipboard,
    );
    let mv = InputEvent::PointerMoved {
        position: Point::new(80.0, 100.0),
    };
    pipeline.handle_event(
        Point::new(80.0, 100.0),
        &mv,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &ScaleSource::default(),
        &clipboard,
    );
    pipeline.perform_rebuilds();

    assert!(!started.get(), "on_start must NOT fire when disabled");
}
```

- [ ] **Step 5: Run the integration test**

Run: `cargo test -p vexo --test edge_pan_detector -- --nocapture`
Expected: PASS (2 tests). If the move event doesn't trigger the recognizer, check that `EdgePanRecognizer` is exported and the `event_handler` feeds Move events to the arena (it does generically — this works without Task 4 because the Move path is recognizer-agnostic; only the Up/consume-release path needs Task 4).

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/edge_pan_detector.rs vexo/src/widgets/mod.rs vexo/tests/edge_pan_detector.rs
git commit -m "feat(widgets): add EdgePanDetector widget with on_start/on_update/on_end

Mirrors GestureDetector's element/render-object plumbing. Registers
EdgePanRecognizer only when enabled; fires pan callbacks on arena win.
Pass-through render object (invisible)."
```

---

### Task 4: `event_handler.rs` drag-winner check

**Files:**
- Modify: `vexo/src/event_handler.rs` (lines ~345-352)

**Interfaces:**
- Consumes: `EdgePanRecognizer` (Task 2)

- [ ] **Step 1: Edit the `is_drag_winner` check**

In `vexo/src/event_handler.rs`, find the `is_drag_winner` check (around line 345). It currently checks only `VerticalDragRecognizer`. Add an `||` branch for `EdgePanRecognizer`:

Change:
```rust
                    let is_drag_winner = arena
                        .winner_recognizer()
                        .map(|r| {
                            r.as_any()
                                .downcast_ref::<crate::gestures::VerticalDragRecognizer>()
                                .is_some()
                        })
                        .unwrap_or(false);
```

To:
```rust
                    let is_drag_winner = arena
                        .winner_recognizer()
                        .map(|r| {
                            r.as_any()
                                .downcast_ref::<crate::gestures::VerticalDragRecognizer>()
                                .is_some()
                                || r
                                    .as_any()
                                    .downcast_ref::<crate::gestures::EdgePanRecognizer>()
                                    .is_some()
                        })
                        .unwrap_or(false);
```

- [ ] **Step 2: Build + run the Task 3 integration test to confirm release is consumed**

Run: `cargo build -p vexo && cargo test -p vexo --test edge_pan_detector -- --nocapture`
Expected: PASS (2 tests). The `on_end` callback now fires correctly on release because the drag winner consumes the release event.

- [ ] **Step 3: Commit**

```bash
git add vexo/src/event_handler.rs
git commit -m "feat(event_handler): treat EdgePanRecognizer as a drag-winner for release

Extends the is_drag_winner check so a winning edge-pan consumes the
release event (no bubble), matching VerticalDragRecognizer's behavior."
```

---

### Task 5: `NavigationController` interactive-pop API

**Files:**
- Modify: `vexo_uikit/src/navigation.rs`

**Interfaces:**
- Produces: `pub fn begin_interactive_pop(&self) -> Option<Vec<Dest>>`, `pub fn commit_interactive_pop(&self) -> Option<Dest>`, `pub fn cancel_interactive_pop(&self)`

- [ ] **Step 1: Write failing tests**

Add tests to the existing `vexo_uikit/tests/navigation_animation_tests.rs` file (append before the final closing of the file, or in a new `#[cfg(test)]`-appropriate section — this file uses integration test style, so append new `#[test]` functions):

```rust
#[test]
fn begin_interactive_pop_returns_from_path_without_mutating() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.clear_pending();
    controller.push("b");
    controller.clear_pending();

    let from = controller
        .begin_interactive_pop()
        .expect("must return from_path when path is non-empty and no pending");
    assert_eq!(from, vec!["a", "b"], "from_path is the current path");
    assert_eq!(
        controller.path(),
        vec!["a", "b"],
        "begin must NOT mutate the path"
    );
    assert!(
        controller.pending().is_none(),
        "begin must NOT set a pending op"
    );
}

#[test]
fn begin_interactive_pop_at_root_returns_none() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    assert!(controller.begin_interactive_pop().is_none());
}

#[test]
fn begin_interactive_pop_returns_none_when_pending() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a"); // sets pending
    assert!(controller.pending().is_some());
    assert!(
        controller.begin_interactive_pop().is_none(),
        "must not begin interactive pop while a push/pop transition is pending"
    );
}

#[test]
fn commit_interactive_pop_pops_path_and_fires_dirty() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.clear_pending();
    controller.push("b");
    controller.clear_pending();

    let dirty_count = Arc::new(AtomicU32::new(0));
    let dc = dirty_count.clone();
    controller.set_dirty_callback(Arc::new(move || {
        dc.fetch_add(1, Ordering::SeqCst);
    }));

    let popped = controller.commit_interactive_pop();
    assert_eq!(popped, Some("b"));
    assert_eq!(controller.path(), vec!["a"]);
    assert!(controller.pending().is_none(), "commit must NOT set pending");
    assert!(
        dirty_count.load(Ordering::SeqCst) >= 1,
        "commit must fire dirty"
    );
}

#[test]
fn commit_interactive_pop_at_root_returns_none() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    assert!(controller.commit_interactive_pop().is_none());
}

#[test]
fn cancel_interactive_pop_does_not_mutate_or_fire_dirty() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.clear_pending();

    let dirty_count = Arc::new(AtomicU32::new(0));
    let dc = dirty_count.clone();
    controller.set_dirty_callback(Arc::new(move || {
        dc.fetch_add(1, Ordering::SeqCst);
    }));

    controller.cancel_interactive_pop();
    assert_eq!(controller.path(), vec!["a"], "cancel must NOT mutate path");
    assert_eq!(
        dirty_count.load(Ordering::SeqCst),
        0,
        "cancel must NOT fire dirty"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo_uikit --test navigation_animation_tests begin_interactive_pop -- --nocapture`
Expected: FAIL — `begin_interactive_pop` method not found (compile error).

- [ ] **Step 3: Implement the three methods**

In `vexo_uikit/src/navigation.rs`, add these three methods to the `impl<Dest: Hash + Eq + Clone + 'static> NavigationController<Dest>` block (after the existing `replace` method, before the `// --- Framework wiring ---` comment):

```rust
    /// Begin an interactive (gesture-driven) pop. Does NOT mutate the path.
    /// Returns the current path snapshot (the `from_path` the view should
    /// render as the outgoing overlay). Returns `None` if the path is empty
    /// (at root) or if a pending (non-interactive) push/pop/replace transition
    /// is already in flight.
    ///
    /// The caller (the view's interactive-pop state) drives the transition
    /// animation directly via `AnimationController::set_value`; on release it
    /// calls `commit_interactive_pop` or `cancel_interactive_pop`.
    pub fn begin_interactive_pop(&self) -> Option<Vec<Dest>> {
        if self.pending.borrow().is_some() {
            return None;
        }
        let path = self.path.borrow();
        if path.is_empty() {
            return None;
        }
        Some(path.clone())
    }

    /// Commit an interactive pop that has animated to completion. Removes the
    /// top of the path. Does NOT set a pending op — the interactive animation
    /// already played the visual transition, so no fire-and-forget animation
    /// is needed. Fires the dirty callback so the view re-renders steady-state
    /// against the new (shorter) path.
    pub fn commit_interactive_pop(&self) -> Option<Dest> {
        let popped = self.path.borrow_mut().pop();
        if popped.is_some() {
            self.notify();
        }
        popped
    }

    /// Cancel an interactive pop. No path mutation, no dirty fire — the view
    /// clears its interactive state and re-renders steady-state against the
    /// unchanged path. The view is responsible for firing its own dirty
    /// callback to trigger the steady-state re-render after clearing state.
    pub fn cancel_interactive_pop(&self) {}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo_uikit --test navigation_animation_tests -- --nocapture`
Expected: PASS (all existing + 6 new tests).

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/src/navigation.rs vexo_uikit/tests/navigation_animation_tests.rs
git commit -m "feat(navigation): add NavigationController interactive-pop API

begin/commit/cancel_interactive_pop let the view drive a gesture-driven
pop without mutating the path until commit. Cancel is a pure no-op so
the path is untouched if the user releases below threshold."
```

---

### Task 6: `NavigationStackViewState` interactive-pop state machine + rendering

**Files:**
- Modify: `vexo_uikit/src/navigation.rs`

**Interfaces:**
- Consumes: `AnimationController::set_value` (Task 1), `EdgePanDetector` (Task 3), `NavigationController::begin/commit/cancel_interactive_pop` (Task 5), `SpringSimulation`, `SpringDescription`, `VelocityTracker`
- Produces: interactive-pop rendering integrated into `NavigationStackView::render`

- [ ] **Step 1: Add the imports**

At the top of `vexo_uikit/src/navigation.rs`, extend the `use vexo::{...}` import block to include the new items. Change the existing `use vexo::{...}` block to also import `EdgePanDetector`, `VelocityTracker`, and the animation simulation types. Add after the existing `use vexo::{...}` block:

```rust
use vexo::animation::{SpringDescription, SpringSimulation};
use vexo::VelocityTracker;
use vexo::EdgePanDetector;
```

- [ ] **Step 2: Add the `InteractivePop` struct + constants**

Add after the `NavTransition` struct definition (around line 409):

```rust
/// In-flight interactive (gesture-driven) pop. Lives behind an
/// `Rc<RefCell<Option<InteractivePop>>>` on `NavigationStackViewState` so the
/// gesture closures (built in `render`, fired outside `render`) can mutate it.
/// Mirrors `ContextMenuState`'s shared `Rc<RefCell<...>>` pattern.
struct InteractivePop<Dest: Hash + Eq + Clone + 'static> {
    controller: AnimationController,
    from_path: Vec<Dest>,
    to_path: Vec<Dest>,
    phase: InteractivePopPhase,
    velocity_tracker: VelocityTracker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InteractivePopPhase {
    Dragging,
    Committing,
    Cancelling,
}

/// Release past 50% progress commits; below cancels. A rightward flick above
/// this velocity also commits even if progress < 50%.
const FLICK_THRESHOLD: f32 = 0.5;
```

- [ ] **Step 3: Add the `interactive_pop` + `content_width` fields to `NavigationStackViewState`**

Change the `NavigationStackViewState` struct definition to add the shared cell and cached width. Change:

```rust
pub struct NavigationStackViewState<Dest: Hash + Eq + Clone + 'static> {
    _marker: PhantomData<Dest>,
    transition: Option<NavTransition<Dest>>,
    /// Cached ticker from `on_mount`. Used to wire transition controllers.
    ticker: Option<Arc<vexo::AnimationTicker>>,
    /// Cached dirty callback from `on_mount`. Used to wire transition controllers.
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}
```

To:

```rust
pub struct NavigationStackViewState<Dest: Hash + Eq + Clone + 'static> {
    _marker: PhantomData<Dest>,
    transition: Option<NavTransition<Dest>>,
    /// Shared cell holding the in-flight interactive pop. `Rc<RefCell<...>>` so
    /// the gesture closures (built in `render`, fired outside `render`) can
    /// mutate it. Mirrors `ContextMenuState`'s shared-cell pattern.
    interactive_pop: Rc<RefCell<Option<InteractivePop<Dest>>>>,
    /// Cached content width from the last `render()`. Read by gesture closures
    /// to convert finger delta_x → progress (0..1).
    content_width: f32,
    /// Cached ticker from `on_mount`. Used to wire transition controllers.
    ticker: Option<Arc<vexo::AnimationTicker>>,
    /// Cached dirty callback from `on_mount`. Used to wire transition controllers.
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}
```

Change the `Default` impl to initialize the new fields:

```rust
impl<Dest: Hash + Eq + Clone + 'static> Default for NavigationStackViewState<Dest> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
            transition: None,
            interactive_pop: Rc::new(RefCell::new(None)),
            content_width: 0.0,
            ticker: None,
            dirty_callback: None,
        }
    }
}
```

- [ ] **Step 4: Update `on_tick` to advance the interactive-pop controller**

Change the `on_tick` method. Replace:

```rust
    fn on_tick(&mut self, now: Instant) {
        if let Some(t) = self.transition.as_mut() {
            t.controller.advance(now);
        }
    }
```

With:

```rust
    fn on_tick(&mut self, now: Instant) {
        if let Some(t) = self.transition.as_mut() {
            t.controller.advance(now);
        }
        if let Some(ip) = self.interactive_pop.borrow_mut().as_mut() {
            ip.controller.advance(now);
        }
    }
```

- [ ] **Step 5: Update `on_rebuild`'s focus-clear guard**

In the `on_rebuild` method, change the condition:

```rust
        if self.transition.is_none() {
            if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
                if nav.controller.pending().is_some() {
                    ctx.clear_focus();
                }
            }
        }
```

To:

```rust
        if self.transition.is_none() && self.interactive_pop.borrow().is_none() {
            if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
                if nav.controller.pending().is_some() {
                    ctx.clear_focus();
                }
            }
        }
```

- [ ] **Step 6: Update `on_unmount` to stop the interactive-pop controller**

In `on_unmount`, add cleanup before `self.transition = None;`:

```rust
    fn on_unmount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
            nav.controller.clear_dirty_callback();
        }
        if let Some(t) = self.transition.as_mut() {
            t.controller.stop();
        }
        self.transition = None;
        if let Some(ip) = self.interactive_pop.borrow_mut().as_mut() {
            ip.controller.stop();
        }
        *self.interactive_pop.borrow_mut() = None;
        self.ticker = None;
        self.dirty_callback = None;
    }
```

- [ ] **Step 7: Add the interactive-pop render branch + completion detection + EdgePanDetector wiring**

This is the core change to the `render` method. The `render` method currently:
1. Checks for pending op → starts transition
2. Checks if transition completed → clears
3. Determines title/can_pop
4. Builds base IndexedStack
5. Computes base_fx/base_alpha
6. Builds content Stack with optional overlay
7. Wraps in clipping DecoratedBox + SafeArea
8. Returns MultiChild column of [nav_bar, content]

We need to add an interactive-pop branch that mirrors the transition branch but uses the finger/spring-driven controller. The cleanest insertion point: after the transition-completion check (step 2), add an interactive-pop completion check. Then in the rendering (steps 3-6), add an `interactive_pop` branch alongside `transition`.

Make these edits to the `render` method:

**7a. Add interactive-pop completion detection** — after the `transition_completed` block (after line ~574, after `state.transition = None;`), insert:

```rust
        // 2b. If an interactive pop is in flight, check if its spring has
        //     settled (phase != Dragging and controller stopped). On settle:
        //     commit or cancel the pop on the controller, clear the cell, and
        //     fire dirty to trigger a steady-state re-render.
        {
            let mut ip_cell = self.interactive_pop.borrow_mut();
            if let Some(ip) = ip_cell.as_ref() {
                if ip.phase != InteractivePopPhase::Dragging && !ip.controller.is_animating() {
                    let phase = ip.phase;
                    drop(ip_cell);
                    match phase {
                        InteractivePopPhase::Committing => {
                            self.controller.commit_interactive_pop();
                        }
                        InteractivePopPhase::Cancelling => {
                            self.controller.cancel_interactive_pop();
                        }
                        InteractivePopPhase::Dragging => {}
                    }
                    *self.interactive_pop.borrow_mut() = None;
                    // Fire dirty to re-render steady state (cancel doesn't fire
                    // dirty itself; commit does but a redundant fire is idempotent).
                    if let Some(cb) = &state.dirty_callback {
                        cb();
                    }
                }
            }
        }
```

**7b. Determine title/can_pop with interactive-pop awareness.** Replace the existing title/can_pop block (step 3, the `let (title, can_pop) = ...` block). Change:

```rust
        let (title, can_pop) = if let Some(t) = state.transition.as_ref() {
            // Use to_path's top for the title.
            if let Some(top) = t.to_path.last() {
                ((self.title)(top), true)
            } else {
                (self.root_title.clone().unwrap_or_default(), false)
            }
        } else {
            let path = self.controller.path();
            if let Some(top) = path.last() {
                ((self.title)(top), true)
            } else {
                (self.root_title.clone().unwrap_or_default(), false)
            }
        };
```

To:

```rust
        let (title, can_pop) = if let Some(t) = state.transition.as_ref() {
            if let Some(top) = t.to_path.last() {
                ((self.title)(top), true)
            } else {
                (self.root_title.clone().unwrap_or_default(), false)
            }
        } else if let Some(ip) = state.interactive_pop.borrow().as_ref() {
            // Interactive pop: title reflects where the user is going (to_path).
            if let Some(top) = ip.to_path.last() {
                ((self.title)(top), true)
            } else {
                (self.root_title.clone().unwrap_or_default(), false)
            }
        } else {
            let path = self.controller.path();
            if let Some(top) = path.last() {
                ((self.title)(top), true)
            } else {
                (self.root_title.clone().unwrap_or_default(), false)
            }
        };
```

**7c. Compute base_index + base_fx/base_alpha with interactive-pop awareness.** Replace the `base_index` and `base_fx_alpha` computation. Change:

```rust
        let base_index = match state.transition.as_ref() {
            None => path.len(),
            Some(t) => match t.direction {
                TransitionDir::Push => t.from_path.len(),
                TransitionDir::Pop | TransitionDir::PopToRoot => t.to_path.len(),
            },
        };
```

To:

```rust
        let base_index = match state.transition.as_ref() {
            None => match state.interactive_pop.borrow().as_ref() {
                None => path.len(),
                Some(ip) => ip.to_path.len(),
            },
            Some(t) => match t.direction {
                TransitionDir::Push => t.from_path.len(),
                TransitionDir::Pop | TransitionDir::PopToRoot => t.to_path.len(),
            },
        };
```

Change the `(base_fx, base_alpha)` computation. Replace:

```rust
        let (base_fx, base_alpha): (f32, f32) = match state.transition.as_ref() {
            None => (0.0, 1.0),
            Some(t) => {
                let raw_t = t.controller.value();
                let eased = self.transition_curve.transform(raw_t);
                base_fx_alpha(t.direction, self.effective_platform(), eased)
            }
        };
```

To:

```rust
        let (base_fx, base_alpha): (f32, f32) = match state.transition.as_ref() {
            None => match state.interactive_pop.borrow().as_ref() {
                None => (0.0, 1.0),
                Some(ip) => {
                    let raw_t = ip.controller.value();
                    let eased = self.transition_curve.transform(raw_t);
                    base_fx_alpha(TransitionDir::Pop, self.effective_platform(), eased)
                }
            },
            Some(t) => {
                let raw_t = t.controller.value();
                let eased = self.transition_curve.transform(raw_t);
                base_fx_alpha(t.direction, self.effective_platform(), eased)
            }
        };
```

**7d. Build the overlay for the interactive pop.** The existing overlay-building block is inside `if let Some(t) = state.transition.as_ref() { ... }`. Add a parallel `else if let Some(ip) = ...` block. Find the existing block that starts with `if let Some(t) = state.transition.as_ref() {` (the overlay block, around line 704) and ends with the closing `}` before `// Wrap the content Stack in a clipping DecoratedBox`. After that closing `}`, add:

```rust
        if state.transition.is_none() {
            if let Some(ip) = state.interactive_pop.borrow().as_ref() {
                let raw_t = ip.controller.value();
                let eased = self.transition_curve.transform(raw_t);
                let platform = self.effective_platform();

                let transition_fn: Rc<
                    dyn Fn(&TransitionCtx, Box<dyn Widget>) -> Box<dyn Widget>,
                > = self
                    .transition
                    .clone()
                    .unwrap_or_else(|| Rc::new(|ctx, child| default_transition(ctx, child)));

                // Outgoing page slides away to the right, revealing the
                // destination underneath. Same default_mobile_transition as a
                // button pop, driven by the finger/spring eased value.
                let outgoing_page = if let Some(top) = ip.from_path.last() {
                    (self.destination)(top)
                } else {
                    self.root.clone_boxed()
                };
                let outgoing_ctx = TransitionCtx {
                    t: eased,
                    is_incoming: false,
                    direction: TransitionDir::Pop,
                    platform,
                };
                let overlay = transition_fn(&outgoing_ctx, outgoing_page);
                content_stack = content_stack.push(
                    Positioned::new(overlay)
                        .top(0.0)
                        .right(0.0)
                        .bottom(0.0)
                        .left(0.0),
                );
            }
        }
```

**7e. Cache `content_width` from MediaQuery.** Find the line `let safe_insets = MediaQuery::of(ctx).padding;` (around line 595) and add after it:

```rust
        let mq = MediaQuery::of(ctx);
        state.content_width = mq.size.width;
        let safe_insets = mq.padding;
```

(Replace the existing `let safe_insets = MediaQuery::of(ctx).padding;` line with the above three lines.)

**7f. Wrap the final output in `EdgePanDetector`.** The `render` method currently returns `MultiChild::new(children![nav_bar, content], ...).boxed()`. Wrap that in `EdgePanDetector`. Change the final return from:

```rust
        MultiChild::new(
            children![nav_bar, content],
            Layout::column()
                .flex_grow(1.0)
                .flex_basis(0.0)
                .min_height(0.0),
        )
        .boxed()
    }
```

To:

```rust
        let column = MultiChild::new(
            children![nav_bar, content],
            Layout::column()
                .flex_grow(1.0)
                .flex_basis(0.0)
                .min_height(0.0),
        );

        // Wrap in EdgePanDetector (always present — stable widget type so the
        // reconciler updates in place when `enabled` toggles between root and
        // non-root). Enabled only on mobile when a pop is possible and no
        // transition/interactive-pop is already in flight.
        let platform = self.effective_platform();
        let can_swipe = platform == Platform::Mobile
            && self.controller.depth() > 0
            && state.transition.is_none()
            && state.interactive_pop.borrow().is_none()
            && self.controller.pending().is_none();

        // Captures for the gesture closures. These are Rc clones / copies —
        // the closures are `move` and fire outside render(), mutating the
        // shared cell and firing dirty to trigger a rebuild.
        let controller = self.controller.clone();
        let ip_cell = state.interactive_pop.clone();
        let dirty_cb = state.dirty_callback.clone();
        let ticker = state.ticker.clone();
        let content_width = state.content_width;

        EdgePanDetector::new(column, can_swipe)
            .on_start(move || {
                let Some(from_path) = controller.begin_interactive_pop() else {
                    return;
                };
                let to_path = if from_path.len() > 1 {
                    from_path[..from_path.len() - 1].to_vec()
                } else {
                    Vec::new()
                };
                let mut controller_anim =
                    AnimationController::new(DEFAULT_MOBILE_TRANSITION_DURATION);
                if let Some(ticker) = &ticker {
                    controller_anim.set_ticker(ticker.clone());
                }
                if let Some(cb) = &dirty_cb {
                    controller_anim.set_dirty_callback(cb.clone());
                }
                *ip_cell.borrow_mut() = Some(InteractivePop {
                    controller: controller_anim,
                    from_path,
                    to_path,
                    phase: InteractivePopPhase::Dragging,
                    velocity_tracker: VelocityTracker::new(),
                });
                if let Some(cb) = &dirty_cb {
                    cb();
                }
            })
            .on_update(move |delta_x| {
                let mut ip_cell = ip_cell.borrow_mut();
                let Some(ip) = ip_cell.as_mut() else {
                    return;
                };
                let progress = if content_width > 0.0 {
                    (delta_x / content_width).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                ip.controller.set_value(progress as f64);
                ip.velocity_tracker.add(Instant::now(), progress);
                drop(ip_cell);
                if let Some(cb) = &dirty_cb {
                    cb();
                }
            })
            .on_end(move |_final_delta_x| {
                let mut ip_cell = ip_cell.borrow_mut();
                let Some(ip) = ip_cell.as_mut() else {
                    return;
                };
                let progress = ip.controller.value() as f32;
                let velocity = ip.velocity_tracker.velocity();
                let phase = if progress > 0.5 || velocity > FLICK_THRESHOLD {
                    InteractivePopPhase::Committing
                } else {
                    InteractivePopPhase::Cancelling
                };
                ip.phase = phase;
                let target = if phase == InteractivePopPhase::Committing {
                    1.0
                } else {
                    0.0
                };
                ip.controller.animate_with(Box::new(SpringSimulation::new(
                    SpringDescription::ios(340.0, 1.0),
                    progress as f64,
                    target,
                    velocity as f64,
                )));
                drop(ip_cell);
                if let Some(cb) = &dirty_cb {
                    cb();
                }
            })
            .boxed()
    }
```

- [ ] **Step 8: Build and fix any compile errors**

Run: `cargo build -p vexo_uikit`
Expected: compiles. If there are borrow errors (e.g., `state.interactive_pop` borrowed while building closures), note that the closures capture `state.interactive_pop.clone()` (an `Rc` clone) — this is a cheap refcount bump, not a borrow. The `let ip_cell = state.interactive_pop.clone();` line handles this. Ensure the closure captures are `move` and capture `Rc` clones, not `&mut` refs.

- [ ] **Step 9: Run existing navigation tests to confirm no regression**

Run: `cargo test -p vexo_uikit --test navigation_animation_tests && cargo test -p vexo_uikit --test navigation_stack_tests`
Expected: PASS (all existing tests still pass).

- [ ] **Step 10: Commit**

```bash
git add vexo_uikit/src/navigation.rs
git commit -m "feat(navigation): integrate interactive swipe-to-pop into NavigationStackView

Adds InteractivePop state machine driven by EdgePanDetector gestures.
Finger drives AnimationController::set_value; release fires a spring
to commit (1.0) or cancel (0.0). Reuses base_fx_alpha +
default_mobile_transition with finger-driven eased value. Mobile-only,
disabled at root / during transitions."
```

---

### Task 7: Integration tests for interactive pop

**Files:**
- Create: `vexo_uikit/tests/navigation_interactive_pop_tests.rs`

**Interfaces:**
- Consumes: `NavigationController`, `NavigationStackView`, `NavigationStackViewState` (Task 6)

- [ ] **Step 1: Write the integration tests**

Create `vexo_uikit/tests/navigation_interactive_pop_tests.rs`:

```rust
//! Integration tests for interactive (swipe-to-pop) navigation.
//!
//! These tests drive the NavigationStackViewState's interactive-pop state
//! machine directly (without the full gesture pipeline) by manipulating the
//! shared `interactive_pop` cell and the NavigationController API. Full
//! end-to-end gesture tests are manual (see the design spec's manual
//! verification checklist).

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vexo::animation::{AnimationTicker, SpringDescription, SpringSimulation};
use vexo::inherited_registry::{InheritedMap, InheritedRegistry};
use vexo::{BuildOwner, ElementKey, RenderContext, Text, VelocityTracker, Widget};
use vexo_uikit::transitions::TransitionDir;
use vexo_uikit::{
    Component, NavigationController, NavigationStackView, NavigationStackViewState,
};

fn make_element_key() -> ElementKey {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    sm.insert(())
}

fn create_render_context<'a>(
    element_id: ElementKey,
    build_owner: &'a BuildOwner,
) -> RenderContext<'a> {
    let inherited_map = InheritedMap::empty();
    let inherited_registry = InheritedRegistry::new();
    RenderContext::new(
        element_id,
        build_owner,
        &inherited_map,
        &inherited_registry,
        Arc::new(|| {}),
    )
}

fn render_stack<Dest: std::hash::Hash + Eq + Clone + 'static>(
    view: NavigationStackView<Dest>,
    state: &mut NavigationStackViewState<Dest>,
) -> Box<dyn Widget> {
    let element_id = make_element_key();
    let build_owner = BuildOwner::new();
    let mut ctx = create_render_context(element_id, &build_owner);
    view.render(state, &mut ctx)
}

fn collect_text(w: &dyn Widget, out: &mut Vec<String>) {
    if let Some(t) = w.as_any().downcast_ref::<Text>() {
        out.push(t.content().to_string());
    }
    if let Some(child) = w.child() {
        collect_text(child, out);
    }
    for child in w.children() {
        collect_text(child.as_ref(), out);
    }
}

fn all_text(w: &dyn Widget) -> Vec<String> {
    let mut out = Vec::new();
    collect_text(w, &mut out);
    out
}

fn make_view(
    controller: NavigationController<&'static str>,
) -> NavigationStackView<&'static str> {
    NavigationStackView::new(controller, Text::new("Root"))
        .root_title("Home")
        .title(|d| d.to_string())
        .destination(|d| Text::new(format!("Page: {}", d)).boxed())
}

fn push_and_clear(controller: &NavigationController<&'static str>, dest: &'static str) {
    controller.push(dest);
    controller.clear_pending();
}

#[test]
fn interactive_pop_renders_both_pages_during_drag() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    push_and_clear(&controller, "a");
    let view = make_view(controller.clone());
    let mut state = NavigationStackViewState::default();

    // Wire a dummy ticker + dirty so the controller can be created in on_start.
    state.ticker = Some(Arc::new(AnimationTicker::new()));
    state.dirty_callback = Some(Arc::new(|| {}));

    // Begin interactive pop and inject a half-progress InteractivePop.
    let from_path = controller
        .begin_interactive_pop()
        .expect("non-empty path with no pending");
    let to_path: Vec<&'static str> = Vec::new();
    let mut anim = vexo::AnimationController::new(Duration::from_millis(350));
    anim.set_ticker(state.ticker.clone().unwrap());
    anim.set_dirty_callback(state.dirty_callback.clone().unwrap());
    anim.set_value(0.5);
    *state.interactive_pop.borrow_mut() = Some(vexo_uikit::InteractivePop {
        controller: anim,
        from_path,
        to_path,
        phase: vexo_uikit::InteractivePopPhase::Dragging,
        velocity_tracker: VelocityTracker::new(),
    });

    let w = render_stack(view, &mut state);
    let texts = all_text(&w);
    // Should contain BOTH the root ("Root" / "Home") and the outgoing page
    // ("Page: a"), because the interactive pop renders the overlay (outgoing)
    // over the base (destination = root).
    assert!(
        texts.iter().any(|t| t.contains("Page: a")),
        "outgoing page must render during interactive pop, got {:?}",
        texts
    );
}

#[test]
fn interactive_pop_commit_clears_state_and_pops_path() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    push_and_clear(&controller, "a");
    let view = make_view(controller.clone());
    let mut state = NavigationStackViewState::default();
    state.ticker = Some(Arc::new(AnimationTicker::new()));
    state.dirty_callback = Some(Arc::new(|| {}));

    let from_path = controller.begin_interactive_pop().unwrap();
    let to_path: Vec<&'static str> = Vec::new();
    let mut anim = vexo::AnimationController::new(Duration::from_millis(350));
    anim.set_ticker(state.ticker.clone().unwrap());
    anim.set_dirty_callback(state.dirty_callback.clone().unwrap());
    // Spring to completion.
    anim.animate_with(Box::new(SpringSimulation::new(
        SpringDescription::ios(340.0, 1.0),
        0.5,
        1.0,
        0.0,
    )));
    *state.interactive_pop.borrow_mut() = Some(vexo_uikit::InteractivePop {
        controller: anim,
        from_path,
        to_path,
        phase: vexo_uikit::InteractivePopPhase::Committing,
        velocity_tracker: VelocityTracker::new(),
    });

    // Advance the spring past settlement. A critically-damped iOS spring at
    // stiffness=340 settles well within 1s.
    let start = Instant::now();
    for _ in 0..100 {
        let now = start + Duration::from_millis(20 * 100);
        if let Some(ip) = state.interactive_pop.borrow_mut().as_mut() {
            ip.controller.advance(start + Duration::from_millis(20));
        }
    }

    // render() should detect completion, call commit_interactive_pop, and
    // clear the cell.
    let _w = render_stack(view, &mut state);

    assert!(
        state.interactive_pop.borrow().is_none(),
        "interactive_pop cell must be cleared after commit"
    );
    assert_eq!(
        controller.path(),
        Vec::<&'static str>::new(),
        "path must be popped after commit"
    );
}

#[test]
fn interactive_pop_cancel_clears_state_without_mutating_path() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    push_and_clear(&controller, "a");
    let view = make_view(controller.clone());
    let mut state = NavigationStackViewState::default();
    state.ticker = Some(Arc::new(AnimationTicker::new()));
    state.dirty_callback = Some(Arc::new(|| {}));

    let from_path = controller.begin_interactive_pop().unwrap();
    let to_path: Vec<&'static str> = Vec::new();
    let mut anim = vexo::AnimationController::new(Duration::from_millis(350));
    anim.set_ticker(state.ticker.clone().unwrap());
    anim.set_dirty_callback(state.dirty_callback.clone().unwrap());
    anim.animate_with(Box::new(SpringSimulation::new(
        SpringDescription::ios(340.0, 1.0),
        0.5,
        0.0,
        0.0,
    )));
    *state.interactive_pop.borrow_mut() = Some(vexo_uikit::InteractivePop {
        controller: anim,
        from_path,
        to_path,
        phase: vexo_uikit::InteractivePopPhase::Cancelling,
        velocity_tracker: VelocityTracker::new(),
    });

    let start = Instant::now();
    for _ in 0..100 {
        if let Some(ip) = state.interactive_pop.borrow_mut().as_mut() {
            ip.controller.advance(start + Duration::from_millis(20));
        }
    }

    let _w = render_stack(view, &mut state);

    assert!(
        state.interactive_pop.borrow().is_none(),
        "interactive_pop cell must be cleared after cancel"
    );
    assert_eq!(
        controller.path(),
        vec!["a"],
        "path must be unchanged after cancel"
    );
}
```

**Note:** The tests reference `vexo_uikit::InteractivePop` and `vexo_uikit::InteractivePopPhase`, which must be `pub`. In Task 6 Step 2, the `InteractivePop` struct and `InteractivePopPhase` enum were defined without `pub`. Update them to `pub` in `vexo_uikit/src/navigation.rs`:

```rust
pub struct InteractivePop<Dest: Hash + Eq + Clone + 'static> { ... }
pub enum InteractivePopPhase { ... }
```

And add them to `vexo_uikit/src/lib.rs` re-exports (find the existing `pub use navigation::NavigationStackView;` line and add `InteractivePop, InteractivePopPhase,`).

- [ ] **Step 2: Run the integration tests**

Run: `cargo test -p vexo_uikit --test navigation_interactive_pop_tests -- --nocapture`
Expected: PASS (3 tests). If the commit test fails because the spring hasn't settled, increase the advance loop count or the per-step time.

- [ ] **Step 3: Run the full test suite for both crates**

Run: `cargo test -p vexo && cargo test -p vexo_uikit`
Expected: PASS (all tests).

- [ ] **Step 4: Commit**

```bash
git add vexo_uikit/tests/navigation_interactive_pop_tests.rs vexo_uikit/src/navigation.rs vexo_uikit/src/lib.rs
git commit -m "test(navigation): add interactive swipe-to-pop integration tests

Verifies drag-phase renders both pages, commit clears state + pops path,
and cancel clears state without mutating path. Exposes InteractivePop
types as pub for test access."
```

---

## Manual Verification (per CLAUDE.md — user-run)

After all tasks pass, ask the user to run `cargo run -p desktop_demo` and verify:

- [ ] Push a page, swipe from left edge → page follows finger, destination dimmed underneath
- [ ] Release past halfway → springs to completion, path pops
- [ ] Release before halfway → springs back, path unchanged
- [ ] Flick quickly from edge → commits even if progress < 0.5
- [ ] Swipe at root → no-op, root scroll still works
- [ ] Swipe during push animation → no-op
- [ ] State preservation: edit text on pushed page, swipe-pop-cancel, re-push → edits intact
- [ ] State preservation: edit text on pushed page, swipe-pop-commit, re-push → fresh page

## Self-Review Checklist

- [ ] Spec coverage: all 6 spec components have tasks (recognizer ✓, detector ✓, set_value ✓, event_handler ✓, controller API ✓, state machine ✓)
- [ ] No placeholders: all code blocks are complete
- [ ] Type consistency: `InteractivePop`, `InteractivePopPhase`, `begin/commit/cancel_interactive_pop` names match across tasks
- [ ] Edge cases from spec all handled by the state machine (root → can_swipe false; during transition → can_swipe false; desktop → can_swipe false)
