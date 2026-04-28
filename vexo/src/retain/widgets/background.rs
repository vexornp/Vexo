//! Background modifier widget - draws a colored background behind a child.

use std::any::Any;

use crate::core::{Bounds, Color, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{
    Element, HitTestContext, Key, LayoutContext,
    PaintContext, RenderObject, RenderObjectId, Widget,
};

/// Background modifier - draws a colored rectangle behind a child widget.
pub struct Background {
    key: Option<Key>,
    child: Box<dyn Widget>,
    color: Color,
}

impl Background {
    /// Create a new background modifier.
    pub fn new(child: Box<dyn Widget>, color: Color) -> Self {
        Self {
            key: None,
            child,
            color,
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

    /// Get the background color.
    pub fn color(&self) -> Color {
        self.color
    }
}

impl Widget for Background {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ModifierElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(BackgroundRenderObject::new(self.color))
    }

    fn clone_box(&self) -> Box<dyn Widget> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            color: self.color,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }
}

/// RenderObject for Background - draws a colored rect.
#[allow(dead_code)]
pub struct BackgroundRenderObject {
    color: Color,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
}

#[allow(dead_code)]
impl BackgroundRenderObject {
    /// Create a new background render object.
    pub fn new(color: Color) -> Self {
        Self {
            color,
            child: None,
            computed_bounds: None,
        }
    }

    /// Set the child render object.
    pub fn set_child(&mut self, child: RenderObjectId) {
        self.child = Some(child);
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for BackgroundRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // Background takes the available space
        let size = Size::new(constraints.max_width, constraints.max_height);
        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        match &self.computed_bounds {
            Some(bounds) => vec![RenderCommand::rect(*bounds, self.color)],
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
        // Return empty for now - child is managed separately
        &[]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retain::Text;

    #[test]
    fn test_background_widget_creation() {
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);

        assert!(bg.key().is_none());
    }

    #[test]
    fn test_background_widget_with_key() {
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED)
            .with_key("my-bg");

        assert_eq!(bg.key(), Some(Key::new("my-bg")));
    }

    #[test]
    fn test_background_creates_render_object() {
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);

        let mut ro = bg.create_render_object();

        // Must layout first to set computed_bounds
        let constraints = LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 100.0,
            max_height: 50.0,
            ..LayoutConstraints::default()
        };
        let mut layout_ctx = LayoutContext::mock();
        let size = ro.layout(constraints, &mut layout_ctx);

        assert_eq!(size.width, 100.0);
        assert_eq!(size.height, 50.0);

        // Should be able to paint
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // Background should return a rect command
        assert_eq!(cmds.len(), 1);
    }
}
