use crate::core::{Logical, Point};
use crate::layout::Layout;
use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::Color;
use crate::input::InputEvent;
use taffy::prelude::{length, NodeId};

pub struct Text {
    pub content: String,
    pub font_size: f32,
    pub color: Color,
    pub key: Option<String>,
    pub layout: Layout,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            font_size: 24.0,
            color: Color::BLACK,
            key: None,
            layout: Layout::default(),
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
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Text {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        // Calculate intrinsic size based on content and font size
        let intrinsic_width = self.content.len() as f32 * (self.font_size * 0.5);
        let intrinsic_height = self.font_size * 1.2;

        // Use Layout properties, falling back to intrinsic size for auto dimensions
        let style = self.layout.clone().to_taffy_style();

        // If width/height are auto, use intrinsic size as the base
        let style = if self.layout.width.is_none() || self.layout.height.is_none() {
            taffy::Style {
                size: taffy::Size {
                    width: self.layout.width.map(|d| d.to_taffy()).unwrap_or_else(|| length(intrinsic_width)),
                    height: self.layout.height.map(|d| d.to_taffy()).unwrap_or_else(|| length(intrinsic_height)),
                },
                ..style
            }
        } else {
            style
        };

        taffy.new_leaf(style).unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: Point<Logical>,
        focused_id: Option<WidgetId>,
        _cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
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
        offset: Point<Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        WidgetResponse::default()
    }
}
