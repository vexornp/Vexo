//! Shadows tab: shadow showcase screen wired into a NavigationStackView.

pub(crate) mod shadow_showcase_screen;

use vexo::Widget;
use vexo_uikit::{NavigationController, NavigationStackView};

pub(crate) fn build_shadows_tab(nav: NavigationController<()>) -> Box<dyn Widget> {
    NavigationStackView::new(nav, shadow_showcase_screen::build_shadow_showcase_screen())
        .root_title("Shadows")
        .boxed()
}
