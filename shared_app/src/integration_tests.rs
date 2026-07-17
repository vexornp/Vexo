//! Full-app pipeline tests that exercise Application::view() and
//! cross-tab interactions. These are integration-level because they
//! assert on the complete widget tree, not individual screens.

use crate::data::ImState;
use crate::data::ImTab;
use std::sync::Arc;
use vexo::animation::AnimationTicker;
use vexo::layout::TaffyLayoutEngine;
use vexo::{Application, RenderObjectRegistry, ThreeTreePipeline};

#[test]
fn test_full_app_view_renders_three_tabs() {
    let mut state = ImState::default();
    let view = ImState::view(&mut state);
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(view);
    assert!(
        pipeline.element_registry().len() > 15,
        "expected many elements for full three-tab shell"
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
fn test_contacts_tab_tab_bar_fits_window() {
    // Regression test: switching to the Contacts tab must not push the
    // tab bar off screen on a short window (800×600).
    let mut state = ImState::default();
    state.tab_controller.switch_to(ImTab::Contacts);
    let view = ImState::view(&mut state);
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(view);
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = vexo::resource::new_font_system();
    pipeline.layout(
        vexo::core::Size::new(800.0, 600.0),
        &mut engine,
        &mut font_system,
    );

    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("root");

    fn find_child(
        ro_reg: &RenderObjectRegistry,
        id: vexo::RenderObjectKey,
        index: usize,
    ) -> Option<vexo::RenderObjectKey> {
        ro_reg.get(id)?.children().get(index).copied()
    }

    // root → TabBarView column → second child (tab bar)
    let tab_view = find_child(ro_reg, root, 0).expect("tab view");
    let tab_bar = find_child(ro_reg, tab_view, 1).expect("tab bar");
    let bar_bounds = ro_reg
        .get(tab_bar)
        .and_then(|ro| ro.computed_bounds())
        .expect("tab bar bounds");

    let bar_bottom = bar_bounds.top + bar_bounds.height();
    assert!(
        bar_bottom <= 600.0,
        "tab bar bottom ({}) must not exceed window height (600). \
         Top={}, Height={}",
        bar_bottom,
        bar_bounds.top,
        bar_bounds.height()
    );
}

#[test]
fn test_tab_bar_claim_prevents_content_safe_area_double_consume() {
    // Regression: the TabBarView wraps page content in SafeAreaClaim::bottom
    // so the content's SafeArea (inside NavigationStackView) sees bottom=0,
    // not the global 34px home-indicator inset. Without this, the content's
    // SafeArea re-applies the bottom padding, creating a gap between the
    // input bar and the tab bar on iOS.
    //
    // This test sets non-zero safe-area insets (mimicking iOS) and verifies:
    //   1. The content SafeArea (inside SafeAreaClaim) has effective.bottom == 0.
    //   2. The tab bar's SafeArea (sibling, NOT inside SafeAreaClaim) has
    //      effective.bottom == 34 (the full global inset — it owns the edge).
    let mut state = ImState::default();
    let view = ImState::view(&mut state);
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(view);

    // Mimic iOS safe-area insets: 44pt top (status bar), 34pt bottom (home indicator)
    pipeline.set_safe_area_source(vexo::core::SafeAreaSource::new(0.0, 0.0, 44.0, 34.0));

    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = vexo::resource::new_font_system();
    pipeline.layout(
        vexo::core::Size::new(390.0, 844.0),
        &mut engine,
        &mut font_system,
    );

    // Find all SafeAreaRenderObjects in the tree by checking which ROs
    // report effective_safe_area() (only SafeAreaRenderObject does).
    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("root");

    fn find_safe_area_ros(
        ro_reg: &RenderObjectRegistry,
        id: vexo::RenderObjectKey,
        out: &mut Vec<vexo::RenderObjectKey>,
    ) {
        if let Some(ro) = ro_reg.get(id) {
            if ro.effective_safe_area().is_some() {
                out.push(id);
            }
            for &child in ro.children() {
                find_safe_area_ros(ro_reg, child, out);
            }
        }
    }

    let mut safe_area_ids = Vec::new();
    find_safe_area_ros(ro_reg, root, &mut safe_area_ids);
    assert!(
        safe_area_ids.len() >= 2,
        "expected at least 2 SafeAreaRenderObjects (content + tab bar), found {}",
        safe_area_ids.len()
    );

    // Classify each SafeArea by its effective bottom inset.
    // Content SafeArea (inside SafeAreaClaim::bottom) → bottom == 0.
    // Tab bar SafeArea (sibling, owns bottom) → bottom == 34.
    let mut found_content = false;
    let mut found_tab_bar = false;
    for id in &safe_area_ids {
        let ro = ro_reg.get(*id).expect("safe area RO");
        let effective = ro.effective_safe_area().expect("effective insets");
        if effective.bottom == 0.0 {
            found_content = true;
        } else if (effective.bottom - 34.0).abs() < 0.01 {
            found_tab_bar = true;
        }
    }

    assert!(
        found_content,
        "content SafeArea should have effective.bottom == 0 (claimed by SafeAreaClaim)"
    );
    assert!(
        found_tab_bar,
        "tab bar SafeArea should have effective.bottom == 34 (owns the edge)"
    );
}
