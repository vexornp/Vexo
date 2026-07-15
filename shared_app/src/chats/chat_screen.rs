//! Chat screen — the pushed destination when a conversation is tapped.

use std::any::Any;
use std::rc::Rc;

use vexo::{
    Color, Column, Component, ComponentState, DecoratedContainer, Flex, LifecycleContext,
    RenderContext, Row, ScrollController, ScrollView, Text, TextEdit, TextEditingController, Theme,
    Widget,
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
pub(crate) struct ChatScreenState {
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
