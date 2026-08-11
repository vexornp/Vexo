//! Message-bubble context menu: builder + reactions pill + actions card,
//! assembled by `builder` into a `MenuContent { reactions, actions, metrics }`.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use vexo::{
    column, row, AlignItems, AnimationController, BoxShadow, Color, Component, ComponentState,
    DecoratedBox, GestureDetector, JustifyContent, Layout, LifecycleContext, MouseCursor,
    RenderContext, Signal, SimpleState, SpringDescription, SpringSimulation, Style,
    SystemCursorKind, Text, Widget, WithLayout,
};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::{ContextMenuController, MenuBuilder, MenuContent, MenuMetrics};

use crate::data::ReactionType;

/// The visual mapping for a reaction: `(FA icon, semantic color)`. The single
/// translation point from domain `ReactionType` to UI glyph + color — so
/// swapping a glyph (e.g. `Like` → a different thumbs-up variant) is a one-line
/// change here, not a rename across the domain layer. Both the reactions pill
/// (`reaction_pill`) and the bubble chip (`reaction_chip`) read from this so
/// the pill and the chip always agree on what a reaction looks like.
///
/// Colors mirror the emoji-semantic palette the inline tuple array used before
/// the `ReactionType` refactor (Q11): blue Like, red Love, yellow Haha, etc.
pub(super) fn reaction_visual(rt: ReactionType) -> (Icons, Color) {
    match rt {
        ReactionType::Like => (Icons::ThumbsUp, Color::rgb(0.10, 0.46, 0.82)),
        ReactionType::Love => (Icons::Heart, Color::rgb(0.91, 0.22, 0.21)),
        ReactionType::Haha => (Icons::FaceLaugh, Color::rgb(0.98, 0.75, 0.18)),
        ReactionType::Wow => (Icons::FaceSurprise, Color::rgb(0.98, 0.55, 0.11)),
        ReactionType::Sad => (Icons::FaceSadTear, Color::rgb(0.36, 0.42, 0.75)),
        ReactionType::Angry => (Icons::FaceAngry, Color::rgb(0.78, 0.16, 0.16)),
    }
}

/// All reaction variants in pill order. The pill iterates this so the visual
/// order lives next to the visual mapping (not in the builder loop).
pub(super) const ALL_REACTIONS: &[ReactionType] = &[
    ReactionType::Like,
    ReactionType::Love,
    ReactionType::Haha,
    ReactionType::Wow,
    ReactionType::Sad,
    ReactionType::Angry,
];

