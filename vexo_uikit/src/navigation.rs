//! NavigationStackView component — a stack navigator with a caller-owned
//! `NavigationController<Dest>`.
//!
//! Provides a SwiftUI `NavigationStack`-style LIFO stack: a root page plus
//! pushed pages. The caller owns the controller and mutates the path via
//! `push`/`pop`/`pop_to_root`/`replace`; the controller's dirty callback
//! (wired during mount) triggers rebuilds so the view always reflects the
//! current top-of-stack.
//!
//! Push/pop transitions are animated. The controller records a *pending
//! transition* (snapshots of the path before/after the mutation) when the
//! caller invokes `push`/`pop`/`pop_to_root`/`replace`. The view's state
//! observes the pending op, runs an `AnimationController` over a duration,
//! and during the transition renders a `Stack` with both the outgoing and
//! incoming pages wrapped in transition builders. When the animation
//! completes, the pending op is cleared and the view returns to its
//! steady-state `IndexedStack` rendering.
//!
//! For a two-column sidebar+detail layout, compose manually with `MultiChild`
//! and `Layout::row()`
//! and a `Signal<Option<T>>` for the selection — see `shared_app` for a
//! worked example. A framework-level `NavigationSplitView` was intentionally
//! removed: it baked in assumptions about the detail content's nav bar that
//! conflicted when composed with a nested `NavigationStackView`.
//!
//! # Example
//!
//! ```ignore
//! let controller: NavigationController<&'static str> = NavigationController::new();
//! NavigationStackView::new(controller, Text::new("Root"))
//!     .root_title("Home")
//!     .title(|d| d.to_string())
//!     .destination(|d| Text::new(format!("Page: {}", d)).boxed())
//!     .boxed()
//! ```

use std::any::Any;
use std::cell::RefCell;
use std::hash::Hash;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vexo::animation::{SpringDescription, SpringSimulation};
use vexo::EdgePanDetector;
use vexo::VelocityTracker;
use vexo::{
    children, AlignItems, AnimationController, Component, ComponentState, CubicBezierCurve, Curve,
    DecoratedBox, FractionalTranslation, IndexedStack, JustifyContent, Layout, LifecycleContext,
    MediaQuery, MultiChild, Opacity, Positioned, RenderContext, SafeArea, Stack, Style, Text,
    Theme, Widget, WithLayout,
};

use crate::platform::Platform;
use crate::theme::tokens;
use crate::theme::tokens::navigation::NavColors;
use crate::transitions::{default_transition, TransitionCtx, TransitionDir};
use vexo_fontawesome::{Icon, Icons};

// ============================================================================
// PENDING TRANSITION OP
// ============================================================================

/// A deferred navigation operation captured at the moment the caller invokes
/// `push`/`pop`/`pop_to_root`/`replace`.
///
/// `from` is the path snapshot before the mutation; `to` is the path snapshot
/// after. `kind` records which operation was performed so the transition
/// builder can animate in the correct direction.
#[derive(Clone, Debug)]
pub struct PendingOp<Dest: Hash + Eq + Clone + 'static> {
    pub from: Vec<Dest>,
    pub to: Vec<Dest>,
    pub kind: TransitionDir,
}

// ============================================================================
// NAVIGATION CONTROLLER
// ============================================================================

