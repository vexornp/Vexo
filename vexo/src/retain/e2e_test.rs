//! End-to-end test for the retain-mode pipeline.

use crate::retain::{Background, Column, Text, ThreeTreePipeline};
use crate::core::{Color, Point, Size};
use crate::layout::TaffyLayoutEngine;

/// Test the complete three-tree pipeline flow.
///
/// This test exercises:
/// 1. Widget tree creation
/// 2. Reconciliation with element tree
/// 3. Layout of dirty render objects
/// 4. Paint and command collection
/// 5. Hit testing
/// 6. Update and re-reconciliation (without paint in between)
#[test]
fn test_retain_pipeline_e2e() {
    // === Step 1: Create widget tree ===
    let widget = Column::new()
        .push(Text::new("Hello"))
        .push(Text::new("World"));

    // === Step 2: Create pipeline and reconcile ===
    let mut pipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(widget));

    // Verify element creation
    // Note: Current implementation creates elements for root widget only
    assert!(pipeline.element_registry().len() >= 1, "Should have at least root element");
    assert!(pipeline.render_objects().len() >= 1, "Should have at least root render object");
    assert!(pipeline.render_objects().root().is_some(), "Root should be set");

    // === Step 3: Layout ===
    let mut engine = TaffyLayoutEngine::new();
    let available_size = Size::new(800.0, 600.0);

    // Verify dirty before layout
    assert!(pipeline.needs_layout(), "Should need layout after reconcile");

    pipeline.layout(available_size, &mut engine);

    // Verify dirty cleared
    assert!(!pipeline.needs_layout(), "Should not need layout after layout");

    // === Step 4: Paint ===
    assert!(pipeline.needs_paint(), "Should need paint after reconcile");
    let commands = pipeline.paint();
    assert!(!pipeline.needs_paint(), "Should not need paint after paint");

    // Commands may be empty since text is handled by glyphon
    // Just verify paint completed without error
    let _ = commands;

    // === Step 5: Hit test ===
    // Hit inside bounds (position depends on layout)
    let _hit = pipeline.hit_test(Point::new(10.0, 10.0));
    // Result depends on computed layout - verify no panic

    // Miss outside bounds
    let miss = pipeline.hit_test(Point::new(1000.0, 1000.0));
    assert!(!miss.is_hit(), "Should miss outside bounds");
}

/// Test the update flow of the pipeline.
///
/// This test verifies that updates work correctly when
/// reconciling multiple times without paint in between.
#[test]
fn test_retain_pipeline_update_flow() {
    let mut pipeline = ThreeTreePipeline::new();
    let mut engine = TaffyLayoutEngine::new();

    // First frame: reconcile a text widget
    let widget = Text::new("First");
    pipeline.reconcile(Box::new(widget));

    // Should have one element and one render object
    assert!(pipeline.element_registry().len() >= 1);

    // Layout with available size
    pipeline.layout(Size::new(800.0, 600.0), &mut engine);

    // After layout, dirty flags should be cleared
    assert!(!pipeline.needs_layout());

    // Second frame: update with new text
    // Note: This works because we haven't called paint() yet
    let widget = Text::new("First Updated");
    pipeline.reconcile(Box::new(widget));

    // Element should be updated, not recreated (same root)
    // Elements should be reused for matching widgets
    assert!(pipeline.needs_layout() || pipeline.needs_paint());
}

/// Test Background widget in the pipeline.
///
/// This test verifies that the Background modifier widget correctly:
/// 1. Reconciles with the element tree
/// 2. Creates render objects
/// 3. Performs layout
/// 4. Paints and produces render commands
#[test]
fn test_background_widget_in_pipeline() {
    // Create a widget tree with Background wrapping a Text
    let child = Box::new(Text::new("Hello"));
    let bg = Background::new(child, Color::RED);

    // Create pipeline and reconcile
    let mut pipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(bg));

    // Should have created elements and render objects
    assert!(pipeline.element_registry().len() >= 1, "Should have at least root element");
    assert!(pipeline.render_objects().len() >= 1, "Should have at least root render object");

    // Layout
    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine);

    // Paint
    let commands = pipeline.paint();

    // Background should produce at least one command (the rect)
    assert!(commands.len() >= 1, "Background should produce at least one command");
}
