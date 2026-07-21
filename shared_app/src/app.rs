//! Application trait impl, Default impl, and UniFFI MobileApp export.

use vexo::{children, AlignItems, Application, Color, Layout, MultiChild, Text, Widget};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::TabBarView;

use crate::chats::build_chats_tab;
use crate::contacts::build_contacts_tab;
use crate::data::{seed, ImTab};
use crate::me::build_me_tab;

use crate::data::ImState;

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
                MultiChild::new(
                    children![
                        Icon::new(icon).with_size(22.0).with_color(color),
                        Text::new(label).with_font_size(11.0).with_color(color),
                    ],
                    Layout::column().gap(2.0).align(AlignItems::Center),
                )
                .boxed()
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
