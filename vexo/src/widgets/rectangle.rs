use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use taffy::prelude::{length, NodeId, Size};
use taffy::Style;

pub struct Rectangle {
    pub width: f32,
    pub height: f32,
    pub color: [f32; 3],
    pub key: Option<String>,
}

impl Rectangle {
    pub fn new(width: f32, height: f32, color: [f32; 3]) -> Self {
        Self {
            width,
            height,
            color,
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
        let x = offset.0 + layout.location.x;
        let y = offset.1 + layout.location.y;
        renderer.add_rect(x, y, layout.size.width, layout.size.height, self.color);
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
