use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::Color;
use crate::input::InputEvent;
use taffy::prelude::{length, NodeId};
use taffy::Style;

pub struct Text {
    pub content: String,
    pub font_size: f32,
    pub color: Color,
    pub key: Option<String>,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            font_size: 24.0,
            color: Color::BLACK,
            key: None,
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
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Text {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        // Calculate intrinsic size based on content and font size
        let width_guess = self.content.len() as f32 * (self.font_size * 0.5);
        let height_guess = self.font_size * 1.2;

        taffy
            .new_leaf(Style {
                size: taffy::Size {
                    width: length(width_guess),
                    height: length(height_guess),
                },
                ..Default::default()
            })
            .unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        _cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
        use crate::utils::Point;

        let layout = taffy.layout(node).unwrap();
        let pos = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        renderer.add_text(self.content.clone(), pos, self.font_size, self.color);
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: crate::utils::Point<crate::utils::Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        WidgetResponse::default()
    }
}
