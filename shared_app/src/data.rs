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
    pub(crate) selected_conv: Signal<Option<ConvId>>,
    /// Dark/light mode. Toggled by `ThemeToggle`. Root `view()` reads this to
    /// pick `ThemeData::dark()`/`light()` and wraps the tree in `Theme::new`.
    pub(crate) is_dark: Signal<bool>,
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

    // Extra avatar palettes for the additional mock conversations used to
    // exercise ScrollView behavior. Kept distinct so rows are visually
    // distinguishable while scrolling.
    let eve_bytes = make_avatar_png(180, 220, 180);
    let frank_bytes = make_avatar_png(220, 180, 140);
    let grace_bytes = make_avatar_png(140, 200, 220);
    let heidi_bytes = make_avatar_png(240, 160, 180);
    let ivan_bytes = make_avatar_png(200, 240, 160);
    let judy_bytes = make_avatar_png(160, 160, 200);
    let mallory_bytes = make_avatar_png(240, 220, 140);
    let niaj_bytes = make_avatar_png(180, 140, 220);
    let oscar_bytes = make_avatar_png(140, 240, 200);
    let peggy_bytes = make_avatar_png(240, 180, 200);
    let trent_bytes = make_avatar_png(180, 200, 240);
    let walter_bytes = make_avatar_png(220, 140, 160);
    let wendy_bytes = make_avatar_png(160, 220, 140);
    let zara_bytes = make_avatar_png(200, 140, 240);
    let yuki_bytes = make_avatar_png(140, 180, 140);
    let xander_bytes = make_avatar_png(240, 200, 180);

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
        Conversation {
            id: ConvId(6),
            name: "Eve".into(),
            avatar_bytes: eve_bytes.clone(),
            unread_count: 1,
            last_preview: "Did you see the news?".into(),
            last_timestamp: 1732339000,
        },
        Conversation {
            id: ConvId(7),
            name: "Frank".into(),
            avatar_bytes: frank_bytes.clone(),
            unread_count: 0,
            last_preview: "Lunch on Friday?".into(),
            last_timestamp: 1732338000,
        },
        Conversation {
            id: ConvId(8),
            name: "Grace".into(),
            avatar_bytes: grace_bytes.clone(),
            unread_count: 3,
            last_preview: "I sent you the docs".into(),
            last_timestamp: 1732337000,
        },
        Conversation {
            id: ConvId(9),
            name: "Heidi".into(),
            avatar_bytes: heidi_bytes.clone(),
            unread_count: 0,
            last_preview: "Thanks for the help!".into(),
            last_timestamp: 1732336000,
        },
        Conversation {
            id: ConvId(10),
            name: "Ivan".into(),
            avatar_bytes: ivan_bytes.clone(),
            unread_count: 0,
            last_preview: "On my way".into(),
            last_timestamp: 1732335000,
        },
        Conversation {
            id: ConvId(11),
            name: "Judy".into(),
            avatar_bytes: judy_bytes.clone(),
            unread_count: 12,
            last_preview: "Charlie: please review the PR".into(),
            last_timestamp: 1732334000,
        },
        Conversation {
            id: ConvId(12),
            name: "Design Team".into(),
            avatar_bytes: mallory_bytes.clone(),
            unread_count: 0,
            last_preview: "Mallory: new mockups are ready".into(),
            last_timestamp: 1732333000,
        },
        Conversation {
            id: ConvId(13),
            name: "Niaj".into(),
            avatar_bytes: niaj_bytes.clone(),
            unread_count: 0,
            last_preview: "Sounds good to me".into(),
            last_timestamp: 1732332000,
        },
        Conversation {
            id: ConvId(14),
            name: "Oscar".into(),
            avatar_bytes: oscar_bytes.clone(),
            unread_count: 4,
            last_preview: "Can we move the call?".into(),
            last_timestamp: 1732331000,
        },
        Conversation {
            id: ConvId(15),
            name: "Peggy".into(),
            avatar_bytes: peggy_bytes.clone(),
            unread_count: 0,
            last_preview: "Happy birthday! 🎉".into(),
            last_timestamp: 1732330000,
        },
        Conversation {
            id: ConvId(16),
            name: "Trent".into(),
            avatar_bytes: trent_bytes.clone(),
            unread_count: 0,
            last_preview: "Reviewing now, give me 10".into(),
            last_timestamp: 1732329000,
        },
        Conversation {
            id: ConvId(17),
            name: "Walter".into(),
            avatar_bytes: walter_bytes.clone(),
            unread_count: 2,
            last_preview: "Where are you?".into(),
            last_timestamp: 1732328000,
        },
        Conversation {
            id: ConvId(18),
            name: "Wendy".into(),
            avatar_bytes: wendy_bytes.clone(),
            unread_count: 0,
            last_preview: "OK great, talk soon".into(),
            last_timestamp: 1732327000,
        },
        Conversation {
            id: ConvId(19),
            name: "Xander".into(),
            avatar_bytes: xander_bytes.clone(),
            unread_count: 0,
            last_preview: "I'll handle it tomorrow".into(),
            last_timestamp: 1732326000,
        },
        Conversation {
            id: ConvId(20),
            name: "Yuki".into(),
            avatar_bytes: yuki_bytes.clone(),
            unread_count: 7,
            last_preview: "Charlie: deploy is green".into(),
            last_timestamp: 1732325000,
        },
        Conversation {
            id: ConvId(21),
            name: "Zara".into(),
            avatar_bytes: zara_bytes.clone(),
            unread_count: 0,
            last_preview: "Forwarded you the email".into(),
            last_timestamp: 1732310000,
        },
        Conversation {
            id: ConvId(22),
            name: "Weekend Hike".into(),
            avatar_bytes: grace_bytes.clone(),
            unread_count: 0,
            last_preview: "Grace: weather looks great".into(),
            last_timestamp: 1732300000,
        },
        Conversation {
            id: ConvId(23),
            name: "Mom".into(),
            avatar_bytes: peggy_bytes.clone(),
            unread_count: 1,
            last_preview: "Call me when you can".into(),
            last_timestamp: 1732200000,
        },
        Conversation {
            id: ConvId(24),
            name: "Book Club".into(),
            avatar_bytes: mallory_bytes.clone(),
            unread_count: 0,
            last_preview: "Next chapter is ch. 7".into(),
            last_timestamp: 1732100000,
        },
        Conversation {
            id: ConvId(25),
            name: "Support".into(),
            avatar_bytes: oscar_bytes.clone(),
            unread_count: 0,
            last_preview: "Your ticket #4821 was resolved".into(),
            last_timestamp: 1732000000,
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
    for id in 6..=25 {
        messages.insert(ConvId(id), vec![]);
    }

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
        selected_conv: Signal::new(None),
        is_dark: Signal::new(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vexo::Image;

    #[test]
    fn test_seed_has_twenty_five_conversations() {
        let s = seed();
        assert_eq!(s.conversations.len(), 25);
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
