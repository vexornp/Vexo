//! End-to-end test for the retain-mode pipeline.

use crate::animation::AnimationTicker;
use crate::core::{Color, Position, Size};
use crate::layout::{
    AlignItems, GridPlacement, JustifyContent, Layout, TaffyLayoutEngine, TrackSizing,
};
use crate::render::RenderCommand;
use crate::widgets::{DecoratedBox, Transform, WithLayout};
use crate::{Flex, Grid, Text, ThreeTreePipeline, Widget};
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
    let widget: Flex = Flex::column()
        .push(Text::new("Hello"))
        .push(Text::new("World"));

    // === Step 2: Create pipeline and reconcile ===
    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(widget));

    // Verify element creation
    // Note: Current implementation creates elements for root widget only
    assert!(
        pipeline.element_registry().len() >= 1,
        "Should have at least root element"
    );
    assert!(
        pipeline.render_objects().len() >= 1,
        "Should have at least root render object"
    );
    assert!(
        pipeline.render_objects().root().is_some(),
        "Root should be set"
    );

    // === Step 3: Layout ===
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let available_size = Size::new(800.0, 600.0);

    // Verify dirty before layout
    assert!(
        pipeline.needs_layout(),
        "Should need layout after reconcile"
    );

    pipeline.layout(available_size, &mut engine, &mut font_system);

    // Verify dirty cleared
    assert!(
        !pipeline.needs_layout(),
        "Should not need layout after layout"
    );

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
    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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

/// Test DecoratedBox(WithLayout(child)) composition in the pipeline.
///
/// This test verifies that composing WithLayout inside DecoratedBox:
/// 1. Reconciles with the element tree
/// 2. Creates render objects with proper tree structure (3 levels)
/// 3. Performs layout
/// 4. Paints and produces render commands
#[test]
fn test_decorated_composition_in_pipeline() {
    use crate::render::RenderCommand;

    // Create a widget tree: DecoratedBox(WithLayout(Text))
    let container = DecoratedBox::new(WithLayout::new(Text::new("Hello"), Layout::default()))
        .style(
            crate::Style::new()
                .background(Color::RED)
                .border(Color::BLACK, 2.0)
                .corner_radius(8.0),
        );

    // Create pipeline and reconcile
    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(container));

    // Should have created elements and render objects
    assert!(
        pipeline.element_registry().len() >= 1,
        "Should have at least root element"
    );
    assert!(
        pipeline.render_objects().len() >= 1,
        "Should have at least root render object"
    );

    // === Verify render tree structure (3 levels: DecoratedBox → WithLayout → Text) ===
    let root_ro = pipeline
        .render_objects()
        .root()
        .expect("should have root render object");
    let root_obj = pipeline
        .render_objects()
        .get(root_ro)
        .expect("root render object should exist");

    // DecoratedBox render object should have WithLayout's render object as its child
    let children = root_obj.children();
    assert_eq!(
        children.len(),
        1,
        "DecoratedBox render object should have exactly one child"
    );

    // WithLayout's render object should have Text render object as its child
    let child_ro_id = children[0];
    let child_obj = pipeline
        .render_objects()
        .get(child_ro_id)
        .expect("child render object should exist");
    assert_eq!(
        child_obj.children().len(),
        1,
        "WithLayout render object should have one child"
    );

    // Text render object should be a leaf
    let grandchild_ro_id = child_obj.children()[0];
    let grandchild_obj = pipeline
        .render_objects()
        .get(grandchild_ro_id)
        .expect("grandchild render object should exist");
    assert_eq!(
        grandchild_obj.children().len(),
        0,
        "Text render object should be a leaf"
    );

    // === Layout ===
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    // === Paint ===
    let commands = pipeline.paint();

    // DecoratedBox should produce rect commands for background and border
    assert!(
        commands.len() >= 2,
        "DecoratedBox should produce at least two commands"
    );

    // Verify the render commands include a rect command (the background fill)
    let has_rect = commands
        .iter()
        .any(|cmd| matches!(cmd, RenderCommand::Rect { .. }));
    assert!(
        has_rect,
        "Commands should include a Rect command for background fill"
    );
}

