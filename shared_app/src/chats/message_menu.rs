//! Message-bubble context menu: builder + reactions pill + actions card,
//! assembled by `builder` into a `MenuContent { reactions, actions, metrics }`.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use vexo::{
    column, row, AlignItems, AnimationController, BoxShadow, Color, Component, ComponentState,
    DecoratedBox, GestureDetector, JustifyContent, Layout, LifecycleContext, MouseCursor,
    RenderContext, Signal, SpringDescription, SpringSimulation, Style, SystemCursorKind, Text,
    Widget, WithLayout,
};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::{ContextMenuController, MenuBuilder, MenuContent, MenuMetrics};

/// Build the `MenuContent` for a message-bubble context menu: a reactions
/// pill (6 FA icons in an 18px-radius pill) above an actions card (3 MenuRows
/// in a 12px-radius card). The host (`ContextMenu`) positions `actions` at
/// the bubble bounds; `reactions` is rendered by a later task. `metrics`
/// holds the laid-out sizes used for positioning + scale-about-center anchors
/// — verified by `test_metrics_match_real_sizes` (Task 8).
pub(super) fn builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| MenuContent {
        reactions: reaction_pill(ctrl.clone(), theme.clone()),
        actions: actions_card(ctrl.clone(), theme.clone()),
        metrics: MenuMetrics {
            // Verified by `test_metrics_match_real_sizes`: the real laid-out
            // sizes (read back from the DecoratedBox render objects after
            // layout) are 222×40 for the pill and 200×134 for the card. The
            // pill is 6 reaction circles × 30px (18px FA icon in a fixed
            // 30×30 circled cell — see `ReactionIcon`) + 5 gaps × 6px + 6px
            // outer padding each side = 222px wide; 30px circle height + 5px
            // top/bottom padding = 40px tall. The circle's `Layout::width/
            // height(30)` forces an exact 30px cell, so (unlike the old
            // padding-based layout) the icon's line height no longer inflates
            // the pill height. The card is taller because each row's text
            // line height (~28px) plus 8px top/bottom padding → ~44px per
            // row × 3 = 134px.
            reactions_size: vexo::core::Size::new(222.0, 40.0),
            actions_size: vexo::core::Size::new(200.0, 134.0),
            gap: 8.0,
        },
    })
}

/// State for `MenuRow` — tracks hover via a reactive `Signal<bool>`.
/// Auto-wired by `#[derive(ComponentState)]` (mirrors `ButtonState` in
/// `vexo_uikit/src/button.rs`).
#[derive(ComponentState, Default)]
struct MenuRowState {
    hovered: Signal<bool>,
}

/// One context-menu item row: leading FontAwesome icon + label, with a
/// hover-tint background and a tap handler that logs + closes the menu.
///
/// `destructive: true` renders icon + text in `theme.error` (used for Delete).
/// `theme` is a snapshot taken in the builder at render time (the builder runs
/// inside `ContextMenu::render`, so this is the live theme).
#[derive(Clone)]
struct MenuRow {
    theme: vexo::ThemeData,
    icon: Icons,
    label: &'static str,
    destructive: bool,
    on_tap: Rc<dyn Fn()>,
}

impl Component for MenuRow {
    type State = MenuRowState;

    fn render(&self, state: &mut MenuRowState, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let hovered = state.hovered.get();
        // ~8% primary wash over surface — slightly stronger than nav's
        // ROW_HOVER_TINT (0.95) so it reads inside the compact menu.
        let row_hover_bg = Color::lerp(self.theme.primary, self.theme.surface, 0.92);
        let bg = if hovered {
            row_hover_bg
        } else {
            Color::TRANSPARENT
        };
        let (icon_color, text_color) = if self.destructive {
            (self.theme.error, self.theme.error)
        } else {
            (self.theme.on_surface_variant, self.theme.on_surface)
        };

        let on_enter = state.hovered.clone();
        let on_exit = state.hovered.clone();
        let on_tap = self.on_tap.clone();

        let content = WithLayout::new(
            row! {
                Icon::new(self.icon).with_size(14.0).with_color(icon_color),
                Text::new(self.label).with_color(text_color),
            }
            .gap(10.0),
            // padding_each(left, right, top, bottom) — 12h, 8v.
            Layout::default()
                .padding_each(12.0, 12.0, 8.0, 8.0)
                .min_width(200.0),
        );

        let decorated =
            DecoratedBox::with_style(content, Style::default().background(bg).corner_radius(6.0));

        // Fluent .on_enter/.on_exit/.cursor wrap `decorated` in MouseRegion(s)
        // (pub(crate) — callers use the fluent Widget trait methods, exactly
        // like Button does in `vexo_uikit/src/button.rs`). Each returns
        // Box<dyn Widget>, so chain them, then wrap the result in
        // GestureDetector for the tap.
        let hovered_area = decorated
            .on_enter(move || on_enter.set(true))
            .on_exit(move || on_exit.set(false))
            .cursor(MouseCursor::System(SystemCursorKind::Pointer));

        GestureDetector::new(hovered_area)
            .on_tap(move || on_tap())
            .boxed()
    }
}

