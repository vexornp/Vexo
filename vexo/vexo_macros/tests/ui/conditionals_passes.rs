use vexo::Text;
use vexo::{column, row};

fn if_without_else() {
    let cond = true;
    column! {
        Text::new("a"),
        if cond { Text::new("b") },
    };
}

fn if_with_else() {
    let cond = true;
    column! {
        if cond { Text::new("yes") } else { Text::new("no") },
    };
}

fn if_in_row() {
    let cond = false;
    row! {
        Text::new("a"),
        if cond { Text::new("b") } else { Text::new("c") },
    };
}

fn nested_if() {
    let a = true;
    let b = false;
    column! {
        if a {
            if b { Text::new("ab") } else { Text::new("a") }
        } else {
            Text::new("not-a")
        },
    };
}

fn main() {}
