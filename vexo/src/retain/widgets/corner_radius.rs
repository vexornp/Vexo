//! CornerRadius modifier widget - applies rounded corners to a child.

use std::any::Any;

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{Layout, LayoutNodeId};
use crate::render::RenderCommand;
use crate::retain::{
    Element, HitTestContext, Key, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectId, Widget,
};

/// CornerRadius modifier - applies rounded corners to a child widget.
pub struct CornerRadius {
    key: Option<Key>,
    child: Box<dyn Widget<()>>,
    radius: f32,
}

impl CornerRadius {
    /// Create a new corner radius modifier.
    pub fn new(child: Box<dyn Widget<()>>, radius: f32) -> Self {
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
    pub fn child(&self) -> &dyn Widget<()> {
        self.child.as_ref()
    }

    /// Get the corner radius.
    pub fn radius(&self) -> f32 {
        self.radius
    }
}

impl Widget<()> for CornerRadius {
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

    fn clone_box(&self) -> Box<dyn Widget<()>> {
        Box::new(Self {
            key: self.key.clone(),
            child: self.child.clone_box(),
            radius: self.radius,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget<()>> {
        Some(self.child.as_ref())
    }
}

/// RenderObject for CornerRadius - applies rounded corners.
#[allow(dead_code)]
pub struct CornerRadiusRenderObject {
    radius: f32,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeId>,
}

#[allow(dead_code)]
impl CornerRadiusRenderObject {
    /// Create a new corner radius render object.
    pub fn new(radius: f32) -> Self {
        Self {
            radius,
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

impl RenderObject for CornerRadiusRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeId]) -> LayoutResult {
        // CornerRadius is a pass-through modifier - it uses the child's layout node
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
        // CornerRadius uses child's bounds
        // Child's apply_layout is called by pipeline traversal
        // We'll get bounds from our layout_node after Taffy computes
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
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
    fn test_corner_radius_render_object_children() {
        let mut ro = CornerRadiusRenderObject::new(10.0);

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
    fn test_corner_radius_paint_without_layout() {
        let ro = CornerRadiusRenderObject::new(10.0);

        // Without layout, paint still returns push/pop commands
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);

        // CornerRadius should return push/pop commands
        assert_eq!(cmds.len(), 2);
        assert!(matches!(cmds[0], RenderCommand::PushCornerRadius { .. }));
        assert!(matches!(cmds[1], RenderCommand::PopCornerRadius));
    }
}
