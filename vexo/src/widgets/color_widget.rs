use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Color;
use crate::Widget;
use taffy::prelude::{length, NodeId, Size};
use taffy::Style;

pub struct ColorWidget {
    pub width: f32,
    pub height: f32,
    pub color: Color,
    pub key: Option<String>,
}

impl ColorWidget {
    pub fn new(width: f32, height: f32, color: impl Into<Color>) -> Self {
        Self {
            width,
            height,
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
        let node = taffy
            .new_leaf(Style {
                size: Size {
                    width: length(self.width),
                    height: length(self.height),
                },
                ..Default::default()
            })
            .unwrap();

        // record the mapping node -> computed WidgetId for this frame
        ctx.record_node_widget(node);
        node
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        use crate::utils::{Point, Size};

        let layout = taffy.layout(node).unwrap();

        // Calculate absolute position by adding offset to layout location
        let x = offset.x + layout.location.x;
        let y = offset.y + layout.location.y;

        // Pass LOGICAL coordinates - shader handles conversion to physical
        // BUG FIX: Previously this was converting to physical, causing double-scaling
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
