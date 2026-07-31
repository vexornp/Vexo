// Compile-pass tests for `column!`/`row!` basic syntax.
// trybuild verifies these compile without error.

use vexo::Text;
use vexo::{column, row};

fn basic_column() {
    column! {
        Text::new("a"),
        Text::new("b"),
    };
}

fn basic_row() {
    row! { Text::new("a"), Text::new("b") };
}

fn semicolon_separators() {
    column! {
        Text::new("a");
        Text::new("b");
    };
}

fn trailing_comma() {
    column! { Text::new("a"), Text::new("b"), };
}

fn empty_block() {
    column! {};
}

fn nested() {
    column! {
        row! {
            Text::new("a"),
            Text::new("b"),
        },
        Text::new("c"),
    };
}

fn single_child_no_separator() {
    column! { Text::new("only") };
}

fn main() {}
