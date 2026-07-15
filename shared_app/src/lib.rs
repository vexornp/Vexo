//! Mocked IM UI — three-tab app shell (Chats / Contacts / Me) with
//! in-memory data, no network or persistence.

use vexo::{AlignItems, Application, Color, Column, Flex, Row, Text, Widget};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::{NavigationStackView, TabBarView};

uniffi::setup_scaffolding!();

mod data;
use data::*;

mod widgets;
use widgets::avatar::avatar;

mod chats;

mod contacts;

// ============================================================================
// PROFILE SCREEN
// ============================================================================

fn build_profile_screen(profile: &Profile) -> Box<dyn Widget> {
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
        let nav_for_chat = state.chats_nav.clone();
        let messages_for_chat = state.messages.clone();
        let contacts = state.contacts.clone();
        let profile = state.profile.clone();
        let me_avatar = profile.avatar_bytes.clone();
        let tab_controller = state.tab_controller.clone();
        let contacts_nav = state.contacts_nav.clone();
        let me_nav = state.me_nav.clone();

        let tab_view = TabBarView::new(
            tab_controller,
            vec![ImTab::Chats, ImTab::Contacts, ImTab::Me],
            move |tab| match tab {
                ImTab::Chats => chats::build_chats_tab(
                    conversations.clone(),
                    nav_for_chat.clone(),
                    messages_for_chat.clone(),
                    me_avatar.clone(),
                ),
                ImTab::Contacts => {
                    contacts::build_contacts_tab(contacts.clone(), contacts_nav.clone())
                }
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
    use std::rc::Rc;
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

    #[test]
    fn test_conversation_list_renders_in_pipeline() {
        use std::sync::Arc;
        use vexo::animation::AnimationTicker;
        use vexo::ThreeTreePipeline;

        let state = seed();
        let view = chats::conversation_list::build_conversation_list_screen(
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
        let chat = chats::chat_screen::ChatScreen {
            conv_id: ConvId(4),
            messages: vec![], // zero messages — minimal content
            avatar_bytes,
            me_avatar_bytes: state.profile.avatar_bytes.clone(),
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
        let view = contacts::contacts_screen::build_contacts_screen(state.contacts.clone());
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
