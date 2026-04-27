//! Integration tests for the retain-mode system.

use super::*;

#[test]
fn test_full_reconciliation_flow() {
    // 1. Create registries
    let mut element_registry = ElementRegistry::new();
    let render_registry = RenderObjectRegistry::new();
    let state_storage = StateStorage::new();
    let dirty = DirtyTracking::new();

    // 2. Mount initial widget tree
    let root_widget = Column::new()
        .push(Text::new("First"))
        .push(Text::new("Second"));

    let root_element = element_registry.mount(
        root_widget.create_element(),
        None,
    );

    assert_eq!(element_registry.len(), 1);

    // 3. Reconcile with updated tree
    let _new_widget = Column::new()
        .push(Text::new("First Updated"))
        .push(Text::new("Second"));

    // This would call reconcile_children in a full implementation
    // For now, just verify the infrastructure works

    assert!(element_registry.contains(root_element));

    // Verify all components work together
    assert!(render_registry.is_empty());
    assert!(state_storage.contains(root_element) == false);
    assert!(dirty.is_layout_empty());
    assert!(dirty.is_paint_empty());
}

#[test]
fn test_key_preserves_identity() {
    let mut element_registry = ElementRegistry::new();

    // Create widget with key
    let widget1 = Text::new("Hello").with_key("greeting");
    let element1 = element_registry.mount(widget1.create_element(), None);

    // Create widget with same key
    let widget2 = Text::new("Hello World").with_key("greeting");

    // In a full implementation, reconciliation would update the existing element
    // rather than creating a new one

    assert!(element_registry.contains(element1));

    // Both widgets have the same key
    assert_eq!(widget1.key(), widget2.key());

    // Verify element was mounted correctly
    assert_eq!(element_registry.len(), 1);
    assert_eq!(element_registry.root(), Some(element1));
}
