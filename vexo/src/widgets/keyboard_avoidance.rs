//! Keyboard avoidance widget — lifts its child above the iOS software keyboard.
//!
//! Reads [`KeyboardInsetSource`](crate::core::KeyboardInsetSource) live each
//! render; when the target changes, the widget's state starts an
//! [`AnimationController`] tween from the current animated inset to the new
//! target, synchronized to the keyboard's own duration/curve. Effective
//! bottom padding each frame is `max(safe_area.bottom, animated_inset)`:
//!
//! - Keyboard down → `safe_area.bottom` (clears home indicator).
//! - Keyboard up → `animated_inset` (keyboard subsumes home indicator).
//!
//! On desktop / Android the source stays at 0 and this widget is a transparent
//! pass-through. Top/left/right padding is always zero — notch/status-bar
//! avoidance is [`SafeArea`](crate::widgets::SafeArea)'s job; the two compose.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::animation::{
    AnimationController, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, LinearCurve,
};
use crate::core::{KeyboardCurve, KeyboardInsetSnapshot};
use crate::{
    Component, ComponentState, LifecycleContext, RenderContext, Widget, WidgetKey, WithLayout,
};

// ============================================================================
// KEYBOARD AVOIDANCE STATE
// ============================================================================

/// State for [`KeyboardAvoidance`]. Owns the inset tween.
pub struct KeyboardAvoidanceState {
    /// Current animated inset (logical px). Read by `render()` each frame.
    animated_inset: f32,
    /// Inset the current tween started from.
    from_inset: f32,
    /// Inset the current tween is animating toward.
    to_inset: f32,
    /// The animation controller (0..1 linear; curve applied in `advance`).
    controller: AnimationController,
    /// Last target snapshot observed from the source. Used to detect changes.
    last_seen: KeyboardInsetSnapshot,
    /// Boxed curve for the current tween. Replaced on each retarget.
    curve: Box<dyn Curve>,
    /// Ticker handle; set on mount so we can stop on unmount.
    /// (AnimationController registers with the ticker itself; we hold the
    /// ticker Arc so we can pass it to a fresh controller on retarget.)
    ticker: Option<Arc<crate::animation::AnimationTicker>>,
    /// Dirty callback; wired on mount so fresh controllers get it.
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Whether `render()` has run at least once. The first render after
    /// mount snaps to the current target (no tween); subsequent renders
    /// detect target changes and start tweens.
    mounted: bool,
}

impl Default for KeyboardAvoidanceState {
    fn default() -> Self {
        Self {
            animated_inset: 0.0,
            from_inset: 0.0,
            to_inset: 0.0,
            controller: AnimationController::new(Duration::ZERO),
            last_seen: KeyboardInsetSnapshot {
                target_height: 0.0,
                duration_secs: 0.0,
                curve: KeyboardCurve::EaseInOut,
                animation_start: None,
            },
            curve: Box::new(EaseInOutCurve),
            ticker: None,
            dirty_callback: None,
            mounted: false,
        }
    }
}

impl KeyboardAvoidanceState {
    /// Effective bottom padding: `max(safe_area.bottom, animated_inset)`.
    pub fn effective_bottom_padding(&self, safe_area_bottom: f32) -> f32 {
        self.animated_inset.max(safe_area_bottom)
    }

