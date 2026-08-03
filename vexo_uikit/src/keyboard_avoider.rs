//! `KeyboardAvoider` — a `Component` that applies the current keyboard inset
//! as bottom padding to its child.
//!
//! Isolates the `MediaQuery` dependency so the parent screen does NOT rebuild
//! on every keyboard animation frame. Only this small component is a
//! `MediaQuery` dependent; the child subtree is wrapped in `Shared` (Rc pointer
//! comparison) so its reconciliation is skipped when the child hasn't changed
//! (keyboard frames) but still runs when the parent passes genuinely new content
//! (e.g. switching conversations).
//!
//! ## When to use
//!
//! Any screen with a text input that should be pushed above the keyboard
//! (chat composers, search bars, forms). Wrap the screen's content in
//! `KeyboardAvoider::new(child)` and the framework handles the rest.
//!
//! ## What it does NOT do
//!
//! - Does not animate. The padding value is read from `MediaQuery` each
//!   frame; the keyboard animation itself is driven by the platform
//!   (iOS keyboard notifications → `keyboard_inset_source` → `MediaQuery`
//!   invalidation → this component re-renders with the new padding).
//! - Does not scroll. If the child content is taller than the area above
//!   the keyboard, the child is responsible for its own scrolling (e.g.,
//!   wrap in `ScrollView`).
//! - Does not resize on desktop. On desktop, `MediaQuery.view_insets.bottom`
//!   is always 0 (no software keyboard), so this component is a no-op.

use std::rc::Rc;

use vexo::{Component, Layout, MediaQuery, RenderContext, Shared, SimpleState, Widget, WithLayout};

/// Wraps a child subtree and applies `MediaQuery.view_insets.bottom` as bottom
/// padding, so the child sits above the software keyboard.
///
/// This component is the ONLY `MediaQuery` dependent in the subtree — the
/// parent screen does not rebuild on keyboard frames. The child is wrapped
/// in `Shared` (Rc pointer comparison) so reconciliation is O(1) on keyboard
/// frames (same Rc → skip) but still propagates when the parent passes new
/// content (different Rc → reconcile).
///
/// The wrapper fills its parent (`flex_grow(1.0)`, `flex_basis(0.0)`,
/// `min_height(0.0)`), so the child area is "everything above the keyboard".
#[derive(Clone)]
pub struct KeyboardAvoider {
    child: Rc<dyn Widget>,
}

impl KeyboardAvoider {
    /// Create a `KeyboardAvoider` wrapping `child`.
    ///
    /// The child is stored as `Rc<dyn Widget>` so that `render()` can hand
    /// `Shared::new(Rc::clone(&self.child))` the *same* `Rc` pointer across
    /// `rebuild_from_state` calls (keyboard frames — same `&self` → same
    /// `Rc` → `SharedElement` skips reconciliation). A fresh `Rc::new(child)`
    /// only runs here, in the constructor, which the parent only calls when
    /// building genuinely new content — so the resulting `KeyboardAvoider`
    /// has a new `Rc` pointer and `SharedElement` reconciles.
    ///
    /// This is the "widget struct as cache" pattern — see
    /// `docs/rebuild-skipping-patterns.md` § "Shared — compare the `Rc`
    /// pointer of an already-built child".
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Rc::new(child),
        }
    }
}

impl Component for KeyboardAvoider {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut SimpleState<()>, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let bottom = MediaQuery::of(ctx).view_insets.bottom;
        // `Rc::clone` (not `Rc::new`) so the pointer matches across
        // `rebuild_from_state` calls on the same widget instance (keyboard
        // frames → `SharedElement` skips), but differs when the parent
        // constructs a fresh `KeyboardAvoider` (new content → reconciles).
        let child = Rc::clone(&self.child);
        WithLayout::new(
            Shared::new(child),
            Layout::default()
                .flex_grow(1.0)
                .flex_basis(0.0)
                .min_height(0.0)
                .padding_each(0.0, 0.0, 0.0, bottom),
        )
        .boxed()
    }
}