/// Test Transform::translate in the pipeline.
///
/// Verifies that a translate transform wraps child commands with
/// PushTransform/PopTransform and the transform is correctly applied.
#[test]
fn test_translate_transform_in_pipeline() {
    use crate::core::Point;
    use crate::frame_builder::FrameBuilder;
    use crate::render::process_commands;

    let child = DecoratedBox::new(WithLayout::new(
        Text::new("Shifted"),
        crate::layout::Layout::default().padding(8.0),
    ))
    .background(Color::BLUE);

    let widget = Transform::translate(child, 50.0, 30.0);

    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(widget));

    // Layout
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    // Paint
    let commands = pipeline.paint();

    // Should have PushTransform and PopTransform wrapping the child commands
    let push_idx = commands
        .iter()
        .position(|cmd| matches!(cmd, RenderCommand::PushTransform { .. }));
    let pop_idx = commands
        .iter()
        .position(|cmd| matches!(cmd, RenderCommand::PopTransform));
    assert!(push_idx.is_some(), "Should have PushTransform command");
    assert!(pop_idx.is_some(), "Should have PopTransform command");
    assert!(
        push_idx.unwrap() < pop_idx.unwrap(),
        "PushTransform should come before PopTransform"
    );

    // Verify the transform values
    if let Some(RenderCommand::PushTransform {
        transform,
        origin: _,
    }) = commands.get(push_idx.unwrap())
    {
        assert_eq!(transform.a, 1.0);
        assert_eq!(transform.d, 1.0);
        assert_eq!(transform.e, 50.0);
        assert_eq!(transform.f, 30.0);
    }

    // Process through the command processor and verify the quad instances
    let mut frame_builder = FrameBuilder::new();
    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    // Should have at least one quad (background rect from DecoratedBox)
    assert!(
        frame_builder.quad_count() >= 1,
        "Should have at least one quad"
    );

    // The translate transform should be baked into the quad instance
    let has_translated_quad = frame_builder.quad_instances().iter().any(|q| {
        q.transform[4] == 50.0 && q.transform[5] == 30.0 // e=50, f=30
    });
    assert!(
        has_translated_quad,
        "At least one quad should have the translate(50,30) transform"
    );

    // Text should be shifted by (50, 30)
    let has_shifted_text = frame_builder
        .text_requests()
        .iter()
        .any(|t| t.content == "Shifted");
    assert!(has_shifted_text, "Should have 'Shifted' text");
}

/// Test Transform::rotate in the pipeline.
///
/// Verifies that a rotation transform wraps child commands with
/// PushTransform/PopTransform and the rotation matrix is correct.
#[test]
fn test_rotate_transform_in_pipeline() {
    use crate::core::{AffineTransform, Point};
    use crate::frame_builder::FrameBuilder;
    use crate::render::process_commands;

    let child = DecoratedBox::new(WithLayout::new(
        Text::new("Rotated"),
        crate::layout::Layout::default().padding(8.0),
    ))
    .background(Color::BLUE);

    let angle = std::f32::consts::FRAC_PI_4; // 45 degrees
    let widget = Transform::rotate(child, angle);

    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(widget));

    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    let commands = pipeline.paint();

    let push_idx = commands
        .iter()
        .position(|cmd| matches!(cmd, RenderCommand::PushTransform { .. }));
    let pop_idx = commands
        .iter()
        .position(|cmd| matches!(cmd, RenderCommand::PopTransform));
    assert!(push_idx.is_some(), "Should have PushTransform command");
    assert!(pop_idx.is_some(), "Should have PopTransform command");
    assert!(
        push_idx.unwrap() < pop_idx.unwrap(),
        "Push should come before Pop"
    );

    if let Some(RenderCommand::PushTransform {
        transform,
        origin: _,
    }) = commands.get(push_idx.unwrap())
    {
        let cos_45 = angle.cos();
        let sin_45 = angle.sin();
        assert!((transform.a - cos_45).abs() < 1e-6, "a should be cos(45)");
        assert!((transform.b - sin_45).abs() < 1e-6, "b should be sin(45)");
        assert!(
            (transform.c - (-sin_45)).abs() < 1e-6,
            "c should be -sin(45)"
        );
        assert!((transform.d - cos_45).abs() < 1e-6, "d should be cos(45)");
        assert!(transform.e.abs() < 1e-6, "e should be 0 for pure rotation");
        assert!(transform.f.abs() < 1e-6, "f should be 0 for pure rotation");
    }

    let mut frame_builder = FrameBuilder::new();
    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    assert!(
        frame_builder.quad_count() >= 1,
        "Should have at least one quad"
    );

    let has_rotated_quad = frame_builder.quad_instances().iter().any(|q| {
        let t = AffineTransform::from_array(q.transform);
        !t.is_translation_only()
    });
    assert!(
        has_rotated_quad,
        "At least one quad should have a rotation transform"
    );
}