    /// Start (or retarget) a tween to `target.target_height`.
    ///
    /// - If `duration_secs == 0.0`, snap immediately (set `animated_inset = target`).
    /// - Otherwise, start a fresh `AnimationController` from 0..1; `from_inset`
    ///   is the current `animated_inset` so mid-tween retargets don't jump.
    ///
    /// When `target.animation_start` is `Some` (the iOS shim captures the
    /// instant `keyboardWillShow/Hide` fired), the tween's `start_time` is
    /// seeded to that instant and the controller is advanced immediately. This
    /// synchronizes the avoidance tween with the OS keyboard's own animation:
    /// the keyboard begins sliding the moment the notification is posted, but
    /// this widget only renders on the next frame, so defaulting `start_time`
    /// to `Instant::now()` would leave the input view ~one frame behind for
    /// the whole animation. Seeding + advancing makes the first painted frame
    /// already reflect the elapsed time, so the input tracks the keyboard in
    /// lockstep. When `animation_start` is `None` (snap path, non-iOS, tests),
    /// the controller falls back to `Instant::now()`.
    pub fn start_tween_to(&mut self, target_height: f32, target: KeyboardInsetSnapshot) {
        self.from_inset = self.animated_inset;
        self.to_inset = target_height;
        self.curve = curve_for(target.curve);

        if target.duration_secs <= 0.0 {
            // Snap path: no animation, jump to target.
            self.animated_inset = target_height;
            self.controller = AnimationController::new(Duration::ZERO);
            self.controller.stop();
            self.last_seen = target;
            return;
        }

        // Build a fresh controller with the new duration. Re-attach the
        // ticker + dirty callback if we have them (set on mount).
        // Stop the outgoing controller first so its ticker callback is
        // unregistered — otherwise the old callback (which just sends this
        // element's id down the dirty channel) leaks into the ticker's
        // active list forever, keeping has_active()==true and driving
        // perpetual frame requests / rebuilds even after the tween settles.
        self.controller.stop();
        let mut controller =
            AnimationController::new(Duration::from_secs_f64(target.duration_secs as f64));
        if let Some(ticker) = &self.ticker {
            controller.set_ticker(ticker.clone());
        }
        if let Some(cb) = &self.dirty_callback {
            controller.set_dirty_callback(cb.clone());
        }
        let synced_start = target.animation_start;
        match synced_start {
            Some(start) => controller.forward_with_start(start),
            None => controller.forward(), // value 0 → 1 over duration
        }
        self.controller = controller;
        self.last_seen = target;

        // Synced path only: advance immediately so the *first* rendered frame
        // reflects the time already elapsed since the OS keyboard animation
        // began. Without this, `animated_inset` would still be `from_inset`
        // on this frame (the controller only updates it via `advance`, which
        // next runs in `on_tick` next frame), lagging the keyboard by a frame.
        // `advance` clamps to `to_inset` and stops the controller if the tween
        // is already complete (e.g. the render ran after the keyboard finished).
        if synced_start.is_some() {
            self.advance(Instant::now());
        }
    }

    /// Advance the tween. Called from `on_tick`.
    pub fn advance(&mut self, now: Instant) {
        self.controller.advance(now);
        let t = self.controller.value();
        let eased = self.curve.transform(t);
        self.animated_inset = self.from_inset + (self.to_inset - self.from_inset) * eased as f32;
    }
}

/// Map a `KeyboardCurve` to a Vexo `Curve` implementation.
pub fn curve_for(curve: KeyboardCurve) -> Box<dyn Curve> {
    match curve {
        KeyboardCurve::EaseInOut => Box::new(EaseInOutCurve),
        KeyboardCurve::EaseIn => Box::new(EaseInCurve),
        KeyboardCurve::EaseOut => Box::new(EaseOutCurve),
        KeyboardCurve::Linear => Box::new(LinearCurve),
    }
}

impl ComponentState for KeyboardAvoidanceState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        self.ticker = Some(ctx.animation_ticker().clone());
        self.dirty_callback = Some(ctx.dirty_callback());
        // The initial snap happens on the first render() call, which reads
        // the source via RenderContext::keyboard_inset(). On mount the
        // animated_inset stays at its default (0.0); if the keyboard is
        // already up, the first render will snap to the current target via
        // the `mounted` flag (see render()).
    }

    fn on_tick(&mut self, now: Instant) {
        self.advance(now);
    }

    fn on_unmount(&mut self, _ctx: &mut LifecycleContext) {
        self.controller.stop();
    }
}

