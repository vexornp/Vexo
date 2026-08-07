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
//! Open is driven by a critical spring (`SpringDescription::ios(340.0, 1.0)`)
//! through a 3-state phase machine (`Closed → Opening → Open`). `show()`
//! starts a forward spring (0.0 → 1.0, phase=Opening); `close()` instantly
//! clears to `Closed` (clears `open` → unmount, no reverse spring). `show()`
//! always resets the spring to start from 0.0, so every open animates 0→1
//! consistently — a re-show after `close()` animates the same as a first open.
//! The host's `on_tick` calls `controller.advance(now)`, which samples the
//! spring and flips Opening→Open on settle.
//!
//! `render()` builds a 3-layer `Stack`: (1) the child content, (2) a
//! transparent full-screen dismiss barrier (`GestureDetector::on_press` →
//! `close()`), (3) the menu cluster anchored at the click point — the actions
//! card's top-left sits at the click point and the reactions pill sits
//! directly above it (separated by `gap`). The cluster scales `0.85 + v*0.15`
//! about the click point on open (each card wrapped via `scale_about_anchor`,
//! which compensates for the painter's center-re-anchoring so the effective
//! transform truly pivots at the click point) — the menu appears to grow out
//! of the click position, with the card's top-left fixed. No dim, no bubble
//! copy.
//!
//! Edge-aware positioning: the cluster's top-left defaults to
//! (click_pos.x, click_pos.y - pill_h - gap) so the card's top-left lands on
//! the click point. If the cluster would overflow the top or bottom of the
//! window, the whole cluster (pill + card together) slides the minimum amount
//! to stay on-screen — the pill-above-card stacking is never reordered.
//! Horizontally the cluster left-clamps to `[8, window_w - cluster_w - 8]` on
//! right overflow. Window size is read via `MediaQuery::of(ctx)` (an
//! `InheritedWidget` dependency, so the host rebuilds on resize).

use std::any::Any;
use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vexo::animation::{AnimationController, AnimationTicker, SpringDescription, SpringSimulation};
use vexo::core::{Logical, Point, Size};
use vexo::{Component, ComponentState, LifecycleContext, RenderContext, Widget};

// ============================================================================
// MenuContent + MenuMetrics + MenuBuilder
// ============================================================================

/// The two cards produced by a menu builder.
///
/// `reactions` is the top pill (emoji/reaction strip); `actions` is the lower
/// card (Copy / Reply / Delete rows). The host positions both relative to the
/// click point using `metrics` for spacing (pill on top, card below, with
/// `gap` between them).
pub struct MenuContent {
    pub reactions: Box<dyn Widget>,
    pub actions: Box<dyn Widget>,
    pub metrics: MenuMetrics,
}

/// Size hints for positioning + transform anchors. These are estimates used
/// by the host to position cards and compute scale-about-point transforms
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

/// Lifecycle phase of the context menu. The 3-state phase machine is driven
/// by a critical spring: `show()` → `Opening`, settle → `Open`; `close()` →
/// `Closed` (instant — clears `open`, unmounts the menu).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Closed,
    Opening,
    Open,
}

/// Snapshot of what `show()` was called with. Held in the controller's shared
/// cell while the menu is open; cleared on `close()`.
///
/// Read by `render()` via `open_snapshot()` to get the click point + builder
/// for the overlay cluster.
struct OpenState {
    click_pos: Point<Logical>,
    builder: MenuBuilder,
}

/// Shared (across controller clones) mutable state.
struct Shared {
    phase: Phase,
    open: Option<OpenState>,
    /// The critical spring driving `Opening`. Same spring as
    /// KeyboardAvoidance/SlideTransition: `SpringDescription::ios(340.0, 1.0)`.
    /// `show()` starts a forward spring from 0.0 → 1.0 (Amendment 2: every
    /// show() animates 0→1 consistently — re-show after close() animates the
    /// same as a first open; close() already unmounted the overlay, so the
    /// value reset has no visible jump). `close()` stops the spring (halts the
    /// sim + unregisters from the ticker) but does NOT reset `value`, so the
    /// frozen value is the leftover from the last advance — harmless, since
    /// show() ignores it. The host's `on_tick` calls `advance(now)` to sample
    /// the spring and flip `Opening → Open` on settle.
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

    /// Open the menu anchored at `click_pos` (the right-click cursor
    /// position in window-logical coords). Starts a forward spring
    /// (0.0 → 1.0) and sets phase to `Opening`. The spring always starts
    /// from 0.0 — `show()` resets the value so every open animates
    /// consistently, whether first open or re-show after `close()`. Since
    /// `close()` already unmounted the overlay, the reset has no visible
    /// jump. `on_tick` flips `Opening` → `Open` when the spring settles.
    pub fn show(&self, click_pos: Point<Logical>, builder: MenuBuilder) {
        let mut s = self.shared.borrow_mut();
        s.open = Some(OpenState { click_pos, builder });
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
        // Forward spring from 0.0 (Amendment 2: show() always resets to 0.0 so
        // every open animates 0→1 consistently — a re-show after close()
        // animates the same as a first open). v0=0 because the user gesture
        // that triggered show() has no carried velocity (unlike a scroll
        // fling). `close()` already unmounted the overlay, so the value reset
        // produces no visible jump.
        s.animation.animate_with(Box::new(SpringSimulation::new(
            SpringDescription::ios(340.0, 1.0),
            0.0,
            1.0,
            0.0,
        )));
        // `animate_with` fires the dirty callback immediately (avoids the
        // render_retain deadlock: without it, the callback only fires on the
        // next tick(), which only runs inside render_retain(), which only
        // runs when a frame is already requested). The callback is an mpsc
        // send — no RefCell reentry — so holding borrow_mut here is safe.
    }

