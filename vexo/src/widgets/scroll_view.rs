//! ScrollView widget - a vertical scrollable container.

use crate::core::{Color, Logical, Point, Rect, WidgetId};
use crate::layout::{FlexDirection, Layout, LayoutContext, LayoutNodeId, LayoutView};
use crate::renderer::UiBatcher;
use crate::render::RenderCommand;
use crate::input::{ButtonState, CursorIcon, InputEvent, Key, NamedKey, PointerButton};
use crate::widgets::{WidgetContext, WidgetResponse};
use crate::Widget;
use std::marker::PhantomData;

// ============================================================================
// SCROLL STATE
// ============================================================================

/// Scroll state stored in ComponentStateStorage.
///
/// This state persists across view rebuilds when the ScrollView has a key.
#[derive(Default, Clone, Debug)]
pub struct ScrollState {
    /// Current vertical scroll offset (0 = top, positive = scrolled down).
    pub offset_y: f32,
    /// Whether user is currently dragging to scroll.
    pub is_dragging: bool,
    /// Y position where drag started (in logical coordinates).
    pub drag_start_y: f32,
    /// Scroll offset when drag started.
    pub drag_start_offset: f32,
}

// ============================================================================
// SCROLL VIEW
// ============================================================================

/// A vertical scrollable container widget.
///
/// ScrollView displays its children in a vertical column and allows scrolling
/// when content exceeds the viewport height. It supports scroll wheel, drag
/// gestures, and keyboard navigation.
///
/// # Example
///
/// ```ignore
/// use vexo::widgets::{ScrollView, Text};
///
/// let scroll = ScrollView::new()
///     .with_key("my-scroll")
///     .push(Text::new("Item 1"))
///     .push(Text::new("Item 2"));
/// ```
pub struct ScrollView<M: Clone + std::fmt::Debug + Send> {
    /// Child widgets.
    children: Vec<Box<dyn Widget<M>>>,
    /// Optional key for state persistence.
    key: Option<String>,
    /// Layout properties for the viewport.
    layout: Layout,
    /// Computed viewport bounds from layout phase.
    computed_layout: Option<crate::testable::ComputedLayout>,
    /// Scrollbar width in logical pixels.
    scrollbar_width: f32,
    _marker: PhantomData<M>,
}

// ============================================================================
// BUILDER API
// ============================================================================

impl<M: Clone + std::fmt::Debug + Send> ScrollView<M> {
    /// Create a new empty ScrollView.
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
            layout: Layout::default(),
            computed_layout: None,
            scrollbar_width: 8.0,
            _marker: PhantomData,
        }
    }

    /// Add a child widget.
    pub fn push(mut self, widget: impl Widget<M> + 'static) -> Self {
        self.children.push(Box::new(widget));
        self
    }

    /// Set a key for state persistence.
    ///
    /// The key allows scroll position to persist across view rebuilds.
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the entire Layout struct.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Set fixed width.
    pub fn width(mut self, value: f32) -> Self {
        self.layout = self.layout.width(value);
        self
    }

    /// Set fixed height.
    pub fn height(mut self, value: f32) -> Self {
        self.layout = self.layout.height(value);
        self
    }

    /// Set scrollbar width in logical pixels.
    pub fn scrollbar_width(mut self, width: f32) -> Self {
        self.scrollbar_width = width;
        self
    }

    /// Draw the scrollbar indicator.
    fn draw_scrollbar(
        &self,
        renderer: &mut UiBatcher,
        viewport: Rect<Logical>,
        offset_y: f32,
        content_height: f32,
    ) {
        let max_scroll = content_height - viewport.size.height;
        if max_scroll <= 0.0 {
            return;
        }

        // Calculate scrollbar dimensions
        let scrollbar_height = (viewport.size.height * viewport.size.height / content_height)
            .min(viewport.size.height)
            .max(20.0); // Minimum thumb size

        let scroll_ratio = offset_y / max_scroll;
        let scrollbar_y = viewport.origin.y + scroll_ratio * (viewport.size.height - scrollbar_height);

        // Draw scrollbar thumb
        let scrollbar_bounds: Rect<Logical> = Rect::from_xywh(
            viewport.origin.x + viewport.size.width - self.scrollbar_width - 2.0,
            scrollbar_y,
            self.scrollbar_width,
            scrollbar_height,
        );

        renderer.add_rect(
            [scrollbar_bounds.origin.x, scrollbar_bounds.origin.y],
            [scrollbar_bounds.size.width, scrollbar_bounds.size.height],
            Color::new(0.5, 0.5, 0.5, 0.5),
            Color::TRANSPARENT,
            self.scrollbar_width / 2.0, // Rounded corners
            0.0,
        );
    }
}

