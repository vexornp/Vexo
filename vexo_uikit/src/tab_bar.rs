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

use vexo::layout::{AlignItems, FlexDirection, JustifyContent};
use vexo::{
    children, Component, ComponentState, DecoratedBox, GestureDetector, IndexedStack, Layout,
    LifecycleContext, MediaQuery, MediaQueryData, MediaQueryMutator, MultiChild, RemoveEdges,
    RenderContext, SafeArea, Style, Theme, Widget, WithLayout,
};

use crate::theme::tokens;

/// Natural height of the tab bar row (excluding safe-area inset), in logical
/// pixels. Matches iOS `UITabBar`'s standard 49pt height.
const TAB_BAR_HEIGHT: f32 = 49.0;

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

    /// Level 3 rebuild-skip (see `docs/rebuild-skipping-patterns.md`).
    /// During keyboard animation, the parent cascades `update()` to us with
    /// fresh closures but the tabs and current selection haven't changed.
    /// Comparing only observable state stops the cascade before it rebuilds
    /// the entire IndexedStack + tab bar UI.
    fn should_rebuild(&self, old: &Self) -> bool {
        self.tabs != old.tabs || self.controller.current() != old.controller.current()
    }

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let current_index = self
            .tabs
            .iter()
            .position(|t| *t == self.controller.current())
            .unwrap_or(0);

        let mut stack = IndexedStack::new(current_index);
        for tab in &self.tabs {
            stack = stack.push((self.page_builder)(tab));
        }

        let nav = tokens::navigation::colors(&Theme::of(ctx));
        let mut bar = MultiChild::empty(Layout::default().width_percent(1.0).height(49.0));
        for tab in &self.tabs {
            let is_selected = *tab == self.controller.current();
            let ctrl = self.controller.clone();
            let tab_clone = tab.clone();
            let content = (self.tab_bar_builder)(tab, is_selected);
            let item = GestureDetector::new(content)
                .on_press(move || ctrl.switch_to(tab_clone.clone()))
                .with_layout(
                    Layout::default()
                        .flex_direction(FlexDirection::Column)
                        .align(AlignItems::Stretch)
                        .flex_grow(1.0)
                        .justify(JustifyContent::Center),
                )
                .boxed();
            bar = bar.push(item);
        }

        let bar = DecoratedBox::with_style(
            MediaQueryMutator::new(
                SafeArea::new(bar.boxed()).top(false).boxed(),
                |parent: &MediaQueryData| {
                    let mut p = parent.padding;
                    p.bottom = parent.viewPadding.bottom;
                    parent.copy_with_padding(p)
                },
            )
            .boxed(),
            Style::default().background(nav.mobile_header_bg),
        );
        let bar = WithLayout::new(bar, Layout::default().flex_grow(0.0).flex_shrink(0.0));

        let hairline = DecoratedBox::with_style(
            MultiChild::empty(
                Layout::row()
                    .height(tokens::navigation::HAIRLINE_THICKNESS)
                    .flex_shrink(0.0),
            ),
            Style::default().background(nav.divider),
        );
        let bar = MultiChild::new(children![hairline, bar], Layout::column().flex_shrink(0.0));

        // IMPORTANT: Do NOT call MediaQuery::of(ctx) here — it would make
        // TabBar a MediaQuery dependent, causing it to rebuild on every
        // keyboard animation frame. Instead, compute tab_bar_height inside
        // the MediaQueryMutator closure, which reads the parent MediaQuery
        // during its own render(). TabBar itself is NOT a dependent.
        let page = MediaQueryMutator::new(stack.boxed(), |parent: &MediaQueryData| {
            let tab_bar_height = TAB_BAR_HEIGHT + parent.viewPadding.bottom;
            let mut v = parent.viewInsets;
            v.bottom = (v.bottom - tab_bar_height).max(0.0);
            parent.copy_with_view_insets(v)
        });
        let page = MediaQuery::remove_padding(page, RemoveEdges::BOTTOM);
        let content = MultiChild::new(
            children![WithLayout::new(page, Layout::flex_fill()), bar,],
            Layout::default()
                .flex_direction(FlexDirection::Column)
                .width_percent(1.0)
                .height_percent(1.0),
        );
        content.boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use vexo::SimpleState;
    use vexo::Text;

    #[derive(Hash, Eq, PartialEq, Clone, Debug)]
    enum TestTab {
        A,
        B,
        C,
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
                TestTab::C => Text::new("Page C").boxed(),
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
                TestTab::C => Text::new("Page C").boxed(),
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
                TestTab::C => Text::new("Page C").boxed(),
            },
            |_, is_selected| Text::new(if is_selected { "[B]" } else { "B" }).boxed(),
        )));
        // No panic = pass.
    }

    #[test]
    fn test_tab_bar_items_are_equal_width_full_height_slots() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::widgets::gesture_detector::GestureDetectorRenderObject;
        use vexo::ThreeTreePipeline;

        let ctrl = TabController::new(TestTab::A);
        let view = TabBarView::new(
            ctrl,
            vec![TestTab::A, TestTab::B, TestTab::C],
            |tab| match tab {
                TestTab::A => Text::new("Page A").boxed(),
                TestTab::B => Text::new("Page B").boxed(),
                TestTab::C => Text::new("Page C").boxed(),
            },
            |tab, _| match tab {
                TestTab::A => Text::new("A").boxed(),
                TestTab::B => Text::new("B").boxed(),
                TestTab::C => Text::new("C").boxed(),
            },
        );
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(Box::new(view));
        let mut engine = vexo::layout::TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(390.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("should have root");

        fn find_gd_bounds(
            ro_reg: &vexo::RenderObjectRegistry,
            id: vexo::RenderObjectKey,
            out: &mut Vec<vexo::core::Bounds<vexo::core::Logical>>,
        ) {
            if let Some(ro) = ro_reg.get(id) {
                if ro
                    .as_any()
                    .downcast_ref::<GestureDetectorRenderObject>()
                    .is_some()
                {
                    if let Some(b) = ro.computed_bounds() {
                        out.push(b);
                    }
                }
                for &c in ro.children() {
                    find_gd_bounds(ro_reg, c, out);
                }
            }
        }

        let mut gd_bounds = Vec::new();
        find_gd_bounds(ro_reg, root, &mut gd_bounds);
        assert_eq!(
            gd_bounds.len(),
            3,
            "expected 3 tab-item GestureDetectors, found {}",
            gd_bounds.len()
        );

        // Sort by left so slot order is A, B, C.
        gd_bounds.sort_by(|a, b| a.left.partial_cmp(&b.left).unwrap());

        // Each slot must be 390/3 = 130 wide and 49 tall.
        for (i, b) in gd_bounds.iter().enumerate() {
            assert!(
                (b.width() - 130.0).abs() < 1.0,
                "slot {} width {} should be ~130 (390/3)",
                i,
                b.width()
            );
            assert!(
                (b.height() - 49.0).abs() < 1.0,
                "slot {} height {} should be ~49",
                i,
                b.height()
            );
            assert!(
                (b.left - (i as f32) * 130.0).abs() < 1.0,
                "slot {} left {} should be ~{}",
                i,
                b.left,
                i * 130
            );
        }

        // No dead space: widths sum to bar width.
        let total: f32 = gd_bounds.iter().map(|b| b.width()).sum();
        assert!(
            (total - 390.0).abs() < 1.0,
            "slot widths sum {} should be ~390 (no dead space)",
            total
        );
    }

    #[test]
    fn test_tab_bar_tap_between_icons_selects_slot() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::input::{ButtonState, InputEvent, PointerButton};
        use vexo::platform::stub_clipboard::StubClipboard;
        use vexo::ThreeTreePipeline;

        let ctrl = TabController::new(TestTab::B);
        let ctrl_for_view = ctrl.clone();
        let view = TabBarView::new(
            ctrl_for_view,
            vec![TestTab::A, TestTab::B, TestTab::C],
            |tab| match tab {
                TestTab::A => Text::new("Page A").boxed(),
                TestTab::B => Text::new("Page B").boxed(),
                TestTab::C => Text::new("Page C").boxed(),
            },
            |tab, _| match tab {
                TestTab::A => Text::new("A").boxed(),
                TestTab::B => Text::new("B").boxed(),
                TestTab::C => Text::new("C").boxed(),
            },
        );
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(Box::new(view));
        let mut engine = vexo::layout::TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(390.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Find the y of the bar (top of the first GestureDetector slot).
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("should have root");
        use vexo::widgets::gesture_detector::GestureDetectorRenderObject;
        fn find_first_gd_absolute_top(
            ro_reg: &vexo::RenderObjectRegistry,
            id: vexo::RenderObjectKey,
            parent_abs_y: f32,
        ) -> Option<f32> {
            let ro = ro_reg.get(id)?;
            let rel_y = ro.computed_bounds().map(|b| b.top).unwrap_or(0.0);
            let abs_y = parent_abs_y + rel_y;
            if ro
                .as_any()
                .downcast_ref::<GestureDetectorRenderObject>()
                .is_some()
            {
                return Some(abs_y);
            }
            let child_parent_abs_y = if ro.is_pass_through() {
                parent_abs_y
            } else {
                abs_y
            };
            for &c in ro.children() {
                if let Some(t) = find_first_gd_absolute_top(ro_reg, c, child_parent_abs_y) {
                    return Some(t);
                }
            }
            None
        }
        let bar_top =
            find_first_gd_absolute_top(ro_reg, root, 0.0).expect("should find a GestureDetector");
        let tap_y = bar_top + 24.5; // middle of the 49pt slot

        // Tap at x=110 (inside slot 0's 0..130 range, but well off the "A" icon
        // which is centered at x~65). Before this change, this position was in
        // dead space between the SpaceBetween-spread items and did nothing.
        let tap_x = 110.0;
        let event = InputEvent::PointerButton {
            position: vexo::core::Point::new(tap_x, tap_y),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        let clipboard: Arc<dyn vexo::platform::Clipboard> = Arc::new(StubClipboard);
        pipeline.handle_event(
            vexo::core::Point::new(tap_x, tap_y),
            &event,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );

        assert_eq!(
            ctrl.current(),
            TestTab::A,
            "tapping at x=110 (slot 0, off-icon) must select tab A"
        );

        // Tap at x=250 (inside slot 1's 130..260 range, off-icon).
        // Must select tab B (the slot owning x=250).
        let tap_x = 250.0;
        let event = InputEvent::PointerButton {
            position: vexo::core::Point::new(tap_x, tap_y),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            vexo::core::Point::new(tap_x, tap_y),
            &event,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        assert_eq!(
            ctrl.current(),
            TestTab::B,
            "tapping at x=250 (slot 1, off-icon) must select tab B"
        );
    }

    #[test]
    fn test_tab_bar_height_constant_during_keyboard_animation() {
        use vexo::animation::AnimationTicker;
        use vexo::layout::TaffyLayoutEngine;
        use vexo::render::RenderCommand;
        use vexo::ThreeTreePipeline;

        // Simulate what RootMediaQuery would produce for two keyboard states.
        // safe_area.bottom = 34 (home indicator), keyboard interpolates 150 -> 0.
        // padding.bottom = max(34 - kh, 0)  — the clamped formula that causes the pop.
        fn mq_for_keyboard(kh: f32) -> MediaQueryData {
            let mut mq = MediaQueryData::all_zero();
            mq.size = vexo::core::Size::new(390.0, 600.0);
            mq.padding.bottom = (34.0 - kh).max(0.0);
            mq.viewInsets.bottom = kh;
            mq.viewPadding.bottom = 34.0;
            mq
        }

        fn build_view(kh: f32) -> Box<dyn Widget> {
            let ctrl = TabController::new(TestTab::A);
            let inner = TabBarView::new(
                ctrl,
                vec![TestTab::A, TestTab::B],
                |tab| match tab {
                    TestTab::A => Text::new("Page A").boxed(),
                    TestTab::B => Text::new("Page B").boxed(),
                    TestTab::C => Text::new("Page C").boxed(),
                },
                |_, is_selected| Text::new(if is_selected { "[A]" } else { "A" }).boxed(),
            );
            MediaQuery::new(mq_for_keyboard(kh), inner).boxed()
        }

        fn bar_bg_height(kh: f32) -> f32 {
            let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
            pipeline.update(build_view(kh));
            let mut engine = TaffyLayoutEngine::new();
            let mut font_system = vexo::resource::new_font_system();
            pipeline.layout(
                vexo::core::Size::new(390.0, 600.0),
                &mut engine,
                &mut font_system,
            );
            let commands = pipeline.paint();
            let nav_bg = crate::theme::tokens::navigation::colors(&vexo::ThemeData::light())
                .mobile_header_bg;
            commands
                .iter()
                .find_map(|cmd| {
                    if let RenderCommand::Rect { bounds, fill, .. } = cmd {
                        if *fill == nav_bg && bounds.width() >= 380.0 {
                            return Some(bounds.height());
                        }
                    }
                    None
                })
                .expect("expected a tab bar background rect")
        }

        // The bar height must be the SAME in both states: 49 + 34 = 83.
        // Before the fix, mid-animation (kh=150) gives 49 (clamped padding=0),
        // and end (kh=0) gives 83 (padding=34) — the "pop".
        let mid = bar_bg_height(150.0);
        let end = bar_bg_height(0.0);
        assert!(
            (mid - 83.0).abs() < 1.0,
            "mid-animation bar height {} should be ~83 (49 + 34 home-indicator inset)",
            mid
        );
        assert!(
            (end - 83.0).abs() < 1.0,
            "end bar height {} should be ~83",
            end
        );
        assert!(
            (mid - end).abs() < 0.5,
            "bar height must be constant: mid={}, end={}",
            mid,
            end
        );
    }

    /// A tiny Component that captures the `MediaQueryData` its subtree receives.
    /// Used to verify what `viewInsets.bottom` the page child sees after
    /// `TabBarView`'s `MediaQueryMutator` chain transforms the parent MQ.
    #[derive(Clone)]
    struct MqCapture {
        captured: Rc<RefCell<Option<MediaQueryData>>>,
    }

    impl Component for MqCapture {
        type State = SimpleState<()>;
        fn render(&self, _state: &mut SimpleState<()>, ctx: &mut RenderContext) -> Box<dyn Widget> {
            *self.captured.borrow_mut() = Some(MediaQuery::of(ctx));
            Text::new("capture").boxed()
        }
    }

    #[test]
    fn test_tab_bar_page_view_insets_match_bar_height() {
        use vexo::animation::AnimationTicker;
        use vexo::ThreeTreePipeline;
        // Simulate mid-keyboard-dismiss: keyboard_height=150, safe_bottom=34.
        // RootMediaQuery would produce: padding.bottom = max(34-150,0) = 0,
        // viewInsets.bottom = 150, viewPadding.bottom = 34.
        let mut mq = MediaQueryData::all_zero();
        mq.size = vexo::core::Size::new(390.0, 600.0);
        mq.padding.bottom = 0.0; // clamped (34 - 150 < 0)
        mq.viewInsets.bottom = 150.0;
        mq.viewPadding.bottom = 34.0;

        let captured = Rc::new(RefCell::new(None));
        let captured_for_page = Rc::clone(&captured);
        let ctrl = TabController::new(TestTab::A);
        let inner = TabBarView::new(
            ctrl,
            vec![TestTab::A],
            move |_| {
                MqCapture {
                    captured: Rc::clone(&captured_for_page),
                }
                .boxed()
            },
            |_, _| Text::new("A").boxed(),
        );
        let view = MediaQuery::new(mq, inner).boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);

        let captured_mq = captured
            .borrow()
            .clone()
            .expect("MqCapture::render should have been called");

        // After Change 2: tab_bar_height = 49 + viewPadding.bottom = 83.
        // Page child sees viewInsets.bottom = max(150 - 83, 0) = 67.
        //
        // Before Change 2: tab_bar_height = 49 + padding.bottom = 49 + 0 = 49
        // (clamped!), so page child sees viewInsets.bottom = max(150 - 49, 0) = 101.
        // The 34pt gap means chat content would overlap the bar by 34pt.
        assert!(
            (captured_mq.viewInsets.bottom - 67.0).abs() < 1.0,
            "page child viewInsets.bottom should be ~67 (150 - 83), got {} — \
             tab_bar_height is using clamped padding.bottom instead of viewPadding.bottom",
            captured_mq.viewInsets.bottom
        );
    }
}