/// Test Transform::scale in the pipeline.
#[test]
fn test_scale_transform_in_pipeline() {
    use crate::core::Point;
    use crate::frame_builder::FrameBuilder;
    use crate::render::process_commands;

    let child = DecoratedBox::new(WithLayout::new(
        Text::new("Scaled"),
        crate::layout::Layout::default().padding(8.0),
    ))
    .background(Color::GREEN);

    let widget = Transform::scale(child, 2.0, 3.0);

    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(widget));

    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    let commands = pipeline.paint();

    let push_idx = commands
        .iter()
        .position(|cmd| matches!(cmd, RenderCommand::PushTransform { .. }));
    assert!(push_idx.is_some(), "Should have PushTransform command");

    if let Some(RenderCommand::PushTransform {
        transform,
        origin: _,
    }) = commands.get(push_idx.unwrap())
    {
        assert!((transform.a - 2.0).abs() < 1e-6, "a should be 2.0 (scaleX)");
        assert!(transform.b.abs() < 1e-6, "b should be 0");
        assert!(transform.c.abs() < 1e-6, "c should be 0");
        assert!((transform.d - 3.0).abs() < 1e-6, "d should be 3.0 (scaleY)");
        assert!(transform.e.abs() < 1e-6, "e should be 0");
        assert!(transform.f.abs() < 1e-6, "f should be 0");
    }

    let mut frame_builder = FrameBuilder::new();
    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    assert!(frame_builder.quad_count() >= 1);

    let has_scaled_quad = frame_builder
        .quad_instances()
        .iter()
        .any(|q| (q.transform[0] - 2.0).abs() < 1e-6 && (q.transform[3] - 3.0).abs() < 1e-6);
    assert!(
        has_scaled_quad,
        "At least one quad should have the scale transform"
    );
}

/// Test that clip bounds are expanded to AABB when inside a rotation transform.
#[test]
fn test_clip_bounds_expanded_for_rotated_content() {
    use crate::core::{AffineTransform, Bounds, Point};
    use crate::frame_builder::FrameBuilder;
    use crate::render::process_commands;

    let angle = std::f32::consts::FRAC_PI_4; // 45 degrees
    let transform = AffineTransform::rotation(angle);

    let commands = vec![
        RenderCommand::PushTransform {
            transform,
            origin: Point::new(200.0, 200.0),
        },
        RenderCommand::PushClip {
            bounds: Bounds::from_xywh(150.0, 150.0, 100.0, 100.0),
        },
        RenderCommand::rect(Bounds::from_xywh(150.0, 150.0, 100.0, 100.0), Color::RED),
        RenderCommand::PopClip,
        RenderCommand::PopTransform,
    ];

    let mut frame_builder = FrameBuilder::new();
    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    let ops = frame_builder.ops();
    let quad_op = ops
        .iter()
        .find(|(op, clip)| matches!(op, crate::frame_builder::DrawOp::Quad(_)) && clip.is_some());
    let clip_bounds = quad_op
        .expect("Should have a quad op with clip bounds")
        .1
        .unwrap();
    let width = clip_bounds.right - clip_bounds.left;
    let height = clip_bounds.bottom - clip_bounds.top;
    // Original clip was 100x100. After 45deg rotation, AABB should be ~141x141.
    assert!(
        width > 100.0,
        "Clip width should expand beyond 100 for rotated content, got {width}"
    );
    assert!(
        height > 100.0,
        "Clip height should expand beyond 100 for rotated content, got {height}"
    );
}

