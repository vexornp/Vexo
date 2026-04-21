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

### Layered Architecture

The framework follows Clean Architecture principles with clear separation of concerns:

```
┌─────────────────────────────────────────────────────────────────┐
│                    PRESENTATION LAYER                           │
│  Application trait, run_desktop_demo()                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    WIDGET LAYER (Domain)                        │
│  Separated traits: Identifiable, Layout, Paint, Interact       │
│  Widget primitives: Text, Button, Container, TextEdit          │
│  Widget combinators: Modifiers (Padding, Background, Border)   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    STATE MANAGEMENT LAYER                       │
│  WidgetStateRegistry: editor state, focus state                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    LAYOUT LAYER                                 │
│  LayoutEngine trait (abstraction)                              │
│  TaffyLayoutEngine (implementation)                            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    RENDERING LAYER                              │
│  RenderCommand enum (immutable draw instructions)              │
│  RenderBackend trait (wgpu, mock for testing)                  │
│  WgpuBackend (production GPU rendering)                        │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    INFRASTRUCTURE LAYER                         │
│  WgpuBackend: GPU rendering, glyphon integration               │
└─────────────────────────────────────────────────────────────────┘
```

### Module Structure

```
vexo/src/
├── lib.rs                      # Public API, Application trait
├── core/                       # Core domain types
│   ├── mod.rs
│   ├── id.rs                   # WidgetId
│   ├── geometry.rs             # Point, Size, Rect, Scale
│   └── color.rs                # Color
├── widget/                     # Separated widget traits
│   ├── mod.rs
│   ├── identifiable.rs         # Identifiable trait
│   ├── layout.rs               # Layout trait, LayoutConstraints
│   ├── paint.rs                # Paint trait, PaintContext
│   └── interact.rs             # Interact trait, InteractionContext
├── widgets/                    # Widget implementations
│   ├── mod.rs                  # Widget<M> trait (compatibility)
│   ├── text.rs
│   ├── button.rs
│   ├── text_edit.rs
│   ├── containers.rs           # Column, Row
│   └── modifiers.rs            # Padding, Background, Border, etc.
├── layout/                     # Layout engine abstraction
│   ├── mod.rs
│   ├── engine.rs               # LayoutEngine trait
│   ├── node.rs                 # LayoutNode, ComputedLayout
│   └── taffy_engine.rs         # TaffyLayoutEngine
├── render/                     # Rendering abstraction
│   ├── mod.rs
│   ├── command.rs              # RenderCommand enum
│   ├── backend.rs              # RenderBackend trait
│   ├── wgpu_backend.rs         # WgpuBackend
│   └── mock_backend.rs         # MockBackend for testing
├── state/                      # State management
│   ├── mod.rs
│   ├── registry.rs             # WidgetStateRegistry
│   ├── editor.rs               # EditorState
│   └── focus.rs                # FocusState
├── input/                      # Input abstraction
│   ├── mod.rs
│   └── event.rs                # InputEvent enum
└── utils.rs                    # Point, Size, Rect (legacy, use core/)
```

### Critical Data Flows

1. **Application Trait** (`vexo/src/lib.rs`): Apps implement a simple MEL architecture:
   - `Message` enum: All possible user interactions (e.g., `Message::Clicked`)
   - `State`: Persistent application state (updated by messages)
   - `update(state, message)`: Pure state transition function
   - `view(state)`: Renders state to a widget tree

2. **Rendering Pipeline**:
   - `Application::view()` → widget tree → `Widget::layout()` (Taffy) → `Widget::draw()` (UiBatcher) → `WgpuBackend.render()`
   - Text is handled separately via glyphon: positioned by Taffy, rendered after geometry via `TextRenderer`

