//! Chat screen — the pushed destination when a conversation is tapped.

use std::any::Any;
use std::rc::Rc;

use vexo::{
    children, AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox, FlexDirection,
    Image, ImageData, Key, Layout, LifecycleContext, MediaQuery, Memo, MultiChild, RenderContext,
    ScrollController, ScrollView, SimpleState, Style, Text, TextEdit, TextEditingController, Theme,
    Widget, WidgetKey, WithLayout,
};
use vexo_uikit::{Button, ButtonVariant};

use crate::data::{ConvId, Message, MessageAuthor};
use crate::widgets::avatar::avatar;

/// Small Component that reads `MediaQuery::of(ctx).viewInsets.bottom` and
/// applies it as bottom padding to its child. By isolating the MediaQuery
/// dependency here (instead of in ChatScreen::render), ChatScreen itself is
/// NOT marked as a MediaQuery dependent and does NOT rebuild on every keyboard
/// animation frame. Only this tiny component rebuilds — and because the child
/// is wrapped in `Memo<()>` (deps never change), `Memo::should_rebuild()`
/// returns false on every cascade, so the child subtree is NOT reconciled.
#[derive(Clone)]
struct KeyboardInsetPadding {
    child: Rc<dyn Widget>,
}

impl KeyboardInsetPadding {
    fn new(child: Rc<dyn Widget>) -> Self {
        Self { child }
    }
}

impl Component for KeyboardInsetPadding {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut SimpleState<()>, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let bottom = MediaQuery::of(ctx).viewInsets.bottom;
        // Memo<()> with unit deps: should_rebuild always returns false after
        // mount, so the child's render() is never re-invoked during keyboard
        // frames. The build closure runs once (on mount) and deep-clones the
        // child widget tree into Memo's internal Rc cache; subsequent parent
        // cascades stop at Memo without touching the child subtree.
        let child = self.child.clone();
        WithLayout::new(
            Memo::new((), move || child.as_ref().clone_boxed()),
            Layout::default()
                .flex_grow(1.0)
                .flex_basis(0.0)
                .min_height(0.0)
                .padding_each(0.0, 0.0, 0.0, bottom),
        )
        .boxed()
    }
}

pub(crate) struct ChatScreen {
    pub(crate) conv_id: ConvId,
    pub(crate) messages: Vec<Message>,
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
    /// to ChatScreen with fresh closure fields but identical data. Comparing
    /// only the data fields the render reads lets the cascade stop here
    /// instead of rebuilding 20+ message bubbles every frame.
    fn should_rebuild(&self, old: &Self) -> bool {
        self.conv_id != old.conv_id
            || self.messages.len() != old.messages.len()
            || self.messages != old.messages
    }

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);

        let mut list = MultiChild::empty(Layout::column().gap(8.0).padding(12.0));
        for msg in &self.messages {
            list = list.push(build_message_bubble(
                msg,
                state.them_avatar(&self.avatar_bytes).clone(),
                state.me_avatar(&self.me_avatar_bytes).clone(),
                &theme,
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

        // Build the content WITHOUT reading MediaQuery — ChatScreen is NOT a
        // MediaQuery dependent, so it does NOT rebuild on keyboard animation
        // frames. The bottom padding is applied by KeyboardInsetPadding below,
        // which IS a MediaQuery dependent but wraps the content in Memo<()>
        // so its rebuild is O(1).
        let content = DecoratedBox::with_style(
            MultiChild::new(
                children![
                    WithLayout::new(
                        ScrollView::new(list.boxed()).controller(self.scroll_controller.clone()),
                        Layout::flex_fill(),
                    ),
                    input_bar,
                ],
                Layout::column()
                    .flex_grow(1.0)
                    .flex_basis(0.0)
                    .min_height(0.0),
            ),
            Style::default().background(theme.background),
        )
        .boxed();

        KeyboardInsetPadding::new(Rc::from(content)).boxed()
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
        MultiChild::new(
            children![
                MultiChild::empty(Layout::default().flex_grow(1.0)),
                bubble,
                me_avatar,
            ],
            Layout::row().gap(8.0),
        )
        .boxed()
    } else {
        let them_avatar = avatar(them_avatar_image, 32.0);
        MultiChild::new(
            children![
                them_avatar,
                bubble,
                MultiChild::empty(Layout::default().flex_grow(1.0)),
            ],
            Layout::row().gap(8.0),
        )
        .boxed()
    }
}

fn build_input_bar(
    controller: TextEditingController,
    on_send: impl FnMut() + 'static,
) -> Box<dyn Widget> {
    WithLayout::new(
        MultiChild::new(
            children![
                WithLayout::new(TextEdit::new(controller), Layout::default().flex_grow(1.0)),
                Button::new("Send")
                    .variant(ButtonVariant::Primary)
                    .shadow(
                        BoxShadow::new(Color::BLACK.with_alpha(0.25))
                            .blur(6.0)
                            .offset(0.0, 2.0),
                    )
                    .on_tap(on_send),
            ],
            Layout::row().gap(8.0),
        ),
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
    use vexo::{RenderObjectRegistry, ThreeTreePipeline};

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
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
        };

        let view = MultiChild::new(children![chat], Layout::column().height(600.0)).boxed();

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
        // Chat screen now wraps content in KeyboardInsetPadding (Component) →
        // WithLayout → Shared (proxy) → DecoratedBox → MultiChild[scrollview, input_bar].
        // Each Component/Shared adds a proxy render object layer. Walk down
        // through all proxy layers until we find the DecoratedBox.
        let mut current = proxy;
        let chat_decorated = loop {
            let child = find_child(ro_reg, current, 0).expect("child of proxy");
            // Check if this child is the DecoratedBox (has 1 child that is the
            // MultiChild column). We detect it by checking if its first child
            // has multiple children (the column has [scrollview, input_bar]).
            if let Some(grandchild) = find_child(ro_reg, child, 0) {
                if ro_reg
                    .get(grandchild)
                    .and_then(|ro| ro.children().len().into())
                    .unwrap_or(0)
                    >= 2
                {
                    break child; // This is the DecoratedBox
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
