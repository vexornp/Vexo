//! Mocked IM UI — three-tab app shell (Chats / Contacts / Me) with
//! in-memory data, no network or persistence.

use std::any::Any;
use std::collections::HashMap;
use std::rc::Rc;

use vexo::{
    AlignItems, Application, Color, Column, Component, ComponentState, DecoratedContainer, Flex,
    Image, ImageData, IndexedStack, Layout, LifecycleContext, RenderContext, Row, ScrollView,
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
    contacts_nav: NavigationController<()>,
    me_nav: NavigationController<()>,
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
        contacts_nav: NavigationController::new(),
        me_nav: NavigationController::new(),
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
    ScrollView::new(list.boxed()).flex_fill().boxed()
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

// ============================================================================
// CHAT SCREEN
// ============================================================================

struct ChatScreen {
    conv_id: ConvId,
    messages: Vec<Message>,
    avatar_bytes: Rc<[u8]>,
    nav: NavigationController<ChatsRoute>,
    on_send: Rc<dyn Fn(&str)>,
    scroll_controller: vexo::ScrollController,
}

impl Clone for ChatScreen {
    fn clone(&self) -> Self {
        Self {
            conv_id: self.conv_id.clone(),
            messages: self.messages.clone(),
            avatar_bytes: Rc::clone(&self.avatar_bytes),
            nav: self.nav.clone(),
            on_send: Rc::clone(&self.on_send),
            scroll_controller: self.scroll_controller.clone(),
        }
    }
}

#[derive(Default)]
struct ChatScreenState {
    text_controller: Option<TextEditingController>,
}

impl ChatScreenState {
    fn sync_controller(&mut self) {
        if self.text_controller.is_none() {
            let mut fs = vexo::resource::new_font_system();
            self.text_controller = Some(TextEditingController::new("", &mut fs));
        }
    }
}

impl ComponentState for ChatScreenState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        self.sync_controller();
        if let Some(tc) = self.text_controller.as_ref() {
            tc.set_dirty_callback(ctx.dirty_callback());
        }
    }
    fn on_update(&mut self, _old_widget: &dyn Any, ctx: &mut LifecycleContext) {
        if let Some(tc) = self.text_controller.as_ref() {
            tc.set_dirty_callback(ctx.dirty_callback());
        }
    }
    fn on_unmount(&mut self, _ctx: &mut LifecycleContext) {
        if let Some(tc) = self.text_controller.as_ref() {
            tc.clear_dirty_callback();
        }
        self.text_controller = None;
    }
}

impl Component for ChatScreen {
    type State = ChatScreenState;

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);

        let mut list = Flex::column().gap(8.0).padding(12.0);
        for msg in &self.messages {
            list = list.push(build_message_bubble(msg, &self.avatar_bytes));
        }

        let scroll_for_send = self.scroll_controller.clone();
        let on_send = Rc::clone(&self.on_send);
        let tc = state
            .text_controller
            .as_ref()
            .expect("text controller set on mount")
            .clone();
        let tc_for_clear = tc.clone();
        let on_send_closure = move || {
            let text = tc_for_clear.text();
            if !text.trim().is_empty() {
                on_send(&text);
                let mut fs = vexo::resource::new_font_system();
                tc_for_clear.set_text("", &mut fs);
                scroll_for_send.jump_to_bottom();
            }
        };

        let input_bar = build_input_bar(tc, on_send_closure);

        Column::new()
            .flex_fill()
            .push(
                ScrollView::new(list.boxed())
                    .controller(self.scroll_controller.clone())
                    .flex_fill(),
            )
            .push(input_bar)
            .background(theme.background)
            .boxed()
    }
}

