//! Shared frame context for render pipeline stages.

use crate::core::{Physical, Size, WidgetId};
use crate::layout::LayoutView;
use crate::state::CursorBlinkState;

/// Shared read-only data for render pipeline stages.
pub struct FrameContext<'a> {
    pub scale: crate::core::Scale,
    pub viewport_physical: Size<Physical>,
    pub layout_view: LayoutView<'a>,
    pub focused_widget_id: Option<WidgetId>,
    pub cursor_blink: &'a CursorBlinkState,
}
