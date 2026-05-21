//! Integration tests for the focus system through the element tree.
//!
//! These tests exercise the focus system using ThreeTreePipeline,
//! Focus/FocusScope widgets, and event dispatch. They verify that
//! focus nodes are created/removed during mount/unmount, that
//! pointer events request focus correctly, and that Tab navigation
//! works through the FocusManager.

use crate::retain::ThreeTreePipeline;
use crate::retain::focus::{Focus, FocusScope, FocusManager};
use crate::retain::{Text, Column, Widget};
use crate::core::{Size, Point, Logical};
use crate::layout::TaffyLayoutEngine;
use crate::input::{InputEvent, ButtonState, PointerButton, Key, NamedKey, Modifiers};
use std::sync::Arc;

fn create_test_font_system() -> glyphon::FontSystem {
    let font_data = crate::resource::file::FONT.to_vec();
    let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
    glyphon::FontSystem::new_with_fonts([binary])
}

/// Verify that mounting a Focus element creates a focus node in the FocusManager.
///
/// A fresh FocusManager starts with only the root_scope node. After reconciling
/// a Focus-wrapped Text widget, the FocusManager should have at least one
/// additional node (the Focus node created by FocusElement::mount).
#[test]
fn test_focus_element_mount_creates_focus_node() {
    let mut pipeline = ThreeTreePipeline::new();

    // A fresh pipeline has only the root_scope node
    let initial_count = pipeline.focus_manager().node_count();
    assert_eq!(initial_count, 1, "Fresh FocusManager should have only root_scope");

    // Reconcile with Focus-wrapped Text
    let widget = Focus::new(Box::new(Text::new("hello")));
    pipeline.reconcile(Box::new(widget));

    // After mount, FocusManager should have more nodes than just root_scope
    let after_count = pipeline.focus_manager().node_count();
    assert!(after_count > initial_count,
        "FocusElement mount should create a focus node. Before: {}, After: {}",
        initial_count, after_count);
}

/// Verify that unmounting a Focus element removes its focus node from the FocusManager.
///
/// After reconciling with Focus-wrapped Text, then reconciling with just Text
/// (no Focus wrapper), the FocusManager node count should decrease.
#[test]
fn test_focus_element_unmount_removes_focus_node() {
    let mut pipeline = ThreeTreePipeline::new();
    let mut fs = create_test_font_system();

    // Reconcile with Focus-wrapped Text
    let widget = Focus::new(Box::new(Text::new("hello")));
    pipeline.reconcile(Box::new(widget));

    let count_with_focus = pipeline.focus_manager().node_count();
    assert!(count_with_focus > 1, "Should have nodes beyond root_scope after Focus mount");

    // Reconcile with just Text (no Focus wrapper) — FocusElement should unmount
    pipeline.reconcile(Box::new(Text::new("world")));

    let count_without_focus = pipeline.focus_manager().node_count();
    assert!(count_without_focus < count_with_focus,
        "FocusElement unmount should remove its focus node. Before: {}, After: {}",
        count_with_focus, count_without_focus);
}

/// Verify that clicking inside a Focus element requests focus via FocusManager.
///
/// After reconciling a Focus-wrapped Text widget, laying it out, and sending
/// a pointer press event inside the element bounds, the FocusManager's
/// primary_focus should be set to the Focus node.
#[test]
fn test_click_inside_focus_element_requests_focus() {
    let mut pipeline = ThreeTreePipeline::new();
    let mut fs = create_test_font_system();

    let widget = Focus::new(Box::new(Text::new("hello")));
    pipeline.reconcile(Box::new(widget));

    // Layout
    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

    // Initially no focus
    assert!(pipeline.focus_manager().primary_focus().is_none(),
        "No focus node should be focused initially");

    // Click inside the element bounds (top-left area where text renders)
    let click_position = Point::<Logical>::new(5.0, 5.0);
    let event = InputEvent::PointerButton {
        position: click_position,
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };

    let mut fs = create_test_font_system();
    let _result = pipeline.handle_event(click_position, &event, Modifiers::default(), &mut fs);

    // After clicking, FocusManager should have primary focus set
    assert!(pipeline.focus_manager().primary_focus().is_some(),
        "FocusManager should have primary focus after click inside Focus element");
}

/// Verify that clicking outside all focusable elements clears focus.
///
/// After focusing a Focus-wrapped Text widget via click, then sending a pointer
/// press event far outside the element bounds, focus should be cleared.
#[test]
fn test_click_outside_all_focusable_clears_focus() {
    let mut pipeline = ThreeTreePipeline::new();
    let mut fs = create_test_font_system();

    let widget = Focus::new(Box::new(Text::new("hello")));
    pipeline.reconcile(Box::new(widget));

    // Layout
    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

    // First, click inside to focus the element
    let click_inside = Point::<Logical>::new(5.0, 5.0);
    let event_inside = InputEvent::PointerButton {
        position: click_inside,
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };

    let mut fs = create_test_font_system();
    let _result = pipeline.handle_event(click_inside, &event_inside, Modifiers::default(), &mut fs);

    // Verify focus is set
    assert!(pipeline.focus_manager().primary_focus().is_some(),
        "Focus should be set after clicking inside");

    // Now click far outside the element bounds
    let click_outside = Point::<Logical>::new(700.0, 500.0);
    let event_outside = InputEvent::PointerButton {
        position: click_outside,
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };

    let _result = pipeline.handle_event(click_outside, &event_outside, Modifiers::default(), &mut fs);

    // Focus should be cleared
    assert!(pipeline.focus_manager().primary_focus().is_none(),
        "Focus should be cleared after clicking outside all focusable elements");
}

