//! CornerRadius modifier widget - applies rounded corners to a child.

use std::any::Any;

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{
    Element, HitTestContext, Key, LayoutContext,
    PaintContext, RenderObject, RenderObjectId, Widget,
};

/// CornerRadius modifier - applies rounded corners to a child widget.
pub struct CornerRadius {
    key: Option<Key>,
    child: Box<dyn Widget>,
    radius: f32,
}

impl CornerRadius {
    /// Create a new corner radius modifier.
    pub fn new(child: Box<dyn Widget>, radius: f32) -> Self {
        Self {
            key: None,
            child,
            radius,
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

    /// Get the corner radius.
    pub fn radius(&self) -> f32 {
        self.radius
    }
}

impl Widget for CornerRadius {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::ModifierElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(CornerRadiusRenderObject::new(self.radius))
    }

    fn clone_box(&self) -> Box<dyn Widget> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            radius: self.radius,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }
}

/// RenderObject for CornerRadius - applies rounded corners.
pub struct CornerRadiusRenderObject {
    radius: f32,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
}

impl CornerRadiusRenderObject {
    /// Create a new corner radius render object.
    pub fn new(radius: f32) -> Self {
        Self {
            radius,
            child: None,
            computed_bounds: None,
        }
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for CornerRadiusRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // CornerRadius takes the available space
        let size = Size::new(constraints.max_width, constraints.max_height);
        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Return push/pop commands for corner radius
        vec![
            RenderCommand::PushCornerRadius { radius: self.radius },
            RenderCommand::PopCornerRadius,
        ]
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
    fn test_corner_radius_widget_creation() {
        let child = Box::new(Text::new("Hello"));
        let cr = CornerRadius::new(child, 10.0);

        assert!(cr.key().is_none());
    }

    #[test]
    fn test_corner_radius_widget_with_key() {
        let child = Box::new(Text::new("Hello"));
        let cr = CornerRadius::new(child, 10.0)
            .with_key("my-corners");

        assert_eq!(cr.key(), Some(Key::new("my-corners")));
    }

    #[test]
    fn test_corner_radius_creates_render_object() {
        let child = Box::new(Text::new("Hello"));
        let cr = CornerRadius::new(child, 10.0);

        let mut ro = cr.create_render_object();

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

        // CornerRadius should return push/pop commands
        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[0], RenderCommand::PushCornerRadius { .. }));
        assert!(matches!(cmds[1], RenderCommand::PopCornerRadius));
    }
}
