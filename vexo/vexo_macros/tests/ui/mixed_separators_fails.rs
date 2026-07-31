use vexo::column;
use vexo::Text;

fn bad() {
    column! {
        Text::new("a"),
        Text::new("b");
        Text::new("c"),
    };
}

fn main() {}
