//! Context menu widget trio: `MenuBuilder`, `ContextMenuController`, `ContextMenu` host.
//!
//! Mirrors the `ScrollController` pattern: the screen owns a controller,
//! wraps its root in `ContextMenu::new(child, controller)`, and wraps each
//! right-clickable element in `context_menu_trigger(child, controller, builder)`.
//!
//! The menu's visual content is fully caller-supplied via `MenuBuilder`. The
//! builder runs at render time (inside `ContextMenu::render`), so it always
//! reads the current theme. Each trigger captures its own builder, so different
//! bubbles can render different menu styles.
//!
//! Task 5 state: open/close is driven by a critical spring
//! (`SpringDescription::ios(340.0, 1.0)`) through a 4-state phase machine
//! (`Closed → Opening → Open → Closing → Closed`). `show()` starts a forward
//! spring (current value → 1.0, phase=Opening); `close()` starts a reverse
//! spring (current value → 0.0, phase=Closing). Both retarget from the live
//! value, so early close / re-show produce no jump. The host's `on_tick`
//! calls `controller.advance(now)`, which samples the spring and flips
//! Opening→Open or Closing→Closed (the latter clears `open` → unmount) on
//! settle.
//!
//! Task 6 wires the spring value `v = controller.animation_value()` (0→1 on
//! open, 1→0 on close) into all three overlay layers: the dim barrier's alpha
//! is `v*0.4` (was fixed 0.4), the bright bubble copy gets a subtle
//! `scale(1+v*0.03)` + `translate_y(-v*4.0)` lift (opacity stays 1.0), and the
//! actions card gets `scale(0.8+v*0.2)` + `opacity(v)`. Scale-about-center is
//! achieved via the `scale_about_center` helper (a translate→scale→translate
//! `Transform` chain). `Opacity` is paint-only (layout + hit-test
//! pass-through), so the dim barrier stays tappable even at v≈0 — barrier
//! dismiss works mid-open (test #5). The reactions pill is still not rendered
//! (Task 7 adds it with positioning).

use std::any::Any;
use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vexo::animation::{AnimationController, AnimationTicker, SpringDescription, SpringSimulation};
use vexo::core::{Bounds, Logical, Size};
use vexo::{Component, ComponentState, LifecycleContext, RenderContext, Widget};

// ============================================================================
// MenuContent + MenuMetrics + MenuBuilder
// ============================================================================

/// The two cards produced by a menu builder.
///
/// `reactions` is the top pill (emoji/reaction strip); `actions` is the lower
/// card (Copy / Reply / Delete rows). The host positions them relative to
/// `bubble_bounds` using `metrics` for spacing. Task 2 renders only `actions`;
/// Task 3 adds the reactions pill and proper styling.
pub struct MenuContent {
    pub reactions: Box<dyn Widget>,
    pub actions: Box<dyn Widget>,
    pub metrics: MenuMetrics,
}

/// Size hints for positioning + transform anchors. These are estimates used
/// by the host to position cards and compute scale-about-center transforms
/// before layout runs. The actual laid-out sizes may differ slightly; these
/// are tuned during implementation.
pub struct MenuMetrics {
    pub reactions_size: Size<Logical>,
    pub actions_size: Size<Logical>,
    pub gap: f32,
}

/// Caller-supplied factory that produces the menu's `MenuContent`.
///
/// Wraps `Rc<dyn Fn(&ContextMenuController, &ThemeData) -> MenuContent>`.
/// `Rc<dyn Fn>` (not `FnMut`): the builder is cloned into the controller's
/// cell and re-invoked on every rebuild; `Rc` keeps clones cheap and matches
/// the single-threaded pattern used elsewhere in `vexo_uikit` (no `Send +
/// Sync` bounds that `Arc` would impose).
///
/// The builder runs inside `ContextMenu::render`, so it always sees the live
/// `ThemeData` — theme toggles re-render the menu correctly. It receives
/// `&ContextMenuController` so its item rows can call `controller.close()` on
/// tap.
#[derive(Clone)]
pub struct MenuBuilder(Rc<dyn Fn(&ContextMenuController, &vexo::ThemeData) -> MenuContent>);

impl MenuBuilder {
    pub fn new(
        f: impl Fn(&ContextMenuController, &vexo::ThemeData) -> MenuContent + 'static,
    ) -> Self {
        Self(Rc::new(f))
    }
}

impl Deref for MenuBuilder {
    type Target = dyn Fn(&ContextMenuController, &vexo::ThemeData) -> MenuContent;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

// ============================================================================
// Phase + OpenState + ContextMenuController
// ============================================================================

/// Lifecycle phase of the context menu. The 4-state phase machine is driven
/// by a critical spring: `show()` → `Opening`, settle → `Open`; `close()` →
/// `Closing`, settle → `Closed` (clears `open`, unmounting the menu).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Closed,
    Opening,
    Open,
    Closing,
}

/// Snapshot of what `show()` was called with. Held in the controller's shared
/// cell while the menu is open; cleared on `close()`.
struct OpenState {
    bubble_bounds: Bounds<Logical>,
    bubble_widget: Box<dyn Widget>,
    builder: MenuBuilder,
}

/// Shared (across controller clones) mutable state.
struct Shared {
    phase: Phase,
    open: Option<OpenState>,
    /// The critical spring driving `Opening`/`Closing`. Same spring as
    /// KeyboardAvoidance/SlideTransition: `SpringDescription::ios(340.0, 1.0)`.
    /// `show()`/`close()` start a forward/reverse spring from the current
    /// value (smooth retarget — no jump on early close or re-show during
    /// close). The host's `on_tick` calls `advance(now)` to sample the spring
    /// and flip phases on settle.
    animation: AnimationController,
    /// Cached so `show()`/`close()` can (re-)wire the `AnimationController`
    /// even if called before the host's `on_mount` (e.g. in tests). Set by
    /// `set_animation_ticker` (called from `on_mount`/`on_update`).
    ticker: Option<Arc<AnimationTicker>>,
    /// Wired by the host's `on_mount`/`on_update`. Invoked by the
    /// `AnimationController` (via `animate_with` + per-tick registration) so
    /// the host rebuilds and re-reads `phase()`/`open_snapshot()`. This
    /// replaces the old `Signal<Option<Point>>` + `signal_value` path — the
    /// builder is `!Send + !Sync` (`Rc<dyn Fn>`), so it can't travel through a
    /// `Signal`, and the controller now owns the open state directly.
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// Controller for a context menu — owns open/close state, the current builder,
/// the animation phase, and hooks for the dirty callback + animation ticker.
///
/// Created by the screen's caller (alongside `ScrollController::new()`), held
/// as a field, `.clone()`d into triggers and the host. The `Rc<RefCell<Shared>>`
/// shares underlying state across clones, so widget-struct recreation on
/// rebuild doesn't lose menu state.
#[derive(Clone)]
pub struct ContextMenuController {
    shared: Rc<RefCell<Shared>>,
}

impl ContextMenuController {
    pub fn new() -> Self {
        Self {
            shared: Rc::new(RefCell::new(Shared {
                phase: Phase::Closed,
                open: None,
                // Duration is unused for the spring path (`animate_with`
                // ignores it — the sim owns the timeline), but the controller
                // requires one for the time-tween path. 600ms matches the
                // critical k=340 spring's perceptual settle.
                animation: AnimationController::new(Duration::from_millis(600)),
                ticker: None,
                dirty_callback: None,
            })),
        }
    }

