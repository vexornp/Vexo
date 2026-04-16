use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Color;
use crate::Widget;
use taffy::prelude::NodeId;
use taffy::Style;

pub struct ColorWidget {
    pub color: Color,
    pub key: Option<String>,
}

impl ColorWidget {
    pub fn new(color: impl Into<Color>) -> Self {
        Self {
            color: color.into(),
            key: None,
        }
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for ColorWidget {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        // ColorWidget has no intrinsic size - use flex_grow to fill available space
        taffy
            .new_leaf(Style {
                flex_grow: 1.0,
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
        use crate::utils::{Point, Size};

        let layout = taffy.layout(node).unwrap();

        let x = offset.x + layout.location.x;
        let y = offset.y + layout.location.y;

        let pos = Point::<crate::utils::Logical>::new(x, y);
        let size = Size::<crate::utils::Logical>::new(layout.size.width, layout.size.height);

        let border_color = crate::Color::WHITE;
        let border_width = 1.0;

        renderer.add_rect(pos.to_array(), size.to_array(), self.color, border_color, border_width, 0.0);
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: crate::utils::Point<crate::utils::Logical>,
        event: &winit::event::WindowEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        WidgetResponse::default()
    }
}
