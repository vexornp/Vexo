//! Tests for Element registry.

use super::*;

/// Mock element for testing.
struct MockElement;

impl Element for MockElement {
    fn mount(&mut self, _context: &mut ElementContext) {}
    fn update(&mut self, _new_widget: Box<dyn Widget>, _context: &mut ElementContext) {}
    fn unmount(&mut self, _context: &mut ElementContext) {}
    fn visit_children(&self, _visitor: &mut dyn FnMut(&dyn Element)) {}
    fn render_object(&self) -> Option<RenderObjectId> { None }
    fn widget_key(&self) -> Option<Key> { None }
    fn can_update(&self, _widget: &dyn std::any::Any) -> bool { true }
}

#[test]
fn test_mount_creates_element() {
    let mut registry = ElementRegistry::new();

    let id = registry.mount(Box::new(MockElement), None);

    assert!(registry.contains(id));
}

#[test]
fn test_unmount_removes_element() {
    let mut registry = ElementRegistry::new();

    let id = registry.mount(Box::new(MockElement), None);
    registry.unmount(id);

    assert!(!registry.contains(id));
}

#[test]
fn test_children_tracking() {
    let mut registry = ElementRegistry::new();

    let parent = registry.mount(Box::new(MockElement), None);
    let child1 = registry.mount(Box::new(MockElement), Some(parent));
    let child2 = registry.mount(Box::new(MockElement), Some(parent));

    let children = registry.children(parent);
    assert_eq!(children.len(), 2);
    assert!(children.contains(&child1));
    assert!(children.contains(&child2));
}