/// Build the `on_tap` closure for a menu item: log `msg` and close the menu.
/// Sugar so the `column!` in `actions_card` stays readable.
fn close_after(ctrl: ContextMenuController, msg: &'static str) -> Rc<dyn Fn()> {
    Rc::new(move || {
        log::debug!("{}", msg);
        ctrl.close();
    })
}

/// State for `ReactionIcon` — owns the hover-scale spring controller.
///
/// The controller is shared (`Rc<RefCell<...>>`) so the `on_enter`/`on_exit`
/// closures can drive it directly via `animate_with`, mirroring how
/// `ContextMenuController::show` starts its spring from an event handler
/// rather than a lifecycle hook. Wired in `on_mount`/`on_update` (ticker +
/// dirty callback), advanced in `on_tick`, stopped in `on_unmount` — the
/// standard three-hook floor (see `SlideTransitionState` for the minimal
/// owned-controller shape, `ContextMenuHostState` for the shared-cell shape).
struct ReactionIconState {
    controller: Rc<RefCell<AnimationController>>,
}

impl Default for ReactionIconState {
    fn default() -> Self {
        // Placeholder duration; the controller is driven by spring sims, not
        // the tween path, so this duration is never sampled. Matches the
        // `SlideTransitionState::default` pattern.
        Self {
            controller: Rc::new(RefCell::new(AnimationController::new(
                Duration::from_millis(300),
            ))),
        }
    }
}

impl ComponentState for ReactionIconState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        let mut ctrl = self.controller.borrow_mut();
        ctrl.set_ticker(ctx.animation_ticker().clone());
        ctrl.set_dirty_callback(ctx.dirty_callback());
    }

    fn on_update(&mut self, _old_widget: &dyn Any, ctx: &mut LifecycleContext) {
        // Re-wire on parent-cascade rebuilds: the dirty callback is
        // element-id-keyed and changes when the widget struct is recreated,
        // even though our shared controller cell persists. Mirrors
        // `ContextMenuHostState::on_update`.
        let mut ctrl = self.controller.borrow_mut();
        ctrl.set_ticker(ctx.animation_ticker().clone());
        ctrl.set_dirty_callback(ctx.dirty_callback());
    }

    fn on_tick(&mut self, now: Instant) {
        self.controller.borrow_mut().advance(now);
    }

    fn on_unmount(&mut self, _ctx: &mut LifecycleContext) {
        self.controller.borrow_mut().stop();
    }
}

/// One reaction icon in the reactions pill: a FontAwesome glyph in a 30×30
/// circled background that scales up (spring) and fades in a neutral surface
/// wash on hover. Tap logs the reaction and closes the menu.
///
/// A single `AnimationController` value `v` (0→1) drives both the icon scale
/// (`Transform::scale(1.0 + v*0.30, ...)`) and the circle background color
/// (`Color::lerp(surface, hover_tint, v)`), so the circle appears as the icon
/// grows — one spring, two effects, perfectly in sync.
///
/// **Circle background opacity strategy**: Both `surface` and `hover_tint`
/// are opaque (alpha=1.0), so the lerp result is always opaque → the circle
/// is always classified as an opaque quad → rendered in Phase 1 (behind
/// text). This is critical: if the circle were semi-transparent (alpha < 1.0)
/// during the animation, it would be classified as a TransparentQuad and
/// rendered in Phase 3 (ON TOP of text), covering the icon glyph with a
/// white square. At v=0, `bg == surface` (same as the pill background) so
/// the circle is invisible; at v=1, `bg == hover_tint` (slightly darker).
///
/// **Icon scale strategy**: `Transform::scale` is paint-only (layout
/// pass-through, cell footprint stays 30×30). The command processor scales
/// `font_size` under the transform (see `command_processor.rs` Text handler),
/// so the glyph actually grows. The painter re-anchors every paint_transform
/// to the child's bounds center, so the scale pivots about the icon's own
/// center.
///
/// The circle is sized 30×30 with `corner_radius(15)` (half the size) → a
/// perfect circle, matching the unread-badge pattern in `conversation_list`.
#[derive(Clone)]
struct ReactionIcon {
    theme: vexo::ThemeData,
    icon: Icons,
    color: Color,
    on_tap: Rc<dyn Fn()>,
}

