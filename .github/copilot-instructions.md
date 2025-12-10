# Vexo UI Platform - AI Coding Agent Instructions

## Architecture Overview

**Vexo** is a cross-platform UI framework written in Rust with support for desktop (winit) and iOS (Metal via wgpu). The workspace is organized into three key crates:

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
   - All widgets implement `Widget<M>` trait (lines 720-748): `layout()`, `draw()`, `on_event()`
   - Container widgets (Row, Column) manage children; leaf widgets (Rectangle, Text) produce geometry
   - Widget focus tracked by `WidgetId`; input routed via `on_event()` returning `WidgetResponse<M>`

### Platform-Specific Initialization

- **Desktop** (`desktop_demo/src/main.rs`): `run_desktop_demo::<State>()` creates winit window, initializes `FrameworkState::new()`
- **iOS** (`shared_app/src/lib.rs`): `MobileApp::init_renderer()` receives Metal layer pointer, initializes `FrameworkState::new_with_ios()`, renders via polling

## Workspace Dependency Management

Central dependency versions defined in `Cargo.toml` workspace section; all crates reference via `{ workspace = true }`. Critical versions:
- wgpu 27.0.1 (GPU backend)
- taffy 0.9.1 (layout engine)
- glyphon (git branch: main for text rendering)
- uniffi 0.30.0 (FFI bindings for iOS)

## Build & Testing Workflows

### Desktop Development
```bash
cargo run -p desktop_demo                    # Run immediately on desktop
cargo build -p vexo --release                # Build framework alone for inspection
```

### iOS Library & Bindings
```bash
# Shell script (build_for_ios.sh) orchestrates entire iOS pipeline:
./build_for_ios.sh
```
This script:
1. `cargo build --target aarch64-apple-ios --release` → `libshared_app.a`
2. Invokes `uniffi-bindgen-swift` (binary from `shared_app` build) → generates `shared_app.swift` + `shared_appFFI.h`
3. Copies artifacts to `VexoDemo/SharedApp/` for Xcode integration

**Important**: `uniffi-bindgen-swift` must be pre-built in `target/debug/` before running the script.

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

Example: `Rectangle::layout()` creates a leaf node with fixed dimensions; `draw()` emits quad geometry via `batcher.push_quad()`.

### Font System & Text Rendering
- FontSystem initialized once per FrameworkState (expensive disk scan on init)
- Embedded `font.ttf` bundled via `include_bytes!()` ensures iOS compatibility (no file IO)
- Text positioned by Taffy; glyphon renders post-geometry via viewport mapping to physical pixels
- Scale factor (`window.scale_factor()`) converts logical to physical pixels

### UniFFI Integration (iOS Export)
- `shared_app/src/lib.rs` defines `#[uniffi::Object]` struct `MobileApp` with `#[uniffi::export]` methods
- Methods `init_renderer()` and `render()` are callable from Swift
- Global static `GLOBAL_FS` holds the framework state (unsafe; consider refactoring with proper Arc/Mutex for production)

## Critical Files & Navigation

- **Widget trait definition**: `vexo/src/lib.rs:720`
- **Application trait definition**: `vexo/src/lib.rs:1594`
- **FrameworkState initialization**: `vexo/src/lib.rs:156` (`new()`, `new_with_ios()`)
- **Render loop**: `vexo/src/lib.rs:377` (`render()` method)
- **Sample application**: `shared_app/src/lib.rs` (State, Message, Application impl)
- **iOS wrapper**: `shared_app/src/lib.rs:73` (MobileApp struct, FFI exports)
- **Build script**: `build_for_ios.sh` (iOS artifacts pipeline)

## Common Tasks

**Add a new button widget**: Implement `Widget<M>`, add `on_event()` handler that returns `WidgetResponse::Message(MyMessage)`.

**Change layout (e.g., horizontal to vertical)**: Swap `Row` for `Column` in `Application::view()`.

**Fix text rendering**: Check scale factor conversion (logical vs physical pixels), font system initialization, and glyphon viewport bounds.

**Debug mobile initialization**: Verify Metal layer pointer validity in `init_renderer()`, ensure `GLOBAL_FS` is set before `render()` calls.

## Troubleshooting Rendering Issues

### Text Not Visible or Misaligned
**Cause**: Scale factor conversion error between logical and physical pixels.
- Verify `window.scale_factor()` is correctly passed to `FrameworkState::new()` or `new_with_ios()`
- Check that `batcher.set_screen_size()` receives logical dimensions (divide by scale_factor)
- Ensure glyphon `Viewport` is updated with physical pixel dimensions: `Resolution { width: config.width, height: config.height }`
- Confirm font size is in logical points (e.g., 24.0 means 24 logical points)

**Solution**: 
```rust
// Logical to physical conversion
let logical_width = self.config.width as f32 / self.scale_factor;
let logical_height = self.config.height as f32 / self.scale_factor;
self.batcher.set_screen_size(logical_width, logical_height);
```

