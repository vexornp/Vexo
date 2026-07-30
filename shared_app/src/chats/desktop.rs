//! Desktop Chats tab: two-column layout (conversation list + chat screen),
//! driven by `Signal<Option<ConvId>>` selection instead of a nav stack.

use std::collections::HashMap;
use std::rc::Rc;

use vexo::{
    children, AlignItems, Component, DecoratedBox, JustifyContent, Layout, MultiChild,
    RenderContext, ScrollController, SimpleState, Style, Text, Theme, Widget, WithLayout,
};
use vexo_uikit::theme::tokens::navigation::{
    self, NavColors, CONVERSATION_LIST_WIDTH, HAIRLINE_THICKNESS, PLACEHOLDER_FONT_SIZE,
};

use crate::chats::chat_screen::ChatScreen;
use crate::chats::conversation_list::build_conversation_list;
use crate::data::{ConvId, Conversation, Message, MessageAuthor};
use crate::widgets::titled_container::titled_container;

/// Desktop Chats page: a two-column row with the conversation list (col 2)
/// and the chat screen or empty placeholder (col 3). Reads `selected_conv`
/// and `messages` signals to determine what to render.
pub(crate) struct DesktopChatsPage {
    pub conversations: Vec<Conversation>,
    pub messages: vexo::Signal<HashMap<ConvId, Vec<Message>>>,
    pub me_avatar: Rc<[u8]>,
    pub selected_conv: vexo::Signal<Option<ConvId>>,
}

impl Clone for DesktopChatsPage {
    fn clone(&self) -> Self {
        Self {
            conversations: self.conversations.clone(),
            messages: self.messages.clone(),
            me_avatar: Rc::clone(&self.me_avatar),
            selected_conv: self.selected_conv.clone(),
        }
    }
}

impl Component for DesktopChatsPage {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let nav_colors = navigation::colors(&theme);
        let selected = self.selected_conv.get_cloned();
        let messages_map = self.messages.get_cloned();

        // --- Column 2: conversation list with title header + right hairline ---
        let selected_conv_for_select = self.selected_conv.clone();
        let list = build_conversation_list(
            self.conversations.clone(),
            selected.clone(),
            &nav_colors,
            &theme,
            move |id| {
                selected_conv_for_select.set_from(&Some(id));
            },
        );
        let col2_content = titled_container("Chats", list, &nav_colors);
        let col2 =
            build_column_with_right_hairline(col2_content, CONVERSATION_LIST_WIDTH, &nav_colors);

        // --- Column 3: chat screen or empty placeholder ---
        let col3 = match selected {
            Some(id) => {
                let msgs = messages_map.get(&id).cloned().unwrap_or_default();
                let avatar = self
                    .conversations
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| Rc::clone(&c.avatar_bytes))
                    .unwrap_or_else(|| Rc::from([0u8; 0]));
                let conv_name = self
                    .conversations
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| format!("Chat {}", id.0));

                let msgs_for_send = self.messages.clone();
                let id_for_send = id.clone();
                let msgs_for_reader = self.messages.clone();
                let id_for_reader = id.clone();
                let chat = ChatScreen {
                    conv_id: id_for_send.clone(),
                    messages: msgs,
                    messages_reader: Rc::new(move || {
                        msgs_for_reader
                            .get_cloned()
                            .get(&id_for_reader)
                            .cloned()
                            .unwrap_or_default()
                    }),
                    avatar_bytes: avatar,
                    me_avatar_bytes: self.me_avatar.clone(),
                    on_send: Rc::new(move |text: &str| {
                        let mut map = msgs_for_send.get_cloned();
                        if let Some(vec) = map.get_mut(&id_for_send) {
                            vec.push(Message {
                                author: MessageAuthor::Me,
                                text: text.to_string(),
                                timestamp: 1732348000,
                            });
                        }
                        msgs_for_send.set_from(&map);
                    }),
                    scroll_controller: ScrollController::new(),
                };

                titled_container(conv_name, chat.boxed(), &nav_colors)
            }
            None => build_empty_placeholder(&nav_colors),
        };

        MultiChild::new(
            children![col2, WithLayout::new(col3, Layout::flex_fill()),],
            Layout::row().width_percent(1.0).height_percent(1.0),
        )
        .boxed()
    }
}

/// Wrap a column's content with a right-edge hairline divider, fixing the
/// total width to `width` (content fills `width - 1px`, hairline is 1px).
fn build_column_with_right_hairline(
    content: Box<dyn Widget>,
    width: f32,
    nav_colors: &NavColors,
) -> Box<dyn Widget> {
    let hairline = DecoratedBox::with_style(
        MultiChild::empty(
            Layout::column()
                .width(HAIRLINE_THICKNESS)
                .height_percent(1.0)
                .flex_shrink(0.0),
        ),
        Style::default().background(nav_colors.divider),
    );

    WithLayout::new(
        MultiChild::new(
            children![WithLayout::new(content, Layout::flex_fill()), hairline,],
            Layout::row()
                .width(width)
                .height_percent(1.0)
                .flex_shrink(0.0),
        ),
        Layout::default(),
    )
    .boxed()
}

/// Empty placeholder for column 3 when no conversation is selected.
fn build_empty_placeholder(nav_colors: &NavColors) -> Box<dyn Widget> {
    DecoratedBox::with_style(
        WithLayout::new(
            Text::new("Select a conversation")
                .with_font_size(PLACEHOLDER_FONT_SIZE)
                .with_color(nav_colors.placeholder_text),
            Layout::flex_fill()
                .align(AlignItems::Center)
                .justify(JustifyContent::Center),
        ),
        Style::default().background(nav_colors.detail_bg),
    )
    .boxed()
}

/// Build the desktop Chats tab. Called from `view()` on desktop platform.
pub(crate) fn build_chats_tab_desktop(
    conversations: Vec<Conversation>,
    messages: vexo::Signal<HashMap<ConvId, Vec<Message>>>,
    me_avatar: Rc<[u8]>,
    selected_conv: vexo::Signal<Option<ConvId>>,
) -> Box<dyn Widget> {
    DesktopChatsPage {
        conversations,
        messages,
        me_avatar,
        selected_conv,
    }
    .boxed()
}