3. **Widget System**:
   - All widgets implement `Widget<M>` trait: `layout()`, `draw()`, `on_event()`
   - **New**: Separated traits for SRP: `Identifiable`, `Layout`, `Paint`, `Interact`
   - Container widgets (Row, Column) manage children; leaf widgets (Rectangle, Text) produce geometry
   - Widget focus tracked by `WidgetId`; input routed via `on_event()` returning `WidgetResponse<M>`

4. **Input Event Flow**:
   - winit events → `InputEvent::from_winit()` → `Widget::on_event()` with `InputEvent`
   - Platform-independent input abstraction enables testing without winit

5. **State Management**:
   - `WidgetStateRegistry` centralizes editor state and focus state
   - `WidgetContext` contains the registry, font system, scale, and cursor position

### Platform-Specific Initialization

- **Desktop** (`desktop_demo/src/main.rs`): `run_desktop_demo::<State>()` creates winit window, initializes `WindowState::new()`
- **iOS** (`shared_app/src/lib.rs`): `MobileApp` exports methods via UniFFI for Swift integration

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
Implement `Widget<M>` in `vexo/src/widgets/`. Required methods:
- `layout()`: Register node with Taffy, return NodeId
- `draw()`: Emit geometry to UiBatcher or text to glyphon
- `on_event()`: Handle `InputEvent`, return `WidgetResponse<M>`
- `key()`: Return unique key for identity (optional)

**New - Separated Traits (SRP)**:
For better testability, implement the separated traits:
- `Identifiable`: `id()` - stable identity for focus/hover
- `Layout`: `constraints()`, `apply_layout()` - layout participation
- `Paint`: `paint()` - generate `RenderCommand`s
- `Interact<M>`: `on_event()` - handle `InputEvent`

### Input Events
Use the platform-independent `InputEvent` enum:
```rust
use vexo::input::{InputEvent, ButtonState, Key, NamedKey};

fn handle_event(event: &InputEvent) {
    match event {
        InputEvent::PointerButton { position, state, .. } => { /* ... */ }
        InputEvent::Keyboard { key, text, state, modifiers } => { /* ... */ }
        InputEvent::PointerMoved { position } => { /* ... */ }
        InputEvent::Scroll { delta } => { /* ... */ }
    }
}
```

### State Management
Use `WidgetStateRegistry` for persistent widget state:
```rust
// In WidgetContext
let editor = ctx.get_or_create_editor("my-editor", "initial text");
ctx.state_registry.request_focus(WidgetId::from_key("my-editor"));
```

### Render Backend
The `RenderBackend` trait enables testing without GPU:
```rust
use vexo::render::{RenderBackend, MockBackend};

let mut backend = MockBackend::new();
backend.prepare(&mut batcher, &mut font_system, config);
backend.render();
// Inspect captured commands
```

### Font System & Text Rendering
- FontSystem initialized once per WindowState (expensive disk scan on init)
- Embedded `font.ttf` bundled via `include_bytes!()` ensures iOS compatibility (no file IO)
- Text positioned by Taffy; glyphon renders post-geometry via viewport mapping to physical pixels
- Scale factor (`window.scale_factor()`) converts logical to physical pixels

### UniFFI Integration (iOS Export)
- `shared_app/src/lib.rs` defines `#[uniffi::Object]` struct `MobileApp` with `#[uniffi::export]` methods
- Methods callable from Swift for iOS integration

## Commit Guidelines

- Do not include "Co-Authored-By: Claude" or similar attribution strings in commit messages

## Key File Locations

- Widget trait definition: `vexo/src/widgets/mod.rs`
- Separated widget traits: `vexo/src/widget/`
- Application trait definition: `vexo/src/lib.rs`
- WindowState: `vexo/src/lib.rs`
- Render backend: `vexo/src/render/wgpu_backend.rs`
- Input events: `vexo/src/input/event.rs`
- State management: `vexo/src/state/registry.rs`
- Sample application: `shared_app/src/lib.rs`
- iOS wrapper: `shared_app/src/lib.rs`
- Build script: `build_for_ios.sh`
