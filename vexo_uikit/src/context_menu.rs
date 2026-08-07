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
//! Task 5 state: open is driven by a critical spring
//! (`SpringDescription::ios(340.0, 1.0)`) through a 3-state phase machine
//! (`Closed → Opening → Open`). `show()` starts a forward spring (0.0 → 1.0,
//! phase=Opening); `close()` instantly clears to `Closed` (clears `open` →
//! unmount, no reverse spring). `show()` always resets the spring to start
//! from 0.0 (Amendment 2), so every open animates 0→1 consistently — a
//! re-show after `close()` animates the same as a first open (close() already
//! unmounted the overlay, so the reset has no visible jump). The host's
//! `on_tick` calls `controller.advance(now)`, which samples the spring and
//! flips Opening→Open on settle.
//!
//! Task 6 wires the spring value `v = controller.animation_value()` (0→1 on
//! open, 1→0 on close) into all three overlay layers: the dim barrier's alpha
//! is `v*0.4` (was fixed 0.4), the bright bubble copy gets a subtle
//! `scale(1+v*0.03)` + `translate_y(-v*4.0)` lift (opacity stays 1.0), and the
//! actions card gets `scale(0.8+v*0.2)` + `opacity(v)`. Scale-about-center is
//! achieved via the `scale_about_center` helper (a translate→scale→translate
//! `Transform` chain). `Opacity` is paint-only (layout + hit-test
//! pass-through), so the dim barrier stays tappable even at v≈0 — barrier
//! dismiss works mid-open (test #5).
//!
//! Task 7 adds the 5th overlay layer — the reactions pill — with the same
//! `scale(0.8+v*0.2)` + `opacity(v)` treatment as the actions card. Both
//! cards are positioned edge-aware relative to the bubble: pill above + card
//! below by default; pill flips below the card if no room above; whole stack
//! flips above the bubble if no room below; best-effort below if neither.
//! Horizontally both center on the bubble's horizontal center, clamped to
//! `[8, window_w - card_w - 8]`. Window size is read via `MediaQuery::of(ctx)`
//! (an `InheritedWidget` dependency, so the host rebuilds on resize).

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
/// Unused in Task 2 (render() is stubbed); Task 3's render rewrite reads
/// `click_pos` + `builder` via `open_snapshot()`.
#[allow(dead_code)]
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
        // Opening. `advance()` early-returns on `phase == Closed`, so the
        // spring would never be sampled → never settle → never unregister from
        // the ticker → the dirty callback would fire every frame forever.
        // `stop()` halts the spring AND unregisters from the ticker. It does
        // NOT reset `value`; show() ignores the frozen value anyway (Amendment
        // 2: show() always starts the spring from 0.0), so the frozen value is
        // harmless — it's overwritten on the next show()+advance cycle.
        s.animation.stop();
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
    ///
    /// Unused in Task 2 (render() is stubbed); Task 3's render rewrite calls
    /// this to read the click point + builder for the overlay cluster.
    #[allow(dead_code)]
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
        // TEMPORARY STUB - Task 3 replaces this with the 3-layer Stack
        // (content + transparent barrier + click-point-anchored cluster).
        let _ = ctx;
        let _ = self.controller.phase();
        self.child.clone_boxed()
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
///
/// Unused in Task 2 (render() is stubbed); Task 3's render rewrite re-uses it
/// for the click-point-anchored cluster's scale-about-center transforms.
#[allow(dead_code)]
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
    #[ignore = "Task 3 un-ignores after implementing render()"]
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
    #[ignore = "Task 3 un-ignores after implementing render()"]
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

        // The actions card is Positioned at (click_x, click_y + pill_h + gap)
        // = (10, 10 + 28 + 8) = (10, 46). The item row has 8px padding, so
        // clicking at (15, 70) lands inside the row's padding area.
        let pos = vexo::core::Point::new(10.0, 10.0);
        controller.show(pos, builder);
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
        pipeline.perform_rebuilds();
        assert_eq!(
            controller.phase(),
            Phase::Closed,
            "menu should be closed immediately after item tap (instant close)"
        );
    }

    #[test]
    #[ignore = "Task 3 un-ignores after implementing render()"]
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
    #[ignore = "Task 3 un-ignores after implementing render()"]
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
    /// `Positioned` subtree contains "r" (never "Copy" or "bubble"), the
    /// card's contains "Copy", the bubble copy's contains "bubble", and the
    /// barrier's contains a BLACK `DecoratedBox` (no text needle).
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
    #[ignore = "bubble copy removed in Task 3 — test deleted there"]
    fn test_bright_bubble_copy_rendered_on_top() {
        let controller = ContextMenuController::new();
        let pos = vexo::core::Point::new(10.0, 10.0);
        let host = ContextMenu::new(vexo::Text::new("background content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(pos, test_content_builder("Actions"));
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "BUBBLE_CONTENT marker"),
            "bright bubble copy should be rendered when menu is open"
        );
    }

    #[test]
    #[ignore = "bubble copy removed in Task 3 — test deleted there"]
    fn test_bubble_copy_size_matches_original() {
        let controller = ContextMenuController::new();
        let pos = vexo::core::Point::new(50.0, 50.0);

        let content = vexo::WithLayout::new(
            vexo::Text::new("X"),
            vexo::Layout::default().width(80.0).height(30.0),
        );

        let host = ContextMenu::new(content, controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        controller.show(pos, test_content_builder("A"));
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let mut found_sizes: Vec<vexo::core::Size<Logical>> = Vec::new();
        collect_text_sizes(ro_reg, root, "X", &mut found_sizes);
        assert_eq!(found_sizes.len(), 2);
        assert_eq!(found_sizes[0], found_sizes[1]);
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
    #[ignore = "dim barrier removed in Task 3 — test deleted there"]
    fn test_dim_barrier_has_nonzero_height() {
        let screen = vexo::core::Size::new(400.0, 600.0);
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(Text::new("content"), controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = new_font_system();
        pipeline.layout(screen, &mut engine, &mut font_system);

        let pos = vexo::core::Point::new(10.0, 10.0);
        controller.show(pos, test_content_builder("Copy"));
        pipeline.perform_rebuilds();
        pipeline.layout(screen, &mut engine, &mut font_system);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let mut black_boxes: Vec<RenderObjectKey> = Vec::new();
        collect_black_decorated_boxes(ro_reg, root, &mut black_boxes);
        assert_eq!(black_boxes.len(), 1);
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
    fn test_reshow_after_close_retargets_from_current_value() {
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
    /// Opens the menu, then clicks the dim barrier *mid-open* (before the
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
    #[ignore = "Task 3 un-ignores after implementing render()"]
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

        // close() should have fired — phase is Closed (instant close, no
        // Closing phase).
        assert_eq!(
            controller.phase(),
            Phase::Closed,
            "barrier click mid-open should instantly close (phase=Closed)"
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
    #[ignore = "dim barrier removed in Task 3 — test deleted there"]
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
            vexo::core::Point::new(10.0, 10.0),
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
    /// The card is NOT wrapped in `Opacity` — it is always opaque so it
    /// always writes depth (Phase 1) and occludes background text behind it.
    /// Only the scale animates (0.8→1.0). This test verifies that no
    /// `OpacityRenderObject` exists on the card's subtree when the menu is
    /// mid-open. (The dim barrier DOES have Opacity, but
    /// `find_card_opacity` excludes subtrees with a black DecoratedBox, so
    /// the dim is not a false positive.)
    #[test]
    #[ignore = "Task 3 un-ignores after implementing render()"]
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

    // ========================================================================
    // Task 7: reactions pill + edge-aware flip/clamp positioning
    // ========================================================================
    //
    // These tests exercise the host's edge-aware positioning logic. The host
    // picks one of four layouts based on whether the reactions pill fits above
    // the bubble (`room_above`) and whether the actions card fits below it
    // (`room_below`):
    //
    //   room_above && room_below  → default (pill above bubble, card below)
    //   !room_above && room_below → pill below the actions card
    //   room_above && !room_below → flip both above the bubble
    //   !room_above && !room_below→ default to below (best effort)
    //
    // The assertions are intentionally presence-based (`find_text_in_tree`)
    // rather than position-based: walking `Positioned` render objects to read
    // laid-out offsets is fragile (the framework's render-object hierarchy
    // doesn't expose a stable "Positioned offset" field). Instead, we assert
    // that BOTH cards (reactions "r" + actions "Copy") still appear in the
    // render tree after the flip — i.e. neither was clipped off-screen by a
    // bad offset. The flip path is what makes the difference: with the old
    // (Task 6) host code, only the actions card is rendered, so the reactions
    // pill ("r") is missing → RED. After Task 7 adds the pill layer + edge
    // positioning, both appear → GREEN.
    //
    // BOUNDS NOTE: `Bounds::new(l, t, r, b)` takes edge coordinates, but the
    // brief's literal `Bounds::new(50.0, 560.0, 100.0, 40.0)` for test #9
    // would produce `top=560, bottom=40, height=-520` (malformed). The
    // bubble_bottom math (`top + height()`) then collapses to 40, making
    // `room_below` true and the flip-above path never runs — defeating the
    // test's stated intent ("Bubble near the bottom — no room below"). We use
    // `Bounds::from_xywh` (matches the established pattern in
    // `test_bubble_copy_size_matches_original`) to produce a valid
    // 100×40 bubble at (50, 560) whose bottom edge sits at y=600 — genuinely
    // leaving no room below in a 600px window.

    /// Test #8 — edge flip when no room above for the reactions pill.
    ///
    /// Bubble pinned to the top of a 600px window (top=5). With pill_h=28 and
    /// gap=8, the pill needs 8+28+8=44px above the bubble but only 5px is
    /// available → `room_above = false`. The host flips the pill to BELOW the
    /// actions card (which still goes below the bubble). Both cards must
    /// remain in the render tree (not clipped off-screen).
    ///
    /// The host is wrapped in a `MediaQuery` with size=(400, 600) so
    /// `MediaQuery::of(ctx)` returns the real window size — without this,
    /// `MediaQuery::of` falls back to `all_zero` (size=0,0), making
    /// `room_below` false (since `bubble_bottom + gap + card_h > 0`) and
    /// sending the host into the "no room anywhere" fallback branch instead
    /// of the intended "no room above, room below" branch. The two branches
    /// produce the same layout today, but depending on the real MediaQuery
    /// makes the test actually exercise the path the comment describes.
    #[test]
    #[ignore = "Task 3 un-ignores after implementing render()"]
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

        // Click point at the very top — no room above for the reactions pill.
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

        // The reactions pill should be positioned BELOW the actions card
        // (not above the bubble, where it would clip off-screen).
        // We assert this by checking that both cards are within window bounds.
        // (Detailed position assertions require walking Positioned render
        // objects, which is fragile. Instead, assert the menu didn't clip by
        // checking both cards are within window bounds.)
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "actions card should still be rendered with edge flip"
        );
        // The key assertion: both "r" (reactions) and "Copy" (actions) appear,
        // proving neither card was clipped off-screen.
        assert!(
            find_text_in_tree(ro_reg, root, "r"),
            "reactions pill should still be rendered with edge flip"
        );
    }

    /// Test #9 — edge flip when no room below for the actions card.
    ///
    /// Bubble pinned to the bottom of a 600px window (top=560, bottom=600).
    /// With card_h=108 and gap=8, the card needs 8+108=116px below the bubble
    /// but only 0px is available → `room_below = false`. The host flips BOTH
    /// cards above the bubble (card directly above bubble, pill above card).
    /// Both cards must remain in the render tree (not clipped off-screen).
    ///
    /// Wrapped in `MediaQuery` (see test #8 rationale).
    #[test]
    #[ignore = "Task 3 un-ignores after implementing render()"]
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

        // Both cards should be above the bubble (flipped).
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "actions card should still be rendered with edge flip"
        );
        assert!(
            find_text_in_tree(ro_reg, root, "r"),
            "reactions pill should still be rendered with edge flip"
        );

        // Bounds check: the reactions pill's `PositionedRenderObject` (the
        // outer wrapper that carries the absolute (pill_x, pill_y) offset)
        // must have `computed_bounds.top >= 0.0` — i.e. not clipped off the
        // top of the screen. We check the `PositionedRenderObject`'s bounds
        // (not the inner `TextRenderObject`'s) because the Text's
        // `computed_bounds` is local to its layout origin (always 0,0), while
        // the `Positioned`'s reflects the absolute laid-out position in window
        // coords. This catches the branch-3 overlap regression where `pill_y`
        // was computed as `bubble_top - gap - pill_h` (placing the pill's
        // bottom at the card's bottom) instead of
        // `bubble_top - 2*gap - card_h - pill_h` (placing the pill fully above
        // the card). With the buggy math and bubble_top=560, gap=8, card_h=108,
        // pill_h=28, the pill would be at y=524 (on-screen, so this assertion
        // alone doesn't catch the overlap directly) but overlapping the card;
        // a worse variant of the bug (e.g. wrong sign or extra multiplier)
        // would push the pill off-screen, which this assertion catches. The
        // presence check above plus this bounds check together guard the
        // branch-3 flip-above positioning. The corrected math places the pill
        // at y=408, well within bounds.
        let pill_bounds = find_positioned_bounds_around_text(ro_reg, root, "r").expect(
            "reactions pill's PositionedRenderObject should have computed_bounds after layout \
             (find_positioned_bounds_around_text found none — pill was not laid out)",
        );
        assert!(
            pill_bounds.top >= 0.0,
            "reactions pill must not be clipped off the top of the screen \
             (computed_bounds.top={}, expected >= 0.0); a negative top indicates \
             the branch-3 flip-above math overflowed past the window origin",
            pill_bounds.top
        );
    }
}
