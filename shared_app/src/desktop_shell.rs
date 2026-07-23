//! DesktopShell — top-level desktop layout: sidebar (tab switcher) + page area.
//!
//! Mirrors `TabBarView`'s shape (controller + page builder + sidebar builder)
//! but renders a left sidebar column instead of a bottom tab bar. The sidebar
//! is icon-only with color highlight (matching mobile's tab bar style).
//!
//! Like `TabBarView`, this is a `Component` whose `ComponentState` wires the
//! `TabController`'s dirty callback on mount/unmount so `switch_to` triggers
//! a rebuild.

use std::any::Any;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;

use vexo::{
    children, AlignItems, Component, ComponentState, DecoratedBox, GestureDetector, IndexedStack,
    JustifyContent, Layout, LifecycleContext, MultiChild, RenderContext, Signal, Style, Theme,
    Widget, WithLayout,
};
use vexo_uikit::theme::tokens::navigation::{self, NavColors, HAIRLINE_THICKNESS, SIDEBAR_WIDTH};
use vexo_uikit::TabController;

use crate::widgets::theme_toggle::ThemeToggle;

// ============================================================================
// TYPE ALIASES FOR BUILDERS
// ============================================================================

/// Builder for sidebar items: receives the tab, selection state, and resolved
/// navigation colors. Returns the item's content (icon + label). The shell
/// wraps this in a `GestureDetector` and applies the `selected_bg` background.
pub(crate) type SidebarBuilder<D> = Arc<dyn Fn(&D, bool, &NavColors) -> Box<dyn Widget>>;

/// Builder for page content. Called once per tab per render.
pub(crate) type PageBuilder<D> = Arc<dyn Fn(&D) -> Box<dyn Widget>>;

// ============================================================================
// DESKTOP SHELL
// ============================================================================

pub(crate) struct DesktopShell<D: Hash + Eq + Clone + 'static> {
    pub controller: TabController<D>,
    pub tabs: Vec<D>,
    pub page_builder: PageBuilder<D>,
    pub sidebar_builder: SidebarBuilder<D>,
    /// Drives the theme toggle pinned to the sidebar bottom.
    pub is_dark: Signal<bool>,
}

impl<D: Hash + Eq + Clone + 'static> Clone for DesktopShell<D> {
    fn clone(&self) -> Self {
        Self {
            controller: self.controller.clone(),
            tabs: self.tabs.clone(),
            page_builder: Arc::clone(&self.page_builder),
            sidebar_builder: Arc::clone(&self.sidebar_builder),
            is_dark: self.is_dark.clone(),
        }
    }
}

// ============================================================================
// STATE (lifecycle wiring for TabController dirty callback)
// ============================================================================

pub(crate) struct DesktopShellStateD<D: Hash + Eq + Clone + 'static>(PhantomData<D>);

impl<D: Hash + Eq + Clone + 'static> Default for DesktopShellStateD<D> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<D: Hash + Eq + Clone + 'static + Any> ComponentState for DesktopShellStateD<D> {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(shell) = ctx.widget().downcast_ref::<DesktopShell<D>>() {
            shell.controller.set_dirty_callback(ctx.dirty_callback());
        }
    }
    fn on_unmount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(shell) = ctx.widget().downcast_ref::<DesktopShell<D>>() {
            shell.controller.clear_dirty_callback();
        }
    }
}

// ============================================================================
// COMPONENT IMPL
// ============================================================================

impl<D: Hash + Eq + Clone + 'static + Any> Component for DesktopShell<D> {
    type State = DesktopShellStateD<D>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let nav_colors = navigation::colors(&Theme::of(ctx));

        // --- Sidebar (column 1): 240px fixed, full height, right hairline ---
        let sidebar = build_sidebar(self, &nav_colors);

        // --- Page area: IndexedStack keeps all pages mounted ---
        let current_index = self
            .tabs
            .iter()
            .position(|t| *t == self.controller.current())
            .unwrap_or(0);

        let mut stack = IndexedStack::new(current_index);
        for tab in &self.tabs {
            stack = stack.push((self.page_builder)(tab));
        }

        MultiChild::new(
            children![sidebar, WithLayout::new(stack, Layout::flex_fill()),],
            Layout::row().width_percent(1.0).height_percent(1.0),
        )
        .boxed()
    }
}

/// Build the sidebar: a 240px-wide column of tab items on `sidebar_bg`,
/// with a 1px right-edge hairline divider.
fn build_sidebar<D: Hash + Eq + Clone + 'static + Any>(
    shell: &DesktopShell<D>,
    nav_colors: &NavColors,
) -> Box<dyn Widget>
where
    D: Any,
{
    // Build sidebar items (top-aligned column).
    let mut items = MultiChild::empty(Layout::column());
    for tab in &shell.tabs {
        let is_selected = *tab == shell.controller.current();
        let ctrl = shell.controller.clone();
        let tab_clone = tab.clone();
        let content = (shell.sidebar_builder)(tab, is_selected, nav_colors);

        let item = GestureDetector::new(content)
            .on_press(move || ctrl.switch_to(tab_clone.clone()))
            .with_layout(
                Layout::default()
                    .width_percent(1.0)
                    .height(48.0)
                    .flex_shrink(0.0)
                    .align(AlignItems::Center)
                    .justify(JustifyContent::Center),
            )
            .boxed();
        items = items.push(item);
    }

    // Flex-grow spacer pushes the toggle to the sidebar bottom.
    items = items.push(MultiChild::empty(Layout::default().flex_grow(1.0)));

    // Theme toggle pinned to the bottom of the sidebar.
    items = items.push(
        WithLayout::new(
            ThemeToggle::new(shell.is_dark.clone()),
            Layout::default()
                .width_percent(1.0)
                .height(48.0)
                .flex_shrink(0.0)
                .align(AlignItems::Center)
                .justify(JustifyContent::Center),
        )
        .boxed(),
    );

    // Sidebar content on sidebar_bg, filling the width minus the hairline.
    let sidebar_content = DecoratedBox::with_style(
        WithLayout::new(items, Layout::flex_fill()),
        Style::default().background(nav_colors.sidebar_bg),
    );

    // Right-edge hairline (1px).
    let hairline = DecoratedBox::with_style(
        MultiChild::empty(
            Layout::column()
                .width(HAIRLINE_THICKNESS)
                .height_percent(1.0)
                .flex_shrink(0.0),
        ),
        Style::default().background(nav_colors.divider),
    );

    WithLayout::new(
        MultiChild::new(
            children![sidebar_content, hairline],
            Layout::row()
                .width(SIDEBAR_WIDTH)
                .height_percent(1.0)
                .flex_shrink(0.0),
        ),
        Layout::default(),
    )
    .boxed()
}
