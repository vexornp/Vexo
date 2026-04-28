//! Border modifier widget - draws a colored border around a child.

use std::any::Any;

use crate::core::{Bounds, Color, Logical, Point, Size};
use crate::layout::{Layout, LayoutNodeId};
use crate::render::RenderCommand;
use crate::retain::{
    Element, HitTestContext, Key, LayoutContext, LayoutResult,
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
#[allow(dead_code)]
pub struct BorderRenderObject {
    color: Color,
    width: f32,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeId>,
}

#[allow(dead_code)]
impl BorderRenderObject {
    /// Create a new border render object.
    pub fn new(color: Color, width: f32) -> Self {
        Self {
            color,
            width,
            child: None,
            computed_bounds: None,
            layout_node: None,
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

impl RenderObject for BorderRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext) -> LayoutResult {
        // Border is a pass-through modifier - it uses the child's layout
        // The child will be laid out by the pipeline's recursive traversal
        // We just need to create a placeholder node for ourselves
        let node = ctx.engine().create_leaf(&Layout::default());
        self.layout_node = Some(node);

        LayoutResult {
            node,
            size: Size::new(0.0, 0.0),
        }
    }

    fn apply_layout(&mut self, ctx: &LayoutContext) {
        // Border uses child's bounds
        // Child's apply_layout is called by pipeline traversal
        // We'll get bounds from our layout_node after Taffy computes
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
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

    fn layout_node(&self) -> Option<LayoutNodeId> {
        self.layout_node
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
    fn test_border_render_object_children() {
        let mut ro = BorderRenderObject::new(Color::BLACK, 2.0);

        // Initially, no children
        assert_eq!(ro.children().len(), 0);

        // Set a child
        let child_id = RenderObjectId::new();
        ro.set_child_id(child_id);

        // Now children() should return the child
        let children = ro.children();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0], child_id);
    }

    #[test]
    fn test_border_render_object_set_child() {
        let mut ro = BorderRenderObject::new(Color::BLACK, 2.0);

        // Use the set_child method
        let child_id = RenderObjectId::new();
        ro.set_child(child_id);

        // Verify the child is set
        assert!(ro.child.is_some());
        assert_eq!(ro.child, Some(child_id));
    }

    #[test]
    fn test_border_paint_without_layout() {
        let ro = BorderRenderObject::new(Color::BLACK, 2.0);

        // Without layout, paint should return empty commands
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        assert_eq!(cmds.len(), 0);
    }
}
