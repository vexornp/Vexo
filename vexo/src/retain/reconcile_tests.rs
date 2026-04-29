//! Tests for reconciliation algorithm.

use super::*;
use super::key::Key;
use super::id::ElementId;
use super::reconcile::Reconcilable;
use std::cell::Cell;

/// Mock widget for testing reconciliation
struct MockWidget {
    key: Option<Key>,
    id: Cell<usize>,
}

impl MockWidget {
    fn new(key: Option<Key>) -> Self {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
        Self {
            key,
            id: Cell::new(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
        }
    }
}

impl Reconcilable for MockWidget {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn can_update(&self, _other: &dyn Reconcilable) -> bool {
        true
    }

    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MockElement {
            key: self.key.clone(),
            render_object: None,
        })
    }
}

/// Mock element for testing reconciliation
struct MockElement {
    key: Option<Key>,
    render_object: Option<RenderObjectId>,
}

impl Element for MockElement {
    fn mount(&mut self, _context: &mut ElementContext) {}
    fn update(&mut self, _new_widget: Box<dyn std::any::Any>, _context: &mut ElementContext) {}
    fn unmount(&mut self, _context: &mut ElementContext) {}
    fn visit_children(&self, _registry: &ElementRegistry, _visitor: &mut dyn FnMut(&dyn Element)) {}
    fn render_object(&self) -> Option<RenderObjectId> { self.render_object }
    fn widget_key(&self) -> Option<Key> { self.key.clone() }
    fn can_update(&self, _widget: &dyn std::any::Any) -> bool { true }
}

#[test]
fn test_reconcile_inserts_new_element() {
    let mut registry = ElementRegistry::new();
    let parent = ElementId::new();

    // Create parent first
    let parent_element = Box::new(MockElement { key: None, render_object: None });
    registry.mount(parent_element, None);
    registry.set_children(parent, vec![]);

    // Initial: empty
    assert_eq!(registry.children(parent).len(), 0);

    // Reconcile with single widget
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(None)),
    ];

    registry.reconcile_children(parent, widgets);

    assert_eq!(registry.children(parent).len(), 1);
}

#[test]
fn test_reconcile_updates_matching_key() {
    let mut registry = ElementRegistry::new();
    let parent = ElementId::new();

    // Create parent
    let parent_element = Box::new(MockElement { key: None, render_object: None });
    registry.mount(parent_element, None);
    registry.set_children(parent, vec![]);

    // Initial widget with key
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key1")))),
    ];
    registry.reconcile_children(parent, widgets);

    let first_child = registry.children(parent)[0];

    // Update with same key
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key1")))),
    ];
    registry.reconcile_children(parent, widgets);

    // Should be same element (updated in place)
    assert_eq!(registry.children(parent)[0], first_child);
}

#[test]
fn test_reconcile_removes_unmatched() {
    let mut registry = ElementRegistry::new();
    let parent = ElementId::new();

    // Create parent
    let parent_element = Box::new(MockElement { key: None, render_object: None });
    registry.mount(parent_element, None);
    registry.set_children(parent, vec![]);

    // Initial: two widgets
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key1")))),
        Box::new(MockWidget::new(Some(Key::new("key2")))),
    ];
    registry.reconcile_children(parent, widgets);

    assert_eq!(registry.children(parent).len(), 2);

    // Update: only one widget
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key1")))),
    ];
    registry.reconcile_children(parent, widgets);

    assert_eq!(registry.children(parent).len(), 1);
}

#[test]
fn test_reconcile_reorders_with_keys() {
    let mut registry = ElementRegistry::new();
    let parent = ElementId::new();

    // Create parent
    let parent_element = Box::new(MockElement { key: None, render_object: None });
    registry.mount(parent_element, None);
    registry.set_children(parent, vec![]);

    // Initial: key1, key2
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key1")))),
        Box::new(MockWidget::new(Some(Key::new("key2")))),
    ];
    registry.reconcile_children(parent, widgets);

    let first_id = registry.children(parent)[0];
    let second_id = registry.children(parent)[1];

    // Reorder: key2, key1
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key2")))),
        Box::new(MockWidget::new(Some(Key::new("key1")))),
    ];
    registry.reconcile_children(parent, widgets);

    // Elements should be reordered
    assert_eq!(registry.children(parent)[0], second_id);
    assert_eq!(registry.children(parent)[1], first_id);
}