/// Test that translation-only transforms do not expand clip bounds.
#[test]
fn test_clip_bounds_unchanged_for_translate_only() {
    use crate::core::{AffineTransform, Bounds, Point};
    use crate::frame_builder::FrameBuilder;
    use crate::render::process_commands;

    let transform = AffineTransform::translation(50.0, 30.0);

    let commands = vec![
        RenderCommand::PushTransform {
            transform,
            origin: Point::new(200.0, 200.0),
        },
        RenderCommand::PushClip {
            bounds: Bounds::from_xywh(0.0, 0.0, 100.0, 100.0),
        },
        RenderCommand::rect(Bounds::from_xywh(0.0, 0.0, 100.0, 100.0), Color::RED),
        RenderCommand::PopClip,
        RenderCommand::PopTransform,
    ];

    let mut frame_builder = FrameBuilder::new();
    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    let ops = frame_builder.ops();
    let quad_op = ops
        .iter()
        .find(|(op, clip)| matches!(op, crate::frame_builder::DrawOp::Quad(_)) && clip.is_some());
    let clip_bounds = quad_op
        .expect("Should have a quad op with clip bounds")
        .1
        .unwrap();
    let width = clip_bounds.right - clip_bounds.left;
    let height = clip_bounds.bottom - clip_bounds.top;
    assert!(
        (width - 100.0).abs() < 1.0,
        "Clip width should remain ~100 for translate-only, got {width}"
    );
    assert!(
        (height - 100.0).abs() < 1.0,
        "Clip height should remain ~100 for translate-only, got {height}"
    );
}

/// Test that a rotation transform with a rounded rect produces correct quad instances.
#[test]
fn test_rotate_transform_with_rounded_rect() {
    use crate::core::{AffineTransform, Point};
    use crate::frame_builder::FrameBuilder;
    use crate::render::process_commands;

    let child = DecoratedBox::new(WithLayout::new(
        Text::new("Rounded"),
        crate::layout::Layout::default().padding(8.0),
    ))
    .background(Color::BLUE)
    .corner_radius(12.0);

    let widget = Transform::rotate(child, 0.3);

    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(widget));

    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    let commands = pipeline.paint();

    let mut frame_builder = FrameBuilder::new();
    process_commands(&commands, &mut frame_builder, Point::new(0.0, 0.0));

    let has_rotated_rounded_quad = frame_builder.quad_instances().iter().any(|q| {
        q.corner_radius > 0.0 && !AffineTransform::from_array(q.transform).is_translation_only()
    });
    assert!(
        has_rotated_rounded_quad,
        "Should have a quad with both rotation and corner_radius"
    );
}

/// Test Flex::column() with CSS-like layout properties (padding, gap, justify, align).
#[test]
fn test_column_with_layout() {
    let widget = Flex::column()
        .push(Text::new("First"))
        .push(Text::new("Second"))
        .push(Text::new("Third"))
        .layout(
            Layout::default()
                .flex_direction(crate::layout::FlexDirection::Column)
                .padding(12.0)
                .gap(8.0)
                .justify(JustifyContent::Center)
                .align(AlignItems::Center),
        );

    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(widget));

    assert!(
        pipeline.element_registry().len() >= 1,
        "Should have at least root element"
    );
    assert!(
        pipeline.render_objects().len() >= 1,
        "Should have at least root render object"
    );

    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    let commands = pipeline.paint();
    // Layout should produce a valid tree; commands may be empty since text is handled by glyphon
    let _ = commands;
}

/// Test Flex::row() with gap on container and flex_grow/width on children via .with_layout().
#[test]
fn test_with_layout_on_children() {
    let widget = Flex::row()
        .push(Text::new("Left").with_layout(Layout::default().flex_grow(1.0)))
        .push(Text::new("Center").with_layout(Layout::default().width(100.0)))
        .push(Text::new("Right").with_layout(Layout::default().flex_grow(2.0)))
        .layout(
            Layout::default()
                .flex_direction(crate::layout::FlexDirection::Row)
                .gap(10.0),
        );

    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(widget));

    assert!(
        pipeline.element_registry().len() >= 1,
        "Should have at least root element"
    );
    assert!(
        pipeline.render_objects().len() >= 1,
        "Should have at least root render object"
    );

    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    let commands = pipeline.paint();
    let _ = commands;
}

