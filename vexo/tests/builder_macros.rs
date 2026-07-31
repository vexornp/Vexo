//! Behavioral tests for the `column!` / `row!` builder macros.
//!
//! These verify the *runtime* shape of the produced widget tree (child counts,
//! layout). Compile-pass/compile-fail cases live in `vexo_macros/tests/ui/`.

use vexo::widgets::{MultiChild, Widget};
use vexo::{column, row};

#[test]
fn column_produces_multichild_with_two_children() {
    let w: MultiChild = column! {
        vexo::Text::new("a"),
        vexo::Text::new("b"),
    };
    assert_eq!(w.children().len(), 2);
}

#[test]
fn row_produces_multichild_with_two_children() {
    let w: MultiChild = row! {
        vexo::Text::new("a"),
        vexo::Text::new("b"),
    };
    assert_eq!(w.children().len(), 2);
}

#[test]
fn empty_column_has_zero_children() {
    let w: MultiChild = column! {};
    assert_eq!(w.children().len(), 0);
}

#[test]
fn single_child_no_trailing_separator() {
    let w: MultiChild = column! { vexo::Text::new("only") };
    assert_eq!(w.children().len(), 1);
}

#[test]
fn trailing_comma_allowed() {
    let w: MultiChild = column! {
        vexo::Text::new("a"),
        vexo::Text::new("b"),
    };
    assert_eq!(w.children().len(), 2);
}

#[test]
fn semicolon_separators_match_comma() {
    let w_comma: MultiChild = column! { vexo::Text::new("a"), vexo::Text::new("b") };
    let w_semi: MultiChild = column! { vexo::Text::new("a"); vexo::Text::new("b") };
    assert_eq!(w_comma.children().len(), w_semi.children().len());
}

#[test]
fn nested_builders_produce_correct_child_count() {
    let w: MultiChild = column! {
        row! {
            vexo::Text::new("a"),
            vexo::Text::new("b"),
        },
        vexo::Text::new("c"),
    };
    assert_eq!(w.children().len(), 2);
    let inner = &w.children()[0];
    assert_eq!(inner.children().len(), 2);
}
