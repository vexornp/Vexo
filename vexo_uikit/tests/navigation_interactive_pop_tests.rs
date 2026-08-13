//! Integration tests for interactive (swipe-to-pop) navigation.
//!
//! These tests drive the NavigationStackViewState's interactive-pop state
//! machine directly (without the full gesture pipeline) by manipulating the
//! shared `interactive_pop` cell and the NavigationController API. Full
//! end-to-end gesture tests are manual (see the design spec's manual
//! verification checklist).

use std::sync::Arc;
use std::time::{Duration, Instant};

use vexo::animation::{AnimationTicker, SpringDescription, SpringSimulation};
use vexo::inherited_registry::{InheritedMap, InheritedRegistry};
use vexo::{BuildOwner, ElementKey, Positioned, RenderContext, Text, VelocityTracker, Widget};
use vexo_uikit::{Component, NavigationController, NavigationStackView, NavigationStackViewState};

fn make_element_key() -> ElementKey {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    sm.insert(())
}

fn create_render_context<'a>(
    element_id: ElementKey,
    build_owner: &'a BuildOwner,
    inherited_map: &'a InheritedMap,
    inherited_registry: &'a InheritedRegistry,
) -> RenderContext<'a> {
    RenderContext::new(
        element_id,
        build_owner,
        inherited_map,
        inherited_registry,
        Arc::new(|| {}),
    )
}

fn render_stack<Dest: std::hash::Hash + Eq + Clone + 'static>(
    view: NavigationStackView<Dest>,
    state: &mut NavigationStackViewState<Dest>,
) -> Box<dyn Widget> {
    let element_id = make_element_key();
    let build_owner = BuildOwner::new();
    let inherited_map = InheritedMap::empty();
    let inherited_registry = InheritedRegistry::new();
    let mut ctx = create_render_context(
        element_id,
        &build_owner,
        &inherited_map,
        &inherited_registry,
    );
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

/// Recursively walk the widget tree and return the first `Positioned` widget
/// found. The interactive-pop overlay is wrapped in a `Positioned` (only when
/// `interactive_pop` is `Some`), so finding one is a structural proof that the
/// overlay rendered — stronger than a text check, since the base
/// `IndexedStack` always mounts all pages offstage.
fn find_positioned(w: &dyn Widget) -> Option<&Positioned> {
    if let Some(p) = w.as_any().downcast_ref::<Positioned>() {
        return Some(p);
    }
    if let Some(child) = w.child() {
        if let Some(found) = find_positioned(child) {
            return Some(found);
        }
    }
    for child in w.children() {
        if let Some(found) = find_positioned(child.as_ref()) {
            return Some(found);
        }
    }
    None
}

fn make_view(controller: NavigationController<&'static str>) -> NavigationStackView<&'static str> {
    NavigationStackView::new(controller, Text::new("Root"))
        .root_title("Home")
        .title(|d| d.to_string())
        .destination(|d| Text::new(format!("Page: {}", d)).boxed())
}

fn push_and_clear(controller: &NavigationController<&'static str>, dest: &'static str) {
    controller.push(dest);
    controller.clear_pending();
}

#[test]
fn interactive_pop_renders_both_pages_during_drag() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    push_and_clear(&controller, "a");
    let view = make_view(controller.clone());
    let mut state = NavigationStackViewState::default();

    // Wire a dummy ticker + dirty so the controller can be created in on_start.
    // Locals are captured first so clones can be handed to the AnimationController
    // before the originals are moved into the state via wire_for_testing.
    let ticker: Arc<AnimationTicker> = Arc::new(AnimationTicker::new());
    let dirty: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});

    // Begin interactive pop and inject a half-progress InteractivePop.
    let from_path = controller
        .begin_interactive_pop()
        .expect("non-empty path with no pending");
    let to_path: Vec<&'static str> = Vec::new();
    let mut anim = vexo::AnimationController::new(Duration::from_millis(350));
    anim.set_ticker(ticker.clone());
    anim.set_dirty_callback(dirty.clone());
    anim.set_value(0.5);

    state.wire_for_testing(ticker, dirty);
    state.set_interactive_pop(vexo_uikit::InteractivePop {
        controller: anim,
        from_path,
        to_path,
        phase: vexo_uikit::InteractivePopPhase::Dragging,
        velocity_tracker: VelocityTracker::new(),
    });

    let w = render_stack(view, &mut state);

    // Structural check: the interactive-pop overlay is wrapped in a
    // `Positioned` (only present when `interactive_pop` is `Some`). The base
    // `IndexedStack` always mounts all pages offstage, so a bare text check
    // would pass even if the overlay never rendered — finding a `Positioned`
    // proves the overlay exists.
    assert!(
        find_positioned(w.as_ref()).is_some(),
        "interactive pop must render a Positioned overlay"
    );

    // The overlay (outgoing page) should contain "Page: a".
    let texts = all_text(&w);
    assert!(
        texts.iter().any(|t| t.contains("Page: a")),
        "outgoing page must render during interactive pop, got {:?}",
        texts
    );
}

