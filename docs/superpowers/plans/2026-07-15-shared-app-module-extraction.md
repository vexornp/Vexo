# shared_app Module Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split `shared_app/src/lib.rs` (1061 lines) into feature modules (data, widgets, chats, contacts, me, app) with zero behavior change, plus a deduped avatar builder and per-feature test co-location.

**Architecture:** Feature folders per tab (`chats/`, `contacts/`, `me/`), a shared `widgets/` module, a `data.rs` for domain types and seed data, and an `app.rs` for the `Application` impl and `MobileApp` UniFFI export. `lib.rs` becomes a thin root with `mod` declarations and `pub use` re-exports.

**Tech Stack:** Rust, vexo (framework), vexo_uikit (NavigationController, TabBarView, Button), vexo_fontawesome (icons), uniffi 0.30 (iOS FFI), image (PNG avatar generation)

## Global Constraints

- **No behavior change.** Same widgets render the same way from the same data. All 13 tests pass unmodified.
- **Public API preserved:** `shared_app::ImState` and `shared_app::MobileApp` remain `pub`. All other items are `pub(crate)`.
- **`ImState` fields become `pub(crate)`** so `app.rs` can access them in `view()`.
- **Commit cadence:** One single commit at the very end (Task 7, Step 5). No per-task commits.
- **Never run `cargo run -p desktop_demo`** — per CLAUDE.md, the assistant must not run the GUI. Only `cargo build` and `cargo test`.
- **`uniffi::setup_scaffolding!()`** stays in `lib.rs` (crate root) — required by UniFFI.
- **`#[derive(ComponentState)]`** on `ImState` generates code via field access (`self.field_name`) — it expands at the definition site in `data.rs`, so `ImState`'s fields can be `pub(crate)` and the derive still works.

---

### Task 1: Extract `data.rs` (domain types, ImState, seed, avatar PNG generator)

**Files:**
- Create: `shared_app/src/data.rs`
- Modify: `shared_app/src/lib.rs` — remove moved code, add `mod data;` and `use` imports

**Interfaces:**
- Produces: `ImState` (pub), `ConvId` (pub(crate)), `ImTab` (pub(crate)), `ChatsRoute` (pub(crate)), `Message` (pub(crate)), `MessageAuthor` (pub(crate)), `Conversation` (pub(crate)), `Contact` (pub(crate)), `Profile` (pub(crate)), `seed()` (pub(crate)), `make_avatar_png()` (pub(crate))

- [ ] **Step 1: Create `shared_app/src/data.rs`**

```rust
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
```

- [ ] **Step 2: Edit `lib.rs` — remove moved code, add module + imports**

In `lib.rs`, make these changes:

1. After the `uniffi::setup_scaffolding!();` line (line 19), add module declaration and data imports:

```rust
mod data;
use data::*;
```

2. Delete the entire `// ====... MOCK DATA ...====` section — from the `#[derive(Hash, Eq, PartialEq, Clone, Debug)]` line that defines `ConvId` (line 25) through the closing brace of `fn seed()` (line 274). This includes: `ConvId`, `ImTab`, `ChatsRoute`, `Message`, `MessageAuthor`, `Conversation`, `Contact`, `Profile`, `ImState`, `make_avatar_png`, and `seed`.

3. The remaining `lib.rs` code (screens, Application impl, tests, MobileApp) must still compile. It references `ConvId`, `ImState`, `Conversation`, etc. — these now come from `use data::*;`.