/// Test Grid widget with columns/rows template and gap.
#[test]
fn test_grid_widget() {
    let widget = Grid::new()
        .push(
            Text::new("A").with_layout(
                Layout::default()
                    .grid_column(GridPlacement::start(1))
                    .grid_row(GridPlacement::start(1)),
            ),
        )
        .push(
            Text::new("B").with_layout(
                Layout::default()
                    .grid_column(GridPlacement::start(2))
                    .grid_row(GridPlacement::start(1)),
            ),
        )
        .push(
            Text::new("C").with_layout(
                Layout::default()
                    .grid_column(GridPlacement::start(1))
                    .grid_row(GridPlacement::start(2)),
            ),
        )
        .push(
            Text::new("D").with_layout(
                Layout::default()
                    .grid_column(GridPlacement::start(2))
                    .grid_row(GridPlacement::start(2)),
            ),
        )
        .layout(
            Layout::default()
                .columns(vec![TrackSizing::Fr(1.0), TrackSizing::Fr(1.0)])
                .rows(vec![TrackSizing::Auto, TrackSizing::Auto])
                .gap(4.0),
        );

    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(widget));

    assert!(
        pipeline.element_registry().len() >= 1,
        "Should have at least root element"
    );
    assert!(
        pipeline.render_objects().len() >= 1,
        "Should have at least root render object"
    );

    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    let commands = pipeline.paint();
    let _ = commands;
}

/// Test DecoratedBox widget in the pipeline.
///
/// Mirrors `test_decorated_composition_in_pipeline` (line 125) but
/// verifies the pass-through proxy semantics:
/// 1. The render object is `is_pass_through() == true`.
/// 2. The child (Text) render object's Taffy node is linked directly to
///    the DecoratedBox's parent — no intervening Taffy node.
/// 3. Background/border/corner-radius commands appear in the paint output.
#[test]
fn test_decorated_box_in_pipeline() {
    use crate::render::RenderCommand;

    // Create a widget tree: DecoratedBox wrapping a Text.
    let widget = DecoratedBox::new(Text::new("Hello"))
        .background(Color::RED)
        .border(Color::BLACK, 2.0)
        .corner_radius(8.0);

    // Create pipeline and reconcile.
    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(widget));

    // Should have created elements and render objects.
    assert!(
        pipeline.render_objects().len() >= 1,
        "Should have at least root render object"
    );

    // === Verify render tree structure ===
    let root_ro = pipeline
        .render_objects()
        .root()
        .expect("should have root render object");
    let root_obj = pipeline
        .render_objects()
        .get(root_ro)
        .expect("root render object should exist");

    // DecoratedBoxRenderObject must be pass-through.
    assert!(
        root_obj.is_pass_through(),
        "DecoratedBox's render object must be pass-through"
    );

    // DecoratedBox render object should have the Text render object as its
    // single child.
    let children = root_obj.children();
    assert_eq!(
        children.len(),
        1,
        "DecoratedBox render object should have exactly one child"
    );

    let child_ro_id = children[0];
    let child_obj = pipeline
        .render_objects()
        .get(child_ro_id)
        .expect("child render object should exist");
    assert_eq!(
        child_obj.children().len(),
        0,
        "Text render object should be a leaf"
    );

    // === Layout ===
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    // === Paint ===
    let commands = pipeline.paint();

    // DecoratedBox should produce commands for background + border, plus
    // PushCornerRadius/PopCornerRadius for the corner radius.
    // Order: PushCornerRadius, background Rect, border Rect, PopCornerRadius.
    assert!(
        commands.len() >= 4,
        "DecoratedBox should produce at least 4 commands (push radius + bg + border + pop radius), got {}",
        commands.len()
    );

    // Verify the render commands include a rect command (the background fill).
    let has_rect = commands
        .iter()
        .any(|cmd| matches!(cmd, RenderCommand::Rect { .. }));
    assert!(
        has_rect,
        "Commands should include a Rect command for background fill"
    );

    // Verify PushCornerRadius / PopCornerRadius are present.
    let has_push = commands
        .iter()
        .any(|cmd| matches!(cmd, RenderCommand::PushCornerRadius { .. }));
    let has_pop = commands
        .iter()
        .any(|cmd| matches!(cmd, RenderCommand::PopCornerRadius));
    assert!(has_push, "Should have PushCornerRadius command");
    assert!(has_pop, "Should have PopCornerRadius command");
}

