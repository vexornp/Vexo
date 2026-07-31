use vexo::column;
use vexo::Text;

fn bad() {
    column! {
        let x = 42;
        Text::new("a");
    };
}

fn main() {}