/// External controller that owns the navigation path for a NavigationStackView.
///
/// Inspired by SwiftUI's `NavigationPath` + Flutter's `TextEditingController`:
/// the caller creates and owns this controller, passing it into
/// NavigationStackView. The controller holds the LIFO stack of pushed
/// destinations; mutating methods (`push`, `pop`, etc.) fire a dirty callback
/// wired by the framework during mount, triggering a rebuild.
///
/// The path and dirty callback are shared via `Rc<RefCell<...>>` so that
/// clones captured in closures *before* wiring still observe mutations and
/// fire the callback once wired. This mirrors `TextEditingController`.
///
/// # Two-phase transitions
///
/// When a transition is in flight, the controller records a `PendingOp`
/// capturing the path before/after the mutation. The view's state observes
/// `pending()`, runs an `AnimationController`, and during the transition
/// renders both the outgoing and incoming pages. When the animation
/// completes, the view's state calls `clear_pending()` and returns to
/// steady-state `IndexedStack` rendering.
pub struct NavigationController<Dest: Hash + Eq + Clone + 'static> {
    path: Rc<RefCell<Vec<Dest>>>,
    pending: Rc<RefCell<Option<PendingOp<Dest>>>>,
    dirty_callback: Rc<RefCell<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl<Dest: Hash + Eq + Clone + 'static> NavigationController<Dest> {
    /// Create a new controller with an empty path (at root).
    pub fn new() -> Self {
        Self {
            path: Rc::new(RefCell::new(Vec::new())),
            pending: Rc::new(RefCell::new(None)),
            dirty_callback: Rc::new(RefCell::new(None)),
        }
    }

    /// Snapshot the current path for inspection.
    pub fn path(&self) -> Vec<Dest> {
        self.path.borrow().clone()
    }

    /// Current stack depth (path length). `0` means at root.
    pub fn depth(&self) -> usize {
        self.path.borrow().len()
    }

    /// Snapshot of the pending transition op, if any.
    pub fn pending(&self) -> Option<PendingOp<Dest>> {
        self.pending.borrow().clone()
    }

    /// Clear the pending op. Called by the view's state when the transition
    /// animation completes.
    pub fn clear_pending(&self) {
        *self.pending.borrow_mut() = None;
    }

    /// Push a new destination onto the stack. The next render starts a
    /// push transition; the actual path mutation takes effect immediately
    /// (so `path()` reflects the new top), but the transition overlay shows
    /// both the outgoing and incoming pages until the animation completes.
    pub fn push(&self, dest: Dest) {
        let from = self.path.borrow().clone();
        let mut to = from.clone();
        to.push(dest.clone());
        self.path.borrow_mut().push(dest);
        *self.pending.borrow_mut() = Some(PendingOp {
            from,
            to,
            kind: TransitionDir::Push,
        });
        self.notify();
    }

    /// Pop the top destination. No-op at root (returns `None`).
    /// Returns the popped value when the path was non-empty.
    pub fn pop(&self) -> Option<Dest> {
        let from = self.path.borrow().clone();
        let popped = self.path.borrow_mut().pop();
        if popped.is_some() {
            let to = self.path.borrow().clone();
            *self.pending.borrow_mut() = Some(PendingOp {
                from,
                to,
                kind: TransitionDir::Pop,
            });
            self.notify();
        }
        popped
    }

    /// Clear the entire path, returning to root. Idempotent: a no-op (and no
    /// dirty fire) if the path is already empty.
    pub fn pop_to_root(&self) {
        let from = self.path.borrow().clone();
        if from.is_empty() {
            return;
        }
        self.path.borrow_mut().clear();
        *self.pending.borrow_mut() = Some(PendingOp {
            from,
            to: Vec::new(),
            kind: TransitionDir::PopToRoot,
        });
        self.notify();
    }

    /// Replace the top of the stack with `dest`. At root (empty path), behaves
    /// as `push(dest)` — documented, not an error.
    pub fn replace(&self, dest: Dest) {
        let from = self.path.borrow().clone();
        {
            let mut p = self.path.borrow_mut();
            if let Some(top) = p.last_mut() {
                *top = dest.clone();
            } else {
                p.push(dest.clone());
            }
        }
        let to = self.path.borrow().clone();
        // Replace animates as a push (new page incoming).
        *self.pending.borrow_mut() = Some(PendingOp {
            from,
            to,
            kind: TransitionDir::Push,
        });
        self.notify();
    }

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

    // --- Framework wiring (called by NavigationStackViewState lifecycle) ---

    /// Wire the dirty callback. Called from `ComponentState::on_mount` (and
    /// `on_update` when the widget's controller instance changes), reading the
    /// controller off `ctx.widget()`. Takes `&self` because the callback cell
    /// is a `RefCell`.
    pub fn set_dirty_callback(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.dirty_callback.borrow_mut() = Some(callback);
    }

    /// Clear the dirty callback. Called from `ComponentState::on_unmount`.
    pub fn clear_dirty_callback(&self) {
        *self.dirty_callback.borrow_mut() = None;
    }

    /// Fire the dirty callback if set. Called by `push`/`pop`/etc. after
    /// mutating the path.
    fn notify(&self) {
        if let Some(cb) = self.dirty_callback.borrow().as_ref() {
            cb();
        }
    }
}

impl<Dest: Hash + Eq + Clone + 'static> Clone for NavigationController<Dest> {
    fn clone(&self) -> Self {
        Self {
            path: Rc::clone(&self.path),
            pending: Rc::clone(&self.pending),
            dirty_callback: Rc::clone(&self.dirty_callback),
        }
    }
}

impl<Dest: Hash + Eq + Clone + 'static> Default for NavigationController<Dest> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// NAVIGATION STACK VIEW
// ============================================================================

/// A stack navigation component: a root page plus a LIFO stack of pushed pages.
///
/// Modeled on SwiftUI's `NavigationStack`. The caller owns a
/// `NavigationController<Dest>` and mutates the path via `push`/`pop`/etc.;
/// the controller's dirty callback (wired during mount) triggers rebuilds so
/// the view always reflects the current top-of-stack.
///
/// The component renders a NavBar (title + optional back button) above either
/// the root widget (empty path) or the destination closure's output (non-empty
/// path). No `ScrollView`, padding, or background is applied to the page —
/// callers wrap their page content as desired.
///
/// # Transitions
///
/// Push/pop is animated. By default the transition is platform-aware: a
/// horizontal slide on mobile (iOS-style), a fade on desktop. Override via
/// `transition()` to supply a custom builder. Duration and curve are
/// configurable via `transition_duration()` and `transition_curve()`.
pub struct NavigationStackView<Dest: Hash + Eq + Clone + 'static> {
    controller: NavigationController<Dest>,
    root: Box<dyn Widget>,
    destination: Rc<dyn Fn(&Dest) -> Box<dyn Widget>>,
    title: Rc<dyn Fn(&Dest) -> String>,
    root_title: Option<String>,
    platform: Option<Platform>,
    transition: Option<Rc<dyn Fn(&TransitionCtx, Box<dyn Widget>) -> Box<dyn Widget>>>,
    transition_duration: Duration,
    transition_curve: Rc<dyn Curve>,
}