/// Build the `MenuContent` for a message-bubble context menu: a reactions
/// pill (6 FA icons in an 18px-radius pill) above an actions card (3 MenuRows
/// in a 12px-radius card). The host (`ContextMenu`) positions `actions` at
/// the bubble bounds; `reactions` is rendered by a later task. `metrics`
/// holds the laid-out sizes used for positioning + scale-about-center anchors
/// — verified by `test_metrics_match_real_sizes` (Task 8).
///
/// `index` is the message's position in the conversation's `Vec<Message>`,
/// captured so each `ReactionIcon`'s `on_tap` can report *which* message was
/// reacted via `on_react(index, rt)`. `on_react` is the host-provided toggle
/// callback (wired in `mod.rs`/`desktop.rs`); the menu just forwards the tap
/// and closes — toggle logic lives in `data::apply_reaction`.
pub(super) fn builder(index: usize, on_react: Rc<dyn Fn(usize, ReactionType)>) -> MenuBuilder {
    MenuBuilder::new(move |ctrl, theme| MenuContent {
        reactions: reaction_pill(ctrl.clone(), theme.clone(), index, on_react.clone()),
        actions: actions_card(ctrl.clone(), theme.clone()),
        metrics: MenuMetrics {
            // Verified by `test_metrics_match_real_sizes`: the real laid-out
            // sizes (read back from the DecoratedBox render objects after
            // layout) are 222×40 for the pill and 200×98 for the card. The
            // pill is 6 reaction circles × 30px (18px FA icon in a fixed
            // 30×30 circled cell — see `ReactionIcon`) + 5 gaps × 6px + 6px
            // outer padding each side = 222px wide; 30px circle height + 5px
            // top/bottom padding = 40px tall. The circle's `Layout::width/
            // height(30)` forces an exact 30px cell, so (unlike the old
            // padding-based layout) the icon's line height no longer inflates
            // the pill height. The card is 3 `MenuRow`s; each row's icon and
            // label share a 14px font size (line height ~17px) plus 8px
            // top/bottom padding → ~33px per row × 3 = 98px.
            reactions_size: vexo::core::Size::new(222.0, 40.0),
            actions_size: vexo::core::Size::new(200.0, 98.0),
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
        // Icon and label share one color + one font size so the glyph and the
        // text read at the same visual weight. `destructive` recolors both to
        // `error` (used for Delete).
        let color = if self.destructive {
            self.theme.error
        } else {
            self.theme.on_surface
        };

        let on_enter = state.hovered.clone();
        let on_exit = state.hovered.clone();
        let on_tap = self.on_tap.clone();

        let content = WithLayout::new(
            row! {
                Icon::new(self.icon).with_size(14.0).with_color(color),
                Text::new(self.label).with_font_size(14.0).with_color(color),
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

/// Build the card chrome `Style` for a menu surface (pill or card).
/// In light mode: surface bg + outline border + black drop shadow.
/// In dark mode:  surface bg + outline border only — the border already
/// provides separation against the dark backdrop, and a black shadow
/// would be invisible against near-black surface anyway (Material dark-
/// mode guidance: de-emphasize shadows).
fn menu_card_style(theme: &vexo::ThemeData, corner_radius: f32) -> Style {
    let style = Style::default()
        .corner_radius(corner_radius)
        .background(theme.surface)
        .border(theme.outline, 1.0);
    if theme.is_dark() {
        style
    } else {
        style.shadow(
            BoxShadow::new(Color::BLACK.with_alpha(0.20))
                .blur(12.0)
                .offset(0.0, 4.0),
        )
    }
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

/// A display-only reaction badge shown below a message bubble when the user
/// has reacted. A 20px circle with a 12px FA icon — the same motif as
/// `ReactionIcon` at badge scale, minus the spring/hover/tap (it's feedback,
/// not an input — to remove a reaction the user re-opens the menu and toggles,
/// per Q4/Q9).
///
/// Built by `reaction_chip` (which resolves the `ReactionType` → `(Icons, Color)`
/// via `reaction_visual`) and placed below the bubble by `assemble_row` in
/// `chat_screen.rs`. Stateless: `SimpleState<()>` — no `AnimationController`,
/// no lifecycle hooks, no reconciliation overhead beyond the default.
#[derive(Clone)]
struct ReactionChip {
    icon: Icons,
    icon_color: Color,
    bg_color: Color,
}

impl Component for ReactionChip {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        DecoratedBox::with_style(
            WithLayout::new(
                Icon::new(self.icon)
                    .with_size(12.0)
                    .with_color(self.icon_color),
                Layout::default()
                    .width(20.0)
                    .height(20.0)
                    .justify(JustifyContent::Center)
                    .align(AlignItems::Center)
                    .flex_shrink(0.0),
            ),
            // Light tint of the semantic color over surface (15%) — a badge
            // bg the full-strength icon reads against. Opaque (alpha=1.0) so
            // it's classified as a Phase-1 quad (behind text) — same opacity
            // strategy as `ReactionIcon`'s circle, avoiding the
            // white-square-on-glyph artifact.
            Style::default()
                .background(self.bg_color)
                .corner_radius(10.0),
        )
        .boxed()
    }
}

/// Build a single `ReactionChip` widget for `rt` against `theme`. Used by
/// `reaction_chip_row` per entry; not called directly outside this module.
///
/// The bg is a 15% tint of the semantic color over `theme.surface` — light
/// enough that the full-strength icon glyph reads clearly against it.
fn reaction_chip(rt: ReactionType, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let (icon, color) = reaction_visual(rt);
    let bg_color = Color::lerp(theme.surface, color, 0.15);
    ReactionChip {
        icon,
        icon_color: color,
        bg_color,
    }
    .boxed()
}

/// Build the row of `ReactionChip` widgets for `rts` against `theme`, laid
/// out left-to-right with a small gap. Returns `None` when `rts` is empty so
/// the caller can drop the chip slot entirely (`assemble_row`'s `chip:
/// Option<Box<dyn Widget>>`).
///
/// Each reaction in `rts` becomes its own 20px circle chip — multiple
/// reactions accumulate (Slack/Discord-style) rather than replacing each
/// other. Order matches `rts` (click order, preserved by
/// `data::apply_reaction`).
pub(super) fn reaction_chip_row(
    rts: &[ReactionType],
    theme: &vexo::ThemeData,
) -> Option<Box<dyn Widget>> {
    if rts.is_empty() {
        return None;
    }
    // Pad the chip row horizontally by `BUBBLE_CONTENT_PADDING` so chip icons
    // align with the bubble's text content edges (not the bubble border):
    //  - "them" (column aligned Start): row's left = bubble left, so chips
    //    start at bubble_left + padding = text content left.
    //  - "me" (column aligned End): row's right = bubble right, so chips end
    //    at bubble_right - padding = text content right.
    // The padding on the trailing side is inert inset space inside the row
    // box (the row is sized to its content and aligned to one edge). Value
    // must stay in sync with `build_bubble`'s padding — hence the shared
    // const.
    let row = row! {
        for rt in rts {
            reaction_chip(*rt, theme)
        }
    }
    .gap(4.0)
    .padding_each(
        crate::chats::chat_screen::BUBBLE_CONTENT_PADDING,
        crate::chats::chat_screen::BUBBLE_CONTENT_PADDING,
        0.0,
        0.0,
    );
    Some(row.boxed())
}

/// The reactions pill: a compact row of 6 FA icons in a pill-shaped
/// (18px radius) DecoratedBox. Each icon is a `ReactionIcon` — tappable
/// (forwards to `on_react(index, rt)` + closes the menu) with a circled
/// hover background and a spring scale-up animation. See `ReactionIcon` for
/// the hover/scale mechanics.
///
/// `index` + `on_react` flow from `builder(index, on_react)` → each icon's
/// `on_tap`, so a tap reports *which* message was reacted. The pill itself
/// stays stateless w.r.t. the current reaction (no highlight — Q4); toggle
/// semantics live in `data::apply_reaction`, invoked by the host's
/// `on_react` closure.
fn reaction_pill(
    ctrl: ContextMenuController,
    theme: vexo::ThemeData,
    index: usize,
    on_react: Rc<dyn Fn(usize, ReactionType)>,
) -> Box<dyn Widget> {
    let row = row! {
        for rt in ALL_REACTIONS {
            let (icon, color) = reaction_visual(*rt);
            let ctrl = ctrl.clone();
            let on_react = on_react.clone();
            let rt = *rt;
            ReactionIcon {
                theme: theme.clone(),
                icon,
                color,
                on_tap: Rc::new(move || {
                    on_react(index, rt);
                    ctrl.close();
                }),
            }
        }
    }
    .gap(6.0)
    .justify(JustifyContent::Center);

    DecoratedBox::with_style(
        WithLayout::new(row, Layout::default().padding_each(6.0, 6.0, 5.0, 5.0)),
        menu_card_style(&theme, 18.0),
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
        menu_card_style(&theme, 12.0),
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
        controller.show(
            vexo::core::Point::new(150.0, 280.0),
            builder(0, Rc::new(|_, _| ())),
        );
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
        let content = builder(0, Rc::new(|_, _| ()))(&controller, &theme);
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

    /// Walk the render tree and return the key of the first
    /// `DecoratedBoxRenderObject` whose `Style.corner_radius` matches `radius`.
    /// Sibling of `find_decorated_box_by_corner_radius` (which returns bounds);
    /// this variant returns the key so the caller can read `style()`.
    fn find_decorated_box_key_by_corner_radius(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        radius: f32,
    ) -> Option<RenderObjectKey> {
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
                return Some(key);
            }
            for &child in ro.children() {
                if let Some(k) = find_decorated_box_key_by_corner_radius(reg, child, radius) {
                    return Some(k);
                }
            }
        }
        None
    }

    /// Regression test for dark-theme inheritance (2026-08-11 design). Wraps
    /// the menu host in a DARK `Theme` using the production wrap order
    /// (`Theme::new(dark, ContextMenu::new(...))`), opens the menu, settles
    /// the open spring, then asserts the actions card:
    ///   1. background == dark.surface  (menu inherited the dark theme)
    ///   2. shadows is empty            (menu_card_style drops shadow in dark)
    /// Catches: builder ignoring the theme arg, ContextMenu not reading
    /// `Theme::of`, `menu_card_style` always adding a shadow.
    #[test]
    fn test_message_menu_inherits_dark_theme() {
        let controller = ContextMenuController::new();
        let host = ContextMenu::new(vexo::Text::new("content"), controller.clone());

        // Production wrap order: Theme OUTSIDE ContextMenu (the fixed order
        // from Task 1). The test builds its own tree, so it validates the
        // builder→theme contract, not the app.rs wrap order per se.
        let dark_theme = vexo::ThemeData::dark();
        let host = vexo::Theme::new(dark_theme.clone(), host);

        // Wrap in MediaQuery (so edge-detection reads a real window size) —
        // mirrors production + test_metrics_match_real_sizes.
        let mq_data = vexo::MediaQueryData {
            size: Size::new(400.0, 600.0),
            ..vexo::MediaQueryData::all_zero()
        };
        let host = vexo::MediaQuery::new(mq_data, host);

        let ticker = Arc::new(AnimationTicker::new());
        let mut pipeline = ThreeTreePipeline::new(ticker.clone());
        pipeline.update(host.boxed());

        let mut engine = TaffyLayoutEngine::new();
        // Register FontAwesome so the pill's FA icons shape with real glyphs
        // (mirrors test_metrics_match_real_sizes).
        let mut font_system = new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Open the menu in the middle of the screen — plenty of room.
        controller.show(
            vexo::core::Point::new(150.0, 280.0),
            builder(0, Rc::new(|_, _| ())),
        );
        pipeline.perform_rebuilds();

        // Settle the open spring (v→1.0) so the card is at full scale and
        // its laid-out size/style reflects the real content.
        std::thread::sleep(std::time::Duration::from_millis(700));
        ticker.tick();
        pipeline.drain_dirty_to_build_owner();
        pipeline.perform_rebuilds();
        pipeline.layout(Size::new(400.0, 600.0), &mut engine, &mut font_system);

        // Find the actions card (corner_radius=12) and read its style.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let card_key = find_decorated_box_key_by_corner_radius(ro_reg, root, 12.0)
            .expect("actions card (corner_radius=12) should exist when menu is open");
        let card_ro = ro_reg
            .get(card_key)
            .and_then(|ro| ro.as_any().downcast_ref::<DecoratedBoxRenderObject>())
            .expect("downcast DecoratedBoxRenderObject");
        let style = card_ro.style();

        // 1. Background must be the dark theme's surface — proves the menu
        //    inherited the dark theme through ContextMenu → builder. If this
        //    is white (0xFFFFFFFF) the menu fell back to ThemeData::light()
        //    — check the Theme/ContextMenu wrap order in app.rs.
        assert_eq!(
            style.background,
            Some(dark_theme.surface),
            "actions card background should be dark theme surface",
        );

        // 2. No shadow in dark mode — menu_card_style drops it (a black
        //    shadow is invisible against near-black dark surface anyway;
        //    Material dark-mode guidance: de-emphasize shadows).
        assert!(
            style.shadows.is_empty(),
            "actions card should have no shadow in dark mode; \
             menu_card_style must branch on theme.is_dark()",
        );
    }
}
