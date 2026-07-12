//! Tests for navigation animation: pending-op state machine, transition
//! builder dispatch, and integration with the existing render path.
//!
//! Tests that require the full `ThreeTreePipeline` (to wire the animation
//! ticker via `on_mount`) are not included here — those are verified
//! manually via `cargo run -p desktop_demo`. The unit-testable surface is:
//!
//! 1. `NavigationController` pending-op recording on push/pop/replace/pop_to_root.
//! 2. `PendingOp` snapshots (from/to/kind).
//! 3. `clear_pending` behavior.
//! 4. Default transition builder dispatch (covered in `transitions.rs`).
//! 5. Render fallback when no ticker is wired (steady-state hard swap).

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use vexo::{
    BuildOwner, DirtyTracking, ElementKey, RenderContext, RenderObjectRegistry, Text, Widget,
};
use vexo_uikit::{NavigationController, NavigationStackView};

use vexo_uikit::transitions::TransitionDir;

// ============================================================================
// HELPERS
// ============================================================================

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
    use vexo_uikit::Component;
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

// ============================================================================
// PENDING OP TESTS
// ============================================================================

#[test]
fn push_records_pending_op_with_correct_snapshots() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.push("b");

    let pending = controller.pending().expect("push must set pending op");
    assert_eq!(pending.from, vec!["a"], "from must be path before push");
    assert_eq!(pending.to, vec!["a", "b"], "to must be path after push");
    assert_eq!(pending.kind, TransitionDir::Push);
}

#[test]
fn pop_records_pending_op_with_correct_snapshots() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.clear_pending(); // clear the push's pending
    controller.push("b");
    controller.clear_pending(); // clear the second push's pending

    controller.pop();
    let pending = controller.pending().expect("pop must set pending op");
    assert_eq!(pending.from, vec!["a", "b"], "from must be path before pop");
    assert_eq!(pending.to, vec!["a"], "to must be path after pop");
    assert_eq!(pending.kind, TransitionDir::Pop);
}

#[test]
fn pop_at_root_does_not_set_pending() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.pop();
    assert!(
        controller.pending().is_none(),
        "pop at root must not set pending"
    );
}

#[test]
fn pop_to_root_records_pending_op() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.clear_pending();
    controller.push("b");
    controller.clear_pending();

    controller.pop_to_root();
    let pending = controller
        .pending()
        .expect("pop_to_root must set pending op");
    assert_eq!(pending.from, vec!["a", "b"]);
    assert!(pending.to.is_empty(), "to must be empty after pop_to_root");
    assert_eq!(pending.kind, TransitionDir::PopToRoot);
}

#[test]
fn pop_to_root_at_root_does_not_set_pending() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.pop_to_root();
    assert!(controller.pending().is_none());
}

#[test]
fn replace_records_pending_op_as_push() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.clear_pending();

    controller.replace("b");
    let pending = controller.pending().expect("replace must set pending op");
    assert_eq!(pending.from, vec!["a"]);
    assert_eq!(pending.to, vec!["b"]);
    assert_eq!(
        pending.kind,
        TransitionDir::Push,
        "replace animates as push"
    );
}

#[test]
fn replace_at_root_records_pending_op_as_push() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.replace("only");
    let pending = controller
        .pending()
        .expect("replace at root must set pending op");
    assert!(pending.from.is_empty());
    assert_eq!(pending.to, vec!["only"]);
    assert_eq!(pending.kind, TransitionDir::Push);
}

#[test]
fn clear_pending_drops_the_pending_op() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    assert!(controller.pending().is_some());
    controller.clear_pending();
    assert!(controller.pending().is_none());
}

#[test]
fn path_is_mutated_immediately_despite_pending() {
    // The controller mutates path immediately (so path() reflects the new top)
    // AND records a pending op (so the view knows a transition is in flight).
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    assert_eq!(
        controller.path(),
        vec!["a"],
        "path must reflect push immediately"
    );
    assert!(controller.pending().is_some(), "pending must also be set");
}

// ============================================================================
// PENDING OP + DIRTY CALLBACK
// ============================================================================

