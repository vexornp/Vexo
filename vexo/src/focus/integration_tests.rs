//! Integration tests for the focus system with the three-tree pipeline.
//!
//! These tests verify that focus management works end-to-end with the
//! element tree, event handling, and build owner synchronization —
//! without requiring a GPU or window.

use std::sync::Arc;

use crate::animation::AnimationTicker;
use crate::core::{Point, ScaleSource, Size};
use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};
use crate::layout::TaffyLayoutEngine;
use crate::{Flex, Focus, Text, ThreeTreePipeline};

fn test_clipboard() -> std::sync::Arc<dyn crate::platform::Clipboard> {
    std::sync::Arc::new(crate::platform::stub_clipboard::StubClipboard)
}

fn create_test_font_system() -> glyphon::FontSystem {
    let font_data = crate::resource::file::FONT.to_vec();
    let binary = glyphon::fontdb::Source::Binary(Arc::new(font_data));
    glyphon::FontSystem::new_with_fonts([binary])
}

fn layout_pipeline(pipeline: &mut ThreeTreePipeline, font_system: &mut glyphon::FontSystem) {
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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    // Reconcile a Focus-wrapped Text widget.
    // Focus uses FocusElement which creates an element but delegates
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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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
        &ScaleSource::default(),
        &test_clipboard(),
    );

    // Focus should be cleared
    assert!(pipeline.focused_element().is_none());
}

#[test]
fn test_programmatic_set_focus() {
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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
        &ScaleSource::default(),
        &test_clipboard(),
    );

    // Focus is cleared because no element handled the event
    assert!(pipeline.focused_element().is_none());
}

