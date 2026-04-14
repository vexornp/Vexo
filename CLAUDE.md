# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
# Desktop development
cargo run -p desktop_demo                    # Run desktop demo immediately
cargo build -p vexo --release                # Build framework alone for inspection

# iOS build (requires uniffi-bindgen-swift pre-built in target/debug/)
./build_for_ios.sh                           # Builds iOS lib + generates Swift bindings
```

## Architecture Overview

**Vexo** is a cross-platform UI framework written in Rust with support for desktop (winit) and iOS (Metal via wgpu). The workspace is organized into three crates:

- **`vexo/`**: Core graphics & UI framework (wgpu rendering, Taffy layout, glyphon text)
- **`shared_app/`**: Platform-agnostic application logic (exports via UniFFI to Swift on iOS)
- **`desktop_demo/`**: Desktop entry point that instantiates the shared app

### Critical Data Flows

1. **Application Trait** (`vexo/src/lib.rs:1594`): Apps implement a simple MEL architecture:
   - `Message` enum: All possible user interactions (e.g., `Message::Clicked`)
   - `State`: Persistent application state (updated by messages)
   - `update(state, message)`: Pure state transition function
   - `view(state)`: Renders state to a widget tree

2. **Rendering Pipeline**:
   - `Application::view()` → widget tree → `Widget::layout()` (Taffy) → `Widget::draw()` (UiBatcher) → wgpu render pass
   - Text is handled separately via glyphon: positioned by Taffy, rendered after geometry via `TextRenderer`

3. **Widget System**:
   - All widgets implement `Widget<M>` trait (`vexo/src/lib.rs:720`): `layout()`, `draw()`, `on_event()`
   - Container widgets (Row, Column) manage children; leaf widgets (Rectangle, Text) produce geometry
   - Widget focus tracked by `WidgetId`; input routed via `on_event()` returning `WidgetResponse<M>`

### Platform-Specific Initialization

- **Desktop** (`desktop_demo/src/main.rs`): `run_desktop_demo::<State>()` creates winit window, initializes `FrameworkState::new()`
- **iOS** (`shared_app/src/lib.rs`): `MobileApp::init_renderer()` receives Metal layer pointer, initializes `FrameworkState::new_with_ios()`, renders via polling

## Workspace Dependency Management

Central dependency versions defined in root `Cargo.toml` workspace section; all crates reference via `{ workspace = true }`. Critical versions:
- wgpu 27.0.1 (GPU backend)
- taffy 0.9.1 (layout engine)
- glyphon (git branch: main for text rendering)
- uniffi 0.30.0 (FFI bindings for iOS)

## Project-Specific Patterns

### Defining New Applications
Implement the `Application` trait in `shared_app/src/lib.rs`:
```rust
pub enum MyMessage { Clicked, TextChanged(String), }
pub struct MyState { /* ... */ }

impl Application for MyState {
    type Message = MyMessage;
    type State = Self;
    fn new() -> Self { /* initialize state */ }
    fn update(state: &mut Self, msg: Self::Message) { /* pure state transition */ }
    fn view(state: &Self) -> Box<dyn Widget<Self::Message>> {
        Box::new(Column::new()
            .push(Button::new(/* ... */, MyMessage::Clicked)))
    }
}
```

### Creating Custom Widgets
Implement `Widget<M>` in `vexo/src/lib.rs`. Required methods:
- `layout()`: Register node with Taffy, return NodeId
- `draw()`: Emit geometry to UiBatcher or text to glyphon
- `on_event()`: Handle input, return `WidgetResponse<M>` (message or none)
- `id()`: Return unique `WidgetId` for focus tracking

### Font System & Text Rendering
- FontSystem initialized once per FrameworkState (expensive disk scan on init)
- Embedded `font.ttf` bundled via `include_bytes!()` ensures iOS compatibility (no file IO)
- Text positioned by Taffy; glyphon renders post-geometry via viewport mapping to physical pixels
- Scale factor (`window.scale_factor()`) converts logical to physical pixels

### UniFFI Integration (iOS Export)
- `shared_app/src/lib.rs` defines `#[uniffi::Object]` struct `MobileApp` with `#[uniffi::export]` methods
- Methods `init_renderer()` and `render()` are callable from Swift
- Global static `GLOBAL_FS` holds the framework state (unsafe; consider refactoring with proper Arc/Mutex for production)

## Key File Locations

- Widget trait definition: `vexo/src/lib.rs:720`
- Application trait definition: `vexo/src/lib.rs:1594`
- FrameworkState initialization: `vexo/src/lib.rs:156` (`new()`, `new_with_ios()`)
- Render loop: `vexo/src/lib.rs:377` (`render()` method)
- Sample application: `shared_app/src/lib.rs` (State, Message, Application impl)
- iOS wrapper: `shared_app/src/lib.rs:73` (MobileApp struct, FFI exports)
- Build script: `build_for_ios.sh` (iOS artifacts pipeline)
