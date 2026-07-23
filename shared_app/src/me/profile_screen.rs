//! Profile screen — the root of the Me tab.
//!
//! Rendered as an iOS-style grouped list: a profile header card followed by
//! grouped settings sections (rounded cards on a gray backdrop), with
//! hairline dividers between rows and colored icon tiles per row. The whole
//! thing scrolls inside a `ScrollView` whose viewport shares the grouped
//! backdrop so overscroll never flashes a different color.

use vexo::layout::JustifyContent;
use vexo::{
    children, AlignItems, Color, Component, DecoratedBox, Layout, MultiChild, RenderContext,
    ScrollView, SimpleState, Style, Text, Theme, Widget, WithLayout,
};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::theme::tokens::navigation;

use crate::data::Profile;
use crate::widgets::avatar::avatar;
use crate::widgets::theme_toggle::ThemeToggle;

// --- iOS UIKit grouped-cell metrics ------------------------------------------------

/// Rounded-card corner radius (UIKit grouped style hardcodes 10pt).
const CARD_RADIUS: f32 = 10.0;
/// Horizontal inset between a card and the screen edges.
const CARD_SIDE_MARGIN: f32 = 16.0;
/// Vertical gap between sections (and above/below the first/last card).
const SECTION_GAP: f32 = 20.0;
/// Gap between a section header label and its card.
const HEADER_TO_CARD_GAP: f32 = 8.0;
/// Row horizontal padding.
const ROW_PAD_H: f32 = 16.0;
/// Row vertical padding (~44-50pt min touch target for single-line rows).
const ROW_PAD_V: f32 = 12.0;
/// Colored icon tile metrics (UIKit grouped-cell icon size is 29pt).
const TILE_SIZE: f32 = 29.0;
const TILE_RADIUS: f32 = 7.0;
const TILE_GLYPH_SIZE: f32 = 16.0;
/// Gap between the icon tile and the row label.
const TILE_LABEL_GAP: f32 = 12.0;
/// Hairline divider thickness (logical px).
const DIVIDER_THICKNESS: f32 = 1.0;
/// Divider left inset — aligns with the row text label (after tile + gap),
/// matching iOS Settings where dividers start after the icon column.
const DIVIDER_LEFT_INSET: f32 = ROW_PAD_H + TILE_SIZE + TILE_LABEL_GAP;
/// Divider right inset — stops before the trailing accessory column.
const DIVIDER_RIGHT_INSET: f32 = ROW_PAD_H;

pub(crate) fn build_profile_screen(
    profile: &Profile,
    is_dark: vexo::Signal<bool>,
) -> Box<dyn Widget> {
    ProfileScreen {
        profile: profile.clone(),
        is_dark,
    }
    .boxed()
}

/// Profile screen component. Reads the theme via `Theme::of(ctx)` so it
/// re-themes when the ancestor `Theme` swaps light/dark.
#[derive(Clone)]
struct ProfileScreen {
    profile: Profile,
    is_dark: vexo::Signal<bool>,
}

impl Component for ProfileScreen {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);

        // The content column carries the side margins (16pt L/R) and the
        // top/bottom section gaps so cards and section headers all align to
        // the same left edge.
        let mut content = MultiChild::empty(Layout::column().padding_each(
            CARD_SIDE_MARGIN,
            CARD_SIDE_MARGIN,
            SECTION_GAP,
            SECTION_GAP,
        ));

        // Header card: avatar-left horizontal cell (no chevron, not tappable).
        content = content.push(build_card(
            vec![build_header_row(&self.profile, &theme)],
            &theme,
        ));

        // Section "Appearance": the Dark Mode toggle row.
        content = content.push(spacer(SECTION_GAP));
        content = content.push(section_header("Appearance", &theme));
        content = content.push(spacer(HEADER_TO_CARD_GAP));
        content = content.push(build_card(
            vec![build_toggle_row(self.is_dark.clone(), &theme)],
            &theme,
        ));

        // Section "General": three navigation rows (chevrons, no-op tap).
        content = content.push(spacer(SECTION_GAP));
        content = content.push(section_header("General", &theme));
        content = content.push(spacer(HEADER_TO_CARD_GAP));
        content = content.push(build_card(
            vec![
                build_nav_row(Icons::Gear, Color::from_hex(0x8E8E93FF), "Settings", &theme),
                build_nav_row(
                    Icons::Bell,
                    Color::from_hex(0xFF3B30FF),
                    "Notifications",
                    &theme,
                ),
                build_nav_row(
                    Icons::CircleInfo,
                    Color::from_hex(0x007AFFFF),
                    "About",
                    &theme,
                ),
            ],
            &theme,
        ));

        // Wrap the scroll content in a themed grouped backdrop so both the
        // viewport and the content share the gray — overscroll never flashes
        // a different color.
        let scroller = WithLayout::new(ScrollView::new(content.boxed()), Layout::flex_fill());
        DecoratedBox::with_style(
            scroller,
            Style::default().background(theme.grouped_background),
        )
        .boxed()
    }
}

/// A fixed-height empty spacer (used for inter-section / header-to-card gaps).
fn spacer(height: f32) -> Box<dyn Widget> {
    WithLayout::new(
        MultiChild::empty(Layout::row()),
        Layout::default().height(height).flex_shrink(0.0),
    )
    .boxed()
}

