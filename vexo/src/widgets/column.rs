use crate::core::{Logical, Point};
use crate::layout::{FlexDirection, Layout, LayoutContext, LayoutNodeId, LayoutView};
use crate::renderer::UiBatcher;
use crate::render::RenderCommand;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::input::InputEvent;

pub struct Column<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
    pub layout: Layout,
    /// Stored computed layout from the layout phase.
    computed_layout: Option<crate::testable::ComputedLayout>,
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testable::{Identifiable, Layout};

    #[test]
    fn test_column_implements_separated_traits() {
        let column: Column<()> = Column::new();

        // Should implement Identifiable
        let _id: Option<WidgetId> = column.id();

        // Should implement Layout
        let _constraints = column.constraints();
    }

    #[test]
    fn test_column_with_key() {
        let column: Column<()> = Column::new().with_key("test-column");

        let id = column.id();
        assert!(id.is_some());
        assert_eq!(id.unwrap(), WidgetId::from_key("test-column"));
    }

    #[test]
    fn test_column_layout_constraints() {
        let column: Column<()> = Column::new()
            .width(200.0)
            .height(100.0);

        let constraints = column.constraints();
        assert!(constraints.is_fixed_width());
        assert!(constraints.is_fixed_height());
        assert_eq!(constraints.min_width, 200.0);
        assert_eq!(constraints.min_height, 100.0);
    }
}

impl<M: Clone + std::fmt::Debug + Send> Column<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
            layout: Layout::default(),
            computed_layout: None,
        }
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the entire Layout struct.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Set uniform padding on all sides.
    pub fn padding(mut self, value: f32) -> Self {
        self.layout = self.layout.padding(value);
        self
    }

    /// Set uniform margin on all sides.
    pub fn margin(mut self, value: f32) -> Self {
        self.layout = self.layout.margin(value);
        self
    }

    /// Set gap between children.
    pub fn gap(mut self, value: f32) -> Self {
        self.layout = self.layout.gap(value);
        self
    }

    /// Enable flex wrapping.
    pub fn flex_wrap(mut self) -> Self {
        self.layout = self.layout.flex_wrap();
        self
    }

    /// Set justify content.
    pub fn justify(mut self, value: crate::layout::JustifyContent) -> Self {
        self.layout = self.layout.justify(value);
        self
    }

    /// Set align items.
    pub fn align(mut self, value: crate::layout::AlignItems) -> Self {
        self.layout = self.layout.align(value);
        self
    }

    /// Set flex grow factor.
    pub fn flex_grow(mut self, value: f32) -> Self {
        self.layout = self.layout.flex_grow(value);
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

    /// Fill available space (sets flex_grow to 1.0).
    pub fn fill(mut self) -> Self {
        self.layout = self.layout.flex_grow(1.0);
        self
    }
}

impl<M: Clone + std::fmt::Debug + Send> Default for Column<M> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SEPARATED TRAIT IMPLEMENTATIONS
// ============================================================================

impl<M: Clone + std::fmt::Debug + Send> crate::testable::Identifiable for Column<M> {
    fn id(&self) -> Option<WidgetId> {
        self.key.as_ref().map(|k| WidgetId::from_key(k))
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::testable::Layout for Column<M> {
    fn constraints(&self) -> crate::testable::LayoutConstraints {
        crate::testable::LayoutConstraints::from_layout(&self.layout)
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::testable::Paint for Column<M> {
    fn paint(&self, _ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        // Container widgets don't paint themselves - children paint
        // The framework will handle child painting
        Vec::new()
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::testable::Interact<M> for Column<M> {
    fn on_event(
        &mut self,
        _event: &InputEvent,
        _ctx: &crate::testable::InteractionContext,
    ) -> crate::testable::InteractionResponse<M> {
        // Container widgets delegate event handling to children
        // The framework will handle child event propagation
        crate::testable::InteractionResponse::default()
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Column<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeId {
        let mut child_nodes: Vec<LayoutNodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(layout_context, widget_context));
        }

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
        if let Some(layout) = layout_view.get_layout(node) {
            let my_offset = Point::<Logical>::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            let child_ids = layout_view.children(node);
            for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
                child_widget.draw(
                    layout_view,
                    child_node_id,
                    renderer,
                    my_offset,
                    focused_id,
                    cursor_blink,
                    widget_context,
                );
            }
        }
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
        if let Some(layout) = layout_view.get_layout(node) {
            let child_ids = layout_view.children(node);
            let my_offset = Point::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            // Handle PointerMoved - find which child contains the pointer and return its cursor
            if let InputEvent::PointerMoved { position } = event {
                for (child, child_node_id) in self.children.iter().zip(child_ids.clone()) {
                    if let Some(child_layout) = layout_view.get_layout(child_node_id) {
                        let child_rect = crate::core::Rect::from_xywh(
                            my_offset.x + child_layout.x(),
                            my_offset.y + child_layout.y(),
                            child_layout.width(),
                            child_layout.height(),
                        );
                        if child_rect.contains(position) {
                            return WidgetResponse {
                                cursor: Some(child.cursor()),
                                ..WidgetResponse::default()
                            };
                        }
                    }
                }
                // Pointer not inside any child - return default cursor
                return WidgetResponse::default();
            }

            // Handle other events
            for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
                let child_response =
                    child.on_event(layout_view, child_node_id, my_offset, event, focused_id, widget_context);

                if child_response.handled || child_response.focus_request.is_some() {
                    return child_response;
                }
            }
        }
        WidgetResponse::default()
    }
}
