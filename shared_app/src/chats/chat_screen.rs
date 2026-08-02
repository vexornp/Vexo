//! Chat screen — the pushed destination when a conversation is tapped.

use std::any::Any;
use std::rc::Rc;

use vexo::{
    column, row, AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox,
    FlexDirection, Image, ImageData, Key, Layout, LifecycleContext, MultiChild, RenderContext,
    ScrollController, ScrollView, Signal, Style, Text, TextEdit, TextEditingController, Theme,
    Widget, WidgetKey, WithLayout,
};
use vexo_uikit::{Button, ButtonVariant, KeyboardAvoider};

use crate::data::{ConvId, Message, MessageAuthor};
use crate::widgets::avatar::avatar;

pub(crate) struct ChatScreen {
    pub(crate) conv_id: ConvId,
    pub(crate) messages: Signal<Vec<Message>>,
    pub(crate) avatar_bytes: Rc<[u8]>,
    pub(crate) me_avatar_bytes: Rc<[u8]>,
    pub(crate) on_send: Rc<dyn Fn(&str)>,
    pub(crate) scroll_controller: ScrollController,
}

impl Clone for ChatScreen {
    fn clone(&self) -> Self {
        Self {
            conv_id: self.conv_id.clone(),
            messages: self.messages.clone(),
            avatar_bytes: Rc::clone(&self.avatar_bytes),
            me_avatar_bytes: Rc::clone(&self.me_avatar_bytes),
            on_send: Rc::clone(&self.on_send),
            scroll_controller: self.scroll_controller.clone(),
        }
    }
}

#[derive(Default)]
pub(crate) struct ChatScreenState {
    text_controller: Option<TextEditingController>,
    /// Decoded avatar image data, cached so we don't re-decode the PNG on
    /// every rebuild (ChatScreen rebuilds on every MediaQuery change, i.e.
    /// every keyboard animation frame — 40+ PNG decodes/frame = 63ms).
    them_avatar_image: Option<ImageData>,
    me_avatar_image: Option<ImageData>,
}

impl ChatScreenState {
    fn sync_controller(&mut self) {
        if self.text_controller.is_none() {
            let mut fs = vexo::resource::new_font_system();
            self.text_controller = Some(TextEditingController::new("", &mut fs));
        }
    }

    /// Lazily decode and cache the avatar images. Called from `render()` on
    /// first use (not `on_mount`, to avoid blocking on images that might not
    /// be needed yet). After the first call, returns the cached `ImageData`.
    fn them_avatar(&mut self, bytes: &Rc<[u8]>) -> &ImageData {
        self.them_avatar_image.get_or_insert_with(|| {
            ImageData::from_bytes(bytes).expect("avatar bytes are valid PNG")
        })
    }

    fn me_avatar(&mut self, bytes: &Rc<[u8]>) -> &ImageData {
        self.me_avatar_image.get_or_insert_with(|| {
            ImageData::from_bytes(bytes).expect("avatar bytes are valid PNG")
        })
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

    fn key(&self) -> Option<WidgetKey> {
        Some(WidgetKey::Local(Key::new(self.conv_id.0.to_string())))
    }

    /// Level 3 rebuild-skip (see `docs/rebuild-skipping-patterns.md`).
    /// During keyboard animation, TabBar and NavigationStack cascade `update()`
    /// to ChatScreen with fresh closure fields but identical data. Only
    /// `conv_id` participates in identity — the `messages` Signal drives
    /// state-driven rebuilds via `RenderContext::signal_value`, so the parent
    /// cascade can stop here without re-rendering message bubbles.
    fn should_rebuild(&self, old: &Self) -> bool {
        self.conv_id != old.conv_id
    }

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);

        let messages = ctx.signal_value(&self.messages);

        let list = column! {
            for msg in &messages {
                build_message_bubble(
                    msg,
                    state.them_avatar(&self.avatar_bytes).clone(),
                    state.me_avatar(&self.me_avatar_bytes).clone(),
                    &theme,
                )
            }
        }
        .gap(8.0)
        .padding(12.0);

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

        // Build the content WITHOUT reading MediaQuery — ChatScreen is NOT a
        // MediaQuery dependent, so it does NOT rebuild on keyboard animation
        // frames. KeyboardAvoider (from vexo_uikit) is the MediaQuery
        // dependent; it wraps the content in Shared so its rebuild is O(1).
        let content = column! {
            WithLayout::new(
                ScrollView::new(list.boxed()).controller(self.scroll_controller.clone()),
                Layout::flex_fill(),
            ),
            input_bar,
        }
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_height(0.0)
        .boxed();

        DecoratedBox::with_style(
            KeyboardAvoider::new(content),
            Style::default().background(theme.background),
        )
        .boxed()
    }
}

fn build_message_bubble(
    msg: &Message,
    them_avatar_image: ImageData,
    me_avatar_image: ImageData,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
    let is_me = msg.author == MessageAuthor::Me;
    let bubble = DecoratedBox::with_style(
        WithLayout::new(
            Text::new(msg.text.as_str())
                .with_font_size(15.0)
                .with_color(if is_me {
                    theme.on_primary
                } else {
                    theme.on_surface
                }),
            Layout::default()
                .flex_direction(FlexDirection::Row)
                .padding(10.0)
                .max_width(220.0)
                .align_self(AlignSelf::Start)
                .flex_shrink(0.0),
        ),
        Style::default()
            .corner_radius(12.0)
            .background(if is_me { theme.primary } else { theme.surface })
            .border(theme.outline, 1.0),
    )
    .boxed();

    if is_me {
        let me_avatar = avatar(me_avatar_image, 32.0);
        row! {
            MultiChild::empty(Layout::default().flex_grow(1.0)),
            bubble,
            me_avatar,
        }
        .gap(8.0)
        .boxed()
    } else {
        let them_avatar = avatar(them_avatar_image, 32.0);
        row! {
            them_avatar,
            bubble,
            MultiChild::empty(Layout::default().flex_grow(1.0)),
        }
        .gap(8.0)
        .boxed()
    }
}