fn build_message_bubble(msg: &Message, avatar_bytes: &Rc<[u8]>) -> Box<dyn Widget> {
    let avatar = Image::from_bytes(avatar_bytes)
        .expect("avatar bytes valid")
        .width(32.0)
        .height(32.0)
        .corner_radius(16.0)
        .clip();

    let bubble = DecoratedContainer::new(
        Text::new(msg.text.as_str())
            .with_font_size(15.0)
            .with_color(if msg.author == MessageAuthor::Me {
                Color::WHITE
            } else {
                Color::BLACK
            }),
    )
    .padding(10.0)
    .corner_radius(12.0)
    .background(if msg.author == MessageAuthor::Me {
        Color::rgb(0.0, 0.5, 1.0)
    } else {
        Color::WHITE
    })
    .border(Color::rgb(0.85, 0.85, 0.85), 1.0)
    .boxed()
    .width(220.0);

    if msg.author == MessageAuthor::Me {
        Row::new()
            .gap(8.0)
            .push(Flex::new().flex_grow(1.0))
            .push(bubble)
            .push(avatar)
            .boxed()
    } else {
        Row::new()
            .gap(8.0)
            .push(avatar)
            .push(bubble)
            .push(Flex::new().flex_grow(1.0))
            .boxed()
    }
}

fn build_input_bar(
    controller: TextEditingController,
    on_send: impl FnMut() + 'static,
) -> Box<dyn Widget> {
    Row::new()
        .gap(8.0)
        .push(TextEdit::new(controller).flex_grow(1.0))
        .push(
            Button::new("Send")
                .variant(ButtonVariant::Primary)
                .on_press(on_send),
        )
        .boxed()
        .padding(8.0)
}

// ============================================================================
// CONTACTS SCREEN
// ============================================================================

fn build_contacts_screen(contacts: Vec<Contact>) -> Box<dyn Widget> {
    let mut list = Flex::column();
    for c in &contacts {
        list = list.push(build_contact_row(c));
    }
    ScrollView::new(list.boxed()).flex_fill().boxed()
}

fn build_contact_row(c: &Contact) -> Box<dyn Widget> {
    let avatar = Image::from_bytes(&c.avatar_bytes)
        .expect("avatar bytes valid")
        .width(40.0)
        .height(40.0)
        .corner_radius(20.0)
        .clip();

    let name = Text::new(c.name.as_str())
        .with_font_size(16.0)
        .with_color(Color::BLACK);
    let status = Text::new(c.status.as_str())
        .with_font_size(13.0)
        .with_color(Color::rgb(0.5, 0.5, 0.5));

    Row::new()
        .gap(12.0)
        .push(avatar)
        .push(
            Column::new()
                .gap(2.0)
                .push(name)
                .push(status)
                .flex_grow(1.0),
        )
        .boxed()
        .padding(12.0)
}

// ============================================================================
// PROFILE SCREEN
// ============================================================================

