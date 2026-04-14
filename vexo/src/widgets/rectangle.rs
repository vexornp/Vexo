use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Color;
use crate::Widget;
use taffy::prelude::{length, NodeId, Size};
use taffy::Style;

pub struct Rectangle {
    pub width: f32,
    pub height: f32,
    pub color: Color,
    pub key: Option<String>,
}

impl Rectangle {
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
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Rectangle {
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
        offset: (f32, f32),
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let scale = ctx.scale.factor();

        // Calculate absolute position by adding offset to layout location
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;

        // Convert logical coordinates to physical coordinates
        let physical_x = x * scale;
        let physical_y = y * scale;

        // Convert logical size to physical size
        let physical_width = layout.size.width * scale;
        let physical_height = layout.size.height * scale;

        // Prepare instance data for the rectangle
        let pos = [physical_x, physical_y];
        let size = [physical_width, physical_height];

        let color = self.color.to_array();

        // Set default border color and width (can be customized later)
        let border_color = [1.0, 1.0, 1.0, 1.0]; // black border
        let border_width = 1.0; // no border by default

        renderer.add_rect(pos, size, color, border_color, border_width);
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: (f32, f32),
        event: &winit::event::WindowEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        WidgetResponse::default()
    }
}
