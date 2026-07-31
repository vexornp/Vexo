#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/*_passes.rs");
    t.compile_fail("tests/ui/*_fails.rs");
}
