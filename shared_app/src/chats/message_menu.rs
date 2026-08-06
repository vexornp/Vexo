//! Message-bubble context menu: builder + reaction row + item rows,
//! assembled by `builder`.

use std::rc::Rc;

use vexo::{
    column, row, BoxShadow, Color, Component, ComponentState, DecoratedBox, GestureDetector,
    JustifyContent, Layout, MouseCursor, RenderContext, Signal, Style, SystemCursorKind, Text,
    Widget, WithLayout,
};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::{ContextMenuController, MenuBuilder, MenuContent, MenuMetrics};

/// Temporary Task-2 builder: splits the previous single-card layout into
/// `MenuContent { reactions, actions, metrics }` so the workspace compiles
/// against the new controller API. Task 3 refines this into the real reactions
/// pill + actions card styling. The `menu_divider` is dropped (per spec: no
/// divider between two separate cards).
pub(super) fn builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| {
        // Actions card: three item rows in a column, wrapped in the same
        // decorated surface as before (corner radius + border + shadow).
        let actions_column = column! {
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
        let actions = DecoratedBox::with_style(
            WithLayout::new(actions_column, Layout::default().min_width(200.0)),
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
        .boxed();

        MenuContent {
            reactions: reaction_row(ctrl.clone(), theme.clone()),
            actions,
            metrics: MenuMetrics {
                reactions_size: vexo::core::Size::new(150.0, 28.0),
                actions_size: vexo::core::Size::new(200.0, 108.0),
                gap: 8.0,
            },
        }
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
/// Sugar so the `column!` in `builder` stays readable.
fn close_after(ctrl: ContextMenuController, msg: &'static str) -> Rc<dyn Fn()> {
    Rc::new(move || {
        log::debug!("{}", msg);
        ctrl.close();
    })
}

/// The top reaction strip: a centered row of 6 FontAwesome reaction icons
/// (standing in for emoji — the text pipeline is monochrome-only and no emoji
/// font is loaded). Each icon is tappable: logs a message and closes the menu.
///
/// Stateless (no hover background) — the cursor still flips to pointer via
/// `.cursor(...)`. Matches the image's compact, low-affordance reaction strip.
fn reaction_row(ctrl: ContextMenuController, theme: vexo::ThemeData) -> Box<dyn Widget> {
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
                        .with_size(16.0)
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
    .gap(8.0)
    .justify(JustifyContent::Center);

    WithLayout::new(
        row,
        Layout::default().padding_each(8.0, 8.0, 4.0, 4.0), // 8h, 4v
    )
    .boxed()
}
