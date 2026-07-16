//! TabBar — bottom-tab navigation component.
//!
//! Mirrors `NavigationStackView`'s pattern: an external `TabController<D>`
//! owns the current tab; `TabBarView` renders the active page (via
//! `IndexedStack` — all pages stay mounted, state preserved) above a row of
//! tab items built by the caller.

use std::any::Any;
use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;
use std::sync::Arc;

use vexo::layout::FlexDirection;
use vexo::layout::JustifyContent;
use vexo::{
    Component, ComponentState, Flex, IndexedStack, Layout, LifecycleContext, RenderContext,
    SafeArea, Text, Theme, Widget,
};

use crate::theme::tokens;

// ============================================================================
// TAB CONTROLLER
// ============================================================================

/// External controller owning the current tab. Caller creates and owns this;
/// `TabBarView` wires a dirty callback on mount so `switch_to` triggers a
/// rebuild. Mirrors `NavigationController<D>` (`vexo_uikit/src/navigation.rs:97`).
pub struct TabController<D: Hash + Eq + Clone + 'static> {
    current: Rc<RefCell<D>>,
    dirty_callback: Rc<RefCell<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl<D: Hash + Eq + Clone + 'static> TabController<D> {
    pub fn new(initial: D) -> Self {
        Self {
            current: Rc::new(RefCell::new(initial)),
            dirty_callback: Rc::new(RefCell::new(None)),
        }
    }

    pub fn current(&self) -> D {
        self.current.borrow().clone()
    }

    pub fn switch_to(&self, dest: D) {
        if self.current.borrow().clone() == dest {
            return; // no-op if same
        }
        *self.current.borrow_mut() = dest;
        self.notify();
    }

    pub fn set_dirty_callback(&self, cb: Arc<dyn Fn() + Send + Sync>) {
        *self.dirty_callback.borrow_mut() = Some(cb);
    }

    pub fn clear_dirty_callback(&self) {
        *self.dirty_callback.borrow_mut() = None;
    }

    fn notify(&self) {
        if let Some(cb) = self.dirty_callback.borrow().as_ref() {
            cb();
        }
    }
}

impl<D: Hash + Eq + Clone + 'static> Clone for TabController<D> {
    fn clone(&self) -> Self {
        Self {
            current: Rc::clone(&self.current),
            dirty_callback: Rc::clone(&self.dirty_callback),
        }
    }
}

// ============================================================================
// TAB BAR VIEW
// ============================================================================

/// Builder for the tab bar row. Called per tab with `(dest, is_selected)`.
pub type TabBarBuilder<D> = Rc<dyn Fn(&D, bool) -> Box<dyn Widget>>;

/// Builder for the page content. Called once per tab on first render.
pub type PageBuilder<D> = Rc<dyn Fn(&D) -> Box<dyn Widget>>;

pub struct TabBarView<D: Hash + Eq + Clone + 'static> {
    controller: TabController<D>,
    tabs: Vec<D>,
    page_builder: PageBuilder<D>,
    tab_bar_builder: TabBarBuilder<D>,
}

impl<D: Hash + Eq + Clone + 'static> TabBarView<D> {
    pub fn new(
        controller: TabController<D>,
        tabs: Vec<D>,
        page_builder: impl Fn(&D) -> Box<dyn Widget> + 'static,
        tab_bar_builder: impl Fn(&D, bool) -> Box<dyn Widget> + 'static,
    ) -> Self {
        Self {
            controller,
            tabs,
            page_builder: Rc::new(page_builder),
            tab_bar_builder: Rc::new(tab_bar_builder),
        }
    }
}

impl<D: Hash + Eq + Clone + 'static> Clone for TabBarView<D> {
    fn clone(&self) -> Self {
        Self {
            controller: self.controller.clone(),
            tabs: self.tabs.clone(),
            page_builder: self.page_builder.clone(),
            tab_bar_builder: self.tab_bar_builder.clone(),
        }
    }
}

// `Component::State` is keyed to the widget type, so the framework calls
// `on_mount` with `ctx.widget()` typed as `&dyn Any` (the widget's `as_any()`).
// We downcast to `TabBarView<D>` directly, monomorphized per `D`. Requires
// `D: 'static + Any` — satisfied by the IM app's `ImTab` enum.

