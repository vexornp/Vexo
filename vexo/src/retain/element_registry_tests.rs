//! Tests for Element registry.

use super::*;

/// Mock element for testing.
struct MockElement;

impl Element for MockElement {
    fn mount(&mut self, _context: &mut ElementContext) {}
    fn update(&mut self, _new_widget: Box<dyn std::any::Any>, _context: &mut ElementContext) {}
    fn unmount(&mut self, _context: &mut ElementContext) {}
    fn render_object(&self) -> Option<RenderObjectKey> { None }
    fn widget_key(&self) -> Option<WidgetKey> { None }
    fn can_update(&self, _widget: &dyn std::any::Any) -> bool { true }
}

#[test]
fn test_mount_creates_element() {
    let mut registry = ElementRegistry::new();

    let id = registry.insert(Box::new(MockElement), None);

    assert!(registry.contains(id));
}

#[test]
fn test_unmount_removes_element() {
    let mut registry = ElementRegistry::new();

    let id = registry.insert(Box::new(MockElement), None);
    registry.unmount(id);

    assert!(!registry.contains(id));
}

#[test]
fn test_children_tracking() {
    let mut registry = ElementRegistry::new();

    let parent = registry.insert(Box::new(MockElement), None);
    let child1 = registry.insert(Box::new(MockElement), Some(parent));
    let child2 = registry.insert(Box::new(MockElement), Some(parent));

    // insert() does NOT add children to parent's list — the pipeline
    // calls add_child() separately after executing ChildOp::Inflate.
    registry.add_child(parent, child1, None);
    registry.add_child(parent, child2, None);

    let children = registry.children(parent);
    assert_eq!(children.len(), 2);
    assert!(children.contains(&child1));
    assert!(children.contains(&child2));
}
