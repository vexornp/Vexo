//! NavigationSplitView component — a two-column navigation layout.
//!
//! Provides a SwiftUI-style `NavigationSplitView` with a collapsible sidebar
//! and a detail pane. The sidebar holds a list of `NavigationItem`s; tapping
//! a row updates the selection and renders the corresponding detail content.
//!
//! # Example
//!
//! ```ignore
//! NavigationSplitView::new(items)
//!     .default_selection("inbox".to_string())
//!     .detail(|id| Column::new().push(Text::new(format!("Detail: {}", id))).boxed())
//!     .on_selection_change(|id| println!("Selected: {}", id))
//!     .boxed()
//! ```

use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;
use std::sync::Arc;

use vexo::layout::JustifyContent;
use vexo::{
    AlignItems, Component, ComponentState, DecoratedContainer, Flex, RenderContext, ScrollView,
    Signal, Text, Widget,
};

use crate::button::{Button, ButtonVariant};
use crate::platform::Platform;
use crate::theme::tokens;

/// Default no-op for `on_selection_change` — a named function so it satisfies
/// the higher-ranked `for<'a> Fn(&'a T)` bound (a closure `|_| {}` would not).
fn noop_change<T>(_: &T) {}

// ============================================================================
// NAVIGATION ITEM
// ============================================================================

/// A single item in the navigation sidebar.
///
/// Holds an identifier, a display label, and an optional leading icon widget.
pub struct NavigationItem<T> {
    pub id: T,
    pub label: String,
    pub icon: Option<Box<dyn Widget>>,
}

impl<T: Clone> Clone for NavigationItem<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            label: self.label.clone(),
            icon: self.icon.as_ref().map(|w| w.clone_boxed()),
        }
    }
}

impl<T: Clone> NavigationItem<T> {
    /// Create a new navigation item with an id and label.
    pub fn new(id: T, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            icon: None,
        }
    }

    /// Attach a leading icon widget to this item.
    pub fn icon(mut self, icon: impl Widget + 'static) -> Self {
        self.icon = Some(Box::new(icon));
        self
    }
}

// ============================================================================
// STATE
// ============================================================================

/// State for the NavigationSplitView component.
///
/// Tracks the current selection and sidebar collapse state via reactive Signals.
/// Because `T` is generic, `ComponentState` is implemented manually rather than
/// derived (the derive macro does not handle generic type parameters).
pub struct NavigationSplitViewState<T: 'static> {
    /// Currently selected item id, or `None` if nothing is selected.
    pub selected: Signal<Option<T>>,
    /// Whether the sidebar is collapsed to a thin strip.
    pub sidebar_collapsed: Signal<bool>,
    /// Whether the mobile detail page is currently shown (pushed) over the
    /// sidebar. Only consulted on `Platform::Mobile`. On desktop this is
    /// always ignored — the sidebar and detail render side-by-side.
    pub detail_visible: Signal<bool>,
}

impl<T: 'static> Default for NavigationSplitViewState<T> {
    fn default() -> Self {
        Self {
            selected: Signal::new(None),
            sidebar_collapsed: Signal::new(false),
            // Mobile starts on the sidebar; selecting an item pushes the detail.
            detail_visible: Signal::new(false),
        }
    }
}

impl<T: 'static> ComponentState for NavigationSplitViewState<T> {
    fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.selected.set_dirty_callback(callback.clone());
        self.sidebar_collapsed.set_dirty_callback(callback.clone());
        self.detail_visible.set_dirty_callback(callback);
    }
}

// ============================================================================
// NAVIGATION SPLIT VIEW WIDGET
// ============================================================================

/// A two-column navigation layout with a collapsible sidebar and detail pane.
///
/// Modeled on SwiftUI's `NavigationSplitView`. The sidebar holds a list of
/// `NavigationItem`s; the detail pane renders content for the current selection
/// via a user-supplied closure.
///
/// The widget is generic over the identifier type `T`, which must be
/// `Hash + Eq + Clone + 'static` (e.g. `String`, `u64`, or a custom enum).
pub struct NavigationSplitView<T: Hash + Eq + Clone + 'static> {
    items: Vec<NavigationItem<T>>,
    detail_builder: Rc<dyn Fn(&T) -> Box<dyn Widget>>,
    on_selection_change: Rc<dyn Fn(&T)>,
    preferred_sidebar_width: f32,
    default_selection: Option<T>,
    placeholder: Option<Box<dyn Widget>>,
    platform: Option<Platform>,
}