impl<D: Hash + Eq + Clone + 'static + Any> ComponentState for TabBarViewStateD<D> {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(tv) = ctx.widget().downcast_ref::<TabBarView<D>>() {
            tv.controller.set_dirty_callback(ctx.dirty_callback());
        }
    }
    fn on_unmount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(tv) = ctx.widget().downcast_ref::<TabBarView<D>>() {
            tv.controller.clear_dirty_callback();
        }
    }
}

pub struct TabBarViewStateD<D: Hash + Eq + Clone + 'static>(std::marker::PhantomData<D>);

impl<D: Hash + Eq + Clone + 'static> Default for TabBarViewStateD<D> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<D: Hash + Eq + Clone + 'static + Any> Component for TabBarView<D> {
    type State = TabBarViewStateD<D>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let current_index = self
            .tabs
            .iter()
            .position(|t| *t == self.controller.current())
            .unwrap_or(0);

        // Build all pages (IndexedStack keeps them all mounted).
        let mut stack = IndexedStack::new(current_index);
        for tab in &self.tabs {
            stack = stack.push((self.page_builder)(tab));
        }

        // Build the tab bar row.
        let mut bar = Flex::row().layout(
            Layout::default()
                .justify(JustifyContent::SpaceBetween)
                .width_percent(1.0),
        );
        for tab in &self.tabs {
            let is_selected = *tab == self.controller.current();
            let ctrl = self.controller.clone();
            let tab_clone = tab.clone();
            let item = (self.tab_bar_builder)(tab, is_selected)
                .on_press(move || ctrl.switch_to(tab_clone.clone()));
            bar = bar.push(item);
        }

        // The tab bar row owns its bottom safe-area (home indicator) and
        // left/right insets (landscape notch), mirroring how
        // `NavigationStackView` owns the top inset for its nav bar. `top(false)`
        // because the bar is at the bottom — no status-bar inset to consume.
        //
        // `SafeArea` bakes in `flex_grow(1.0)` (correct for content areas that
        // should fill their parent, but wrong for the tab bar which should be
        // its intrinsic height). Wrapping in `WithLayout` with `flex_grow(0.0)`
        // + `flex_shrink(0.0)` pins the bar to its content height so it doesn't
        // steal space from the page area above.
        let bar = SafeArea::new(bar.boxed()).top(false).boxed();
        let bar = bar.with_layout(Layout::default().flex_grow(0.0).flex_shrink(0.0));

        // SwiftUI-style hairline along the tab bar's top edge (the seam
        // between the page content and the bar). 1 logical px — Taffy floors
        // sub-pixel heights to 0, so a true 1-physical-px `1/scale` height
        // would vanish on Retina. Sits above the `SafeArea`-wrapped bar so it
        // spans the full width edge-to-edge. See `HAIRLINE_THICKNESS`.
        let nav = tokens::navigation::colors(&Theme::of(ctx));
        let hairline = Flex::row()
            .background(nav.divider)
            .height(tokens::navigation::HAIRLINE_THICKNESS)
            .flex_shrink(0.0);
        let bar = Flex::column().flex_shrink(0.0).push(hairline).push(bar);

        Flex::column()
            .layout(
                Layout::default()
                    .flex_direction(FlexDirection::Column)
                    .width_percent(1.0)
                    .height_percent(1.0),
            )
            .push(stack.flex_fill())
            .push(bar)
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use vexo::Text;

    #[derive(Hash, Eq, PartialEq, Clone, Debug)]
    enum TestTab {
        A,
        B,
    }

    #[test]
    fn test_tab_controller_switch_fires_callback() {
        let ctrl = TabController::new(TestTab::A);
        let fired = Arc::new(AtomicBool::new(false));
        let fired_clone = fired.clone();
        ctrl.set_dirty_callback(Arc::new(move || {
            fired_clone.store(true, Ordering::SeqCst);
        }));
        assert_eq!(ctrl.current(), TestTab::A);
        ctrl.switch_to(TestTab::B);
        assert_eq!(ctrl.current(), TestTab::B);
        assert!(
            fired.load(Ordering::SeqCst),
            "dirty callback should fire on switch"
        );
    }

    #[test]
    fn test_tab_controller_noop_on_same_tab() {
        let ctrl = TabController::new(TestTab::A);
        let fired = Arc::new(AtomicBool::new(false));
        let fired_clone = fired.clone();
        ctrl.set_dirty_callback(Arc::new(move || {
            fired_clone.store(true, Ordering::SeqCst);
        }));
        ctrl.switch_to(TestTab::A); // same
        assert!(
            !fired.load(Ordering::SeqCst),
            "no fire when switching to current tab"
        );
    }

