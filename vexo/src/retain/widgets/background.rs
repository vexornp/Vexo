//! Background modifier widget - draws a colored background behind a child.

use std::any::Any;

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position, Size};
use crate::layout::{Layout, LayoutNodeId};
use crate::render::RenderCommand;
use crate::retain::{
    Element, HitTestContext, Key, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectId, Widget,
};

/// Background modifier - draws a colored rectangle behind a child widget.
pub struct Background {
    key: Option<Key>,
    child: Box<dyn Widget<()>>,
    color: Color,
}

impl Background {
    /// Create a new background modifier.
    pub fn new(child: Box<dyn Widget<()>>, color: Color) -> Self {
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
    pub fn child(&self) -> &dyn Widget<()> {
        self.child.as_ref()
    }

    /// Get the background color.
    pub fn color(&self) -> Color {
        self.color
    }
}

impl Widget<()> for Background {
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

    fn clone_box(&self) -> Box<dyn Widget<()>> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            color: self.color,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget<()>> {
        Some(self.child.as_ref())
    }
}

/// RenderObject for Background - draws a colored rect.
#[allow(dead_code)]
pub struct BackgroundRenderObject {
    color: Color,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeId>,
}

#[allow(dead_code)]
impl BackgroundRenderObject {
    /// Create a new background render object.
    pub fn new(color: Color) -> Self {
        Self {
            color,
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

impl RenderObject for BackgroundRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeId]) -> LayoutResult {
        // Background is a pass-through modifier - it uses the child's layout node
        match child_nodes.first() {
            Some(child_node) => {
                // Pass through child's node
                self.layout_node = Some(*child_node);
                LayoutResult {
                    node: *child_node,
                    size: Size::new(0.0, 0.0),
                }
            }
            None => {
                // No child, create empty leaf
                let node = ctx.engine().create_leaf(&Layout::default());
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::new(0.0, 0.0),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &LayoutContext) {
        // Background uses child's bounds
        // Child's apply_layout is called by pipeline traversal
        // We'll get bounds from our layout_node after Taffy computes
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        match &self.computed_bounds {
            Some(bounds) => {
                // Get the absolute position where this background should be painted.
                // The context already calculated the absolute position from the
                // parent chain, so we just use it directly.
                let pos: Position<Logical, Absolute> = ctx.absolute_position();

                // Create absolute bounds at the correct position with our size
                let absolute_bounds = Bounds::new(
                    pos.x,
                    pos.y,
                    pos.x + bounds.width(),
                    pos.y + bounds.height(),
                );

                vec![RenderCommand::rect(absolute_bounds, self.color)]
            }
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

    fn computed_bounds(&self) -> Option<crate::core::Bounds<crate::core::Logical>> {
        self.computed_bounds
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
    fn test_background_render_object_children() {
        let mut ro = BackgroundRenderObject::new(Color::RED);

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
    fn test_background_render_object_set_child() {
        let mut ro = BackgroundRenderObject::new(Color::BLUE);

        // Use the set_child method
        let child_id = RenderObjectId::new();
        ro.set_child(child_id);

        // Verify the child is set
        assert!(ro.child.is_some());
        assert_eq!(ro.child, Some(child_id));
    }

    #[test]
    fn test_background_paint_without_layout() {
        let ro = BackgroundRenderObject::new(Color::RED);

        // Without layout, paint should return empty commands
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        assert_eq!(cmds.len(), 0);
    }
}
