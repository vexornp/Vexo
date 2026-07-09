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
//! For a two-column sidebar+detail layout, compose manually with `Flex::row`
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

use vexo::{
    AlignItems, AnimationController, Component, ComponentState, Curve, EaseInOutCurve, Flex,
    IndexedStack, LifecycleContext, Opacity, Positioned, RenderContext, Stack, Text, Widget,
};

use crate::button::{Button, ButtonVariant};
use crate::platform::Platform;
use crate::theme::tokens;
use crate::transitions::{default_transition, TransitionCtx, TransitionDir};

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
    transition_curve: Box<dyn Curve>,
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
            transition_curve: Box::new(EaseInOutCurve),
        }
    }
}

/// Default transition duration on mobile (iOS convention).
const DEFAULT_MOBILE_TRANSITION_DURATION: Duration = Duration::from_millis(300);
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
            transition_curve: Box::new(EaseInOutCurve),
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
    /// `TransitionCtx` (with the eased `t`, direction, platform, and cached
    /// `page_width`) and the page widget, and returns the page wrapped in
    /// transform/opacity widgets as desired.
    ///
    /// When `None`, the view uses `default_transition` (mobile slide /
    /// desktop fade).
    pub fn transition<F: Fn(&TransitionCtx, Box<dyn Widget>) -> Box<dyn Widget> + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.transition = Some(Rc::new(f));
        self
    }

    /// Override the transition duration. Default: 300ms (mobile), 200ms
    /// (desktop).
    pub fn transition_duration(mut self, duration: Duration) -> Self {
        self.transition_duration = duration;
        self
    }

    /// Override the transition curve. Default: `EaseInOutCurve`.
    pub fn transition_curve(mut self, curve: Box<dyn Curve>) -> Self {
        self.transition_curve = curve;
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
    /// Cached page width in logical pixels. Read from the nav content area's
    /// render object bounds; falls back to `TransitionCtx::DEFAULT_PAGE_WIDTH`
    /// on the first frame before layout has run.
    page_width: f32,
}

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
    /// Cached ticker from `on_mount`. Used to wire transition controllers.
    ticker: Option<Arc<vexo::AnimationTicker>>,
    /// Cached dirty callback from `on_mount`. Used to wire transition controllers.
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl<Dest: Hash + Eq + Clone + 'static> Default for NavigationStackViewState<Dest> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
            transition: None,
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
    }
}

