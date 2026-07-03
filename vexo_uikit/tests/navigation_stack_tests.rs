use vexo_uikit::NavigationController;

#[test]
fn controller_default_path_is_empty() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    assert!(
        controller.path().is_empty(),
        "new controller path must be empty"
    );
    assert_eq!(controller.depth(), 0, "new controller depth must be 0");
}

#[test]
fn controller_push_pop_round_trip() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    assert_eq!(controller.path(), vec!["a"]);
    assert_eq!(controller.depth(), 1);
    controller.push("b");
    assert_eq!(controller.path(), vec!["a", "b"]);
    assert_eq!(controller.depth(), 2);

    assert_eq!(controller.pop(), Some("b"));
    assert_eq!(controller.path(), vec!["a"]);
    assert_eq!(controller.pop(), Some("a"));
    assert_eq!(controller.path(), Vec::<&str>::new());
    assert_eq!(controller.depth(), 0);
}

#[test]
fn controller_pop_to_root_clears_path() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.push("b");
    controller.push("c");
    controller.pop_to_root();
    assert!(controller.path().is_empty());
    assert_eq!(controller.depth(), 0);
}

#[test]
fn controller_replace_swaps_top() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.replace("b");
    assert_eq!(controller.path(), vec!["b"]);
    assert_eq!(controller.depth(), 1);
}

#[test]
fn controller_replace_at_root_behaves_as_push() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.replace("only");
    assert_eq!(controller.path(), vec!["only"]);
    assert_eq!(controller.depth(), 1);
}

#[test]
fn controller_pop_at_root_is_noop() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    assert_eq!(controller.pop(), None);
    assert!(controller.path().is_empty());
    assert_eq!(controller.depth(), 0);
}

#[test]
fn controller_depth_tracks_path_length() {
    let controller: NavigationController<u32> = NavigationController::new();
    assert_eq!(controller.depth(), 0);
    controller.push(1);
    assert_eq!(controller.depth(), 1);
    controller.push(2);
    assert_eq!(controller.depth(), 2);
    controller.pop();
    assert_eq!(controller.depth(), 1);
    controller.pop_to_root();
    assert_eq!(controller.depth(), 0);
}

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

#[test]
fn controller_notify_fires_dirty_callback_on_push() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    controller.push("a");
    controller.push("b");
    assert_eq!(
        counter.load(Ordering::SeqCst),
        2,
        "push must fire dirty callback"
    );
}

#[test]
fn controller_notify_fires_dirty_callback_on_pop_only_when_nonempty() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    controller.push("a");
    controller.pop();
    assert_eq!(counter.load(Ordering::SeqCst), 2);
    controller.pop(); // at root — no fire
    assert_eq!(
        counter.load(Ordering::SeqCst),
        2,
        "pop at root must NOT fire"
    );
}

#[test]
fn controller_pop_to_root_does_not_fire_when_already_at_root() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    controller.pop_to_root();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "pop_to_root at root must NOT fire"
    );
}

#[test]
fn controller_clear_dirty_callback_silences_notify() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    controller.clear_dirty_callback();
    controller.push("a");
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "after clear, push must NOT fire"
    );
}

#[test]
fn controller_clone_shares_path_and_callback() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    let clone = controller.clone();
    clone.push("a"); // mutate via clone
    assert_eq!(
        controller.path(),
        vec!["a"],
        "clone must share path storage"
    );
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "clone must fire shared callback"
    );
}

use vexo::{Text, Widget};
use vexo_uikit::NavigationStackView;

#[test]
fn stack_view_can_be_constructed_with_builder_methods() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root"))
        .root_title("Home")
        .title(|d| format!("{}", d))
        .destination(|d| Text::new(format!("Page: {}", d)).boxed())
        .platform(vexo_uikit::Platform::Mobile);
    // No assertion on render yet — just that construction compiles and does not panic.
    let _ = view;
}