4. Remove `ComponentState` and `Signal` from the `use vexo::{...}` import in `lib.rs` if they are no longer used directly in `lib.rs` (they are used in `data.rs` now). Keep any imports still needed by the remaining code in `lib.rs`. Specifically, check: `Component` and `ComponentState` are used by `ChatScreen` (still in `lib.rs` at this point), so keep them. `Signal` is used in the `Application::view()` closure (still in `lib.rs`), so keep it. The `use data::*;` glob brings in `ConvId`, `ImState`, etc.

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build -p shared_app`
Expected: compiles with no errors. If there are "unused import" warnings for items now only used in `data.rs`, remove them from `lib.rs`'s `use vexo::{...}`.

- [ ] **Step 4: Run tests to verify behavior**

Run: `cargo test -p shared_app`
Expected: all 13 tests pass. The tests use `use super::*;` which picks up `use data::*;` from `lib.rs`, so `seed()`, `ConvId`, `ImState`, etc. are all in scope.

---

### Task 2: Extract `widgets/avatar.rs` (deduped avatar builder)

**Files:**
- Create: `shared_app/src/widgets/mod.rs`
- Create: `shared_app/src/widgets/avatar.rs`
- Modify: `shared_app/src/lib.rs` — add `mod widgets;`, replace 5 inline avatar constructions with `avatar()` calls

**Interfaces:**
- Consumes: `vexo::{Image, Widget}`, `std::rc::Rc`
- Produces: `avatar(bytes: &Rc<[u8]>, diameter: f32) -> Box<dyn Widget>`

- [ ] **Step 1: Create `shared_app/src/widgets/mod.rs`**

```rust
//! Cross-feature reusable widgets.

pub(crate) mod avatar;
```

- [ ] **Step 2: Create `shared_app/src/widgets/avatar.rs`**

```rust
//! Deduped circular avatar builder.
//!
//! Replaces the 5 inline `Image::from_bytes(...).width(d).height(d)
//! .corner_radius(d/2).clip()` blocks that were copy-pasted across
//! all four screens.

use std::rc::Rc;

use vexo::{Image, Widget};

/// Build a circular avatar widget from PNG bytes.
///
/// `diameter` sets both width and height; corner radius is half the
/// diameter for a perfect circle; `clip()` rounds the visible corners.
pub(crate) fn avatar(bytes: &Rc<[u8]>, diameter: f32) -> Box<dyn Widget> {
    Image::from_bytes(bytes)
        .expect("avatar bytes are valid PNG")
        .width(diameter)
        .height(diameter)
        .corner_radius(diameter / 2.0)
        .clip()
}
```

- [ ] **Step 3: Add `mod widgets;` to `lib.rs`**

After the `mod data;` line added in Task 1, add:

```rust
mod widgets;
use widgets::avatar::avatar;
```

- [ ] **Step 4: Replace 5 inline avatar constructions in `lib.rs`**

Replace each of the following blocks with the corresponding `avatar()` call:

**Site 1 — `build_conversation_row` (conversation list, diameter 40):**

Replace:
```rust
    let avatar = Image::from_bytes(&conv.avatar_bytes)
        .expect("avatar bytes are valid PNG")
        .width(40.0)
        .height(40.0)
        .corner_radius(20.0)
        .clip();
```
With:
```rust
    let avatar = avatar(&conv.avatar_bytes, 40.0);
```

**Site 2 — `build_message_bubble`, me avatar (diameter 32):**

Replace:
```rust
        let me_avatar = Image::from_bytes(me_avatar_bytes)
            .expect("avatar bytes valid")
            .width(32.0)
            .height(32.0)
            .corner_radius(16.0)
            .clip();
```
With:
```rust
        let me_avatar = avatar(me_avatar_bytes, 32.0);
```

**Site 3 — `build_message_bubble`, them avatar (diameter 32):**

Replace:
```rust
        let them_avatar = Image::from_bytes(them_avatar_bytes)
            .expect("avatar bytes valid")
            .width(32.0)
            .height(32.0)
            .corner_radius(16.0)
            .clip();
```
With:
```rust
        let them_avatar = avatar(them_avatar_bytes, 32.0);
```

**Site 4 — `build_contact_row` (diameter 40):**

Replace:
```rust
    let avatar = Image::from_bytes(&c.avatar_bytes)
        .expect("avatar bytes valid")
        .width(40.0)
        .height(40.0)
        .corner_radius(20.0)
        .clip();
```
With:
```rust
    let avatar = avatar(&c.avatar_bytes, 40.0);
```

**Site 5 — `build_profile_screen` (diameter 80):**

Replace:
```rust
    let avatar = Image::from_bytes(&profile.avatar_bytes)
        .expect("avatar bytes valid")
        .width(80.0)
        .height(80.0)
        .corner_radius(40.0)
        .clip();
