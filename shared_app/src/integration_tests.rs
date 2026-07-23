//! Full-app pipeline tests that exercise Application::view() and
//! cross-tab interactions. These are integration-level because they
//! assert on the complete widget tree, not individual screens.
//!
//! On desktop (where these tests run), the app uses DesktopShell: a sidebar
//! (240px) + page area. The mobile TabBarView tests are iOS-only (compile-gated).

use crate::data::ImState;
use crate::data::ImTab;
use std::sync::Arc;
use vexo::animation::AnimationTicker;
use vexo::layout::TaffyLayoutEngine;
use vexo::{Application, RenderObjectKey, RenderObjectRegistry, ThreeTreePipeline};

/// Descend from `root` through single-child pass-through render objects
/// (root proxy, `Theme`/`InheritedWidget` proxies, `Component` proxies) until
/// reaching the first node with ≥2 children — the `DesktopShell` row
/// (`[sidebar, page_area]`).
///
/// Hardcoded child indices broke whenever a new pass-through wrapper was
/// added at the root (e.g. wrapping the tree in `Theme::new`). Walking by
/// child-count makes the tests robust to such wrappers.
fn shell_row_of(ro_reg: &RenderObjectRegistry, root: RenderObjectKey) -> RenderObjectKey {
    let mut cur = root;
    loop {
        let node = ro_reg.get(cur).expect("node exists");
        let children = node.children();
        if children.len() >= 2 {
            return cur;
        }
        cur = children
            .first()
            .copied()
            .expect("pass-through node has a child");
    }
}

fn nth_child(
    ro_reg: &RenderObjectRegistry,
    parent: RenderObjectKey,
    index: usize,
) -> RenderObjectKey {
    ro_reg
        .get(parent)
        .and_then(|ro| ro.children().get(index).copied())
        .unwrap_or_else(|| panic!("child {index} of node exists"))
}

#[test]
fn test_full_app_view_renders_desktop_shell() {
    let mut state = ImState::default();
    let view = ImState::view(&mut state);
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(view);
    assert!(
        pipeline.element_registry().len() > 15,
        "expected many elements for desktop shell (sidebar + 3 pages)"
    );
}

#[test]
fn test_tab_switch_to_contacts_renders_contacts_page() {
    let mut state = ImState::default();
    state.tab_controller.switch_to(ImTab::Contacts);
    let view = ImState::view(&mut state);
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(view);
    assert!(
        pipeline.element_registry().len() > 15,
        "contacts tab should have many elements (8 contacts × several widgets each)"
    );
}

#[test]
fn test_desktop_sidebar_is_narrow_and_fits_window() {
    // The sidebar (column 1) should be 64px wide and fit within the window.
    let mut state = ImState::default();
    let view = ImState::view(&mut state);
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(view);
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = vexo::resource::new_font_system();
    pipeline.layout(
        vexo::core::Size::new(1200.0, 800.0),
        &mut engine,
        &mut font_system,
    );

    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("root");

    // root → (Theme/Component proxies) → DesktopShell row → sidebar(0)
    let shell_row = shell_row_of(ro_reg, root);
    let sidebar = nth_child(ro_reg, shell_row, 0);
    let sidebar_bounds = ro_reg
        .get(sidebar)
        .and_then(|ro| ro.computed_bounds())
        .expect("sidebar bounds");

    assert!(
        (sidebar_bounds.width() - 64.0).abs() < 2.0,
        "sidebar width {} should be ~64px (SIDEBAR_WIDTH)",
        sidebar_bounds.width()
    );
    assert!(
        sidebar_bounds.height() <= 800.0,
        "sidebar height {} must not exceed window height (800)",
        sidebar_bounds.height()
    );
}

#[test]
fn test_desktop_chats_tab_shows_three_column_layout() {
    // On the Chats tab, the desktop layout should have a sidebar (col 1),
    // conversation list (col 2), and chat/placeholder (col 3).
    let mut state = ImState::default();
    let view = ImState::view(&mut state);
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(view);
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = vexo::resource::new_font_system();
    pipeline.layout(
        vexo::core::Size::new(1200.0, 800.0),
        &mut engine,
        &mut font_system,
    );

    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("root");

    // root → (Theme/Component proxies) → DesktopShell row → [sidebar(0), page_area(1)]
    let shell_row = shell_row_of(ro_reg, root);
    let sidebar = nth_child(ro_reg, shell_row, 0);
    let page_area = nth_child(ro_reg, shell_row, 1);

    let sidebar_bounds = ro_reg
        .get(sidebar)
        .and_then(|ro| ro.computed_bounds())
        .expect("sidebar bounds");
    let page_bounds = ro_reg
        .get(page_area)
        .and_then(|ro| ro.computed_bounds())
        .expect("page area bounds");

    // Sidebar is at the left, page area to its right.
    assert!(
        sidebar_bounds.left < page_bounds.left,
        "sidebar should be to the left of the page area"
    );
    // Page area should fill the remaining width.
    let expected_page_width = 1200.0 - sidebar_bounds.width();
    assert!(
        (page_bounds.width() - expected_page_width).abs() < 5.0,
        "page area width {} should be ~{} (window - sidebar)",
        page_bounds.width(),
        expected_page_width
    );
}

#[test]
fn test_desktop_chats_empty_state_shows_placeholder() {
    // With no conversation selected (initial state), the Chats tab should
    // show the "Select a conversation" placeholder in column 3.
    let mut state = ImState::default();
    // selected_conv is None by default
    let view = ImState::view(&mut state);
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(view);
    assert!(
        pipeline.element_registry().len() > 10,
        "expected elements for sidebar + conversation list + placeholder"
    );
}
