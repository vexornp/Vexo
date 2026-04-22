use crate::core::{Logical, Point, Size};
use crate::layout::{Layout, LayoutContext, LayoutNodeId, LayoutView};
use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Color;
use crate::Widget;
use crate::input::InputEvent;

pub struct ColorWidget {
    pub color: Color,
    pub key: Option<String>,
    pub layout: Layout,
}

impl ColorWidget {
    pub fn new(color: impl Into<Color>) -> Self {
        Self {
            color: color.into(),
            key: None,
            layout: Layout::default(),
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
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

    /// Set flex grow factor.
    pub fn flex_grow(mut self, value: f32) -> Self {
        self.layout = self.layout.flex_grow(value);
        self
    }

    /// Set the entire Layout struct.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for ColorWidget {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeId {
        // Use Layout properties, defaulting to flex_grow: 1.0 if not specified
        let layout = if self.layout.flex_grow.is_none() && self.layout.width.is_none() && self.layout.height.is_none() {
            Layout::default().flex_grow(1.0)
        } else {
            self.layout.clone()
        };

        layout_context.create_leaf(&layout)
    }

    fn draw(
        &self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        _cursor_blink: &crate::CursorBlinkState,
        widget_context: &mut WidgetContext,
    ) {
        if let Some(layout) = layout_view.get_layout(node) {
            let x = offset.x + layout.x();
            let y = offset.y + layout.y();

            let pos = Point::<Logical>::new(x, y);
            let size = Size::<Logical>::new(layout.width(), layout.height());

            let border_color = crate::Color::WHITE;
            let border_width = 1.0;

            renderer.add_rect(pos.to_array(), size.to_array(), self.color, border_color, border_width, 0.0);
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
        WidgetResponse::default()
    }
}