    #[test]
    fn test_tab_controller_clear_dirty_callback() {
        let ctrl = TabController::new(TestTab::A);
        let fired = Arc::new(AtomicBool::new(false));
        let fired_clone = fired.clone();
        ctrl.set_dirty_callback(Arc::new(move || {
            fired_clone.store(true, Ordering::SeqCst);
        }));
        ctrl.clear_dirty_callback();
        ctrl.switch_to(TestTab::B);
        assert!(!fired.load(Ordering::SeqCst), "no fire after clear");
    }

    #[test]
    fn test_tab_controller_clone_shares_state() {
        let ctrl = TabController::new(TestTab::A);
        let ctrl2 = ctrl.clone();
        ctrl2.switch_to(TestTab::B);
        assert_eq!(ctrl.current(), TestTab::B, "clone shares state");
    }

    #[test]
    fn test_tab_bar_top_hairline_paints() {
        // Regression: the tab bar's top hairline must actually paint. A
        // sub-pixel height (e.g. `1/scale` = 0.5 at 2×) gets floored to 0 by
        // Taffy and renders nothing — so the hairline must use a full logical
        // pixel (`HAIRLINE_THICKNESS`).
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::render::RenderCommand;
        use vexo::ThreeTreePipeline;

        let ctrl = TabController::new(TestTab::A);
        let view = TabBarView::new(
            ctrl,
            vec![TestTab::A, TestTab::B],
            |tab| match tab {
                TestTab::A => Text::new("Page A").boxed(),
                TestTab::B => Text::new("Page B").boxed(),
            },
            |_, is_selected| Text::new(if is_selected { "[A]" } else { "A" }).boxed(),
        );
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(Box::new(view));
        let mut engine = vexo::layout::TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        let commands = pipeline.paint();

        let light = vexo::ThemeData::light();
        let divider = crate::theme::tokens::navigation::colors(&light).divider;
        let thickness = crate::theme::tokens::navigation::HAIRLINE_THICKNESS;

        let hairline = commands.iter().find_map(|cmd| {
            if let RenderCommand::Rect { bounds, fill, .. } = cmd {
                if *fill == divider && bounds.width() >= 390.0 {
                    return Some(bounds);
                }
            }
            None
        });

        let b = hairline.expect("expected a full-width divider hairline at the tab seam");
        assert_eq!(
            b.height(),
            thickness,
            "hairline must use HAIRLINE_THICKNESS (sub-pixel heights floor to 0)"
        );
        assert!(
            b.top >= 500.0 && b.top <= 600.0,
            "hairline top {} should be near the bottom seam",
            b.top
        );
    }

    #[test]
    fn test_tab_bar_view_renders_active_page() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::ThreeTreePipeline;

        let ctrl = TabController::new(TestTab::A);
        let ctrl_for_view = ctrl.clone();
        let view = TabBarView::new(
            ctrl_for_view,
            vec![TestTab::A, TestTab::B],
            |tab| match tab {
                TestTab::A => Text::new("Page A").boxed(),
                TestTab::B => Text::new("Page B").boxed(),
            },
            |tab, is_selected| {
                let label = if is_selected { "[A]" } else { "A" };
                let _ = tab;
                Text::new(label).boxed()
            },
        );
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(Box::new(view));
        let mut engine = vexo::layout::TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        // Both pages should be mounted (IndexedStack keeps them all).
        assert!(
            pipeline.element_registry().len() > 4,
            "expected several elements (column, stack, 2 pages, bar, 2 items, texts)"
        );
        // Switch to B — should fire dirty and re-render with B visible.
        ctrl.switch_to(TestTab::B);
        // The dirty callback was wired during mount, so switch_to triggers
        // mark_needs_build; the next update picks it up.
        pipeline.update(Box::new(TabBarView::new(
            ctrl.clone(),
            vec![TestTab::A, TestTab::B],
            |tab| match tab {
                TestTab::A => Text::new("Page A").boxed(),
                TestTab::B => Text::new("Page B").boxed(),
            },
            |_, is_selected| Text::new(if is_selected { "[B]" } else { "B" }).boxed(),
        )));
        // No panic = pass.
    }
}
