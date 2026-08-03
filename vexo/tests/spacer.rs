//! End-to-end test for Spacer: verifies that a `Spacer` placed before a
//! fixed-width sibling inside a row container absorbs the leftover space
//! and pushes the sibling to the right edge.
//!
//! Mirrors the `chat_screen.rs` use case this widget was introduced to
//! replace (`MultiChild::empty(Layout::default().flex_grow(1.0))`).

use vexo::layout::{Layout, LayoutEngine, TaffyLayoutEngine};
use vexo::render_objects::SpacerRenderObject;
use vexo::RenderObject;
use vexo::Size;

fn create_test_font_system() -> glyphon::FontSystem {
    let font_data = vexo::resource::file::FONT.to_vec();
    let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
    glyphon::FontSystem::new_with_fonts([binary])
}

#[test]
fn spacer_in_row_pushes_sibling_to_right_edge() {
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();

    // Spacer render object — created exactly the way `Spacer::new()` creates it.
    let mut spacer_ro = SpacerRenderObject::new();
    let spacer_node = {
        let mut ctx = vexo::LayoutContext::new(&mut engine, &mut font_system);
        spacer_ro.layout(&mut ctx, &[]).node
    };

    // Fixed-width sibling simulating the chat bubble.
    let bubble_node = engine.create_leaf(&Layout::default().width(80.0).height(20.0));

    // Row container 200px wide, 20px tall, holding [spacer, bubble].
    let row = engine.create_container(
        &Layout::row().width(200.0).height(20.0),
        &[spacer_node, bubble_node],
    );

    engine.compute(row, Size::new(200.0, 20.0), &mut font_system);

    let spacer_layout = engine.get_layout(spacer_node).expect("spacer has layout");
    let bubble_layout = engine.get_layout(bubble_node).expect("bubble has layout");

    // Spacer absorbs leftover width: 200 - 80 = 120.
    assert_eq!(spacer_layout.x(), 0.0);
    assert_eq!(spacer_layout.width(), 120.0);
    assert_eq!(spacer_layout.height(), 20.0);

    // Bubble is pushed to the right edge.
    assert_eq!(bubble_layout.x(), 120.0);
    assert_eq!(bubble_layout.width(), 80.0);
    assert_eq!(bubble_layout.height(), 20.0);

    // Total width adds up to the parent width.
    assert_eq!(spacer_layout.width() + bubble_layout.width(), 200.0);
}

#[test]
fn two_spacers_split_free_space_evenly() {
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();

    let mut spacer_a = SpacerRenderObject::new();
    let mut spacer_b = SpacerRenderObject::new();

    let spacer_a_node = {
        let mut ctx = vexo::LayoutContext::new(&mut engine, &mut font_system);
        spacer_a.layout(&mut ctx, &[]).node
    };
    let spacer_b_node = {
        let mut ctx = vexo::LayoutContext::new(&mut engine, &mut font_system);
        spacer_b.layout(&mut ctx, &[]).node
    };

    let bubble_node = engine.create_leaf(&Layout::default().width(50.0).height(20.0));

    let row = engine.create_container(
        &Layout::row().width(200.0).height(20.0),
        &[spacer_a_node, bubble_node, spacer_b_node],
    );

    engine.compute(row, Size::new(200.0, 20.0), &mut font_system);

    let a_layout = engine
        .get_layout(spacer_a_node)
        .expect("spacer A has layout");
    let b_layout = engine
        .get_layout(spacer_b_node)
        .expect("spacer B has layout");
    let bubble_layout = engine.get_layout(bubble_node).expect("bubble has layout");

    // Free space = 200 - 50 = 150, split evenly = 75 each.
    assert_eq!(a_layout.width(), 75.0);
    assert_eq!(b_layout.width(), 75.0);
    assert_eq!(bubble_layout.width(), 50.0);

    // Layout left-to-right: A at 0, bubble at 75, B at 125.
    assert_eq!(a_layout.x(), 0.0);
    assert_eq!(bubble_layout.x(), 75.0);
    assert_eq!(b_layout.x(), 125.0);
}