### Widgets Rendering at Wrong Positions
**Cause**: Taffy layout not computed or offset accumulation in draw traversal.
- Verify `taffy.compute_layout()` is called after layout registration
- Check that `offset` parameter in `draw()` is correctly accumulated through parent containers
- Ensure `Rectangle::draw()` uses `taffy.layout()` to get positioned layout, not just stored dimensions

**Solution**: Container widgets (Row/Column) must accumulate offsets:
```rust
// In parent container's draw():
for (child_layout, child_widget) in children_with_layouts {
    let child_offset = (parent_offset.0 + child_layout.x, parent_offset.1 + child_layout.y);
    child_widget.draw(taffy, child_node, batcher, child_offset, ...);
}
```

### Blank/Black Screen or Render Crashes
**Cause**: GPU surface not configured or vertex/index buffers too small.
- Check `is_surface_configured` flag before rendering (must be true after first resize)
- Verify vertex buffer size (default 1MB) is sufficient: `UiBatcher::vertices.len()` should not exceed capacity
- Ensure `index_buffer` is large enough: watch for wgpu validation errors in logs

**Solution**: Monitor buffer sizes in debug builds:
```rust
if self.batcher.vertices.len() > 1024 * 1024 / std::mem::size_of::<Vertex>() {
    log::warn!("Vertex buffer may overflow!");
}
```

### Text Not Rendering (Only Geometry Visible)
**Cause**: FontSystem or glyphon initialization failed, or text not added to glyphon buffers.
- Verify `FontSystem::new()` completed (expensive disk scan; check logs for warnings)
- Confirm embedded `font.ttf` is present and loaded via `font_system.db_mut().load_font_data()`
- Check that `TextArea` is added to glyphon's `text_areas` before `text_renderer.prepare()`
- On iOS, never assume system fonts are available; always use embedded font

**Solution**:
```rust
// Ensure font is loaded once
let font_data = include_bytes!("../font.ttf").to_vec();
font_system.db_mut().load_font_data(font_data);

// Add text areas before rendering
let text_areas = vec![TextArea { ... }];
text_renderer.prepare(&device, &queue, &mut font_system, &mut atlas, &viewport, &text_areas, &mut swash_cache)?;
```

### Mobile (iOS) Rendering Stalls or Crashes
**Cause**: Metal layer pointer invalid, `GLOBAL_FS` not initialized, or render called before init.
- Verify Metal layer pointer passed to `init_renderer()` is valid (check Objective-C side in ViewController)
- Ensure `init_renderer()` completes before any `render()` calls
- Check that `GLOBAL_FS` is properly set: `unsafe { GLOBAL_FS = Some(fs); }`
- Confirm `render()` checks `if let Some(val) = rp` before dereferencing

**Solution**: Add defensive checks in `render()`:
```rust
pub fn render(&self) {
    unsafe {
        let rp = &mut *&raw mut GLOBAL_FS;
        if let Some(val) = rp {
            let _ = val.render();
        } else {
            eprintln!("Error: GLOBAL_FS not initialized. Call init_renderer first!");
        }
    }
}
```

### Layout Jittering or Flickering
**Cause**: Widget tree recreated every frame with different dimensions, causing Taffy re-layout.
- Check `Application::view()` doesn't conditionally change widget tree structure based on transient state
- Verify `root_node_id` is properly reset and cleared: `self.taffy.clear()` each frame
- Ensure widget dimensions are stable (not computed from intermediate state)

**Solution**: Keep `view()` pure and deterministic based only on `State`:
```rust
// Bad: dimension changes based on transient data
fn view(state: &Self) -> Box<dyn Widget<Message>> {
    let width = state.internal_buffer.len() as f32; // Unstable!
    Box::new(Rectangle::new(width, 100.0, [1.0, 0.0, 0.0]))
}

// Good: dimension tied to stable state
fn view(state: &Self) -> Box<dyn Widget<Message>> {
    Box::new(Rectangle::new(state.width, state.height, [1.0, 0.0, 0.0]))
}
```

### Memory Leaks or Performance Degradation
**Cause**: Repeated allocations in render loop (glyphon, Taffy clones, widget allocations).
- Avoid cloning large data structures in `view()` or `draw()`
- Don't create new `FontSystem` or `TextAtlas` per frame
- Reuse `WidgetContext` editors instead of creating new `Editor` instances
- Profile with `cargo flamegraph` on desktop to identify hot paths

**Solution**: Keep expensive objects as fields in `FrameworkState`, reuse in render loop:
```rust
pub struct FrameworkState<A> {
    font_system: glyphon::FontSystem, // Single instance
    atlas: glyphon::TextAtlas,         // Reused each frame
    widget_context: WidgetContext,     // Holds editor cache
    // ... other fields
}
```