    /// Close the menu instantly. Sets phase to `Closed` and clears `open`
    /// (unmount on next rebuild). No reverse spring, no animation. No-op if
    /// already `Closed`.
    pub fn close(&self) {
        let mut s = self.shared.borrow_mut();
        if s.phase == Phase::Closed {
            return;
        }
        s.phase = Phase::Closed;
        s.open = None;
        // A forward spring may still be running if close() was called mid-
        // Opening. `stop()` halts the spring AND unregisters from the ticker
        // (so the dirty callback stops firing on subsequent ticks). But
        // `stop()` does NOT itself fire the dirty callback — so we must fire
        // it explicitly here to trigger the host rebuild that unmounts the
        // overlay. Without this, close() sets phase=Closed but the host never
        // re-renders, so the menu stays visible forever.
        s.animation.stop();
        let cb = s.dirty_callback.clone();
        drop(s);
        if let Some(cb) = cb {
            cb();
        }
    }

    pub fn phase(&self) -> Phase {
        self.shared.borrow().phase
    }

    /// The live spring value in `[0.0, 1.0]`. Drives the open scale
    /// (`0.85 + v * 0.15`) applied to the menu cluster. Read at render time.
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

        if !s.animation.is_animating() {
            if s.phase == Phase::Opening {
                s.phase = Phase::Open;
            }
        }
    }

    /// Snapshot the current open state (clones the click point + builder).
    /// Returns `None` when closed. Called by the host during `render()` only
    /// when `phase() != Closed`.
    pub(crate) fn open_snapshot(&self) -> Option<(Point<Logical>, MenuBuilder)> {
        let s = self.shared.borrow();
        s.open.as_ref().map(|o| (o.click_pos, o.builder.clone()))
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
/// When `controller.show(click_pos, builder)` is called (e.g. by a
/// `context_menu_trigger` on right-click), the host rebuilds and shows a
/// floating menu anchored at the click position — the menu's content is
/// whatever the caller's `builder` returns — with a dismiss barrier behind it.
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
        // Advance the spring and flip Opening→Open on settle. The host element
        // is marked dirty by the controller's dirty callback (fired by
        // `animate_with` on start and by `advance` on each sample), so
        // `perform_rebuilds` → `element.animate(now)` → `state.on_tick(now)`
        // reaches here every frame while the spring is active.
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
            if let Some((click_pos, builder)) = self.controller.open_snapshot() {
                let controller = self.controller.clone();
                let content = builder(&controller, &theme);
                let metrics = content.metrics;

                let mq = vexo::MediaQuery::of(ctx);
                let window_w = mq.size.width;
                let window_h = mq.size.height;

                // === Cluster geometry ===
                // Layout: the actions card's top-left sits at the click point,
                // and the reactions pill sits directly above it (pill bottom
                // edge = card top edge, separated by `gap`). The whole cluster
                // therefore straddles the click point vertically: pill above,
                // card at+below. The cluster scales 0.85→1.0 about the click
                // point (the card's top-left), so the menu appears to grow out
                // of the click position.
                let gap = metrics.gap;
                let pill_w = metrics.reactions_size.width;
                let pill_h = metrics.reactions_size.height;
                let card_w = metrics.actions_size.width;
                let card_h = metrics.actions_size.height;
                let cluster_w = pill_w.max(card_w);
                let cluster_h = pill_h + gap + card_h;

                // === Horizontal: left-clamp ===
                // Keep the cluster's right edge inside the window (8px margin).
                // In the default (non-edge) case cluster_x = click_pos.x.
                let cluster_x = if window_w > 0.0 {
                    let lo = 8.0;
                    let hi = (window_w - cluster_w - 8.0).max(lo);
                    click_pos.x.max(lo).min(hi)
                } else {
                    click_pos.x
                };

                // === Vertical: shift-to-fit ===
                // Default placement: card top-left = click point, pill above it
                //   → cluster_top (pill top) = click_pos.y - pill_h - gap
                //   → cluster_bottom = click_pos.y + card_h
                // If the cluster overflows the top (< 8) or bottom (> window_h
                // - 8) edge, slide the whole cluster (pill + card together) the
                // minimum amount to bring it back on-screen. This keeps the
                // pill-above-card stacking intact at all times — no reordering.
                let default_cluster_top = click_pos.y - pill_h - gap;
                let cluster_y = if window_h > 0.0 {
                    let top_min = 8.0;
                    let bottom_max = (window_h - 8.0).max(top_min + cluster_h);
                    if default_cluster_top < top_min {
                        // Pill would clip off the top → slide down.
                        top_min
                    } else if default_cluster_top + cluster_h > bottom_max {
                        // Card would clip off the bottom → slide up.
                        (bottom_max - cluster_h).max(top_min)
                    } else {
                        default_cluster_top
                    }
                } else {
                    default_cluster_top
                };

                let pill_x = cluster_x;
                let pill_y = cluster_y;
                let card_x = cluster_x;
                let card_y = cluster_y + pill_h + gap;

                // === Layer [2]: transparent dismiss barrier ===
                let ctrl_for_barrier = controller.clone();
                let barrier = vexo::Positioned::new(
                    vexo::GestureDetector::new(vexo::WithLayout::new(
                        vexo::Text::new(""),
                        vexo::Layout::default()
                            .width_percent(1.0)
                            .height_percent(1.0),
                    ))
                    .on_press(move || {
                        ctrl_for_barrier.close();
                    }),
                )
                .left(0.0)
                .top(0.0)
                .right(0.0)
                .bottom(0.0);
                stack = stack.push(barrier);

                // === Layer [3]: menu cluster (pill + card), scaled about click point ===
                // The menu grows from the click point: scale 0.85→1.0 anchored at
                // click_pos (the card's top-left in the default placement). Both
                // cards share the same anchor so the whole cluster expands
                // together from the click position. The anchor stays at click_pos
                // even when the cluster is edge-shifted — the grow-from-click
                // effect is preserved regardless of final placement.
                //
                // Each card needs its OWN transform matrix because the painter
                // re-anchors every paint_transform to that object's bounds center
                // (see `scale_about_anchor` for the compensation math). The pill
                // and card have different centers, so each gets a matrix tuned to
                // make the EFFECTIVE transform scale about click_pos.
                let scale = (0.85 + v * 0.15) as f32;
                let pill_center =
                    vexo::core::Point::new(pill_x + pill_w * 0.5, pill_y + pill_h * 0.5);
                let card_center =
                    vexo::core::Point::new(card_x + card_w * 0.5, card_y + card_h * 0.5);
                let positioned_pill = vexo::Positioned::new(scale_about_anchor(
                    content.reactions,
                    scale,
                    click_pos,
                    pill_center,
                ))
                .left(pill_x)
                .top(pill_y);
                stack = stack.push(positioned_pill);

                let positioned_card = vexo::Positioned::new(scale_about_anchor(
                    content.actions,
                    scale,
                    click_pos,
                    card_center,
                ))
                .left(card_x)
                .top(card_y);
                stack = stack.push(positioned_card);
            }
        }

        stack.boxed()
    }
}

