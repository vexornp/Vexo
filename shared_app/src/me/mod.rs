//! Me tab: profile screen wired into a NavigationStackView.

pub(crate) mod profile_screen;

use vexo::{Signal, Widget};
use vexo_uikit::{NavigationController, NavigationStackView};

use crate::data::Profile;

pub(crate) fn build_me_tab(
    profile: &Profile,
    nav: NavigationController<()>,
    is_dark: Signal<bool>,
) -> Box<dyn Widget> {
    NavigationStackView::new(nav, profile_screen::build_profile_screen(profile, is_dark))
        .root_title("Me")
        .boxed()
}
