//! Conversation list — unified component used by both PC and Mobile.
//!
//! Both platforms render the same theme-token-aware rows. Mobile passes
//! `selected = None` (no row highlight); desktop passes the live selection
//! so the active conversation is highlighted.
//!
//! `ConversationList` is a `Component` that subscribes to the `messages`
//! Signal via `ctx.signal_value`, deriving each row's preview/timestamp from
//! the latest message (with seed fallback). This state-driven rebuild
//! bypasses `should_rebuild` gates (TabBarView, NavigationStackView) so the
//! list refreshes on mobile even while the chat screen is pushed.

use std::collections::HashMap;
use std::rc::Rc;

use vexo::layout::JustifyContent;
use vexo::{
    children, AlignItems, AlignSelf, Component, DecoratedBox, ImageData, Layout, MultiChild,
    Positioned, RenderContext, ScrollView, Signal, SimpleState, Stack, Style, Text, Theme,
    ThemeData, Widget, WithLayout,
};
use vexo_uikit::theme::tokens::navigation::{self, NavColors};

use crate::data::{ConvId, Conversation, Message};
use crate::widgets::avatar::avatar;

pub(crate) struct ConversationList {
    pub(crate) conversations: Vec<Conversation>,
    pub(crate) messages: Signal<HashMap<ConvId, Vec<Message>>>,
    pub(crate) selected: Option<ConvId>,
    pub(crate) on_select: Rc<dyn Fn(ConvId)>,
}

impl Clone for ConversationList {
    fn clone(&self) -> Self {
        Self {
            conversations: self.conversations.clone(),
            messages: self.messages.clone(),
            selected: self.selected.clone(),
            on_select: Rc::clone(&self.on_select),
        }
    }
}

impl Component for ConversationList {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let nav_colors = navigation::colors(&theme);
        let messages = ctx.signal_value(&self.messages);

        let mut list = MultiChild::empty(Layout::column());
        for conv in &self.conversations {
            let is_selected = self.selected == Some(conv.id.clone());
            let on_select = Rc::clone(&self.on_select);
            let id = conv.id.clone();
            let (preview, timestamp) = latest_preview(conv, &messages);
            let row = build_conversation_row(
                conv,
                &preview,
                timestamp,
                is_selected,
                &nav_colors,
                &theme,
                move || {
                    on_select(id.clone());
                },
            );
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
}

/// Derive the (preview, timestamp) for a conversation's row from the latest
/// message in the map, falling back to the conversation's seed values when
/// no messages exist (or the conversation is absent from the map).
fn latest_preview(conv: &Conversation, messages: &HashMap<ConvId, Vec<Message>>) -> (String, u64) {
    messages
        .get(&conv.id)
        .and_then(|v| v.last())
        .map(|m| (m.text.clone(), m.timestamp))
        .unwrap_or_else(|| (conv.last_preview.clone(), conv.last_timestamp))
}

fn build_conversation_row(
    conv: &Conversation,
    preview: &str,
    timestamp: u64,
    is_selected: bool,
    nav_colors: &NavColors,
    theme: &ThemeData,
    on_press: impl FnMut() + 'static,
) -> Box<dyn Widget> {
    let avatar = avatar(
        ImageData::from_bytes(&conv.avatar_bytes).expect("avatar bytes are valid PNG"),
        40.0,
    );

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
    let preview_text = Text::new(preview)
        .with_font_size(13.0)
        .with_color(preview_color)
        .with_max_lines(1);

    let info_col = MultiChild::new(
        children![name_text, preview_text],
        Layout::column().gap(2.0).flex_grow(1.0),
    );

    let time_text = Text::new(format_timestamp(timestamp).as_str())
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
    use crate::data::{Message, MessageAuthor};
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    #[test]
    fn test_conversation_list_renders_in_pipeline() {
        let state = crate::data::seed();
        let view = ConversationList {
            conversations: state.conversations.clone(),
            messages: state.messages.clone(),
            selected: None,
            on_select: Rc::new(|_| {}),
        }
        .boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 5,
            "expected multiple elements for 25 conversation rows"
        );
    }

    #[test]
    fn test_latest_preview_uses_last_message() {
        let state = crate::data::seed();
        let conv = state
            .conversations
            .iter()
            .find(|c| c.id == ConvId(1))
            .unwrap();
        let mut messages = state.messages.get_cloned();
        messages.get_mut(&ConvId(1)).unwrap().push(Message {
            author: MessageAuthor::Me,
            text: "New latest message".into(),
            timestamp: 1732399999,
        });
        let (preview, ts) = latest_preview(conv, &messages);
        assert_eq!(preview, "New latest message");
        assert_eq!(ts, 1732399999);
    }

    #[test]
    fn test_latest_preview_falls_back_for_empty_vec() {
        let state = crate::data::seed();
        let conv = state
            .conversations
            .iter()
            .find(|c| c.id == ConvId(4))
            .unwrap();
        let messages = state.messages.get_cloned();
        let (preview, ts) = latest_preview(conv, &messages);
        assert_eq!(preview, conv.last_preview);
        assert_eq!(ts, conv.last_timestamp);
    }

    #[test]
    fn test_latest_preview_falls_back_when_absent_from_map() {
        let state = crate::data::seed();
        let conv = state
            .conversations
            .iter()
            .find(|c| c.id == ConvId(1))
            .unwrap();
        let empty_map: HashMap<ConvId, Vec<Message>> = HashMap::new();
        let (preview, ts) = latest_preview(conv, &empty_map);
        assert_eq!(preview, conv.last_preview);
        assert_eq!(ts, conv.last_timestamp);
    }
}