    /// Open the menu anchored to `bubble_bounds`, carrying a clone of the
    /// bubble widget (Task 6 renders a lifted copy). Starts a forward spring
    /// (current value → 1.0) and sets phase to `Opening`. The spring retargets
    /// from the current value, so calling `show()` during a `Closing` spring
    /// (re-show) produces no jump — the spring reverses direction smoothly.
    /// `on_tick` flips `Opening` → `Open` when the spring settles.
    pub fn show(
        &self,
        bubble_bounds: Bounds<Logical>,
        bubble_widget: Box<dyn Widget>,
        builder: MenuBuilder,
    ) {
        let mut s = self.shared.borrow_mut();
        s.open = Some(OpenState {
            bubble_bounds,
            bubble_widget,
            builder,
        });
        s.phase = Phase::Opening;
        // (Re-)wire the ticker + dirty callback into the AnimationController.
        // The host does this in on_mount/on_update, but show() may be called
        // before on_mount (e.g. in tests) or with a stale wiring after a
        // retarget, so we refresh here to be safe. Clone to locals first so
        // the immutable borrow of `s.ticker`/`s.dirty_callback` ends before
        // the mutable borrow of `s.animation`.
        if let Some(ticker) = s.ticker.clone() {
            s.animation.set_ticker(ticker);
        }
        if let Some(cb) = s.dirty_callback.clone() {
            s.animation.set_dirty_callback(cb);
        }
        // Forward spring from the *current* value (smooth retarget). v0=0
        // because the user gesture that triggered show() has no carried
        // velocity (unlike a scroll fling).
        let from = s.animation.value();
        s.animation.animate_with(Box::new(SpringSimulation::new(
            SpringDescription::ios(340.0, 1.0),
            from,
            1.0,
            0.0,
        )));
        // `animate_with` fires the dirty callback immediately (avoids the
        // render_retain deadlock: without it, the callback only fires on the
        // next tick(), which only runs inside render_retain(), which only
        // runs when a frame is already requested). The callback is an mpsc
        // send — no RefCell reentry — so holding borrow_mut here is safe.
    }

    /// Close the menu. Starts a reverse spring (current value → 0.0) and
    /// sets phase to `Closing`. The spring retargets from the current value,
    /// so calling `close()` during an `Opening` spring (early close) produces
    /// no jump — the spring reverses direction smoothly. `on_tick` flips
    /// `Closing` → `Closed` and clears `open` (unmount) when the spring
    /// settles. No-op if already `Closed`.
    pub fn close(&self) {
        let mut s = self.shared.borrow_mut();
        if s.phase == Phase::Closed {
            return;
        }
        s.phase = Phase::Closing;
        if let Some(ticker) = s.ticker.clone() {
            s.animation.set_ticker(ticker);
        }
        if let Some(cb) = s.dirty_callback.clone() {
            s.animation.set_dirty_callback(cb);
        }
        // Reverse spring from the current value (smooth retarget).
        let from = s.animation.value();
        s.animation.animate_with(Box::new(SpringSimulation::new(
            SpringDescription::ios(340.0, 1.0),
            from,
            0.0,
            0.0,
        )));
    }

    pub fn phase(&self) -> Phase {
        self.shared.borrow().phase
    }

    /// The live spring value in `[0.0, 1.0]`. Drives the open/close transforms
    /// (Task 6) and the dim opacity (Task 6). Read at render time.
    pub fn animation_value(&self) -> f64 {
        self.shared.borrow().animation.value()
    }

    /// Store the animation ticker and wire it into the `AnimationController`.
    /// Called by the host's `on_mount`/`on_update`. The ticker is what lets
    /// `on_tick` fire: `animate_with` registers the controller's dirty
    /// callback with the ticker, so `ticker.tick()` marks the host element
    /// dirty, and the next `perform_rebuilds()` calls `on_tick` → `advance`.
    pub fn set_animation_ticker(&self, t: Arc<AnimationTicker>) {
        let mut s = self.shared.borrow_mut();
        s.ticker = Some(t.clone());
        s.animation.set_ticker(t);
    }

    /// Store the host's dirty callback and wire it into the
    /// `AnimationController`. Invoked by `animate_with` (on start) and on
    /// every spring sample (inside `advance`) to trigger a host rebuild.
    pub fn set_dirty_callback(&self, cb: Arc<dyn Fn() + Send + Sync>) {
        let mut s = self.shared.borrow_mut();
        s.dirty_callback = Some(cb.clone());
        s.animation.set_dirty_callback(cb);
    }

    /// Advance the spring and handle phase transitions. Called by the host's
    /// `on_tick` (which fires every frame via `perform_rebuilds` →
    /// `element.animate(now)` → `state.on_tick(now)`).
    ///
    /// On settle (`!is_animating()`):
    /// - `Opening` → `Open` (menu fully shown; spring holds at 1.0).
    /// - `Closing` → `Closed` + clear `open` (menu unmounts; spring holds at 0.0).
    ///
    /// No-op when `Closed` (no spring running, nothing to advance). This
    /// guards against the host's `on_tick` firing after the menu has already
    /// settled closed — without it, `advance` would re-sample a stopped
    /// controller and potentially re-fire the dirty callback.
    pub(crate) fn advance(&self, now: Instant) {
        let mut s = self.shared.borrow_mut();
        if s.phase == Phase::Closed {
            return;
        }
        s.animation.advance(now);

        // Check for settle. `advance` sets `drive = Stopped` and unregisters
        // from the ticker when the sim reports `is_done`; `is_animating()`
        // reflects that.
        if !s.animation.is_animating() {
            match s.phase {
                Phase::Opening => {
                    s.phase = Phase::Open;
                }
                Phase::Closing => {
                    s.phase = Phase::Closed;
                    s.open = None;
                }
                _ => {}
            }
        }
    }