#[test]
fn push_fires_dirty_callback_and_sets_pending() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    controller.push("a");
    assert_eq!(counter.load(Ordering::SeqCst), 1, "push must fire dirty");
    assert!(controller.pending().is_some(), "push must set pending");
}

#[test]
fn pop_fires_dirty_callback_and_sets_pending() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");
    controller.clear_pending();

    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    controller.set_dirty_callback(Arc::new(move || {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    controller.pop();
    assert_eq!(counter.load(Ordering::SeqCst), 1, "pop must fire dirty");
    assert!(controller.pending().is_some(), "pop must set pending");
}

// ============================================================================
// RENDER FALLBACK (NO TICKER WIRED)
// ============================================================================

#[test]
fn render_without_ticker_clears_pending_and_shows_steady_state() {
    // When on_mount hasn't been called (no ticker cached in state), render
    // must clear the pending op and fall through to steady-state IndexedStack
    // rendering. This is the "hard swap" fallback used in unit tests.
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("detail");

    let view = NavigationStackView::new(controller.clone(), Text::new("Root"))
        .title(|d| format!("Title-{}", d))
        .destination(|d| Text::new(format!("Body-{}", d)).boxed());
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();

    let tree = render_stack(view, &mut state);
    let texts = all_text(tree.as_ref());

    // After render, pending should be cleared (fallback path).
    assert!(
        controller.pending().is_none(),
        "render without ticker must clear pending"
    );
    // The pushed page body should be visible (steady-state IndexedStack).
    assert!(
        texts.iter().any(|t| t == "Body-detail"),
        "steady-state must show pushed page, got: {:?}",
        texts
    );
}

#[test]
fn render_after_pop_without_ticker_shows_root() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("detail");

    let view = NavigationStackView::new(controller.clone(), Text::new("Root"))
        .destination(|d| Text::new(format!("Body-{}", d)).boxed());
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();

    // First render: push pending is cleared, steady-state shows pushed page.
    let _tree1 = render_stack(view.clone(), &mut state);

    // Pop and re-render: pop pending is cleared, steady-state shows root.
    controller.pop();
    let tree2 = render_stack(view, &mut state);
    let texts = all_text(tree2.as_ref());

    assert!(
        texts.iter().any(|t| t == "Root"),
        "after pop, root must be visible, got: {:?}",
        texts
    );
    assert!(
        !texts.iter().any(|t| t == "Body-detail"),
        "after pop, pushed body must NOT be visible, got: {:?}",
        texts
    );
}

// ============================================================================
// TRANSITION BUILDER API
// ============================================================================

#[test]
fn custom_transition_builder_is_invoked() {
    use vexo_uikit::TransitionCtx;

    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();

    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("detail");

    let view = NavigationStackView::new(controller.clone(), Text::new("Root"))
        .destination(|d| Text::new(format!("Body-{}", d)).boxed())
        .transition(move |_ctx: &TransitionCtx, child: Box<dyn Widget>| {
            c.fetch_add(1, Ordering::SeqCst);
            child
        });
    let mut state = vexo_uikit::NavigationStackViewState::<&'static str>::default();

    // Without a ticker, the transition won't actually run, but the builder
    // should NOT be invoked (we fall through to steady-state). So the counter
    // should remain 0.
    let _tree = render_stack(view, &mut state);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        0,
        "transition builder must NOT be invoked without a ticker (fallback path)"
    );
}

#[test]
fn transition_duration_builder_compiles() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root"))
        .transition_duration(std::time::Duration::from_millis(500));
    let _ = view;
}

#[test]
fn transition_curve_builder_compiles() {
    use vexo::EaseInCurve;
    let controller: NavigationController<&'static str> = NavigationController::new();
    let view = NavigationStackView::new(controller, Text::new("Root"))
        .transition_curve(Box::new(EaseInCurve));
    let _ = view;
}

// ============================================================================
// CLONE SEMANTICS
// ============================================================================

#[test]
fn controller_clone_shares_pending() {
    let controller: NavigationController<&'static str> = NavigationController::new();
    controller.push("a");

    let clone = controller.clone();
    let pending = clone.pending();
    assert!(pending.is_some(), "clone must share pending storage");

    // Clear via the clone; the original must also see it cleared.
    clone.clear_pending();
    assert!(
        controller.pending().is_none(),
        "clear_pending via clone must clear on original"
    );
}