impl Component for ReactionIcon {
    type State = ReactionIconState;

    fn render(&self, state: &mut ReactionIconState, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let v = state.controller.borrow().value();
        let scale = 1.0 + (v as f32) * 0.30;
        // ~6% on_surface wash over surface — a subtle neutral gray circle that
        // reads on the pill's surface background. The icon already carries the
        // semantic color, so the circle stays neutral (unlike MenuRow's tint,
        // which uses primary since the row has no colored glyph).
        let hover_tint = Color::lerp(self.theme.surface, self.theme.on_surface, 0.06);
        // Lerp between surface (invisible at v=0) and hover_tint (visible at
        // v=1). Both are opaque → always Phase 1 → behind text. See the widget
        // doc comment for why alpha animation would cause a white square.
        let bg = Color::lerp(self.theme.surface, hover_tint, v);

        let on_enter_controller = state.controller.clone();
        let on_exit_controller = state.controller.clone();
        let on_tap = self.on_tap.clone();

        let circle = DecoratedBox::with_style(
            WithLayout::new(
                Icon::new(self.icon).with_size(18.0).with_color(self.color),
                Layout::default()
                    .width(30.0)
                    .height(30.0)
                    .justify(JustifyContent::Center)
                    .align(AlignItems::Center)
                    .flex_shrink(0.0),
            ),
            Style::default().background(bg).corner_radius(15.0),
        );

        // Paint-only scale; layout stays 30×30. The command processor scales
        // font_size under the transform, so the glyph actually grows.
        let scaled = circle.scale(scale, scale);

        let interactive = scaled
            .on_enter(move || {
                // `from = current value` lets an interrupted hover-out reverse
                // smoothly from wherever it was, no snap. Same shape as
                // `ContextMenuController::show`'s spring start.
                let from = on_enter_controller.borrow().value();
                on_enter_controller
                    .borrow_mut()
                    .animate_with(Box::new(SpringSimulation::new(
                        SpringDescription::ios(340.0, 1.0),
                        from,
                        1.0,
                        0.0,
                    )));
            })
            .on_exit(move || {
                let from = on_exit_controller.borrow().value();
                on_exit_controller
                    .borrow_mut()
                    .animate_with(Box::new(SpringSimulation::new(
                        SpringDescription::ios(340.0, 1.0),
                        from,
                        0.0,
                        0.0,
                    )));
            })
            .cursor(MouseCursor::System(SystemCursorKind::Pointer));

        GestureDetector::new(interactive)
            .on_tap(move || on_tap())
            .boxed()
    }
}