// ============================================================================
// KEYBOARD AVOIDANCE WIDGET
// ============================================================================

/// A widget that lifts its child above the iOS software keyboard.
///
/// Wraps `child` in a column whose bottom padding is
/// `max(safe_area.bottom, animated_keyboard_inset)`. When the keyboard
/// appears, the padding animates in sync with the OS keyboard slide (using
/// the duration + curve reported by UIKit).
///
/// On desktop / Android the source stays at 0, so this is a transparent
/// pass-through. Only the bottom edge is padded; for notch/status-bar
/// avoidance, compose with [`SafeArea`](crate::widgets::SafeArea).
pub struct KeyboardAvoidance {
    child: Box<dyn Widget>,
    key: Option<WidgetKey>,
}

impl KeyboardAvoidance {
    /// Create a new `KeyboardAvoidance` wrapping `child`.
    ///
    /// The keyboard-inset source is read live each render via
    /// [`RenderContext::keyboard_inset()`], which reflects the app-wide
    /// source plumbed through `BuildOwner` / `WindowState` (all-zero on
    /// desktop / Android). For tests, drive the source through the
    /// pipeline's `set_keyboard_inset_source(...)`.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            key: None,
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Clone for KeyboardAvoidance {
    fn clone(&self) -> Self {
        Self {
            child: self.child.clone_boxed(),
            key: self.key.clone(),
        }
    }
}

impl Component for KeyboardAvoidance {
    type State = KeyboardAvoidanceState;

    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        // 1. Read live source snapshot from the app-wide source plumbed
        //    through BuildOwner / RenderContext (matches how SafeArea reads
        //    safe_area()).
        let snap = ctx.keyboard_inset();

        // 2. First render after mount: snap to the current target (no
        //    tween). The keyboard is already in whatever state it's in;
        //    animating from 0 would be wrong. Subsequent target changes
        //    start a tween synchronized to the keyboard's own duration/curve.
        if !state.mounted {
            state.animated_inset = snap.target_height;
            state.from_inset = snap.target_height;
            state.to_inset = snap.target_height;
            state.last_seen = snap;
            state.mounted = true;
        } else if snap != state.last_seen {
            state.start_tween_to(snap.target_height, snap);
        }

        // 3. Effective bottom padding = animated keyboard inset only.
        //    Safe-area avoidance (home indicator, notch) is `SafeArea`'s job —
        //    the two widgets compose. An ancestor like TabBarView already
        //    claims the bottom safe area, so reading ctx.safe_area() here
        //    would double-pad. When the keyboard is down, padding is 0
        //    (transparent pass-through).
        let bottom = state.animated_inset;

        // Build the layout: column with bottom padding, fills parent.
        let layout = crate::layout::Layout::default()
            .flex_direction(crate::layout::FlexDirection::Column)
            .align(crate::layout::AlignItems::Stretch)
            .flex_grow(1.0)
            .min_height(0.0)
            .padding_each(0.0, 0.0, 0.0, bottom);

        WithLayout::new(self.child.clone_boxed(), layout).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::AnimationTicker;
    use crate::ThreeTreePipeline;
    use std::sync::Arc;

    // ----- Helper: build a pipeline + render context with given sources -----
    fn build_pipeline() -> (ThreeTreePipeline, Arc<AnimationTicker>) {
        let ticker = Arc::new(AnimationTicker::new());
        let pipeline = ThreeTreePipeline::new(ticker.clone());
        (pipeline, ticker)
    }

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    // ----- Widget-level behavior tests (no pipeline; just exercise state) -----

    #[test]
    fn effective_padding_zero_when_no_keyboard_no_safe_area() {
        // Desktop: source = 0, safe area = 0 → padding 0.
        let mut state = KeyboardAvoidanceState::default();
        state.animated_inset = 0.0;
        let safe_bottom = 0.0;
        let pad = state.effective_bottom_padding(safe_bottom);
        assert_eq!(pad, 0.0);
    }