#[test]
fn test_set_focus_creates_node_on_demand() {
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    // Reconcile a Flex::column() with two Text children
    pipeline.reconcile(Box::new(
        Flex::column()
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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    let mut font_system = create_test_font_system();

    // Reconcile a TextEdit wrapped in Focus.
    // TextEdit is a Component that requests focus on pointer press.
    let controller = crate::TextEditingController::new("Hello", &mut font_system);
    let widget = Focus::new(crate::TextEdit::new(controller.clone()));
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
        &ScaleSource::default(),
        &test_clipboard(),
    );

    // Focus should be set (TextEdit requested focus)
    assert!(pipeline.focused_element().is_some());
}

#[test]
fn test_focus_wrapper_inflates_child() {
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    // Reconcile Focus::new(Text::new("Hello"))
    // Focus now overrides children() to return the child as a slice,
    // so FocusElement should inflate the child.
    let widget = Focus::new(Text::new("Hello"));
    pipeline.reconcile(Box::new(widget));

    // Should have at least 2 elements (FocusElement + Text LeafElement)
    assert!(pipeline.element_registry().len() >= 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Focus ↔ Element tree sync tests
// ─────────────────────────────────────────────────────────────────────────────

/// Mounting a widget tree should create one focus node per element.
#[test]
fn test_mount_creates_focus_node_for_every_element() {
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    // Flex::column() + 2 Texts = 3 application focus nodes
    let widget = Flex::column()
        .push(Text::new("first"))
        .push(Text::new("second"));
    pipeline.reconcile(Box::new(widget));

    assert_eq!(
        pipeline.focus_manager().app_node_count(),
        3,
        "Expected 3 focus nodes (Flex::column() + 2 Texts)"
    );
}

/// Unmounting by reconciling an empty root should remove all focus nodes.
#[test]
fn test_unmount_removes_all_focus_nodes() {
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    let widget = Flex::column().push(Text::new("a")).push(Text::new("b"));
    pipeline.reconcile(Box::new(widget));
    assert_eq!(pipeline.focus_manager().app_node_count(), 3);

    // Reconcile a leaf widget — the old Flex::column() subtree is unmounted
    pipeline.reconcile(Box::new(Text::new("replacement")));

    assert!(
        pipeline.focus_manager().app_node_count() <= 1,
        "Focus nodes should be removed after reconciling a replacement widget"
    );
}

/// Replacing a child via rebuild should not leak focus nodes.
#[test]
fn test_rebuild_replaces_focus_nodes() {
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    // Flex::column() [ Text("old") ]
    let widget = Flex::column().push(Text::new("old"));
    pipeline.reconcile(Box::new(widget));

    // Flex::column() [ Text("new") ]
    let updated = Flex::column().push(Text::new("new"));
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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    // Flex::column() [ Text("a") ]
    let widget = Flex::column().push(Text::new("a"));
    pipeline.reconcile(Box::new(widget));
    assert_eq!(pipeline.focus_manager().app_node_count(), 2);

    // Flex::column() [ Text("a"), Text("b") ]
    let updated = Flex::column().push(Text::new("a")).push(Text::new("b"));
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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    // Flex::column() [ Text("a"), Text("b") ]
    let widget = Flex::column().push(Text::new("a")).push(Text::new("b"));
    pipeline.reconcile(Box::new(widget));
    assert_eq!(pipeline.focus_manager().app_node_count(), 3);

    // Flex::column() [ Text("a") ]
    let updated = Flex::column().push(Text::new("a"));
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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    // Flex::column() [ Text("a"), Text("b") ]
    let widget = Flex::column().push(Text::new("a")).push(Text::new("b"));
    pipeline.reconcile(Box::new(widget));

    let fm = pipeline.focus_manager();

    // Root should have one child (Flex)
    let root_node = fm.get(fm.root_scope()).unwrap();
    assert_eq!(
        root_node.children.len(),
        1,
        "Root should have one child (Flex)"
    );

    // Flex's node should have 2 children (Texts)
    let column_id = root_node.children[0];
    let column_node = fm.get(column_id).unwrap();
    assert_eq!(
        column_node.children.len(),
        2,
        "Flex focus node should have 2 children"
    );
}

/// A focused element that gets unmounted should not leave a dangling focus reference.
#[test]
fn test_unmount_focused_element_clears_focus() {
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    // Flex::column() [ Focus(Text("a")), Text("b") ]
    let widget = Flex::column()
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
        column_node
            .children
            .first()
            .and_then(|id| fm.get(*id).and_then(|n| n.element_key))
            .expect("Should have a Focus element")
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
    // This unmounts the entire Flex::column() subtree (including the focused Focus element).
    pipeline.reconcile(Box::new(Text::new("replacement")));

    // Focus node count should decrease (Flex::column() subtree unmounted)
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
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

    for i in 0..5 {
        let widget = Flex::column()
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

// ─────────────────────────────────────────────────────────────────────────────
// on_focus_change callback integration tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod on_focus_change_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Test that on_focus_change fires when focus is gained and lost via the pipeline.
    #[test]
    fn test_on_focus_change_callback_via_pipeline() {
        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        let mut font_system = create_test_font_system();

        let callback_fired = Arc::new(AtomicBool::new(false));
        let callback_fired_clone = callback_fired.clone();

        let focus_widget = Focus::new(Text::new("Hello")).on_focus_change(move |focused| {
            if !focused {
                callback_fired_clone.store(true, Ordering::Relaxed);
            }
        });

        pipeline.reconcile(Box::new(focus_widget));
        layout_pipeline(&mut pipeline, &mut font_system);

        // Find the FocusElement's element key
        let focus_element_key = {
            let fm = pipeline.focus_manager();
            let root_node = fm.get(fm.root_scope()).unwrap();
            root_node
                .children
                .first()
                .and_then(|id| fm.get(*id).and_then(|n| n.element_key))
                .expect("Should have a Focus element")
        };

        // Focus the FocusElement
        pipeline.set_focus(Some(focus_element_key));
        assert!(pipeline.focused_element().is_some());

        // Now unfocus by clicking outside
        let event = pointer_press(500.0, 500.0);
        pipeline.handle_event(
            Point::new(500.0, 500.0),
            &event,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        // The on_focus_change callback should have fired with false
        assert!(
            callback_fired.load(Ordering::Relaxed),
            "on_focus_change(false) should have fired when clicking outside"
        );
    }

    /// Test that on_focus_change fires on a Focus wrapper when a descendant
    /// ScrollView gains and loses focus.
    #[test]
    fn test_on_focus_change_with_scrollview_descendant() {
        use crate::widgets::Widget;
        use crate::ScrollView;

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        let mut font_system = create_test_font_system();

        let focus_gained = Arc::new(AtomicBool::new(false));
        let focus_lost = Arc::new(AtomicBool::new(false));
        let focus_gained_clone = focus_gained.clone();
        let focus_lost_clone = focus_lost.clone();

        let focus_widget = Focus::new(
            ScrollView::new(
                Flex::column()
                    .push(Text::new("Line 1"))
                    .push(Text::new("Line 2")),
            )
            .width(200.0)
            .height(100.0),
        )
        .on_focus_change(move |focused| {
            if focused {
                focus_gained_clone.store(true, Ordering::Relaxed);
            } else {
                focus_lost_clone.store(true, Ordering::Relaxed);
            }
        });

        pipeline.reconcile(Box::new(focus_widget));
        layout_pipeline(&mut pipeline, &mut font_system);

        // Click inside the scroll view to focus it (ScrollViewElement requests focus on click)
        let event = pointer_press(10.0, 10.0);
        pipeline.handle_event(
            Point::new(10.0, 10.0),
            &event,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(
            focus_gained.load(Ordering::Relaxed),
            "on_focus_change(true) should have fired when clicking inside ScrollView"
        );

        // Click outside to unfocus
        let event = pointer_press(500.0, 500.0);
        pipeline.handle_event(
            Point::new(500.0, 500.0),
            &event,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(
            focus_lost.load(Ordering::Relaxed),
            "on_focus_change(false) should have fired when clicking outside to unfocus"
        );
    }

    /// Test that a Component wrapping Focus+ScrollView properly
    /// updates its visual state when focus is lost by clicking outside.
    /// This mirrors the FocusableScrollList pattern from shared_app.
    #[test]
    fn test_stateful_widget_focus_loss_updates_state() {
        use crate::reactive::Signal;
        use crate::widgets::Widget;
        use crate::Component;
        use crate::ComponentState;
        use crate::ScrollView;
        use std::sync::atomic::{AtomicI32, Ordering};

        // Use a thread-local to observe focus changes from inside the state
        use std::cell::RefCell;
        thread_local! {
            static FOCUS_STATE: RefCell<Arc<AtomicI32>> = RefCell::new(Arc::new(AtomicI32::new(0)));
        }

        // --- FocusableScrollList equivalent ---
        #[derive(Clone)]
        struct FocusableScrollList;

        struct FocusableScrollListState {
            is_focused: Signal<bool>,
            focus_state: Arc<AtomicI32>,
        }

        impl Default for FocusableScrollListState {
            fn default() -> Self {
                let fs = Arc::new(AtomicI32::new(0));
                FOCUS_STATE.with(|cell| *cell.borrow_mut() = fs.clone());
                Self {
                    is_focused: Signal::new(false),
                    focus_state: fs,
                }
            }
        }

        impl ComponentState for FocusableScrollListState {
            fn set_dirty_callback(&mut self, callback: std::sync::Arc<dyn Fn() + Send + Sync>) {
                self.is_focused.set_dirty_callback(callback);
            }
        }

        impl Component for FocusableScrollList {
            type State = FocusableScrollListState;

            fn render(
                &self,
                state: &mut Self::State,
                _ctx: &mut crate::RenderContext,
            ) -> Box<dyn Widget> {
                let is_focused_clone = state.is_focused.clone();
                let fs = state.focus_state.clone();
                Focus::new(
                    ScrollView::new(
                        Flex::column()
                            .push(Text::new("Line 1"))
                            .push(Text::new("Line 2")),
                    )
                    .width(200.0)
                    .height(100.0),
                )
                .on_focus_change(move |focused| {
                    is_focused_clone.set(focused);
                    fs.store(if focused { 1 } else { -1 }, Ordering::Relaxed);
                })
                .boxed()
            }
        }

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        let mut font_system = create_test_font_system();

        // Reconcile the FocusableScrollList widget
        let widget: Box<dyn Widget> = FocusableScrollList.boxed();
        pipeline.reconcile(widget);
        layout_pipeline(&mut pipeline, &mut font_system);

        let focus_state = FOCUS_STATE.with(|cell| cell.borrow().clone());

        // Click inside the scroll view to focus it
        let event = pointer_press(10.0, 10.0);
        pipeline.handle_event(
            Point::new(10.0, 10.0),
            &event,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(
            pipeline.focused_element().is_some(),
            "Focus should be gained after clicking inside ScrollView"
        );
        assert_eq!(
            focus_state.load(Ordering::Relaxed),
            1,
            "on_focus_change(true) should have fired"
        );

        // Now click outside to unfocus
        let event = pointer_press(500.0, 500.0);
        pipeline.handle_event(
            Point::new(500.0, 500.0),
            &event,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(
            pipeline.focused_element().is_none(),
            "Focus should be cleared after clicking outside ScrollView"
        );
        assert_eq!(
            focus_state.load(Ordering::Relaxed),
            -1,
            "on_focus_change(false) should have been called when clicking outside"
        );
    }

    /// Test that clicking on a child inside a ScrollView AFTER scrolling
    /// still properly focuses the ScrollView. This was a bug where
    /// hit_test_recursive computed wrong absolute_bounds for children of
    /// scrolled content, causing is_pointer_inside() to fail for the
    /// ScrollViewElement.
    #[test]
    fn test_scrollview_focus_after_scroll() {
        use crate::widgets::Widget;
        use crate::ScrollView;
        use std::sync::atomic::Ordering;

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        let mut font_system = create_test_font_system();

        let focus_gained = Arc::new(AtomicBool::new(false));
        let focus_gained_clone = focus_gained.clone();

        // Create a ScrollView with many items so it can scroll
        let mut column = Flex::column();
        for i in 0..20 {
            column = column.push(Text::new(&format!("Item {}", i)));
        }

        let focus_widget = Focus::new(ScrollView::new(column).width(200.0).height(100.0))
            .on_focus_change(move |focused| {
                if focused {
                    focus_gained_clone.store(true, Ordering::Relaxed);
                }
            });

        pipeline.reconcile(Box::new(focus_widget));
        layout_pipeline(&mut pipeline, &mut font_system);

        // First, scroll down WITHOUT clicking to focus first.
        // This simulates: user scrolls with mouse wheel, then clicks.
        // Scroll delta y = -100.0 means scroll down significantly.
        let scroll_event = InputEvent::Scroll {
            position: Point::new(10.0, 50.0),
            delta: Point::new(0.0, -100.0),
        };
        pipeline.handle_event(
            Point::new(10.0, 50.0),
            &scroll_event,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        // Process rebuilds triggered by scroll offset change and re-layout,
        // simulating a frame render cycle
        pipeline.perform_rebuilds();
        layout_pipeline(&mut pipeline, &mut font_system);

        // Now click inside the scroll view on scrolled content.
        // After scrolling down, items that were below the viewport
        // are now visible. The pointer is at (10, 50) which is inside
        // the viewport (0,0 to 200,100).
        let event = pointer_press(10.0, 50.0);
        pipeline.handle_event(
            Point::new(10.0, 50.0),
            &event,
            Modifiers::default(),
            &mut font_system,
            &ScaleSource::default(),
            &test_clipboard(),
        );

        assert!(
            focus_gained.load(Ordering::Relaxed),
            "Focus should be gained when clicking inside ScrollView after scrolling"
        );
    }
}