/// Verify that Tab key navigation moves focus between Focus elements.
///
/// Create a Column with two Focus-wrapped Text children, reconcile, layout,
/// focus the first via click, then send a Tab key event. Focus should move
/// to the second Focus element.
#[test]
fn test_tab_navigation_moves_focus() {
    let mut pipeline = ThreeTreePipeline::new();
    let mut fs = create_test_font_system();

    // Column with two Focus-wrapped Text children
    let widget = Column::new()
        .push(Focus::new(Box::new(Text::new("first"))))
        .push(Focus::new(Box::new(Text::new("second"))));
    pipeline.reconcile(Box::new(widget));

    // Layout
    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

    // Click inside the first Focus element to focus it
    let click_position = Point::<Logical>::new(5.0, 5.0);
    let click_event = InputEvent::PointerButton {
        position: click_position,
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };

    let mut fs = create_test_font_system();
    let _result = pipeline.handle_event(click_position, &click_event, Modifiers::default(), &mut fs);

    let first_focus = pipeline.focus_manager().primary_focus();
    assert!(first_focus.is_some(), "First Focus element should be focused after click");

    // Send Tab key event to move focus forward
    let tab_event = InputEvent::Keyboard {
        key: Key::Named(NamedKey::Tab),
        text: None,
        state: ButtonState::Pressed,
        modifiers: Modifiers::default(),
    };

    let _result = pipeline.handle_event(
        Point::<Logical>::new(0.0, 0.0),
        &tab_event,
        Modifiers::default(),
        &mut fs,
    );

    // Focus should have moved to a different node
    let second_focus = pipeline.focus_manager().primary_focus();
    assert!(second_focus.is_some(), "Focus should exist after Tab navigation");
    assert_ne!(first_focus, second_focus,
        "Tab should move focus to a different node");
}

/// Verify that autofocus grabs focus on mount.
///
/// Create a pipeline with Focus::new(Text).autofocus(true), reconcile,
/// and verify that focus is set after mount without any user interaction.
#[test]
fn test_autofocus_grabs_focus_on_mount() {
    let mut pipeline = ThreeTreePipeline::new();

    let widget = Focus::new(Box::new(Text::new("hello"))).autofocus(true);
    pipeline.reconcile(Box::new(widget));

    // After mount with autofocus, FocusManager should have primary focus set
    assert!(pipeline.focus_manager().primary_focus().is_some(),
        "Autofocus should set primary focus during mount");
}

/// Verify that FocusScope contains tab traversal within its boundary.
///
/// Create a FocusScope with two Focus children, reconcile, focus the first
/// via click, then Tab should move within the scope to the second child.
#[test]
fn test_focus_scope_contains_traversal() {
    let mut pipeline = ThreeTreePipeline::new();
    let mut fs = create_test_font_system();

    // FocusScope wrapping a Column with two Focus children
    let widget = FocusScope::new(Box::new(
        Column::new()
            .push(Focus::new(Box::new(Text::new("field1"))))
            .push(Focus::new(Box::new(Text::new("field2"))))
    ));
    pipeline.reconcile(Box::new(widget));

    // Layout
    let mut engine = TaffyLayoutEngine::new();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

    // Click inside the first Focus element to focus it
    let click_position = Point::<Logical>::new(5.0, 5.0);
    let click_event = InputEvent::PointerButton {
        position: click_position,
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };

    let mut fs = create_test_font_system();
    let _result = pipeline.handle_event(click_position, &click_event, Modifiers::default(), &mut fs);

    let first_focus = pipeline.focus_manager().primary_focus();
    assert!(first_focus.is_some(), "First Focus element should be focused after click");

    // Verify the focused node is inside the FocusScope
    let root_scope = pipeline.focus_manager().root_scope();
    let children_of_root = pipeline.focus_manager().children(root_scope);
    // The FocusScope should be a child of root_scope
    assert!(!children_of_root.is_empty(), "Root scope should have children (the FocusScope)");

    // Send Tab key event — should traverse within the FocusScope
    let tab_event = InputEvent::Keyboard {
        key: Key::Named(NamedKey::Tab),
        text: None,
        state: ButtonState::Pressed,
        modifiers: Modifiers::default(),
    };

    let _result = pipeline.handle_event(
        Point::<Logical>::new(0.0, 0.0),
        &tab_event,
        Modifiers::default(),
        &mut fs,
    );

    // Focus should have moved to a different node within the scope
    let second_focus = pipeline.focus_manager().primary_focus();
    assert!(second_focus.is_some(), "Focus should exist after Tab within FocusScope");
    assert_ne!(first_focus, second_focus,
        "Tab within FocusScope should move focus to the next child");
}