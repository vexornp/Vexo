//! Chat screen — the pushed destination when a conversation is tapped.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use vexo::platform::file_picker::FilePicker;
use vexo::{
    column, row, AlignItems, AlignSelf, Component, ComponentState, DecoratedBox, FlexDirection,
    GestureDetector, Image, JustifyContent, Key, Layout, LifecycleContext, RenderContext,
    ScrollController, ScrollView, Signal, Spacer, Style, Text, TextEdit, TextEditingController,
    Theme, Widget, WidgetKey, WithLayout,
};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::{context_menu_trigger, ContextMenuController, KeyboardAvoider};

use crate::chats::message_menu;
use crate::data::{AvatarSource, ConvId, Message, MessageAuthor, MessageKind, ReactionType};
use crate::widgets::avatar::Avatar;

pub(crate) struct ChatScreen {
    pub(crate) conv_id: ConvId,
    /// Root messages Signal (identity-stable for this element's lifetime).
    /// Must be a root Signal, NOT a `Signal::derive` created in parent
    /// render — see "Signal field rule" in
    /// `docs/rebuild-skipping-patterns.md`. The per-conversation derived
    /// Signal lives in `ChatScreenState::derived_messages` (created in
    /// `on_mount`), so its subscription survives `should_rebuild == false`.
    pub(crate) messages: Signal<std::collections::HashMap<ConvId, Vec<Message>>>,
    pub(crate) avatar: AvatarSource,
    pub(crate) me_avatar: AvatarSource,
    pub(crate) on_send: Rc<dyn Fn(MessageKind)>,
    /// Toggle-callback for reactions. `(index, rt)` where `index` is the
    /// message's position in the conversation `Vec<Message>`. Wired in
    /// `mod.rs`/`desktop.rs` to call `data::apply_reaction` against the root
    /// messages Signal — mirrors `on_send`'s shape. The menu calls this on
    /// tap (see `message_menu::builder(index, on_react)`).
    pub(crate) on_react: Rc<dyn Fn(usize, ReactionType)>,
    pub(crate) scroll_controller: ScrollController,
    pub(crate) context_menu: ContextMenuController,
    pub(crate) file_picker: std::sync::Arc<dyn FilePicker>,
}

impl Clone for ChatScreen {
    fn clone(&self) -> Self {
        Self {
            conv_id: self.conv_id.clone(),
            messages: self.messages.clone(),
            avatar: self.avatar.clone(),
            me_avatar: self.me_avatar.clone(),
            on_send: Rc::clone(&self.on_send),
            on_react: Rc::clone(&self.on_react),
            scroll_controller: self.scroll_controller.clone(),
            context_menu: self.context_menu.clone(),
            file_picker: self.file_picker.clone(),
        }
    }
}

// Note: cannot #[derive(Default)] because `Signal<Vec<Message>>` doesn't
// impl Default. We implement Default manually below, initializing
// `derived_messages` to `None` (populated in `on_mount`).
pub(crate) struct ChatScreenState {
    text_controller: Option<TextEditingController>,
    /// Derived per-conversation messages Signal, created once in `on_mount`
    /// from the root `messages` Signal + `conv_id`. Lives in State (not the
    /// Widget struct) so its Arc identity is stable across widget
    /// replacements — critical for `should_rebuild == false` (see the
    /// "Signal field rule" section in `docs/rebuild-skipping-patterns.md`).
    derived_messages: Option<Signal<Vec<Message>>>,
}

impl Default for ChatScreenState {
    fn default() -> Self {
        Self {
            text_controller: None,
            derived_messages: None,
        }
    }
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
        // Deliberately do NOT wire the controller's dirty callback here.
        // The controller is Rc-shared with the child TextEdit, whose
        // on_mount sets the callback to its own element's dirty callback.
        // If ChatScreen overwrites it (here or in on_update), every keystroke
        // rebuilds the ENTIRE ChatScreen (all message bubbles) instead of
        // just the TextEdit. See docs/rebuild-skipping-patterns.md.

        // Create the derived per-conversation Signal from the root messages
        // Signal + conv_id. This must live in State (not Widget) so the
        // derived's Arc identity is stable across parent cascades — when
        // should_rebuild returns false, render() is skipped and
        // depend_on_signal is not re-called, but the subscription on the
        // State-owned derived survives. See the "Signal field rule" in
        // docs/rebuild-skipping-patterns.md.
        let widget = ctx
            .widget()
            .downcast_ref::<ChatScreen>()
            .expect("ChatScreenState::on_mount: widget must be ChatScreen");
        let conv_id = widget.conv_id.clone();
        let root = widget.messages.clone();
        self.derived_messages = Some(Signal::derive(root, move |map| {
            map.get(&conv_id).cloned().unwrap_or_default()
        }));
    }
    fn on_update(&mut self, old_widget: &dyn Any, ctx: &mut LifecycleContext) {
        // Deliberately do NOT wire the controller's dirty callback here —
        // see on_mount for why. The callback is owned by the child TextEdit
        // for the element's lifetime.

        // Recreate the derived per-conversation Signal when conv_id changes.
        // ChatScreen sits in a single-child slot (inside `titled_container`),
        // and the framework's `can_update` checks the widget TYPE, not the
        // key — so switching conversations takes the update path, NOT a
        // remount. That means `on_mount` does NOT re-run, and without this
        // `derived_messages` would stay bound to the OLD conv_id forever
        // (render() would keep showing the first-opened conversation). The
        // `key()` is ineffective for single-child slots (keys only drive
        // sibling reconciliation in multi-child containers). See "Signal
        // field rule" in docs/rebuild-skipping-patterns.md.
        let old = old_widget
            .downcast_ref::<ChatScreen>()
            .expect("ChatScreenState::on_update: old widget must be ChatScreen");
        let new = ctx
            .widget()
            .downcast_ref::<ChatScreen>()
            .expect("ChatScreenState::on_update: widget must be ChatScreen");
        if old.conv_id != new.conv_id {
            let conv_id = new.conv_id.clone();
            let root = new.messages.clone();
            self.derived_messages = Some(Signal::derive(root, move |map| {
                map.get(&conv_id).cloned().unwrap_or_default()
            }));
            // Clear draft text so the previous conversation's unsent input
            // doesn't leak into the newly-selected one.
            if let Some(tc) = self.text_controller.as_ref() {
                let mut fs = vexo::resource::new_font_system();
                tc.set_text("", &mut fs);
            }
        }
    }
    fn on_unmount(&mut self, _ctx: &mut LifecycleContext) {
        // Deliberately do NOT clear the controller's dirty callback —
        // TextEdit::on_unmount owns that. We only drop our Rc reference.
        self.text_controller = None;
    }
}

#[cfg(test)]
thread_local! {
    static CHAT_SCREEN_RENDER_COUNT: std::cell::Cell<u32> = std::cell::Cell::new(0);
}

impl Component for ChatScreen {
    type State = ChatScreenState;

    fn key(&self) -> Option<WidgetKey> {
        Some(WidgetKey::Local(Key::new(self.conv_id.0.to_string())))
    }

    /// Level 3 rebuild-skip (see `docs/rebuild-skipping-patterns.md`).
    /// During keyboard animation, TabBar and NavigationStack cascade `update()`
    /// to ChatScreen with fresh closure fields but identical data. Only
    /// `conv_id` participates in identity — the derived messages Signal in
    /// State drives state-driven rebuilds via `RenderContext::depend_on_signal`,
    /// so the parent cascade can stop here without re-rendering message
    /// bubbles. See "Signal field rule" in `rebuild-skipping-patterns.md`
    /// for why the derived must live in State, not Widget.
    fn should_rebuild(&self, old: &Self) -> bool {
        self.conv_id != old.conv_id
    }

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        #[cfg(test)]
        CHAT_SCREEN_RENDER_COUNT.with(|c| c.set(c.get() + 1));

