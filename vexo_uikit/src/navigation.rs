//! NavigationStackView component — a stack navigator with a caller-owned
//! `NavigationController<Dest>`.
//!
//! Provides a SwiftUI `NavigationStack`-style LIFO stack: a root page plus
//! pushed pages. The caller owns the controller and mutates the path via
//! `push`/`pop`/`pop_to_root`/`replace`; the controller's dirty callback
//! (wired during mount) triggers rebuilds so the view always reflects the
//! current top-of-stack.
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

use vexo::{
    AlignItems, Component, ComponentState, Flex, IndexedStack, LifecycleContext, RenderContext,
    Text, Widget,
};

use crate::button::{Button, ButtonVariant};
use crate::platform::Platform;
use crate::theme::tokens;

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
pub struct NavigationController<Dest: Hash + Eq + Clone + 'static> {
    path: Rc<RefCell<Vec<Dest>>>,
    dirty_callback: Rc<RefCell<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl<Dest: Hash + Eq + Clone + 'static> NavigationController<Dest> {
    /// Create a new controller with an empty path (at root).
    pub fn new() -> Self {
        Self {
            path: Rc::new(RefCell::new(Vec::new())),
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

    /// Push a new destination onto the stack. The next render shows its page.
    pub fn push(&self, dest: Dest) {
        self.path.borrow_mut().push(dest);
        self.notify();
    }

    /// Pop the top destination. No-op at root (returns `None`).
    /// Returns the popped value when the path was non-empty.
    pub fn pop(&self) -> Option<Dest> {
        let popped = self.path.borrow_mut().pop();
        if popped.is_some() {
            self.notify();
        }
        popped
    }

    /// Clear the entire path, returning to root. Idempotent: a no-op (and no
    /// dirty fire) if the path is already empty.
    pub fn pop_to_root(&self) {
        let mut p = self.path.borrow_mut();
        if p.is_empty() {
            return;
        }
        p.clear();
        drop(p);
        self.notify();
    }

    /// Replace the top of the stack with `dest`. At root (empty path), behaves
    /// as `push(dest)` — documented, not an error.
    pub fn replace(&self, dest: Dest) {
        let mut p = self.path.borrow_mut();
        if let Some(top) = p.last_mut() {
            *top = dest;
        } else {
            p.push(dest);
        }
        drop(p);
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
pub struct NavigationStackView<Dest: Hash + Eq + Clone + 'static> {
    controller: NavigationController<Dest>,
    root: Box<dyn Widget>,
    destination: Rc<dyn Fn(&Dest) -> Box<dyn Widget>>,
    title: Rc<dyn Fn(&Dest) -> String>,
    root_title: Option<String>,
    platform: Option<Platform>,
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
        }
    }
}

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

    /// Override the platform. Currently a no-op (rendering is identical on all
    /// platforms); reserved for future desktop adaptation.
    pub fn platform(mut self, p: Platform) -> Self {
        self.platform = Some(p);
        self
    }
}

/// State for the NavigationStackView component.
///
/// Has NO controller field. The controller lives on the widget (caller-supplied).
/// Lifecycle hooks read it off `ctx.widget()` and wire/unwire its dirty callback
/// — exactly like `TextEditState`. The state exists only to host the lifecycle
/// hooks; `set_dirty_callback` is a no-op (no state-owned Signals).
pub struct NavigationStackViewState<Dest: Hash + Eq + Clone + 'static> {
    _marker: PhantomData<Dest>,
}

impl<Dest: Hash + Eq + Clone + 'static> Default for NavigationStackViewState<Dest> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<Dest: Hash + Eq + Clone + 'static> ComponentState for NavigationStackViewState<Dest> {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
            nav.controller.set_dirty_callback(ctx.dirty_callback());
        }
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
    }

    fn on_unmount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(nav) = ctx.widget().downcast_ref::<NavigationStackView<Dest>>() {
            nav.controller.clear_dirty_callback();
        }
    }
}

impl<Dest: Hash + Eq + Clone + 'static> Component for NavigationStackView<Dest> {
    type State = NavigationStackViewState<Dest>;

    fn render(&self, _state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let path = self.controller.path();
        let (title, can_pop) = if let Some(top) = path.last() {
            ((self.title)(top), true)
        } else {
            (self.root_title.clone().unwrap_or_default(), false)
        };

        let nav_bar = self.build_nav_bar(&title, can_pop);

        // Build the full page stack so all pages stay mounted (state preserved
        // across push/pop). IndexedStack shows only the top page; the rest are
        // kept offstage via Offstage, so their elements — and thus their state
        // (ComponentState, focus, TextEditingControllers) — persist.
        //
        // Index 0 = root; each pushed dest is index 1..=depth. The top is
        // `path.len()` (i.e. the last pushed dest, or 0 when at root).
        let mut stack = IndexedStack::new(path.len());
        stack = stack.push(self.root.clone_boxed());
        for dest in path.iter() {
            stack = stack.push((self.destination)(dest));
        }

        Flex::column().push(nav_bar).push(stack.boxed()).boxed()
    }
}

impl<Dest: Hash + Eq + Clone + 'static> NavigationStackView<Dest> {
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
