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

use std::hash::Hash;
use std::rc::Rc;
use std::sync::Arc;

use vexo::layout::JustifyContent;
use vexo::{
    AlignItems, Component, ComponentState, DecoratedContainer, Flex, RenderContext, ScrollView,
    Signal, Text, Widget,
};

use crate::button::{Button, ButtonVariant};
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
}

impl<T: 'static> Default for NavigationSplitViewState<T> {
    fn default() -> Self {
        Self {
            selected: Signal::new(None),
            sidebar_collapsed: Signal::new(false),
        }
    }
}

impl<T: 'static> ComponentState for NavigationSplitViewState<T> {
    fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.selected.set_dirty_callback(callback.clone());
        self.sidebar_collapsed.set_dirty_callback(callback);
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

        DecoratedContainer::new(row)
            .background(bg)
            .padding(tokens::navigation::ROW_PADDING)
            .boxed()
            .on_press(move || {
                selected_signal.set_from(&Some(item_id.clone()));
                let id = item_id.clone();
                (on_change_cb)(&id);
            })
    }

    fn build_detail(&self, selected: &Option<T>) -> Box<dyn Widget> {
        let content: Box<dyn Widget> = match selected {
            Some(id) => (self.detail_builder)(id),
            None => self
                .placeholder
                .as_ref()
                .map(|p| p.clone_boxed())
                .unwrap_or_else(Self::default_placeholder),
        };
        content.flex_grow(1.0)
    }
}

impl<T: Hash + Eq + Clone + 'static> Component for NavigationSplitView<T> {
    type State = NavigationSplitViewState<T>;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let collapsed = state.sidebar_collapsed.get();
        let signal_selected = state.selected.get_cloned();
        let selected = self.effective_selection(&signal_selected);

        let sidebar = if collapsed {
            self.build_collapsed_strip(state)
        } else {
            self.build_sidebar(state, &selected)
        };

        let detail = self.build_detail(&selected);

        Flex::row()
            .background(tokens::navigation::DETAIL_BG)
            .push(sidebar)
            .push(detail)
            .boxed()
    }
}