        let theme = Theme::of(ctx);

        // Read the State-owned derived Signal (not the root). The derived
        // filters to this conversation's messages and its Arc identity is
        // stable (created once in on_mount), so the subscription survives
        // should_rebuild == false. See "Signal field rule" in
        // docs/rebuild-skipping-patterns.md.
        let messages = ctx.depend_on_signal(
            state
                .derived_messages
                .as_ref()
                .expect("derived_messages must be set in on_mount before render"),
        );

        let ctrl = self.context_menu.clone();
        let on_react = Rc::clone(&self.on_react);
        let list = column! {
            for (index, msg) in messages.iter().enumerate() {
                let is_me = msg.author == MessageAuthor::Me;
                let bubble = build_bubble(msg, &theme);
                let bubble_with_menu = context_menu_trigger(
                    bubble,
                    ctrl.clone(),
                    message_menu::builder(index, Rc::clone(&on_react)),
                );
                // Chip row is the sole feedback for set reactions (the pill
                // has no highlight — Q4). Built at the call site (Q12) from
                // `msg.reactions` + `theme`; `assemble_row` places it below
                // the bubble, aligned to the author's side. Multiple reactions
                // accumulate into a horizontal row of chips (one per reaction).
                let chip = message_menu::reaction_chip_row(&msg.reactions, &theme);
                let src = if is_me {
                    self.me_avatar.clone()
                } else {
                    self.avatar.clone()
                };
                let avatar_widget: Box<dyn Widget> = Avatar::new(src, 32.0).boxed();
                assemble_row(bubble_with_menu, chip, avatar_widget, is_me)
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
                on_send(MessageKind::Text(text));
                let mut fs = vexo::resource::new_font_system();
                tc_for_clear.set_text("", &mut fs);
                scroll_for_send.jump_to_bottom();
            }
        };

        let file_picker_for_attach = self.file_picker.clone();
        let on_send_for_attach = Rc::clone(&self.on_send);
        let scroll_for_attach = self.scroll_controller.clone();
        let on_attach = move || {
            let on_send_for_attach = Rc::clone(&on_send_for_attach);
            let scroll_for_attach = scroll_for_attach.clone();
            file_picker_for_attach.pick_file(Box::new(move |picked| {
                if let Some(picked) = picked {
                    let attachment = crate::data::FileAttachment {
                        name: picked.name,
                        mime: picked.mime,
                        size: picked.bytes.len() as u64,
                        bytes: std::sync::Arc::from(picked.bytes),
                    };
                    on_send_for_attach(MessageKind::File(attachment));
                    scroll_for_attach.jump_to_bottom();
                }
            }));
        };

        let input_bar = build_input_bar(tc, on_send_closure, on_attach, &theme);

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

/// Horizontal padding inside the message bubble (between the bubble border
/// and the text content). The reaction chip row mirrors this so chip icons
/// align with the text content edges, not the bubble border — see
/// `message_menu::reaction_chip_row`.
pub(crate) const BUBBLE_CONTENT_PADDING: f32 = 10.0;

/// Build just the message bubble (DecoratedBox + content), without the avatar
/// or row layout. This is what gets wrapped in `context_menu_trigger` so the
/// trigger's bounds match the bubble, not the full-width row. Branches on
/// `msg.kind`: text renders as a text bubble, files render as an image
/// thumbnail (for images) or an icon+name+size card (for non-images).
fn build_bubble(msg: &Message, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    let is_me = msg.author == MessageAuthor::Me;
    let content = match &msg.kind {
        MessageKind::Text(text) => build_text_content(text, is_me, theme),
        MessageKind::File(file) => build_file_content(file, is_me, theme),
    };
    DecoratedBox::with_style(
        content,
        Style::default()
            .corner_radius(12.0)
            .background(if is_me { theme.primary } else { theme.surface })
            .border(theme.outline, 1.0),
    )
    .boxed()
}

fn build_text_content(text: &str, is_me: bool, theme: &vexo::ThemeData) -> Box<dyn Widget> {
    WithLayout::new(
        Text::new(text).with_font_size(15.0).with_color(if is_me {
            theme.on_primary
        } else {
            theme.on_surface
        }),
        Layout::default()
            .flex_direction(FlexDirection::Row)
            .padding(BUBBLE_CONTENT_PADDING)
            .max_width(220.0)
            .align_self(AlignSelf::Start)
            .flex_shrink(0.0),
    )
    .boxed()
}

fn build_file_content(
    file: &crate::data::FileAttachment,
    is_me: bool,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
    if file.mime.starts_with("image/") {
        if let Some(data) = decode_thumbnail_cached(file) {
            let (w, h) = fit_image_within(data.width, data.height, 180.0, 180.0);
            let image = Image::new((*data).clone());
            return WithLayout::new(image, Layout::default().width(w).height(h).flex_shrink(0.0))
                .boxed();
        }
    }
    let icon_color = if is_me {
        theme.on_primary
    } else {
        theme.on_surface
    };
    let text_color = icon_color;
    let muted_color = icon_color.with_alpha(0.6);
    let icon = if file.mime.starts_with("image/") {
        Icons::FileImage
    } else {
        Icons::File
    };
    column! {
        Icon::new(icon).with_color(icon_color),
        Text::new(file.name.as_str()).with_font_size(14.0).with_color(text_color),
        Text::new(format_file_size(file.size).as_str()).with_font_size(12.0).with_color(muted_color),
    }
    .gap(4.0)
    .padding(BUBBLE_CONTENT_PADDING)
    .boxed()
}

fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    }
}

/// Compute (width, height) that fit an image of intrinsic size `(iw, ih)`
/// within a `max_w` x `max_h` box, preserving aspect ratio. Never upscales
/// (scale capped at 1.0) so small images render at their natural size.
const THUMBNAIL_MAX_DIM: u32 = 256;

thread_local! {
    static THUMB_CACHE: RefCell<HashMap<(String, u64), Rc<vexo::ImageData>>> =
        RefCell::new(HashMap::new());
}

fn thumb_key(file: &crate::data::FileAttachment) -> (String, u64) {
    (file.name.clone(), file.size)
}

fn decode_thumbnail_cached(file: &crate::data::FileAttachment) -> Option<Rc<vexo::ImageData>> {
    let key = thumb_key(file);
    if let Some(cached) = THUMB_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Some(cached);
    }
    let start = std::time::Instant::now();
    let data = decode_thumbnail(&file.bytes, THUMBNAIL_MAX_DIM)?;
    let elapsed = start.elapsed();
    log::debug!(
        "[THMB] decoded '{}' ({} bytes) in {:.1}ms → {}x{}",
        file.name,
        file.size,
        elapsed.as_secs_f32() * 1000.0,
        data.width,
        data.height
    );
    let data = Rc::new(data);
    THUMB_CACHE.with(|c| c.borrow_mut().insert(key, data.clone()));
    Some(data)
}

fn decode_thumbnail(bytes: &[u8], max_dim: u32) -> Option<vexo::ImageData> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = if img.width() > max_dim || img.height() > max_dim {
        img.thumbnail(max_dim, max_dim)
    } else {
        img
    };
    let rgba = resized.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    if width == 0 || height == 0 {
        return None;
    }
    Some(vexo::ImageData {
        pixels: rgba.into_raw(),
        width,
        height,
    })
}

fn fit_image_within(iw: u32, ih: u32, max_w: f32, max_h: f32) -> (f32, f32) {
    let iw = iw as f32;
    let ih = ih as f32;
    if iw <= 0.0 || ih <= 0.0 {
        return (max_w, max_h);
    }
    let scale = (max_w / iw).min(max_h / ih).min(1.0);
    (iw * scale, ih * scale)
}

