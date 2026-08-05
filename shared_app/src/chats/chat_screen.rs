//! Chat screen — the pushed destination when a conversation is tapped.

use std::any::Any;
use std::rc::Rc;

use vexo::{
    column, row, AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox,
    FlexDirection, ImageData, Key, Layout, LifecycleContext, RenderContext, ScrollController,
    ScrollView, Signal, Spacer, Style, Text, TextEdit, TextEditingController, Theme, Widget,
    WidgetKey, WithLayout,
};
use vexo_uikit::{
    context_menu_trigger, Button, ButtonVariant, ContextMenu, ContextMenuController,
    KeyboardAvoider, MenuBuilder,
};

use crate::data::{ConvId, Message, MessageAuthor};
use crate::widgets::avatar::avatar;

pub(crate) struct ChatScreen {
    pub(crate) conv_id: ConvId,
    pub(crate) messages: Signal<Vec<Message>>,
    pub(crate) avatar_bytes: Rc<[u8]>,
    pub(crate) me_avatar_bytes: Rc<[u8]>,
    pub(crate) on_send: Rc<dyn Fn(&str)>,
    pub(crate) scroll_controller: ScrollController,
    pub(crate) context_menu: ContextMenuController,
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
            context_menu: self.context_menu.clone(),
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

        let ctrl = self.context_menu.clone();
        let list = column! {
            for msg in &messages {
                context_menu_trigger(
                    build_message_bubble(
                        msg,
                        state.them_avatar(&self.avatar_bytes).clone(),
                        state.me_avatar(&self.me_avatar_bytes).clone(),
                        &theme,
                    ),
                    ctrl.clone(),
                    placeholder_menu_builder(),
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

        let input_bar = build_input_bar(tc, on_send_closure, &theme);

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
            Spacer::new(),
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
            Spacer::new(),
        }
        .gap(8.0)
        .boxed()
    }
}

fn build_input_bar(
    controller: TextEditingController,
    on_send: impl FnMut() + 'static,
    theme: &vexo::ThemeData,
) -> Box<dyn Widget> {
    row! {
        WithLayout::new(
            TextEdit::new(controller)
                .with_background(theme.surface)
                .with_text_color(theme.on_surface)
                .with_border_color(theme.outline),
            Layout::default().flex_grow(1.0),
        ),
        Button::new("Send")
            .variant(ButtonVariant::Primary)
            .shadow(
                BoxShadow::new(Color::BLACK.with_alpha(0.25))
                    .blur(6.0)
                    .offset(0.0, 2.0),
            )
            .on_tap(on_send),
    }
    .gap(8.0)
    .padding(8.0)
    .boxed()
}

fn placeholder_menu_builder() -> MenuBuilder {
    MenuBuilder::new(|ctrl, theme| {
        // (label, log message) pairs. Bound as a single `item` per iteration
        // (not destructured) to stay within the `column!` macro's known
        // `for x in iter` single-binding form.
        let labels: [(&str, &str); 3] = [
            ("Copy", "context menu: Copy"),
            ("Reply", "context menu: Reply"),
            ("Delete", "context menu: Delete"),
        ];
        let column = vexo::column! {
            for item in labels {
                let ctrl = ctrl.clone();
                vexo::GestureDetector::new(
                    vexo::WithLayout::new(
                        vexo::Text::new(item.0).with_color(theme.on_surface),
                        vexo::Layout::default().padding(8.0).width(160.0),
                    ),
                )
                .on_tap(move || {
                    log::debug!("{}", item.1);
                    ctrl.close();
                })
            }
        };
        vexo::DecoratedBox::with_style(
            vexo::WithLayout::new(column, vexo::Layout::default().min_width(160.0)),
            vexo::Style::default()
                .corner_radius(8.0)
                .background(theme.surface)
                .border(theme.outline, 1.0)
                .shadow(
                    vexo::BoxShadow::new(vexo::Color::BLACK.with_alpha(0.25))
                        .blur(6.0)
                        .offset(0.0, 2.0),
                ),
        )
        .boxed()
    })
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

    /// Walk the render tree and return true if any `TextRenderObject` contains
    /// `needle`. `RenderObjectRegistry` exposes no `iter()`, so recurse from
    /// `key` (same pattern as the `test_signal_value_registers_dependency_and_
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
            messages: Signal::derive(messages_signal, |map| {
                map.get(&ConvId(1)).cloned().unwrap_or_default()
            }),
            avatar_bytes: seed_avatar(ConvId(1)),
            me_avatar_bytes: seed_me_avatar(),
            on_send: Rc::new(|_| ()),
            scroll_controller: ScrollController::new(),
            context_menu: ContextMenuController::new(),
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
            context_menu: ContextMenuController::new(),
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
            context_menu: ContextMenuController::new(),
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

    #[test]
    fn test_chat_screen_input_bar_uses_theme_colors() {
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
            context_menu: ContextMenuController::new(),
        };
        let dark_theme = vexo::ThemeData::dark();
        let themed = vexo::Theme::new(dark_theme.clone(), view);

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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

        let input_bar = build_input_bar(controller.clone(), || {}, &theme);

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

        let input_bar = build_input_bar(controller.clone(), || {}, &theme);

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
                messages: Signal::derive(messages_signal, |map| {
                    map.get(&ConvId(1)).cloned().unwrap_or_default()
                }),
                avatar_bytes: seed_avatar(ConvId(1)),
                me_avatar_bytes: seed_me_avatar(),
                on_send: Rc::new(|_| ()),
                scroll_controller: ScrollController::new(),
                context_menu: controller.clone(),
            },
            controller.clone(),
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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
    fn test_left_click_bubble_does_not_open_context_menu() {
        let messages_signal = seed_messages_signal();
        let controller = ContextMenuController::new();
        let view = ContextMenu::new(
            ChatScreen {
                conv_id: ConvId(1),
                messages: Signal::derive(messages_signal, |map| {
                    map.get(&ConvId(1)).cloned().unwrap_or_default()
                }),
                avatar_bytes: seed_avatar(ConvId(1)),
                me_avatar_bytes: seed_me_avatar(),
                on_send: Rc::new(|_| ()),
                scroll_controller: ScrollController::new(),
                context_menu: controller.clone(),
            },
            controller.clone(),
        )
        .boxed();

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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
            controller.position_signal().get(),
            None,
            "left-click should NOT open the context menu"
        );
    }
}