impl<Dest: Hash + Eq + Clone + 'static> Clone for NavigationStackView<Dest> {
    fn clone(&self) -> Self {
        Self {
            controller: self.controller.clone(),
            root: self.root.clone_boxed(),
            destination: self.destination.clone(),
            title: self.title.clone(),
            root_title: self.root_title.clone(),
            platform: self.platform,
            transition: self.transition.clone(),
            transition_duration: self.transition_duration,
            transition_curve: Rc::clone(&self.transition_curve),
        }
    }
}

/// Default transition duration on mobile (iOS native push/pop duration).
const DEFAULT_MOBILE_TRANSITION_DURATION: Duration = Duration::from_millis(350);
/// Default transition duration on desktop.
const DEFAULT_DESKTOP_TRANSITION_DURATION: Duration = Duration::from_millis(200);

impl<Dest: Hash + Eq + Clone + 'static> NavigationStackView<Dest> {
    /// Create a stack view with the given controller and root page widget.
    pub fn new(controller: NavigationController<Dest>, root: impl Widget + 'static) -> Self {
        Self {
            controller,
            root: Box::new(root),
            destination: Rc::new(|_| Text::new("").boxed()),
            title: Rc::new(|_| String::new()),
            root_title: None,
            platform: None,
            transition: None,
            transition_duration: DEFAULT_MOBILE_TRANSITION_DURATION,
            // iOS-style ease-out-cubic: strong end-deceleration produces the
            // "more slow at finish" feel of native UINavigationController
            // push/pop, and makes the incoming page cover most travel early
            // (perceptually "appearing near settled position" rather than
            // sliding in from the right edge).
            transition_curve: Rc::new(CubicBezierCurve::new(0.33, 1.0, 0.68, 1.0)),
        }
    }

    /// Provide a closure that builds the page widget for a pushed destination.
    /// Called at most once per rebuild, with `path.last()`.
    pub fn destination<F: Fn(&Dest) -> Box<dyn Widget> + 'static>(mut self, f: F) -> Self {
        self.destination = Rc::new(f);
        self
    }

    /// Provide a closure returning the NavBar title for a pushed destination.
    /// Default: returns an empty string.
    pub fn title<F: Fn(&Dest) -> String + 'static>(mut self, f: F) -> Self {
        self.title = Rc::new(f);
        self
    }

    /// Set the NavBar title shown when at root. Default: `None` (empty title).
    pub fn root_title(mut self, title: impl Into<String>) -> Self {
        self.root_title = Some(title.into());
        self
    }

    /// Override the platform. Used to select the default transition style
    /// (mobile = slide, desktop = fade) and to pass to the transition builder.
    pub fn platform(mut self, p: Platform) -> Self {
        self.platform = Some(p);
        self
    }

    /// Supply a custom transition builder. The builder receives a
    /// `TransitionCtx` (with the eased `t`, direction, and platform) and the
    /// page widget, and returns the page wrapped in transform/opacity widgets
    /// as desired.
    ///
    /// When `None`, the view uses `default_transition` (mobile slide /
    /// desktop fade). The default mobile slide is expressed in fractions of
    /// the page's own laid-out size via `FractionalTranslation`, so it is
    /// correct at any window width without the builder needing to know the
    /// pixel width.
    pub fn transition<F: Fn(&TransitionCtx, Box<dyn Widget>) -> Box<dyn Widget> + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.transition = Some(Rc::new(f));
        self
    }

    /// Override the transition duration. Default: 350ms (mobile), 200ms
    /// (desktop).
    pub fn transition_duration(mut self, duration: Duration) -> Self {
        self.transition_duration = duration;
        self
    }

    /// Override the transition curve. Default: cubic-bezier(0.33, 1, 0.68, 1)
    /// (iOS-style ease-out-cubic).
    pub fn transition_curve(mut self, curve: impl Curve + 'static) -> Self {
        self.transition_curve = Rc::new(curve);
        self
    }

    /// Resolve the effective platform (explicit override or `Platform::current()`).
    fn effective_platform(&self) -> Platform {
        self.platform.unwrap_or_else(Platform::current)
    }

    /// Resolve the effective transition duration (mobile/desktop default if
    /// not explicitly overridden).
    fn effective_transition_duration(&self) -> Duration {
        // If the caller set a non-default duration, respect it.
        if self.transition_duration != DEFAULT_MOBILE_TRANSITION_DURATION {
            return self.transition_duration;
        }
        // Otherwise pick the platform default.
        match self.effective_platform() {
            Platform::Mobile => DEFAULT_MOBILE_TRANSITION_DURATION,
            Platform::Desktop => DEFAULT_DESKTOP_TRANSITION_DURATION,
        }
    }
}

/// In-flight transition state held in `NavigationStackViewState`.
struct NavTransition<Dest: Hash + Eq + Clone + 'static> {
    direction: TransitionDir,
    controller: AnimationController,
    from_path: Vec<Dest>,
    to_path: Vec<Dest>,
}