/// Assemble the full message row: avatar + (bubble + optional reaction chip)
/// + spacer. The bubble is already wrapped in the context menu trigger by the
/// caller — `assemble_row` only handles the row layout (avatar position +
/// spacer for "me" alignment) and placing the `chip` below the bubble.
///
/// The chip is a sibling *below* the bubble, outside the `context_menu_trigger`
/// (the trigger wraps only the bubble — the thing you right-click — so menu
/// positioning keys off bubble bounds, not bubble+chip). The bubble+chip
/// column aligns to the author's side: `End` for "me" (right), `Start` for
/// "them" (left), so the chip sits under the bubble, not centered under the
/// whole row.
fn assemble_row(
    bubble_with_menu: Box<dyn Widget>,
    chip: Option<Box<dyn Widget>>,
    avatar_widget: Box<dyn Widget>,
    is_me: bool,
) -> Box<dyn Widget> {
    // Align the chip to the author's side — `End` for me (right), `Start` for
    // them (left). Keeps the chip under the bubble, not drifting to row center.
    let bubble_align = if is_me {
        AlignItems::End
    } else {
        AlignItems::Start
    };
    let bubble_and_chip = column! {
        bubble_with_menu,
        chip,
    }
    .align(bubble_align);

    if is_me {
        row! {
            Spacer::new(),
            bubble_and_chip,
            avatar_widget,
        }
        .gap(8.0)
        .boxed()
    } else {
        row! {
            avatar_widget,
            bubble_and_chip,
            Spacer::new(),
        }
        .gap(8.0)
        .boxed()
    }
}