    #[test]
    fn effective_padding_uses_safe_area_when_keyboard_down() {
        let mut state = KeyboardAvoidanceState::default();
        state.animated_inset = 0.0;
        let safe_bottom = 34.0;
        let pad = state.effective_bottom_padding(safe_bottom);
        assert_eq!(pad, 34.0);
    }

    #[test]
    fn effective_padding_uses_keyboard_when_up() {
        let mut state = KeyboardAvoidanceState::default();
        state.animated_inset = 300.0;
        let safe_bottom = 34.0;
        let pad = state.effective_bottom_padding(safe_bottom);
        assert_eq!(pad, 300.0); // max(34, 300)
    }

    #[test]
    fn effective_padding_never_below_safe_area_during_slide() {
        // Mid-slide: animated_inset = 10 (below safe area 34).
        let mut state = KeyboardAvoidanceState::default();
        state.animated_inset = 10.0;
        let safe_bottom = 34.0;
        let pad = state.effective_bottom_padding(safe_bottom);
        assert_eq!(pad, 34.0); // max(34, 10)
    }

    #[test]
    fn start_tween_snaps_when_duration_zero() {
        // duration_secs == 0 → snap immediately, no animation.
        let mut state = KeyboardAvoidanceState::default();
        state.from_inset = 0.0;
        state.animated_inset = 0.0;
        state.controller = AnimationController::new(Duration::ZERO);
        state.start_tween_to(
            300.0,
            KeyboardInsetSnapshot {
                target_height: 300.0,
                duration_secs: 0.0,
                curve: KeyboardCurve::EaseInOut,
                animation_start: None,
            },
        );
        // Controller should have advanced to completion on the first advance()
        // call; but start_tween_to also sets animated_inset = to_inset when
        // duration is zero (snap path).
        assert_eq!(state.animated_inset, 300.0);
        assert_eq!(state.to_inset, 300.0);
    }

    #[test]
    fn advance_tween_interpolates_halfway() {
        let mut state = KeyboardAvoidanceState::default();
        state.from_inset = 0.0;
        state.animated_inset = 0.0;
        state.controller = AnimationController::new(Duration::from_millis(250));
        state.start_tween_to(
            300.0,
            KeyboardInsetSnapshot {
                target_height: 300.0,
                duration_secs: 0.25,
                curve: KeyboardCurve::Linear, // linear so halfway is exactly 150
                animation_start: None,
            },
        );
        let start = state.controller.start_time().unwrap();
        state.advance(start + Duration::from_millis(125));
        assert!(
            (state.animated_inset - 150.0).abs() < 1.0,
            "expected ~150 at halfway, got {}",
            state.animated_inset
        );
    }

    #[test]
    fn advance_tween_completes_at_target() {
        let mut state = KeyboardAvoidanceState::default();
        state.from_inset = 0.0;
        state.animated_inset = 0.0;
        state.controller = AnimationController::new(Duration::from_millis(250));
        state.start_tween_to(
            300.0,
            KeyboardInsetSnapshot {
                target_height: 300.0,
                duration_secs: 0.25,
                curve: KeyboardCurve::Linear,
                animation_start: None,
            },
        );
        let start = state.controller.start_time().unwrap();
        state.advance(start + Duration::from_millis(260));
        assert!(
            (state.animated_inset - 300.0).abs() < 0.5,
            "expected 300 at completion, got {}",
            state.animated_inset
        );
    }

