use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use vexo::{
    BuildOwner, DirtyTracking, ElementKey, Flex, RenderContext, RenderObjectRegistry, Text, Widget,
};
use vexo_uikit::{
    Component, NavigationItem, NavigationSplitView, NavigationSplitViewState, Platform,
};

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

fn sample_items() -> Vec<NavigationItem<&'static str>> {
    vec![
        NavigationItem::new("inbox", "Inbox"),
        NavigationItem::new("starred", "Starred"),
        NavigationItem::new("sent", "Sent"),
    ]
}

fn render_view<T: std::hash::Hash + Eq + Clone + 'static>(
    view: NavigationSplitView<T>,
    state: &mut NavigationSplitViewState<T>,
) -> Box<dyn Widget> {
    let element_id = make_element_key();
    let mut dirty = DirtyTracking::new();
    let mut render_objects = RenderObjectRegistry::new();
    let build_owner = BuildOwner::new();
    let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    view.render(state, &mut ctx)
}

/// Recursively collect the `content()` of every `Text` widget in the tree.
///
/// Walks both `child()` (single-child modifier widgets) and `children()`
/// (multi-child containers). Leaf widgets contribute their text.
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

// ============================================================================
// State defaults
// ============================================================================

#[test]
fn navigation_state_default_detail_visible_is_false() {
    let state: NavigationSplitViewState<&str> = NavigationSplitViewState::default();
    assert!(
        !state.detail_visible.get(),
        "detail_visible must default to false (mobile starts on sidebar)"
    );
}

#[test]
fn navigation_state_detail_visible_signal_round_trips() {
    let state: NavigationSplitViewState<&str> = NavigationSplitViewState::default();
    state.detail_visible.set(true);
    assert!(state.detail_visible.get());
    state.detail_visible.set(false);
    assert!(!state.detail_visible.get());
}

// ============================================================================
// Render does-not-panic (all three modes)
// ============================================================================

#[test]
fn navigation_desktop_render_does_not_panic() {
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Desktop)
        .detail(|id| Text::new(format!("Detail: {}", id)).boxed());
    let mut state = NavigationSplitViewState::default();
    let _tree = render_view(view, &mut state);
}

#[test]
fn navigation_mobile_sidebar_render_does_not_panic() {
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Mobile)
        .detail(|id| Text::new(format!("Detail: {}", id)).boxed());
    let mut state = NavigationSplitViewState::default();
    let _tree = render_view(view, &mut state);
}

#[test]
fn navigation_mobile_detail_render_does_not_panic() {
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Mobile)
        .detail(|id| Text::new(format!("Detail: {}", id)).boxed());
    let mut state = NavigationSplitViewState::default();
    state.detail_visible.set(true);
    state.selected.set_from(&Some("inbox"));
    let _tree = render_view(view, &mut state);
}

// ============================================================================
// Conditional rendering: detail_builder invocation counts
// ============================================================================

fn make_counted_detail<T: Clone>() -> (Arc<AtomicU32>, impl Fn(&T) -> Box<dyn Widget>)
where
    T: std::fmt::Display,
{
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    let closure = move |id: &T| {
        c.fetch_add(1, Ordering::SeqCst);
        Text::new(format!("Detail: {}", id)).boxed()
    };
    (counter, closure)
}

#[test]
fn navigation_mobile_sidebar_does_not_invoke_detail_builder() {
    let (counter, detail_closure) = make_counted_detail::<&str>();
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Mobile)
        .detail(detail_closure);
    let mut state = NavigationSplitViewState::default();
    // detail_visible == false → sidebar shown → detail builder NOT called
    let _tree = render_view(view, &mut state);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "detail builder must not run when mobile sidebar is shown"
    );
}

#[test]
fn navigation_mobile_detail_invokes_detail_builder() {
    let (counter, detail_closure) = make_counted_detail::<&str>();
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Mobile)
        .detail(detail_closure);
    let mut state = NavigationSplitViewState::default();
    state.detail_visible.set(true);
    state.selected.set_from(&Some("inbox"));
    let _tree = render_view(view, &mut state);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "detail builder must run exactly once when mobile detail page is shown"
    );
}

#[test]
fn navigation_desktop_always_invokes_detail_builder_regardless_of_detail_visible() {
    let (counter, detail_closure) = make_counted_detail::<&str>();
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Desktop)
        .default_selection("inbox")
        .detail(detail_closure);
    let mut state = NavigationSplitViewState::default();
    // detail_visible is false (default), but on desktop the detail is always
    // rendered side-by-side, so the builder must still be called.
    assert!(!state.detail_visible.get());
    let _tree = render_view(view, &mut state);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "desktop must always invoke detail builder (detail_visible is mobile-only)"
    );
}

// ============================================================================
// Mobile default_selection semantics: sidebar first, not detail
// ============================================================================

#[test]
fn navigation_mobile_with_default_selection_still_shows_sidebar_first() {
    let (counter, detail_closure) = make_counted_detail::<&str>();
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Mobile)
        .default_selection("inbox")
        .detail(detail_closure);
    let mut state = NavigationSplitViewState::default();

    // default_selection highlights the row but does NOT push the detail page.
    // The user starts on the sidebar and must tap to drill in.
    assert!(!state.detail_visible.get());
    let _tree = render_view(view, &mut state);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "mobile with default_selection must start on sidebar, not invoke detail builder"
    );
}