fn build_profile_screen(profile: &Profile) -> Box<dyn Widget> {
    let avatar = Image::from_bytes(&profile.avatar_bytes)
        .expect("avatar bytes valid")
        .width(80.0)
        .height(80.0)
        .corner_radius(40.0)
        .clip();

    let name = Text::new(profile.name.as_str())
        .with_font_size(22.0)
        .with_color(Color::BLACK);
    let email = Text::new(profile.email.as_str())
        .with_font_size(14.0)
        .with_color(Color::rgb(0.5, 0.5, 0.5));

    let header = Column::new()
        .gap(4.0)
        .push(avatar)
        .push(name)
        .push(email)
        .boxed()
        .padding(24.0);

    let settings = vec!["Settings", "Notifications", "About"];
    let mut settings_list = Flex::column();
    for label in settings {
        settings_list = settings_list.push(
            Row::new()
                .gap(8.0)
                .push(
                    Text::new(label)
                        .with_font_size(16.0)
                        .with_color(Color::BLACK)
                        .flex_grow(1.0),
                )
                .push(
                    Text::new("›")
                        .with_font_size(20.0)
                        .with_color(Color::rgb(0.6, 0.6, 0.6)),
                )
                .boxed()
                .padding(16.0),
        );
    }

    Column::new()
        .push(header)
        .push(settings_list.boxed())
        .boxed()
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
        let conversations = state.conversations.clone();
        let messages_for_view = state.messages.clone();
        let nav_for_list = state.chats_nav.clone();
        let nav_for_chat = state.chats_nav.clone();
        let convs_for_chat = state.conversations.clone();
        let messages_for_chat = state.messages.clone();
        let contacts = state.contacts.clone();
        let profile = state.profile.clone();
        let tab_controller = state.tab_controller.clone();
        let contacts_nav = state.contacts_nav.clone();
        let me_nav = state.me_nav.clone();

        let tab_view = TabBarView::new(
            tab_controller,
            vec![ImTab::Chats, ImTab::Contacts, ImTab::Me],
            move |tab| match tab {
                ImTab::Chats => {
                    let chats_root =
                        build_conversation_list_screen(conversations.clone(), nav_for_list.clone());
                    let nav = nav_for_chat.clone();
                    let convs = convs_for_chat.clone();
                    let msgs = messages_for_chat.clone();
                    NavigationStackView::new(nav_for_chat.clone(), chats_root)
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
                                let nav_back = nav.clone();
                                let msgs_for_send = msgs.clone();
                                let id_for_send = id.clone();
                                ChatScreen {
                                    conv_id: id_for_send.clone(),
                                    messages: m,
                                    avatar_bytes: avatar,
                                    nav: nav_back,
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
                ImTab::Contacts => NavigationStackView::new(
                    contacts_nav.clone(),
                    build_contacts_screen(contacts.clone()),
                )
                .root_title("Contacts")
                .boxed(),
                ImTab::Me => {
                    NavigationStackView::new(me_nav.clone(), build_profile_screen(&profile))
                        .root_title("Me")
                        .boxed()
                }
            },
            |tab, is_selected| {
                let (icon, label) = match tab {
                    ImTab::Chats => (Icons::Comment, "Chats"),
                    ImTab::Contacts => (Icons::User, "Contacts"),
                    ImTab::Me => (Icons::Gear, "Me"),
                };
                let color = if is_selected {
                    Color::rgb(0.0, 0.5, 1.0)
                } else {
                    Color::rgb(0.5, 0.5, 0.5)
                };
                Column::new()
                    .gap(2.0)
                    .align(AlignItems::Center)
                    .push(Icon::new(icon).with_size(22.0).with_color(color))
                    .push(Text::new(label).with_font_size(11.0).with_color(color))
                    .boxed()
                    .padding(8.0)
            },
        );

        let _ = messages_for_view;
        tab_view.boxed()
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

    #[test]
    fn test_chat_screen_renders_messages() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::ThreeTreePipeline;

        let state = seed();
        let messages = state
            .messages
            .get_cloned()
            .get(&ConvId(1))
            .cloned()
            .unwrap();
        let avatar_bytes = state
            .conversations
            .iter()
            .find(|c| c.id == ConvId(1))
            .unwrap()
            .avatar_bytes
            .clone();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages,
            avatar_bytes,
            nav: state.chats_nav.clone(),
            on_send: Rc::new(|_| ()),
            scroll_controller: vexo::ScrollController::new(),
        }
        .boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 4,
            "expected multiple elements for 3 messages + input bar"
        );
    }

    #[test]
    fn test_chat_screen_input_bar_pinned_to_bottom_with_few_messages() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::layout::TaffyLayoutEngine;
        use vexo::{RenderObject, RenderObjectRegistry, ThreeTreePipeline};

        // Regression: with zero messages, the input bar must be pinned to
        // the bottom of the view, not floating right below the (empty)
        // message list. The ChatScreen is wrapped in a fixed-height Column
        // to simulate the IndexedStack parent in the real app. Without
        // flex_fill() on the ChatScreen's root Column, it shrinks to content
        // height and the input bar appears near the top instead of the bottom.
        let state = seed();
        let avatar_bytes = state
            .conversations
            .iter()
            .find(|c| c.id == ConvId(4))
            .unwrap()
            .avatar_bytes
            .clone();
        let chat = ChatScreen {
            conv_id: ConvId(4),
            messages: vec![], // zero messages — minimal content
            avatar_bytes,
            nav: state.chats_nav.clone(),
            on_send: Rc::new(|_| ()),
            scroll_controller: vexo::ScrollController::new(),
        };

        let view = Column::new().height(600.0).push(chat).boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        // Walk: root (outer Column) → child[0] (ProxyRO for ChatScreen)
        // → child[0] (ChatScreen root Column) → child[1] (input bar wrapper).
        fn find_child(
            ro_reg: &RenderObjectRegistry,
            id: vexo::RenderObjectKey,
            index: usize,
        ) -> Option<vexo::RenderObjectKey> {
            ro_reg.get(id)?.children().get(index).copied()
        }

        let proxy = find_child(ro_reg, root, 0).expect("proxy");
        let chat_col = find_child(ro_reg, proxy, 0).expect("chat column");
        let input_wrapper = find_child(ro_reg, chat_col, 1).expect("input bar wrapper");
        let input_bounds = ro_reg
            .get(input_wrapper)
            .and_then(|ro| ro.computed_bounds())
            .expect("input bar bounds");

        let input_bottom = input_bounds.top + input_bounds.height();
        assert!(
            input_bottom >= 599.0,
            "input bar bottom ({}) should be at the view bottom (600). \
             Top={}, Height={}",
            input_bottom,
            input_bounds.top,
            input_bounds.height()
        );
    }

    #[test]
    fn test_contacts_screen_renders_in_pipeline() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::ThreeTreePipeline;

        let state = seed();
        let view = build_contacts_screen(state.contacts.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 4,
            "expected multiple elements for 8 contacts"
        );
    }

    #[test]
    fn test_profile_screen_renders_in_pipeline() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::ThreeTreePipeline;

        let state = seed();
        let view = build_profile_screen(&state.profile);
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 2,
            "expected multiple elements for profile header + settings rows"
        );
    }

    #[test]
    fn test_full_app_view_renders_three_tabs() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::ThreeTreePipeline;

        let mut state = seed();
        let view = ImState::view(&mut state);
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 15,
            "expected many elements for full three-tab shell"
        );
    }

    #[test]
    fn test_tab_switch_to_contacts_renders_contacts_page() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::ThreeTreePipeline;

        let mut state = seed();
        state.tab_controller.switch_to(ImTab::Contacts);
        let view = ImState::view(&mut state);
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 15,
            "contacts tab should have many elements (8 contacts × several widgets each)"
        );
    }

    #[test]
    fn test_contacts_tab_tab_bar_fits_window() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::layout::TaffyLayoutEngine;
        use vexo::{RenderObject, RenderObjectRegistry, ThreeTreePipeline};

        // Regression test: switching to the Contacts tab must not push the
        // tab bar off screen on a short window (800×600). Before the fix,
        // the contacts page's min-content (8 rows × 64px = 512px + 44px nav
        // bar = 556px) propagated through the layout chain and overflowed
        // the window (556 + 58 = 614 > 600), pushing the tab bar 14px below
        // the visible area.
        let mut state = seed();
        state.tab_controller.switch_to(ImTab::Contacts);
        let view = ImState::view(&mut state);
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(800.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        // Walk the tree to find the tab bar: it's the SECOND top-level child
        // of the TabBarView column (the first is the page area). The tab bar
        // is a WithLayout wrapping a SafeArea wrapping a Flex::row with 3
        // tab items. We identify it by walking root → child → second child.
        fn find_child(
            ro_reg: &RenderObjectRegistry,
            id: vexo::RenderObjectKey,
            index: usize,
        ) -> Option<vexo::RenderObjectKey> {
            ro_reg.get(id)?.children().get(index).copied()
        }

        // root → TabBarView column → second child (tab bar)
        let tab_view = find_child(ro_reg, root, 0).expect("tab view");
        let tab_bar = find_child(ro_reg, tab_view, 1).expect("tab bar");
        let bar_bounds = ro_reg
            .get(tab_bar)
            .and_then(|ro| ro.computed_bounds())
            .expect("tab bar bounds");

        // The tab bar's bottom edge must be within the window height (600).
        let bar_bottom = bar_bounds.top + bar_bounds.height();
        assert!(
            bar_bottom <= 600.0,
            "tab bar bottom ({}) must not exceed window height (600). \
             Top={}, Height={}",
            bar_bottom,
            bar_bounds.top,
            bar_bounds.height()
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
