//! Reconciliation algorithm for updating trees.

use std::collections::{HashMap, HashSet};

use super::element::ElementRegistry;
use super::id::ElementKey;
use super::key::WidgetKey;

// Import Key for tests
#[cfg(test)]
use super::Key;

/// Trait for widgets that can be reconciled.
/// This is a minimal trait for the reconciliation algorithm.
pub trait Reconcilable {
    /// Get the key for this widget
    fn key(&self) -> Option<WidgetKey>;

    /// Check if this widget can update an existing element
    fn can_update(&self, other: &dyn Reconcilable) -> bool;

    /// Create an element for this widget
    fn create_element(&self) -> Box<dyn super::Element>;
}

impl ElementRegistry {
    /// Reconcile children of a parent element with new widgets.
    ///
    /// This implements Flutter's diffing algorithm:
    /// 1. Build key map for existing children
    /// 2. Match new widgets to existing elements by key
    /// 3. Fall back to position-based matching for non-keyed widgets
    /// 4. Unmount unmatched elements
    /// 5. Mount new widgets
    pub fn reconcile_children(&mut self, parent: ElementKey, new_widgets: Vec<Box<dyn Reconcilable>>) {
        // 1. Build key map for existing children (local keys only)
        let existing_children = self.children(parent).to_vec();
        let key_map: HashMap<WidgetKey, ElementKey> = existing_children
            .iter()
            .filter_map(|&id| {
                self.get(id)
                    .and_then(|el| el.widget_key().map(|k| (k, id)))
            })
            .collect();

        // 2. Match new widgets to existing elements
        let mut new_children = Vec::new();
        let mut matched = HashSet::new();

        for (index, widget) in new_widgets.iter().enumerate() {
            let element_id = match widget.key() {
                Some(WidgetKey::Local(key)) => {
                    // Local key: look up in map
                    if let Some(&id) = key_map.get(&WidgetKey::Local(key)) {
                        matched.insert(id);
                        Some(id)
                    } else {
                        None
                    }
                }
                Some(WidgetKey::Global(_)) => {
                    // Global keys are handled by the pipeline's global registry
                    // For now, fall back to position-based matching
                    if let Some(&id) = existing_children.get(index) {
                        if !matched.contains(&id) {
                            matched.insert(id);
                            Some(id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                None => {
                    // Non-keyed: match by position
                    if let Some(&id) = existing_children.get(index) {
                        if !matched.contains(&id) {
                            matched.insert(id);
                            Some(id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            };

            if let Some(id) = element_id {
                // Update existing element
                new_children.push(id);
            } else {
                // Mount new element
                let element = widget.create_element();
                let id = self.insert(element, Some(parent));
                new_children.push(id);
            }
        }

        // 3. Unmount unmatched elements
        for &id in &existing_children {
            if !matched.contains(&id) {
                self.unmount(id);
            }
        }

        // 4. Update children order
        self.set_children(parent, new_children);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::Key;
    use super::super::{Element, ElementContext, RenderObjectKey};
    use std::cell::Cell;

    struct MockWidget {
        key: Option<WidgetKey>,
        id: Cell<usize>,
    }

    impl MockWidget {
        fn new(key: Option<WidgetKey>) -> Self {
            static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
            Self {
                key,
                id: Cell::new(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
            }
        }
    }

    impl Reconcilable for MockWidget {
        fn key(&self) -> Option<WidgetKey> {
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

    struct MockElement {
        key: Option<WidgetKey>,
        render_object: Option<RenderObjectKey>,
    }

    impl Element for MockElement {
        fn mount(&mut self, _context: &mut ElementContext) {}
        fn update(&mut self, _new_widget: Box<dyn std::any::Any>, _context: &mut ElementContext) {}
        fn unmount(&mut self, _context: &mut ElementContext) {}
        fn render_object(&self) -> Option<RenderObjectKey> { self.render_object }
        fn widget_key(&self) -> Option<WidgetKey> { self.key.clone() }
        fn can_update(&self, _widget: &dyn std::any::Any) -> bool { true }
    }

    fn make_element_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    #[test]
    fn test_reconcile_inserts_new_element() {
        let mut registry = ElementRegistry::new();

        // Create parent first
        let parent_element = Box::new(MockElement { key: None, render_object: None });
        let parent = registry.insert(parent_element, None);
        registry.set_children(parent, vec![]);

        let widgets: Vec<Box<dyn Reconcilable>> = vec![
            Box::new(MockWidget::new(None)),
        ];

        registry.reconcile_children(parent, widgets);

        assert_eq!(registry.children(parent).len(), 1);
    }

    #[test]
    fn test_reconcile_updates_matching_key() {
        let mut registry = ElementRegistry::new();

        // Create parent
        let parent_element = Box::new(MockElement { key: None, render_object: None });
        let parent = registry.insert(parent_element, None);
        registry.set_children(parent, vec![]);

        // Initial widget with key
        let widgets: Vec<Box<dyn Reconcilable>> = vec![
            Box::new(MockWidget::new(Some(WidgetKey::Local(Key::new("key1"))))),
        ];
        registry.reconcile_children(parent, widgets);

        let first_child = registry.children(parent)[0];

        // Update with same key
        let widgets: Vec<Box<dyn Reconcilable>> = vec![
            Box::new(MockWidget::new(Some(WidgetKey::Local(Key::new("key1"))))),
        ];
        registry.reconcile_children(parent, widgets);

        // Should be same element (updated in place)
        assert_eq!(registry.children(parent)[0], first_child);
    }

    #[test]
    fn test_reconcile_removes_unmatched() {
        let mut registry = ElementRegistry::new();

        // Create parent
        let parent_element = Box::new(MockElement { key: None, render_object: None });
        let parent = registry.insert(parent_element, None);
        registry.set_children(parent, vec![]);

        // Initial: two widgets
        let widgets: Vec<Box<dyn Reconcilable>> = vec![
            Box::new(MockWidget::new(Some(WidgetKey::Local(Key::new("key1"))))),
            Box::new(MockWidget::new(Some(WidgetKey::Local(Key::new("key2"))))),
        ];
        registry.reconcile_children(parent, widgets);

        assert_eq!(registry.children(parent).len(), 2);

        // Update: only one widget
        let widgets: Vec<Box<dyn Reconcilable>> = vec![
            Box::new(MockWidget::new(Some(WidgetKey::Local(Key::new("key1"))))),
        ];
        registry.reconcile_children(parent, widgets);

        assert_eq!(registry.children(parent).len(), 1);
    }

    #[test]
    fn test_reconcile_reorders_with_keys() {
        let mut registry = ElementRegistry::new();

        // Create parent
        let parent_element = Box::new(MockElement { key: None, render_object: None });
        let parent = registry.insert(parent_element, None);
        registry.set_children(parent, vec![]);

        // Initial: key1, key2
        let widgets: Vec<Box<dyn Reconcilable>> = vec![
            Box::new(MockWidget::new(Some(WidgetKey::Local(Key::new("key1"))))),
            Box::new(MockWidget::new(Some(WidgetKey::Local(Key::new("key2"))))),
        ];
        registry.reconcile_children(parent, widgets);

        let first_id = registry.children(parent)[0];
        let second_id = registry.children(parent)[1];

        // Reorder: key2, key1
        let widgets: Vec<Box<dyn Reconcilable>> = vec![
            Box::new(MockWidget::new(Some(WidgetKey::Local(Key::new("key2"))))),
            Box::new(MockWidget::new(Some(WidgetKey::Local(Key::new("key1"))))),
        ];
        registry.reconcile_children(parent, widgets);

        // Elements should be reordered
        assert_eq!(registry.children(parent)[0], second_id);
        assert_eq!(registry.children(parent)[1], first_id);
    }
}