// ============================================================================
// Structural: top-level widget is Flex with 2 children in all modes
// ============================================================================

#[test]
fn navigation_desktop_top_level_is_flex_row_with_two_children() {
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Desktop)
        .detail(|id| Text::new(format!("Detail: {}", id)).boxed());
    let mut state = NavigationSplitViewState::default();
    let tree = render_view(view, &mut state);

    let flex = tree
        .as_any()
        .downcast_ref::<Flex>()
        .expect("top-level widget should be a Flex");
    assert_eq!(
        flex.children().len(),
        2,
        "desktop layout must have sidebar + detail = 2 children"
    );
}

#[test]
fn navigation_mobile_sidebar_top_level_is_flex_with_two_children() {
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Mobile)
        .detail(|id| Text::new(format!("Detail: {}", id)).boxed());
    let mut state = NavigationSplitViewState::default();
    let tree = render_view(view, &mut state);

    let flex = tree
        .as_any()
        .downcast_ref::<Flex>()
        .expect("top-level widget should be a Flex");
    assert_eq!(
        flex.children().len(),
        2,
        "mobile sidebar layout must have header + scroll = 2 children"
    );
}

#[test]
fn navigation_mobile_detail_top_level_is_flex_with_two_children() {
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Mobile)
        .detail(|id| Text::new(format!("Detail: {}", id)).boxed());
    let mut state = NavigationSplitViewState::default();
    state.detail_visible.set(true);
    state.selected.set_from(&Some("inbox"));
    let tree = render_view(view, &mut state);

    let flex = tree
        .as_any()
        .downcast_ref::<Flex>()
        .expect("top-level widget should be a Flex");
    assert_eq!(
        flex.children().len(),
        2,
        "mobile detail layout must have header + scroll = 2 children"
    );
}

// ============================================================================
// Content verification: mobile detail page title reflects selected item label
// ============================================================================

#[test]
fn navigation_mobile_detail_title_is_selected_item_label() {
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Mobile)
        .detail(|id| Text::new(format!("Body: {}", id)).boxed());
    let mut state = NavigationSplitViewState::default();
    state.detail_visible.set(true);
    state.selected.set_from(&Some("inbox"));
    let tree = render_view(view, &mut state);

    let texts = all_text(tree.as_ref());

    // The mobile detail header shows the selected item's label ("Inbox").
    assert!(
        texts.iter().any(|t| t == "Inbox"),
        "mobile detail header title must be the selected item's label 'Inbox', got: {:?}",
        texts
    );
    // The detail body content is also present.
    assert!(
        texts.iter().any(|t| t == "Body: inbox"),
        "mobile detail body must contain 'Body: inbox', got: {:?}",
        texts
    );
}

#[test]
fn navigation_mobile_sidebar_shows_navigation_title_not_detail() {
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Mobile)
        .detail(|id| Text::new(format!("Body: {}", id)).boxed());
    let mut state = NavigationSplitViewState::default();
    // detail_visible == false → sidebar shown
    let tree = render_view(view, &mut state);

    let texts = all_text(tree.as_ref());

    // The mobile sidebar header shows "Navigation".
    assert!(
        texts.iter().any(|t| t == "Navigation"),
        "mobile sidebar header must show 'Navigation', got: {:?}",
        texts
    );
    // Sidebar must not render detail body content.
    assert!(
        !texts.iter().any(|t| t.starts_with("Body:")),
        "mobile sidebar must not render detail body content, got: {:?}",
        texts
    );
    // Sidebar must show the item labels.
    assert!(
        texts.iter().any(|t| t == "Inbox"),
        "mobile sidebar must list item label 'Inbox', got: {:?}",
        texts
    );
}

#[test]
fn navigation_mobile_detail_does_not_show_navigation_title() {
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Mobile)
        .detail(|id| Text::new(format!("Body: {}", id)).boxed());
    let mut state = NavigationSplitViewState::default();
    state.detail_visible.set(true);
    state.selected.set_from(&Some("inbox"));
    let tree = render_view(view, &mut state);

    let texts = all_text(tree.as_ref());

    // Detail page must not show the sidebar's "Navigation" title.
    assert!(
        !texts.iter().any(|t| t == "Navigation"),
        "mobile detail page must not show sidebar 'Navigation' title, got: {:?}",
        texts
    );
}

// ============================================================================
// Desktop regression: sidebar + detail side-by-side, both visible
// ============================================================================

#[test]
fn navigation_desktop_shows_both_sidebar_and_detail_content() {
    let view = NavigationSplitView::new(sample_items())
        .platform(Platform::Desktop)
        .default_selection("inbox")
        .detail(|id| Text::new(format!("Body: {}", id)).boxed());
    let mut state = NavigationSplitViewState::default();
    let tree = render_view(view, &mut state);

    let texts = all_text(tree.as_ref());

    // Desktop must show sidebar item labels.
    assert!(
        texts.iter().any(|t| t == "Inbox"),
        "desktop sidebar must list item 'Inbox', got: {:?}",
        texts
    );
    // Desktop must also show the detail body (side-by-side).
    assert!(
        texts.iter().any(|t| t == "Body: inbox"),
        "desktop must show detail body 'Body: inbox' alongside sidebar, got: {:?}",
        texts
    );
}
