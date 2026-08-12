//! Conversation list — unified component used by both PC and Mobile.
//!
//! Both platforms render the same theme-token-aware rows. Mobile passes
//! `selected = None` (no row highlight); desktop passes the live selection
//! so the active conversation is highlighted.
//!
//! `ConversationList` is a `Component` that subscribes to the `messages`
//! Signal via `ctx.depend_on_signal`, deriving each row's preview/timestamp from
//! the latest message (with seed fallback). This state-driven rebuild
//! bypasses `should_rebuild` gates (TabBarView, NavigationStackView) so the
//! list refreshes on mobile even while the chat screen is pushed.
//!
//! Each row is itself a `Component` (`ConversationRow`) owning its own
//! `is_hovered` Signal, so a hover toggle rebuilds only that row, not the
//! whole list. Hover is gated to `Platform::Desktop` (mirrors `Button`):
//! mobile has no hover input device, so the hover color is dead code there.
//! When a row is selected, hover is suppressed — selection is authoritative
//! (macOS Finder/Mail convention).

use std::collections::HashMap;
use std::rc::Rc;

use vexo::{
    column, row, Component, ComponentState, DecoratedBox, ImageData, Layout, Positioned,
    RenderContext, ScrollView, Signal, SimpleState, Stack, Style, Text, Theme, Widget, WithLayout,
};
use vexo_uikit::platform::Platform;
use vexo_uikit::theme::tokens::navigation::{self, ROW_INSET, ROW_PILL_RADIUS};

use crate::data::{AvatarSource, ConvId, Conversation, Message};
use crate::widgets::avatar::{avatar, avatar_border_ring, network_avatar, unread_badge};

pub(crate) struct ConversationList {
    pub(crate) conversations: Vec<Conversation>,
    pub(crate) messages: Signal<HashMap<ConvId, Vec<Message>>>,
    pub(crate) selected: Option<ConvId>,
    pub(crate) on_select: Rc<dyn Fn(ConvId)>,
}

impl Clone for ConversationList {
    fn clone(&self) -> Self {
        Self {
            conversations: self.conversations.clone(),
            messages: self.messages.clone(),
            selected: self.selected.clone(),
            on_select: Rc::clone(&self.on_select),
        }
    }
}

impl Component for ConversationList {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let nav_colors = navigation::colors(&theme);
        let messages = ctx.depend_on_signal(&self.messages);

        let list = column! {
            for conv in &self.conversations {
                ConversationRow::from_conv(conv, &self.selected, &self.on_select, &messages)
            }
        };
        // Paint a themed background behind the list so the pane isn't left
        // showing the window's white clear in dark mode. Rows are transparent
        // when unselected, so this background is what the user sees between rows.
        DecoratedBox::with_style(
            WithLayout::new(ScrollView::new(list.boxed()), Layout::flex_fill()),
            Style::default().background(nav_colors.detail_bg),
        )
        .boxed()
    }
}

/// State for `ConversationRow`. Tracks hover via a reactive `Signal`; the
/// `#[derive(ComponentState)]` auto-wires the signal so the element is marked
/// dirty on `Signal::set`. Mirrors `ButtonState` (minus `is_pressed`, which
/// the row doesn't need — selection already provides click feedback).
#[derive(ComponentState, Default)]
struct ConversationRowState {
    is_hovered: Signal<bool>,
}

/// A single conversation row. Owns its hover state so hover toggles rebuild
/// only this row, not the entire list.
///
/// Inputs are owned scalars (avatar bytes behind `Rc<[u8]>` to avoid cloning
/// the PNG per row). `is_selected` is computed by the parent list and passed
/// in — the row does not know its own `ConvId` for selection, only whether
/// *this* row is currently selected. The tap callback is a pre-built
/// `Rc<dyn Fn()>` (the list closes over `on_select` + `id`), so the row is
/// decoupled from the selection contract.
#[derive(Clone)]
struct ConversationRow {
    name: String,
    avatar: AvatarSource,
    unread_count: u32,
    preview: String,
    timestamp: u64,
    is_selected: bool,
    platform: Option<Platform>,
    on_tap: Rc<dyn Fn()>,
}

