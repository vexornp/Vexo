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
use crate::render_objects::{ContainerRenderObject, OffstageRenderObject, OpacityRenderObject};
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