    /// Snapshot the current open state (clones bounds, clones the bubble
    /// widget, clones the builder). Returns `None` when closed.
    /// Called by the host during `render()` only when `phase() != Closed`.
    /// The bubble widget isn't used by the host in Task 2 (Task 6 renders a
    /// lifted copy during the open animation), but it's returned here so the
    /// snapshot signature is stable across tasks.
    pub(crate) fn open_snapshot(&self) -> Option<(Bounds<Logical>, Box<dyn Widget>, MenuBuilder)> {
        let s = self.shared.borrow();
        s.open.as_ref().map(|o| {
            (
                o.bubble_bounds,
                o.bubble_widget.clone_boxed(),
                o.builder.clone(),
            )
        })
    }
}

impl Default for ContextMenuController {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ContextMenu host (Component) + ContextMenuHostState
// ============================================================================

/// A host widget that renders a context menu overlay on top of its child.
///
/// Wrap the screen's root content in `ContextMenu::new(content, controller)`.
/// When `controller.show(bubble_bounds, bubble_widget, builder)` is called
/// (e.g. by a `context_menu_trigger` on right-click), the host rebuilds and
/// shows a floating menu at the bubble position — the menu's content is
/// whatever the caller's `builder` returns — with a full-size dismiss barrier
/// behind it.
///
/// The host must be OUTSIDE any `ScrollView` (which clips with `overflow:
/// Hidden`). Place it at the screen root so the menu floats above all content.
pub struct ContextMenu {
    controller: ContextMenuController,
    child: Box<dyn Widget>,
}

impl ContextMenu {
    pub fn new(child: impl Widget + 'static, controller: ContextMenuController) -> Self {
        Self {
            controller,
            child: Box::new(child),
        }
    }
}

impl Clone for ContextMenu {
    fn clone(&self) -> Self {
        Self {
            controller: self.controller.clone(),
            child: self.child.clone_boxed(),
        }
    }
}

/// Host state for `ContextMenu`. Wires the controller's dirty callback AND
/// animation ticker in `on_mount`/`on_update` so `show()`/`close()` (called
/// from event handlers or programmatically) trigger a host rebuild, and the
/// spring is advanced each frame. Stores a clone of the controller so
/// `on_tick` can call `controller.advance(now)` — the controller isn't
/// reachable from `on_tick`'s signature (no `LifecycleContext`).
pub struct ContextMenuHostState {
    controller: Option<ContextMenuController>,
}

impl Default for ContextMenuHostState {
    fn default() -> Self {
        Self { controller: None }
    }
}

impl ComponentState for ContextMenuHostState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(menu) = ctx.widget().downcast_ref::<ContextMenu>() {
            self.controller = Some(menu.controller.clone());
            menu.controller.set_dirty_callback(ctx.dirty_callback());
            menu.controller
                .set_animation_ticker(ctx.animation_ticker().clone());
        }
    }
    fn on_update(&mut self, _old: &dyn Any, ctx: &mut LifecycleContext) {
        // Re-wire on every parent-cascade update. The controller is shared via
        // Rc<RefCell>, so identity comparison (Rc::ptr_eq) isn't meaningful
        // here — the widget struct is recreated each rebuild but the shared
        // cell persists. Just re-store the current dirty callback + ticker.
        if let Some(menu) = ctx.widget().downcast_ref::<ContextMenu>() {
            self.controller = Some(menu.controller.clone());
            menu.controller.set_dirty_callback(ctx.dirty_callback());
            menu.controller
                .set_animation_ticker(ctx.animation_ticker().clone());
        }
    }
    fn on_tick(&mut self, now: Instant) {
        // Advance the spring and flip phases on settle (Opening→Open,
        // Closing→Closed + clear open). The host element is marked dirty by
        // the controller's dirty callback (fired by `animate_with` on start
        // and by `advance` on each sample), so `perform_rebuilds` →
        // `element.animate(now)` → `state.on_tick(now)` reaches here every
        // frame while the spring is active.
        if let Some(ctrl) = &self.controller {
            ctrl.advance(now);
        }
    }
}

impl Component for ContextMenu {
    type State = ContextMenuHostState;

    fn render(
        &self,
        _state: &mut ContextMenuHostState,
        ctx: &mut RenderContext,
    ) -> Box<dyn Widget> {
        let theme = vexo::Theme::of(ctx);
        let phase = self.controller.phase();
        let v = self.controller.animation_value();

        let mut stack = vexo::Stack::new().push(self.child.clone_boxed());

        if phase != Phase::Closed {
            if let Some((bubble_bounds, bubble_widget, builder)) = self.controller.open_snapshot() {
                let controller = self.controller.clone();
                // Run the builder up-front so `metrics` is available for the
                // card's scale-about-center anchor + gap positioning. The
                // builder runs at render time (reads the live theme).
                let content = builder(&controller, &theme);
                let metrics = content.metrics;

                // [2] Dim barrier — alpha = v * 0.4 (was fixed 0.4 in Task 4).
                // Structure: Positioned(0,0,0,0) → GestureDetector.on_press(→
                // close) → Opacity(v*0.4) → DecoratedBox(BLACK) →
                // WithLayout(width_percent=1.0, height_percent=1.0) → Text("").
                //
                // CRITICAL: `DecoratedBox` is a pass-through render object —
                // it inherits its *child's* bounds, not its parent's. So
                // `DecoratedBox` must WRAP `WithLayout` (not the other way
                // around). `WithLayout`'s width_percent/height_percent give it
                // full-screen bounds; `DecoratedBox` then inherits those
                // full-screen bounds and paints the black.
                //
                // Opacity is paint-only (layout + hit-test pass-through), so
                // even at v≈0 (dim invisible) the GestureDetector still
                // receives the press → barrier dismiss works mid-open. This
                // is the property test #5 relies on.
                let ctrl_for_barrier = controller.clone();
                let dim_alpha = (v * 0.4) as f32;
                let barrier = vexo::Positioned::new(
                    vexo::GestureDetector::new(vexo::Opacity::new(
                        vexo::DecoratedBox::with_style(
                            vexo::WithLayout::new(
                                vexo::Text::new(""),
                                vexo::Layout::default()
                                    .width_percent(1.0)
                                    .height_percent(1.0),
                            ),
                            vexo::Style::default().background(vexo::Color::BLACK),
                        ),
                        dim_alpha,
                    ))
                    .on_press(move || ctrl_for_barrier.close()),
                )
                .left(0.0)
                .top(0.0)
                .right(0.0)
                .bottom(0.0);
                stack = stack.push(barrier);

                // [3] Bright bubble copy — scale 1+v*0.03 (subtle grow), lift
                // -v*4.0px (rises as the menu opens), opacity 1.0 (always full
                // bright — the focal point). The scale is applied about the
                // bubble's center via scale_about_center; the lift is applied
                // to the Positioned.top offset (so the bubble rises as it
                // scales). Tappable to dismiss (matches iMessage: tapping the
                // lifted bubble closes the menu).
                let ctrl_for_bubble = controller.clone();
                let bubble_scale = 1.0 + v * 0.03;
                let bubble_lift = -(v * 4.0) as f32;
                let bw = bubble_bounds.width();
                let bh = bubble_bounds.height();
                let bubble_copy = vexo::Positioned::new(
                    vexo::GestureDetector::new(scale_about_center(
                        bubble_widget,
                        bubble_scale as f32,
                        bubble_scale as f32,
                        bw,
                        bh,
                    ))
                    .on_press(move || ctrl_for_bubble.close()),
                )
                .left(bubble_bounds.left)
                .top(bubble_bounds.top + bubble_lift);
                stack = stack.push(bubble_copy);

                // [5] Actions card — scale 0.8+v*0.2 (grows 80%→100%), opacity
                // v (fades in), positioned below the bubble with `metrics.gap`
                // (was hardcoded 8.0 in Task 4). Task 7 adds the reactions pill
                // above the bubble and proper edge-aware positioning.
                let card_scale = 0.8 + v * 0.2;
                let card_opacity = v as f32;
                let actions_x = bubble_bounds.left;
                let actions_y = bubble_bounds.top + bubble_bounds.height() + metrics.gap;
                let positioned_actions = vexo::Positioned::new(vexo::Opacity::new(
                    scale_about_center(
                        content.actions,
                        card_scale as f32,
                        card_scale as f32,
                        metrics.actions_size.width,
                        metrics.actions_size.height,
                    ),
                    card_opacity,
                ))
                .left(actions_x)
                .top(actions_y);
                stack = stack.push(positioned_actions);
            }
        }

        stack.boxed()
    }
}

