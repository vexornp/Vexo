//! Conversation list screen — the root of the Chats tab.

use vexo::{Color, Column, DecoratedContainer, Flex, Row, ScrollView, Text, Widget};
use vexo_uikit::NavigationController;

use crate::data::{ChatsRoute, Conversation};
use crate::widgets::avatar::avatar;

pub(crate) fn build_conversation_list_screen(
    conversations: Vec<Conversation>,
    nav: NavigationController<ChatsRoute>,
) -> Box<dyn Widget> {
    let mut list = Flex::column();
    for conv in &conversations {
        let nav_for_row = nav.clone();
        let id = conv.id.clone();
        let row = build_conversation_row(conv, move || {
            nav_for_row.push(ChatsRoute::Chat(id.clone()));
        });
        list = list.push(row);
    }
    ScrollView::new(list.boxed()).flex_fill().boxed()
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

    let info_col = Column::new().gap(2.0).push(name_text).push(preview_text);

    let time_text = Text::new(format_timestamp(conv.last_timestamp).as_str())
        .with_font_size(12.0)
        .with_color(Color::rgb(0.6, 0.6, 0.6));

    let right_col = if conv.unread_count > 0 {
        let badge = DecoratedContainer::new(
            Text::new(conv.unread_count.to_string())
                .with_font_size(11.0)
                .with_color(Color::WHITE),
        )
        .background(Color::rgb(0.0, 0.5, 1.0))
        .corner_radius(10.0)
        .boxed();
        Column::new().gap(4.0).push(time_text).push(badge)
    } else {
        Column::new().push(time_text)
    };

    Row::new()
        .gap(12.0)
        .push(avatar)
        .push(info_col.flex_grow(1.0))
        .push(right_col)
        .boxed()
        .padding(12.0)
        .on_press(on_press)
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
            "expected multiple elements for 5 conversation rows"
        );
    }
}