impl ConversationRow {
    fn from_conv(
        conv: &Conversation,
        selected: &Option<ConvId>,
        on_select: &Rc<dyn Fn(ConvId)>,
        messages: &HashMap<ConvId, Vec<Message>>,
    ) -> Self {
        let on_select = Rc::clone(on_select);
        let id = conv.id.clone();
        let (preview, timestamp) = latest_preview(conv, messages);
        Self {
            name: conv.name.clone(),
            avatar: conv.avatar.clone(),
            unread_count: conv.unread_count,
            preview,
            timestamp,
            is_selected: *selected == Some(conv.id.clone()),
            platform: None,
            on_tap: Rc::new(move || on_select(id.clone())),
        }
    }

    fn effective_platform(&self) -> Platform {
        self.platform.unwrap_or_else(Platform::current)
    }
}

impl Component for ConversationRow {
    type State = ConversationRowState;

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let nav_colors = navigation::colors(&theme);
        let is_hovered = state.is_hovered.get();

        let avatar: Box<dyn Widget> = match &self.avatar {
            AvatarSource::Bytes(bytes) => avatar(
                ImageData::from_bytes(bytes).expect("avatar bytes are valid PNG"),
                40.0,
            ),
            AvatarSource::Url(url) => network_avatar(url.clone(), 40.0),
        };

        // 1px outline ring so a white/clear-background avatar still reads as
        // a circle against the (white, in light mode) pane. Paints on top of
        // the image via Stack push order; the badge sits above the ring.
        let border_ring = avatar_border_ring(40.0, theme.outline);

        let name_color = nav_colors.row_text;
        let preview_color = nav_colors.placeholder_text;

        let name_text = Text::new(self.name.as_str())
            .with_font_size(16.0)
            .with_color(name_color);
        let preview_text = Text::new(self.preview.as_str())
            .with_font_size(13.0)
            .with_color(preview_color)
            .with_max_lines(1);

        let info_col = column! { name_text, preview_text }.gap(2.0).flex_grow(1.0);

        let time_text = Text::new(format_timestamp(self.timestamp).as_str())
            .with_font_size(12.0)
            .with_color(name_color);

        let right_col = column! { time_text }.flex_shrink(0.0);

        let badge: Option<Box<dyn Widget>> = if self.unread_count > 0 {
            Some(
                Positioned::new(unread_badge(self.unread_count, &theme))
                    .top(-4.0)
                    .right(-4.0)
                    .boxed(),
            )
        } else {
            None
        };

        let avatar_with_badge = Stack::new()
            .with_layout(Layout::stack().width(40.0).height(40.0).flex_shrink(0.0))
            .push(avatar)
            .push(border_ring)
            .push(badge)
            .boxed();

        // Precedence: selected > hover (desktop only) > transparent.
        // Hover is suppressed when selected so the selected row stays visually
        // anchored (macOS Finder/Mail pattern).
        let row_bg: Option<vexo::Color> = if self.is_selected {
            Some(nav_colors.sidebar_selected_bg)
        } else if is_hovered && self.effective_platform() == Platform::Desktop {
            Some(nav_colors.row_hover_bg)
        } else {
            None
        };

        let inner = WithLayout::new(
            row! { avatar_with_badge, info_col, right_col }.gap(12.0),
            Layout::default().padding(12.0),
        );

        // Pill geometry: a 4px horizontal margin (desktop-only) is applied
        // unconditionally so content position is stable across
        // selected/hovered/unselected states. The margin lives on the outer
        // `WithLayout` (always present); the background+radius live on the
        // inner `DecoratedBox` (only painted when `row_bg` is `Some`).
        //
        // Mobile skips the inset — it never paints a pill (no selection, no
        // hover), so paying the layout cost there would shift content for no
        // benefit. See `navigation::ROW_INSET` for the deferral rationale.
        let pill_margin = if self.effective_platform() == Platform::Desktop {
            ROW_INSET
        } else {
            0.0
        };