```
With:
```rust
    let avatar = avatar(&profile.avatar_bytes, 80.0);
```

- [ ] **Step 5: Remove `Image` from `lib.rs` imports if no longer used directly**

After the replacements, `Image` may no longer be used directly in `lib.rs` (all avatar constructions now go through `avatar()`). Check if `Image` appears anywhere else in `lib.rs`. If not, remove it from the `use vexo::{...}` import to avoid an unused-import warning. The test `test_avatar_bytes_decode` uses `Image::from_bytes` — but that test will move to `data.rs` in Task 7. For now, if the test still references `Image`, keep the import.

- [ ] **Step 6: Build and test**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: compiles, all 13 tests pass.

---

### Task 3: Extract `chats/` (conversation list + chat screen + tab wiring)

**Files:**
- Create: `shared_app/src/chats/mod.rs`
- Create: `shared_app/src/chats/conversation_list.rs`
- Create: `shared_app/src/chats/chat_screen.rs`
- Modify: `shared_app/src/lib.rs` — add `mod chats;`, replace the Chats tab closure body with `build_chats_tab(...)` call

**Interfaces:**
- Consumes from Task 1: `Conversation`, `ConvId`, `ChatsRoute`, `Message`, `MessageAuthor` (from `data.rs`)
- Consumes from Task 2: `avatar()` (from `widgets/avatar.rs`)
- Produces: `build_chats_tab(conversations, nav, messages, me_avatar) -> Box<dyn Widget>`

- [ ] **Step 1: Create `shared_app/src/chats/mod.rs`**

```rust
//! Chats tab: conversation list + chat screen, wired into a NavigationStackView.

mod chat_screen;
mod conversation_list;

use std::collections::HashMap;
use std::rc::Rc;

use vexo::{Signal, Text, Widget};
use vexo_uikit::{NavigationController, NavigationStackView};