/// The reactions pill: a compact row of 6 FA icons in a pill-shaped
/// (18px radius) DecoratedBox. Each icon is a `ReactionIcon` — tappable
/// (logs + closes the menu) with a circled hover background and a spring
/// scale-up animation. See `ReactionIcon` for the hover/scale mechanics.
fn reaction_pill(ctrl: ContextMenuController, theme: vexo::ThemeData) -> Box<dyn Widget> {
    // (icon, color, log message) pairs. Each icon gets a unique emoji-semantic
    // color so the glyph reads at a glance. The log messages use emoji
    // codepoints in the string literal for grep-ability — they're just log
    // text, never rendered.
    let reactions: [(Icons, Color, &str); 6] = [
        (
            Icons::ThumbsUp,
            Color::rgb(0.10, 0.46, 0.82),
            "context menu: thumbsup",
        ),
        (
            Icons::Heart,
            Color::rgb(0.91, 0.22, 0.21),
            "context menu: heart",
        ),
        (
            Icons::FaceLaugh,
            Color::rgb(0.98, 0.75, 0.18),
            "context menu: laugh",
        ),
        (
            Icons::FaceSurprise,
            Color::rgb(0.98, 0.55, 0.11),
            "context menu: surprise",
        ),
        (
            Icons::FaceSadTear,
            Color::rgb(0.36, 0.42, 0.75),
            "context menu: sad",
        ),
        (
            Icons::FaceAngry,
            Color::rgb(0.78, 0.16, 0.16),
            "context menu: angry",
        ),
    ];

    let row = row! {
        for (icon, color, msg) in reactions {
            let ctrl = ctrl.clone();
            ReactionIcon {
                theme: theme.clone(),
                icon,
                color,
                on_tap: Rc::new(move || {
                    log::debug!("{}", msg);
                    ctrl.close();
                }),
            }
        }
    }
    .gap(6.0)
    .justify(JustifyContent::Center);

    DecoratedBox::with_style(
        WithLayout::new(row, Layout::default().padding_each(6.0, 6.0, 5.0, 5.0)),
        Style::default()
            .corner_radius(18.0)
            .background(theme.surface)
            .border(theme.outline, 1.0)
            .shadow(
                BoxShadow::new(Color::BLACK.with_alpha(0.20))
                    .blur(12.0)
                    .offset(0.0, 4.0),
            ),
    )
    .boxed()
}

