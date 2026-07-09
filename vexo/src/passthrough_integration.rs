//! Integration tests for pass-through render objects.
//!
//! Verifies that pass-through ROs (Opacity, Transform, Offstage-onstage)
//! link the grandparent's Taffy node directly to the grandchild's,
//! so the grandchild receives the grandparent's constraints.

use crate::core::AffineTransform;
use crate::core::SafeAreaSource;
use crate::core::{Color, Size};
use crate::dirty::DirtyTracking;
use crate::id::{ElementKey, RenderObjectKey};
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutEngine, TaffyLayoutEngine};
use crate::layouter::Layouter;
use crate::render::RenderCommand;
use crate::render_object::{LayoutContext, RenderObject, RenderObjectRegistry};
use crate::render_objects::{
    ContainerRenderObject, OffstageRenderObject, OpacityRenderObject, PositionedInsets,
    PositionedRenderObject, TextRenderObject,
};
use crate::widgets::transform::TransformRenderObject;

fn create_test_font_system() -> glyphon::FontSystem {
    let font_data = include_bytes!("../font.ttf").to_vec();
    let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
    glyphon::FontSystem::new_with_fonts([binary])
}

fn make_element_key() -> ElementKey {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    sm.insert(())
}

fn column_layout() -> Layout {
    Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch)
        .width_percent(1.0)
}

/// Build a tree: Flex::column → Opacity → (child RO provided).
/// Returns (root_key, opacity_key, child_key).
fn build_opacity_tree(
    registry: &mut RenderObjectRegistry,
    child_ro: Box<dyn RenderObject>,
) -> (RenderObjectKey, RenderObjectKey, RenderObjectKey) {
    let flex_elem = make_element_key();
    let opacity_elem = make_element_key();
    let child_elem = make_element_key();

    let flex_ro = Box::new(ContainerRenderObject::new(column_layout()));
    let opacity_ro = Box::new(OpacityRenderObject::new(0.5));
    let child_key = registry.create(child_ro, child_elem);
    let opacity_key = registry.create(opacity_ro, opacity_elem);
    let flex_key = registry.create(flex_ro, flex_elem);

    registry.set_child(opacity_key, child_key);
    registry.set_child(flex_key, opacity_key);
    registry.set_root(flex_key);
    (flex_key, opacity_key, child_key)
}

#[test]
fn test_passthrough_opacity_child_receives_grandparent_width() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    // Child: a simple container with fixed width we can read back.
    // Use a leaf-like container so we can read its computed bounds.
    let child_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));
    let (flex_key, opacity_key, child_key) = build_opacity_tree(&mut registry, child_ro);

    dirty.mark_needs_layout(flex_key);
    dirty.mark_needs_layout(opacity_key);
    dirty.mark_needs_layout(child_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let child_bounds = registry
        .get(child_key)
        .unwrap()
        .computed_bounds()
        .expect("child should have computed bounds");

    // Without Opacity in the way, the child (width unset, stretch) would fill
    // the Flex's width (300). With pass-through Opacity, the child should STILL
    // receive 300 — the grandparent links the grandchild directly.
    assert_eq!(
        child_bounds.width(),
        300.0,
        "pass-through Opacity must let grandchild receive grandparent's width"
    );
}