#[test]
fn test_reconcile_preserves_state_on_update() {
    let mut registry = ElementRegistry::new();
    let parent = ElementId::new();

    // Create parent
    let parent_element = Box::new(MockElement { key: None, render_object: None });
    registry.mount(parent_element, None);
    registry.set_children(parent, vec![]);

    // Initial widget with key
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key1")))),
    ];
    registry.reconcile_children(parent, widgets);

    let element_count_after_first = registry.len();

    // Update with same key - should reuse element
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key1")))),
    ];
    registry.reconcile_children(parent, widgets);

    // Element count should not increase (element was reused, not created)
    assert_eq!(registry.len(), element_count_after_first);
}

#[test]
fn test_reconcile_handles_insertion_in_middle() {
    let mut registry = ElementRegistry::new();
    let parent = ElementId::new();

    // Create parent
    let parent_element = Box::new(MockElement { key: None, render_object: None });
    registry.mount(parent_element, None);
    registry.set_children(parent, vec![]);

    // Initial: key1, key3
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key1")))),
        Box::new(MockWidget::new(Some(Key::new("key3")))),
    ];
    registry.reconcile_children(parent, widgets);

    let first_id = registry.children(parent)[0];
    let second_id = registry.children(parent)[1];

    // Insert key2 in the middle: key1, key2, key3
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key1")))),
        Box::new(MockWidget::new(Some(Key::new("key2")))),
        Box::new(MockWidget::new(Some(Key::new("key3")))),
    ];
    registry.reconcile_children(parent, widgets);

    // key1 and key3 should still be at positions 0 and 2
    assert_eq!(registry.children(parent)[0], first_id);
    assert_eq!(registry.children(parent)[2], second_id);
    // New element (key2) should be at position 1
    assert_ne!(registry.children(parent)[1], first_id);
    assert_ne!(registry.children(parent)[1], second_id);
}

#[test]
fn test_reconcile_handles_non_keyed_position_matching() {
    let mut registry = ElementRegistry::new();
    let parent = ElementId::new();

    // Create parent
    let parent_element = Box::new(MockElement { key: None, render_object: None });
    registry.mount(parent_element, None);
    registry.set_children(parent, vec![]);

    // Initial: two non-keyed widgets
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(None)),
        Box::new(MockWidget::new(None)),
    ];
    registry.reconcile_children(parent, widgets);

    let first_id = registry.children(parent)[0];
    let second_id = registry.children(parent)[1];

    // Update with two non-keyed widgets at same positions
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(None)),
        Box::new(MockWidget::new(None)),
    ];
    registry.reconcile_children(parent, widgets);

    // Elements should match by position (not key since none have keys)
    assert_eq!(registry.children(parent)[0], first_id);
    assert_eq!(registry.children(parent)[1], second_id);
}

#[test]
fn test_reconcile_clears_all_children() {
    let mut registry = ElementRegistry::new();
    let parent = ElementId::new();

    // Create parent
    let parent_element = Box::new(MockElement { key: None, render_object: None });
    registry.mount(parent_element, None);
    registry.set_children(parent, vec![]);

    // Initial: two widgets
    let widgets: Vec<Box<dyn Reconcilable>> = vec![
        Box::new(MockWidget::new(Some(Key::new("key1")))),
        Box::new(MockWidget::new(Some(Key::new("key2")))),
    ];
    registry.reconcile_children(parent, widgets);

    assert_eq!(registry.children(parent).len(), 2);

    // Clear all children
    let widgets: Vec<Box<dyn Reconcilable>> = vec![];
    registry.reconcile_children(parent, widgets);

    assert_eq!(registry.children(parent).len(), 0);
}
