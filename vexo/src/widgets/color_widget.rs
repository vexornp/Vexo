use crate::core::{Color, Point, Rect, Size, WidgetId};
use crate::input::InputEvent;
use crate::layout::Layout;
use crate::render::RenderCommand;

pub struct ColorWidget {
    pub color: Color,
    pub key: Option<String>,
    pub layout: Layout,
    computed_layout: Option<crate::widget::ComputedLayout>,
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

impl crate::widget::Identifiable for ColorWidget {
    fn id(&self) -> Option<WidgetId> {
        self.key.as_ref().map(|k| WidgetId::from_key(k))
    }
}

impl crate::widget::Layout for ColorWidget {
    fn constraints(&self) -> crate::widget::LayoutConstraints {
        let mut constraints = crate::widget::LayoutConstraints::from_layout(&self.layout);
        // Default to flex_grow: 1.0 if not specified
        if self.layout.flex_grow.is_none() && self.layout.width.is_none() && self.layout.height.is_none() {
            constraints.flex_grow = 1.0;
        }
        constraints
    }

    fn apply_layout(&mut self, layout: crate::widget::ComputedLayout) {
        self.computed_layout = Some(layout);
    }
}

impl crate::widget::Paint for ColorWidget {
    fn paint(&self, ctx: &mut crate::widget::PaintContext) -> Vec<RenderCommand> {
        let layout = match &self.computed_layout {
            Some(l) => l,
            None => return Vec::new(),
        };

        let pos = Point::new(
            ctx.offset().x + layout.x(),
            ctx.offset().y + layout.y(),
        );
        let size = Size::new(layout.width(), layout.height());

        vec![RenderCommand::rect_with_border(
            Rect::new(pos, size),
            self.color,
            Color::WHITE,
            1.0,
        )]
    }
}

impl<M: Clone + std::fmt::Debug + Send> crate::widget::Interact<M> for ColorWidget {
    fn on_event(
        &mut self,
        _event: &InputEvent,
        _ctx: &crate::widget::InteractionContext,
    ) -> crate::widget::InteractionResponse<M> {
        crate::widget::InteractionResponse::default()
    }
}
