//! `ThemeToggle` — a sun/moon icon button that flips an `is_dark: Signal<bool>`.
//!
//! Reads `Theme::of(ctx)` for its own icon color so it re-themes with the
//! subtree. Shows the *target* mode (moon when light → tap goes dark;
//! sun when dark → tap goes light), matching common toggle affordances.

use vexo::{Component, RenderContext, SimpleState, Theme, Widget};
use vexo_fontawesome::{Icon, Icons};

/// A theme-toggle button bound to `is_dark`.
///
/// Render this anywhere inside a `Theme` ancestor. On tap it flips the
/// signal; the root `view()` re-runs (the signal lives on `ImState`),
/// swapping `ThemeData::light()` ↔ `dark()` and rebuilding dependents.
#[derive(Clone)]
pub(crate) struct ThemeToggle {
    is_dark: vexo::Signal<bool>,
}

impl ThemeToggle {
    pub(crate) fn new(is_dark: vexo::Signal<bool>) -> Self {
        Self { is_dark }
    }
}

impl Component for ThemeToggle {
    type State = SimpleState<()>;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let dark = self.is_dark.get();
        // Icon shows the TARGET mode: tap to go there.
        let icon = if dark { Icons::Sun } else { Icons::Moon };
        let is_dark = self.is_dark.clone();

        Icon::new(icon)
            .with_size(20.0)
            .with_color(theme.on_surface_variant)
            .boxed()
            .on_tap(move || {
                is_dark.set(!is_dark.get());
            })
    }
}
