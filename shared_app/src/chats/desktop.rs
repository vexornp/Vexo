//! Desktop Chats tab: two-column layout (conversation list + chat screen),
//! driven by `Signal<Option<ConvId>>` selection instead of a nav stack.

use std::collections::HashMap;
use std::rc::Rc;

use vexo::{
    column, row, AlignItems, Component, DecoratedBox, JustifyContent, Layout, RenderContext,
    ScrollController, SimpleState, Style, Text, Theme, Widget, WithLayout,
};
use vexo_uikit::theme::tokens::navigation::{
    self, NavColors, CONVERSATION_LIST_WIDTH, HAIRLINE_THICKNESS, PLACEHOLDER_FONT_SIZE,
};
use vexo_uikit::ContextMenuController;

use crate::chats::chat_screen::ChatScreen;
use crate::chats::conversation_list::ConversationList;
use crate::data::{
    apply_reaction, AvatarSource, ConvId, Conversation, Message, MessageAuthor, ReactionType,
};
use crate::widgets::titled_container::titled_container;

/// Desktop Chats page: a two-column row with the conversation list (col 2)
/// and the chat screen or empty placeholder (col 3). Reads `selected_conv`
/// and `messages` signals to determine what to render.
pub(crate) struct DesktopChatsPage {
    pub conversations: Vec<Conversation>,
    pub messages: vexo::Signal<HashMap<ConvId, Vec<Message>>>,
    pub me_avatar: AvatarSource,
    pub selected_conv: vexo::Signal<Option<ConvId>>,
    pub context_menu: ContextMenuController,
}

impl Clone for DesktopChatsPage {
    fn clone(&self) -> Self {
        Self {
            conversations: self.conversations.clone(),
            messages: self.messages.clone(),
            me_avatar: self.me_avatar.clone(),
            selected_conv: self.selected_conv.clone(),
            context_menu: self.context_menu.clone(),
        }
    }
}

impl Component for DesktopChatsPage {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let nav_colors = navigation::colors(&theme);
        let selected = self.selected_conv.get_cloned();

        // --- Column 2: conversation list with title header + right hairline ---
        let selected_conv_for_select = self.selected_conv.clone();
        let list = ConversationList {
            conversations: self.conversations.clone(),
            messages: self.messages.clone(),
            selected: selected.clone(),
            on_select: Rc::new(move |id| {
                selected_conv_for_select.set_from(&Some(id));
            }),
        }
        .boxed();
        let col2_content = titled_container("Chats", list, &nav_colors);
        let col2 =
            build_column_with_right_hairline(col2_content, CONVERSATION_LIST_WIDTH, &nav_colors);

        // --- Column 3: chat screen or empty placeholder ---
        let col3 = match selected {
            Some(id) => {
                let avatar = self
                    .conversations
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.avatar.clone())
                    .expect("selected conv must exist in conversations");
                let conv_name = self
                    .conversations
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| format!("Chat {}", id.0));

                let msgs_for_send = self.messages.clone();
                let id_for_send = id.clone();
                let msgs_for_react = self.messages.clone();
                let id_for_react = id.clone();
                // Pass the ROOT Signal to ChatScreen (not a derived per-conv
                // Signal). ChatScreen filters by conv_id in render(). This
                // ensures the dirty_callback is registered on the root Signal,
                // which persists across widget replacements — critical for
                // state-driven rebuilds when should_rebuild returns false.
                let messages = self.messages.clone();
                let chat = ChatScreen {
                    conv_id: id_for_send.clone(),
                    messages,
                    avatar,
                    me_avatar: self.me_avatar.clone(),
                    on_send: Rc::new(move |text: &str| {
                        let mut map = msgs_for_send.get_cloned();
                        if let Some(vec) = map.get_mut(&id_for_send) {
                            vec.push(Message {
                                author: MessageAuthor::Me,
                                text: text.to_string(),
                                timestamp: 1732348000,
                                reactions: vec![],
                            });
                        }
                        msgs_for_send.set_from(&map);
                    }),
                    on_react: Rc::new(move |index: usize, rt: ReactionType| {
                        let mut map = msgs_for_react.get_cloned();
                        if let Some(vec) = map.get_mut(&id_for_react) {
                            apply_reaction(vec, index, rt);
                        }
                        msgs_for_react.set_from(&map);
                    }),
                    scroll_controller: ScrollController::new(),
                    context_menu: self.context_menu.clone(),
                };

                titled_container(conv_name, chat.boxed(), &nav_colors)
            }
            None => build_empty_placeholder(&nav_colors),
        };

        row! {
            col2,
            WithLayout::new(col3, Layout::flex_fill()),
        }
        .width_percent(1.0)
        .height_percent(1.0)
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
        column! {}
            .width(HAIRLINE_THICKNESS)
            .height_percent(1.0)
            .flex_shrink(0.0),
        Style::default().background(nav_colors.divider),
    );

    WithLayout::new(
        row! {
            WithLayout::new(content, Layout::flex_fill()),
            hairline,
        }
        .width(width)
        .height_percent(1.0)
        .flex_shrink(0.0),
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
    me_avatar: AvatarSource,
    selected_conv: vexo::Signal<Option<ConvId>>,
    context_menu: ContextMenuController,
) -> Box<dyn Widget> {
    DesktopChatsPage {
        conversations,
        messages,
        me_avatar,
        selected_conv,
        context_menu,
    }
    .boxed()
}
