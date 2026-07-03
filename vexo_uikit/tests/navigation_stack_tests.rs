use vexo_uikit::Component;
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

#[test]
fn stack_view_state_default_compiles() {
    fn assert_default<T: Default>() {}
    assert_default::<vexo_uikit::navigation::NavigationStackViewState<&'static str>>();
}

use vexo::{BuildOwner, DirtyTracking, ElementKey, Flex, RenderContext, RenderObjectRegistry};

fn make_element_key() -> ElementKey {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    sm.insert(())
}

fn create_render_context<'a>(
    element_id: ElementKey,
    dirty: &'a mut DirtyTracking,
    render_objects: &'a mut RenderObjectRegistry,
    build_owner: &'a BuildOwner,
) -> RenderContext<'a> {
    RenderContext {
        element_id,
        dirty,
        render_objects,
        build_owner,
    }
}

fn render_stack<Dest: std::hash::Hash + Eq + Clone + 'static>(
    view: NavigationStackView<Dest>,
    state: &mut vexo_uikit::NavigationStackViewState<Dest>,
) -> Box<dyn Widget> {
    let element_id = make_element_key();
    let mut dirty = DirtyTracking::new();
    let mut render_objects = RenderObjectRegistry::new();
    let build_owner = BuildOwner::new();
    let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    view.render(state, &mut ctx)
}

fn collect_text(w: &dyn Widget, out: &mut Vec<String>) {
    if let Some(t) = w.as_any().downcast_ref::<Text>() {
        out.push(t.content().to_string());
    }
    if let Some(child) = w.child() {
        collect_text(child, out);
    }
    for child in w.children() {
        collect_text(child.as_ref(), out);
    }
}

fn all_text(w: &dyn Widget) -> Vec<String> {
    let mut out = Vec::new();
    collect_text(w, &mut out);
    out
}

/// Recursively check whether the tree contains any `Button` widget.
///
/// Used to detect the NavBar back button: `Button` stores its label as a
/// private `String` (only converted to `Text` inside its own `render()`), so
/// the back button's label is NOT visible to `collect_text`. Walking for
/// `Button` widgets instead is the reliable detector.
fn contains_button(w: &dyn Widget) -> bool {
    if w.as_any().downcast_ref::<vexo_uikit::Button>().is_some() {
        return true;
    }
    if let Some(child) = w.child() {
        if contains_button(child) {
            return true;
        }
    }
    for child in w.children() {
        if contains_button(child.as_ref()) {
            return true;
        }
    }
    false
}

#[test]
fn stack_render_root_does_not_panic() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root page")).root_title("Home");
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let _tree = render_stack(view, &mut state);
}

#[test]
fn stack_root_top_level_is_flex_column_with_two_children() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root page")).root_title("Home");
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    let flex = tree
        .as_any()
        .downcast_ref::<Flex>()
        .expect("top-level widget should be a Flex");
    assert_eq!(
        flex.children().len(),
        2,
        "root layout must have NavBar + root = 2 children"
    );
}

#[test]
fn stack_root_has_no_back_button() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root page")).root_title("Home");
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    assert!(
        !contains_button(tree.as_ref()),
        "root must NOT render a back button (NavBar should have title only)"
    );
}

#[test]
fn stack_navbar_title_uses_root_title_at_root() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root page")).root_title("Home");
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    let texts = all_text(tree.as_ref());
    assert!(
        texts.iter().any(|t| t == "Home"),
        "root NavBar must show root_title 'Home', got: {:?}",
        texts
    );
}

#[test]
fn stack_navbar_title_is_empty_when_root_title_unset() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root page"));
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    // Should not panic and should still have 2 children (NavBar with empty title + root).
    let flex = tree
        .as_any()
        .downcast_ref::<Flex>()
        .expect("top-level should be Flex");
    assert_eq!(flex.children().len(), 2);
}

#[test]
fn stack_render_pushed_page_does_not_panic() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("detail");
    let view = NavigationStackView::new(controller, Text::new("Root page"))
        .root_title("Home")
        .title(|d| format!("Title-{}", d))
        .destination(|d| Text::new(format!("Body-{}", d)).boxed());
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let _tree = render_stack(view, &mut state);
}

#[test]
fn stack_pushed_page_has_back_button() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("detail");
    let view = NavigationStackView::new(controller, Text::new("Root page"))
        .root_title("Home")
        .destination(|d| Text::new(format!("Body-{}", d)).boxed());
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    assert!(
        contains_button(tree.as_ref()),
        "pushed page NavBar must render a back Button (got no Button in tree)"
    );
}

fn make_counted_destination<T: std::fmt::Display>(
) -> (Arc<AtomicU32>, impl Fn(&T) -> Box<dyn Widget>) {
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    let closure = move |d: &T| {
        c.fetch_add(1, Ordering::SeqCst);
        Text::new(format!("Body: {}", d)).boxed()
    };
    (counter, closure)
}

#[test]
fn stack_destination_not_invoked_at_root() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let (counter, dest) = make_counted_destination::<&'static str>();
    let view = NavigationStackView::new(controller, Text::new("Root")).destination(dest);
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let _tree = render_stack(view, &mut state);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "destination builder must NOT run at root"
    );
}

#[test]
fn stack_destination_invoked_once_per_render_when_pushed() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    let (counter, dest) = make_counted_destination::<&'static str>();
    let view = NavigationStackView::new(controller, Text::new("Root")).destination(dest);
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let _tree = render_stack(view, &mut state);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "destination builder must run exactly once when pushed"
    );
}

#[test]
fn stack_navbar_title_uses_destination_title_when_pushed() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("detail");
    let view = NavigationStackView::new(controller, Text::new("Root"))
        .root_title("Home")
        .title(|d| format!("Title-{}", d))
        .destination(|d| Text::new(format!("Body-{}", d)).boxed());
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();
    let tree = render_stack(view, &mut state);

    let texts = all_text(tree.as_ref());
    assert!(
        texts.iter().any(|t| t == "Title-detail"),
        "pushed NavBar must use destination title 'Title-detail', got: {:?}",
        texts
    );
    assert!(
        !texts.iter().any(|t| t == "Home"),
        "pushed NavBar must NOT show root_title 'Home', got: {:?}",
        texts
    );
}
