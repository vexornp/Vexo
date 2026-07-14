use shared_app::ImState;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Logger is already initialized in vexo::run_desktop_demo
    // Set RUST_LOG=debug environment variable to see retain mode partial update logs
    vexo::run_desktop_demo::<ImState>()
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