/// In-flight interactive (gesture-driven) pop. Lives behind an
/// `Rc<RefCell<Option<InteractivePop>>>` on `NavigationStackViewState` so the
/// gesture closures (built in `render`, fired outside `render`) can mutate it.
/// Mirrors `ContextMenuState`'s shared `Rc<RefCell<...>>` pattern.
pub struct InteractivePop<Dest: Hash + Eq + Clone + 'static> {
    pub controller: AnimationController,
    pub from_path: Vec<Dest>,
    pub to_path: Vec<Dest>,
    pub phase: InteractivePopPhase,
    pub velocity_tracker: VelocityTracker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractivePopPhase {
    Dragging,
    Committing,
    Cancelling,
}

/// Release past 50% progress commits; below cancels. A rightward flick above
/// this velocity also commits even if progress < 50%.
const FLICK_THRESHOLD: f32 = 0.5;

/// State for the NavigationStackView component.
///
/// Owns the in-flight transition's `AnimationController`. The controller lives
/// in the state (not on the widget) because it persists across rebuilds and
/// must be advanced by `on_tick`.
///
/// The state also caches the `AnimationTicker` and dirty callback (wired in
/// `on_mount`) so that `render` can construct and start a transition
/// controller when it observes a pending op from the navigation controller.
pub struct NavigationStackViewState<Dest: Hash + Eq + Clone + 'static> {
    _marker: PhantomData<Dest>,
    transition: Option<NavTransition<Dest>>,
    /// Shared cell holding the in-flight interactive pop. `Rc<RefCell<...>>` so
    /// the gesture closures (built in `render`, fired outside `render`) can
    /// mutate it. Mirrors `ContextMenuState`'s shared-cell pattern.
    pub interactive_pop: Rc<RefCell<Option<InteractivePop<Dest>>>>,
    /// Cached content width from the last `render()`. Read by gesture closures
    /// to convert finger delta_x → progress (0..1).
    content_width: f32,
    /// Cached ticker from `on_mount`. Used to wire transition controllers.
    pub ticker: Option<Arc<vexo::AnimationTicker>>,
    /// Cached dirty callback from `on_mount`. Used to wire transition controllers.
    pub dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}

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

impl<Dest: Hash + Eq + Clone + 'static> ComponentState for NavigationStackViewState<Dest> {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
            nav.controller.set_dirty_callback(ctx.dirty_callback());
        }
        // Cache the ticker and dirty callback so render() can wire a
        // transition controller when a pending op is observed.
        self.ticker = Some(ctx.animation_ticker().clone());
        self.dirty_callback = Some(ctx.dirty_callback());
    }

    fn on_update(&mut self, old_widget: &dyn Any, ctx: &mut LifecycleContext) {
        let old = old_widget.downcast_ref::<NavigationStackView<Dest>>();
        let new = ctx.widget().downcast_ref::<NavigationStackView<Dest>>();
        if let (Some(old), Some(new)) = (old, new) {
            // Re-wire only if the controller instance changed. Identity is
            // determined by Rc::ptr_eq on the shared path cell.
            if !Rc::ptr_eq(&old.controller.path, &new.controller.path) {
                old.controller.clear_dirty_callback();
                new.controller.set_dirty_callback(ctx.dirty_callback());
            }
        }
        // Refresh cached wiring in case the element ID changed.
        self.ticker = Some(ctx.animation_ticker().clone());
        self.dirty_callback = Some(ctx.dirty_callback());
    }

    fn on_unmount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
            nav.controller.clear_dirty_callback();
        }
        // Stop any in-flight transition controller to unregister from the ticker.
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

    fn on_tick(&mut self, now: Instant) {
        if let Some(t) = self.transition.as_mut() {
            t.controller.advance(now);
            // Completion is detected in render() (which has access to the
            // navigation controller to clear its pending op). Here we just
            // advance the controller.
        }
        if let Some(ip) = self.interactive_pop.borrow_mut().as_mut() {
            ip.controller.advance(now);
        }
    }

    fn on_rebuild(&mut self, ctx: &mut LifecycleContext) {
        // Was: ctx.clear_focus() inside render() when a pending op was
        // observed. Now: same check, same call, but in the lifecycle hook —
        // render() stays pure.
        //
        // A navigation transition is starting (push or pop). Clear primary
        // focus now, on the same frame the animation begins, rather than
        // letting it linger on the outgoing page.
        //
        // Why this matters: on iOS, a TextEdit holding focus keeps the
        // software keyboard up. Without this call, tapping Back on a
        // focused chat screen would leave the keyboard stuck on screen
        // for the entire pop animation (and beyond), because the outgoing
        // page stays mounted as the transition overlay and retains focus
        // until it unmounts at the end — and even then nothing re-synced
        // the keyboard.
        //
        // `clear_focus()` is deferred (stashed on BuildOwner, applied after
        // this rebuild pass), and `FocusManager::unfocus()` is a no-op when
        // nothing is focused, so this is harmless for pushes from an
        // unfocused list.
        if self.transition.is_none() && self.interactive_pop.borrow().is_none() {
            if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
                if nav.controller.pending().is_some() {
                    ctx.clear_focus();
                }
            }
        }
    }
}

