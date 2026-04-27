//! ContainerRenderObject implementation for Column and Row.

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::LayoutConstraints;
use crate::render::RenderCommand;
use crate::retain::{HitTestContext, LayoutContext, PaintContext, RenderObject, RenderObjectId};

/// RenderObject for container widgets (Column, Row).
///
/// Container render objects hold references to child render objects
/// and participate in layout but do not paint anything themselves.
///
/// # Example
///
/// ```ignore
/// use vexo::retain::render_objects::ContainerRenderObject;
///
/// let mut container = ContainerRenderObject::new_column();
/// container.add_child(child_id);
/// ```
pub struct ContainerRenderObject {
    children: Vec<RenderObjectId>,
    is_row: bool,
    computed_bounds: Option<Bounds<Logical>>,
}

impl ContainerRenderObject {
    /// Create a new column container.
    pub fn new_column() -> Self {
        Self {
            children: Vec::new(),
            is_row: false,
            computed_bounds: None,
        }
    }

    /// Create a new row container.
    pub fn new_row() -> Self {
        Self {
            children: Vec::new(),
            is_row: true,
            computed_bounds: None,
        }
    }

    /// Add a child render object.
    pub fn add_child(&mut self, child: RenderObjectId) {
        self.children.push(child);
    }

    /// Set children directly.
    pub fn set_children(&mut self, children: Vec<RenderObjectId>) {
        self.children = children;
    }

    /// Check if this is a row layout.
    pub fn is_row(&self) -> bool {
        self.is_row
    }

    /// Get the computed bounds.
    pub fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    /// Clear all children.
    pub fn clear_children(&mut self) {
        self.children.clear();
    }

    /// Get the number of children.
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

impl RenderObject for ContainerRenderObject {
    fn layout(&mut self, constraints: LayoutConstraints, _ctx: &mut LayoutContext) -> Size<Logical> {
        // Container layout is delegated to Taffy
        // This just returns the constrained size
        let size = Size::new(constraints.max_width, constraints.max_height);
        self.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, size.width, size.height));
        size
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        // Containers don't paint themselves, children do
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectId] {
        &self.children
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_render_object_column() {
        let obj = ContainerRenderObject::new_column();
        assert!(!obj.is_row());
        assert_eq!(obj.children().len(), 0);
        assert_eq!(obj.child_count(), 0);
        assert!(obj.computed_bounds().is_none());
    }

    #[test]
    fn test_container_render_object_row() {
        let obj = ContainerRenderObject::new_row();
        assert!(obj.is_row());
        assert_eq!(obj.children().len(), 0);
        assert_eq!(obj.child_count(), 0);
    }

    #[test]
    fn test_container_add_child() {
        let mut obj = ContainerRenderObject::new_column();
        let child_id = RenderObjectId::new();
        obj.add_child(child_id);

        assert_eq!(obj.children().len(), 1);
        assert_eq!(obj.child_count(), 1);
        assert_eq!(obj.children()[0], child_id);
    }

    #[test]
    fn test_container_set_children() {
        let mut obj = ContainerRenderObject::new_column();
        let child1 = RenderObjectId::new();
        let child2 = RenderObjectId::new();

        obj.set_children(vec![child1, child2]);

        assert_eq!(obj.children().len(), 2);
        assert_eq!(obj.children()[0], child1);
        assert_eq!(obj.children()[1], child2);
    }

    #[test]
    fn test_container_clear_children() {
        let mut obj = ContainerRenderObject::new_column();
        obj.add_child(RenderObjectId::new());
        obj.add_child(RenderObjectId::new());
        assert_eq!(obj.child_count(), 2);

        obj.clear_children();

        assert_eq!(obj.child_count(), 0);
    }

    #[test]
    fn test_container_layout() {
        let mut obj = ContainerRenderObject::new_column();
        let constraints = LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 200.0,
            max_height: 100.0,
            ..LayoutConstraints::default()
        };
        let mut ctx = LayoutContext::mock();

        let size = obj.layout(constraints, &mut ctx);

        assert_eq!(size.width, 200.0);
        assert_eq!(size.height, 100.0);
        assert!(obj.computed_bounds().is_some());

        let bounds = obj.computed_bounds().unwrap();
        assert_eq!(bounds.width(), 200.0);
        assert_eq!(bounds.height(), 100.0);
    }

    #[test]
    fn test_container_hit_test_inside() {
        let mut obj = ContainerRenderObject::new_column();
        let constraints = LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 200.0,
            max_height: 100.0,
            ..LayoutConstraints::default()
        };
        let mut ctx = LayoutContext::mock();

        obj.layout(constraints, &mut ctx);

        // Should hit inside bounds
        assert!(obj.hit_test(Point::new(100.0, 50.0), &HitTestContext::mock()));
        assert!(obj.hit_test(Point::new(0.0, 0.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_container_hit_test_outside() {
        let mut obj = ContainerRenderObject::new_column();
        let constraints = LayoutConstraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 200.0,
            max_height: 100.0,
            ..LayoutConstraints::default()
        };
        let mut ctx = LayoutContext::mock();

        obj.layout(constraints, &mut ctx);

        // Should miss outside bounds
        assert!(!obj.hit_test(Point::new(300.0, 50.0), &HitTestContext::mock()));
        assert!(!obj.hit_test(Point::new(100.0, 200.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_container_hit_test_no_layout() {
        let obj = ContainerRenderObject::new_column();

        // Without layout, computed_bounds is None, so hit test should fail
        assert!(!obj.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_container_paint() {
        let obj = ContainerRenderObject::new_column();

        // Paint returns empty (containers don't paint)
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert!(result.is_empty());
    }

    #[test]
    fn test_container_children_trait() {
        let mut obj = ContainerRenderObject::new_row();
        let child1 = RenderObjectId::new();
        let child2 = RenderObjectId::new();

        obj.add_child(child1);
        obj.add_child(child2);

        // Test the RenderObject::children() trait method
        let children = RenderObject::children(&obj);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], child1);
        assert_eq!(children[1], child2);
    }
}
