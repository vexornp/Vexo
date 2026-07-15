//! Domain types and mock data for the IM app.
//!
//! No UI code lives here — only data types, the app state struct,
//! seed data, and the avatar PNG generator.

use std::collections::HashMap;
use std::rc::Rc;

use vexo::{ComponentState, Signal};
use vexo_uikit::{NavigationController, TabController};

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) struct ConvId(pub(crate) u32);

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) enum ImTab {
    Chats,
    Contacts,
    Me,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) enum ChatsRoute {
    List,
    Chat(ConvId),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Message {
    pub author: MessageAuthor,
    pub text: String,
    pub timestamp: u64, // unix seconds (mocked)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MessageAuthor {
    Them,
    Me,
}

#[derive(Clone, Debug)]
pub(crate) struct Conversation {
    pub id: ConvId,
    pub name: String,
    pub avatar_bytes: Rc<[u8]>,
    pub unread_count: u32,
    pub last_preview: String,
    pub last_timestamp: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct Contact {
    pub id: u32,
    pub name: String,
    pub avatar_bytes: Rc<[u8]>,
    pub status: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Profile {
    pub name: String,
    pub email: String,
    pub avatar_bytes: Rc<[u8]>,
}

#[derive(ComponentState)]
pub struct ImState {
    pub(crate) conversations: Vec<Conversation>,
    pub(crate) messages: Signal<HashMap<ConvId, Vec<Message>>>,
    pub(crate) contacts: Vec<Contact>,
    pub(crate) profile: Profile,
    pub(crate) tab_controller: TabController<ImTab>,
    pub(crate) chats_nav: NavigationController<ChatsRoute>,
    pub(crate) contacts_nav: NavigationController<()>,
    pub(crate) me_nav: NavigationController<()>,
}

/// Generate a 64x64 solid-color PNG for an avatar. Uses the `image` crate
/// (already a workspace dep). Returns the encoded PNG bytes.
pub(crate) fn make_avatar_png(r: u8, g: u8, b: u8) -> Rc<[u8]> {
    use image::{ImageBuffer, Rgba, RgbaImage};
    let mut img: RgbaImage = ImageBuffer::new(64, 64);
    for (_, _, pixel) in img.enumerate_pixels_mut() {
        *pixel = Rgba([r, g, b, 255]);
    }
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("PNG encode must succeed");
    bytes.into_inner().into()
}

pub(crate) fn seed() -> ImState {
    let alice_bytes = make_avatar_png(120, 180, 255);
    let bob_bytes = make_avatar_png(180, 220, 120);
    let group_bytes = make_avatar_png(220, 160, 200);
    let charlie_bytes = make_avatar_png(255, 200, 120);
    let diana_bytes = make_avatar_png(200, 200, 240);

    // "Me" gets a single, distinct avatar color used in every conversation,
    // independent of whichever chat is open.
    let me_bytes = make_avatar_png(130, 100, 200);

    let conversations = vec![
        Conversation {
            id: ConvId(1),
            name: "Alice".into(),
            avatar_bytes: alice_bytes.clone(),
            unread_count: 2,
            last_preview: "See you tomorrow!".into(),
            last_timestamp: 1732347520,
        },
        Conversation {
            id: ConvId(2),
            name: "Bob".into(),
            avatar_bytes: bob_bytes.clone(),
            unread_count: 0,
            last_preview: "Got it, thanks".into(),
            last_timestamp: 1732347050,
        },
        Conversation {
            id: ConvId(3),
            name: "Group Chat".into(),
            avatar_bytes: group_bytes.clone(),
            unread_count: 5,
            last_preview: "Charlie: sounds good".into(),
            last_timestamp: 1732346700,
        },
        Conversation {
            id: ConvId(4),
            name: "Charlie".into(),
            avatar_bytes: charlie_bytes.clone(),
            unread_count: 0,
            last_preview: "Let me check and get back".into(),
            last_timestamp: 1732345000,
        },
        Conversation {
            id: ConvId(5),
            name: "Diana".into(),
            avatar_bytes: diana_bytes.clone(),
            unread_count: 0,
            last_preview: "Meeting at 3pm".into(),
            last_timestamp: 1732340000,
        },
    ];

    let mut messages: HashMap<ConvId, Vec<Message>> = HashMap::new();
    messages.insert(
        ConvId(1),
        vec![
            Message {
                author: MessageAuthor::Them,
                text: "Hey! Are we still on for tomorrow?".into(),
                timestamp: 1732347000,
            },
            Message {
                author: MessageAuthor::Me,
                text: "Yes, definitely!".into(),
                timestamp: 1732347300,
            },
            Message {
                author: MessageAuthor::Them,
                text: "See you tomorrow!".into(),
                timestamp: 1732347520,
            },
        ],
    );
    messages.insert(
        ConvId(2),
        vec![
            Message {
                author: MessageAuthor::Them,
                text: "Did you get the file?".into(),
                timestamp: 1732346800,
            },
            Message {
                author: MessageAuthor::Me,
                text: "Got it, thanks".into(),
                timestamp: 1732347050,
            },
        ],
    );
    messages.insert(
        ConvId(3),
        vec![Message {
            author: MessageAuthor::Them,
            text: "Charlie: sounds good".into(),
            timestamp: 1732346700,
        }],
    );
    messages.insert(ConvId(4), vec![]);
    messages.insert(ConvId(5), vec![]);

    let contacts = vec![
        Contact {
            id: 1,
            name: "Alice".into(),
            avatar_bytes: alice_bytes.clone(),
            status: "Online".into(),
        },
        Contact {
            id: 2,
            name: "Bob".into(),
            avatar_bytes: bob_bytes.clone(),
            status: "Last seen 10:00".into(),
        },
        Contact {
            id: 3,
            name: "Charlie".into(),
            avatar_bytes: charlie_bytes.clone(),
            status: "Online".into(),
        },
        Contact {
            id: 4,
            name: "Diana".into(),
            avatar_bytes: diana_bytes.clone(),
            status: "Away".into(),
        },
        Contact {
            id: 5,
            name: "Eve".into(),
            avatar_bytes: make_avatar_png(180, 220, 180),
            status: "Offline".into(),
        },
        Contact {
            id: 6,
            name: "Frank".into(),
            avatar_bytes: make_avatar_png(220, 180, 140),
            status: "Online".into(),
        },
        Contact {
            id: 7,
            name: "Grace".into(),
            avatar_bytes: make_avatar_png(140, 200, 220),
            status: "Last seen yesterday".into(),
        },
        Contact {
            id: 8,
            name: "Heidi".into(),
            avatar_bytes: make_avatar_png(240, 160, 180),
            status: "Online".into(),
        },
    ];

    let profile = Profile {
        name: "Alice".into(),
        email: "alice@example.com".into(),
        avatar_bytes: me_bytes,
    };

    ImState {
        conversations,
        messages: Signal::new(messages),
        contacts,
        profile,
        tab_controller: TabController::new(ImTab::Chats),
        chats_nav: NavigationController::new(),
        contacts_nav: NavigationController::new(),
        me_nav: NavigationController::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vexo::Image;

    #[test]
    fn test_seed_has_five_conversations() {
        let s = seed();
        assert_eq!(s.conversations.len(), 5);
    }

    #[test]
    fn test_seed_messages_for_alice() {
        let s = seed();
        let map = s.messages.get_cloned();
        let msgs = map.get(&ConvId(1)).expect("Alice has messages");
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn test_seed_contacts_count() {
        let s = seed();
        assert_eq!(s.contacts.len(), 8);
    }

    #[test]
    fn test_avatar_bytes_decode() {
        let bytes = make_avatar_png(255, 0, 0);
        let img = Image::from_bytes(&bytes);
        assert!(img.is_ok(), "avatar bytes must decode as PNG");
    }

    #[test]
    fn test_tab_controller_starts_on_chats() {
        let s = seed();
        assert_eq!(s.tab_controller.current(), ImTab::Chats);
    }
}
