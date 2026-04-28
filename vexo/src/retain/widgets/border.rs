//! Border modifier widget - draws a colored border around a child.

use std::any::Any;

use crate::core::{Bounds, Color, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{
    Element, HitTestContext, Key, LayoutContext,
    PaintContext, RenderObject, RenderObjectId, Widget,
};

/// Border modifier - draws a colored border around a child widget.
pub struct Border {
    key: Option<Key>,
    child: Box<dyn Widget>,
    color: Color,
    width: f32,
}

impl Border {
    /// Create a new border modifier.
    pub fn new(child: Box<dyn Widget>, color: Color, width: f32) -> Self {
        Self {
            key: None,
            child,
            color,
            width,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }

    /// Get the border color.
    pub fn color(&self) -> Color {
        self.color
    }

    /// Get the border width.
    pub fn width(&self) -> f32 {
        self.width
    }
}

impl Widget for Border {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ModifierElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(BorderRenderObject::new(self.color, self.width))
    }

    fn clone_box(&self) -> Box<dyn Widget> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            color: self.color,
            width: self.width,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }
}

/// RenderObject for Border - draws a colored border.
pub struct BorderRenderObject {
    color: Color,
    width: f32,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
}

impl BorderRenderObject {
    /// Create a new border render object.
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            color,
            width,
            child: None,
            computed_bounds: None,
        }
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for BorderRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // Border takes the available space
        let size = Size::new(constraints.max_width, constraints.max_height);
        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        match &self.computed_bounds {
            Some(bounds) => vec![RenderCommand::rect_with_border(
                *bounds,
                Color::TRANSPARENT,
                self.color,
                self.width,
            )],
            None => vec![],
        }
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectId] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectId) {
        self.child = Some(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::Text;

    #[test]
    fn test_border_widget_creation() {
        let child = Box::new(Text::new("Hello"));
        let border = Border::new(child, Color::BLACK, 2.0);

        assert!(border.key().is_none());
    }

    #[test]
    fn test_border_widget_with_key() {
        let child = Box::new(Text::new("Hello"));
        let border = Border::new(child, Color::BLACK, 2.0)
            .with_key("my-border");

        assert_eq!(border.key(), Some(Key::new("my-border")));
    }

    #[test]
    fn test_border_creates_render_object() {
        let child = Box::new(Text::new("Hello"));
        let border = Border::new(child, Color::BLACK, 2.0);

        let mut ro = border.create_render_object();

        // Must layout first to set computed_bounds
        let constraints = LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 100.0,
            max_height: 50.0,
            ..LayoutConstraints::default()
        };
        let mut layout_ctx = LayoutContext::mock();
        ro.layout(constraints, &mut layout_ctx);

        // Should be able to paint
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Border should return a rect_with_border command
        assert_eq!(cmds.len(), 1);
    }
}