impl<Dest: Hash + Eq + Clone + 'static> Component for NavigationStackView<Dest> {
    type State = NavigationStackViewState<Dest>;

    /// Level 3 rebuild-skip (see `docs/rebuild-skipping-patterns.md`).
    /// During keyboard animation, the parent cascades `update()` to us with
    /// fresh closures but the controller's path hasn't changed. Comparing
    /// observable controller state stops the cascade before it rebuilds the
    /// entire page stack. Note: state-driven rebuilds (Signal, MediaQuery
    /// invalidation) bypass this hook — those still re-render, which is why
    /// rotation and safe-area changes still work.
    fn should_rebuild(&self, old: &Self) -> bool {
        self.controller.path() != old.controller.path() || self.controller.pending().is_some()
    }

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        // 1. Check for a pending op from the controller. If present and we
        //    don't yet have a transition, start one — but only if the state
        //    has a ticker (i.e., on_mount was called). Without a ticker, the
        //    transition can't run, so we clear the pending op immediately and
        //    fall through to steady-state rendering (a hard swap).
        if state.transition.is_none() {
            if let Some(pending) = self.controller.pending() {
                if let (Some(ticker), Some(cb)) =
                    (state.ticker.as_ref(), state.dirty_callback.as_ref())
                {
                    let duration = self.effective_transition_duration();
                    let mut controller = AnimationController::new(duration);
                    controller.set_ticker(ticker.clone());
                    controller.set_dirty_callback(cb.clone());
                    controller.forward();
                    state.transition = Some(NavTransition {
                        direction: pending.kind,
                        controller,
                        from_path: pending.from.clone(),
                        to_path: pending.to.clone(),
                    });
                } else {
                    // No ticker available (test harness or pre-mount). Clear
                    // the pending op and render steady-state.
                    self.controller.clear_pending();
                }
            }
        }

        // 2. If a transition is in flight, check if it has completed.
        let transition_completed = if let Some(t) = state.transition.as_ref() {
            t.controller.direction() == vexo::AnimationDirection::Stopped
                && t.controller.value() >= 1.0
        } else {
            false
        };
        if transition_completed {
            self.controller.clear_pending();
            state.transition = None;
        }

        // 2b. If an interactive pop is in flight, check if its spring has
        //     settled (phase != Dragging and controller stopped). On settle:
        //     commit or cancel the pop on the controller, clear the cell, and
        //     fire dirty to trigger a steady-state re-render.
        {
            let ip_cell = state.interactive_pop.borrow_mut();
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
                    *state.interactive_pop.borrow_mut() = None;
                    // Fire dirty to re-render steady state (cancel doesn't fire
                    // dirty itself; commit does but a redundant fire is idempotent).
                    if let Some(cb) = &state.dirty_callback {
                        cb();
                    }
                }
            }
        }

        // 3. Determine the "current" path for nav-bar title / can_pop.
        //    During transition, use the to_path (the destination) so the
        //    nav bar reflects where the user is going.
        let (title, can_pop) = if let Some(t) = state.transition.as_ref() {
            // Use to_path's top for the title.
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

        let mq = MediaQuery::of(ctx);
        state.content_width = mq.size.width;
        let safe_insets = mq.padding;
        let nav = tokens::navigation::colors(&Theme::of(ctx));
        let nav_bar = self.build_nav_bar(&title, can_pop, &safe_insets, &nav);

        // 4. Build the page content area.
        //
        // The content widget is ALWAYS a `Stack` (stable type), regardless of
        // whether a transition is in flight. This is critical: if the widget
        // type alternated between `IndexedStack` (steady) and `Stack`
        // (transition), the reconciler's `can_update()` (which checks
        // `type_id()` equality) would fail on the type swap and replace the
        // entire subtree — unmounting every page element (and its state, e.g.
        // `TextEditingController` edits). On return to steady state, fresh
        // page elements would mount with default state, losing user input.
        //
        // The base `IndexedStack` is ALWAYS the Stack's first (in-flow) child
        // (optionally wrapped in an `Opacity` during transitions — Opacity is
        // layout pass-through, so it preserves the in-flow sizing and the
        // child element subtree). Because the slot-0 widget type never
        // changes, the reconciler updates it in place; the `Offstage`-wrapped
        // page children inside it stay mounted across push/pop, preserving
        // their state.
        //
        // The base IndexedStack always shows the page that STAYS PUT during
        // the animation — the "underneath" page. Only the transient "moving"
        // page is rendered in the overlay (a `Positioned` sibling), which is
        // inflated when a transition starts and unmounted when it ends.
        //
        //   Steady   : base index = path.len() (current top), no overlay,
        //              Opacity alpha = 1.0.
        //   Push     : base index = from_path.len() (old top, preserved state),
        //              base Opacity alpha 1 → 0.85 so it dims slightly as the
        //              incoming page slides over it (mitigates text bleed-through
        //              when the new page's background is transparent). Overlay = incoming page.
        //   Pop      : base index = to_path.len() (new top = path.len(), the
        //              destination we're popping back to, preserved state),
        //              base Opacity alpha 0.85 → 1 (it's the incoming page,
        //              un-dimming as the outgoing page slides away).
        //              Overlay = outgoing page animating out, revealing base.
        //
        // This split is what preserves state across navigation: the
        // destination page on pop is the base IndexedStack's already-mounted
        // child (with its edits intact), never a fresh overlay page.
        let path = self.controller.path();

        // The base index: point at the "underneath" page.
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

        let mut base_stack = IndexedStack::new(base_index);
        base_stack = base_stack.push(self.root.clone_boxed());
        for dest in path.iter() {
            base_stack = base_stack.push((self.destination)(dest));
        }

        // The base is ALWAYS wrapped in an `Opacity(FractionalTranslation(...))`
        // (stable widget types), even in steady state. This is critical for the
        // same reason the outer `Stack` is always a `Stack`: if the base widget
        // type flipped between bare `IndexedStack` (steady) and
        // `Opacity(FractionalTranslation(IndexedStack))` (transition), the
        // reconciler's `can_update()` (type-based) would replace the subtree on
        // the swap, unmounting the page elements and losing their state (e.g.
        // TextEditingController edits).
        //
        // `Opacity` and `FractionalTranslation` are both layout pass-through and
        // preserve their child element across changes, so wrapping is safe. At
        // steady state `base_fx = 0.0` makes `FractionalTranslation` a paint-time
        // no-op (`paint_transform()` returns `None`) — zero rendering cost.
        //
        // `Opacity` renders its subtree to an offscreen SaveLayer group and
        // composites the group at the given alpha, preserving internal paint
        // order (background → text → dim). This fixes the white-rectangle bug
        // where CPU alpha-multiplication dropped the page background's alpha
        // below 1.0, reclassifying it as a transparent quad (Phase 3, after
        // text) and causing light text to render on the window's white clear
        // color before the dark background was composited.
        //
        // Offset/alpha rules (SwiftUI-style dual-view animation on mobile;
        // fade-only on desktop):
        //   Push (mobile) : base (old top) slides left 30%, dims 1.0 → 0.85.
        //   Pop  (mobile) : base (destination) slides back to 0, un-dims 0.85 → 1.0.
        //   Desktop       : base_fx = 0.0 always (no slide); alpha fades as before.
        //   Steady        : base_fx = 0.0, alpha = 1.0 (no-op wrappers).
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
        let base_widget: Box<dyn Widget> = Opacity::new(
            FractionalTranslation::new(base_stack, base_fx, 0.0),
            base_alpha,
        )
        .boxed();

        // The base is an IN-FLOW child of the Stack (not Positioned). The
        // Stack's layout is `flex_direction: Column, align: Stretch,
        // width_percent(1.0), height_percent(1.0)`, so the base
        // (Opacity is layout pass-through → IndexedStack fills the Stack)
        // occupies the content area.
        let mut content_stack = Stack::new().push(base_widget);

        if let Some(t) = state.transition.as_ref() {
            let raw_t = t.controller.value();
            let eased = self.transition_curve.transform(raw_t);

            let platform = self.effective_platform();

            let transition_fn: Rc<dyn Fn(&TransitionCtx, Box<dyn Widget>) -> Box<dyn Widget>> =
                self.transition
                    .clone()
                    .unwrap_or_else(|| Rc::new(|ctx, child| default_transition(ctx, child)));

            // The overlay renders only the "moving" page. The underneath page
            // is already shown by the base IndexedStack (with preserved state).
            let overlay: Box<dyn Widget> = match t.direction {
                TransitionDir::Push => {
                    // Incoming page slides in over the (still-mounted) old top.
                    let incoming_page = self.build_page_for_path_top(&t.to_path);
                    let incoming_ctx = TransitionCtx {
                        t: eased,
                        is_incoming: true,
                        direction: t.direction,
                        platform,
                    };
                    transition_fn(&incoming_ctx, incoming_page)
                }
                TransitionDir::Pop | TransitionDir::PopToRoot => {
                    // Outgoing page slides away, revealing the destination
                    // (the base IndexedStack's new top) underneath.
                    let outgoing_page = self.build_page_for_path_top(&t.from_path);
                    let outgoing_ctx = TransitionCtx {
                        t: eased,
                        is_incoming: false,
                        direction: t.direction,
                        platform,
                    };
                    transition_fn(&outgoing_ctx, outgoing_page)
                }
            };

            content_stack = content_stack.push(
                Positioned::new(overlay)
                    .top(0.0)
                    .right(0.0)
                    .bottom(0.0)
                    .left(0.0),
            );
        }

        if state.transition.is_none() {
            if let Some(ip) = state.interactive_pop.borrow().as_ref() {
                let raw_t = ip.controller.value();
                let eased = self.transition_curve.transform(raw_t);
                let platform = self.effective_platform();

                let transition_fn: Rc<dyn Fn(&TransitionCtx, Box<dyn Widget>) -> Box<dyn Widget>> =
                    self.transition
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

        // Wrap the content `Stack` in a clipping `DecoratedBox` so the
        // moving page's full-perimeter shadow (attached in
        // `default_mobile_transition`) is clipped to the nav content area —
        // only the leading-edge strip is visible, matching iOS native. Also
        // fixes a latent bleed bug where the sliding overlay's `Positioned`
        // page could paint outside the nav stack bounds.
        //
        // The wrapper is ALWAYS present (steady + transition, all platforms)
        // for type-stability: if the type flipped between bare `Stack`
        // (steady) and `DecoratedBox(Stack)` (transition), the
        // reconciler would remount the subtree and lose page state. At steady
        // state the base page fills the content area exactly, so the clip is
        // a cheap no-op scissor.
        //
        // The composition is `DecoratedBox(WithLayout(content_stack))` with
        // `width_percent(1.0).height_percent(1.0)` so it fills the SafeArea
        // exactly — otherwise the content overflows past the tab bar.
        let clipped: Box<dyn Widget> = DecoratedBox::with_style(
            WithLayout::new(
                content_stack,
                Layout::default().width_percent(1.0).height_percent(1.0),
            ),
            Style::default().clip(),
        )
        .boxed();

        // The nav bar handles the top safe-area inset itself (background
        // extends under the status bar). The content area only needs
        // left/right/bottom insets — top is already consumed by the bar.
        let content = WithLayout::new(SafeArea::new(clipped).top(false), Layout::flex_fill());

        // flex_fill() fills the parent and prevents the column's content
        // (a tall scrollable page) from propagating its min-content upward.
        // Without this, a page taller than the available space (e.g. 8
        // contacts inside a TabBarView on a short window) pushes the tab bar
        // off screen. The page's own ScrollView handles the overflow.
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
            .on_start({
                let controller = controller.clone();
                let ticker = ticker.clone();
                let dirty_cb = dirty_cb.clone();
                let ip_cell = ip_cell.clone();
                move || {
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
                }
            })
            .on_update({
                let dirty_cb = dirty_cb.clone();
                let ip_cell = ip_cell.clone();
                move |delta_x| {
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
                }
            })
            .on_end({
                let dirty_cb = dirty_cb.clone();
                let ip_cell = ip_cell.clone();
                move |_final_delta_x| {
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
                }
            })
            .boxed()
    }
}

impl<Dest: Hash + Eq + Clone + 'static> NavigationStackView<Dest> {
    /// Build the page widget for the top of a given path snapshot.
    /// If the path is empty, returns the root widget.
    fn build_page_for_path_top(&self, path: &[Dest]) -> Box<dyn Widget> {
        if let Some(top) = path.last() {
            (self.destination)(top)
        } else {
            self.root.clone_boxed()
        }
    }

    /// Build the NavBar chrome: title text + optional back button.
    ///
    /// The title is centered in the *full* bar width (iOS `UINavigationBar`
    /// behavior), regardless of whether a back button is present. This is
    /// achieved with a `Stack` of two layers:
    ///
    /// - **Title layer (in-flow, bottom of stack)**: a full-width row with
    ///   `JustifyContent::Center` + `AlignItems::Center`, so the title's
    ///   center is always the bar's center.
    /// - **Back-button layer (`Positioned` overlay, on top, only when
    ///   `can_pop`)**: the back button at its intrinsic width, anchored to
    ///   the leading edge (left = safe-area inset + header padding) and
    ///   vertically centered. Because it is absolutely positioned, it does
    ///   not participate in the title row's flex layout, so the title stays
    ///   centered in the full bar.
    ///
    /// Tradeoff: a very long title may overlap the back button. This matches
    /// iOS `UINavigationBar` default behavior; callers wanting gap-aware
    /// centering should truncate/ellipsize the title upstream.
    ///
    /// # Safe-area handling (matches Flutter `AppBar`)
    ///
    /// The bar's background extends edge-to-edge (no outer horizontal
    /// padding), and the bar height is increased by the top safe-area inset
    /// so the background covers the status bar. The left/right safe-area
    /// insets are applied as the back button's leading offset (and an
    /// equivalent right padding keeps the title row symmetric) so the back
    /// button doesn't tuck under the notch.
    fn build_nav_bar(
        &self,
        title: &str,
        can_pop: bool,
        safe: &vexo::layout::EdgeInsets,
        nav: &NavColors,
    ) -> Box<dyn Widget> {
        let title_text = Text::new(title)
            .with_font_size(tokens::navigation::MOBILE_TITLE_FONT_SIZE)
            .with_color(nav.mobile_title);

        let h_pad = tokens::navigation::MOBILE_HEADER_PADDING;

        // Title layer: centered in the full bar width. Padded left/right by
        // the safe-area inset + header padding so a long title doesn't tuck
        // under the notch (or the back button) at the trailing edge — but
        // the row itself still centers within the padded box, so for titles
        // that fit, centering equals full-bar center (the symmetric padding
        // cancels out).
        let title_row = MultiChild::new(
            children![title_text],
            Layout::row()
                .align(AlignItems::Center)
                .justify(JustifyContent::Center)
                .padding_each(safe.left + h_pad, safe.right + h_pad, 0.0, 0.0)
                .width_percent(1.0)
                .height_percent(1.0),
        );

        let mut content_stack = Stack::new().push(title_row);

        // Back-button overlay: absolutely positioned at the leading edge,
        // vertically centered, intrinsic width. Does not affect the title
        // row's layout.
        //
        // The back button is composed manually (Icon + Text in a row) rather
        // than using the `Button` widget because `Button` only accepts a
        // `String` label and renders it with the default font family, so a
        // FontAwesome icon codepoint would not shape correctly. The manual
        // composition uses `Icon` (which sets the FontAwesome font family) for
        // the chevron and `Text` for the "Back" label, both colored with
        // `nav.back_color`.
        //
        // The composition is wrapped in a *Column* with
        // `JustifyContent::Center` (main-axis centering) and a definite
        // height equal to the bar's content area (`MOBILE_HEADER_HEIGHT`),
        // so the button is vertically centered within the bar regardless of
        // how Taffy resolves percentage heights inside the absolute
        // `Positioned` container.
        if can_pop {
            let controller = self.controller.clone();
            let back_color = nav.back_color;
            let back_icon = Icon::new(Icons::ChevronLeft)
                .with_size(tokens::navigation::BACK_ICON_SIZE)
                .with_color(back_color);
            let back_label_text = Text::new(tokens::navigation::BACK_LABEL)
                .with_font_size(tokens::navigation::BACK_FONT_SIZE)
                .with_color(back_color);
            let back_row = MultiChild::new(
                children![back_icon, back_label_text],
                Layout::row()
                    .align(AlignItems::Center)
                    .gap(tokens::navigation::BACK_ICON_LABEL_GAP)
                    .flex_shrink(0.0),
            );
            let back_layer = MultiChild::new(
                children![back_row.on_tap(move || {
                    controller.pop();
                })],
                Layout::column()
                    .justify(JustifyContent::Center)
                    .height(tokens::navigation::MOBILE_HEADER_HEIGHT),
            );
            content_stack = content_stack.push(
                Positioned::new(back_layer)
                    .top(0.0)
                    .left(safe.left + h_pad)
                    .bottom(0.0),
            );
        }

        // Outer bar: background edge-to-edge, height includes top inset.
        // The Stack fills the WithLayout's content box (width 100%, height
        // = MOBILE_HEADER_HEIGHT + safe.top), and the top safe-area inset
        // is applied as padding so content sits below the status bar while
        // the background extends under it.
        let bar_row = DecoratedBox::with_style(
            WithLayout::new(
                content_stack,
                Layout::default()
                    .padding_each(0.0, 0.0, safe.top, 0.0)
                    .height(tokens::navigation::MOBILE_HEADER_HEIGHT + safe.top)
                    .flex_shrink(0.0),
            ),
            Style::default().background(nav.mobile_header_bg),
        );

        // SwiftUI-style hairline along the bar's bottom edge. 1 logical pixel
        // (Taffy floors sub-pixel heights to 0, so a true 1-physical-px
        // `1/scale` height would vanish on Retina). 1 logical px renders as 1
        // physical px at 1× and 2 at 2× — matching macOS `Divider`.
        let hairline = DecoratedBox::with_style(
            MultiChild::empty(
                Layout::row()
                    .height(tokens::navigation::HAIRLINE_THICKNESS)
                    .flex_shrink(0.0),
            ),
            Style::default().background(nav.divider),
        );

        // Wrap bar + hairline in a fixed-height column so the outer
        // NavigationStackView column keeps a single nav-bar child; the
        // hairline sits flush against the bar's bottom edge, full width.
        MultiChild::new(
            children![bar_row, hairline],
            Layout::column().flex_shrink(0.0),
        )
        .boxed()
    }
}

