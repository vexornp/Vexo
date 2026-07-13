use vexo::{Color, ThemeData};
use vexo_uikit::theme::tokens::{button, navigation};

#[test]
fn button_primary_bg_maps_to_theme_primary() {
    let c = button::colors(&ThemeData::light());
    assert_eq!(c.primary_bg, ThemeData::light().primary);
}

#[test]
fn button_disabled_opacity_is_half() {
    assert!((button::DISABLED_OPACITY - 0.5).abs() < 0.01);
}

#[test]
fn navigation_sidebar_bg_maps_to_theme_surface() {
    let n = navigation::colors(&ThemeData::light());
    assert_eq!(n.sidebar_bg, ThemeData::light().surface);
}

#[test]
fn resolvers_differ_between_light_and_dark() {
    let l = button::colors(&ThemeData::light());
    let d = button::colors(&ThemeData::dark());
    // primary_bg is the same (brand blue in both), but destructive differs.
    assert_ne!(l.destructive_bg, d.destructive_bg);

    let ln = navigation::colors(&ThemeData::light());
    let dn = navigation::colors(&ThemeData::dark());
    assert_ne!(ln.sidebar_bg, dn.sidebar_bg);
    assert_ne!(ln.mobile_header_bg, dn.mobile_header_bg);
}