/// Wrap `child` in a `Transform` whose EFFECTIVE paint transform scales the
/// child about `anchor` (e.g. the click point), keeping `anchor` fixed.
///
/// ## Why the matrix isn't a plain scale-about-anchor
///
/// The painter (`vexo/src/painter.rs`) wraps every render object's
/// `paint_transform()` matrix `M` as `T(center) ∘ M ∘ T(-center)` before
/// applying it to child paint commands, where `center` is the render object's
/// bounds center (= `absolute_position + size/2`). This re-anchors ANY matrix
/// to the object's center — it's there so rotations/scales naturally pivot
/// about the object's own center.
///
/// A naive `T(anchor) ∘ S(s) ∘ T(-anchor)` matrix (the old `scale_about_point`)
/// gets double-anchored by this center-wrapping, producing an effective
/// transform that scales about `anchor + center` rather than `anchor`. The
/// visual symptom: the card's top-left (at `anchor`) drifts during the
/// animation instead of staying fixed — the menu appears to grow from the
/// card's far corner toward the click point.
///
/// ## The fix
///
/// Solve for the matrix `M` that makes the painter's wrapped form equal to a
/// scale about `anchor`:
///
/// ```text
/// want:  T(center) ∘ M ∘ T(-center) == T(anchor) ∘ S(s) ∘ T(-anchor)
/// =>     M = S(s) ∘ T((1-s)*(anchor - center))
/// ```
///
/// `M` is a scale by `s` plus a translation of `(1-s)*(anchor - center)`. The
/// translation vanishes as `s → 1`, so at rest `M` is identity and the card
/// paints at its natural laid-out position (top-left at the click point). At
/// `s < 1` the effective transform scales every point about `anchor`, so the
/// card's top-left (which sits at `anchor` in the default placement) stays
/// fixed while the rest of the card grows outward from the click point.
///
/// ## Hit-test caveat
///
/// The framework's hit-tester (`vexo/src/hit_test.rs`) applies `inv(M)`
/// directly to the pointer and is only consistent with the painter when `M` is
/// center-anchored. Our `M` is center-anchored only when `anchor == center`,
/// so taps at `s < 1` may land slightly off. This is acceptable: the menu is
/// only interacted with at `s = 1` (fully open), where `M` is identity and
/// hit-testing is exact. (The old `scale_about_point` matrix had the same
/// property — this fix changes only the visual, not the hit-test behavior.)
///
/// `TransformRenderObject` is a layout pass-through: the child's laid-out
/// bounds propagate up unchanged, and the transform is applied only at paint
/// + hit-test time.
fn scale_about_anchor(
    child: Box<dyn Widget>,
    s: f32,
    anchor: Point<Logical>,
    obj_center: Point<Logical>,
) -> Box<dyn Widget> {
    let tx = (1.0 - s) * (anchor.x - obj_center.x);
    let ty = (1.0 - s) * (anchor.y - obj_center.y);
    let transform =
        vexo::AffineTransform::translation(tx, ty).mul(&vexo::AffineTransform::scale(s, s));
    vexo::Transform::new(child, transform).boxed()
}

// ============================================================================
// context_menu_trigger — sugar for wrapping a child with right-click detection
// ============================================================================

