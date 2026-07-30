//! Chats tab: conversation list + chat screen, wired into a NavigationStackView.

pub(crate) mod chat_screen;
pub(crate) mod conversation_list;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod desktop;

use std::collections::HashMap;
use std::rc::Rc;

use vexo::{Component, RenderContext, Signal, SimpleState, Text, Theme, Widget};
use vexo_uikit::theme::tokens::navigation;
use vexo_uikit::{NavigationController, NavigationStackView};

use crate::chats::conversation_list::build_conversation_list;
use crate::data::{ChatsRoute, ConvId, Conversation, Message, MessageAuthor};

/// Mobile Chats page. Renders the conversation list via the unified
/// theme-token-aware builder, then wraps it in a `NavigationStackView` so
/// tapping a row pushes the chat screen.
struct MobileChatsPage {
    conversations: Vec<Conversation>,
    nav: NavigationController<ChatsRoute>,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,
    me_avatar: Rc<[u8]>,
}

impl Clone for MobileChatsPage {
    fn clone(&self) -> Self {
        Self {
            conversations: self.conversations.clone(),
            nav: self.nav.clone(),
            messages: self.messages.clone(),
            me_avatar: Rc::clone(&self.me_avatar),
        }
    }
}

impl Component for MobileChatsPage {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let nav_colors = navigation::colors(&theme);
        let nav_for_select = self.nav.clone();
        let chats_root = build_conversation_list(
            self.conversations.clone(),
            None,
            &nav_colors,
            &theme,
            move |id| {
                nav_for_select.push(ChatsRoute::Chat(id));
            },
        );

        let convs = self.conversations.clone();
        let msgs = self.messages.clone();
        let me_avatar_for_dest = self.me_avatar.clone();
        let nav = self.nav.clone();

        NavigationStackView::new(nav, chats_root)
            .root_title("Chats")
            .title(|d| match d {
                ChatsRoute::Chat(id) => format!("Chat {}", id.0),
                _ => String::new(),
            })
            .destination(move |d| match d {
                ChatsRoute::Chat(id) => {
                    let m = msgs.get_cloned().get(id).cloned().unwrap_or_default();
                    let avatar = convs
                        .iter()
                        .find(|c| c.id == *id)
                        .map(|c| Rc::clone(&c.avatar_bytes))
                        .unwrap_or_else(|| Rc::from([0u8; 0]));
                    let msgs_for_send = msgs.clone();
                    let id_for_send = id.clone();
                    let msgs_for_reader = msgs.clone();
                    let id_for_reader = id.clone();
                    chat_screen::ChatScreen {
                        conv_id: id_for_send.clone(),
                        messages: m,
                        messages_reader: Rc::new(move || {
                            msgs_for_reader
                                .get_cloned()
                                .get(&id_for_reader)
                                .cloned()
                                .unwrap_or_default()
                        }),
                        avatar_bytes: avatar,
                        me_avatar_bytes: me_avatar_for_dest.clone(),
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
                        scroll_controller: vexo::ScrollController::new(),
                    }
                    .boxed()
                }
                _ => Text::new("").boxed(),
            })
            .boxed()
    }
}

/// Build the mobile Chats tab. Called from `view()` on mobile platform.
pub(crate) fn build_chats_tab(
    conversations: Vec<Conversation>,
    nav: NavigationController<ChatsRoute>,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,
    me_avatar: Rc<[u8]>,
) -> Box<dyn Widget> {
    MobileChatsPage {
        conversations,
        nav,
        messages,
        me_avatar,
    }
    .boxed()
}