impl<T: Hash + Eq + Clone + 'static> Clone for NavigationSplitView<T> {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
            detail_builder: self.detail_builder.clone(),
            on_selection_change: self.on_selection_change.clone(),
            preferred_sidebar_width: self.preferred_sidebar_width,
            default_selection: self.default_selection.clone(),
            placeholder: self.placeholder.as_ref().map(|w| w.clone_boxed()),
            platform: self.platform,
        }
    }
}

impl<T: Hash + Eq + Clone + 'static> NavigationSplitView<T> {
    /// Create a new NavigationSplitView with the given sidebar items.
    pub fn new(items: Vec<NavigationItem<T>>) -> Self {
        Self {
            items,
            detail_builder: Rc::new(|_| Self::default_placeholder()),
            on_selection_change: Rc::new(noop_change::<T>),
            preferred_sidebar_width: tokens::navigation::SIDEBAR_WIDTH,
            default_selection: None,
            placeholder: None,
            platform: None,
        }
    }

    /// Provide a closure that builds the detail content for a given selection id.
    ///
    /// Called on every rebuild with the current selection. Returns a widget
    /// tree that fills the detail pane.
    pub fn detail<F: Fn(&T) -> Box<dyn Widget> + 'static>(mut self, f: F) -> Self {
        self.detail_builder = Rc::new(f);
        self
    }

    /// Set a callback invoked whenever the user selects a sidebar item.
    ///
    /// The callback receives the newly selected id by reference. It is NOT
    /// fired for `default_selection` (which is a display-only initial value).
    ///
    /// The callback is `Fn` (not `FnMut`) — use interior mutability (e.g.
    /// `Signal::set`, `Rc<RefCell<...>>`) if you need to mutate captured state.
    pub fn on_selection_change<F: Fn(&T) + 'static>(mut self, f: F) -> Self {
        self.on_selection_change = Rc::new(f);
        self
    }

    /// Set the preferred width of the expanded sidebar (default 240.0).
    pub fn preferred_sidebar_width(mut self, width: f32) -> Self {
        self.preferred_sidebar_width = width;
        self
    }

    /// Set the initial selection used for display before the user picks an item.
    ///
    /// The sidebar highlights this row and the detail pane shows its content,
    /// but `on_selection_change` is not fired for the default. If the user
    /// selects a different row, the signal takes over permanently.
    pub fn default_selection(mut self, id: T) -> Self {
        self.default_selection = Some(id);
        self
    }

    /// Override the placeholder shown when no item is selected.
    pub fn placeholder(mut self, widget: Box<dyn Widget>) -> Self {
        self.placeholder = Some(widget);
        self
    }

    /// Override the platform for this view.
    ///
    /// If not set, uses `Platform::current()`. On `Platform::Mobile` the view
    /// renders as two separate full-screen pages (sidebar, then pushed detail
    /// page with a back button) instead of a side-by-side split.
    pub fn platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    fn effective_platform(&self) -> Platform {
        self.platform.unwrap_or_else(Platform::current)
    }

    fn default_placeholder() -> Box<dyn Widget> {
        DecoratedContainer::new(
            Text::new("Select an item")
                .with_color(tokens::navigation::PLACEHOLDER_TEXT_COLOR)
                .with_font_size(tokens::navigation::PLACEHOLDER_FONT_SIZE),
        )
        .background(tokens::navigation::DETAIL_BG)
        .padding(24.0)
        .boxed()
    }

    /// Resolve the effective selection: signal value, or default if unset.
    fn effective_selection(&self, signal_value: &Option<T>) -> Option<T> {
        signal_value
            .clone()
            .or_else(|| self.default_selection.clone())
    }

    fn build_collapsed_strip(&self, state: &mut NavigationSplitViewState<T>) -> Box<dyn Widget> {
        let expand_signal = state.sidebar_collapsed.clone();
        let toggle = Button::new("\u{25B6}") // ▶
            .variant(ButtonVariant::Ghost)
            .on_press(move || {
                expand_signal.set(false);
            })
            .boxed();

        DecoratedContainer::new(toggle)
            .background(tokens::navigation::SIDEBAR_BG)
            .padding(tokens::navigation::HEADER_PADDING)
            .width(tokens::navigation::COLLAPSED_WIDTH)
            .flex_shrink(0.0)
            .boxed()
    }

    fn build_sidebar(
        &self,
        state: &mut NavigationSplitViewState<T>,
        selected: &Option<T>,
    ) -> Box<dyn Widget> {
        let header = self.build_sidebar_header(state);
        let items_column = self.build_items_column(state, selected);
        let scroll = ScrollView::new(items_column).flex_grow(1.0);

        Flex::column()
            .background(tokens::navigation::SIDEBAR_BG)
            .width(self.preferred_sidebar_width)
            .flex_shrink(0.0)
            .push(header)
            .push(scroll)
            .boxed()
    }

    fn build_sidebar_header(&self, state: &mut NavigationSplitViewState<T>) -> Box<dyn Widget> {
        let title = Text::new("Navigation")
            .with_font_size(tokens::navigation::HEADER_FONT_SIZE)
            .with_color(tokens::navigation::HEADER_TEXT_COLOR);

        let collapse_signal = state.sidebar_collapsed.clone();
        let toggle = Button::new("\u{25C0}") // ◀
            .variant(ButtonVariant::Ghost)
            .on_press(move || {
                collapse_signal.set(true);
            })
            .boxed();

        Flex::row()
            .justify(JustifyContent::SpaceBetween)
            .align(AlignItems::Center)
            .padding(tokens::navigation::HEADER_PADDING)
            .background(tokens::navigation::HEADER_BG)
            .push(title)
            .push(toggle)
            .boxed()
    }

    fn build_items_column(
        &self,
        state: &mut NavigationSplitViewState<T>,
        selected: &Option<T>,
    ) -> Box<dyn Widget> {
        let mut col = Flex::column();
        for item in &self.items {
            let is_selected = selected.as_ref() == Some(&item.id);
            let row = self.build_item_row(item, is_selected, state);
            col = col.push(row);
        }
        col.boxed()
    }

    fn build_item_row(
        &self,
        item: &NavigationItem<T>,
        is_selected: bool,
        state: &mut NavigationSplitViewState<T>,
    ) -> Box<dyn Widget> {
        let text_color = if is_selected {
            tokens::navigation::SELECTED_TEXT_COLOR
        } else {
            tokens::navigation::ROW_TEXT_COLOR
        };
        let label = Text::new(&item.label)
            .with_font_size(tokens::navigation::ROW_FONT_SIZE)
            .with_color(text_color);

        let mut row = Flex::row().align(AlignItems::Center).gap(8.0);
        if let Some(ref icon) = item.icon {
            row = row.push(icon.clone_boxed());
        }
        row = row.push(label);

        let bg = if is_selected {
            tokens::navigation::SELECTED_BG
        } else {
            tokens::navigation::ROW_BG
        };

        let selected_signal = state.selected.clone();
        let on_change_cb = self.on_selection_change.clone();
        let item_id = item.id.clone();
        let is_mobile = self.effective_platform() == Platform::Mobile;
        let detail_visible_signal = state.detail_visible.clone();

        DecoratedContainer::new(row)
            .background(bg)
            .padding(tokens::navigation::ROW_PADDING)
            .boxed()
            .on_press(move || {
                selected_signal.set_from(&Some(item_id.clone()));
                if is_mobile {
                    detail_visible_signal.set_from(&true);
                }
                let id = item_id.clone();
                (on_change_cb)(&id);
            })
    }

    fn build_detail_content(&self, selected: &Option<T>) -> Box<dyn Widget> {
        match selected {
            Some(id) => (self.detail_builder)(id),
            None => self
                .placeholder
                .as_ref()
                .map(|p| p.clone_boxed())
                .unwrap_or_else(Self::default_placeholder),
        }
    }

    // ========================================================================
    // Mobile builders
    // ========================================================================
    //
    // On mobile the sidebar and detail never render side-by-side. The sidebar
    // fills the screen; selecting an item pushes the detail page (which has a
    // back button). This matches iOS NavigationStack semantics.

    fn build_mobile_sidebar(
        &self,
        state: &mut NavigationSplitViewState<T>,
        selected: &Option<T>,
    ) -> Box<dyn Widget> {
        let title = Text::new("Navigation")
            .with_font_size(tokens::navigation::HEADER_FONT_SIZE)
            .with_color(tokens::navigation::HEADER_TEXT_COLOR);

        // Minimal header: title only, no collapse toggle (collapse is a
        // desktop concept). Fixed height to match the mobile detail header.
        let header = Flex::row()
            .align(AlignItems::Center)
            .padding(tokens::navigation::MOBILE_HEADER_PADDING)
            .background(tokens::navigation::MOBILE_HEADER_BG)
            .height(tokens::navigation::MOBILE_HEADER_HEIGHT)
            .flex_shrink(0.0)
            .push(title)
            .boxed();

        let items_column = self.build_items_column(state, selected);
        let scroll = ScrollView::new(items_column).flex_grow(1.0);

        Flex::column()
            .background(tokens::navigation::SIDEBAR_BG)
            .flex_grow(1.0)
            .push(header)
            .push(scroll)
            .boxed()
    }

    fn build_mobile_detail_page(
        &self,
        state: &mut NavigationSplitViewState<T>,
        selected: &Option<T>,
    ) -> Box<dyn Widget> {
        let title = selected
            .as_ref()
            .and_then(|id| self.items.iter().find(|i| &i.id == id))
            .map(|i| i.label.clone())
            .unwrap_or_default();

        let header = self.build_mobile_detail_header(state, &title);
        let body = self.build_detail_content(selected);
        let scroll = ScrollView::new(body).flex_grow(1.0);

        Flex::column()
            .background(tokens::navigation::DETAIL_BG)
            .flex_grow(1.0)
            .push(header)
            .push(scroll)
            .boxed()
    }

    fn build_mobile_detail_header(
        &self,
        state: &mut NavigationSplitViewState<T>,
        title: &str,
    ) -> Box<dyn Widget> {
        let back_signal = state.detail_visible.clone();
        let back_label = format!(
            "{} {}",
            tokens::navigation::BACK_CHEVRON,
            tokens::navigation::BACK_LABEL
        );
        let back_button = Button::new(back_label)
            .variant(ButtonVariant::Ghost)
            .on_press(move || back_signal.set_from(&false))
            .boxed();

        let title_text = Text::new(title)
            .with_font_size(tokens::navigation::MOBILE_TITLE_FONT_SIZE)
            .with_color(tokens::navigation::MOBILE_TITLE_COLOR);

        Flex::row()
            .align(AlignItems::Center)
            .gap(8.0)
            .padding(tokens::navigation::MOBILE_HEADER_PADDING)
            .background(tokens::navigation::MOBILE_HEADER_BG)
            .height(tokens::navigation::MOBILE_HEADER_HEIGHT)
            .flex_shrink(0.0)
            .push(back_button)
            .push(title_text)
            .boxed()
    }
}

impl<T: Hash + Eq + Clone + 'static> Component for NavigationSplitView<T> {
    type State = NavigationSplitViewState<T>;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let signal_selected = state.selected.get_cloned();
        let selected = self.effective_selection(&signal_selected);

        match self.effective_platform() {
            Platform::Desktop => {
                let collapsed = state.sidebar_collapsed.get();
                let sidebar = if collapsed {
                    self.build_collapsed_strip(state)
                } else {
                    self.build_sidebar(state, &selected)
                };
                let detail = self.build_detail_content(&selected).flex_grow(1.0);

                Flex::row()
                    .background(tokens::navigation::DETAIL_BG)
                    .push(sidebar)
                    .push(detail)
                    .boxed()
            }
            Platform::Mobile => {
                if state.detail_visible.get() {
                    self.build_mobile_detail_page(state, &selected)
                } else {
                    self.build_mobile_sidebar(state, &selected)
                }
            }
        }
    }
}

// ============================================================================
// NAVIGATION STACK VIEW
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
