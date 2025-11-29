use shared_app::State;

fn main() -> anyhow::Result<()> {
    vexo::run_desktop_demo::<State>()
}

/*

### How to Run

1.  **Run Desktop:**
    ```bash
    cargo run -p desktop_demo
    ```
2.  **Build Mobile Library:**
    ```bash
    cargo build -p shared_app --target aarch64-apple-ios --release

*/