    #[test]
    fn mid_tween_retarget_starts_from_current_animated_value() {
        let mut state = KeyboardAvoidanceState::default();
        state.from_inset = 0.0;
        state.animated_inset = 0.0;
        state.controller = AnimationController::new(Duration::from_millis(100));
        state.start_tween_to(
            300.0,
            KeyboardInsetSnapshot {
                target_height: 300.0,
                duration_secs: 0.1,
                curve: KeyboardCurve::Linear,
                animation_start: None,
            },
        );
        let start = state.controller.start_time().unwrap();
        state.advance(start + Duration::from_millis(25)); // 25% → 75
        assert!((state.animated_inset - 75.0).abs() < 1.0);

        // Retarget to 0 (keyboardWillHide) — new tween should start from 75.
        state.start_tween_to(
            0.0,
            KeyboardInsetSnapshot {
                target_height: 0.0,
                duration_secs: 0.1,
                curve: KeyboardCurve::Linear,
                animation_start: None,
            },
        );
        assert_eq!(
            state.from_inset, 75.0,
            "from_inset must be current animated value"
        );
        assert_eq!(state.to_inset, 0.0);
        let start2 = state.controller.start_time().unwrap();
        state.advance(start2 + Duration::from_millis(50)); // 50% of 0→75 reversed = 75-37.5
        assert!(
            (state.animated_inset - 37.5).abs() < 1.5,
            "expected ~37.5 halfway down from 75, got {}",
            state.animated_inset
        );
    }

    #[test]
    fn synced_animation_start_seeds_controller_and_advances_immediately() {
        // Regression: when the iOS shim reports the instant the keyboard
        // animation began (animation_start = Some(...)), the avoidance tween
        // must seed its controller's start_time to THAT instant — not
        // Instant::now() — and advance immediately so the first rendered
        // frame reflects the time already elapsed. Otherwise the input view
        // lags the keyboard by a frame for the whole animation: the keyboard
        // appears to cover the input, then the input catches up underneath.
        let mut state = KeyboardAvoidanceState::default();
        state.from_inset = 0.0;
        state.animated_inset = 0.0;
        state.controller = AnimationController::new(Duration::ZERO);

        // Pretend the keyboard notification fired 50ms ago (a few frames back).
        let notif_instant = Instant::now() - Duration::from_millis(50);

        state.start_tween_to(
            300.0,
            KeyboardInsetSnapshot {
                target_height: 300.0,
                duration_secs: 0.25,
                curve: KeyboardCurve::Linear,
                animation_start: Some(notif_instant),
            },
        );

        // start_time must be the notification instant, not Instant::now().
        // (Still Some => the tween hasn't completed, i.e. elapsed < duration.)
        assert_eq!(state.controller.start_time(), Some(notif_instant));

        // The immediate advance inside start_tween_to must have advanced
        // animated_inset to reflect ~50ms of a 250ms linear tween = 20% = 60px.
        // Allow slack for the time between capturing `notif_instant` and the
        // internal Instant::now() in advance — so the lower bound is 60, and
        // it must be well short of the 300 target (not completed).
        assert!(
            state.animated_inset >= 60.0 && state.animated_inset < 250.0,
            "expected animated_inset to reflect ~50ms elapsed (~60px, partway), \
             got {}; should already be partway — not 0 (no advance) and not 300 \
             (completed)",
            state.animated_inset
        );
        assert_eq!(state.from_inset, 0.0);
        assert_eq!(state.to_inset, 300.0);
    }

    #[test]
    fn synced_animation_start_already_complete_snaps_to_target() {
        // Edge case: if the render ran after the keyboard animation already
        // finished (elapsed >= duration), the immediate advance must clamp to
        // the target and stop the controller — no overshoot, no perpetual
        // ticking.
        let mut state = KeyboardAvoidanceState::default();
        state.from_inset = 0.0;
        state.animated_inset = 0.0;
        state.controller = AnimationController::new(Duration::ZERO);

        // Notification fired 500ms ago; duration is only 250ms — already done.
        let notif_instant = Instant::now() - Duration::from_millis(500);

        state.start_tween_to(
            300.0,
            KeyboardInsetSnapshot {
                target_height: 300.0,
                duration_secs: 0.25,
                curve: KeyboardCurve::Linear,
                animation_start: Some(notif_instant),
            },
        );

        assert!(
            (state.animated_inset - 300.0).abs() < 0.5,
            "expected snap to target (300) when elapsed > duration, got {}",
            state.animated_inset
        );
        assert!(state.controller.start_time().is_none());
    }