#[test]
fn test_nested_passthrough_links_correctly() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let flex_elem = make_element_key();
    let opacity_elem = make_element_key();
    let transform_elem = make_element_key();
    let child_elem = make_element_key();

    let flex_ro = Box::new(ContainerRenderObject::new(column_layout()));
    let opacity_ro = Box::new(OpacityRenderObject::new(0.5));
    let transform_ro = Box::new(TransformRenderObject::new(
        AffineTransform::translation(10.0, 0.0),
        true,
    ));
    let child_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));

    let child_key = registry.create(child_ro, child_elem);
    let transform_key = registry.create(transform_ro, transform_elem);
    let opacity_key = registry.create(opacity_ro, opacity_elem);
    let flex_key = registry.create(flex_ro, flex_elem);

    registry.set_child(transform_key, child_key);
    registry.set_child(opacity_key, transform_key);
    registry.set_child(flex_key, opacity_key);
    registry.set_root(flex_key);

    dirty.mark_needs_layout(flex_key);
    dirty.mark_needs_layout(opacity_key);
    dirty.mark_needs_layout(transform_key);
    dirty.mark_needs_layout(child_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let child_bounds = registry
        .get(child_key)
        .unwrap()
        .computed_bounds()
        .expect("child should have computed bounds");

    assert_eq!(
        child_bounds.width(),
        300.0,
        "nested pass-through (Opacity→Transform) must link grandchild to grandparent"
    );
}

#[test]
fn test_passthrough_adopts_child_size() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let flex_elem = make_element_key();
    let opacity_elem = make_element_key();
    let child_elem = make_element_key();

    let flex_ro = Box::new(ContainerRenderObject::new(
        Layout::default()
            .flex_direction(FlexDirection::Row)
            .align(AlignItems::Start),
    ));
    let opacity_ro = Box::new(OpacityRenderObject::new(0.5));
    let child_ro = Box::new(ContainerRenderObject::new(
        Layout::default().width(120.0).height(60.0),
    ));

    let child_key = registry.create(child_ro, child_elem);
    let opacity_key = registry.create(opacity_ro, opacity_elem);
    let flex_key = registry.create(flex_ro, flex_elem);

    registry.set_child(opacity_key, child_key);
    registry.set_child(flex_key, opacity_key);
    registry.set_root(flex_key);

    dirty.mark_needs_layout(flex_key);
    dirty.mark_needs_layout(opacity_key);
    dirty.mark_needs_layout(child_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let opacity_bounds = registry
        .get(opacity_key)
        .unwrap()
        .computed_bounds()
        .expect("opacity should have bounds");
    let child_bounds = registry
        .get(child_key)
        .unwrap()
        .computed_bounds()
        .expect("child should have bounds");

    // Pass-through RO adopts child's size.
    assert_eq!(opacity_bounds.width(), child_bounds.width());
    assert_eq!(opacity_bounds.height(), child_bounds.height());
    assert_eq!(opacity_bounds.width(), 120.0);
    assert_eq!(opacity_bounds.height(), 60.0);
}

#[test]
fn test_passthrough_removal_no_double_cleanup() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let flex_elem = make_element_key();
    let opacity_elem = make_element_key();
    let child_elem = make_element_key();

    let flex_ro = Box::new(ContainerRenderObject::new(column_layout()));
    let opacity_ro = Box::new(OpacityRenderObject::new(0.5));
    let child_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));

    let child_key = registry.create(child_ro, child_elem);
    let opacity_key = registry.create(opacity_ro, opacity_elem);
    let flex_key = registry.create(flex_ro, flex_elem);

    registry.set_child(opacity_key, child_key);
    registry.set_child(flex_key, opacity_key);
    registry.set_root(flex_key);

    dirty.mark_needs_layout(flex_key);
    dirty.mark_needs_layout(opacity_key);
    dirty.mark_needs_layout(child_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    // Remove the Opacity RO (pass-through). Should NOT orphan the child's node.
    registry.remove(opacity_key);
    let orphaned = registry.drain_orphaned_layout_nodes();
    assert!(
        orphaned.is_empty(),
        "removing pass-through Opacity must not orphan the child's node"
    );

    // Now remove the child RO. This SHOULD orphan its node.
    registry.remove(child_key);
    let orphaned = registry.drain_orphaned_layout_nodes();
    assert_eq!(orphaned.len(), 1, "child's node should be orphaned once");

    // engine.remove_node should not panic on the single orphaned node.
    for node in orphaned {
        engine.remove_node(node);
    }
}

