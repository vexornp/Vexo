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

// ─────────────────────────────────────────────────────────────────────────────
// Focus ↔ Element tree sync tests
// ─────────────────────────────────────────────────────────────────────────────

/// Mounting a widget tree should create one focus node per element.
#[test]
fn test_mount_creates_focus_node_for_every_element() {
    let mut pipeline = ThreeTreePipeline::new();

    // Column + 2 Texts = 3 application focus nodes
    let widget = Column::new()
        .push(Text::new("first"))
        .push(Text::new("second"));
    pipeline.reconcile(Box::new(widget));

    assert_eq!(
        pipeline.focus_manager().app_node_count(),
        3,
        "Expected 3 focus nodes (Column + 2 Texts)"
    );
}

/// Unmounting by reconciling an empty root should remove all focus nodes.
#[test]
fn test_unmount_removes_all_focus_nodes() {
    let mut pipeline = ThreeTreePipeline::new();

    let widget = Column::new()
        .push(Text::new("a"))
        .push(Text::new("b"));
    pipeline.reconcile(Box::new(widget));
    assert_eq!(pipeline.focus_manager().app_node_count(), 3);

    // Reconcile a leaf widget — the old Column subtree is unmounted
    pipeline.reconcile(Box::new(Text::new("replacement")));

    assert!(
        pipeline.focus_manager().app_node_count() <= 1,
        "Focus nodes should be removed after reconciling a replacement widget"
    );
}

/// Replacing a child via rebuild should not leak focus nodes.
#[test]
fn test_rebuild_replaces_focus_nodes() {
    let mut pipeline = ThreeTreePipeline::new();

    // Column [ Text("old") ]
    let widget = Column::new().push(Text::new("old"));
    pipeline.reconcile(Box::new(widget));

    // Column [ Text("new") ]
    let updated = Column::new().push(Text::new("new"));
    pipeline.update(Box::new(updated));

    assert_eq!(
        pipeline.focus_manager().app_node_count(),
        2,
        "Focus node count should stay at 2 after rebuild"
    );
}

/// Adding a child via rebuild should add a focus node.
#[test]
fn test_rebuild_adds_focus_node_for_new_child() {
    let mut pipeline = ThreeTreePipeline::new();

    // Column [ Text("a") ]
    let widget = Column::new().push(Text::new("a"));
    pipeline.reconcile(Box::new(widget));
    assert_eq!(pipeline.focus_manager().app_node_count(), 2);

    // Column [ Text("a"), Text("b") ]
    let updated = Column::new()
        .push(Text::new("a"))
        .push(Text::new("b"));
    pipeline.update(Box::new(updated));

    assert_eq!(
        pipeline.focus_manager().app_node_count(),
        3,
        "Adding a child should add one focus node"
    );
}

/// Removing a child via rebuild should remove its focus node.
#[test]
fn test_rebuild_removes_focus_node_for_removed_child() {
    let mut pipeline = ThreeTreePipeline::new();

    // Column [ Text("a"), Text("b") ]
    let widget = Column::new()
        .push(Text::new("a"))
        .push(Text::new("b"));
    pipeline.reconcile(Box::new(widget));
    assert_eq!(pipeline.focus_manager().app_node_count(), 3);

    // Column [ Text("a") ]
    let updated = Column::new().push(Text::new("a"));
    pipeline.update(Box::new(updated));

    assert_eq!(
        pipeline.focus_manager().app_node_count(),
        2,
        "Removing a child should remove its focus node"
    );
}

/// Focus tree should mirror the element tree parent-child structure.
#[test]
fn test_focus_tree_mirrors_element_tree_structure() {
    let mut pipeline = ThreeTreePipeline::new();

    // Column [ Text("a"), Text("b") ]
    let widget = Column::new()
        .push(Text::new("a"))
        .push(Text::new("b"));
    pipeline.reconcile(Box::new(widget));

    let fm = pipeline.focus_manager();

    // Root should have one child (Column)
    let root_node = fm.get(fm.root_scope()).unwrap();
    assert_eq!(root_node.children.len(), 1, "Root should have one child (Column)");

    // Column's node should have 2 children (Texts)
    let column_id = root_node.children[0];
    let column_node = fm.get(column_id).unwrap();
    assert_eq!(column_node.children.len(), 2, "Column's focus node should have 2 children");
}

/// A focused element that gets unmounted should not leave a dangling focus reference.
#[test]
fn test_unmount_focused_element_clears_focus() {
    let mut pipeline = ThreeTreePipeline::new();

    // Column [ Focus(Text("a")), Text("b") ]
    let widget = Column::new()
        .push(Focus::new(Text::new("a")))
        .push(Text::new("b"));
    pipeline.reconcile(Box::new(widget));

    let initial_node_count = pipeline.focus_manager().app_node_count();

    // Find the Focus element key from the focus tree and focus it
    let focus_element_key = {
        let fm = pipeline.focus_manager();
        let root_node = fm.get(fm.root_scope()).unwrap();
        let column_id = root_node.children[0];
        let column_node = fm.get(column_id).unwrap();
        column_node.children.first().and_then(|id| {
            fm.get(*id).and_then(|n| n.element_key)
        }).expect("Should have a Focus element")
    };

    pipeline.set_focus(Some(focus_element_key));

    // Verify it's focused
    {
        let fm = pipeline.focus_manager();
        assert_eq!(
            fm.primary_focus(),
            fm.node_for_element(focus_element_key),
            "Element should be focused after set_focus"
        );
    }

    // Rebuild with a completely different root type to force full unmount/remount.
    // This unmounts the entire Column subtree (including the focused Focus element).
    pipeline.reconcile(Box::new(Text::new("replacement")));

    // Focus node count should decrease (Column subtree unmounted)
    let new_node_count = pipeline.focus_manager().app_node_count();
    assert!(
        new_node_count < initial_node_count,
        "Focus nodes should decrease after unmounting focused subtree ({new_node_count} >= {initial_node_count})"
    );

    // Primary focus should be cleared since the focused element was unmounted
    {
        let fm = pipeline.focus_manager();
        assert!(
            fm.primary_focus().is_none(),
            "Primary focus should be cleared after focused element is unmounted"
        );
    }
}

/// Multiple reconcile cycles should not leak focus nodes.
#[test]
fn test_repeated_reconcile_no_leaks() {
    let mut pipeline = ThreeTreePipeline::new();

    for i in 0..5 {
        let widget = Column::new()
            .push(Text::new(format!("cycle-{i}-a")))
            .push(Text::new(format!("cycle-{i}-b")));
        pipeline.reconcile(Box::new(widget));
        assert_eq!(
            pipeline.focus_manager().app_node_count(),
            3,
            "Iteration {i}: expected 3 nodes after mount"
        );

        // Reconcile with a leaf to unmount previous subtree
        pipeline.reconcile(Box::new(Text::new(format!("cycle-{i}-leaf"))));
        assert!(
            pipeline.focus_manager().app_node_count() <= 1,
            "Iteration {i}: focus nodes should decrease after reconciling replacement"
        );
    }
}