#[test]
fn interactive_pop_commit_clears_state_and_pops_path() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    push_and_clear(&controller, "a");
    let view = make_view(controller.clone());
    let mut state = NavigationStackViewState::default();

    let ticker: Arc<AnimationTicker> = Arc::new(AnimationTicker::new());
    let dirty: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});

    let from_path = controller.begin_interactive_pop().unwrap();
    let to_path: Vec<&'static str> = Vec::new();
    let mut anim = vexo::AnimationController::new(Duration::from_millis(350));
    anim.set_ticker(ticker.clone());
    anim.set_dirty_callback(dirty.clone());
    // Spring to completion.
    anim.animate_with(Box::new(SpringSimulation::new(
        SpringDescription::ios(340.0, 1.0),
        0.5,
        1.0,
        0.0,
    )));

    state.wire_for_testing(ticker, dirty);
    state.set_interactive_pop(vexo_uikit::InteractivePop {
        controller: anim,
        from_path,
        to_path,
        phase: vexo_uikit::InteractivePopPhase::Committing,
        velocity_tracker: VelocityTracker::new(),
    });

    // Advance the spring past settlement. A critically-damped iOS spring at
    // stiffness=340 settles well within 1s.
    let start = Instant::now();
    for i in 0..100u64 {
        let now = start + Duration::from_millis(20 * i);
        if let Some(ip) = state.interactive_pop_cell().borrow_mut().as_mut() {
            ip.controller.advance(now);
        }
    }

    // render() should detect completion, call commit_interactive_pop, and
    // clear the cell.
    let _w = render_stack(view, &mut state);

    assert!(
        state.interactive_pop_cell().borrow().is_none(),
        "interactive_pop cell must be cleared after commit"
    );
    assert_eq!(
        controller.path(),
        Vec::<&'static str>::new(),
        "path must be popped after commit"
    );
}

#[test]
fn interactive_pop_cancel_clears_state_without_mutating_path() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    push_and_clear(&controller, "a");
    let view = make_view(controller.clone());
    let mut state = NavigationStackViewState::default();

    let ticker: Arc<AnimationTicker> = Arc::new(AnimationTicker::new());
    let dirty: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});

    let from_path = controller.begin_interactive_pop().unwrap();
    let to_path: Vec<&'static str> = Vec::new();
    let mut anim = vexo::AnimationController::new(Duration::from_millis(350));
    anim.set_ticker(ticker.clone());
    anim.set_dirty_callback(dirty.clone());
    anim.animate_with(Box::new(SpringSimulation::new(
        SpringDescription::ios(340.0, 1.0),
        0.5,
        0.0,
        0.0,
    )));

    state.wire_for_testing(ticker, dirty);
    state.set_interactive_pop(vexo_uikit::InteractivePop {
        controller: anim,
        from_path,
        to_path,
        phase: vexo_uikit::InteractivePopPhase::Cancelling,
        velocity_tracker: VelocityTracker::new(),
    });

    let start = Instant::now();
    for i in 0..100u64 {
        let now = start + Duration::from_millis(20 * i);
        if let Some(ip) = state.interactive_pop_cell().borrow_mut().as_mut() {
            ip.controller.advance(now);
        }
    }

    let _w = render_stack(view, &mut state);

    assert!(
        state.interactive_pop_cell().borrow().is_none(),
        "interactive_pop cell must be cleared after cancel"
    );
    assert_eq!(
        controller.path(),
        vec!["a"],
        "path must be unchanged after cancel"
    );
}
