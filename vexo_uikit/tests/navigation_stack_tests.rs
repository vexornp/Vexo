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
