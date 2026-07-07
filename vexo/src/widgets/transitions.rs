//! Transition widgets — animated wrappers driven by an `AnimationController`.
//!
//! `SlideTransition` and `FadeTransition` are `Component`s whose `State` owns
//! an `AnimationController`. On mount, the controller is wired to the
//! per-frame ticker and started. Each frame, `on_tick` advances the controller
//! and `render` reads `controller.value()` to produce a `Transform` or
//! `Opacity` wrapper around the child.
//!
//! This matches Flutter's `SlideTransition` / `FadeTransition` design, with
//! one simplification: each transition owns its controller (the caller does
//! not pass one in). This avoids making `AnimationController` shared/cloneable
//! and is sufficient for navigation's push/pop model where each transition is
//! independent.
//!
//! # Example
//!
//! ```ignore
//! use vexo::{SlideTransition, FadeTransition, EaseInOutCurve};
//! use std::time::Duration;
//!
//! // Slide a child in from the right over 300ms
//! let slide = SlideTransition::horizontal(
//!     Text::new("Hello"),
//!     300.0,   // begin offset (logical px)
//!     0.0,     // end offset
//!     Duration::from_millis(300),
//! );
//!
//! // Fade a child in over 200ms
//! let fade = FadeTransition::new(
//!     Text::new("Hello"),
//!     0.0,   // begin opacity
//!     1.0,   // end opacity
//!     Duration::from_millis(200),
//! );
//! ```

use std::time::{Duration, Instant};

use crate::animation::{AnimationController, Curve, EaseInOutCurve};
use crate::stateful_widget::{Component, ComponentState, LifecycleContext, RenderContext};
use crate::widgets::opacity::Opacity;
use crate::widgets::transform::Transform;
use crate::widgets::Widget;
use crate::WidgetKey;

// ============================================================================
// SLIDE TRANSITION
// ============================================================================

/// Direction along which the slide offset is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideDirection {
    /// Offset applied on the X axis (horizontal slide).
    Horizontal,
    /// Offset applied on the Y axis (vertical slide).
    Vertical,
}

/// A widget that translates its child from `begin` to `end` offset as the
/// animation progresses from 0 to 1.
///
/// The transition owns its `AnimationController` and starts it on mount.
/// Each frame, `render` reads the (curve-eased) controller value and wraps
/// the child in `Transform::translate`.
pub struct SlideTransition {
    key: Option<WidgetKey>,
    direction: SlideDirection,
    begin: f32,
    end: f32,
    curve: Box<dyn Curve>,
    duration: Duration,
    child: Box<dyn Widget>,
}

impl SlideTransition {
    /// Create a horizontal slide transition.
    pub fn horizontal(
        child: impl Widget + 'static,
        begin: f32,
        end: f32,
        duration: Duration,
    ) -> Self {
        Self {
            key: None,
            direction: SlideDirection::Horizontal,
            begin,
            end,
            curve: Box::new(EaseInOutCurve),
            duration,
            child: Box::new(child),
        }
    }

    /// Create a vertical slide transition.
    pub fn vertical(
        child: impl Widget + 'static,
        begin: f32,
        end: f32,
        duration: Duration,
    ) -> Self {
        Self {
            key: None,
            direction: SlideDirection::Vertical,
            begin,
            end,
            curve: Box::new(EaseInOutCurve),
            duration,
            child: Box::new(child),
        }
    }

    /// Override the default `EaseInOutCurve`.
    pub fn curve(mut self, curve: Box<dyn Curve>) -> Self {
        self.curve = curve;
        self
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// The current direction.
    pub fn direction(&self) -> SlideDirection {
        self.direction
    }

    /// The begin offset.
    pub fn begin(&self) -> f32 {
        self.begin
    }

    /// The end offset.
    pub fn end(&self) -> f32 {
        self.end
    }

    /// The configured duration.
    pub fn duration(&self) -> Duration {
        self.duration
    }
}

impl Clone for SlideTransition {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            direction: self.direction,
            begin: self.begin,
            end: self.end,
            // Curves are stateless pure functions; cloning is not supported on
            // `Box<dyn Curve>` so we reconstruct a default EaseInOutCurve.
            // Callers that need a custom curve should rebuild the transition
            // rather than clone it.
            curve: Box::new(EaseInOutCurve),
            duration: self.duration,
            child: self.child.clone_boxed(),
        }
    }
}

/// State for `SlideTransition`. Owns the `AnimationController`.
pub struct SlideTransitionState {
    controller: AnimationController,
}

impl Default for SlideTransitionState {
    fn default() -> Self {
        // duration placeholder; replaced with the widget's duration in on_mount
        Self {
            controller: AnimationController::new(Duration::from_millis(300)),
        }
    }
}

impl ComponentState for SlideTransitionState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(widget) = ctx.widget().downcast_ref::<SlideTransition>() {
            // Reconstruct the controller with the widget's actual duration.
            self.controller = AnimationController::new(widget.duration);
            self.controller.set_ticker(ctx.animation_ticker().clone());
            self.controller.set_dirty_callback(ctx.dirty_callback());
            self.controller.forward();
        }
    }

    fn on_tick(&mut self, now: Instant) {
        self.controller.advance(now);
    }

    fn on_unmount(&mut self, _ctx: &mut LifecycleContext) {
        // Stop the controller to unregister from the ticker.
        self.controller.stop();
    }
}

