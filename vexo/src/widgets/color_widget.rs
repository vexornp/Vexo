use crate::core::{Bounds, Color, Point, WidgetId};
use crate::input::InputEvent;
use crate::layout::{Layout, LayoutContext, LayoutNodeId, LayoutView};
use crate::render::RenderCommand;
use crate::renderer::UiBatcher;
use crate::widgets::{Widget, WidgetContext, WidgetResponse};

pub struct ColorWidget {
    pub color: Color,
    pub key: Option<String>,
    pub layout: Layout,
    computed_layout: Option<crate::testable::ComputedLayout>,
}

impl ColorWidget {
    pub fn new(color: impl Into<Color>) -> Self {
        Self {
            color: color.into(),
            key: None,
            layout: Layout::default(),
            computed_layout: None,
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

impl crate::testable::Identifiable for ColorWidget {
    fn id(&self) -> Option<WidgetId> {
        self.key.as_ref().map(|k| WidgetId::from_key(k))
    }
}

impl crate::testable::Layout for ColorWidget {
    fn constraints(&self) -> crate::testable::LayoutConstraints {
        let mut constraints = crate::testable::LayoutConstraints::from_layout(&self.layout);
        // Default to flex_grow: 1.0 if not specified
        if self.layout.flex_grow.is_none() && self.layout.width.is_none() && self.layout.height.is_none() {
            constraints.flex_grow = 1.0;
        }
        constraints
    }

    fn apply_layout(&mut self, layout: crate::testable::ComputedLayout) {
        self.computed_layout = Some(layout);
    }
}

impl crate::testable::Paint for ColorWidget {
    fn paint(&self, ctx: &mut crate::testable::PaintContext) -> Vec<RenderCommand> {
        let layout = match &self.computed_layout {
            Some(l) => l,
            None => return Vec::new(),
        };

        let bounds = Bounds::from_xywh(
            ctx.offset().x + layout.x(),
            ctx.offset().y + layout.y(),
            layout.width(),
            layout.height(),
        );

        vec![RenderCommand::rect_with_border(
            bounds,
            self.color,
            Color::WHITE,
            1.0,
        )]
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::testable::Interact<M> for ColorWidget {
    fn on_event(
        &mut self,
        _event: &InputEvent,
        _ctx: &crate::testable::InteractionContext,
    ) -> crate::testable::InteractionResponse<M> {
        crate::testable::InteractionResponse::default()
    }
}

// Legacy Widget trait implementation
#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for ColorWidget {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, layout_context: &mut LayoutContext, widget_context: &mut WidgetContext) -> LayoutNodeId {
        layout_context.create_leaf(&self.layout)
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
        offset: Point<crate::core::Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        widget_context: &mut WidgetContext,
    ) {
        if let Some(layout) = layout_view.get_layout(node) {
            let bounds = crate::core::Bounds::from_xywh(
                offset.x + layout.x(),
                offset.y + layout.y(),
                layout.width(),
                layout.height(),
            );
            renderer.add_rect(bounds, self.color, Color::WHITE, 1.0, 0.0);
        }
    }

    fn on_event(
        &mut self,
        layout_view: &LayoutView,
        node: LayoutNodeId,
        offset: Point<crate::core::Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        widget_context: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        WidgetResponse::default()
    }
}
