//! Message-bubble context menu: builder + reactions pill + actions card,
//! assembled by `builder` into a `MenuContent { reactions, actions, metrics }`.

use std::rc::Rc;

use vexo::{
    column, row, BoxShadow, Color, Component, ComponentState, DecoratedBox, GestureDetector,
    JustifyContent, Layout, MouseCursor, RenderContext, Signal, Style, SystemCursorKind, Text,
    Widget, WithLayout,
};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::{ContextMenuController, MenuBuilder, MenuContent, MenuMetrics};

/// Build the `MenuContent` for a message-bubble context menu: a reactions
/// pill (6 FA icons in an 18px-radius pill) above an actions card (3 MenuRows
/// in a 12px-radius card). The host (`ContextMenu`) positions `actions` at
/// the bubble bounds; `reactions` is rendered by a later task. `metrics`
/// holds the placeholder sizes used for positioning — Task 8 tunes them.
pub(super) fn builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| MenuContent {
        reactions: reaction_pill(ctrl.clone(), theme.clone()),
        actions: actions_card(ctrl.clone(), theme.clone()),
        metrics: MenuMetrics {
            reactions_size: vexo::core::Size::new(150.0, 28.0),
            actions_size: vexo::core::Size::new(200.0, 108.0),
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

/// The reactions pill: a compact row of 6 FA icons in a pill-shaped
/// (18px radius) DecoratedBox. Each icon is tappable: logs a message and
/// closes the menu. Stateless (no hover background) — the cursor still flips
/// to pointer via `.cursor(...)`.
fn reaction_pill(ctrl: ContextMenuController, theme: vexo::ThemeData) -> Box<dyn Widget> {
    // (icon, log message) pairs. The log messages use emoji codepoints in the
    // string literal for grep-ability — they're just log text, never rendered.
    let reactions: [(Icons, &str); 6] = [
        (Icons::ThumbsUp, "context menu: thumbsup"),
        (Icons::Heart, "context menu: heart"),
        (Icons::FaceLaugh, "context menu: laugh"),
        (Icons::FaceSurprise, "context menu: surprise"),
        (Icons::FaceSadTear, "context menu: sad"),
        (Icons::FaceAngry, "context menu: angry"),
    ];

    let row = row! {
        for (icon, msg) in reactions {
            let ctrl = ctrl.clone();
            GestureDetector::new(
                WithLayout::new(
                    Icon::new(icon)
                        .with_size(18.0)
                        .with_color(theme.on_surface_variant),
                    Layout::default().padding(6.0),
                )
                .boxed()
                .cursor(MouseCursor::System(SystemCursorKind::Pointer)),
            )
            .on_tap(move || {
                log::debug!("{}", msg);
                ctrl.close();
            })
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