/// Wrap `child` with a right-click handler that opens the context menu
/// anchored at the click cursor position, rendering content from `builder`.
///
/// Equivalent to:
/// ```ignore
/// child.on_secondary_press(move |pos, _bounds| {
///     controller.show(pos, builder);
/// })
/// ```
pub fn context_menu_trigger(
    child: impl Widget + 'static,
    controller: ContextMenuController,
    builder: MenuBuilder,
) -> Box<dyn Widget> {
    let ctrl = controller.clone();
    child.on_secondary_press(move |pos, _bounds| {
        ctrl.show(pos, builder.clone());
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use vexo::core::Bounds;
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
        // The spring starts from 0.0 (Amendment 2: show() always resets to
        // 0.0), so animation_value is still ~0.0 immediately after show()
        // (the first sample happens on the next on_tick/advance).
        let pos = vexo::core::Point::new(10.0, 20.0);
        controller.show(pos, test_content_builder("Copy"));
        assert_eq!(controller.phase(), Phase::Opening);

        // close() instantly clears to Closed — no Closing phase.
        controller.close();
        assert_eq!(controller.phase(), Phase::Closed);
    }

    #[test]
    fn test_controller_clone_shares_state() {
        let controller = ContextMenuController::new();
        let cloned = controller.clone();

        let pos = vexo::core::Point::new(50.0, 60.0);
        cloned.show(pos, test_content_builder("A"));

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

        // Open the menu anchored at click point (100, 200) with a builder that
        // renders "Copy" in the actions card.
        let pos = vexo::core::Point::new(100.0, 200.0);
        controller.show(pos, test_content_builder("Copy"));
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

        // This test has no MediaQuery wrapper, so the host reads window_h=0
        // and skips edge shift-to-fit: the card's top-left lands exactly at
        // the click point (10, 10). (With a real window, the pill above would
        // clip the top and the cluster would shift down — but not here.)
        let pos = vexo::core::Point::new(10.0, 10.0);
        controller.show(pos, builder);
        pipeline.perform_rebuilds();
        // Settle the open spring (v→1.0, phase→Open) before tapping. The host
        // scales the card `0.85 + v*0.15` about the click point; right after
        // show() (v≈0) the card is at 85% scale and its hit region is shifted
        // by the scale-about-point transform, so (50, 30) no longer lands on
        // the row. At v=1 the scale is 1.0 (identity), so the hit-test works
        // exactly as at rest. This mirrors real usage: the user taps an item
        // after the menu has opened.
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

        // This test has no MediaQuery wrapper, so the host reads window_h=0
        // and skips edge shift-to-fit: the card's top-left lands exactly at
        // the click point (10, 10), spanning (10,10)-(170,55). The item row
        // has 8px padding, so clicking at (50, 30) lands inside the row.
        let primary_press = InputEvent::PointerButton {
            position: vexo::core::Point::new(50.0, 30.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        let primary_release = InputEvent::PointerButton {
            position: vexo::core::Point::new(50.0, 30.0),
            button: PointerButton::Primary,
            state: ButtonState::Released,
        };
        pipeline.handle_event(
            vexo::core::Point::new(50.0, 30.0),
            &primary_press,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );
        pipeline.handle_event(
            vexo::core::Point::new(50.0, 30.0),
            &primary_release,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        // DEBUG: print the card's Positioned bounds + animation value.
        {
            let v = controller.animation_value();
            let ro_reg = pipeline.render_objects();
            let root = ro_reg.root().expect("root");
            if let Some(b) = find_positioned_bounds_around_text(ro_reg, root, "Copy") {
                eprintln!(
                    "DEBUG: animation_value={:.6}, scale={:.6}, card Positioned bounds={:?} \
                     (left={}, top={}, w={}, h={})",
                    v,
                    0.85 + v * 0.15,
                    b,
                    b.left,
                    b.top,
                    b.width(),
                    b.height()
                );
            } else {
                eprintln!("DEBUG: could not find card Positioned bounds");
            }
        }

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
        pipeline.perform_rebuilds();
        assert_eq!(
            controller.phase(),
            Phase::Closed,
            "menu should be closed immediately after item tap (instant close)"
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

        let pos = vexo::core::Point::new(10.0, 10.0);
        controller.show(pos, test_content_builder("Copy"));
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
        let pos = vexo::core::Point::new(10.0, 10.0);
        controller.show(pos, builder);
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

    /// Walk the render tree and return the `computed_bounds` of the
    /// `PositionedRenderObject` whose subtree contains a `TextRenderObject`
    /// with content matching `needle`. Returns `None` if no match is found OR
    /// the match has no laid-out bounds yet.
    ///
    /// Used by the edge-flip tests to assert a card was positioned on-screen
    /// (not clipped off the top): the `PositionedRenderObject`'s
    /// `computed_bounds` reflects the absolute laid-out position (left/top in
    /// window coords), unlike the inner `TextRenderObject`'s
    /// `computed_bounds` which is local to its layout origin (always 0,0).
    /// Identifying the right `Positioned` is unambiguous: the pill's
    /// `Positioned` subtree contains "r", the card's contains "Copy". (The
    /// transparent dismiss barrier carries an empty `Text` — no needle — so
    /// it is never matched by this helper.)
    fn find_positioned_bounds_around_text(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        needle: &str,
    ) -> Option<Bounds<Logical>> {
        if let Some(ro) = reg.get(key) {
            let is_positioned = ro
                .as_any()
                .downcast_ref::<vexo::render_objects::PositionedRenderObject>()
                .is_some();
            if is_positioned && find_text_in_tree(reg, key, needle) {
                if let Some(b) = ro.computed_bounds() {
                    return Some(b);
                }
            }
            for &child in ro.children() {
                if let Some(b) = find_positioned_bounds_around_text(reg, child, needle) {
                    return Some(b);
                }
            }
        }
        None
    }

    // ========================================================================
    // Task 5: spring-driven phase machine lifecycle tests
    // ========================================================================
    //
    // These tests exercise the 3-state phase machine (Closed → Opening →
    // Open) driven by a critical spring
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
            vexo::core::Point::new(10.0, 10.0),
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
    fn test_close_is_instant_no_reverse_spring() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            vexo::core::Point::new(10.0, 10.0),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        assert_eq!(controller.phase(), Phase::Open);

        controller.close();
        pipeline.perform_rebuilds();

        assert_eq!(
            controller.phase(),
            Phase::Closed,
            "close() should instantly clear to Closed — no Closing phase"
        );
        assert!(
            controller.open_snapshot().is_none(),
            "open state should be cleared immediately after close()"
        );
    }

    #[test]
    fn test_early_close_during_open_is_instant() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            vexo::core::Point::new(10.0, 10.0),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();

        std::thread::sleep(std::time::Duration::from_millis(150));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();

        controller.close();
        pipeline.perform_rebuilds();

        assert_eq!(
            controller.phase(),
            Phase::Closed,
            "early close() should be instant — no Closing phase"
        );
        assert!(controller.open_snapshot().is_none());
        assert!(
            !ticker.has_active(),
            "spring should be unregistered from ticker after close() (stop() called)"
        );
    }

    #[test]
    fn test_reshow_after_close_restarts_from_zero() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open and advance partway so the spring is genuinely mid-open
        // (0 < v < 1). We close mid-open (not after settle) so the frozen
        // value (mid_value) is meaningfully > 0 — this lets the reshow below
        // prove show() resets the spring to 0.0 (Amendment 2) rather than
        // retargeting from the frozen mid_value. If we closed after settle
        // (value=1.0), a reshow without reset would spring 1.0→1.0 (instant),
        // masking the difference; closing mid-open makes the reset observable.
        controller.show(
            vexo::core::Point::new(10.0, 10.0),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();
        std::thread::sleep(std::time::Duration::from_millis(150));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        let mid_value = controller.animation_value();
        assert!(
            mid_value > 0.0 && mid_value < 1.0,
            "spring should be mid-open (0<v<1), got {}",
            mid_value
        );

        // Instant close mid-open: phase→Closed, open cleared. `close()` stops
        // the spring (unregisters from ticker) but does NOT reset `value`, so
        // the value is frozen at mid_value until the next show().
        controller.close();
        pipeline.perform_rebuilds();
        assert_eq!(controller.phase(), Phase::Closed);

        // Re-show immediately (no tick between close and show): show() resets
        // the spring to start from 0.0 (Amendment 2), so the forward spring
        // runs 0.0 → 1.0 fresh — every show() animates consistently, whether
        // first open or re-show after close. Since close() already unmounted
        // the overlay, the value reset has no visible jump.
        controller.show(
            vexo::core::Point::new(20.0, 20.0),
            test_content_builder("Reply"),
        );
        pipeline.perform_rebuilds();
        assert_eq!(controller.phase(), Phase::Opening);

        let value_after_reshow = controller.animation_value();
        assert!(
            value_after_reshow < 0.1,
            "value after reshow ({}) should be near 0.0 — show() resets the \
             spring to start from 0.0 (Amendment 2), not retarget from the \
             frozen mid_value ({})",
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
    /// Opens the menu, then clicks the transparent barrier *mid-open* (before the
    /// spring settles). The barrier's `on_press` fires `controller.close()`,
    /// which instantly clears phase to `Closed` and unmounts the menu (no
    /// `Closing` phase, no reverse spring).
    ///
    /// To be genuinely "mid-animation", the spring value must be meaningfully
    /// between 0 and 1 when the barrier is clicked. Immediately after
    /// `show()` + one `perform_rebuilds()`, the spring has only advanced by
    /// microseconds and `animation_value() ≈ 0`. We advance real time ~150ms
    /// (past the spring's initial ramp, value ≈ 0.5) before clicking, so the
    /// `mid_value` assertion (0 < v < 1) is meaningful — it confirms the
    /// barrier was actually hit mid-open, not after settle.
    #[test]
    fn test_barrier_dismiss_during_animation() {
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
            vexo::core::Point::new(10.0, 10.0),
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

        // Click far away (on the transparent barrier) mid-open.
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

        // close() should have fired — phase is Closed (instant close, no
        // Closing phase).
        assert_eq!(
            controller.phase(),
            Phase::Closed,
            "barrier click mid-open should instantly close (phase=Closed)"
        );
    }

    /// Walk the render tree and return the opacity of the
    /// `OpacityRenderObject` whose subtree contains `needle` text (i.e. the
    /// actions card's `Opacity` wrapper). Returns `None` when no such opacity
    /// node exists.
    ///
    /// The card is never wrapped in `Opacity` (always opaque for depth-write
    /// occlusion), so this returns `None` — verified by
    /// `test_card_has_no_opacity_fade`.
    fn find_card_opacity(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        needle: &str,
    ) -> Option<f32> {
        let ro = reg.get(key)?;
        if ro.opacity().is_some() && find_text_in_tree(reg, key, needle) {
            return ro.opacity();
        }
        for &child in ro.children() {
            if let Some(op) = find_card_opacity(reg, child, needle) {
                return Some(op);
            }
        }
        None
    }

    /// Verifies the actions card is never wrapped in `Opacity`.
    ///
    /// The card is always opaque so it always writes depth (Phase 1) and
    /// occludes background text behind it. Only the scale animates
    /// (0.92→1.0 about the click point). This test opens the menu, advances
    /// ~150ms (mid-open, 0 < v < 1), then asserts no `OpacityRenderObject`
    /// exists on the card's subtree (`find_card_opacity` returns `None`).
    ///
    /// (The card's scale-about-point transform can't be inspected here —
    /// `TransformRenderObject` is `pub(crate)` in `vexo` and not re-exported.
    /// `TransformRenderObject` is a layout pass-through, so the card's
    /// `computed_bounds` are unchanged by the transform.)
    #[test]
    fn test_card_has_no_opacity_fade() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            vexo::core::Point::new(10.0, 10.0),
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
        let card_opacity = find_card_opacity(ro_reg, root, "Copy");
        assert!(
            card_opacity.is_none(),
            "card should NOT be wrapped in Opacity (always opaque for depth-write occlusion), \
             but found opacity={:?} at v={:.4}",
            card_opacity,
            v
        );
    }

    /// Walk the render tree and return the `paint_transform` matrix of the
    /// render object whose subtree contains `needle` text. Returns `None` when
    /// no such transform-bearing node exists. Used by
    /// `test_scale_anchor_keeps_card_top_left_fixed` to inspect the card's
    /// `Transform` matrix at mid-animation.
    ///
    /// `paint_transform()` is a `RenderObject` trait method (like `opacity()`),
    /// so it's callable on `&dyn RenderObject` without downcasting — even
    /// though `TransformRenderObject` itself is `pub(crate)` in `vexo`.
    fn find_paint_transform_around_text(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        needle: &str,
    ) -> Option<vexo::AffineTransform> {
        let ro = reg.get(key)?;
        if let Some(t) = ro.paint_transform() {
            if find_text_in_tree(reg, key, needle) {
                return Some(t);
            }
        }
        for &child in ro.children() {
            if let Some(t) = find_paint_transform_around_text(reg, child, needle) {
                return Some(t);
            }
        }
        None
    }

    /// Verifies the open scale truly anchors at the click point: at
    /// mid-animation (0 < v < 1, scale < 1) the card's `paint_transform`
    /// matrix equals `S(s) + T((1-s)*(click - card_center))`, and the
    /// EFFECTIVE transform (after the painter's center re-anchoring) maps the
    /// card's top-left corner — which sits at the click point — back to the
    /// click point. I.e. the top-left stays fixed during the grow animation.
    ///
    /// This is the regression guard for the `scale_about_anchor` fix. The old
    /// `scale_about_point` matrix (`T(click) ∘ S(s) ∘ T(-click)`) got double-
    /// anchored by the painter's center-wrapping, producing an effective
    /// transform that scaled about `click + center`; the card's top-left
    /// drifted during the animation (visually: the menu grew from the card's
    /// far corner toward the click point). This test would fail against the
    /// old matrix because the effective transform would NOT map click_pos to
    /// itself.
    ///
    /// No `MediaQuery` wrapper → the host reads `window_h = 0` and skips edge
    /// shift-to-fit, so the card's top-left lands exactly at `click_pos`
    /// (card_center = click_pos + (card_w/2, card_h/2)). `test_content_builder`
    /// metrics: card 200×108.
    #[test]
    fn test_scale_anchor_keeps_card_top_left_fixed() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Click in the middle of the screen — no edge shift, card top-left =
        // click_pos. card metrics: 200×108 → card_center = click + (100, 54).
        let click = vexo::core::Point::<Logical>::new(100.0, 300.0);
        let card_w = 200.0_f32;
        let card_h = 108.0_f32;
        let card_center =
            vexo::core::Point::<Logical>::new(click.x + card_w * 0.5, click.y + card_h * 0.5);
        controller.show(click, test_content_builder("Copy"));
        pipeline.perform_rebuilds();

        // Advance to mid-open (0 < v < 1) so the scale is meaningfully < 1.
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

        let s = 0.85 + v * 0.15;

        // Expected matrix M = S(s) + T((1-s)*(click - card_center)).
        let exp_tx = (1.0 - s) as f32 * (click.x - card_center.x);
        let exp_ty = (1.0 - s) as f32 * (click.y - card_center.y);
        let expected = vexo::AffineTransform::translation(exp_tx, exp_ty)
            .mul(&vexo::AffineTransform::scale(s as f32, s as f32));

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let matrix = find_paint_transform_around_text(ro_reg, root, "Copy").expect(
            "card's Transform render object should have a paint_transform at mid-animation",
        );

        let got = matrix.to_array();
        let exp = expected.to_array();
        let tol = 1e-3;
        for i in 0..6 {
            assert!(
                (got[i] - exp[i]).abs() < tol,
                "paint_transform[{}] = {} but expected {} (s={:.4}, v={:.4}); \
                 the matrix must be S(s)+T((1-s)*(click-center)) so the painter's \
                 center re-anchoring yields a scale about click_pos",
                i,
                got[i],
                exp[i],
                s,
                v
            );
        }

        // The real proof: the EFFECTIVE transform (painter applies
        // T(center) ∘ M ∘ T(-center)) must map the card's top-left corner
        // (which sits at click_pos) back to click_pos — i.e. the top-left
        // stays fixed during the grow animation.
        let effective = |p: vexo::core::Point<Logical>| -> vexo::core::Point<Logical> {
            let rel = vexo::core::Point::new(p.x - card_center.x, p.y - card_center.y);
            let m = matrix.transform_point(rel);
            vexo::core::Point::new(m.x + card_center.x, m.y + card_center.y)
        };
        let mapped_top_left = effective(click);
        assert!(
            (mapped_top_left.x - click.x).abs() < tol && (mapped_top_left.y - click.y).abs() < tol,
            "effective transform should map card top-left (click_pos {:?}) to itself, \
             got {:?} (s={:.4}); a non-fixed top-left means the menu grows from the \
             wrong anchor — the symptom of the old scale_about_point double-anchoring bug",
            click,
            mapped_top_left,
            s
        );

        // Sanity: a non-anchor point (card bottom-right) should move TOWARD
        // click_pos under the effective scale-about-click — proving the scale
        // really pivots at click_pos, not just that top-left is (trivially)
        // fixed by an identity matrix.
        let card_br = vexo::core::Point::<Logical>::new(click.x + card_w, click.y + card_h);
        let mapped_br = effective(card_br);
        let expected_br = vexo::core::Point::<Logical>::new(
            click.x + s as f32 * card_w,
            click.y + s as f32 * card_h,
        );
        assert!(
            (mapped_br.x - expected_br.x).abs() < tol && (mapped_br.y - expected_br.y).abs() < tol,
            "effective transform should scale card bottom-right toward click_pos: \
             got {:?}, expected {:?} (click + s*size, s={:.4})",
            mapped_br,
            expected_br,
            s
        );
    }

    // ========================================================================
    // Task 7: reactions pill + edge-aware shift-to-fit positioning
    // ========================================================================
    //
    // These tests exercise the host's edge-aware positioning logic. The host
    // places the actions card's top-left at the click point and the reactions
    // pill directly above it (pill bottom = card top, separated by `gap`), so
    // the cluster straddles the click point vertically. The cluster's
    // top-left defaults to (click_x, click_y - pill_h - gap); if that would
    // overflow the top (< 8) or bottom (> window_h - 8) edge, the whole
    // cluster slides the minimum amount to stay on-screen:
    //
    //   pill clips top    → cluster_y = 8
    //   card clips bottom → cluster_y = (window_h - 8) - cluster_h
    //   otherwise         → cluster_y = click_y - pill_h - gap
    //
    // The cluster's internal stacking (pill-above-card) is never reordered —
    // only `cluster_y` shifts. The horizontal left-clamp
    // (`cluster_x = click_x.max(8).min(window_w - cluster_w - 8)`) is exercised
    // by `test_horizontal_clamp_when_near_right_edge`.
    //
    // Test #8 and #9 below are presence-guards: they assert both cards remain
    // in the render tree (not clipped off-screen) after the host picks a
    // placement. The real shift-position assertions live in
    // `test_vertical_flip_when_no_room_below` (Task 8 tests).
    //
    // Test #9 clicks at (50, 560) in a 600px window. The cluster is
    // pill_h(28) + gap(8) + card_h(108) = 144px tall; its default top
    // (560 - 28 - 8 = 524) + cluster_h(144) = 668 > 592, so the card would
    // clip the bottom edge — the host slides the cluster up to
    // cluster_y = 592 - 144 = 448.

    /// Test #8 — presence guard: click near the top edge. With click y=5, the
    /// pill (28px + 8px gap above the click) would clip the top
    /// (5 - 28 - 8 = -31 < 8), so the host slides the cluster down to
    /// cluster_y=8. Both cards render on-screen. This test guards that the
    /// top-edge placement doesn't clip cards off-screen. (The real shift
    /// assertion is in `test_vertical_flip_when_no_room_below`.)
    ///
    /// The host is wrapped in a `MediaQuery` with size=(400, 600) so
    /// `MediaQuery::of(ctx)` returns the real window size for edge detection.
    #[test]
    fn test_edge_flip_when_no_room_above() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        // Wrap in MediaQuery so the host reads a real window size via
        // `MediaQuery::of(ctx)` for edge detection.
        let mq_data = vexo::MediaQueryData {
            size: Size::new(400.0, 600.0),
            ..vexo::MediaQueryData::all_zero()
        };
        let host = vexo::MediaQuery::new(mq_data, host);
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Click point at the very top. The pill would clip the top edge
        // (5 - 28 - 8 = -31 < 8), so the host slides the cluster down to
        // cluster_y=8. Presence guard: neither card should be clipped.
        controller.show(
            vexo::core::Point::new(50.0, 5.0),
            test_content_builder("Copy"),
        );
        // Settle to Open so we can inspect the laid-out positions.
        pipeline.perform_rebuilds();
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Both cards should be present in the render tree (not clipped).
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "actions card should still be rendered with top-edge click"
        );
        // The key assertion: both "r" (reactions) and "Copy" (actions) appear,
        // proving neither card was clipped off-screen.
        assert!(
            find_text_in_tree(ro_reg, root, "r"),
            "reactions pill should still be rendered with top-edge click"
        );
    }

    /// Test #9 — edge shift when no room below for the actions card.
    ///
    /// Click pinned near the bottom of a 600px window (y=560). The card's
    /// top-left defaults to the click point, so the cluster's default top is
    /// 560 - 28 - 8 = 524; the cluster bottom (524 + 144 = 668) overflows the
    /// window (592), so the host slides the whole cluster up to
    /// cluster_y = 592 - 144 = 448. Both cards must remain in the render tree
    /// (not clipped off-screen).
    ///
    /// Wrapped in `MediaQuery` (see test #8 rationale).
    #[test]
    fn test_edge_flip_when_no_room_below() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let mq_data = vexo::MediaQueryData {
            size: Size::new(400.0, 600.0),
            ..vexo::MediaQueryData::all_zero()
        };
        let host = vexo::MediaQuery::new(mq_data, host);
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Click point near the bottom — no room below for the actions card.
        controller.show(
            vexo::core::Point::new(50.0, 560.0),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Both cards should be on-screen (cluster shifted up).
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "actions card should still be rendered with edge shift"
        );
        assert!(
            find_text_in_tree(ro_reg, root, "r"),
            "reactions pill should still be rendered with edge shift"
        );

        // Bounds check: the reactions pill's `PositionedRenderObject` (the
        // outer wrapper that carries the absolute (pill_x, pill_y) offset)
        // must have `computed_bounds.top >= 0.0` — i.e. not clipped off the
        // top of the screen. We check the `PositionedRenderObject`'s bounds
        // (not the inner `TextRenderObject`'s) because the Text's
        // `computed_bounds` is local to its layout origin (always 0,0), while
        // the `Positioned`'s reflects the absolute laid-out position in window
        // coords. With click y=560 and cluster_h=144, the host slides up:
        // `cluster_y = (600 - 8) - 144 = 448`, so pill_top should be 448 (well
        // within bounds). A bug in the shift math (wrong sign, missing
        // cluster_h term) could push pill_top negative, which this assertion
        // catches. The presence checks above plus this bounds check together
        // guard the shift-up positioning.
        let pill_bounds = find_positioned_bounds_around_text(ro_reg, root, "r").expect(
            "reactions pill's PositionedRenderObject should have computed_bounds after layout \
             (find_positioned_bounds_around_text found none — pill was not laid out)",
        );
        assert!(
            pill_bounds.top >= 0.0,
            "reactions pill must not be clipped off the top of the screen \
             (computed_bounds.top={}, expected >= 0.0); a negative top indicates \
             the shift-up math overflowed past the window origin",
            pill_bounds.top
        );
    }

    /// Test — click-point anchor: opening the menu at a known click_pos places
    /// the card's Positioned at (click_x, click_y) and the pill's at
    /// (click_x, click_y - pill_h - gap), when there's room (no shift/clamp).
    #[test]
    fn test_click_point_anchor_default_placement() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let mq_data = vexo::MediaQueryData {
            size: Size::new(400.0, 600.0),
            ..vexo::MediaQueryData::all_zero()
        };
        let host = vexo::MediaQuery::new(mq_data, host);
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Click at (100, 200). test_content_builder's metrics: pill 150×28,
        // card 200×108, gap 8 → cluster 200×144. Default cluster top =
        // 200 - 28 - 8 = 164 (≥ 8 ✓); cluster bottom = 164 + 144 = 308
        // (≤ 592 ✓). No shift. Cluster width = max(150, 200) = 200, fits
        // right (100 + 200 = 300 < 392). No clamp.
        controller.show(
            vexo::core::Point::new(100.0, 200.0),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();
        // Settle to Open so scale = 1.0 (Positioned offsets are unaffected by
        // the scale transform — Transform is paint-only — but settle anyway
        // for a clean state).
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        // Pill "r" Positioned should be at (100, 200 - 28 - 8) = (100, 164).
        let pill_bounds = find_positioned_bounds_around_text(ro_reg, root, "r")
            .expect("pill Positioned should have bounds");
        assert!(
            (pill_bounds.left - 100.0).abs() < 0.5,
            "pill left should be click_x (100), got {}",
            pill_bounds.left
        );
        assert!(
            (pill_bounds.top - 164.0).abs() < 0.5,
            "pill top should be click_y - pill_h - gap (164), got {}",
            pill_bounds.top
        );

        // Card "Copy" Positioned should be at (100, 200) — the click point.
        let card_bounds = find_positioned_bounds_around_text(ro_reg, root, "Copy")
            .expect("card Positioned should have bounds");
        assert!(
            (card_bounds.left - 100.0).abs() < 0.5,
            "card left should be click_x (100), got {}",
            card_bounds.left
        );
        assert!(
            (card_bounds.top - 200.0).abs() < 0.5,
            "card top should be click_y (200), got {}",
            card_bounds.top
        );
    }

    // ========================================================================
    // Task 8: edge-case tests (vertical shift, horizontal clamp, instant dismiss)
    // ========================================================================
    //
    // These tests cover the spec's required edge cases against the Task 3
    // 3-layer Stack render. They use `test_content_builder` whose metrics are
    // pill 150×28, card 200×108, gap 8 → cluster_w = max(150, 200) = 200,
    // cluster_h = 28 + 8 + 108 = 144. (The real builder's metrics are larger
    // — 222×44 / 200×134, cluster 222×186 — but these tests use the test
    // builder, so assertions cite 200/144, not 222/186.)

    /// Test — vertical shift: click near the bottom edge slides the cluster
    /// up so it doesn't overflow the window.
    ///
    /// `test_content_builder` metrics: pill_h=28, gap=8, card_h=108 →
    /// cluster_h = 144. Click at y=590 in a 600px window:
    ///   default cluster top = 590 - 28 - 8 = 554
    ///   cluster bottom = 554 + 144 = 698 > 592 (window_h - 8) → clip
    ///   → cluster_y = 592 - 144 = 448 (slid up)
    /// The pill sits at the top of the cluster, so pill_top = cluster_y = 448.
    #[test]
    fn test_vertical_flip_when_no_room_below() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let mq_data = vexo::MediaQueryData {
            size: Size::new(400.0, 600.0),
            ..vexo::MediaQueryData::all_zero()
        };
        let host = vexo::MediaQuery::new(mq_data, host);
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Click at y=590 (near bottom). cluster_h=144 (test_content_builder).
        controller.show(
            vexo::core::Point::new(100.0, 590.0),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();
        // Settle to Open so scale = 1.0 (Positioned offsets are unaffected by
        // the scale transform — Transform is paint-only — but settle anyway
        // for a clean state, mirroring test_click_point_anchor_default_placement).
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let pill_bounds = find_positioned_bounds_around_text(ro_reg, root, "r")
            .expect("pill Positioned should have bounds");
        // Pill should be ABOVE the click point after the shift.
        assert!(
            pill_bounds.top < 590.0,
            "pill top ({}) should be above click_y (590) after shift",
            pill_bounds.top
        );
        // cluster_y = (window_h - 8) - cluster_h = 592 - 144 = 448.
        assert!(
            (pill_bounds.top - 448.0).abs() < 1.0,
            "pill top should be cluster_y (448 = (window_h - 8) - cluster_h = 592 - 144), got {}",
            pill_bounds.top
        );
    }

    /// Test — horizontal left-clamp: click near the right edge shifts the
    /// cluster left so its right edge stays at window_w - 8.
    ///
    /// `test_content_builder` metrics: pill_w=150, card_w=200 → cluster_w =
    /// max(150, 200) = 200. Click at x=390 in a 400px window:
    ///   lo = 8.0
    ///   hi = 400 - 200 - 8 = 192
    ///   cluster_x = 390.max(8).min(192) = 192 (clamped left)
    /// The pill sits at the cluster's left edge, so pill_left = cluster_x = 192.
    #[test]
    fn test_horizontal_clamp_when_near_right_edge() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let mq_data = vexo::MediaQueryData {
            size: Size::new(400.0, 600.0),
            ..vexo::MediaQueryData::all_zero()
        };
        let host = vexo::MediaQuery::new(mq_data, host);
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Click at x=390 (near right edge). cluster_w=200 (test_content_builder).
        controller.show(
            vexo::core::Point::new(390.0, 200.0),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let pill_bounds = find_positioned_bounds_around_text(ro_reg, root, "r")
            .expect("pill Positioned should have bounds");
        // cluster_x = window_w - 8 - cluster_w = 400 - 8 - 200 = 192.
        assert!(
            (pill_bounds.left - 192.0).abs() < 1.0,
            "pill left should be clamped to 192 (window_w - 8 - cluster_w = 400 - 8 - 200), got {}",
            pill_bounds.left
        );
    }

    /// Test — instant dismiss: close() immediately sets phase=Closed and
    /// the overlay layers unmount on the next rebuild (no Closing phase, no
    /// reverse spring — Task 1 dropped the Closing phase).
    #[test]
    fn test_close_unmounts_overlay_immediately() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());
        let ticker = Arc::new(AnimationTicker::new());

        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(
            vexo::core::Point::new(100.0, 200.0),
            test_content_builder("Copy"),
        );
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Menu content is mounted while open.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "menu should be rendered when open"
        );

        // close() instantly clears to Closed — no Closing phase, no reverse
        // spring. The overlay unmounts on the next rebuild.
        controller.close();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            !find_text_in_tree(ro_reg, root, "Copy"),
            "menu content should be unmounted immediately after close()"
        );
        assert_eq!(controller.phase(), Phase::Closed);
    }

    /// Regression test: `close()` must fire the dirty callback so the host
    /// rebuilds and unmounts the overlay. Without this, `close()` sets
    /// phase=Closed but the host never re-renders — the menu stays visible
    /// forever (observed during Task 6 manual verification). The root cause:
    /// `animation.stop()` unregisters from the ticker but does NOT itself fire
    /// the dirty callback, so `close()` must fire it explicitly.
    ///
    /// This test uses a counting dirty callback (no pipeline) to isolate the
    /// controller behavior from the host rebuild cycle — `perform_rebuilds`
    /// masks the bug because it rebuilds unconditionally, bypassing the
    /// dirty-callback gate that the real app relies on.
    #[test]
    fn test_close_fires_dirty_callback() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc as StdArc;

        let controller = ContextMenuController::new();
        let count = StdArc::new(AtomicU32::new(0));
        let count_for_cb = count.clone();
        controller.set_dirty_callback(Arc::new(move || {
            count_for_cb.fetch_add(1, Ordering::SeqCst);
        }));

        // Wire a ticker so show()'s animate_with registers + fires dirty.
        let ticker = Arc::new(AnimationTicker::new());
        controller.set_animation_ticker(ticker.clone());

        // show() fires dirty once (animate_with fires immediately).
        controller.show(
            vexo::core::Point::new(10.0, 10.0),
            test_content_builder("Copy"),
        );
        let after_show = count.load(Ordering::SeqCst);
        assert!(
            after_show >= 1,
            "show() should fire the dirty callback at least once, got {}",
            after_show
        );

        // close() must fire dirty so the host rebuilds to unmount the overlay.
        // This is the regression guard: before the fix, close() set phase=Closed
        // but never fired dirty, so the host never rebuilt.
        controller.close();
        let after_close = count.load(Ordering::SeqCst);
        assert!(
            after_close > after_show,
            "close() should fire the dirty callback (count before={}, after={}); \
             without it the host never rebuilds and the overlay stays mounted",
            after_show,
            after_close
        );
    }
}