    #[test]
    fn curve_mapping_matches_uikit_raw_values() {
        assert_eq!(
            curve_for(KeyboardCurve::EaseInOut).transform(0.5),
            EaseInOutCurve.transform(0.5)
        );
        assert_eq!(
            curve_for(KeyboardCurve::EaseIn).transform(0.5),
            EaseInCurve.transform(0.5)
        );
        assert_eq!(
            curve_for(KeyboardCurve::EaseOut).transform(0.5),
            EaseOutCurve.transform(0.5)
        );
        assert_eq!(
            curve_for(KeyboardCurve::Linear).transform(0.5),
            LinearCurve.transform(0.5)
        );
    }

    // ----- Layout integration test (uses ThreeTreePipeline) -----

    #[test]
    fn widget_mounts_in_pipeline() {
        let (mut pipeline, _ticker) = build_pipeline();
        let view = KeyboardAvoidance::new(crate::Text::new("hi")).boxed();
        pipeline.update(view);
        assert!(
            pipeline.element_registry().len() > 2,
            "expected element tree to mount"
        );
    }

    #[test]
    fn layout_shrinks_child_by_keyboard_inset() {
        // Spec: a non-zero keyboard inset must shrink the child's computed
        // bounds by ~inset. We set the source to 300px (snap, no animation),
        // lay out at 400×800, and assert some render object in the tree has
        // computed-bounds height ~500 (800 − 300), within ±5px tolerance.
        // The flex_fill wrapper expands to fill the content area, so its
        // ContainerRenderObject is the one that reflects the inset.
        use crate::core::{Bounds, KeyboardCurve, KeyboardInsetSource, Logical, Size};
        use crate::layout::{Layout, TaffyLayoutEngine};
        use crate::widgets::WithLayout;
        use crate::RenderObjectKey;

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));

        // Plumb the source BEFORE update() so the widget's first render
        // (which snaps to the current target) sees 300px.
        let source = KeyboardInsetSource::default();
        source.set_target(300.0, 0.0, KeyboardCurve::EaseInOut, None);
        pipeline.set_keyboard_inset_source(source);

        // Wrap Text in a flex_fill WithLayout so the wrapper expands to fill
        // the available content area (800 − 300 = 500). A bare Text sizes
        // to its content (~29px) and wouldn't reflect the inset; the flex_fill
        // wrapper does.
        let child = WithLayout::new(crate::Text::new("hi"), Layout::flex_fill());
        let view = KeyboardAvoidance::new(child).boxed();
        pipeline.update(view);

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        pipeline.layout(Size::new(400.0, 800.0), &mut engine, &mut font_system);

        // Walk the render-object tree and collect every computed bounds.
        // We expect at least one RO whose height is ~500 (the flex_fill
        // wrapper filling the 800 − 300 content area).
        let ro_reg = pipeline.render_objects();
        let root = ro_reg.root().expect("root");

        fn collect_bounds(
            ro_reg: &crate::RenderObjectRegistry,
            id: RenderObjectKey,
            out: &mut Vec<Bounds<Logical>>,
        ) {
            if let Some(ro) = ro_reg.get(id) {
                if let Some(b) = ro.computed_bounds() {
                    out.push(b);
                }
                for &child in ro.children() {
                    collect_bounds(ro_reg, child, out);
                }
            }
        }

        let mut all_bounds = Vec::new();
        collect_bounds(ro_reg, root, &mut all_bounds);

        // Find any RO whose height is within ±5px of 500.
        let found = all_bounds
            .iter()
            .find(|b| (b.height() - 500.0).abs() <= 5.0);
        assert!(
            found.is_some(),
            "expected some render object with height ~500 (800 − 300 inset), \
             but no computed bounds matched. All bounds: {all_bounds:?}"
        );
    }
}