/// Wrap `child` in a single `Transform` that scales about its center:
/// `M = translate(w/2, h/2) ∘ scale(sx, sy) ∘ translate(-w/2, -h/2)`.
///
/// Composing the three-step chain into ONE `AffineTransform` (rather than
/// three nested `Transform` widgets) is deliberate: the framework's
/// hit-tester checks `is_inside` against the child's bounds at EACH
/// `Transform` render object after applying that RO's inverse transform. With
/// three nested ROs (translate → scale → translate), the outer translate's
/// inverse shifts the point far from the origin (e.g. `(5,22)` → `(-95,-32)`),
/// failing the per-level bounds check — even at v=1 where the scale is
/// identity — so taps on the scaled card silently miss and fall through to
/// the dim barrier. A single composed `Transform` applies the full inverse in
/// one step, so `is_inside` is checked against the correctly-mapped point.
///
/// The composed matrix works out to `{ a: sx, d: sy, e: w/2*(1-sx),
/// f: h/2*(1-sy) }`: scale `(sx, sy)` with a compensating translation so the
/// center `(w/2, h/2)` stays fixed.
///
/// `TransformRenderObject` is a layout pass-through (`is_pass_through() ==
/// true`): the child's laid-out bounds propagate up unchanged, and the
/// transform is applied only at paint + hit-test time. So wrapping a widget
/// in `scale_about_center` does NOT change its `computed_bounds` — only its
/// painted appearance and hit region.
fn scale_about_center(child: Box<dyn Widget>, sx: f32, sy: f32, w: f32, h: f32) -> Box<dyn Widget> {
    let transform = vexo::AffineTransform::translation(w / 2.0, h / 2.0)
        .mul(&vexo::AffineTransform::scale(sx, sy))
        .mul(&vexo::AffineTransform::translation(-w / 2.0, -h / 2.0));
    vexo::Transform::new(child, transform).boxed()
}

// ============================================================================
// context_menu_trigger — sugar for wrapping a child with right-click detection
// ============================================================================

