//! Conversation list — unified component used by both PC and Mobile.
//!
//! Both platforms render the same theme-token-aware rows. Mobile passes
//! `selected = None` (no row highlight); desktop passes the live selection
//! so the active conversation is highlighted.

use vexo::layout::JustifyContent;
use vexo::{
    children, AlignItems, AlignSelf, DecoratedBox, Layout, MultiChild, Positioned, ScrollView,
    Stack, Style, Text, ThemeData, Widget, WithLayout,
};
use vexo_uikit::theme::tokens::navigation::NavColors;

use crate::data::{ConvId, Conversation};
use crate::widgets::avatar::avatar;

/// Build the conversation list. `on_select` is invoked with the tapped
/// conversation's id. Pass `selected = None` on platforms that don't
/// highlight a row (mobile).
pub(crate) fn build_conversation_list(
    conversations: Vec<Conversation>,
    selected: Option<ConvId>,
    nav_colors: &NavColors,
    theme: &ThemeData,
    on_select: impl Fn(ConvId) + Clone + 'static,
) -> Box<dyn Widget> {
    let mut list = MultiChild::empty(Layout::column());
    for conv in &conversations {
        let is_selected = selected == Some(conv.id.clone());
        let on_select = on_select.clone();
        let id = conv.id.clone();
        let row = build_conversation_row(conv, is_selected, nav_colors, theme, move || {
            on_select(id.clone());
        });
        list = list.push(row);
    }
    // Paint a themed background behind the list so the pane isn't left
    // showing the window's white clear in dark mode. Rows are transparent
    // when unselected, so this background is what the user sees between rows.
    DecoratedBox::with_style(
        WithLayout::new(ScrollView::new(list.boxed()), Layout::flex_fill()),
        Style::default().background(nav_colors.detail_bg),
    )
    .boxed()
}

fn build_conversation_row(
    conv: &Conversation,
    is_selected: bool,
    nav_colors: &NavColors,
    theme: &ThemeData,
    on_press: impl FnMut() + 'static,
) -> Box<dyn Widget> {
    let avatar = avatar(&conv.avatar_bytes, 40.0);

    let name_color = if is_selected {
        nav_colors.selected_text
    } else {
        nav_colors.row_text
    };
    let preview_color = if is_selected {
        nav_colors.selected_text
    } else {
        nav_colors.placeholder_text
    };

    let name_text = Text::new(conv.name.as_str())
        .with_font_size(16.0)
        .with_color(name_color);
    let preview_text = Text::new(conv.last_preview.as_str())
        .with_font_size(13.0)
        .with_color(preview_color);

    let info_col = MultiChild::new(
        children![name_text, preview_text],
        Layout::column().gap(2.0).flex_grow(1.0),
    );

    let time_text = Text::new(format_timestamp(conv.last_timestamp).as_str())
        .with_font_size(12.0)
        .with_color(name_color);

    let right_col = MultiChild::new(children![time_text], Layout::column());

    let badge: Option<Box<dyn Widget>> = if conv.unread_count > 0 {
        Some(
            Positioned::new(unread_badge(conv.unread_count, theme))
                .top(-4.0)
                .right(-4.0)
                .boxed(),
        )
    } else {
        None
    };

    let avatar_with_badge = Stack::new()
        .with_layout(Layout::stack().width(40.0).height(40.0))
        .push(avatar)
        .push(badge)
        .boxed();

    let row_bg = if is_selected {
        Some(nav_colors.selected_bg)
    } else {
        None
    };

    let inner = WithLayout::new(
        MultiChild::new(
            children![avatar_with_badge, info_col, right_col],
            Layout::row().gap(12.0),
        ),
        Layout::default().padding(12.0),
    );

    if let Some(bg) = row_bg {
        DecoratedBox::with_style(inner.on_tap(on_press), Style::default().background(bg)).boxed()
    } else {
        inner.on_tap(on_press).boxed()
    }
}

fn unread_badge(count: u32, theme: &ThemeData) -> Box<dyn Widget> {
    DecoratedBox::with_style(
        WithLayout::new(
            Text::new(count.to_string())
                .with_font_size(11.0)
                .with_color(theme.on_error),
            Layout::default()
                .width(20.0)
                .height(20.0)
                .justify(JustifyContent::Center)
                .align(AlignItems::Center)
                .align_self(AlignSelf::Start)
                .flex_shrink(0.0),
        ),
        Style::default().background(theme.error).corner_radius(10.0),
    )
    .boxed()
}

fn format_timestamp(ts: u64) -> String {
    let secs = ts % 86400;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    format!("{:02}:{:02}", hours, mins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::{ThemeData, ThreeTreePipeline};
    use vexo_uikit::theme::tokens::navigation;

    #[test]
    fn test_conversation_list_renders_in_pipeline() {
        let state = crate::data::seed();
        let theme = ThemeData::light();
        let nav_colors = navigation::colors(&theme);
        let view = build_conversation_list(
            state.conversations.clone(),
            None,
            &nav_colors,
            &theme,
            |_| {},
        );
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 5,
            "expected multiple elements for 25 conversation rows"
        );
    }
}