use crate::data::{ChatsRoute, Conversation, ConvId, Message, MessageAuthor};

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
    let nav_for_dest = nav.clone();
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
                let nav_back = nav_for_dest.clone();
                let msgs_for_send = msgs.clone();
                let id_for_send = id.clone();
                chat_screen::ChatScreen {
                    conv_id: id_for_send.clone(),
                    messages: m,
                    avatar_bytes: avatar,
                    me_avatar_bytes: me_avatar_for_dest.clone(),
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
```

- [ ] **Step 2: Create `shared_app/src/chats/conversation_list.rs`**

```rust
//! Conversation list screen — the root of the Chats tab.

use vexo::{Color, Column, DecoratedContainer, Flex, Row, ScrollView, Text, Widget};
use vexo_uikit::NavigationController;

use crate::data::{Conversation, ChatsRoute};
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
```

- [ ] **Step 3: Create `shared_app/src/chats/chat_screen.rs`**

```rust
//! Chat screen — the pushed destination when a conversation is tapped.

use std::any::Any;
use std::rc::Rc;

use vexo::{
    Color, Column, Component, ComponentState, DecoratedContainer, Flex, LifecycleContext,
    RenderContext, Row, ScrollView, ScrollController, Signal, Text, TextEdit,
    TextEditingController, Theme, Widget,
};
use vexo_uikit::{Button, ButtonVariant, NavigationController};

use crate::data::{ChatsRoute, ConvId, Message, MessageAuthor};
use crate::widgets::avatar::avatar;

pub(crate) struct ChatScreen {
    pub conv_id: ConvId,
    pub messages: Vec<Message>,
    pub avatar_bytes: Rc<[u8]>,
    pub me_avatar_bytes: Rc<[u8]>,
    pub nav: NavigationController<ChatsRoute>,
    pub on_send: Rc<dyn Fn(&str)>,
    pub scroll_controller: ScrollController,
}

impl Clone for ChatScreen {
    fn clone(&self) -> Self {
        Self {
            conv_id: self.conv_id.clone(),
            messages: self.messages.clone(),
            avatar_bytes: Rc::clone(&self.avatar_bytes),
            me_avatar_bytes: Rc::clone(&self.me_avatar_bytes),
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
            list = list.push(build_message_bubble(
                msg,
                &self.avatar_bytes,
                &self.me_avatar_bytes,
            ));
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

fn build_message_bubble(
    msg: &Message,
    them_avatar_bytes: &Rc<[u8]>,
    me_avatar_bytes: &Rc<[u8]>,
) -> Box<dyn Widget> {
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
    .max_width(220.0)
    .boxed();

    if msg.author == MessageAuthor::Me {
        let me_avatar = avatar(me_avatar_bytes, 32.0);
        Row::new()
            .gap(8.0)
            .push(Flex::new().flex_grow(1.0))
            .push(bubble)
            .push(me_avatar)
            .boxed()
    } else {
        let them_avatar = avatar(them_avatar_bytes, 32.0);
        Row::new()
            .gap(8.0)
            .push(them_avatar)
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
```

- [ ] **Step 4: Add `mod chats;` to `lib.rs`**

After `mod widgets;`, add:

```rust
mod chats;
```

- [ ] **Step 5: Remove chats code from `lib.rs`**

Delete these sections from `lib.rs`:
1. The entire `// ====... CONVERSATION LIST SCREEN ...====` section — from `fn build_conversation_list_screen` through `fn format_timestamp` (the closing brace before the `// ====... CHAT SCREEN ...====` comment).
2. The entire `// ====... CHAT SCREEN ...====` section — from `struct ChatScreen` through the closing brace of `fn build_input_bar`.

- [ ] **Step 6: Replace the Chats tab closure in `view()` with `build_chats_tab()` call**

In `lib.rs`'s `impl Application for ImState` → `fn view()`, find the `ImTab::Chats =>` branch inside the `TabBarView::new(...)` closure. Replace the entire branch body (from `let chats_root =` through `.boxed()`) with:

```rust
                ImTab::Chats => chats::build_chats_tab(
                    conversations.clone(),
                    nav_for_chat.clone(),
                    messages_for_chat.clone(),
                    me_avatar.clone(),
                ),
```

Then remove the now-unused clones that were specific to the Chats branch:
- Remove `let nav_for_list = state.chats_nav.clone();` (line 648 in original — `build_chats_tab` uses a single `nav` param).
- Remove `let convs_for_chat = state.conversations.clone();` (no longer needed — `build_chats_tab` takes `conversations` directly).

Keep `let nav_for_chat = state.chats_nav.clone();`, `let messages_for_chat = state.messages.clone();`, `let me_avatar = ...` — these are still passed to `build_chats_tab`.

- [ ] **Step 7: Clean up unused imports in `lib.rs`**

After removing the chats code, several imports may be unused in `lib.rs`:
- `DecoratedContainer` — was used in `build_conversation_row` and `build_message_bubble`. Check if still used in `lib.rs` (contacts and profile screens still use it). Keep if used, remove if not.
- `Component`, `ComponentState`, `LifecycleContext`, `RenderContext` — were used by `ChatScreen`. If `ChatScreen` was the only `Component` in `lib.rs`, remove these.
- `TextEdit`, `TextEditingController` — were used by `build_input_bar`. Remove if no longer used.
- `ScrollController` / `vexo::ScrollController` — was used in `ChatScreen`. Remove if no longer used.

Run `cargo build -p shared_app` and fix any "unused import" warnings by removing the flagged imports from `lib.rs`.

- [ ] **Step 8: Build and test**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: compiles, all 13 tests pass. The tests that reference `build_conversation_list_screen` and `ChatScreen` (still in `lib.rs`'s test module at this point) must use `chats::conversation_list::build_conversation_list_screen` and `chats::chat_screen::ChatScreen`. Update the test code in `lib.rs` to use these paths. Specifically:

In `test_conversation_list_renders_in_pipeline`:
```rust
let view = chats::conversation_list::build_conversation_list_screen(
    state.conversations.clone(),
    state.chats_nav.clone(),
);
```

In `test_chat_screen_renders_messages` and `test_chat_screen_input_bar_pinned_to_bottom_with_few_messages`:
```rust
let view = chats::chat_screen::ChatScreen {
    conv_id: ConvId(1),
    messages,
    avatar_bytes,
    me_avatar_bytes: state.profile.avatar_bytes.clone(),
    nav: state.chats_nav.clone(),
    on_send: Rc::new(|_| ()),
    scroll_controller: vexo::ScrollController::new(),
}
.boxed();
```

(These tests will move to their feature modules in Task 7, but for now they stay in `lib.rs` with updated paths.)

---

### Task 4: Extract `contacts/` (contacts screen + tab wiring)

**Files:**
- Create: `shared_app/src/contacts/mod.rs`
- Create: `shared_app/src/contacts/contacts_screen.rs`
- Modify: `shared_app/src/lib.rs` — add `mod contacts;`, replace the Contacts tab closure with `build_contacts_tab()` call

**Interfaces:**
- Consumes: `Contact` (from `data.rs`), `avatar()` (from `widgets/avatar.rs`)
- Produces: `build_contacts_tab(contacts, nav) -> Box<dyn Widget>`

- [ ] **Step 1: Create `shared_app/src/contacts/mod.rs`**

```rust
//! Contacts tab: contact list wired into a NavigationStackView.

mod contacts_screen;

use vexo::Widget;
use vexo_uikit::{NavigationController, NavigationStackView};

use crate::data::Contact;

pub(crate) fn build_contacts_tab(
    contacts: Vec<Contact>,
    nav: NavigationController<()>,
) -> Box<dyn Widget> {
    NavigationStackView::new(nav, contacts_screen::build_contacts_screen(contacts))
        .root_title("Contacts")
        .boxed()
}
```

- [ ] **Step 2: Create `shared_app/src/contacts/contacts_screen.rs`**

```rust
//! Contacts list screen.

use vexo::{Color, Column, Flex, Row, ScrollView, Text, Widget};

use crate::data::Contact;
use crate::widgets::avatar::avatar;

pub(crate) fn build_contacts_screen(contacts: Vec<Contact>) -> Box<dyn Widget> {
    let mut list = Flex::column();
    for c in &contacts {
        list = list.push(build_contact_row(c));
    }
    ScrollView::new(list.boxed()).flex_fill().boxed()
}

fn build_contact_row(c: &Contact) -> Box<dyn Widget> {
    let avatar = avatar(&c.avatar_bytes, 40.0);

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
```

- [ ] **Step 3: Add `mod contacts;` to `lib.rs`**

After `mod chats;`, add:

```rust
mod contacts;
```

- [ ] **Step 4: Remove contacts code from `lib.rs`**

Delete the entire `// ====... CONTACTS SCREEN ...====` section — from `fn build_contacts_screen` through the closing brace of `fn build_contact_row`.

- [ ] **Step 5: Replace the Contacts tab closure in `view()` with `build_contacts_tab()` call**

In `lib.rs`'s `fn view()`, replace the `ImTab::Contacts =>` branch body with:

```rust
                ImTab::Contacts => contacts::build_contacts_tab(
                    contacts.clone(),
                    contacts_nav.clone(),
                ),
```

- [ ] **Step 6: Update test paths and clean up imports**

In `test_contacts_screen_renders_in_pipeline` (still in `lib.rs`), update:
```rust
let view = contacts::contacts_screen::build_contacts_screen(state.contacts.clone());
```

Clean up any now-unused imports in `lib.rs` (e.g., if `Color`, `Flex`, `Row`, `ScrollView` are no longer used directly in `lib.rs`).

- [ ] **Step 7: Build and test**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: compiles, all 13 tests pass.

---

### Task 5: Extract `me/` (profile screen + tab wiring)

**Files:**
- Create: `shared_app/src/me/mod.rs`
- Create: `shared_app/src/me/profile_screen.rs`
- Modify: `shared_app/src/lib.rs` — add `mod me;`, replace the Me tab closure with `build_me_tab()` call

**Interfaces:**
- Consumes: `Profile` (from `data.rs`), `avatar()` (from `widgets/avatar.rs`)
- Produces: `build_me_tab(profile: &Profile, nav: NavigationController<()>) -> Box<dyn Widget>`

- [ ] **Step 1: Create `shared_app/src/me/mod.rs`**

```rust
//! Me tab: profile screen wired into a NavigationStackView.

mod profile_screen;

use vexo::Widget;
use vexo_uikit::{NavigationController, NavigationStackView};

use crate::data::Profile;

pub(crate) fn build_me_tab(
    profile: &Profile,
    nav: NavigationController<()>,
) -> Box<dyn Widget> {
    NavigationStackView::new(nav, profile_screen::build_profile_screen(profile))
        .root_title("Me")
        .boxed()
}
```

- [ ] **Step 2: Create `shared_app/src/me/profile_screen.rs`**

```rust
//! Profile screen — the root of the Me tab.

use vexo::{Color, Column, Flex, Row, Text, Widget};

use crate::data::Profile;
use crate::widgets::avatar::avatar;

pub(crate) fn build_profile_screen(profile: &Profile) -> Box<dyn Widget> {
    let avatar = avatar(&profile.avatar_bytes, 80.0);

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
```

- [ ] **Step 3: Add `mod me;` to `lib.rs`**

After `mod contacts;`, add:

```rust
mod me;
```

- [ ] **Step 4: Remove profile code from `lib.rs`**

Delete the entire `// ====... PROFILE SCREEN ...====` section — from `fn build_profile_screen` through its closing brace.

- [ ] **Step 5: Replace the Me tab closure in `view()` with `build_me_tab()` call**

In `lib.rs`'s `fn view()`, replace the `ImTab::Me =>` branch body with:

```rust
                ImTab::Me => me::build_me_tab(&profile, me_nav.clone()),
```

- [ ] **Step 6: Update test paths and clean up imports**

In `test_profile_screen_renders_in_pipeline` (still in `lib.rs`), update:
```rust
let view = me::profile_screen::build_profile_screen(&state.profile);
```

Clean up any now-unused imports in `lib.rs`.

- [ ] **Step 7: Build and test**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: compiles, all 13 tests pass.

---

### Task 6: Extract `app.rs` (Application impl, Default, MobileApp) + slim `lib.rs`

**Files:**
- Create: `shared_app/src/app.rs`
- Modify: `shared_app/src/lib.rs` — becomes minimal root with mod declarations + re-exports

**Interfaces:**
- Consumes: `ImState`, `seed()` (from `data.rs`), `build_chats_tab` (from `chats/mod.rs`), `build_contacts_tab` (from `contacts/mod.rs`), `build_me_tab` (from `me/mod.rs`)
- Produces: `ImState` (re-exported pub), `MobileApp` (pub, UniFFI Object)

- [ ] **Step 1: Create `shared_app/src/app.rs`**

```rust
//! Application trait impl, Default impl, and UniFFI MobileApp export.

use vexo::{AlignItems, Application, Color, Column, Text, Widget};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::TabBarView;

use crate::chats::build_chats_tab;
use crate::contacts::build_contacts_tab;
use crate::data::{seed, ImState, ImTab};
use crate::me::build_me_tab;

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
        let messages_for_chat = state.messages.clone();
        let contacts = state.contacts.clone();
        let profile = state.profile.clone();
        let me_avatar = profile.avatar_bytes.clone();
        let tab_controller = state.tab_controller.clone();
        let contacts_nav = state.contacts_nav.clone();
        let me_nav = state.me_nav.clone();
        let chats_nav = state.chats_nav.clone();

        let tab_view = TabBarView::new(
            tab_controller,
            vec![ImTab::Chats, ImTab::Contacts, ImTab::Me],
            move |tab| match tab {
                ImTab::Chats => build_chats_tab(
                    conversations.clone(),
                    chats_nav.clone(),
                    messages_for_chat.clone(),
                    me_avatar.clone(),
                ),
                ImTab::Contacts => build_contacts_tab(contacts.clone(), contacts_nav.clone()),
                ImTab::Me => build_me_tab(&profile, me_nav.clone()),
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

        tab_view.boxed()
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
```

- [ ] **Step 2: Replace `lib.rs` with minimal root**

Replace the ENTIRE contents of `shared_app/src/lib.rs` with:

```rust
//! Mocked IM UI — three-tab app shell (Chats / Contacts / Me) with
//! in-memory data, no network or persistence.

mod app;
mod chats;
mod contacts;
mod data;
mod me;
mod widgets;

#[cfg(test)]
mod integration_tests;

pub use app::{ImState, MobileApp};

uniffi::setup_scaffolding!();
```

Note: The `mod integration_tests;` declaration is used in Task 7. For now, create a placeholder file `shared_app/src/integration_tests.rs` with just a comment so the module compiles:

```rust
//! Integration tests for the full app view — populated in Task 7.
```

- [ ] **Step 3: Fix imports in `app.rs`**

Run `cargo build -p shared_app` and fix any import errors. The exact imports needed in `app.rs` depend on what `TabBarView`'s item builder closure uses. Key imports:
- `AlignItems` — used in the tab item builder
- `Color` — used for tab item colors
- `Column` — used in tab item builder
- `Text` — used in tab item builder
- `Widget` — the return type
- `Application` — the trait being implemented
- `Icon` (from `vexo_fontawesome`) — used in tab item builder. Note: `Icon` may also be re-exported from `vexo` — check which path the original code uses. The original imports `use vexo_fontawesome::{Icon, Icons};`, so use that.
- `Icons` (from `vexo_fontawesome`) — used for icon selection
- `TabBarView` (from `vexo_uikit`) — used in view()
- `Platform` — imported in original but may not be used directly. Remove if unused.
- `Theme` / `ThemeData` — imported in original but may not be used in `app.rs` (they were used in `ChatScreen::render`, which is now in `chats/chat_screen.rs`). Remove if unused.

Do NOT include `Theme` or `ThemeData` in `app.rs` imports unless the compiler requires them.

- [ ] **Step 4: Build and test**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: compiles. The 5 tests that were in `lib.rs`'s `mod tests` (data tests + screen tests) are now GONE because `lib.rs` no longer has a `mod tests`. They will be re-created in Task 7 in their feature modules. For now, only the tests that can be compiled from the remaining modules will run. If `cargo test` reports 0 tests, that is expected — the tests come back in Task 7.

Actually wait — the tests were in `lib.rs` which is now replaced. So all tests are temporarily gone. This is fine because Task 7 re-adds them. But to be safe, verify the build compiles even if tests are empty.

Run: `cargo build -p shared_app && cargo build -p desktop_demo`
Expected: both compile. `desktop_demo` uses `shared_app::ImState` which is re-exported from `app.rs`.

---

### Task 7: Move tests to per-feature homes + remove dead code + final commit

**Files:**
- Modify: `shared_app/src/data.rs` — add `#[cfg(test)] mod tests` with data tests
- Modify: `shared_app/src/chats/conversation_list.rs` — add test
- Modify: `shared_app/src/chats/chat_screen.rs` — add tests
- Modify: `shared_app/src/contacts/contacts_screen.rs` — add test
- Modify: `shared_app/src/me/profile_screen.rs` — add test
- Modify: `shared_app/src/integration_tests.rs` — add full-app pipeline tests
- Verify: `shared_app/src/lib.rs` — no dead `messages_for_view` code (already removed in Task 6)

**Interfaces:**
- No new interfaces. Tests consume existing public(crate) APIs.

- [ ] **Step 1: Add data tests to `data.rs`**

Append to the end of `shared_app/src/data.rs`:

```rust
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
```

- [ ] **Step 2: Add conversation list test to `chats/conversation_list.rs`**

Append to the end of `shared_app/src/chats/conversation_list.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    #[test]
    fn test_conversation_list_renders_in_pipeline() {
        let state = crate::data::seed();
        let view = build_conversation_list_screen(
            state.conversations.clone(),
            state.chats_nav.clone(),
        );
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 5,
            "expected multiple elements for 5 conversation rows"
        );
    }
}
```

- [ ] **Step 3: Add chat screen tests to `chats/chat_screen.rs`**

Append to the end of `shared_app/src/chats/chat_screen.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::layout::TaffyLayoutEngine;
    use vexo::{RenderObject, RenderObjectRegistry, ThreeTreePipeline};

    #[test]
    fn test_chat_screen_renders_messages() {
        let state = crate::data::seed();
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
            me_avatar_bytes: state.profile.avatar_bytes.clone(),
            nav: state.chats_nav.clone(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
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
        // Regression: with zero messages, the input bar must be pinned to
        // the bottom of the view, not floating right below the (empty)
        // message list.
        let state = crate::data::seed();
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
            me_avatar_bytes: state.profile.avatar_bytes.clone(),
            nav: state.chats_nav.clone(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
        };

        let view = vexo::Column::new().height(600.0).push(chat).boxed();

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
}
```

- [ ] **Step 4: Add contacts test to `contacts/contacts_screen.rs`**

Append to the end of `shared_app/src/contacts/contacts_screen.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    #[test]
    fn test_contacts_screen_renders_in_pipeline() {
        let state = crate::data::seed();
        let view = build_contacts_screen(state.contacts.clone());
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 4,
            "expected multiple elements for 8 contacts"
        );
    }
}
```

- [ ] **Step 5: Add profile test to `me/profile_screen.rs`**

Append to the end of `shared_app/src/me/profile_screen.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    #[test]
    fn test_profile_screen_renders_in_pipeline() {
        let state = crate::data::seed();
        let view = build_profile_screen(&state.profile);
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 2,
            "expected multiple elements for profile header + settings rows"
        );
    }
}
```

- [ ] **Step 6: Write integration tests in `integration_tests.rs`**

Replace the placeholder content of `shared_app/src/integration_tests.rs` with:

```rust
//! Full-app pipeline tests that exercise Application::view() and
//! cross-tab interactions. These are integration-level because they
//! assert on the complete widget tree, not individual screens.

use crate::app::ImState;
use crate::data::ImTab;
use std::sync::Arc;
use vexo::animation::AnimationTicker;
use vexo::layout::TaffyLayoutEngine;
use vexo::{Application, RenderObject, RenderObjectRegistry, ThreeTreePipeline};

#[test]
fn test_full_app_view_renders_three_tabs() {
    let mut state = ImState::default();
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
    let mut state = ImState::default();
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
    // Regression test: switching to the Contacts tab must not push the
    // tab bar off screen on a short window (800×600).
    let mut state = ImState::default();
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
```

- [ ] **Step 7: Build and run all tests**

Run: `cargo build -p shared_app && cargo test -p shared_app`
Expected: compiles, all 13 tests pass:
- 5 in `data.rs` (seed, messages, contacts, avatar, tab controller)
- 1 in `chats/conversation_list.rs` (conversation list renders)
- 2 in `chats/chat_screen.rs` (chat renders, input bar pinned)
- 1 in `contacts/contacts_screen.rs` (contacts renders)
- 1 in `me/profile_screen.rs` (profile renders)
- 3 in `integration_tests.rs` (full app, tab switch, tab bar fits)

- [ ] **Step 8: Verify consumer compiles**

Run: `cargo build -p desktop_demo`
Expected: compiles with no errors. This proves `shared_app::ImState` is still accessible.

- [ ] **Step 9: Run full workspace test suite**

Run: `cargo test --workspace`
Expected: all tests pass across all crates (vexo, vexo_uikit, shared_app, desktop_demo).

- [ ] **Step 10: Single commit**

```bash
git add -A
git commit -m "refactor(shared_app): extract feature modules from lib.rs

Split the 1061-line lib.rs into feature modules:
- data.rs: domain types, ImState, seed(), make_avatar_png()
- widgets/avatar.rs: deduped avatar builder (5 sites → 1 function)
- chats/: conversation_list + chat_screen + tab wiring
- contacts/: contacts_screen + tab wiring
- me/: profile_screen + tab wiring
- app.rs: Application impl, Default, MobileApp (UniFFI)
- integration_tests.rs: full-app pipeline tests

Tests co-located per feature. lib.rs reduced to ~15 lines of mod
declarations and pub re-exports. No behavior change; all 13 tests pass."
```
