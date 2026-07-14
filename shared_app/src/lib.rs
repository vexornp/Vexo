//! Mocked IM UI — three-tab app shell (Chats / Contacts / Me) with
//! in-memory data, no network or persistence.

use std::any::Any;
use std::collections::HashMap;
use std::rc::Rc;

use vexo::{
    Application, Color, Column, Component, ComponentState, DecoratedContainer, Flex, Image,
    ImageData, IndexedStack, Layout, LifecycleContext, RenderContext, Row, SafeArea, ScrollView,
    Signal, Text, TextEdit, TextEditingController, Theme, ThemeData, Widget,
};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::{
    Button, ButtonVariant, NavigationController, NavigationStackView, Platform, TabBarView,
    TabController,
};

uniffi::setup_scaffolding!();

// ============================================================================
// MOCK DATA
// ============================================================================

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct ConvId(pub u32);

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
enum ImTab {
    Chats,
    Contacts,
    Me,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
enum ChatsRoute {
    List,
    Chat(ConvId),
}

#[derive(Clone, Debug, PartialEq)]
struct Message {
    author: MessageAuthor,
    text: String,
    timestamp: u64, // unix seconds (mocked)
}

#[derive(Clone, Debug, PartialEq)]
enum MessageAuthor {
    Them,
    Me,
}

#[derive(Clone, Debug)]
struct Conversation {
    id: ConvId,
    name: String,
    avatar_bytes: Rc<[u8]>,
    unread_count: u32,
    last_preview: String,
    last_timestamp: u64,
}

#[derive(Clone, Debug)]
struct Contact {
    id: u32,
    name: String,
    avatar_bytes: Rc<[u8]>,
    status: String,
}

#[derive(Clone, Debug)]
struct Profile {
    name: String,
    email: String,
    avatar_bytes: Rc<[u8]>,
}

#[derive(ComponentState)]
pub struct ImState {
    conversations: Vec<Conversation>,
    messages: Signal<HashMap<ConvId, Vec<Message>>>,
    contacts: Vec<Contact>,
    profile: Profile,
    tab_controller: TabController<ImTab>,
    chats_nav: NavigationController<ChatsRoute>,
}

/// Generate a 64x64 solid-color PNG for an avatar. Uses the `image` crate
/// (already a workspace dep). Returns the encoded PNG bytes.
fn make_avatar_png(r: u8, g: u8, b: u8) -> Rc<[u8]> {
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

fn seed() -> ImState {
    let alice_bytes = make_avatar_png(120, 180, 255);
    let bob_bytes = make_avatar_png(180, 220, 120);
    let group_bytes = make_avatar_png(220, 160, 200);
    let charlie_bytes = make_avatar_png(255, 200, 120);
    let diana_bytes = make_avatar_png(200, 200, 240);

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
        avatar_bytes: alice_bytes,
    };

    ImState {
        conversations,
        messages: Signal::new(messages),
        contacts,
        profile,
        tab_controller: TabController::new(ImTab::Chats),
        chats_nav: NavigationController::new(),
    }
}

// ============================================================================
// CONVERSATION LIST SCREEN
// ============================================================================

fn build_conversation_list_screen(
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
    ScrollView::new(list.boxed()).flex_grow(1.0).boxed()
}

fn build_conversation_row(
    conv: &Conversation,
    on_press: impl FnMut() + 'static,
) -> Box<dyn Widget> {
    let avatar = Image::from_bytes(&conv.avatar_bytes)
        .expect("avatar bytes are valid PNG")
        .width(40.0)
        .height(40.0)
        .corner_radius(20.0)
        .clip();

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

// Placeholder Application impl — full view() comes in Task 8.
impl Default for ImState {
    fn default() -> Self {
        seed()
    }
}

impl Application for ImState {
    type State = Self;

    fn new() -> Self::State {
        seed()
    }

    fn register_fonts(font_system: &mut glyphon::FontSystem) {
        vexo_fontawesome::register_fonts(font_system);
    }

    fn view(state: &mut Self::State) -> Box<dyn Widget> {
        // Placeholder — replaced in Task 8.
        Text::new("IM UI placeholder").boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_conversation_list_renders_in_pipeline() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::ThreeTreePipeline;

        let state = seed();
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

#[derive(uniffi::Object)]
pub struct MobileApp {}

#[uniffi::export]
impl MobileApp {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {}
    }

    pub fn start_app(&self) {
        let rt = vexo::run_desktop_demo::<ImState>();
        match rt {
            Ok(_) => println!("App exited normally"),
            Err(e) => println!("App exited with error: {:?}", e),
        }
    }
}
