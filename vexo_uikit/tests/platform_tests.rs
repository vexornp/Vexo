use vexo_uikit::Platform;

#[test]
fn platform_current_returns_a_variant() {
    let platform = Platform::current();
    match platform {
        Platform::Desktop | Platform::Mobile => {}
    }
}
