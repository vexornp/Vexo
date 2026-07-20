//! Conversation list screen — the root of the Chats tab.

use vexo::layout::JustifyContent;
use vexo::{
    children, AlignItems, AlignSelf, Color, DecoratedBox, Layout, MultiChild, Positioned,
    ScrollView, Stack, Style, Text, Widget, WithLayout,
};
use vexo_uikit::NavigationController;

use crate::data::{ChatsRoute, Conversation};
use crate::widgets::avatar::avatar;

pub(crate) fn build_conversation_list_screen(
    conversations: Vec<Conversation>,
    nav: NavigationController<ChatsRoute>,
) -> Box<dyn Widget> {
    let mut list = MultiChild::empty(Layout::column());
    for conv in &conversations {
        let nav_for_row = nav.clone();
        let id = conv.id.clone();
        let row = build_conversation_row(conv, move || {
            nav_for_row.push(ChatsRoute::Chat(id.clone()));
        });
        list = list.push(row);
    }
    WithLayout::new(ScrollView::new(list.boxed()), Layout::flex_fill()).boxed()
}

fn build_conversation_row(
    conv: &Conversation,
    on_press: impl FnMut() + 'static,
) -> Box<dyn Widget> {
    let avatar = avatar(&conv.avatar_bytes, 40.0);

    let name_text = Text::new(conv.name.as_str())
        .with_font_size(16.0)
        .with_color(Color::BLACK);
    let preview_text = Text::new(conv.last_preview.as_str())
        .with_font_size(13.0)
        .with_color(Color::rgb(0.5, 0.5, 0.5));

    let info_col = MultiChild::new(
        children![name_text, preview_text],
        Layout::column().gap(2.0).flex_grow(1.0),
    );

    let time_text = Text::new(format_timestamp(conv.last_timestamp).as_str())
        .with_font_size(12.0)
        .with_color(Color::rgb(0.6, 0.6, 0.6));

    let right_col = MultiChild::new(children![time_text], Layout::column());

    let badge: Option<Box<dyn Widget>> = if conv.unread_count > 0 {
        Some(
            Positioned::new(unread_badge(conv.unread_count))
                .top(-4.0)
                .right(-4.0)
                .boxed(),
        )
    } else {
        None
    };

    let avatar_with_badge = Stack::new()
        .width(40.0)
        .height(40.0)
        .push(avatar)
        .push(badge)
        .boxed();

    WithLayout::new(
        MultiChild::new(
            children![avatar_with_badge, info_col, right_col],
            Layout::row().gap(12.0),
        ),
        Layout::default().padding(12.0),
    )
    .on_tap(on_press)
}

fn unread_badge(count: u32) -> Box<dyn Widget> {
    DecoratedBox::new(WithLayout::new(
        Text::new(count.to_string())
            .with_font_size(11.0)
            .with_color(Color::WHITE),
        Layout::default()
            .width(20.0)
            .height(20.0)
            .justify(JustifyContent::Center)
            .align(AlignItems::Center)
            .align_self(AlignSelf::Start)
            .flex_shrink(0.0),
    ))
    .style(
        Style::default()
            .background(Color::rgb(1.0, 0.0, 0.0))
            .corner_radius(10.0),
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
    use vexo::ThreeTreePipeline;

    #[test]
    fn test_conversation_list_renders_in_pipeline() {
        let state = crate::data::seed();
        let view =
            build_conversation_list_screen(state.conversations.clone(), state.chats_nav.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 5,
            "expected multiple elements for 25 conversation rows"
        );
    }
}