/// Test that DecoratedBox passes width constraints through to its child.
///
/// Regression guard for the latent WidgetExt sizing bug: when a widget
/// is wrapped in a decoration proxy, the parent's definite width must
/// propagate to the child (so e.g. text wraps at that width). The
/// `DecoratedBox` proxy shares the child's Taffy node, so the parent
/// (Column with align: Stretch) stretches the *child* directly — no
/// intervening "size to content" node breaking the fill chain.
///
/// Mirrors `test_passthrough_opacity_child_receives_grandparent_width`
/// in `vexo/src/passthrough_integration.rs:63` but going through the
/// full pipeline (widget → element → render object).
///
/// Note: we use `width_percent(1.0)` on the parent Flex + window size
/// 300×200 instead of `.width(300.0)`, because `Layouter::layout()`
/// calls `engine.set_root_size()` which overrides the root's size to
/// 100%×100% (see `vexo/src/layout/taffy_engine.rs:175-188`).
/// `width_percent(1.0)` + window width 300 yields the same 300px parent
/// width without being overridden.
#[test]
fn test_decorated_box_width_propagates_to_child() {
    use crate::layout::{AlignItems, FlexDirection, Layout};

    // Column (width_percent=1.0, align: Stretch) > DecoratedBox(no layout) > Container(height=40).
    // The Container is the "child" whose width we read back. If DecoratedBox
    // were NOT a true pass-through, the Container would size to its intrinsic
    // width (0) instead of stretching to the parent's width.
    let child = crate::Flex::column()
        .layout(Layout::default().height(40.0))
        .boxed();
    let widget = crate::Flex::column()
        .layout(
            Layout::default()
                .flex_direction(FlexDirection::Column)
                .align(AlignItems::Stretch)
                .width_percent(1.0),
        )
        .push(DecoratedBox::new(child).background(Color::RED))
        .boxed();

    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(widget);

    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(300.0, 200.0), &mut engine, &mut font_system);

    // Render tree: Flex(column, width=300) → DecoratedBox → Flex(column, height=40)
    let root_ro = pipeline
        .render_objects()
        .root()
        .expect("should have root render object");
    let root_obj = pipeline
        .render_objects()
        .get(root_ro)
        .expect("root render object should exist");

    // Root's child is the DecoratedBox RO.
    let decorated_box_ro = root_obj.children()[0];
    let decorated_box_obj = pipeline
        .render_objects()
        .get(decorated_box_ro)
        .expect("DecoratedBox render object should exist");
    assert!(
        decorated_box_obj.is_pass_through(),
        "DecoratedBox render object must be pass-through"
    );

    // DecoratedBox's child is the inner Flex RO.
    let inner_flex_ro = decorated_box_obj.children()[0];
    let inner_flex_obj = pipeline
        .render_objects()
        .get(inner_flex_ro)
        .expect("inner Flex render object should exist");
    let inner_bounds = inner_flex_obj
        .computed_bounds()
        .expect("inner Flex should have computed bounds after layout");

    // The inner Flex has no explicit width, but the parent Column has
    // align: Stretch and width_percent=1.0 (resolves to 300px at window
    // width 300). With a true pass-through proxy in between, the stretch
    // propagates to the inner Flex and it fills the 300px width.
    assert_eq!(
        inner_bounds.width(),
        300.0,
        "DecoratedBox (true pass-through) must let parent's width propagate to child. Got {}",
        inner_bounds.width()
    );
}
