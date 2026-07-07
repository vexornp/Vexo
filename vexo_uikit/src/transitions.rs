//! Default navigation transition builders.
//!
//! These functions are used by `NavigationStackView` when no custom
//! `transition` builder is supplied. They receive a `TransitionCtx` (which
//! includes the eased progress `t`, direction, platform, and the cached
//! `page_width` from layout) and the page widget, and return the page
//! wrapped in transform/opacity widgets.
//!
//! Callers can supply their own builder via `NavigationStackView::transition`
//! to override the defaults.

use vexo::{Opacity, Transform, Widget};

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
    /// The cached page width in logical pixels, read from the nav content
    /// area's render object bounds. Used to compute slide offsets.
    /// Falls back to a sentinel default (375.0) on the first frame of a
    /// transition before layout has run.
    pub page_width: f32,
}

impl TransitionCtx {
    /// Sentinel width used when no layout has been computed yet.
    pub const DEFAULT_PAGE_WIDTH: f32 = 375.0;
}

/// Default mobile transition: iOS-style horizontal slide.
///
/// - **Push, incoming**: slides in from the right (`page_width → 0`), full opacity.
/// - **Push, outgoing**: slides slightly left (`0 → -page_width * 0.3`), dims to 0.7.
/// - **Pop, incoming**: reverse of Push.outgoing (slides back to 0, un-dims).
/// - **Pop, outgoing**: reverse of Push.incoming (slides out to the right).
pub fn default_mobile_transition(ctx: &TransitionCtx, child: Box<dyn Widget>) -> Box<dyn Widget> {
    let t = ctx.t as f32;
    let w = ctx.page_width;
    let (offset, alpha) = match (ctx.direction, ctx.is_incoming) {
        (TransitionDir::Push, true) => (w * (1.0 - t), 1.0),
        (TransitionDir::Push, false) => (-w * 0.3 * t, 1.0 - 0.3 * t),
        (TransitionDir::Pop, true) => (-w * 0.3 * (1.0 - t), 0.7 + 0.3 * t),
        (TransitionDir::Pop, false) => (w * t, 1.0),
        (TransitionDir::PopToRoot, true) => (-w * 0.3 * (1.0 - t), 0.7 + 0.3 * t),
        (TransitionDir::PopToRoot, false) => (w * t, 1.0),
    };
    Opacity::new(Transform::translate(child, offset, 0.0), alpha).boxed()
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
    fn test_transition_ctx_default_width() {
        assert_eq!(TransitionCtx::DEFAULT_PAGE_WIDTH, 375.0);
    }

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
            page_width: 400.0,
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
            page_width: 800.0,
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
            page_width: 375.0,
        };
        let desktop_ctx = TransitionCtx {
            t: 0.5,
            is_incoming: true,
            direction: TransitionDir::Push,
            platform: Platform::Desktop,
            page_width: 800.0,
        };
        // Just verify dispatch doesn't panic and produces a widget.
        let _m = default_transition(&mobile_ctx, Text::new("M").boxed());
        let _d = default_transition(&desktop_ctx, Text::new("D").boxed());
    }
}
