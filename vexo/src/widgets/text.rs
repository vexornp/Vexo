use crate::core::{Logical, Point};
use crate::layout::{Layout, LayoutContext, LayoutNodeId, LayoutView, MeasureContext, TextMeasureContext};
use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::Color;
use crate::input::InputEvent;

pub struct Text {
    pub content: String,
    pub font_size: f32,
    pub color: Color,
    pub key: Option<String>,
    pub layout: Layout,
    pub line_height: f32,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            font_size: 24.0,
            color: Color::BLACK,
            key: None,
            layout: Layout::default(),
            line_height: 1.2,
        }
    }

    /// Set the font size.
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
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

    /// Set custom line height multiplier.
    ///
    /// Default is 1.2. A value of 1.5 gives 50% extra spacing between lines.
    pub fn line_height(mut self, multiplier: f32) -> Self {
        self.line_height = multiplier;
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Text {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeId {
        // Create measurement context for accurate text sizing
        let measure_context = MeasureContext::Text(TextMeasureContext {
            content: self.content.clone(),
            font_size: self.font_size,
            line_height: self.line_height,
        });

        // Create node with context - Taffy will call measure during compute
        layout_context.create_leaf_with_context(&self.layout, measure_context)
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
            let pos = Point::new(
                offset.x + layout.x(),
                offset.y + layout.y(),
            );

            renderer.add_text(self.content.clone(), pos, self.font_size, self.color);
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