impl<M: Clone + std::fmt::Debug + Send> Default for ScrollView<M> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SEPARATED TRAIT IMPLEMENTATIONS (Minimal for tests)
// ============================================================================

impl<M: Clone + std::fmt::Debug + Send> crate::testable::Identifiable for ScrollView<M> {
    fn id(&self) -> Option<WidgetId> {
        self.key.as_ref().map(|k| WidgetId::from_key(k))
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::testable::Layout for ScrollView<M> {
    fn constraints(&self) -> crate::testable::LayoutConstraints {
        crate::testable::LayoutConstraints::from_layout(&self.layout)
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::testable::Paint for ScrollView<M> {
    fn paint(&self, _ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        // Container widgets don't paint themselves - children paint
        Vec::new()
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::testable::Interact<M> for ScrollView<M> {
    fn on_event(
        &mut self,
        _event: &InputEvent,
        _ctx: &crate::testable::InteractionContext,
    ) -> crate::testable::InteractionResponse<M> {
        // Container widgets delegate event handling to children
        crate::testable::InteractionResponse::default()
    }
}

// ============================================================================
// WIDGET TRAIT IMPLEMENTATION
// ============================================================================

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for ScrollView<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(
        &mut self,
        layout_context: &mut LayoutContext,
        widget_context: &mut WidgetContext,
    ) -> LayoutNodeId {
        // Layout all children first, collecting their node IDs
        let mut child_nodes: Vec<LayoutNodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(layout_context, widget_context));
        }

        // Create container with flex direction Column
        let layout = Layout {
            flex_direction: Some(FlexDirection::Column),
            ..self.layout.clone()
        };

        layout_context.create_container(&layout, &child_nodes)
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
    }

    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        crate::testable::Paint::paint(self, ctx)
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        widget_context: &mut WidgetContext,
    ) {
        let viewport_layout = match layout_view.get_layout(node) {
            Some(l) => l,
            None => return,
        };

        let viewport_bounds = viewport_layout.bounds;
        let viewport_offset: Point<Logical> = Point::new(
            offset.x + viewport_layout.x(),
            offset.y + viewport_layout.y(),
        );

        // Get scroll state
        let state_key = self.key.clone().unwrap_or_else(|| "__scroll_default__".to_string());
        let scroll_state = widget_context.state_mut()
            .component_storage()
            .get_or_create::<ScrollState>(&state_key);
        let offset_y = scroll_state.offset_y;

        // Calculate content height from children
        let child_ids = layout_view.children(node);
        let content_height: f32 = child_ids.iter()
            .filter_map(|id| layout_view.get_layout(*id))
            .map(|l| l.bounds.origin.y - viewport_bounds.origin.y + l.bounds.size.height)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        // Push clip for viewport - use absolute screen coordinates
        let clip_bounds = Rect::from_xywh(
            viewport_offset.x,
            viewport_offset.y,
            viewport_bounds.size.width,
            viewport_bounds.size.height,
        );
        renderer.push_clip(clip_bounds);

        // Draw children with scroll offset applied
        for (child_widget, child_node_id) in self.children.iter().zip(child_ids.iter()) {
            if let Some(child_layout) = layout_view.get_layout(*child_node_id) {
                // Apply scroll offset to child position
                let child_offset = Point::new(
                    viewport_offset.x,
                    viewport_offset.y - offset_y,
                );

                child_widget.draw(
                    layout_view,
                    *child_node_id,
                    renderer,
                    child_offset,
                    focused_id,
                    cursor_blink,
                    widget_context,
                );
            }
        }

        // Pop clip
        renderer.pop_clip();

        // Draw scrollbar if content exceeds viewport
        if content_height > viewport_bounds.size.height {
            self.draw_scrollbar(renderer, viewport_bounds, offset_y, content_height);
        }
    }

    fn cursor(&self) -> CursorIcon {
        CursorIcon::Default
    }

    fn on_event(
        &mut self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_context: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        let viewport_layout = match layout_view.get_layout(node) {
            Some(l) => l,
            None => return WidgetResponse::default(),
        };

        let viewport_bounds = viewport_layout.bounds;
        let viewport_offset: Point<Logical> = Point::new(
            offset.x + viewport_layout.x(),
            offset.y + viewport_layout.y(),
        );

        // Calculate content height from children
        let child_ids = layout_view.children(node);
        let content_height: f32 = child_ids.iter()
            .filter_map(|id| layout_view.get_layout(*id))
            .map(|l| l.bounds.origin.y - viewport_bounds.origin.y + l.bounds.size.height)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let max_scroll = (content_height - viewport_bounds.size.height).max(0.0);

        // Get or create scroll state
        let state_key = self.key.clone().unwrap_or_else(|| "__scroll_default__".to_string());
        let scroll_state = widget_context.state_mut()
            .component_storage()
            .get_or_create::<ScrollState>(&state_key);

        // Handle scroll wheel
        // Note: macOS natural scrolling - scrolling down (positive delta.y) should move content up (decrease offset)
        if let InputEvent::Scroll { delta } = event {
            if max_scroll > 0.0 {
                scroll_state.offset_y = (scroll_state.offset_y - delta.y).clamp(0.0, max_scroll);
                return WidgetResponse { handled: true, ..Default::default() };
            }
        }

        // Handle drag gesture start
        if let InputEvent::PointerButton {
            position,
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        } = event {
            if viewport_bounds.contains(position) && max_scroll > 0.0 {
                scroll_state.is_dragging = true;
                scroll_state.drag_start_y = position.y;
                scroll_state.drag_start_offset = scroll_state.offset_y;
                return WidgetResponse { handled: true, ..Default::default() };
            }
        }

        // Handle drag gesture move
        if let InputEvent::PointerMoved { position } = event {
            if scroll_state.is_dragging {
                let drag_delta = scroll_state.drag_start_y - position.y;
                scroll_state.offset_y = (scroll_state.drag_start_offset + drag_delta)
                    .clamp(0.0, max_scroll);
                return WidgetResponse { handled: true, ..Default::default() };
            }
        }

        // Handle drag gesture end
        if let InputEvent::PointerButton {
            button: PointerButton::Primary,
            state: ButtonState::Released,
            ..
        } = event {
            scroll_state.is_dragging = false;
        }

        // Handle keyboard navigation
        if let InputEvent::Keyboard { key, state: ButtonState::Pressed, .. } = event {
            let scroll_id = self.key.as_ref().map(|k| WidgetId::from_key(k));
            if focused_id == scroll_id {
                match key {
                    Key::Named(NamedKey::ArrowDown) => {
                        scroll_state.offset_y = (scroll_state.offset_y + 20.0)
                            .clamp(0.0, max_scroll);
                        return WidgetResponse { handled: true, ..Default::default() };
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        scroll_state.offset_y = (scroll_state.offset_y - 20.0)
                            .clamp(0.0, max_scroll);
                        return WidgetResponse { handled: true, ..Default::default() };
                    }
                    Key::Named(NamedKey::PageDown) => {
                        scroll_state.offset_y = (scroll_state.offset_y + viewport_bounds.size.height)
                            .clamp(0.0, max_scroll);
                        return WidgetResponse { handled: true, ..Default::default() };
                    }
                    Key::Named(NamedKey::PageUp) => {
                        scroll_state.offset_y = (scroll_state.offset_y - viewport_bounds.size.height)
                            .clamp(0.0, max_scroll);
                        return WidgetResponse { handled: true, ..Default::default() };
                    }
                    _ => {}
                }
            }
        }

        // Extract scroll offset before propagating to children (to release borrow on widget_context)
        let offset_y = scroll_state.offset_y;

        // Propagate events to children with scroll offset applied
        for (child, child_node_id) in self.children.iter_mut().zip(child_ids.iter()) {
            if let Some(child_layout) = layout_view.get_layout(*child_node_id) {
                let child_top = child_layout.bounds.origin.y - viewport_bounds.origin.y - offset_y;
                let child_bottom = child_top + child_layout.bounds.size.height;

                // Only propagate to visible children
                if child_bottom >= 0.0 && child_top <= viewport_bounds.size.height {
                    let child_offset = Point::new(
                        viewport_offset.x,
                        viewport_offset.y - offset_y,
                    );

                    let response = child.on_event(
                        layout_view,
                        *child_node_id,
                        child_offset,
                        event,
                        focused_id,
                        widget_context,
                    );

                    if response.handled || response.focus_request.is_some() {
                        return response;
                    }
                }
            }
        }

        WidgetResponse::default()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testable::{Identifiable, Layout as LayoutTrait};

    #[test]
    fn test_scroll_view_implements_separated_traits() {
        let scroll: ScrollView<()> = ScrollView::new();

        // Should implement Identifiable
        let _id: Option<WidgetId> = scroll.id();

        // Should implement Layout
        let _constraints = scroll.constraints();
    }

    #[test]
    fn test_scroll_view_with_key() {
        let scroll: ScrollView<()> = ScrollView::new().with_key("test-scroll");

        let id = scroll.id();
        assert!(id.is_some());
        assert_eq!(id.unwrap(), WidgetId::from_key("test-scroll"));
    }

    #[test]
    fn test_scroll_view_layout_constraints() {
        let scroll: ScrollView<()> = ScrollView::new()
            .width(200.0)
            .height(100.0);

        let constraints = scroll.constraints();
        assert!(constraints.is_fixed_width());
        assert!(constraints.is_fixed_height());
        assert_eq!(constraints.min_width, 200.0);
        assert_eq!(constraints.min_height, 100.0);
    }

    #[test]
    fn test_scroll_view_paint_returns_empty_without_layout() {
        use crate::testable::{Paint, PaintContext};
        let scroll: ScrollView<()> = ScrollView::new();
        let mut ctx = PaintContext::default();
        let commands = Paint::paint(&scroll, &mut ctx);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_scroll_state_default() {
        let state = ScrollState::default();
        assert_eq!(state.offset_y, 0.0);
        assert!(!state.is_dragging);
        assert_eq!(state.drag_start_y, 0.0);
        assert_eq!(state.drag_start_offset, 0.0);
    }

    #[test]
    fn test_scroll_view_with_children() {
        let scroll: ScrollView<()> = ScrollView::new()
            .push(crate::widgets::Text::new("Item 1"))
            .push(crate::widgets::Text::new("Item 2"));

        assert_eq!(scroll.children.len(), 2);
    }

    #[test]
    fn test_scroll_view_scrollbar_width() {
        let scroll: ScrollView<()> = ScrollView::new()
            .scrollbar_width(12.0);

        assert_eq!(scroll.scrollbar_width, 12.0);
    }
}
