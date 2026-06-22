use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::animation::{AnimationController, AnimationTicker, AnimationDirection, ColorTween, FloatTween, Tween};
use crate::core::Color;

#[test]
fn test_full_animation_flow() {
    let ticker = Arc::new(AnimationTicker::new());
    let dirty_count = Arc::new(AtomicUsize::new(0));
    let dirty_count_clone = dirty_count.clone();

    let mut ctrl = AnimationController::new(Duration::from_secs(1));
    ctrl.set_dirty_callback(Arc::new(move || {
        dirty_count_clone.fetch_add(1, Ordering::SeqCst);
    }));
    ctrl.set_ticker(ticker.clone());

    ctrl.forward();
    ticker.tick();
    assert!(dirty_count.load(Ordering::SeqCst) >= 1);

    let start = ctrl.start_time().unwrap();
    let now = start + Duration::from_millis(500);
    ctrl.advance(now);

    let color_tween = ColorTween::new(Color::RED, Color::BLUE);
    let color = color_tween.lerp(ctrl.value());
    assert!((color.r - 0.5).abs() < 0.01);
    assert!((color.b - 0.5).abs() < 0.01);
}

#[test]
fn test_float_tween_with_controller() {
    let mut ctrl = AnimationController::new(Duration::from_secs(1));
    ctrl.forward();

    let start = ctrl.start_time().unwrap();
    let now = start + Duration::from_millis(250);
    ctrl.advance(now);

    let float_tween = FloatTween::new(0.0, 100.0);
    let value = float_tween.lerp(ctrl.value());
    assert!((value - 25.0).abs() < 1.0);
}

#[test]
fn test_animation_completes_and_unregisters() {
    let ticker = Arc::new(AnimationTicker::new());
    let mut ctrl = AnimationController::new(Duration::from_millis(100));
    ctrl.set_dirty_callback(Arc::new(|| {}));
    ctrl.set_ticker(ticker.clone());
    ctrl.forward();

    assert!(ticker.has_active());

    // Advance past completion
    let start = ctrl.start_time().unwrap();
    ctrl.advance(start + Duration::from_millis(150));

    assert!(!ticker.has_active());
    assert_eq!(ctrl.direction(), AnimationDirection::Stopped);
    assert!((ctrl.value() - 1.0).abs() < 0.01);
}
