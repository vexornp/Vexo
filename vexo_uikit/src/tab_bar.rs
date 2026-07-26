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
    LifecycleContext, MultiChild, RenderContext, SafeArea, SafeAreaClaim, Style, Theme, Widget,
    WithLayout,
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
        //
        // The `DecoratedBox` paints `mobile_header_bg` edge-to-edge so the bar
        // (and the home-indicator inset below it) has a themed background
        // instead of showing the window's clear color (white in dark mode).
        let bar = DecoratedBox::with_style(
            SafeArea::new(bar.boxed()).top(false).boxed(),
            Style::default().background(nav.mobile_header_bg),
        );
        let bar = WithLayout::new(bar, Layout::default().flex_grow(0.0).flex_shrink(0.0));

        // SwiftUI-style hairline along the tab bar's top edge (the seam
        // between the page content and the bar). 1 logical px — Taffy floors
        // sub-pixel heights to 0, so a true 1-physical-px `1/scale` height
        // would vanish on Retina. Sits above the `SafeArea`-wrapped bar so it
        // spans the full width edge-to-edge. See `HAIRLINE_THICKNESS`.
        let hairline = DecoratedBox::with_style(
            MultiChild::empty(
                Layout::row()
                    .height(tokens::navigation::HAIRLINE_THICKNESS)
                    .flex_shrink(0.0),
            ),
            Style::default().background(nav.divider),
        );
        let bar = MultiChild::new(children![hairline, bar], Layout::column().flex_shrink(0.0));

        // Collapse the tab bar in sync with the keyboard avoidance tween.
        //
        // `AnimatedKeyboardInset` (provided by `KeyboardAvoidance` as an
        // InheritedWidget) gives us the live animated inset each frame. The
        // tab bar height animates as `max(0, TAB_BAR_HEIGHT - animated_inset)`:
        //
        //   - Keyboard fully up (inset ≥ 49): tab bar height = 0 (collapsed).
        //     KeyboardAvoidance pads by the full inset → input bar sits
        //     exactly at the keyboard's top edge.
        //   - Keyboard fully down (inset = 0): tab bar height = 49 (natural).
        //     KeyboardAvoidance pads by 0 → input bar sits above the tab bar.
        //   - During dismiss (inset animating 318→0): tab bar stays
        //     collapsed while inset > 49, then gradually reappears as inset
        //     drops below 49. The input bar tracks the keyboard's top edge
        //     the entire time — no instant layout jump.
        //
        // Before this fix, the tab bar snapped based on `target_height > 0`
        // (the keyboard's TARGET, which flips instantly). On dismiss, the tab
        // bar reappeared instantly (49px jump) while KeyboardAvoidance was
        // still animating, making the keyboard appear to "run away" downward.
        //
        // When no `KeyboardAvoidance` ancestor provides the animated inset
        // (desktop, tests, or pages without KeyboardAvoidance), fall back to
        // the keyboard's target_height — matching the old instant behavior.
        let animated_inset = ctx
            .depend_on_inherited_widget::<f32>()
            .unwrap_or_else(|| ctx.keyboard_inset().target_height);
        let collapse = (TAB_BAR_HEIGHT - animated_inset).max(0.0);

        if collapse <= 0.0 {
            // Tab bar fully collapsed — keyboard covers its area.
            MultiChild::new(
                children![WithLayout::new(
                    SafeAreaClaim::bottom(stack),
                    Layout::flex_fill()
                ),],
                Layout::default()
                    .flex_direction(FlexDirection::Column)
                    .width_percent(1.0)
                    .height_percent(1.0),
            )
            .boxed()
        } else {
            // Tab bar partially or fully visible. Animate its height from 0
            // to TAB_BAR_HEIGHT by constraining the bar's layout.
            let bar = WithLayout::new(bar, Layout::default().height(collapse).flex_shrink(0.0));
            MultiChild::new(
                children![
                    WithLayout::new(SafeAreaClaim::bottom(stack), Layout::flex_fill()),
                    bar,
                ],
                Layout::default()
                    .flex_direction(FlexDirection::Column)
                    .width_percent(1.0)
                    .height_percent(1.0),
            )
            .boxed()
        }
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
}
