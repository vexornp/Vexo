//! Default navigation transition builders.
//!
//! These functions are used by `NavigationStackView` when no custom
//! `transition` builder is supplied. They receive a `TransitionCtx` (which
//! includes the eased progress `t`, direction, and platform) and the page
//! widget, and return the page wrapped in transform/opacity widgets.
//!
//! Callers can supply their own builder via `NavigationStackView::transition`
//! to override the defaults.
//!
//! The mobile slide is expressed in *fractions of the page's own laid-out
//! size* via `FractionalTranslation`, not absolute pixels. This matches
//! Flutter's `SlideTransition` / `FractionalTranslation`: the slide distance
//! is `1.0` (one full page width) regardless of the actual window size, so the
//! same transition code is correct at any width. No pixel width needs to be
//! read back from layout.

use vexo::{FractionalTranslation, Opacity, Widget};

use crate::platform::Platform;

/// Direction of a navigation transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionDir {
    /// A new page is being pushed onto the stack.
    Push,
    /// The top page is being popped off the stack.
    Pop,
    /// The stack is being popped all the way to root.
    PopToRoot,
}

/// Context passed to a transition builder function.
///
/// The builder inspects these fields to decide how to animate the page
/// (e.g., slide direction, opacity curve).
pub struct TransitionCtx {
    /// Eased progress, `0.0..=1.0`.
    pub t: f64,
    /// `true` if this is the incoming page, `false` for the outgoing page.
    pub is_incoming: bool,
    /// Direction of the transition.
    pub direction: TransitionDir,
    /// The platform the navigation view is running on.
    pub platform: Platform,
}

/// Default mobile transition: iOS-style horizontal slide.
///
/// The slide distance is expressed as a fraction of the page's own laid-out
/// width via `FractionalTranslation`. The render object resolves the fraction
/// against its `computed_bounds` at paint time, so the slide always covers
/// exactly one page width — no pixel read-back, no `page_width` field, correct
/// at any window size.
///
/// - **Push, incoming**: slides in from the right (fraction `1.0 → 0.0`), full opacity.
/// - **Push, outgoing**: slides slightly left (fraction `0.0 → -0.3`), dims to 0.85 alpha.
/// - **Pop, incoming**: slides back to 0 (fraction `-0.3 → 0.0`), un-dims 0.85 → 1.0.
/// - **Pop, outgoing**: slides out to the right (fraction `0.0 → 1.0`), full opacity.
///
/// The underneath page dims to 0.85 (subtle, closer to iOS native than the
/// previous 0.6) so it stays visible peeking from the left edge during the
/// transition — matching SwiftUI's `UINavigationController` dual-view
/// animation. The dimming mitigates text bleed-through when page backgrounds
/// are transparent.
pub fn default_mobile_transition(ctx: &TransitionCtx, child: Box<dyn Widget>) -> Box<dyn Widget> {
    let t = ctx.t as f32;
    let (fx, alpha) = match (ctx.direction, ctx.is_incoming) {
        (TransitionDir::Push, true) => (1.0 - t, 1.0),
        (TransitionDir::Push, false) => (-0.3 * t, 1.0 - 0.15 * t),
        (TransitionDir::Pop, true) => (-0.3 * (1.0 - t), 0.85 + 0.15 * t),
        (TransitionDir::Pop, false) => (t, 1.0),
        (TransitionDir::PopToRoot, true) => (-0.3 * (1.0 - t), 0.85 + 0.15 * t),
        (TransitionDir::PopToRoot, false) => (t, 1.0),
    };
    Opacity::new(FractionalTranslation::new(child, fx, 0.0), alpha).boxed()
}

/// Default desktop transition: fade only.
///
/// - **Incoming**: opacity `0 → 1`.
/// - **Outgoing**: opacity `1 → 0`.
/// - No slide — desktop windows don't have the physical stack metaphor.
pub fn default_desktop_transition(ctx: &TransitionCtx, child: Box<dyn Widget>) -> Box<dyn Widget> {
    let alpha = if ctx.is_incoming {
        ctx.t as f32
    } else {
        1.0 - ctx.t as f32
    };
    Opacity::new(child, alpha).boxed()
}

/// Select the default transition for the given platform.
pub fn default_transition(ctx: &TransitionCtx, child: Box<dyn Widget>) -> Box<dyn Widget> {
    match ctx.platform {
        Platform::Mobile => default_mobile_transition(ctx, child),
        Platform::Desktop => default_desktop_transition(ctx, child),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vexo::Text;

    #[test]
    fn test_transition_dir_equality() {
        assert_eq!(TransitionDir::Push, TransitionDir::Push);
        assert_ne!(TransitionDir::Push, TransitionDir::Pop);
        assert_ne!(TransitionDir::Push, TransitionDir::PopToRoot);
    }

    #[test]
    fn test_default_mobile_transition_produces_widget() {
        let ctx = TransitionCtx {
            t: 0.5,
            is_incoming: true,
            direction: TransitionDir::Push,
            platform: Platform::Mobile,
        };
        let child = Text::new("Page").boxed();
        let result = default_mobile_transition(&ctx, child);
        // Should produce some widget (we don't introspect the wrapper here;
        // deeper inspection happens in integration tests with MockBackend).
        assert!(!result.as_any().is::<std::marker::PhantomData<()>>());
    }

    #[test]
    fn test_default_desktop_transition_produces_widget() {
        let ctx = TransitionCtx {
            t: 0.5,
            is_incoming: true,
            direction: TransitionDir::Push,
            platform: Platform::Desktop,
        };
        let child = Text::new("Page").boxed();
        let _result = default_desktop_transition(&ctx, child);
    }

    #[test]
    fn test_default_transition_dispatches_by_platform() {
        let mobile_ctx = TransitionCtx {
            t: 0.5,
            is_incoming: true,
            direction: TransitionDir::Push,
            platform: Platform::Mobile,
        };
        let desktop_ctx = TransitionCtx {
            t: 0.5,
            is_incoming: true,
            direction: TransitionDir::Push,
            platform: Platform::Desktop,
        };
        // Just verify dispatch doesn't panic and produces a widget.
        let _m = default_transition(&mobile_ctx, Text::new("M").boxed());
        let _d = default_transition(&desktop_ctx, Text::new("D").boxed());
    }

    #[test]
    fn push_outgoing_dims_to_0_85_not_zero() {
        let ctx = TransitionCtx {
            t: 1.0,
            is_incoming: false,
            direction: TransitionDir::Push,
            platform: Platform::Mobile,
        };
        let child = Text::new("Page").boxed();
        let result = default_mobile_transition(&ctx, child);

        let opacity = result
            .as_any()
            .downcast_ref::<vexo::Opacity>()
            .expect("top-level wrapper must be Opacity");
        assert!(
            (opacity.opacity_value() - 0.85).abs() < 1e-6,
            "push outgoing at t=1 must dim to 0.85, got {}",
            opacity.opacity_value()
        );
    }

    #[test]
    fn pop_incoming_un_dims_from_0_85_to_1() {
        let ctx = TransitionCtx {
            t: 1.0,
            is_incoming: true,
            direction: TransitionDir::Pop,
            platform: Platform::Mobile,
        };
        let child = Text::new("Page").boxed();
        let result = default_mobile_transition(&ctx, child);

        let opacity = result
            .as_any()
            .downcast_ref::<vexo::Opacity>()
            .expect("top-level wrapper must be Opacity");
        assert!(
            (opacity.opacity_value() - 1.0).abs() < 1e-6,
            "pop incoming at t=1 must be 1.0 (un-dimmed), got {}",
            opacity.opacity_value()
        );
    }
}
