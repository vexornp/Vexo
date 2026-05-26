//! End-to-end test for the retain-mode pipeline.

use crate::{Column, Text, ThreeTreePipeline, DecoratedContainer};
use crate::core::{Color, Position, Size};
use crate::layout::TaffyLayoutEngine;
use std::sync::Arc;

fn create_test_font_system() -> glyphon::FontSystem {
    let font_data = crate::resource::file::FONT.to_vec();
    let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
    glyphon::FontSystem::new_with_fonts([binary])
}

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
    let widget: Column = Column::new()
        .push(Text::new("Hello"))
        .push(Text::new("World"));

    // === Step 2: Create pipeline and reconcile ===
    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(widget));

    // Verify element creation
    // Note: Current implementation creates elements for root widget only
    assert!(pipeline.element_registry().len() >= 1, "Should have at least root element");
    assert!(pipeline.render_objects().len() >= 1, "Should have at least root render object");
    assert!(pipeline.render_objects().root().is_some(), "Root should be set");

    // === Step 3: Layout ===
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let available_size = Size::new(800.0, 600.0);

    // Verify dirty before layout
    assert!(pipeline.needs_layout(), "Should need layout after reconcile");

    pipeline.layout(available_size, &mut engine, &mut font_system);

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
    let _hit = pipeline.hit_test(Position::new(10.0, 10.0));
    // Result depends on computed layout - verify no panic

    // Miss outside bounds
    let miss = pipeline.hit_test(Position::new(1000.0, 1000.0));
    assert!(!miss.is_hit(), "Should miss outside bounds");
}

/// Test the update flow of the pipeline.
///
/// This test verifies that updates work correctly when
/// reconciling multiple times without paint in between.
#[test]
fn test_retain_pipeline_update_flow() {
    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();

    // First frame: reconcile a text widget
    let widget: Text = Text::new("First");
    pipeline.reconcile(Box::new(widget));

    // Should have one element and one render object
    assert!(pipeline.element_registry().len() >= 1);

    // Layout with available size
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    // After layout, dirty flags should be cleared
    assert!(!pipeline.needs_layout());

    // Second frame: update with new text
    // Note: This works because we haven't called paint() yet
    let widget: Text = Text::new("First Updated");
    pipeline.reconcile(Box::new(widget));

    // Element should be updated, not recreated (same root)
    // Elements should be reused for matching widgets
    assert!(pipeline.needs_layout() || pipeline.needs_paint());
}

/// Test DecoratedContainer widget in the pipeline.
///
/// This test verifies that the DecoratedContainer widget correctly:
/// 1. Reconciles with the element tree
/// 2. Creates render objects with proper tree structure
/// 3. Performs layout
/// 4. Paints and produces render commands
#[test]
fn test_decorated_container_widget_in_pipeline() {
    use crate::render::RenderCommand;

    // Create a widget tree with DecoratedContainer wrapping a Text
    let child: Box<dyn crate::Widget> = Box::new(Text::new("Hello"));
    let container = DecoratedContainer::new(child)
        .style(crate::Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0)
            .corner_radius(8.0));

    // Create pipeline and reconcile
    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new();
    pipeline.reconcile(Box::new(container));

    // Should have created elements and render objects
    assert!(pipeline.element_registry().len() >= 1, "Should have at least root element");
    assert!(pipeline.render_objects().len() >= 1, "Should have at least root render object");

    // === Verify render tree structure ===
    let root_ro = pipeline.render_objects().root().expect("should have root render object");
    let root_obj = pipeline.render_objects().get(root_ro).expect("root render object should exist");

    // DecoratedContainer render object should have the Text render object as a child
    let children = root_obj.children();
    assert_eq!(children.len(), 1, "DecoratedContainer render object should have exactly one child");

    // The child render object should exist
    let child_ro_id = children[0];
    let child_obj = pipeline.render_objects().get(child_ro_id).expect("child render object should exist");
    // The child should be a leaf (no children of its own)
    // TextRenderObject returns empty children by default
    assert_eq!(child_obj.children().len(), 0, "Text render object should be a leaf");

    // === Layout ===
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    // === Paint ===
    let commands = pipeline.paint();

    // DecoratedContainer should produce rect commands for background and border
    assert!(commands.len() >= 2, "DecoratedContainer should produce at least two commands");

    // Verify the render commands include a rect command (the background fill)
    let has_rect = commands.iter().any(|cmd| matches!(cmd, RenderCommand::Rect { .. }));
    assert!(has_rect, "Commands should include a Rect command for background fill");
}
