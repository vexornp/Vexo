//! Full-app pipeline tests that exercise Application::view() and
//! cross-tab interactions. These are integration-level because they
//! assert on the complete widget tree, not individual screens.

use crate::app::ImState;
use crate::data::ImTab;
use std::sync::Arc;
use vexo::animation::AnimationTicker;
use vexo::layout::TaffyLayoutEngine;
use vexo::{Application, RenderObject, RenderObjectRegistry, ThreeTreePipeline};

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