impl<Dest: Hash + Eq + Clone + 'static> Component for NavigationStackView<Dest> {
    type State = NavigationStackViewState<Dest>;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
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
                        page_width: TransitionCtx::DEFAULT_PAGE_WIDTH,
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
        } else {
            let path = self.controller.path();
            if let Some(top) = path.last() {
                ((self.title)(top), true)
            } else {
                (self.root_title.clone().unwrap_or_default(), false)
            }
        };

        let nav_bar = self.build_nav_bar(&title, can_pop);

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
        //              base Opacity alpha 1 → 0 so it fades out as the
        //              incoming page slides over it (prevents the old page's
        //              text from showing through the new page's transparent
        //              background during the overlap). Overlay = incoming page.
        //   Pop      : base index = to_path.len() (new top = path.len(), the
        //              destination we're popping back to, preserved state),
        //              base Opacity alpha 0 → 1 (it's the incoming page,
        //              fading in as the outgoing page slides away).
        //              Overlay = outgoing page animating out, revealing base.
        //
        // This split is what preserves state across navigation: the
        // destination page on pop is the base IndexedStack's already-mounted
        // child (with its edits intact), never a fresh overlay page.
        let path = self.controller.path();

        // The base index: point at the "underneath" page.
        let base_index = match state.transition.as_ref() {
            None => path.len(),
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

        // The base is ALWAYS wrapped in an `Opacity` (stable widget type),
        // even in steady state. This is critical for the same reason the
        // outer `Stack` is always a `Stack`: if the base widget type flipped
        // between bare `IndexedStack` (steady) and `Opacity(IndexedStack)`
        // (transition), the reconciler's `can_update()` (type-based) would
        // replace the subtree on the swap, unmounting the page elements and
        // losing their state (e.g. TextEditingController edits).
        //
        // `Opacity` is layout pass-through and preserves its child element
        // across opacity changes (like `Offstage`), so wrapping is safe.
        //
        // Alpha rules (mirror the original two-page transition's incoming/
        // outgoing curves so the visual matches):
        //   Push         : base (old top) is the OUTGOING page — fades 1 → 0
        //                  as the incoming page slides over it. Prevents the
        //                  old page's text showing through the new page's
        //                  transparent background. At t=1 the base is
        //                  invisible, so the hard cut to steady state (base
        //                  becomes the new top at full opacity) shows no jump.
        //   Pop/PopToRoot: base (destination) is the INCOMING page — fades
        //                  0 → 1 as the outgoing page slides away. At t=0 the
        //                  base is invisible so the outgoing page (at center,
        //                  full opacity) is the only thing visible — no overlap
        //                  with the destination's text through the outgoing
        //                  page's transparent background.
        //   Steady       : full opacity (1.0).
        let base_alpha = match state.transition.as_ref() {
            None => 1.0,
            Some(t) => {
                let raw_t = t.controller.value();
                let eased = self.transition_curve.transform(raw_t);
                match t.direction {
                    TransitionDir::Push => 1.0 - eased as f32,
                    TransitionDir::Pop | TransitionDir::PopToRoot => eased as f32,
                }
            }
        };
        let base_widget: Box<dyn Widget> = Opacity::new(base_stack, base_alpha).boxed();

        // The base is an IN-FLOW child of the Stack (not Positioned). The
        // Stack's layout is `flex_direction: Column, align: Stretch,
        // width_percent(1.0), height_percent(1.0)`, so the base
        // (Opacity is layout pass-through → IndexedStack fills the Stack)
        // occupies the content area. The overlay, when present, is a
        // `Positioned` (absolute) sibling painted on top — it does not affect
        // the base's in-flow layout.
        let mut content_stack = Stack::new().push(base_widget);

        if let Some(t) = state.transition.as_ref() {
            let raw_t = t.controller.value();
            let eased = self.transition_curve.transform(raw_t);

            let platform = self.effective_platform();
            let page_width = t.page_width;

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
                        page_width,
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
                        page_width,
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

        let content: Box<dyn Widget> = content_stack.boxed();

        Flex::column().push(nav_bar).push(content).boxed()
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
    /// `can_pop == false` (at root) → no back button, title occupies the row.
    /// `can_pop == true` → back button on the left, title after it.
    fn build_nav_bar(&self, title: &str, can_pop: bool) -> Box<dyn Widget> {
        let mut row = Flex::row()
            .align(AlignItems::Center)
            .gap(8.0)
            .padding(tokens::navigation::MOBILE_HEADER_PADDING)
            .background(tokens::navigation::MOBILE_HEADER_BG)
            .height(tokens::navigation::MOBILE_HEADER_HEIGHT)
            .flex_shrink(0.0);

        if can_pop {
            let controller = self.controller.clone();
            let back_label = format!(
                "{} {}",
                tokens::navigation::BACK_CHEVRON,
                tokens::navigation::BACK_LABEL
            );
            let back_button = Button::new(back_label)
                .variant(ButtonVariant::Ghost)
                .on_press(move || {
                    controller.pop();
                })
                .boxed();
            row = row.push(back_button);
        }

        let title_text = Text::new(title)
            .with_font_size(tokens::navigation::MOBILE_TITLE_FONT_SIZE)
            .with_color(tokens::navigation::MOBILE_TITLE_COLOR);
        row = row.push(title_text);

        row.boxed()
    }
}
