//! Integration tests for the focus system with the three-tree pipeline.
//!
//! These tests verify that focus management works end-to-end with the
//! element tree, event handling, and build owner synchronization —
//! without requiring a GPU or window.

use std::sync::Arc;

use crate::core::{Point, Size};
use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
use crate::layout::TaffyLayoutEngine;
use crate::retain::{Column, Focus, Text, ThreeTreePipeline};

fn create_test_font_system() -> glyphon::FontSystem {
    let font_data = crate::resource::file::FONT.to_vec();
    let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
    glyphon::FontSystem::new_with_fonts([binary])
}

fn layout_pipeline(
    pipeline: &mut ThreeTreePipeline,
    font_system: &mut glyphon::FontSystem,
) {
    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, font_system);
}

fn pointer_press(x: f32, y: f32) -> InputEvent {
    InputEvent::PointerButton {
        position: Point::new(x, y),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[test]
fn test_focus_manager_in_pipeline() {
    let mut pipeline = ThreeTreePipeline::new();

    // Reconcile a Focus-wrapped Text widget.
    // Focus uses ContainerElement which creates an element but delegates
    // render object creation to the child. The element tree should still
    // be populated.
    let widget = Focus::new(Text::new("Hello"));
    pipeline.reconcile(Box::new(widget));

    // Should have at least one element in the registry
    assert!(!pipeline.element_registry().is_empty());
    assert!(pipeline.element_registry().root().is_some());
}

#[test]
fn test_click_outside_clears_focus() {
    let mut pipeline = ThreeTreePipeline::new();
    let mut font_system = create_test_font_system();

    // Reconcile a plain Text widget and set focus on it
    pipeline.reconcile(Box::new(Text::new("Hello")));
    layout_pipeline(&mut pipeline, &mut font_system);

    let root = pipeline.element_registry().root().unwrap();
    pipeline.set_focus(Some(root));
    assert!(pipeline.focused_element().is_some());

    // Click far outside the text bounds — hit test misses, focus is cleared
    let event = pointer_press(500.0, 500.0);
    pipeline.handle_event(
        Point::new(500.0, 500.0),
        &event,
        Modifiers::default(),
        &mut font_system,
    );

    // Focus should be cleared
    assert!(pipeline.focused_element().is_none());
}

#[test]
fn test_programmatic_set_focus() {
    let mut pipeline = ThreeTreePipeline::new();

    // Reconcile a simple Text widget (no Focus wrapper)
    pipeline.reconcile(Box::new(Text::new("Hello")));

    let root = pipeline.element_registry().root().unwrap();

    // No focus initially
    assert!(pipeline.focused_element().is_none());

    // Set focus programmatically — should create a FocusNode on demand
    pipeline.set_focus(Some(root));
    assert_eq!(pipeline.focused_element(), Some(root));
}

#[test]
fn test_programmatic_clear_focus() {
    let mut pipeline = ThreeTreePipeline::new();

    pipeline.reconcile(Box::new(Text::new("Hello")));
    let root = pipeline.element_registry().root().unwrap();

    // Set focus first
    pipeline.set_focus(Some(root));
    assert!(pipeline.focused_element().is_some());

    // Clear focus
    pipeline.set_focus(None);
    assert!(pipeline.focused_element().is_none());
}

#[test]
fn test_focus_syncs_to_build_owner() {
    let mut pipeline = ThreeTreePipeline::new();

    pipeline.reconcile(Box::new(Text::new("Hello")));
    let root = pipeline.element_registry().root().unwrap();

    // Set focus via set_focus()
    pipeline.set_focus(Some(root));

    // Call update() to sync focus to BuildOwner
    pipeline.update(Box::new(Text::new("Hello")));

    // BuildOwner should have the focused element
    assert_eq!(pipeline.build_owner().focused_element(), Some(root));
}

#[test]
fn test_click_inside_hit_succeeds_then_unfocuses() {
    let mut pipeline = ThreeTreePipeline::new();
    let mut font_system = create_test_font_system();

    // Reconcile a plain Text widget (has render objects for hit testing)
    pipeline.reconcile(Box::new(Text::new("Hello")));
    layout_pipeline(&mut pipeline, &mut font_system);

    // Set focus on the root element programmatically
    let root = pipeline.element_registry().root().unwrap();
    pipeline.set_focus(Some(root));
    assert!(pipeline.focused_element().is_some());

    // Click inside the text bounds — hit test succeeds but no element
    // handles the event (Text's LeafElement returns None from on_event),
    // so the event handler clears focus on pointer press.
    let event = pointer_press(5.0, 5.0);
    pipeline.handle_event(
        Point::new(5.0, 5.0),
        &event,
        Modifiers::default(),
        &mut font_system,
    );

    // Focus is cleared because no element handled the event
    assert!(pipeline.focused_element().is_none());
}

#[test]
fn test_set_focus_creates_node_on_demand() {
    let mut pipeline = ThreeTreePipeline::new();

    // Reconcile a Text widget without Focus wrapper
    pipeline.reconcile(Box::new(Text::new("Hello")));
    let root = pipeline.element_registry().root().unwrap();

    // Text elements don't create focus nodes on mount, but set_focus()
    // should create one on demand and request focus
    pipeline.set_focus(Some(root));

    // Focus should be set successfully
    assert_eq!(pipeline.focused_element(), Some(root));
}

#[test]
fn test_multiple_focus_requests_last_wins() {
    let mut pipeline = ThreeTreePipeline::new();

    // Reconcile a Column with two Text children
    pipeline.reconcile(Box::new(
        Column::new()
            .push(Text::new("First"))
            .push(Text::new("Second")),
    ));

    // Get the root element
    let root = pipeline.element_registry().root().unwrap();

    // Set focus on root
    pipeline.set_focus(Some(root));
    assert_eq!(pipeline.focused_element(), Some(root));

    // Set focus to None — should clear
    pipeline.set_focus(None);
    assert!(pipeline.focused_element().is_none());

    // Set focus on root again — should work
    pipeline.set_focus(Some(root));
    assert_eq!(pipeline.focused_element(), Some(root));
}

#[test]
fn test_click_to_focus_with_stateful_element() {
    let mut pipeline = ThreeTreePipeline::new();
    let mut font_system = create_test_font_system();

    // Reconcile a TextEdit wrapped in Focus.
    // TextEdit is a StatefulWidget that requests focus on pointer press.
    let controller = crate::retain::TextEditingController::new("Hello", &mut font_system);
    let widget = Focus::new(crate::retain::TextEdit::new(controller.clone()));
    pipeline.reconcile(Box::new(widget));
    layout_pipeline(&mut pipeline, &mut font_system);

    // Click inside the text bounds — TextEdit's StatefulElement
    // requests focus on pointer press.
    let event = pointer_press(5.0, 5.0);
    pipeline.handle_event(
        Point::new(5.0, 5.0),
        &event,
        Modifiers::default(),
        &mut font_system,
    );

    // Focus should be set (TextEdit requested focus)
    assert!(pipeline.focused_element().is_some());
}

#[test]
fn test_focus_wrapper_inflates_child() {
    let mut pipeline = ThreeTreePipeline::new();

    // Reconcile Focus::new(Text::new("Hello"))
    // Focus now overrides children() to return the child as a slice,
    // so ContainerElement should inflate the child.
    let widget = Focus::new(Text::new("Hello"));
    pipeline.reconcile(Box::new(widget));

    // Should have at least 2 elements (Focus ContainerElement + Text LeafElement)
    assert!(pipeline.element_registry().len() >= 2);
}