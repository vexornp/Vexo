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