/// A small muted section header label, sentence-case (iOS UIKit default).
fn section_header(label: &str, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    WithLayout::new(
        Text::new(label)
            .with_font_size(13.0)
            .with_color(theme.on_surface_variant),
        Layout::default(),
    )
    .boxed()
}

/// Wrap a list of rows in a rounded card on `theme.surface`, inserting a
/// hairline divider between each pair of rows (none after the last).
fn build_card(rows: Vec<Box<dyn Widget>>, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let mut col = MultiChild::empty(Layout::column());
    let last = rows.len();
    for (i, row) in rows.into_iter().enumerate() {
        col = col.push(row);
        if i + 1 < last {
            col = col.push(divider(theme));
        }
    }
    DecoratedBox::with_style(
        col,
        Style::default()
            .background(theme.surface)
            .corner_radius(CARD_RADIUS),
    )
    .boxed()
}

/// A 1pt hairline divider, left-inset to align with the row text label and
/// right-inset before the trailing accessory column.
///
/// Uses `NavColors.divider` (the opaque, pre-composited separator color) so
/// the in-card row separators are pixel-identical to the nav chrome hairlines
/// (sidebar edge, conversation-list edge) in both light and dark mode.
fn divider(theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let nav_colors = navigation::colors(theme);
    WithLayout::new(
        DecoratedBox::with_style(
            MultiChild::empty(Layout::row().height(DIVIDER_THICKNESS).flex_shrink(0.0)),
            Style::default().background(nav_colors.divider),
        ),
        Layout::default()
            .padding_each(DIVIDER_LEFT_INSET, DIVIDER_RIGHT_INSET, 0.0, 0.0)
            .flex_shrink(0.0),
    )
    .boxed()
}

/// A colored rounded-square tile with a white glyph (iOS Settings icon style).
fn icon_tile(icon: Icons, tile_color: Color) -> Box<dyn Widget> {
    DecoratedBox::with_style(
        WithLayout::new(
            Icon::new(icon)
                .with_size(TILE_GLYPH_SIZE)
                .with_color(Color::WHITE),
            Layout::default()
                .width(TILE_SIZE)
                .height(TILE_SIZE)
                .justify(JustifyContent::Center)
                .align(AlignItems::Center)
                .flex_shrink(0.0),
        ),
        Style::default()
            .background(tile_color)
            .corner_radius(TILE_RADIUS),
    )
    .boxed()
}

/// Header row: avatar on the left, name + email stacked to the right.
/// Display-only (no chevron, not tappable).
fn build_header_row(profile: &Profile, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let avatar_widget = avatar(&profile.avatar_bytes, 56.0);
    let name = Text::new(profile.name.as_str())
        .with_font_size(17.0)
        .with_color(theme.on_background);
    let email = Text::new(profile.email.as_str())
        .with_font_size(13.0)
        .with_color(theme.on_surface_variant);
    let text_col = MultiChild::new(
        children![name, email],
        Layout::column().gap(2.0).flex_grow(1.0),
    );
    WithLayout::new(
        MultiChild::new(
            children![avatar_widget, text_col],
            Layout::row().gap(12.0).align(AlignItems::Center),
        ),
        Layout::default().padding_each(ROW_PAD_H, ROW_PAD_H, ROW_PAD_V, ROW_PAD_V),
    )
    .boxed()
}

/// The Dark Mode toggle row: icon tile + label on the left, `ThemeToggle`
/// on the right. No chevron (the toggle is the trailing control).
fn build_toggle_row(is_dark: vexo::Signal<bool>, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let dark = is_dark.get();
    let icon = if dark { Icons::Sun } else { Icons::Moon };
    let tile = icon_tile(icon, theme.primary);
    let label = WithLayout::new(
        Text::new("Dark Mode")
            .with_font_size(16.0)
            .with_color(theme.on_background),
        Layout::default().flex_grow(1.0),
    );
    WithLayout::new(
        MultiChild::new(
            children![tile, label, ThemeToggle::new(is_dark)],
            Layout::row().gap(TILE_LABEL_GAP).align(AlignItems::Center),
        ),
        Layout::default().padding_each(ROW_PAD_H, ROW_PAD_H, ROW_PAD_V, ROW_PAD_V),
    )
    .boxed()
}

/// A navigation row: icon tile + label on the left, a thin chevron on the
/// right. The chevron is purely visual (tap does nothing).
fn build_nav_row(
    icon: Icons,
    tile_color: Color,
    label: &str,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
    let tile = icon_tile(icon, tile_color);
    let label = WithLayout::new(
        Text::new(label)
            .with_font_size(16.0)
            .with_color(theme.on_background),
        Layout::default().flex_grow(1.0),
    );
    let chevron = Icon::new(Icons::ChevronRight)
        .with_size(13.0)
        .with_color(theme.on_surface_variant);
    WithLayout::new(
        MultiChild::new(
            children![tile, label, chevron],
            Layout::row().gap(TILE_LABEL_GAP).align(AlignItems::Center),
        ),
        Layout::default().padding_each(ROW_PAD_H, ROW_PAD_H, ROW_PAD_V, ROW_PAD_V),
    )
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    #[test]
    fn test_profile_screen_renders_in_pipeline() {
        let state = crate::data::seed();
        let view = build_profile_screen(&state.profile, state.is_dark.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 30,
            "expected a full grouped-list tree (header card + 2 sections + dividers + icon tiles)"
        );
    }
}
