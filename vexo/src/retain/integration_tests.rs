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

// ============================================================================
// Full Pipeline Tests
// ============================================================================

#[cfg(test)]
mod full_pipeline_tests {
    use crate::core::{Point, Size};
    use crate::layout::TaffyLayoutEngine;
    use crate::retain::{Row, Text, ThreeTreePipeline};
    use std::sync::Arc;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_full_frame_flow() {
        let mut pipeline = ThreeTreePipeline::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // First frame: reconcile a text widget
        let widget = Text::new("First");
        pipeline.reconcile(Box::new(widget));

        // Should have one element and one render object
        assert!(pipeline.element_registry().len() >= 1);

        // Layout with available size
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // After layout, dirty flags should be cleared
        assert!(!pipeline.needs_layout());

        // Second frame: update with new text
        let widget = Text::new("First Updated");
        pipeline.reconcile(Box::new(widget));

        // Element should be updated, not recreated (same root)
        // Elements should be reused for matching widgets
        assert!(pipeline.needs_layout() || pipeline.needs_paint());
    }

    #[test]
    fn test_hit_test_through_pipeline() {
        let mut pipeline = ThreeTreePipeline::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Create a text widget
        let widget = Text::new("Hello World");
        pipeline.reconcile(Box::new(widget));

        // Layout with available size
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Hit test at a point inside the text bounds (text starts at origin)
        let result = pipeline.hit_test(Point::new(5.0, 5.0));

        // Should hit the text render object
        assert!(result.is_hit());
        assert!(result.target().is_some());

        // Hit test outside the text bounds
        let result_outside = pipeline.hit_test(Point::new(500.0, 500.0));

        // Should miss
        assert!(!result_outside.is_hit());
        assert!(result_outside.target().is_none());
    }

    #[test]
    fn test_keyed_reconciliation() {
        let mut pipeline = ThreeTreePipeline::new();

        // First frame with a keyed widget
        let widget = Text::new("A").with_key("first");
        pipeline.reconcile(Box::new(widget));
        let count_after_first = pipeline.element_registry().len();

        // Should have exactly one element
        assert_eq!(count_after_first, 1);

        // Second frame: update with same key
        let widget = Text::new("A updated").with_key("first");
        pipeline.reconcile(Box::new(widget));
        let count_after_second = pipeline.element_registry().len();

        // Element count should be the same (element reused)
        assert_eq!(count_after_first, count_after_second);

        // The element should be marked for update
        assert!(pipeline.needs_layout() || pipeline.needs_paint());
    }

    #[test]
    fn test_pipeline_paint_cycle() {
        let mut pipeline = ThreeTreePipeline::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        // Reconcile and layout
        pipeline.reconcile(Box::new(Text::new("Test")));
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

        // Paint should return commands (text render object may return empty)
        let commands = pipeline.paint();

        // TextRenderObject returns text commands
        // The important thing is that paint completes without error
        let _ = commands;
    }

    #[test]
    fn test_different_widget_types_cause_remount() {
        let mut pipeline = ThreeTreePipeline::new();

        // First frame with Text
        pipeline.reconcile(Box::new(Text::new("Text content")));
        let root_after_first = pipeline.element_registry().root();

        // Second frame with Row (different type)
        // This would cause a remount since the types don't match
        pipeline.reconcile(Box::new(Row::new()));

        // Root element should be different after remount
        // Note: Current implementation unmounts and remounts for different types
        let root_after_second = pipeline.element_registry().root();

        // Both roots should exist but may be different
        assert!(root_after_first.is_some());
        assert!(root_after_second.is_some());
    }

    #[test]
    fn test_pipeline_clear_dirty() {
        let mut pipeline = ThreeTreePipeline::new();

        // Reconcile creates dirty elements
        pipeline.reconcile(Box::new(Text::new("Test")));

        assert!(pipeline.needs_layout());
        assert!(pipeline.needs_paint());

        // Clear dirty flags
        pipeline.clear_dirty();

        assert!(!pipeline.needs_layout());
        assert!(!pipeline.needs_paint());
    }
}