// ============================================================================
// BASE FX / ALPHA (DUAL-VIEW OFFSET ANIMATION)
// ============================================================================

mod base_fx_alpha_tests {
    use vexo_uikit::base_fx_alpha;
    use vexo_uikit::platform::Platform;
    use vexo_uikit::transitions::TransitionDir;

    #[test]
    fn push_mobile_slides_left_and_dims() {
        // t=0: in place, full opacity
        let (fx, alpha) = base_fx_alpha(TransitionDir::Push, Platform::Mobile, 0.0);
        assert!((fx - 0.0).abs() < 1e-6, "fx at t=0 must be 0, got {}", fx);
        assert!(
            (alpha - 1.0).abs() < 1e-6,
            "alpha at t=0 must be 1.0, got {}",
            alpha
        );

        // t=0.5: slid 15% left, dimmed to 0.8
        let (fx, alpha) = base_fx_alpha(TransitionDir::Push, Platform::Mobile, 0.5);
        assert!(
            (fx - (-0.15)).abs() < 1e-6,
            "fx at t=0.5 must be -0.15, got {}",
            fx
        );
        assert!(
            (alpha - 0.8).abs() < 1e-6,
            "alpha at t=0.5 must be 0.8, got {}",
            alpha
        );

        // t=1.0: slid 30% left, dimmed to 0.6
        let (fx, alpha) = base_fx_alpha(TransitionDir::Push, Platform::Mobile, 1.0);
        assert!(
            (fx - (-0.3)).abs() < 1e-6,
            "fx at t=1.0 must be -0.3, got {}",
            fx
        );
        assert!(
            (alpha - 0.6).abs() < 1e-6,
            "alpha at t=1.0 must be 0.6, got {}",
            alpha
        );
    }

    #[test]
    fn pop_mobile_slides_back_and_un_dims() {
        // t=0: slid 30% left, dimmed to 0.6 (reverse of push end)
        let (fx, alpha) = base_fx_alpha(TransitionDir::Pop, Platform::Mobile, 0.0);
        assert!(
            (fx - (-0.3)).abs() < 1e-6,
            "fx at t=0 must be -0.3, got {}",
            fx
        );
        assert!(
            (alpha - 0.6).abs() < 1e-6,
            "alpha at t=0 must be 0.6, got {}",
            alpha
        );

        // t=1.0: in place, full opacity
        let (fx, alpha) = base_fx_alpha(TransitionDir::Pop, Platform::Mobile, 1.0);
        assert!((fx - 0.0).abs() < 1e-6, "fx at t=1.0 must be 0, got {}", fx);
        assert!(
            (alpha - 1.0).abs() < 1e-6,
            "alpha at t=1.0 must be 1.0, got {}",
            alpha
        );
    }

    #[test]
    fn pop_to_root_mobile_matches_pop() {
        let pop = base_fx_alpha(TransitionDir::Pop, Platform::Mobile, 0.3);
        let pop_to_root = base_fx_alpha(TransitionDir::PopToRoot, Platform::Mobile, 0.3);
        assert!((pop.0 - pop_to_root.0).abs() < 1e-6);
        assert!((pop.1 - pop_to_root.1).abs() < 1e-6);
    }

    #[test]
    fn push_desktop_no_offset_fade_only() {
        let (fx, alpha) = base_fx_alpha(TransitionDir::Push, Platform::Desktop, 0.5);
        assert!(
            (fx - 0.0).abs() < 1e-6,
            "desktop must have no offset, got {}",
            fx
        );
        assert!(
            (alpha - 0.5).abs() < 1e-6,
            "desktop alpha at t=0.5 must be 0.5, got {}",
            alpha
        );
    }

    #[test]
    fn pop_desktop_no_offset_fade_only() {
        let (fx, alpha) = base_fx_alpha(TransitionDir::Pop, Platform::Desktop, 0.5);
        assert!(
            (fx - 0.0).abs() < 1e-6,
            "desktop must have no offset, got {}",
            fx
        );
        assert!(
            (alpha - 0.5).abs() < 1e-6,
            "desktop alpha at t=0.5 must be 0.5, got {}",
            alpha
        );
    }
}