fn build_input_bar(
    controller: TextEditingController,
    on_send: impl FnMut() + 'static,
) -> Box<dyn Widget> {
    WithLayout::new(
        row! {
            WithLayout::new(TextEdit::new(controller), Layout::default().flex_grow(1.0)),
            Button::new("Send")
                .variant(ButtonVariant::Primary)
                .shadow(
                    BoxShadow::new(Color::BLACK.with_alpha(0.25))
                        .blur(6.0)
                        .offset(0.0, 2.0),
                )
                .on_tap(on_send),
        }
        .gap(8.0),
        Layout::default().padding(8.0),
    )
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::layout::TaffyLayoutEngine;
    use vexo::{
        column, row, RenderObjectKey, RenderObjectRegistry, Signal, TextRenderObject,
        ThreeTreePipeline,
    };

    fn seed_messages_signal() -> Signal<std::collections::HashMap<ConvId, Vec<Message>>> {
        crate::data::seed().messages.clone()
    }

    fn seed_avatar(conv_id: ConvId) -> Rc<[u8]> {
        crate::data::seed()
            .conversations
            .iter()
            .find(|c| c.id == conv_id)
            .unwrap()
            .avatar_bytes
            .clone()
    }

    fn seed_me_avatar() -> Rc<[u8]> {
        crate::data::seed().profile.avatar_bytes.clone()
    }

    #[test]
    fn test_chat_screen_renders_messages() {
        let messages_signal = seed_messages_signal();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: Signal::derive(messages_signal, |map| {
                map.get(&ConvId(1)).cloned().unwrap_or_default()
            }),
            avatar_bytes: seed_avatar(ConvId(1)),
            me_avatar_bytes: seed_me_avatar(),
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
    fn test_chat_screen_reads_live_messages_from_signal() {
        let messages_signal = seed_messages_signal();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: Signal::derive(messages_signal.clone(), |map| {
                map.get(&ConvId(1)).cloned().unwrap_or_default()
            }),
            avatar_bytes: seed_avatar(ConvId(1)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
        }
        .boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 4,
            "expected multiple elements for 3 messages read from derived signal, got {}",
            pipeline.element_registry().len()
        );

        // Send a new message via the root Signal — this exercises the full
        // derive chain: root `set_from` → derived Signal's subscriber closure
        // → ChatScreen's `signal_value` dependency is marked dirty →
        // `perform_rebuilds()` re-renders and the new bubble appears.
        let mut updated_map = messages_signal.get_cloned();
        let new_message_text = "LIVE_UPDATE_TEST_MESSAGE";
        updated_map.get_mut(&ConvId(1)).unwrap().push(Message {
            author: MessageAuthor::Me,
            text: new_message_text.to_string(),
            timestamp: 1732348000,
        });
        messages_signal.set_from(&updated_map);
        pipeline.perform_rebuilds();

        // Walk the render tree and assert the new message text appears in a
        // TextRenderObject. `RenderObjectRegistry` exposes no `iter()`, so
        // recurse from the root (same pattern as the
        // `test_signal_value_registers_dependency_and_rebuilds` test in
        // `vexo/src/stateful_widget.rs`).
        fn find_text_in_tree(
            reg: &RenderObjectRegistry,
            key: RenderObjectKey,
            needle: &str,
        ) -> bool {
            let ro = match reg.get(key) {
                Some(ro) => ro,
                None => return false,
            };
            if ro
                .as_any()
                .downcast_ref::<TextRenderObject>()
                .map_or(false, |t| t.content().contains(needle))
            {
                return true;
            }
            for &child in ro.children() {
                if find_text_in_tree(reg, child, needle) {
                    return true;
                }
            }
            false
        }

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("render tree should have a root");
        assert!(
            find_text_in_tree(ro_reg, root, new_message_text),
            "new message text should appear in render tree after signal set + rebuild"
        );
    }

    #[test]
    fn test_chat_screen_input_bar_pinned_to_bottom_with_few_messages() {
        let empty_map: std::collections::HashMap<ConvId, Vec<Message>> =
            std::collections::HashMap::new();
        let empty_signal = Signal::new(empty_map);
        let chat = ChatScreen {
            conv_id: ConvId(4),
            messages: Signal::derive(empty_signal, |_| Vec::new()),
            avatar_bytes: seed_avatar(ConvId(4)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
        };

        let view = column! { chat }.height(600.0).boxed();

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
        let mut current = proxy;
        let chat_decorated = loop {
            let child = find_child(ro_reg, current, 0).expect("child of proxy");
            if let Some(grandchild) = find_child(ro_reg, child, 0) {
                if ro_reg
                    .get(grandchild)
                    .and_then(|ro| ro.children().len().into())
                    .unwrap_or(0)
                    >= 2
                {
                    break child;
                }
            }
            current = child;
        };
        let chat_col = find_child(ro_reg, chat_decorated, 0).expect("chat column");
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