        let pill = WithLayout::new(
            inner,
            Layout::default().margin_each(pill_margin, pill_margin, 0.0, 0.0),
        );

        // Conditionally wrap in DecoratedBox only when there's a background
        // to paint — avoids a no-op DecoratedBox render object per row in the
        // common (unselected, unhovered) case. The radius rides along with
        // the background; both are conditional on the same `row_bg`.
        let root: Box<dyn Widget> = if let Some(bg) = row_bg {
            DecoratedBox::with_style(
                pill,
                Style::default()
                    .background(bg)
                    .corner_radius(ROW_PILL_RADIUS),
            )
            .boxed()
        } else {
            pill.boxed()
        };

        let is_hovered_signal = state.is_hovered.clone();
        let is_hovered_signal_exit = state.is_hovered.clone();
        let on_tap_cb = Rc::clone(&self.on_tap);

        root.on_enter(move || {
            is_hovered_signal.set(true);
        })
        .on_exit(move || {
            is_hovered_signal_exit.set(false);
        })
        .on_tap(move || {
            on_tap_cb();
        })
    }
}

/// Derive the (preview, timestamp) for a conversation's row from the latest
/// message in the map, falling back to the conversation's seed values when
/// no messages exist (or the conversation is absent from the map).
fn latest_preview(conv: &Conversation, messages: &HashMap<ConvId, Vec<Message>>) -> (String, u64) {
    messages
        .get(&conv.id)
        .and_then(|v| v.last())
        .map(|m| (m.text.clone(), m.timestamp))
        .unwrap_or_else(|| (conv.last_preview.clone(), conv.last_timestamp))
}

fn format_timestamp(ts: u64) -> String {
    let secs = ts % 86400;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let formatted: String = format!("{:02}:{:02}", hours, mins);
    formatted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Message, MessageAuthor};
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::ThreeTreePipeline;

    #[test]
    fn test_conversation_list_renders_in_pipeline() {
        let state = crate::data::seed();
        let view = ConversationList {
            conversations: state.conversations.clone(),
            messages: state.messages.clone(),
            selected: None,
            on_select: Rc::new(|_| {}),
        }
        .boxed();
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        crate::test_util::install_test_image_cache(&mut pipeline);
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 5,
            "expected multiple elements for 25 conversation rows"
        );
    }

    #[test]
    fn test_latest_preview_uses_last_message() {
        let state = crate::data::seed();
        let conv = state
            .conversations
            .iter()
            .find(|c| c.id == ConvId(1))
            .unwrap();
        let mut messages = state.messages.get_cloned();
        messages.get_mut(&ConvId(1)).unwrap().push(Message {
            author: MessageAuthor::Me,
            text: "New latest message".into(),
            timestamp: 1732399999,
            reactions: vec![],
        });
        let (preview, ts) = latest_preview(conv, &messages);
        assert_eq!(preview, "New latest message");
        assert_eq!(ts, 1732399999);
    }

    #[test]
    fn test_latest_preview_falls_back_for_empty_vec() {
        let state = crate::data::seed();
        let conv = state
            .conversations
            .iter()
            .find(|c| c.id == ConvId(4))
            .unwrap();
        let messages = state.messages.get_cloned();
        let (preview, ts) = latest_preview(conv, &messages);
        assert_eq!(preview, conv.last_preview);
        assert_eq!(ts, conv.last_timestamp);
    }

    #[test]
    fn test_latest_preview_falls_back_when_absent_from_map() {
        let state = crate::data::seed();
        let conv = state
            .conversations
            .iter()
            .find(|c| c.id == ConvId(1))
            .unwrap();
        let empty_map: HashMap<ConvId, Vec<Message>> = HashMap::new();
        let (preview, ts) = latest_preview(conv, &empty_map);
        assert_eq!(preview, conv.last_preview);
        assert_eq!(ts, conv.last_timestamp);
    }
}