impl Component for SlideTransition {
    type State = SlideTransitionState;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let t = state.controller.value();
        let eased = self.curve.transform(t);
        let offset = self.begin + (self.end - self.begin) * eased as f32;
        let (dx, dy) = match self.direction {
            SlideDirection::Horizontal => (offset, 0.0),
            SlideDirection::Vertical => (0.0, offset),
        };
        Transform::translate(self.child.clone_boxed(), dx, dy).boxed()
    }

    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }
}

// ============================================================================
// FADE TRANSITION
// ============================================================================

/// A widget that fades its child from `begin` to `end` opacity as the
/// animation progresses from 0 to 1.
///
/// The transition owns its `AnimationController` and starts it on mount.
/// Each frame, `render` reads the (curve-eased) controller value and wraps
/// the child in `Opacity`.
pub struct FadeTransition {
    key: Option<WidgetKey>,
    begin: f32,
    end: f32,
    curve: Box<dyn Curve>,
    duration: Duration,
    child: Box<dyn Widget>,
}

impl FadeTransition {
    /// Create a new fade transition.
    ///
    /// `begin` and `end` are opacity values in `[0.0, 1.0]`.
    pub fn new(child: impl Widget + 'static, begin: f32, end: f32, duration: Duration) -> Self {
        Self {
            key: None,
            begin: begin.clamp(0.0, 1.0),
            end: end.clamp(0.0, 1.0),
            curve: Box::new(EaseInOutCurve),
            duration,
            child: Box::new(child),
        }
    }

    /// Override the default `EaseInOutCurve`.
    pub fn curve(mut self, curve: Box<dyn Curve>) -> Self {
        self.curve = curve;
        self
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// The begin opacity.
    pub fn begin(&self) -> f32 {
        self.begin
    }

    /// The end opacity.
    pub fn end(&self) -> f32 {
        self.end
    }

    /// The configured duration.
    pub fn duration(&self) -> Duration {
        self.duration
    }
}

impl Clone for FadeTransition {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            begin: self.begin,
            end: self.end,
            curve: Box::new(EaseInOutCurve),
            duration: self.duration,
            child: self.child.clone_boxed(),
        }
    }
}

/// State for `FadeTransition`. Owns the `AnimationController`.
pub struct FadeTransitionState {
    controller: AnimationController,
}

impl Default for FadeTransitionState {
    fn default() -> Self {
        Self {
            controller: AnimationController::new(Duration::from_millis(300)),
        }
    }
}

impl ComponentState for FadeTransitionState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(widget) = ctx.widget().downcast_ref::<FadeTransition>() {
            self.controller = AnimationController::new(widget.duration);
            self.controller.set_ticker(ctx.animation_ticker().clone());
            self.controller.set_dirty_callback(ctx.dirty_callback());
            self.controller.forward();
        }
    }

    fn on_tick(&mut self, now: Instant) {
        self.controller.advance(now);
    }

    fn on_unmount(&mut self, _ctx: &mut LifecycleContext) {
        self.controller.stop();
    }
}

impl Component for FadeTransition {
    type State = FadeTransitionState;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let t = state.controller.value();
        let eased = self.curve.transform(t);
        let alpha = self.begin + (self.end - self.begin) * eased as f32;
        Opacity::new(self.child.clone_boxed(), alpha.clamp(0.0, 1.0)).boxed()
    }

    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Text;

    #[test]
    fn test_slide_transition_creation() {
        let t =
            SlideTransition::horizontal(Text::new("Hi"), 100.0, 0.0, Duration::from_millis(300));
        assert_eq!(t.direction(), SlideDirection::Horizontal);
        assert_eq!(t.begin(), 100.0);
        assert_eq!(t.end(), 0.0);
        assert_eq!(t.duration(), Duration::from_millis(300));
    }

    #[test]
    fn test_slide_transition_vertical() {
        let t = SlideTransition::vertical(Text::new("Hi"), 50.0, 0.0, Duration::from_millis(200));
        assert_eq!(t.direction(), SlideDirection::Vertical);
    }

    #[test]
    fn test_fade_transition_creation() {
        let t = FadeTransition::new(Text::new("Hi"), 0.0, 1.0, Duration::from_millis(200));
        assert_eq!(t.begin(), 0.0);
        assert_eq!(t.end(), 1.0);
        assert_eq!(t.duration(), Duration::from_millis(200));
    }

    #[test]
    fn test_fade_transition_clamps() {
        let t = FadeTransition::new(Text::new("Hi"), -0.5, 1.5, Duration::from_millis(200));
        assert_eq!(t.begin(), 0.0);
        assert_eq!(t.end(), 1.0);
    }

    #[test]
    fn test_slide_transition_clone_preserves_fields() {
        let t =
            SlideTransition::horizontal(Text::new("Hi"), 100.0, 0.0, Duration::from_millis(300))
                .with_key("slide");
        let cloned = t.clone();
        assert_eq!(cloned.direction(), SlideDirection::Horizontal);
        assert_eq!(cloned.begin(), 100.0);
        assert_eq!(cloned.end(), 0.0);
        assert_eq!(cloned.duration(), Duration::from_millis(300));
        assert!(cloned.key.is_some());
    }
}
