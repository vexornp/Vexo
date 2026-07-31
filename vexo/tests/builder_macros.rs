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

#[test]
fn if_without_else_false_renders_nothing() {
    let cond = false;
    let w: MultiChild = column! {
        vexo::Text::new("always"),
        if cond { vexo::Text::new("maybe") },
    };
    assert_eq!(w.children().len(), 1);
}

#[test]
fn if_without_else_true_renders_one() {
    let cond = true;
    let w: MultiChild = column! {
        vexo::Text::new("always"),
        if cond { vexo::Text::new("maybe") },
    };
    assert_eq!(w.children().len(), 2);
}

#[test]
fn if_with_else_renders_exactly_one() {
    let w: MultiChild = column! {
        if true { vexo::Text::new("a") } else { vexo::Text::new("b") },
    };
    assert_eq!(w.children().len(), 1);
}

#[test]
fn if_with_else_false_takes_else_branch() {
    let w: MultiChild = column! {
        if false { vexo::Text::new("a") } else { vexo::Text::new("b") },
    };
    assert_eq!(w.children().len(), 1);
}

#[test]
fn for_loop_renders_all_iterations() {
    let items = vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
        "d".to_string(),
    ];
    let w: MultiChild = column! {
        for s in &items { vexo::Text::new(s) },
    };
    assert_eq!(w.children().len(), 4);
}

#[test]
fn for_loop_empty_renders_nothing() {
    let items: Vec<String> = vec![];
    let w: MultiChild = column! {
        for s in &items { vexo::Text::new(s) },
    };
    assert_eq!(w.children().len(), 0);
}

#[test]
fn for_loop_interleaved_with_plain() {
    let items = vec!["x".to_string(), "y".to_string()];
    let w: MultiChild = column! {
        vexo::Text::new("header"),
        for s in &items { vexo::Text::new(s) },
    };
    assert_eq!(w.children().len(), 3);
}
