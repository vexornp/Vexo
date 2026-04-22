use crate::core::{Color, Logical, Point};
use crate::layout::{Layout, LayoutContext, LayoutNodeId, LayoutView, MeasureContext, TextMeasureContext};
use crate::renderer::UiBatcher;
use crate::render::RenderCommand;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::input::InputEvent;

pub struct Text {
    pub content: String,
    pub font_size: f32,
    pub color: Color,
    pub key: Option<String>,
    pub layout: Layout,
    pub line_height: f32,
    /// Stored computed layout from the layout phase.
    computed_layout: Option<crate::widget::ComputedLayout>,
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{Identifiable, Layout};

    #[test]
    fn test_text_implements_separated_traits() {
        let text = Text::new("Hello");

        // Should implement Identifiable
        let _id: Option<WidgetId> = text.id();

        // Should implement Layout
        let _constraints = text.constraints();
    }
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
            computed_layout: None,
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

// ============================================================================
// SEPARATED TRAIT IMPLEMENTATIONS
// ============================================================================

impl crate::widget::Identifiable for Text {
    fn id(&self) -> Option<WidgetId> {
        self.key.as_ref().map(|k| WidgetId::from_key(k))
    }
}

impl crate::widget::Layout for Text {
    fn constraints(&self) -> crate::widget::LayoutConstraints {
        crate::widget::LayoutConstraints::from_layout(&self.layout)
    }

    fn apply_layout(&mut self, layout: crate::widget::ComputedLayout) {
        self.computed_layout = Some(layout);
    }
}

impl crate::widget::Paint for Text {
    fn paint(&self, ctx: &mut crate::widget::PaintContext) -> Vec<RenderCommand> {
        let layout = match &self.computed_layout {
            Some(l) => l,
            None => return Vec::new(),
        };

        let pos = Point::new(
            ctx.offset().x + layout.x(),
            ctx.offset().y + layout.y(),
        );

        vec![RenderCommand::text(
            self.content.clone(),
            pos,
            self.font_size,
            self.color,
        )]
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::widget::Interact<M> for Text {
    fn on_event(
        &mut self,
        _event: &InputEvent,
        _ctx: &crate::widget::InteractionContext,
    ) -> crate::widget::InteractionResponse<M> {
        crate::widget::InteractionResponse::default()
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

    fn apply_layout(&mut self, layout: crate::widget::ComputedLayout) {
        self.computed_layout = Some(layout);
    }

    fn paint(&self, ctx: &mut crate::widget::PaintContext) -> Vec<RenderCommand> {
        crate::widget::Paint::paint(self, ctx)
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

            // Convert core::Color to crate::Color for legacy renderer
            let color = crate::Color::new(self.color.r, self.color.g, self.color.b, self.color.a);
            renderer.add_text(self.content.clone(), pos, self.font_size, color);
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
