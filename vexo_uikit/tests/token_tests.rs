use vexo_uikit::Color;

#[test]
fn button_primary_bg_is_blue() {
    let bg = vexo_uikit::theme::tokens::button::PRIMARY_BG;
    assert_eq!(bg, Color::rgb(0.0, 0.478, 1.0));
}

#[test]
fn button_disabled_alpha_is_half() {
    let alpha = vexo_uikit::theme::tokens::button::DISABLED_ALPHA;
    assert!((alpha - 0.5).abs() < 0.01);
}