fn build_input_bar(
    controller: TextEditingController,
    on_send: impl FnMut() + 'static,
    on_attach: impl FnMut() + 'static,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
    let attach_button = GestureDetector::new(
        DecoratedBox::with_style(
            WithLayout::new(
                Icon::new(Icons::Paperclip).with_color(theme.on_surface),
                Layout::default().padding(10.0),
            )
            .boxed(),
            Style::default()
                .corner_radius(8.0)
                .background(theme.surface)
                .border(theme.outline, 1.0),
        )
        .boxed(),
    )
    .on_tap(on_attach)
    .boxed();

    row! {
        attach_button,
        WithLayout::new(
            TextEdit::new(controller)
                .with_background(theme.surface)
                .with_text_color(theme.on_surface)
                .with_border_color(theme.outline),
            Layout::default().flex_grow(1.0),
        ),
        WithLayout::new(
            GestureDetector::new(
                DecoratedBox::with_style(
                    WithLayout::new(
                        Icon::new(Icons::PaperPlane)
                            .with_size(20.0)
                            .with_color(theme.on_primary),
                        Layout::default()
                            .width(36.0)
                            .height(36.0)
                            .flex_direction(FlexDirection::Row)
                            .justify(JustifyContent::Center)
                            .align(AlignItems::Center),
                    )
                    .boxed(),
                    Style::default()
                        .corner_radius(18.0)
                        .background(theme.primary),
                )
                .boxed(),
            )
            .on_tap(on_send)
            .boxed(),
            Layout::default().align_self(AlignSelf::Center),
        ),
    }
    .gap(8.0)
    .padding(8.0)
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::layout::TaffyLayoutEngine;
    use vexo::{
        column, RenderObjectKey, RenderObjectRegistry, Signal, TextRenderObject, ThreeTreePipeline,
    };
    use vexo_uikit::ContextMenu;

    fn seed_messages_signal() -> Signal<std::collections::HashMap<ConvId, Vec<Message>>> {
        crate::data::seed().messages.clone()
    }

    fn seed_avatar(conv_id: ConvId) -> AvatarSource {
        crate::data::seed()
            .conversations
            .iter()
            .find(|c| c.id == conv_id)
            .unwrap()
            .avatar
            .clone()
    }

    fn seed_me_avatar() -> AvatarSource {
        crate::data::seed().profile.avatar.clone()
    }

    /// Walk the render tree and return true if any `TextRenderObject` contains
    /// `needle`. `RenderObjectRegistry` exposes no `iter()`, so recurse from
    /// `key` (same pattern as the `test_depend_on_signal_registers_dependency_and_
    /// rebuilds` test in `vexo/src/stateful_widget.rs`).
    fn find_text_in_tree(reg: &RenderObjectRegistry, key: RenderObjectKey, needle: &str) -> bool {
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

    #[test]
    fn test_chat_screen_renders_messages() {
        let messages_signal = seed_messages_signal();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal,
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
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
            messages: messages_signal.clone(),
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 4,
            "expected multiple elements for 3 messages read from derived signal, got {}",
            pipeline.element_registry().len()
        );

        // Send a new message via the root Signal — this exercises the full
        // derive chain: root `set_from` → derived Signal's subscriber closure
        // → ChatScreen's `depend_on_signal` dependency is marked dirty →
        // `perform_rebuilds()` re-renders and the new bubble appears.
        let mut updated_map = messages_signal.get_cloned();
        let new_message_text = "LIVE_UPDATE_TEST_MESSAGE";
        updated_map.get_mut(&ConvId(1)).unwrap().push(Message {
            author: MessageAuthor::Me,
            kind: MessageKind::Text(new_message_text.to_string()),
            timestamp: 1732348000,
            reactions: vec![],
        });
        messages_signal.set_from(&updated_map);
        pipeline.perform_rebuilds();

        // Walk the render tree and assert the new message text appears in a
        // TextRenderObject. `RenderObjectRegistry` exposes no `iter()`, so
        // recurse from the root (same pattern as the
        // `test_depend_on_signal_registers_dependency_and_rebuilds` test in
        // `vexo/src/stateful_widget.rs`).
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("render tree should have a root");
        assert!(
            find_text_in_tree(ro_reg, root, new_message_text),
            "new message text should appear in render tree after signal set + rebuild"
        );
    }

    /// Regression for "clicking a different conversation row doesn't update
    /// the chat screen — it keeps showing the first-clicked conversation's
    /// messages."
    ///
    /// ChatScreen lives in a single-child slot, so switching conversations
    /// takes the UPDATE path (`can_update` checks type, not key). `on_mount`
    /// does NOT re-run, so `on_update` must recreate the derived per-
    /// conversation Signal when `conv_id` changes — otherwise `render()`
    /// keeps reading the stale derived bound to the OLD `conv_id`.
    #[test]
    fn test_chat_screen_updates_messages_when_conv_id_changes() {
        let messages_signal = seed_messages_signal();

        // Mount with conv 1.
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal.clone(),
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);

        let conv1_marker = "Hey! Are we still on for tomorrow?";
        let conv2_marker = "Did you get the file?";
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root after mount");
        assert!(
            find_text_in_tree(ro_reg, root, conv1_marker),
            "conv 1 message should render after mount"
        );

        // Switch to conv 2 — same single-child slot, same type → update path
        // (this is the path the bug broke: on_update must rebind derived).
        let new_view = ChatScreen {
            conv_id: ConvId(2),
            messages: messages_signal.clone(),
            avatar: seed_avatar(ConvId(2)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();
        pipeline.update(new_view);
        pipeline.perform_rebuilds();

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root after update");
        assert!(
            find_text_in_tree(ro_reg, root, conv2_marker),
            "conv 2 message should render after switching conversations"
        );
        assert!(
            !find_text_in_tree(ro_reg, root, conv1_marker),
            "conv 1 message should NOT render after switching to conv 2 \
             (derived Signal must be rebound to the new conv_id in on_update)"
        );
    }

    #[test]
    fn test_chat_screen_input_bar_pinned_to_bottom_with_few_messages() {
        let empty_map: std::collections::HashMap<ConvId, Vec<Message>> =
            std::collections::HashMap::new();
        let empty_signal = Signal::new(empty_map);
        let chat = ChatScreen {
            conv_id: ConvId(4),
            messages: empty_signal,
            avatar: seed_avatar(ConvId(4)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        };

        let view = column! { chat }.height(600.0).boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
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

    #[test]
    fn test_chat_screen_input_bar_uses_theme_colors() {
        let messages_signal = seed_messages_signal();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal,
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        };
        let dark_theme = vexo::ThemeData::dark();
        let themed = vexo::Theme::new(dark_theme.clone(), view);

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(themed.boxed());

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        fn find_in_tree(
            reg: &RenderObjectRegistry,
            key: RenderObjectKey,
            predicate: &dyn Fn(&dyn vexo::RenderObject) -> bool,
        ) -> Option<RenderObjectKey> {
            let ro = reg.get(key)?;
            if predicate(ro) {
                return Some(key);
            }
            for &child in ro.children() {
                if let Some(found) = find_in_tree(reg, child, predicate) {
                    return Some(found);
                }
            }
            None
        }

        fn collect_matching(
            reg: &RenderObjectRegistry,
            key: RenderObjectKey,
            predicate: &dyn Fn(&dyn vexo::RenderObject) -> bool,
            out: &mut Vec<RenderObjectKey>,
        ) {
            if let Some(ro) = reg.get(key) {
                if predicate(ro) {
                    out.push(key);
                }
                for &child in ro.children() {
                    collect_matching(reg, child, predicate, out);
                }
            }
        }

        fn subtree_contains(
            reg: &RenderObjectRegistry,
            key: RenderObjectKey,
            target: RenderObjectKey,
        ) -> bool {
            if key == target {
                return true;
            }
            match reg.get(key) {
                Some(ro) => ro
                    .children()
                    .iter()
                    .any(|&child| subtree_contains(reg, child, target)),
                None => false,
            }
        }

        let text_edit_key = find_in_tree(ro_reg, root, &|ro| {
            ro.as_any()
                .downcast_ref::<vexo::TextEditRenderObject>()
                .is_some()
        })
        .expect("should find a TextEditRenderObject in the input bar");
        let text_edit_ro = ro_reg
            .get(text_edit_key)
            .and_then(|ro| ro.as_any().downcast_ref::<vexo::TextEditRenderObject>())
            .expect("downcast TextEditRenderObject");
        assert_eq!(
            text_edit_ro.color(),
            dark_theme.on_surface,
            "input bar text color should match dark theme on_surface"
        );

        let mut all_decorated: Vec<RenderObjectKey> = Vec::new();
        collect_matching(
            ro_reg,
            root,
            &|ro| {
                ro.as_any()
                    .downcast_ref::<vexo::render_objects::DecoratedBoxRenderObject>()
                    .is_some()
            },
            &mut all_decorated,
        );
        // `all_decorated` is in DFS pre-order, so ancestors appear before
        // descendants. The ChatScreen root itself is a DecoratedBox (wrapping
        // KeyboardAvoider) and is an ancestor of the input bar's TextEdit, so a
        // forward search would return the root — which has no border. The
        // input bar's own DecoratedBox (created inside TextEdit, carrying the
        // border) is the INNERMOST enclosing DecoratedBox of the
        // TextEditRenderObject. Iterate in reverse so the deepest match wins.
        let decorated_key = all_decorated
            .iter()
            .rev()
            .copied()
            .find(|&k| subtree_contains(ro_reg, k, text_edit_key))
            .expect("should find the DecoratedBox enclosing the input bar's TextEditRenderObject");
        let decorated_ro = ro_reg
            .get(decorated_key)
            .and_then(|ro| {
                ro.as_any()
                    .downcast_ref::<vexo::render_objects::DecoratedBoxRenderObject>()
            })
            .expect("downcast DecoratedBoxRenderObject");
        let border = decorated_ro
            .style()
            .border
            .as_ref()
            .expect("input bar DecoratedBox should have a border");
        assert_eq!(
            border.color, dark_theme.outline,
            "input bar border color should match dark theme outline"
        );
    }

    /// Regression for "paste a lot of text into the input bar → cursor renders
    /// vertically outside its border, the more text the more offset."
    ///
    /// Reproduces the real chat screen structure: `build_input_bar` (with the
    /// real `Button`) inside the column + `KeyboardAvoider` + `DecoratedBox`
    /// exactly as `ChatScreen::render` builds it. Then mirrors the exact caret
    /// math from `TextEditRenderObject::paint`:
    ///
    ///   vertical_offset = max(0, (content_height - text_height) / 2)
    ///   caret_bottom    = vertical_offset + cursor_y + line_height
    ///
    /// and asserts the caret bottom stays within the TextEdit content box
    /// (i.e. inside the border). If the layout fails to grow the content box
    /// to fit wrapped text, `content_height` stays at ~1 line while `cursor_y`
    /// grows to N*line_height, so `caret_bottom` exceeds `content_height`.
    #[test]
    fn test_input_bar_cursor_stays_inside_border_with_wrapped_text() {
        let mut fs = vexo::resource::new_font_system();
        // Long single-line text (no newlines) so wrapping is driven purely by
        // the computed layout width — exactly the "paste a lot of text" case.
        let long_text = "The quick brown fox jumps over the lazy dog. ".repeat(8);
        let controller = vexo::TextEditingController::new(&long_text, &mut fs);
        let theme = vexo::ThemeData::light();

        let input_bar = build_input_bar(controller.clone(), || {}, || {}, &theme);

        // Mirror ChatScreen::render's structure faithfully.
        let content = column! {
            WithLayout::new(
                ScrollView::new(column! {}.boxed()),
                Layout::flex_fill(),
            ),
            input_bar,
        }
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_height(0.0)
        .boxed();

        let view = DecoratedBox::with_style(
            vexo_uikit::KeyboardAvoider::new(content),
            Style::default().background(theme.background),
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        pipeline.layout(vexo::core::Size::new(400.0, 600.0), &mut engine, &mut fs);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        // Walk the render tree to find the TextEditRenderObject.
        fn find_te(reg: &RenderObjectRegistry, key: RenderObjectKey) -> Option<RenderObjectKey> {
            let ro = reg.get(key)?;
            if ro
                .as_any()
                .downcast_ref::<vexo::TextEditRenderObject>()
                .is_some()
            {
                return Some(key);
            }
            for &child in ro.children() {
                if let Some(found) = find_te(reg, child) {
                    return Some(found);
                }
            }
            None
        }
        let te_key = find_te(ro_reg, root).expect("should find TextEditRenderObject");
        let te_ro = ro_reg
            .get(te_key)
            .and_then(|ro| ro.as_any().downcast_ref::<vexo::TextEditRenderObject>())
            .expect("downcast TextEditRenderObject");
        let content_bounds = te_ro
            .computed_bounds()
            .expect("TextEditRenderObject should have computed bounds");
        let content_height = content_bounds.height();

        // Buffer state after layout (apply_layout called set_layout_width).
        let (_cursor_x, cursor_y) = controller
            .cursor_position()
            .expect("cursor should be positioned after layout");
        let line_height = controller.line_height();
        let text_height: f32 = {
            let editor = controller.editor();
            let editor = editor.borrow();
            let mut h = 0.0f32;
            for run in editor.buffer().layout_runs() {
                h = h.max(run.line_top + run.line_height);
            }
            h
        };

        // Sanity: the text must actually have wrapped.
        let one_line = controller.font_size() * vexo::layout::DEFAULT_LINE_HEIGHT_MULTIPLIER;
        assert!(
            text_height > one_line * 1.5,
            "test setup failure: text should wrap to multiple lines, but \
             text_height={} vs one_line={}",
            text_height,
            one_line
        );

        // Mirror TextEditRenderObject::paint's caret math.
        let vertical_offset = ((content_height - text_height) / 2.0).max(0.0);
        let caret_bottom_rel = vertical_offset + cursor_y as f32 + line_height;

        assert!(
            caret_bottom_rel <= content_height + 0.5,
            "caret would render outside the input bar border. \
             content_height={}, text_height={}, cursor_y={}, line_height={}, \
             vertical_offset={}, caret_bottom_rel={}. \
             If content_height ~= one line while cursor_y spans many wrapped \
             lines, the caret drifts below the border (the reported bug).",
            content_height,
            text_height,
            cursor_y,
            line_height,
            vertical_offset,
            caret_bottom_rel
        );
    }

    /// Same as above but exercises the INCREMENTAL path: mount the input bar
    /// with EMPTY text, lay out, THEN paste a lot of text and re-layout —
    /// exactly what a user does. This is the path that triggers the reported
    /// "cursor drifts outside the border" bug.
    #[test]
    fn test_input_bar_cursor_stays_inside_border_after_paste() {
        let mut fs = vexo::resource::new_font_system();
        let controller = vexo::TextEditingController::new("", &mut fs);
        let theme = vexo::ThemeData::light();

        let input_bar = build_input_bar(controller.clone(), || {}, || {}, &theme);

        let content = column! {
            WithLayout::new(
                ScrollView::new(column! {}.boxed()),
                Layout::flex_fill(),
            ),
            input_bar,
        }
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_height(0.0)
        .boxed();

        let view = DecoratedBox::with_style(
            vexo_uikit::KeyboardAvoider::new(content),
            Style::default().background(theme.background),
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        // First layout: empty text, single-line input bar.
        pipeline.layout(vexo::core::Size::new(400.0, 600.0), &mut engine, &mut fs);

        // Now paste a lot of text (the reported trigger).
        let long_text = "The quick brown fox jumps over the lazy dog. ".repeat(8);
        controller.paste(&long_text, &mut fs);
        // Drive the rebuild queued by the controller's dirty callback, then
        // re-layout — mirrors the event loop's rebuild+layout cycle.
        pipeline.perform_rebuilds();
        pipeline.layout(vexo::core::Size::new(400.0, 600.0), &mut engine, &mut fs);

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        fn find_te(reg: &RenderObjectRegistry, key: RenderObjectKey) -> Option<RenderObjectKey> {
            let ro = reg.get(key)?;
            if ro
                .as_any()
                .downcast_ref::<vexo::TextEditRenderObject>()
                .is_some()
            {
                return Some(key);
            }
            for &child in ro.children() {
                if let Some(found) = find_te(reg, child) {
                    return Some(found);
                }
            }
            None
        }
        let te_key = find_te(ro_reg, root).expect("should find TextEditRenderObject");
        let te_ro = ro_reg
            .get(te_key)
            .and_then(|ro| ro.as_any().downcast_ref::<vexo::TextEditRenderObject>())
            .expect("downcast TextEditRenderObject");
        let content_height = te_ro
            .computed_bounds()
            .expect("TextEditRenderObject should have computed bounds")
            .height();

        let (_cursor_x, cursor_y) = controller
            .cursor_position()
            .expect("cursor should be positioned after layout");
        let line_height = controller.line_height();
        let text_height: f32 = {
            let editor = controller.editor();
            let editor = editor.borrow();
            let mut h = 0.0f32;
            for run in editor.buffer().layout_runs() {
                h = h.max(run.line_top + run.line_height);
            }
            h
        };

        let one_line = controller.font_size() * vexo::layout::DEFAULT_LINE_HEIGHT_MULTIPLIER;
        assert!(
            text_height > one_line * 1.5,
            "test setup failure: pasted text should wrap to multiple lines, but \
             text_height={} vs one_line={}",
            text_height,
            one_line
        );

        let vertical_offset = ((content_height - text_height) / 2.0).max(0.0);
        let caret_bottom_rel = vertical_offset + cursor_y as f32 + line_height;

        assert!(
            caret_bottom_rel <= content_height + 0.5,
            "caret renders outside the input bar border after paste. \
             content_height={}, text_height={}, cursor_y={}, line_height={}, \
             vertical_offset={}, caret_bottom_rel={}.",
            content_height,
            text_height,
            cursor_y,
            line_height,
            vertical_offset,
            caret_bottom_rel
        );
    }

    #[test]
    fn test_right_click_bubble_opens_context_menu() {
        let messages_signal = seed_messages_signal();
        let controller = ContextMenuController::new();
        let view = ContextMenu::new(
            ChatScreen {
                conv_id: ConvId(1),
                messages: messages_signal,
                avatar: seed_avatar(ConvId(1)),
                me_avatar: seed_me_avatar(),
                on_send: Rc::new(|_| ()),
                on_react: Rc::new(|_, _| ()),
                scroll_controller: ScrollController::new(),
                context_menu: controller.clone(),
                file_picker: crate::test_util::test_file_picker(),
            },
            controller.clone(),
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Before right-click: no "Copy" in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            !find_text_in_tree(ro_reg, root, "Copy"),
            "menu should not be visible before right-click"
        );

        // Right-click at a position inside the first message bubble.
        // The message list is inside a ScrollView with 12px padding.
        // The first bubble starts at approximately (12 + 32 + 8, 12) = (52, 12)
        // (avatar 32px + gap 8px + 12px list padding). Click at (60, 20).
        let secondary_press = vexo::input::InputEvent::PointerButton {
            position: vexo::core::Point::new(60.0, 20.0),
            button: vexo::input::PointerButton::Secondary,
            state: vexo::input::ButtonState::Pressed,
        };
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        pipeline.handle_event(
            vexo::core::Point::new(60.0, 20.0),
            &secondary_press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // After right-click: "Copy" should appear in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, "Copy"),
            "menu item 'Copy' should appear in render tree after right-clicking a bubble"
        );
    }

    #[test]
    fn test_right_click_menu_contains_reactions_and_items() {
        // Regression + presence net for the styled menu: after right-click,
        // the render tree must contain all three item labels (Copy/Reply/Delete).
        // Reaction icons are FA codepoints (not human-readable), so we don't
        // assert them here — their presence is visually verified.
        let messages_signal = seed_messages_signal();
        let controller = ContextMenuController::new();
        let view = ContextMenu::new(
            ChatScreen {
                conv_id: ConvId(1),
                messages: messages_signal,
                avatar: seed_avatar(ConvId(1)),
                me_avatar: seed_me_avatar(),
                on_send: Rc::new(|_| ()),
                on_react: Rc::new(|_, _| ()),
                scroll_controller: ScrollController::new(),
                context_menu: controller.clone(),
                file_picker: crate::test_util::test_file_picker(),
            },
            controller.clone(),
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Right-click at a position inside the first message bubble.
        // (Same coordinates as test_right_click_bubble_opens_context_menu:
        // first bubble starts at approx (52, 12) — avatar 32 + gap 8 + 12 pad.)
        let secondary_press = vexo::input::InputEvent::PointerButton {
            position: vexo::core::Point::new(60.0, 20.0),
            button: vexo::input::PointerButton::Secondary,
            state: vexo::input::ButtonState::Pressed,
        };
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        pipeline.handle_event(
            vexo::core::Point::new(60.0, 20.0),
            &secondary_press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // All three item labels must appear in the render tree.
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        for label in ["Copy", "Reply", "Delete"] {
            assert!(
                find_text_in_tree(ro_reg, root, label),
                "menu item '{}' should appear in render tree after right-clicking a bubble",
                label,
            );
        }
    }

    /// After the `build_message_bubble` → `build_bubble` + `assemble_row`
    /// refactor, the trigger wraps ONLY the bubble (not the avatar). Right-
    /// clicking the avatar area must NOT open the menu — the trigger's bounds
    /// are the bubble's bounds, not the full-width row's.
    #[test]
    fn test_right_click_on_avatar_does_not_open_menu() {
        let messages_signal = seed_messages_signal();
        let controller = ContextMenuController::new();
        let view = ContextMenu::new(
            ChatScreen {
                conv_id: ConvId(1),
                messages: messages_signal,
                avatar: seed_avatar(ConvId(1)),
                me_avatar: seed_me_avatar(),
                on_send: Rc::new(|_| ()),
                on_react: Rc::new(|_, _| ()),
                scroll_controller: ScrollController::new(),
                context_menu: controller.clone(),
                file_picker: crate::test_util::test_file_picker(),
            },
            controller.clone(),
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Right-click at the avatar position (x=15, before the bubble at
        // x≈52). The first row's layout is: 12px list padding | 32px avatar |
        // 8px gap | bubble. So x=15 lands on the avatar, NOT the bubble.
        let secondary_press = vexo::input::InputEvent::PointerButton {
            position: vexo::core::Point::new(15.0, 20.0),
            button: vexo::input::PointerButton::Secondary,
            state: vexo::input::ButtonState::Pressed,
        };
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        pipeline.handle_event(
            vexo::core::Point::new(15.0, 20.0),
            &secondary_press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();

        assert_eq!(
            controller.phase(),
            vexo_uikit::Phase::Closed,
            "right-click on avatar should NOT open the menu (trigger wraps bubble only)"
        );
    }

    #[test]
    fn test_left_click_bubble_does_not_open_context_menu() {
        let messages_signal = seed_messages_signal();
        let controller = ContextMenuController::new();
        let view = ContextMenu::new(
            ChatScreen {
                conv_id: ConvId(1),
                messages: messages_signal,
                avatar: seed_avatar(ConvId(1)),
                me_avatar: seed_me_avatar(),
                on_send: Rc::new(|_| ()),
                on_react: Rc::new(|_, _| ()),
                scroll_controller: ScrollController::new(),
                context_menu: controller.clone(),
                file_picker: crate::test_util::test_file_picker(),
            },
            controller.clone(),
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Left-click at a position inside the first message bubble.
        let primary_press = vexo::input::InputEvent::PointerButton {
            position: vexo::core::Point::new(60.0, 20.0),
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Pressed,
        };
        let primary_release = vexo::input::InputEvent::PointerButton {
            position: vexo::core::Point::new(60.0, 20.0),
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Released,
        };
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        pipeline.handle_event(
            vexo::core::Point::new(60.0, 20.0),
            &primary_press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.handle_event(
            vexo::core::Point::new(60.0, 20.0),
            &primary_release,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();

        assert_eq!(
            controller.phase(),
            vexo_uikit::Phase::Closed,
            "left-click should NOT open the context menu"
        );
    }

    /// Walk the render tree and count `DecoratedBoxRenderObject`s whose
    /// `corner_radius` ≈ `radius`. Used by the reaction-chip tests to count
    /// chips (corner_radius=10.0) — the chip's 20px circle uses
    /// `corner_radius(10.0)`, distinct from the bubble (12.0) and the input
    /// bar's TextEdit DecoratedBox (6.0).
    fn count_decorated_by_corner_radius(
        reg: &RenderObjectRegistry,
        key: RenderObjectKey,
        radius: f32,
    ) -> usize {
        let mut count = 0;
        if let Some(ro) = reg.get(key) {
            let matches = ro
                .as_any()
                .downcast_ref::<vexo::render_objects::DecoratedBoxRenderObject>()
                .map_or(false, |d| {
                    d.style()
                        .corner_radius
                        .as_ref()
                        .map_or(false, |cr| (cr.radius - radius).abs() < 0.01)
                });
            if matches {
                count += 1;
            }
            for &child in ro.children() {
                count += count_decorated_by_corner_radius(reg, child, radius);
            }
        }
        count
    }

    /// Chip-presence test (Q13): seed data has 2 reactions on ConvId(1)
    /// (messages 0 and 2 — `Like` and `Love`). After layout, the render tree
    /// must contain exactly 2 chip circles (DecoratedBox corner_radius=10.0),
    /// proving the chip renders for pre-seeded reactions without any user
    /// interaction.
    #[test]
    fn test_reaction_chip_renders_for_seeded_reactions() {
        let messages_signal = seed_messages_signal();
        let controller = ContextMenuController::new();
        let view = ContextMenu::new(
            ChatScreen {
                conv_id: ConvId(1),
                messages: messages_signal,
                avatar: seed_avatar(ConvId(1)),
                me_avatar: seed_me_avatar(),
                on_send: Rc::new(|_| ()),
                on_react: Rc::new(|_, _| ()),
                scroll_controller: ScrollController::new(),
                context_menu: controller.clone(),
                file_picker: crate::test_util::test_file_picker(),
            },
            controller,
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        let chip_count = count_decorated_by_corner_radius(ro_reg, root, 10.0);
        assert_eq!(
            chip_count, 2,
            "seed data has 2 reactions on ConvId(1) → expected 2 chips (corner_radius=10); got {}",
            chip_count,
        );
    }

    /// End-to-end toggle test (Q13): seed state (2 chips) → invoke
    /// `on_react(1, Like)` on the unreactioned message → rebuild → assert 3
    /// chips → invoke `on_react(1, Like)` again → rebuild → assert back to 2
    /// chips. Exercises the full Signal→derive→rebuild→render cycle for
    /// reactions, mirroring the existing send-message test.
    #[test]
    fn test_reaction_toggle_end_to_end() {
        let messages_signal = seed_messages_signal();
        let controller = ContextMenuController::new();

        // `on_react` mirrors the production wiring: get_cloned → apply_reaction
        // → set_from. Captures the root signal so the mutation propagates to
        // ChatScreen's derived signal.
        let msgs_for_react = messages_signal.clone();
        let on_react: Rc<dyn Fn(usize, ReactionType)> = Rc::new(move |index, rt| {
            let mut map = msgs_for_react.get_cloned();
            if let Some(vec) = map.get_mut(&ConvId(1)) {
                crate::data::apply_reaction(vec, index, rt);
            }
            msgs_for_react.set_from(&map);
        });

        let view = ContextMenu::new(
            ChatScreen {
                conv_id: ConvId(1),
                messages: messages_signal,
                avatar: seed_avatar(ConvId(1)),
                me_avatar: seed_me_avatar(),
                on_send: Rc::new(|_| ()),
                on_react: on_react.clone(),
                scroll_controller: ScrollController::new(),
                context_menu: controller.clone(),
                file_picker: crate::test_util::test_file_picker(),
            },
            controller,
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        // Baseline: 2 seeded chips (messages 0 and 2 have reactions).
        assert_eq!(
            count_decorated_by_corner_radius(ro_reg, root, 10.0),
            2,
            "baseline: 2 seeded reactions → 2 chips",
        );

        // Toggle ON: react with `Like` on message 1 (unreactioned in seed).
        on_react(1, ReactionType::Like);
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert_eq!(
            count_decorated_by_corner_radius(ro_reg, root, 10.0),
            3,
            "after on_react(1, Like): 3 reactions → 3 chips",
        );

        // Toggle OFF: same reaction on same message clears it.
        on_react(1, ReactionType::Like);
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert_eq!(
            count_decorated_by_corner_radius(ro_reg, root, 10.0),
            2,
            "after on_react(1, Like) again: toggle cleared → back to 2 chips",
        );
    }

    /// Regression for accumulate semantics (replaces the old one-reaction-
    /// per-message replace model). On a single message, reacting with `Like`
    /// then `Love` then `Haha` must produce THREE chips under that bubble
    /// (not one chip showing the latest). Tapping `Love` again then removes
    /// only the `Love` chip, leaving `Like` and `Haha`.
    ///
    /// Counts chips across the whole tree: baseline 2 (seed) → +1 = 3 →
    /// +1 = 4 → +1 = 5 (three on message 1) → toggle off Love = 4. Each
    /// step exercises the full Signal→derive→rebuild→render cycle.
    #[test]
    fn test_reaction_accumulates_multiple_chips() {
        let messages_signal = seed_messages_signal();
        let messages_for_check = messages_signal.clone();
        let controller = ContextMenuController::new();

        let msgs_for_react = messages_signal.clone();
        let on_react: Rc<dyn Fn(usize, ReactionType)> = Rc::new(move |index, rt| {
            let mut map = msgs_for_react.get_cloned();
            if let Some(vec) = map.get_mut(&ConvId(1)) {
                crate::data::apply_reaction(vec, index, rt);
            }
            msgs_for_react.set_from(&map);
        });

        let view = ContextMenu::new(
            ChatScreen {
                conv_id: ConvId(1),
                messages: messages_signal,
                avatar: seed_avatar(ConvId(1)),
                me_avatar: seed_me_avatar(),
                on_send: Rc::new(|_| ()),
                on_react: on_react.clone(),
                scroll_controller: ScrollController::new(),
                context_menu: controller.clone(),
                file_picker: crate::test_util::test_file_picker(),
            },
            controller,
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        // Baseline: 2 seeded chips (messages 0 and 2 each have one reaction).
        {
            let ro_reg = pipeline.render_objects();
            let root = ro_reg.root().expect("root");
            assert_eq!(
                count_decorated_by_corner_radius(ro_reg, root, 10.0),
                2,
                "baseline: 2 seeded chips",
            );
        }

        // First reaction on message 1: [Like] → one new chip → total 3.
        on_react(1, ReactionType::Like);
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        {
            let ro_reg = pipeline.render_objects();
            let root = ro_reg.root().expect("root");
            assert_eq!(
                count_decorated_by_corner_radius(ro_reg, root, 10.0),
                3,
                "after on_react(1, Like): message 1 now has 1 chip → total 3",
            );
        }

        // Second DISTINCT reaction on the SAME message: under the old replace
        // model this would stay at 3 (Love replaces Like). Under accumulate
        // semantics it must become 4 (Like + Love both present).
        on_react(1, ReactionType::Love);
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        {
            let ro_reg = pipeline.render_objects();
            let root = ro_reg.root().expect("root");
            assert_eq!(
                count_decorated_by_corner_radius(ro_reg, root, 10.0),
                4,
                "after on_react(1, Love): message 1 should ACCUMULATE to 2 chips \
                 (Like + Love), not replace → total 4. If this is 3, the old \
                 replace semantics are still in effect.",
            );
        }

        // Third distinct reaction: [Like, Love, Haha] → total 5.
        on_react(1, ReactionType::Haha);
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        {
            let ro_reg = pipeline.render_objects();
            let root = ro_reg.root().expect("root");
            assert_eq!(
                count_decorated_by_corner_radius(ro_reg, root, 10.0),
                5,
                "after on_react(1, Haha): message 1 has 3 chips → total 5",
            );
        }

        // Toggle off the middle one: [Like, Love, Haha] → [Like, Haha] →
        // total 4. Verifies toggle-off removes ONLY the tapped reaction,
        // preserving the others.
        on_react(1, ReactionType::Love);
        pipeline.perform_rebuilds();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        {
            let ro_reg = pipeline.render_objects();
            let root = ro_reg.root().expect("root");
            assert_eq!(
                count_decorated_by_corner_radius(ro_reg, root, 10.0),
                4,
                "after on_react(1, Love) again: toggle-off removes only Love; \
                 Like and Haha remain → total 4",
            );
        }

        // Final state sanity: message 1 should have exactly [Like, Haha].
        let msgs = messages_for_check
            .get_cloned()
            .get(&ConvId(1))
            .expect("ConvId(1) exists")
            .clone();
        assert_eq!(
            msgs[1].reactions,
            vec![ReactionType::Like, ReactionType::Haha],
            "message 1 final reactions should be [Like, Haha] after the \
             sequence",
        );
    }

    /// Regression for "every keystroke in the input bar rebuilds the entire
    /// ChatScreen (all message bubbles) instead of just the TextEdit."
    ///
    /// ChatScreen::on_update used to re-set the TextEditingController's dirty
    /// callback to ChatScreen's own dirty callback on every parent cascade.
    /// Since the controller is Rc-shared with the child TextEdit, and
    /// TextEdit::on_update only re-sets when the controller Rc changes (which
    /// it never does — same State-owned controller), ChatScreen's callback
    /// permanently won after the first cascade. Every keystroke then marked
    /// ChatScreen dirty, triggering a full re-render of all message bubbles.
    ///
    /// The fix: don't touch the controller's dirty callback in ChatScreen at
    /// all — let TextEdit own it. This test verifies that a keystroke does
    /// NOT trigger ChatScreen.render().
    #[test]
    fn test_keystroke_does_not_rebuild_chat_screen() {
        let messages_signal = seed_messages_signal();
        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal,
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );
        pipeline.perform_rebuilds();

        // Simulate a keyboard-animation frame: cascade with identical
        // conv_id. should_rebuild returns false, so render() is skipped.
        // But on_update DOES run — and before the fix, it overwrote the
        // controller's dirty callback with ChatScreen's callback.
        let same_view = ChatScreen {
            conv_id: ConvId(1),
            messages: seed_messages_signal(),
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();
        pipeline.update(same_view);
        pipeline.perform_rebuilds();

        // Click on the TextEdit to focus it. The input bar is at the bottom
        // of the 600px view (8px padding); x=50 is inside the TextEdit.
        let click_pos = vexo::core::Point::new(50.0, 580.0);
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        let press = vexo::input::InputEvent::PointerButton {
            position: click_pos,
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Pressed,
        };
        let release = vexo::input::InputEvent::PointerButton {
            position: click_pos,
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Released,
        };
        pipeline.handle_event(
            click_pos,
            &press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.handle_event(
            click_pos,
            &release,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();

        // Reset the counter — we only care about the keystroke below.
        CHAT_SCREEN_RENDER_COUNT.with(|c| c.set(0));

        // Type a character. The controller's notify() should fire TextEdit's
        // dirty callback (not ChatScreen's), so only TextEdit rebuilds.
        let key_event = vexo::input::InputEvent::Keyboard {
            key: vexo::input::Key::Character("a".to_string()),
            text: Some("a".to_string()),
            state: vexo::input::ButtonState::Pressed,
            modifiers: vexo::input::Modifiers::default(),
        };
        pipeline.handle_event(
            click_pos,
            &key_event,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();

        // ChatScreen.render() should NOT have been called — only TextEdit
        // should have rebuilt.
        CHAT_SCREEN_RENDER_COUNT.with(|c| {
            assert_eq!(
                c.get(),
                0,
                "ChatScreen.render() should NOT run on keystroke — only \
                 TextEdit should rebuild. If this fails, ChatScreen is \
                 overwriting the controller's dirty callback in on_mount \
                 or on_update."
            );
        });
    }

    /// A file message with image bytes renders an `ImageRenderObject` in the
    /// render tree (the thumbnail), not just text.
    #[test]
    fn test_file_message_renders_image_thumbnail() {
        let png_bytes: Arc<[u8]> = Arc::from(crate::data::make_avatar_png(255, 100, 50).to_vec());
        let mut messages_map: std::collections::HashMap<ConvId, Vec<Message>> =
            std::collections::HashMap::new();
        messages_map.insert(
            ConvId(1),
            vec![Message {
                author: MessageAuthor::Them,
                kind: MessageKind::File(crate::data::FileAttachment {
                    name: "photo.png".into(),
                    mime: "image/png".into(),
                    size: png_bytes.len() as u64,
                    bytes: png_bytes,
                }),
                timestamp: 1732347000,
                reactions: vec![],
            }],
        );
        let messages_signal = vexo::Signal::new(messages_map);

        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal,
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        // Return the ImageRenderObject's computed bounds if one exists in the
        // tree. We assert not just presence but NON-ZERO bounds: a 0x0 image
        // renders nothing (ImageRenderObject::paint guards on width>0 &&
        // height>0), which is the regression where the WithLayout wrapper
        // collapsed because it had no explicit width/height.
        fn find_image_bounds(
            reg: &RenderObjectRegistry,
            key: RenderObjectKey,
        ) -> Option<vexo::core::Bounds<vexo::core::Logical>> {
            let ro = reg.get(key)?;
            if let Some(img) = ro
                .as_any()
                .downcast_ref::<vexo::render_objects::ImageRenderObject>()
            {
                return img.computed_bounds();
            }
            for &child in ro.children() {
                if let Some(b) = find_image_bounds(reg, child) {
                    return Some(b);
                }
            }
            None
        }

        let img_bounds = find_image_bounds(ro_reg, root)
            .expect("file message with image bytes should render an ImageRenderObject (thumbnail)");
        assert!(
            img_bounds.width() > 0.0 && img_bounds.height() > 0.0,
            "image thumbnail must have non-zero bounds to actually paint; \
             got {:?}",
            img_bounds
        );
    }

    /// A file message with non-image bytes renders the filename and size
    /// as text in the render tree (the file card), not an image thumbnail.
    #[test]
    fn test_file_message_renders_file_card_for_non_image() {
        let file_bytes: Arc<[u8]> = Arc::from(b"%PDF-1.4 fake pdf content".to_vec());
        let mut messages_map: std::collections::HashMap<ConvId, Vec<Message>> =
            std::collections::HashMap::new();
        let file_name = "report.pdf".to_string();
        messages_map.insert(
            ConvId(1),
            vec![Message {
                author: MessageAuthor::Them,
                kind: MessageKind::File(crate::data::FileAttachment {
                    name: file_name.clone(),
                    mime: String::new(),
                    size: file_bytes.len() as u64,
                    bytes: file_bytes,
                }),
                timestamp: 1732347000,
                reactions: vec![],
            }],
        );
        let messages_signal = vexo::Signal::new(messages_map);

        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal,
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");
        assert!(
            find_text_in_tree(ro_reg, root, &file_name),
            "file card should render the filename '{}' as text",
            file_name
        );
    }

    /// Tapping the attach button calls the mock FilePicker, which returns
    /// canned PNG bytes. The `on_send` callback fires with
    /// `MessageKind::File(...)`, and the file message appears in the
    /// render tree (the filename "test.png" renders as text in the
    /// file-card OR the thumbnail Image renders — we assert the filename
    /// appears via the file-card path by using a small PNG that decodes
    /// as a thumbnail, so we assert the message count grew instead).
    #[test]
    fn test_attach_button_sends_file_message() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let messages_signal = seed_messages_signal();
        let send_count = Arc::new(AtomicUsize::new(0));
        let send_count_for_closure = send_count.clone();
        let msgs_for_send = messages_signal.clone();

        let picker = crate::test_util::mock_png_picker();
        let picker_arc: std::sync::Arc<dyn vexo::platform::file_picker::FilePicker> =
            picker.clone();

        let on_send: Rc<dyn Fn(MessageKind)> = Rc::new(move |kind| {
            send_count_for_closure.fetch_add(1, Ordering::SeqCst);
            let mut map = msgs_for_send.get_cloned();
            if let Some(vec) = map.get_mut(&ConvId(1)) {
                vec.push(Message {
                    author: MessageAuthor::Me,
                    kind,
                    timestamp: 1732348000,
                    reactions: vec![],
                });
            }
            msgs_for_send.set_from(&map);
        });

        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal.clone(),
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send: on_send.clone(),
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: picker_arc,
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let baseline_msgs = messages_signal.get_cloned().get(&ConvId(1)).unwrap().len();

        // The attach button is at the left of the input bar, which is at
        // the bottom of the 600px view. The input bar has 8px padding.
        // Click at x=20 (left side, where the attach button lives), y=580.
        let click_pos = vexo::core::Point::new(20.0, 580.0);
        let press = vexo::input::InputEvent::PointerButton {
            position: click_pos,
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Pressed,
        };
        let release = vexo::input::InputEvent::PointerButton {
            position: click_pos,
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Released,
        };
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        pipeline.handle_event(
            click_pos,
            &press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.handle_event(
            click_pos,
            &release,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();

        assert_eq!(
            send_count.load(Ordering::SeqCst),
            1,
            "on_send should fire exactly once after tapping attach"
        );
        let after_msgs_map = messages_signal.get_cloned();
        let after_msgs = after_msgs_map.get(&ConvId(1)).unwrap().len();
        assert_eq!(
            after_msgs,
            baseline_msgs + 1,
            "a new message should be appended after tapping attach"
        );
        let last_msg = after_msgs_map
            .get(&ConvId(1))
            .unwrap()
            .last()
            .expect("at least one message after attach");
        assert!(
            matches!(&last_msg.kind, MessageKind::File(_)),
            "the sent message should be MessageKind::File, got {:?}",
            last_msg.kind
        );
    }

    /// When the FilePicker returns `None` (user cancels), tapping the
    /// attach button does NOT send a message.
    #[test]
    fn test_attach_button_picker_none_does_not_send() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let messages_signal = seed_messages_signal();
        let send_count = Arc::new(AtomicUsize::new(0));
        let send_count_for_closure = send_count.clone();

        let on_send: Rc<dyn Fn(MessageKind)> = Rc::new(move |_kind| {
            send_count_for_closure.fetch_add(1, Ordering::SeqCst);
        });

        let view = ChatScreen {
            conv_id: ConvId(1),
            messages: messages_signal,
            avatar: seed_avatar(ConvId(1)),
            me_avatar: seed_me_avatar(),
            on_send,
            on_react: Rc::new(|_, _| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
            file_picker: crate::test_util::test_file_picker(),
        }
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = vexo::resource::new_font_system();
        vexo_fontawesome::register_fonts(&mut font_system);
        pipeline.layout(
            vexo::core::Size::new(400.0, 600.0),
            &mut engine,
            &mut font_system,
        );

        let click_pos = vexo::core::Point::new(20.0, 580.0);
        let press = vexo::input::InputEvent::PointerButton {
            position: click_pos,
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Pressed,
        };
        let release = vexo::input::InputEvent::PointerButton {
            position: click_pos,
            button: vexo::input::PointerButton::Primary,
            state: vexo::input::ButtonState::Released,
        };
        let clipboard: std::sync::Arc<dyn vexo::platform::Clipboard> =
            std::sync::Arc::new(vexo::platform::stub_clipboard::StubClipboard);
        pipeline.handle_event(
            click_pos,
            &press,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.handle_event(
            click_pos,
            &release,
            vexo::input::Modifiers::default(),
            &mut font_system,
            &vexo::core::ScaleSource::default(),
            &clipboard,
        );
        pipeline.perform_rebuilds();

        assert_eq!(
            send_count.load(Ordering::SeqCst),
            0,
            "on_send should NOT fire when picker returns None (cancel)"
        );
    }
}