#[test]
fn test_offstage_flag_flip_in_pipeline() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    let container_elem = make_element_key();
    let off1_elem = make_element_key();
    let off2_elem = make_element_key();
    let child1_elem = make_element_key();
    let child2_elem = make_element_key();

    let container_ro = Box::new(ContainerRenderObject::new(column_layout()));
    let off1_ro = Box::new(OffstageRenderObject::new(false)); // onstage
    let off2_ro = Box::new(OffstageRenderObject::new(true)); // offstage
    let child1_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));
    let child2_ro = Box::new(ContainerRenderObject::new(Layout::default().height(40.0)));

    let child1_key = registry.create(child1_ro, child1_elem);
    let child2_key = registry.create(child2_ro, child2_elem);
    let off1_key = registry.create(off1_ro, off1_elem);
    let off2_key = registry.create(off2_ro, off2_elem);
    let container_key = registry.create(container_ro, container_elem);

    registry.set_child(off1_key, child1_key);
    registry.set_child(off2_key, child2_key);
    // Add both offstage ROs as children of the container.
    {
        let container = registry.get_mut(container_key).unwrap();
        container.as_mut().add_child(off1_key);
        container.as_mut().add_child(off2_key);
    }
    registry.set_root(container_key);

    for k in [container_key, off1_key, off2_key, child1_key, child2_key] {
        dirty.mark_needs_layout(k);
    }

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    // Initially: off1 onstage, child1 should have width 300.
    let child1_bounds = registry
        .get(child1_key)
        .unwrap()
        .computed_bounds()
        .expect("child1 should have bounds");
    assert_eq!(
        child1_bounds.width(),
        300.0,
        "onstage child1 should fill width"
    );

    // off2 offstage: zero-size bounds.
    let off2_bounds = registry
        .get(off2_key)
        .unwrap()
        .computed_bounds()
        .expect("off2 should have bounds");
    assert_eq!(off2_bounds.width(), 0.0);
    assert_eq!(off2_bounds.height(), 0.0);

    // Flip: off1 -> offstage, off2 -> onstage
    {
        let off1 = registry.get_mut(off1_key).unwrap();
        off1.as_mut()
            .as_any_mut()
            .downcast_mut::<OffstageRenderObject>()
            .unwrap()
            .set_offstage(true);
    }
    {
        let off2 = registry.get_mut(off2_key).unwrap();
        off2.as_mut()
            .as_any_mut()
            .downcast_mut::<OffstageRenderObject>()
            .unwrap()
            .set_offstage(false);
    }
    dirty.mark_needs_layout(off1_key);
    dirty.mark_needs_layout(off2_key);
    // The container must re-layout so its Taffy child list picks up the
    // Offstage ROs' new layout_node() identities after the flag flip.
    dirty.mark_needs_layout(container_key);

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(300.0, 200.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    // After flip: off2 onstage, child2 should have width 300.
    let child2_bounds = registry
        .get(child2_key)
        .unwrap()
        .computed_bounds()
        .expect("child2 should have bounds after flip");
    assert_eq!(
        child2_bounds.width(),
        300.0,
        "newly-onstage child2 should fill width after flip"
    );

    // off1 now offstage: zero-size bounds.
    let off1_bounds = registry
        .get(off1_key)
        .unwrap()
        .computed_bounds()
        .expect("off1 should have bounds after flip");
    assert_eq!(off1_bounds.width(), 0.0);
    assert_eq!(off1_bounds.height(), 0.0);
}