/// Wrap `child` with a right-click handler that opens the context menu
/// anchored to the child's global bounds, rendering content from `builder`.
///
/// The child widget is cloned and passed to `controller.show()` as the
/// `bubble_widget` (Task 6 renders a lifted copy of it during the open
/// animation). Equivalent to:
/// ```ignore
/// child.on_secondary_press(move |_pos, bounds| {
///     controller.show(bounds, child.clone_boxed(), builder);
/// })
/// ```
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    builder: MenuBuilder,
) -> Box<dyn Widget> {
    let ctrl = controller.clone();
    let bubble_widget = child.clone_boxed();
    child.on_secondary_press(move |_pos, bounds| {
        ctrl.show(bounds, bubble_widget.clone_boxed(), builder.clone());
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use vexo::core::ScaleSource;
    use vexo::input::{ButtonState, InputEvent, Modifiers, PointerButton};
    use vexo::layout::TaffyLayoutEngine;
    use vexo::platform::stub_clipboard::StubClipboard;
    use vexo::platform::Clipboard;
    use vexo::render_objects::TextRenderObject;
    use vexo::resource::new_font_system;
    use vexo::RenderObjectKey;
    use vexo::RenderObjectRegistry;
    use vexo::Text;
    use vexo::ThreeTreePipeline;

    fn test_clipboard() -> Arc<dyn Clipboard> {
        Arc::new(StubClipboard)
    }

    /// New-API builder: returns `MenuContent` with placeholder reactions and
    /// a `Text` actions card carrying `label`. Ignores controller + theme.
    fn test_content_builder(label: &'static str) -> MenuBuilder {
        MenuBuilder::new(move |_ctrl, _theme| MenuContent {
            reactions: vexo::Text::new("r").boxed(),
            actions: vexo::Text::new(label).boxed(),
            metrics: MenuMetrics {
                reactions_size: vexo::core::Size::new(150.0, 28.0),
                actions_size: vexo::core::Size::new(200.0, 108.0),
                gap: 8.0,
            },
        })
    }

    fn find_text_in_tree(reg: &RenderObjectRegistry, key: RenderObjectKey, needle: &str) -> bool {
        let ro = match reg.get(key) {
            Some(ro) => ro,
            None => return false,
        };
        if ro
            .as_any()
            .downcast_ref::<TextRenderObject>()
            .map_or(false, |t| t.content().contains(needle))
        {
            return true;
        }
        for &child in ro.children() {
            if find_text_in_tree(reg, child, needle) {
                return true;
            }
        }
        false
    }

    #[test]
    fn test_controller_show_close_new_api() {
        let controller = ContextMenuController::new();
        assert_eq!(controller.phase(), Phase::Closed);
        assert!((controller.animation_value() - 0.0).abs() < 1e-9);

        // show() now starts a forward spring — phase is Opening, not Open.
        // The spring starts from the current value (0.0), so animation_value
        // is still ~0.0 immediately after show() (the first sample happens on
        // the next on_tick/advance).
        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(10.0, 20.0, 100.0, 50.0);
        controller.show(bounds, bubble_widget, test_content_builder("Copy"));
        assert_eq!(controller.phase(), Phase::Opening);

        // close() starts a reverse spring — phase is Closing, not Closed.
        controller.close();
        assert_eq!(controller.phase(), Phase::Closing);
    }

    #[test]
    fn test_controller_clone_shares_state() {
        let controller = ContextMenuController::new();
        let cloned = controller.clone();

        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(50.0, 60.0, 150.0, 100.0);
        cloned.show(bounds, bubble_widget, test_content_builder("A"));

        // The original sees the same state (shared via Rc<RefCell>). show()
        // starts a spring, so phase is Opening (not Open) immediately after.
        assert_eq!(controller.phase(), Phase::Opening);
        assert!(controller.open_snapshot().is_some());
    }

    #[test]
    fn test_host_closed_has_only_content() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // When closed, the render tree should NOT contain the menu content.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            !find_text_in_tree(ro_reg, root, "Copy"),
            "menu content 'Copy' should not be rendered when menu is closed"
        );
    }

    #[test]
    fn test_host_open_renders_menu_at_position() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Open the menu anchored to a bubble at (100, 200) with a builder that
        // renders "Copy" in the actions card.
        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(100.0, 200.0, 300.0, 250.0);
        controller.show(bounds, bubble_widget, test_content_builder("Copy"));
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // The menu content should now appear in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "menu content 'Copy' should be rendered when menu is open"
        );
    }

    #[test]
    fn test_item_tap_fires_on_select_and_closes() {
        let selected = Rc::new(std::cell::Cell::new(false));
        let selected_clone = selected.clone();

        // A builder that renders a single tappable row in `actions`. on_tap
        // flips the cell and closes the menu — mirrors a real menu item.
        let builder = MenuBuilder::new(move |ctrl, _theme| {
            let ctrl = ctrl.clone();
            let selected = selected_clone.clone();
            let row = vexo::GestureDetector::new(vexo::WithLayout::new(
                vexo::Text::new("Copy"),
                vexo::Layout::default().padding(8.0).width(160.0),
            ))
            .on_tap(move || {
                selected.set(true);
                ctrl.close();
            });
            MenuContent {
                reactions: vexo::Text::new("r").boxed(),
                actions: row.boxed(),
                metrics: MenuMetrics {
                    reactions_size: vexo::core::Size::new(150.0, 28.0),
                    actions_size: vexo::core::Size::new(200.0, 108.0),
                    gap: 8.0,
                },
            }
        });

        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let ticker = Arc::new(AnimationTicker::new());
        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // The actions card is Positioned below the bubble at
        // (bubble_bounds.left, bubble_bounds.top + bubble_bounds.height() + gap)
        // = (10, 10 + 40 + 8) = (10, 58). The item row has 8px padding, so
        // clicking at (15, 70) lands inside the row's padding area.
        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(10.0, 10.0, 200.0, 40.0);
        controller.show(bounds, bubble_widget, builder);
        pipeline.perform_rebuilds();
        // Settle the open spring (v→1.0, phase→Open) before tapping. Task 6
        // scales the card to 0.8+v*0.2 and fades it to opacity v; right after
        // show() (v≈0) the card is at 80% scale + 0 opacity and its hit region
        // is shifted by the scale-about-center transform, so (15, 70) no longer
        // lands on the row. At v=1 the scale is 1.0 (identity) and opacity 1.0,
        // so the hit-test works exactly as in Task 4. This mirrors real usage:
        // the user taps an item after the menu has opened.
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        assert_eq!(controller.phase(), Phase::Open);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let primary_press = InputEvent::PointerButton {
            position: vexo::core::Point::new(15.0, 70.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        let primary_release = InputEvent::PointerButton {
            position: vexo::core::Point::new(15.0, 70.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            vexo::core::Point::new(15.0, 70.0),
            &primary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        pipeline.handle_event(
            vexo::core::Point::new(15.0, 70.0),
            &primary_release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(selected.get(), "on_tap should have fired");
        // After the tap, the menu should close (controller.close() called by
        // the item's on_tap closure). Because the menu was fully open (v=1.0)
        // when tapped, close() starts a reverse spring from 1.0→0.0 that is
        // NOT instantly done (unlike the v≈0 case) — phase is `Closing` right
        // after. Advance real time past settle so the spring finishes and the
        // phase machine flips `Closing` → `Closed` (clearing `open`).
        pipeline.perform_rebuilds();
        assert_eq!(
            controller.phase(),
            Phase::Closing,
            "menu should be mid-close right after the item tap fires close()"
        );
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        assert_eq!(
            controller.phase(),
            Phase::Closed,
            "menu should be closed after the close spring settles"
        );
    }

    #[test]
    fn test_barrier_dismiss_on_outside_click() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(10.0, 10.0, 200.0, 40.0);
        controller.show(bounds, bubble_widget, test_content_builder("Copy"));
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Click far away from the menu — should hit the barrier and close.
        let primary_press = InputEvent::PointerButton {
            position: vexo::core::Point::new(350.0, 550.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            vexo::core::Point::new(350.0, 550.0),
            &primary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        pipeline.perform_rebuilds();
        assert_eq!(
            controller.phase(),
            Phase::Closed,
            "menu should be closed after clicking outside (barrier dismiss)"
        );
    }

    #[test]
    fn test_builder_reads_current_theme() {
        // A builder that encodes theme.surface.r into the rendered text label
        // (placed in the actions card). The builder runs inside
        // `ContextMenu::render`, so it must re-run with the *current* theme
        // whenever the `Theme` InheritedWidget changes — this is the whole
        // justification for running the builder at render time instead of
        // pre-building the menu widget.
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        // Two distinct themes so the assertion can tell them apart. We compute
        // the expected labels from the themes themselves (rather than hardcoding
        // float strings) so the test stays robust to color-preset tweaks.
        let light_theme = vexo::ThemeData::light();
        let dark_theme = vexo::ThemeData::dark();
        let light_label = format!("surface-r-{}", light_theme.surface.r);
        let dark_label = format!("surface-r-{}", dark_theme.surface.r);
        assert_ne!(
            light_label, dark_label,
            "light and dark surface.r must differ for this test to be meaningful"
        );

        // Wrap the host in Theme(light) so the builder reads the light theme
        // via Theme::of(ctx) during render.
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(vexo::Theme::new(light_theme.clone(), host.clone()).boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Open the menu. The builder runs in render() and must read the light
        // theme's surface.r.
        let builder = MenuBuilder::new(|_ctrl, theme| {
            let r = theme.surface.r;
            MenuContent {
                reactions: vexo::Text::new("r").boxed(),
                actions: vexo::Text::new(format!("surface-r-{}", r)).boxed(),
                metrics: MenuMetrics {
                    reactions_size: vexo::core::Size::new(150.0, 28.0),
                    actions_size: vexo::core::Size::new(200.0, 108.0),
                    gap: 8.0,
                },
            }
        });
        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(10.0, 10.0, 200.0, 40.0);
        controller.show(bounds, bubble_widget, builder);
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, &light_label),
            "builder should have rendered the light theme's surface.r ({:?})",
            light_label
        );

        // Toggle: re-wrap the host in Theme(dark). The InheritedWidget change
        // invalidates the ContextMenu element (a Theme::of dependent), forcing
        // render() — and thus the builder — to re-run with the dark theme.
        // The controller state (open + builder) is shared via Rc<RefCell>, so
        // the menu stays open across the toggle.
        pipeline.update(vexo::Theme::new(dark_theme.clone(), host.clone()).boxed());
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, &dark_label),
            "builder should have re-run with the dark theme's surface.r ({:?}) after the toggle",
            dark_label
        );
        assert!(
            !find_text_in_tree(ro_reg, root, &light_label),
            "light theme's label must be gone after toggling to dark — the builder re-ran"
        );
    }

    /// Walk the render tree and collect the `computed_bounds` sizes of every
    /// `TextRenderObject` whose content contains `needle`. Used by the
    /// dual-render spike test (#7) to assert the in-content and bright-copy
    /// Text render objects have identical laid-out sizes.
    fn collect_text_sizes(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        needle: &str,
        out: &mut Vec<vexo::core::Size<Logical>>,
    ) {
        if let Some(ro) = reg.get(key) {
            if ro
                .as_any()
                .downcast_ref::<TextRenderObject>()
                .map_or(false, |t| t.content().contains(needle))
            {
                if let Some(b) = ro.computed_bounds() {
                    out.push(vexo::core::Size::new(b.width(), b.height()));
                }
            }
            for &child in ro.children() {
                collect_text_sizes(reg, child, needle, out);
            }
        }
    }

    #[test]
    fn test_bright_bubble_copy_rendered_on_top() {
        let controller = ContextMenuController::new();
        let bubble_text = "BUBBLE_CONTENT marker";
        let bubble_widget = vexo::Text::new(bubble_text).boxed();
        let bounds = vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0);

        let host = ContextMenu::new(vexo::Text::new("background content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open the menu with the bubble widget. The host should render a
        // bright copy of `bubble_widget` on top of the dim barrier.
        controller.show(bounds, bubble_widget, test_content_builder("Actions"));
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // The bubble widget's text should appear in the render tree (as the
        // bright copy on top of the dim). It does NOT appear in the host's
        // background content ("background content") nor in the actions card
        // ("Actions"), so a presence check is sufficient.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, bubble_text),
            "bright bubble copy should be rendered when menu is open"
        );
    }

    #[test]
    fn test_bubble_copy_size_matches_original() {
        let controller = ContextMenuController::new();
        // A bubble widget with a known intrinsic size (pinned via WithLayout).
        let bubble_widget = vexo::WithLayout::new(
            vexo::Text::new("X"),
            vexo::Layout::default().width(80.0).height(30.0),
        )
        .boxed();
        // Bounds use (left, top, width, height) — `Bounds::from_xywh` produces
        // valid (left, top, right, bottom) from x/y/w/h. The brief's literal
        // `Bounds::new(50.0, 50.0, 80.0, 30.0)` would be (left=50, top=50,
        // right=80, bottom=30) → negative width/height; using from_xywh keeps
        // the intent (bubble at (50,50) sized 80x30) without malformed edges.
        let bounds = vexo::core::Bounds::from_xywh(50.0, 50.0, 80.0, 30.0);

        // Wrap the bubble widget in the content tree too, so it renders twice
        // (once in-content, once as the bright copy on top of the dim).
        let content = vexo::WithLayout::new(
            bubble_widget.clone_boxed(),
            vexo::Layout::default().width(80.0).height(30.0),
        );

        let host = ContextMenu::new(content, controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            bounds,
            bubble_widget.clone_boxed(),
            test_content_builder("A"),
        );
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Find all TextRenderObjects with content "X" in the tree. There
        // should be two (one in-content, one as the bright copy). Assert
        // their computed_bounds sizes match — this is the dual-render spike
        // gate: if sizes diverge, the dual-render assumption is wrong and the
        // spec's cutout-frame fallback must be used.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let mut found_sizes: Vec<vexo::core::Size<Logical>> = Vec::new();
        collect_text_sizes(ro_reg, root, "X", &mut found_sizes);
        assert_eq!(
            found_sizes.len(),
            2,
            "should find 2 'X' TextRenderObjects (in-content + bright copy)"
        );
        assert_eq!(
            found_sizes[0], found_sizes[1],
            "in-content and bright copy sizes must match (dual-render is deterministic)"
        );
    }

    /// Walk the render tree and collect the `RenderObjectKey`s of every
    /// `DecoratedBoxRenderObject` whose `Style.background` is `Some(BLACK)`.
    /// Used by the dim-barrier height regression test to locate the dim's
    /// render object (the only BLACK-background DecoratedBox in the menu's
    /// render tree when the menu is open).
    fn collect_black_decorated_boxes(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        out: &mut Vec<RenderObjectKey>,
    ) {
        if let Some(ro) = reg.get(key) {
            let is_black_bg = ro
                .as_any()
                .downcast_ref::<vexo::render_objects::DecoratedBoxRenderObject>()
                .map_or(false, |d| {
                    d.style()
                        .background
                        .map_or(false, |c| c == vexo::Color::BLACK)
                });
            if is_black_bg {
                out.push(key);
            }
            for &child in ro.children() {
                collect_black_decorated_boxes(reg, child, out);
            }
        }
    }

    /// Regression test for the dim-barrier sizing bug.
    ///
    /// Before the fix, the dim barrier's `DecoratedBox(BLACK)` was nested
    /// INSIDE `WithLayout` — so its child was `Text("")`. `DecoratedBox` is a
    /// pass-through render object, so it inherits its CHILD's bounds, not the
    /// `WithLayout`'s full-screen bounds. The `WithLayout` Column's
    /// `align_items: Stretch` only affects the cross axis (width), not the
    /// main axis (height), so `Text("")` stretched to full width (400px) but
    /// kept its intrinsic line height (~29px) on the main axis. The
    /// pass-through `DecoratedBox` inherited those 400x29 bounds → the dim
    /// painted only a 29px strip at the top of the screen instead of covering
    /// the full window. Tap-to-close still worked because the
    /// `GestureDetector` reads the `WithLayout`'s full-screen bounds (its own
    /// layout node), not the `DecoratedBox`'s — masking the bug from existing
    /// tests.
    ///
    /// After the fix, `DecoratedBox` WRAPS `WithLayout`, inheriting the
    /// `WithLayout`'s full-screen bounds (from `width_percent(1.0)` +
    /// `height_percent(1.0)`). This test walks the render tree when the menu
    /// is open, finds the dim's `DecoratedBoxRenderObject` (the one with a
    /// BLACK background), and asserts its `computed_bounds` cover the full
    /// screen — both height > 0 (the literal regression guard) and height
    /// equal to the screen height (the actual bug catcher: the buggy version
    /// produced 29px, not 600px).
    #[test]
    fn test_dim_barrier_has_nonzero_height() {
        let screen = vexo::core::Size::new(400.0, 600.0);
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(screen, &mut engine, &mut font_system);

        // Open the menu — the dim barrier should be in the render tree.
        let bubble_widget = vexo::Text::new("bubble").boxed();
        let bounds = vexo::core::Bounds::new(10.0, 10.0, 200.0, 40.0);
        controller.show(bounds, bubble_widget, test_content_builder("Copy"));
        pipeline.perform_rebuilds();
        pipeline.layout(screen, &mut engine, &mut font_system);

        // Find the dim's DecoratedBoxRenderObject (BLACK background).
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let mut black_boxes: Vec<RenderObjectKey> = Vec::new();
        collect_black_decorated_boxes(ro_reg, root, &mut black_boxes);
        assert_eq!(
            black_boxes.len(),
            1,
            "exactly one BLACK-background DecoratedBox (the dim) should exist when menu is open"
        );

        let dim_ro = ro_reg
            .get(black_boxes[0])
            .expect("dim DecoratedBoxRenderObject should be registered");
        let dim_bounds = dim_ro
            .computed_bounds()
            .expect("dim DecoratedBox should have computed_bounds after layout");

        // Literal regression guard: height must be > 0.
        assert!(
            dim_bounds.height() > 0.0,
            "dim barrier height must be > 0 (got {}); zero height means the dim paints nothing",
            dim_bounds.height()
        );

        // Actual bug catcher: the dim must cover the FULL screen height. The
        // buggy nesting (DecoratedBox inside WithLayout) produced a 29px-tall
        // strip — non-zero, so the `> 0` check above passed, but the dim was
        // visually broken (only the top 29px dimmed). The fixed nesting
        // (DecoratedBox wraps WithLayout) yields full-screen coverage.
        assert_eq!(
            dim_bounds.height(),
            screen.height,
            "dim barrier height must equal screen height ({}); got {} — if this is a small \
             value (~font line height), DecoratedBox is inside WithLayout and inheriting \
             Text(\"\")'s intrinsic line height instead of WithLayout's full-screen bounds",
            screen.height,
            dim_bounds.height()
        );
        assert_eq!(
            dim_bounds.width(),
            screen.width,
            "dim barrier width must equal screen width ({})",
            screen.width
        );
    }

    // ========================================================================
    // Task 5: spring-driven phase machine lifecycle tests
    // ========================================================================
    //
    // These tests exercise the 4-state phase machine (Closed → Opening →
    // Open → Closing → Closed) driven by a critical spring
    // (`SpringDescription::ios(340.0, 1.0)`). They use the
    // `pump(ticker, pipeline)` pattern: `std::thread::sleep` to advance real
    // time past the spring's settle point (~0.6s for k=340 critical), then
    // `ticker.tick()` (fires the controller's dirty callback registered via
    // `animate_with`), `drain_dirty_to_build_owner()` (moves the dirty mark
    // into the BuildOwner), and `perform_rebuilds()` (advances the spring via
    // `on_tick` → `controller.advance(now)` and re-renders).

    #[test]
    fn test_show_starts_open_spring() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // show() starts the forward spring.
        controller.show(
            vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
            vexo::Text::new("bubble").boxed(),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();

        assert_eq!(controller.phase(), Phase::Opening);
        assert!(
            controller.animation_value() < 1.0,
            "spring should not be settled yet (value={})",
            controller.animation_value()
        );

        // Advance real time past settle (~0.6s for critical spring k=340).
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();

        assert_eq!(controller.phase(), Phase::Open);
        assert!(
            (controller.animation_value() - 1.0).abs() < 0.01,
            "spring should have settled to 1.0 (value={})",
            controller.animation_value()
        );
    }

    #[test]
    fn test_close_starts_reverse_spring_not_immediate_unmount() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open and settle.
        controller.show(
            vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
            vexo::Text::new("bubble").boxed(),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        assert_eq!(controller.phase(), Phase::Open);

        // Close — should start reverse spring, NOT immediately clear.
        controller.close();
        pipeline.perform_rebuilds();

        assert_eq!(controller.phase(), Phase::Closing);
        assert!(
            controller.animation_value() > 0.0,
            "spring should still be mid-reverse (value={})",
            controller.animation_value()
        );

        // Advance past settle.
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();

        assert_eq!(controller.phase(), Phase::Closed);
        assert!(
            (controller.animation_value() - 0.0).abs() < 0.01,
            "spring should have settled to 0.0 (value={})",
            controller.animation_value()
        );
    }

    #[test]
    fn test_early_close_during_open_reverses_smoothly() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
            vexo::Text::new("bubble").boxed(),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();

        // Advance partway (value should be between 0 and 1).
        std::thread::sleep(std::time::Duration::from_millis(150));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        let mid_value = controller.animation_value();
        assert!(
            mid_value > 0.0 && mid_value < 1.0,
            "mid-value should be 0<v<1, got {}",
            mid_value
        );

        // Close mid-open.
        controller.close();
        pipeline.perform_rebuilds();
        assert_eq!(controller.phase(), Phase::Closing);

        // The value right after close should NOT jump to 1.0 — it should be
        // near mid_value (the spring starts from the current value).
        let value_after_close = controller.animation_value();
        assert!(
            (value_after_close - mid_value).abs() < 0.15,
            "value after close ({}) should be near mid_value ({}) — no jump to 1.0",
            value_after_close,
            mid_value
        );

        // Settle to Closed.
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        assert_eq!(controller.phase(), Phase::Closed);
    }

    #[test]
    fn test_reshow_during_close_retargets_upward() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open and settle.
        controller.show(
            vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
            vexo::Text::new("bubble").boxed(),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();

        // Start close, advance partway down.
        controller.close();
        pipeline.perform_rebuilds();
        std::thread::sleep(std::time::Duration::from_millis(150));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        let mid_value = controller.animation_value();
        assert!(mid_value > 0.0 && mid_value < 1.0);

        // Re-show with new bounds — should retarget upward.
        controller.show(
            vexo::core::Bounds::new(20.0, 20.0, 100.0, 40.0),
            vexo::Text::new("bubble2").boxed(),
            test_content_builder("Reply"),
        );
        pipeline.perform_rebuilds();
        assert_eq!(controller.phase(), Phase::Opening);

        let value_after_reshow = controller.animation_value();
        assert!(
            (value_after_reshow - mid_value).abs() < 0.15,
            "value after reshow ({}) should be near mid_value ({}) — no jump",
            value_after_reshow,
            mid_value
        );

        // Settle to Open.
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        assert_eq!(controller.phase(), Phase::Open);
    }

    // ========================================================================
    // Task 6: spring-driven transforms + barrier dismiss mid-animation
    // ========================================================================

    /// Test #5 — barrier dismiss during animation.
    ///
    /// Opens the menu, then clicks the dim barrier *mid-open* (before the
    /// spring settles). The barrier's `on_press` fires `controller.close()`,
    /// which starts the reverse spring → phase becomes `Closing` (the spring
    /// is mid-reverse, not yet settled to `Closed`).
    ///
    /// To be genuinely "mid-animation", the spring value must be meaningfully
    /// between 0 and 1 when the barrier is clicked. Immediately after
    /// `show()` + one `perform_rebuilds()`, the spring has only advanced by
    /// microseconds and `animation_value() ≈ 0` — so a `close()` from ≈0 to 0
    /// would satisfy `is_done()` instantly (zero displacement), flipping phase
    /// straight to `Closed`. That would make the `Phase::Closing` assertion
    /// impossible. We therefore advance real time ~150ms (past the spring's
    /// initial ramp, value ≈ 0.5) before clicking, so the reverse spring has
    /// real distance to travel and `phase` is `Closing` when asserted.
    #[test]
    fn test_dim_barrier_dismiss_during_animation() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open — don't wait for settle (we want mid-animation).
        controller.show(
            vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
            vexo::Text::new("bubble").boxed(),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Advance real time ~150ms so the spring is genuinely mid-open
        // (animation_value ≈ 0.5). Without this, the spring value would be
        // ≈0 and a close() from ≈0→0 would settle instantly (phase=Closed),
        // defeating the "during animation" intent of the test.
        std::thread::sleep(std::time::Duration::from_millis(150));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        let mid_value = controller.animation_value();
        assert!(
            mid_value > 0.0 && mid_value < 1.0,
            "spring should be mid-open (0<v<1) when barrier is clicked, got {}",
            mid_value
        );

        // Click far away (on the dim barrier) mid-open.
        let primary_press = vexo::input::InputEvent::PointerButton {
            position: vexo::core::Point::new(350.0, 550.0),
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Pressed,
        };
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        pipeline.handle_event(
            vexo::core::Point::new(350.0, 550.0),
            &primary_press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();

        // close() should have fired — phase is Closing (not Closed yet, spring
        // is mid-reverse).
        assert_eq!(
            controller.phase(),
            Phase::Closing,
            "barrier click mid-open should start close (phase=Closing)"
        );
    }

    /// Walk the render tree and return true if the subtree rooted at `key`
    /// contains a `DecoratedBoxRenderObject` whose `Style.background` is
    /// `Some(BLACK)` (the dim barrier's black fill).
    fn subtree_has_black_decorated_box(reg: &RenderObjectRegistry, key: RenderObjectKey) -> bool {
        if let Some(ro) = reg.get(key) {
            let is_black = ro
                .as_any()
                .downcast_ref::<vexo::render_objects::DecoratedBoxRenderObject>()
                .map_or(false, |d| {
                    d.style()
                        .background
                        .map_or(false, |c| c == vexo::Color::BLACK)
                });
            if is_black {
                return true;
            }
            for &child in ro.children() {
                if subtree_has_black_decorated_box(reg, child) {
                    return true;
                }
            }
        }
        false
    }

    /// Walk the render tree and return the opacity of the
    /// `OpacityRenderObject` whose subtree contains a BLACK-background
    /// `DecoratedBox` (i.e. the dim barrier's `Opacity` wrapper). Returns
    /// `None` when no such opacity node exists.
    ///
    /// This uniquely identifies the dim's opacity: the dim is the only
    /// `Opacity` in the menu whose subtree contains a BLACK `DecoratedBox`.
    /// (The actions card's `Opacity` subtree contains the card's text, not a
    /// BLACK box.)
    fn find_dim_opacity(reg: &RenderObjectRegistry, key: RenderObjectKey) -> Option<f32> {
        let ro = reg.get(key)?;
        if ro.opacity().is_some() && subtree_has_black_decorated_box(reg, key) {
            return ro.opacity();
        }
        for &child in ro.children() {
            if let Some(op) = find_dim_opacity(reg, child) {
                return Some(op);
            }
        }
        None
    }

    /// Walk the render tree and return the opacity of the
    /// `OpacityRenderObject` whose subtree contains `needle` text but NO
    /// BLACK-background `DecoratedBox` (i.e. the actions card's `Opacity`
    /// wrapper). Returns `None` when no such opacity node exists.
    ///
    /// Before Task 6, the actions card has no `Opacity` wrapper (rendered at
    /// full opacity directly) → returns `None`. After Task 6, the card is
    /// wrapped in `Opacity(v)` → returns `Some(v)`.
    fn find_card_opacity(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        needle: &str,
    ) -> Option<f32> {
        let ro = reg.get(key)?;
        if ro.opacity().is_some()
            && find_text_in_tree(reg, key, needle)
            && !subtree_has_black_decorated_box(reg, key)
        {
            return ro.opacity();
        }
        for &child in ro.children() {
            if let Some(op) = find_card_opacity(reg, child, needle) {
                return Some(op);
            }
        }
        None
    }

    /// RED→GREEN test for the spring-driven dim opacity.
    ///
    /// Before Task 6: the dim barrier's `Opacity` is fixed at `0.4` regardless
    /// of the spring value. After Task 6: the dim opacity is `v * 0.4`, where
    /// `v = controller.animation_value()`.
    ///
    /// We open the menu, advance real time ~150ms so the spring is genuinely
    /// mid-open (`v ≈ 0.76`), then read the dim's `OpacityRenderObject` from
    /// the render tree and assert its opacity ≈ `v * 0.4` (NOT the fixed
    /// `0.4`). Before Task 6 this fails (dim=0.4, expected ≈0.30); after Task
    /// 6 it passes (dim=v*0.4).
    #[test]
    fn test_dim_opacity_tracks_spring_value() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
            vexo::Text::new("bubble").boxed(),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();

        // Advance ~150ms so the spring is mid-open (0 < v < 1).
        std::thread::sleep(std::time::Duration::from_millis(150));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let v = controller.animation_value();
        assert!(
            v > 0.05 && v < 0.99,
            "spring should be mid-open for this test to be meaningful, got v={}",
            v
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let dim_opacity = find_dim_opacity(ro_reg, root).expect(
            "dim OpacityRenderObject should exist when menu is open (find_dim_opacity found none)",
        );

        let expected = (v * 0.4) as f32;
        assert!(
            (dim_opacity - expected).abs() < 0.01,
            "dim opacity should track v*0.4 (v={:.4}, expected≈{:.4}, got {:.4}); \
             if got 0.4 the dim is still fixed-alpha (pre-Task-6 behavior)",
            v,
            expected,
            dim_opacity
        );
    }

    /// RED→GREEN test for the spring-driven actions-card opacity.
    ///
    /// Before Task 6: the actions card is rendered at full opacity with no
    /// `Opacity` wrapper. After Task 6: the card is wrapped in `Opacity(v)`.
    ///
    /// We open the menu, advance ~150ms (v ≈ 0.76), then assert the card has
    /// an `OpacityRenderObject` with opacity ≈ `v`. Before Task 6: no such
    /// opacity node exists → `find_card_opacity` returns `None` → `expect`
    /// panics → RED. After Task 6: `Opacity(v)` is present → GREEN.
    ///
    /// (The card's scale-about-center transform can't be inspected here —
    /// `TransformRenderObject` is `pub(crate)` in `vexo` and not re-exported.
    /// The card opacity check plus the dim opacity check together prove the
    /// spring value `v` is read and applied to multiple overlay layers. The
    /// bubble's scale/lift transform is verified indirectly by the existing
    /// `test_bright_bubble_copy_rendered_on_top` /
    /// `test_bubble_copy_size_matches_original` tests, which confirm the
    /// bubble copy still renders at the correct size after wrapping it in the
    /// transform chain — `TransformRenderObject` is a layout pass-through, so
    /// the Text's computed bounds are unchanged.)
    #[test]
    fn test_card_opacity_tracks_spring_value() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            vexo::core::Bounds::new(10.0, 10.0, 100.0, 40.0),
            vexo::Text::new("bubble").boxed(),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();

        std::thread::sleep(std::time::Duration::from_millis(150));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let v = controller.animation_value();
        assert!(
            v > 0.05 && v < 0.99,
            "spring should be mid-open for this test to be meaningful, got v={}",
            v
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let card_opacity = find_card_opacity(ro_reg, root, "Copy").expect(
            "card OpacityRenderObject should exist when menu is open after Task 6 \
             (find_card_opacity found none — card is not wrapped in Opacity yet)",
        );
        let expected = v as f32;
        assert!(
            (card_opacity - expected).abs() < 0.01,
            "card opacity should track v (v={:.4}, expected≈{:.4}, got {:.4})",
            v,
            expected,
            card_opacity
        );
    }
}