/// Compute the base (underneath) page's fractional offset and alpha for a
/// navigation transition.
///
/// On mobile, the underneath page slides left ~30% and dims to 0.85 alpha
/// (SwiftUI-style dual-view offset animation). On desktop, it fades in place
/// (no offset — desktop has no stack metaphor).
///
/// - Push: base is the outgoing (old top) page — slides left, dims 1.0 → 0.85.
/// - Pop/PopToRoot: base is the incoming (destination) page — slides back to
///   0, un-dims 0.85 → 1.0.
///
/// Returns `(base_fx, base_alpha)`. `base_fx` is the fractional horizontal
/// offset (negative = left, resolved against page width at paint time).
/// `base_alpha` is the opacity multiplier `0.0..=1.0`.
pub fn base_fx_alpha(direction: TransitionDir, platform: Platform, eased: f64) -> (f32, f32) {
    match (direction, platform) {
        (TransitionDir::Push, Platform::Mobile) => {
            ((-0.3 * eased) as f32, (1.0 - 0.15 * eased) as f32)
        }
        (TransitionDir::Pop | TransitionDir::PopToRoot, Platform::Mobile) => {
            ((-0.3 * (1.0 - eased)) as f32, (0.85 + 0.15 * eased) as f32)
        }
        (TransitionDir::Push, Platform::Desktop) => (0.0, (1.0 - eased) as f32),
        (TransitionDir::Pop | TransitionDir::PopToRoot, Platform::Desktop) => (0.0, eased as f32),
    }
}