/// The actions card: Copy/Reply/Delete rows in a 12px-radius DecoratedBox
/// with the standard surface/outline/shadow styling.
fn actions_card(ctrl: ContextMenuController, theme: vexo::ThemeData) -> Box<dyn Widget> {
    let column = column! {
        MenuRow {
            theme: theme.clone(),
            icon: Icons::Copy,
            label: "Copy",
            destructive: false,
            on_tap: close_after(ctrl.clone(), "context menu: Copy"),
        },
        MenuRow {
            theme: theme.clone(),
            icon: Icons::Reply,
            label: "Reply",
            destructive: false,
            on_tap: close_after(ctrl.clone(), "context menu: Reply"),
        },
        MenuRow {
            theme: theme.clone(),
            icon: Icons::Trash,
            label: "Delete",
            destructive: true,
            on_tap: close_after(ctrl.clone(), "context menu: Delete"),
        },
    };

    DecoratedBox::with_style(
        WithLayout::new(column, Layout::default().min_width(200.0)),
        Style::default()
            .corner_radius(12.0)
            .background(theme.surface)
            .border(theme.outline, 1.0)
            .shadow(
                BoxShadow::new(Color::BLACK.with_alpha(0.20))
                    .blur(12.0)
                    .offset(0.0, 4.0),
            ),
    )
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::core::{Logical, Size};
    use vexo::layout::TaffyLayoutEngine;
    use vexo::render_objects::DecoratedBoxRenderObject;
    use vexo::resource::new_font_system;
    use vexo::{RenderObjectKey, RenderObjectRegistry, ThreeTreePipeline};
    use vexo_uikit::{ContextMenu, ContextMenuController};

    /// Walk the render tree and return the `computed_bounds` of the first
    /// `DecoratedBoxRenderObject` whose `Style.corner_radius` matches `radius`.
    /// Used by `test_metrics_match_real_sizes` to locate the reactions pill
    /// (radius 18) and actions card (radius 12) and read back their laid-out
    /// sizes.
    fn find_decorated_box_by_corner_radius(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        radius: f32,
    ) -> Option<vexo::core::Bounds<Logical>> {
        if let Some(ro) = reg.get(key) {
            let matches = ro
                .as_any()
                .downcast_ref::<DecoratedBoxRenderObject>()
                .map_or(false, |d| {
                    d.style()
                        .corner_radius
                        .as_ref()
                        .map_or(false, |cr| (cr.radius - radius).abs() < 0.01)
                });
            if matches {
                if let Some(b) = ro.computed_bounds() {
                    return Some(b);
                }
            }
            for &child in ro.children() {
                if let Some(b) = find_decorated_box_by_corner_radius(reg, child, radius) {
                    return Some(b);
                }
            }
        }
        None
    }

    /// Task 8 — metrics verification. Opens the menu with the REAL
    /// `message_menu::builder()` (which produces the FA-icon reactions pill
    /// and the 3-row actions card), settles the spring so the cards are at
    /// full scale (v=1.0 → scale=0.8+v*0.2=1.0), lays out, then reads back
    /// the actual laid-out sizes of the pill (DecoratedBox corner_radius=18)
    /// and card (corner_radius=12) from the render tree.
    ///
    /// Asserts the real sizes match the `MenuMetrics` constants in `builder()`
    /// within ~15px. If this test fails, update the constants in `builder()`
    /// to match the real sizes printed in the assertion message.
    #[test]
    fn test_metrics_match_real_sizes() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());

        // Wrap in MediaQuery (so edge-detection reads a real window size) and
        // Theme (so the builder reads the live theme) — mirrors production.
        let mq_data = vexo::MediaQueryData {
            size: Size::new(400.0, 600.0),
            ..vexo::MediaQueryData::all_zero()
        };
        let host = vexo::MediaQuery::new(mq_data, host);
        let host = vexo::Theme::new(vexo::ThemeData::light(), host);

        let ticker = Arc::new(AnimationTicker::new());
        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        // Register FontAwesome so the pill's FA icons shape with real glyphs
        // (otherwise icons fall back to Roboto and the pill width is wrong).
        let mut font_system = new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);

        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Bubble in the middle of the screen — plenty of room above + below
        // so neither card flips (default layout: pill above, card below).
        // Click point in the middle of the screen — plenty of room above +
        // below so neither card flips (default layout: pill above, card below).
        controller.show(vexo::core::Point::new(150.0, 280.0), builder());
        pipeline.perform_rebuilds();

        // Settle the open spring (v→1.0, phase→Open) so the cards are at full
        // scale and their laid-out sizes reflect the real content sizes.
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Read the MenuMetrics constants directly from `builder()` so this
        // test always compares real sizes against the current constants (not
        // a hardcoded copy that can drift).
        let theme = vexo::ThemeData::light();
        let content = builder()(&controller, &theme);
        let expected_pill: Size<Logical> = content.metrics.reactions_size;
        let expected_card: Size<Logical> = content.metrics.actions_size;

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        let pill_bounds = find_decorated_box_by_corner_radius(ro_reg, root, 18.0).expect(
            "reactions pill DecoratedBox (corner_radius=18) should exist when menu is open",
        );
        let card_bounds = find_decorated_box_by_corner_radius(ro_reg, root, 12.0)
            .expect("actions card DecoratedBox (corner_radius=12) should exist when menu is open");

        let real_pill: Size<Logical> = Size::new(pill_bounds.width(), pill_bounds.height());
        let real_card: Size<Logical> = Size::new(card_bounds.width(), card_bounds.height());

        let tol = 15.0;
        let pill_dx = (real_pill.width - expected_pill.width).abs();
        let pill_dy = (real_pill.height - expected_pill.height).abs();
        let card_dx = (real_card.width - expected_card.width).abs();
        let card_dy = (real_card.height - expected_card.height).abs();

        // Informational: print real sizes for tuning.
        eprintln!(
            "metrics: pill real={:?} expected={:?} (dx={:.1}, dy={:.1}); \
             card real={:?} expected={:?} (dx={:.1}, dy={:.1})",
            real_pill, expected_pill, pill_dx, pill_dy, real_card, expected_card, card_dx, card_dy,
        );

        assert!(
            pill_dx <= tol && pill_dy <= tol,
            "reactions pill size mismatch: real={:?} expected={:?} (dx={:.1}, dy={:.1}, tol={}); \
             update MenuMetrics.reactions_size in builder() to match",
            real_pill,
            expected_pill,
            pill_dx,
            pill_dy,
            tol,
        );
        assert!(
            card_dx <= tol && card_dy <= tol,
            "actions card size mismatch: real={:?} expected={:?} (dx={:.1}, dy={:.1}, tol={}); \
             update MenuMetrics.actions_size in builder() to match",
            real_card,
            expected_card,
            card_dx,
            card_dy,
            tol,
        );
    }
}
