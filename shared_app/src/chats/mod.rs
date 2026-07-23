//! Chats tab: conversation list + chat screen, wired into a NavigationStackView.

pub(crate) mod chat_screen;
pub(crate) mod conversation_list;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod desktop;

use std::collections::HashMap;
use std::rc::Rc;

use vexo::{Signal, Text, Widget};
use vexo_uikit::{NavigationController, NavigationStackView};

use crate::data::{ChatsRoute, ConvId, Conversation, Message, MessageAuthor};

pub(crate) fn build_chats_tab(
    conversations: Vec<Conversation>,
    nav: NavigationController<ChatsRoute>,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,
    me_avatar: Rc<[u8]>,
) -> Box<dyn Widget> {
    let chats_root =
        conversation_list::build_conversation_list_screen(conversations.clone(), nav.clone());

    let convs = conversations.clone();
    let msgs = messages.clone();
    let me_avatar_for_dest = me_avatar.clone();

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
                chat_screen::ChatScreen {
                    conv_id: id_for_send.clone(),
                    messages: m,
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
