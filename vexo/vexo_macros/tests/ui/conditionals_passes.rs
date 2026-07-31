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

fn for_loop() {
    let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    column! {
        for s in &items {
            Text::new(s)
        },
    };
}

fn for_loop_interleaved() {
    let cond = true;
    let items = vec!["x".to_string()];
    column! {
        Text::new("header"),
        if cond { Text::new("cond") },
        for s in &items { Text::new(s) },
    };
}

fn match_expr() {
    #[derive(PartialEq)]
    enum S {
        Loading,
        Error,
    }
    let s = S::Loading;
    column! {
        match s {
            S::Loading => Text::new("loading"),
            S::Error => Text::new("error"),
        },
    };
}

fn match_with_block_body() {
    let n = 2;
    column! {
        match n {
            1 => { Text::new("one") },
            _ => {
                let s = "other";
                Text::new(s)
            },
        },
    };
}

fn main() {}
