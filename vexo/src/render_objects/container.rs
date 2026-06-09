//! ContainerRenderObject implementation for Column and Row.

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{Layout, LayoutNodeKey};
use crate::render::RenderCommand;
use crate::{HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey};

/// RenderObject for container widgets (Column, Row).
///
/// Container render objects hold references to child render objects
/// and participate in layout but do not paint themselves.
///
/// # Example
///
/// ```ignore
/// use vexo::render_objects::ContainerRenderObject;
/// use vexo::layout::{Layout, FlexDirection, AlignItems};
///
/// let layout = Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch);
/// let mut container = ContainerRenderObject::new(layout);
/// container.add_child(child_id);
/// ```
pub struct ContainerRenderObject {
    children: Vec<RenderObjectKey>,
    layout: Layout,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl ContainerRenderObject {
    /// Create a new container with the given layout.
    pub fn new(layout: Layout) -> Self {
        Self {
            children: Vec::new(),
            layout,
            computed_bounds: None,
            layout_node: None,
        }
    }

    /// Add a child render object.
    pub fn add_child(&mut self, child: RenderObjectKey) {
        self.children.push(child);
    }

    /// Set children directly.
    pub fn set_children(&mut self, children: Vec<RenderObjectKey>) {
        self.children = children;
    }

    /// Set the layout, returning true if it changed.
    pub fn set_layout(&mut self, layout: Layout) -> bool {
        if self.layout != layout {
            self.layout = layout;
            true
        } else {
            false
        }
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
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        match self.layout_node {
            Some(existing) => {
                // Incremental: update existing node
                ctx.engine().set_style(existing, &self.layout);
                ctx.engine().set_children(existing, child_nodes);
                LayoutResult {
                    node: existing,
                    size: Size::new(0.0, 0.0),
                }
            }
            None => {
                // First frame: create new node
                let node = ctx.engine().create_container(&self.layout, child_nodes);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::new(0.0, 0.0),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        // Container reads computed bounds from Taffy
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
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

    fn children(&self) -> &[RenderObjectKey] {
        &self.children
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn add_child(&mut self, child: RenderObjectKey) {
        self.children.push(child);
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine, FlexDirection, AlignItems};

    fn column_layout() -> Layout {
        Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch)
    }

    fn row_layout() -> Layout {
        Layout::default().flex_direction(FlexDirection::Row).align(AlignItems::Stretch)
    }

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_container_render_object_column() {
        let obj = ContainerRenderObject::new(column_layout());
        assert_eq!(obj.children().len(), 0);
        assert_eq!(obj.child_count(), 0);
        assert!(obj.computed_bounds().is_none());
    }

    #[test]
    fn test_container_render_object_row() {
        let obj = ContainerRenderObject::new(row_layout());
        assert_eq!(obj.children().len(), 0);
        assert_eq!(obj.child_count(), 0);
    }

    #[test]
    fn test_container_add_child() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let child_id = sm.insert(());
        obj.add_child(child_id);

        assert_eq!(obj.children().len(), 1);
        assert_eq!(obj.child_count(), 1);
        assert_eq!(obj.children()[0], child_id);
    }

    #[test]
    fn test_container_set_children() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let child1 = sm.insert(());
        let child2 = sm.insert(());

        obj.set_children(vec![child1, child2]);

        assert_eq!(obj.children().len(), 2);
        assert_eq!(obj.children()[0], child1);
        assert_eq!(obj.children()[1], child2);
    }

    #[test]
    fn test_container_clear_children() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        obj.add_child(sm.insert(()));
        obj.add_child(sm.insert(()));
        assert_eq!(obj.child_count(), 2);

        obj.clear_children();

        assert_eq!(obj.child_count(), 0);
    }

    #[test]
    fn test_container_layout_creates_node() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = obj.layout(&mut ctx, &[]);

        // Should have created a layout node
        assert!(obj.layout_node.is_some());
        assert_eq!(obj.layout_node, Some(result.node));
    }

    #[test]
    fn test_container_apply_layout() {
        let mut obj = ContainerRenderObject::new(column_layout());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create node
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            let _result = obj.layout(&mut ctx, &[]);
        }

        // Compute layout
        let root = engine.create_leaf(&Layout::default());
        engine.compute(root, Size::new(200.0, 100.0), &mut font_system);

        // Apply layout should read computed bounds
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            obj.apply_layout(&mut ctx);
        }

        // After apply_layout, computed_bounds should be set (though may be zero
        // since the node isn't part of the computed tree properly)
        // The key thing is it doesn't crash
    }

    #[test]
    fn test_container_hit_test_no_layout() {
        let obj = ContainerRenderObject::new(column_layout());

        // Without layout, computed_bounds is None, so hit test should fail
        assert!(!obj.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()));
    }

    #[test]
    fn test_container_paint() {
        let obj = ContainerRenderObject::new(column_layout());

        // Paint returns empty (containers don't paint)
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = obj.paint(&mut ctx);

        assert!(result.is_empty());
    }

    #[test]
    fn test_container_children_trait() {
        let mut obj = ContainerRenderObject::new(row_layout());
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let child1 = sm.insert(());
        let child2 = sm.insert(());

        obj.add_child(child1);
        obj.add_child(child2);

        // Test the RenderObject::children() trait method
        let children = RenderObject::children(&obj);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], child1);
        assert_eq!(children[1], child2);
    }

    #[test]
    fn test_container_set_layout_change_detection() {
        let mut obj = ContainerRenderObject::new(column_layout());

        // Setting the same layout should return false (no change)
        assert!(!obj.set_layout(column_layout()));

        // Setting a different layout should return true (changed)
        assert!(obj.set_layout(row_layout()));

        // Setting the same new layout again should return false
        assert!(!obj.set_layout(row_layout()));
    }
}