#[test]
fn test_nav_transition_text_does_not_wrap() {
    let mut registry = RenderObjectRegistry::new();
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    let mut dirty = DirtyTracking::new();

    // Tree (outgoing page only, for simplicity):
    //   Flex::column (root, fills 375 width)
    //   ├── nav_bar (Flex::row, width 140, flex_shrink 0)
    //   └── Stack
    //       └── Positioned(L=R=T=B=0)
    //           └── Opacity(0.5)            ← pass-through
    //               └── Transform(translate) ← pass-through
    //                   └── page Column (padding 24)
    //                       └── Text("This is a long text that should not wrap")

    let root_elem = make_element_key();
    let navbar_elem = make_element_key();
    let stack_elem = make_element_key();
    let pos_elem = make_element_key();
    let opacity_elem = make_element_key();
    let transform_elem = make_element_key();
    let page_elem = make_element_key();
    let text_elem = make_element_key();

    let root_ro = Box::new(ContainerRenderObject::new(
        Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .width_percent(1.0)
            .height_percent(1.0),
    ));
    let navbar_ro = Box::new(ContainerRenderObject::new(
        Layout::default()
            .flex_direction(FlexDirection::Row)
            .width(140.0)
            .flex_shrink(0.0),
    ));
    let stack_ro = Box::new(ContainerRenderObject::new(
        Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .width_percent(1.0)
            .height_percent(1.0),
    ));
    let pos_ro = Box::new(PositionedRenderObject::new(PositionedInsets {
        left: Some(0.0),
        right: Some(0.0),
        top: Some(0.0),
        bottom: Some(0.0),
    }));
    let opacity_ro = Box::new(OpacityRenderObject::new(0.5));
    let transform_ro = Box::new(TransformRenderObject::new(
        AffineTransform::translation(0.0, 0.0),
        true,
    ));
    let page_ro = Box::new(ContainerRenderObject::new(
        Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .padding(24.0),
    ));
    let text_ro = Box::new(
        TextRenderObject::new("This is a long text that should not wrap")
            .with_font_size(16.0)
            .with_line_height(1.2),
    );

    let text_key = registry.create(text_ro, text_elem);
    let page_key = registry.create(page_ro, page_elem);
    let transform_key = registry.create(transform_ro, transform_elem);
    let opacity_key = registry.create(opacity_ro, opacity_elem);
    let pos_key = registry.create(pos_ro, pos_elem);
    let stack_key = registry.create(stack_ro, stack_elem);
    let navbar_key = registry.create(navbar_ro, navbar_elem);
    let root_key = registry.create(root_ro, root_elem);

    registry.set_child(page_key, text_key);
    registry.set_child(transform_key, page_key);
    registry.set_child(opacity_key, transform_key);
    registry.set_child(pos_key, opacity_key);
    registry.set_child(stack_key, pos_key);
    registry.set_child(root_key, navbar_key);
    {
        let root = registry.get_mut(root_key).unwrap();
        root.as_mut().add_child(stack_key);
    }
    registry.set_root(root_key);

    for k in [
        root_key,
        navbar_key,
        stack_key,
        pos_key,
        opacity_key,
        transform_key,
        page_key,
        text_key,
    ] {
        dirty.mark_needs_layout(k);
    }

    Layouter::layout(
        &mut registry,
        &mut dirty,
        Size::new(375.0, 667.0),
        &mut engine,
        &mut font_system,
        SafeAreaSource::default(),
    );

    let text_bounds = registry
        .get(text_key)
        .unwrap()
        .computed_bounds()
        .expect("text should have bounds");

    // The text's natural width ("This is a long text that should not wrap" @ 16px)
    // is ~290px. With padding 24px (48 total), the page Column needs ~338px.
    // Window is 375px. The text should NOT wrap — its height should be ~one line
    // (16.0 * 1.2 = 19.2), not multiple lines.
    let single_line_height = 16.0 * 1.2;
    assert!(
        text_bounds.height() <= single_line_height * 1.5,
        "text should not wrap (height {} should be ~one line {}); \
         width was {}",
        text_bounds.height(),
        single_line_height,
        text_bounds.width()
    );
    assert!(
        text_bounds.width() >= 280.0,
        "text should be on one line (width {} should be >= natural ~290); \
         this means it received enough width through the pass-through ROs",
        text_bounds.width()
    );
}
