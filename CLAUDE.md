# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) while working with code in this repository.

## First Principles Thinking Guide

When analyzing tasks, especially architecture or complex logic, adhere to these principles:

1. **Deconstruct Problems:** Break complex tasks into fundamental, irrefutable truths. Avoid analogies or "how we did it before".
2. **Challenge Assumptions:** Explicitly identify and question all assumptions. If a decision is based on convention, justify it or rebuild from scratch.
3. **Invert the Problem:** Identify what we want to avoid, then work backward to define the correct solution.
4. **Prefer Simplicity:** If a simpler, more efficient approach exists, state it before implementing.
5. **Ask "Why?":** Use the "5 Whys" approach to get to the root cause of issues, not just surface-level symptoms.
6. **Isolate Before Theorizing:** When a bug only appears in a complex widget tree, strip the tree to the minimum repro first (e.g., a single widget in a bare Column). This narrows the search space dramatically before forming hypotheses. "Works alone, breaks in a tree" immediately points at the surrounding flex chain, not the widget itself.

## Core Design Philosophy: Everything Is a Widget

Vexo's core design philosophy is **"Everything is a widget."** Any UI concern — visual, interactive, or behavioral — should first be expressed as a widget in the widget tree, not as global state, imperative calls, or framework-level special cases.

**When adding any new feature, ask first: can this be a widget?**

- **Composability over imperative APIs** — A `MouseRegion` widget that sets the cursor is better than a global `set_cursor()` call. A `GestureDetector` widget is better than an event listener registry.
- **Declarative over imperative** — Widgets describe *what* should happen; the framework handles *how*. Prefer `MouseRegion { cursor: Cursor::Pointer, child: ... }` over `on_mouse_enter(|| set_cursor(Pointer))`.
- **Scope over global** — Widgets naturally scope behavior to subtrees. Cursor, focus, gestures, tooltips, animations — all scoped by tree position, no global state needed.
- **Consistency** — The same mental model applies everywhere: padding is a widget, gestures are a widget, cursors are a widget, focus is a widget.

If a feature cannot be a widget (e.g., it affects infrastructure below the widget layer), justify why before implementing it imperatively.

## Think Before Coding

Don't assume. Don't hide confusion. Surface tradeoffs.

Before implementing:

- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## Build & Run Commands

```bash
# Desktop development
cargo run -p desktop_demo                    # Run desktop demo immediately
cargo build -p vexo --release                # Build framework alone for inspection

# iOS build
# First-time only: build the host-side bindgen binary (the [[bin]] of shared_app)
cargo build -p shared_app                        # Produces target/debug/uniffi-bindgen-swift
# Then either run the script manually, OR just build from Xcode (the VexoDemo
# scheme has a Build pre-action that runs the script automatically).
./build_for_ios.sh                               # Builds iOS lib + generates Swift bindings (location-independent)
```

## Architecture Overview

**Vexo** is a cross-platform UI framework written in Rust with support for desktop (winit) and iOS (Metal via wgpu). The workspace is organized into three crates:

- **`vexo/`**: Core graphics & UI framework (wgpu rendering, Taffy layout, glyphon text)
- **`shared_app/`**: Platform-agnostic application logic (exports via UniFFI to Swift on iOS)
- **`desktop_demo/`**: Desktop entry point that instantiates the shared app

### Three-Tree Architecture

Vexo uses Flutter's three-tree architecture for efficient UI updates:

1. **Widget tree** — immutable descriptions of UI (what to show)
2. **Element tree** — mutable lifecycle managers (manages state and children)
3. **Render object tree** — performs layout and painting (how to show it)

```
┌─────────────────────────────────────────────────────────────────┐
│                    PRESENTATION LAYER                           │
│  Application trait, run_desktop_demo()                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    WIDGET LAYER (Domain)                        │
│  Widget trait (build() → Element)                              │
│  Widget primitives: Text, TextEditContent, Column, Row         │
│  Widget combinators: DecoratedContainer, GestureDetector        │
│  Stateful widgets: Component, Signal                                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    ELEMENT LAYER                                │
│  Element trait, ElementRegistry                                │
│  LeafElement, ContainerElement, DecoratedContainerElement      │
│  StatefulElement, GestureDetectorElement                       │
│  update_child() reconciles children during rebuild             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    RENDER OBJECT LAYER                          │
│  RenderObject trait, RenderObjectRegistry                      │
│  TextRenderObject, TextEditRenderObject                        │
│  ContainerRenderObject, DecoratedContainerRenderObject         │
│  CursorBlinkState for text cursor animation                    │
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
│  ThreeTreePipeline (orchestrates element/render object trees)  │
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
├── lib.rs                      # Public API, Application trait, re-exports
├── core/                       # Core domain types
│   ├── mod.rs
│   ├── id.rs                   # WidgetId
│   ├── geometry.rs             # Point, Size, Rect, Scale
│   └── color.rs                # Color
├── widgets/                    # Widget implementations
│   ├── mod.rs                  # Widget trait definition
│   ├── container.rs            # Column, Row
│   ├── text.rs                 # Text
│   ├── text_edit.rs            # TextEdit, TextEditingController
│   ├── text_edit_content.rs    # TextEditContent
│   ├── decorated_container.rs  # DecoratedContainer
│   └── gesture_detector.rs     # GestureDetector
├── elements/                   # Element implementations
│   ├── mod.rs
│   ├── leaf.rs                 # LeafElement
│   ├── container.rs            # ContainerElement
│   └── render_object_element.rs
├── render_objects/             # RenderObject implementations
│   ├── mod.rs
│   ├── text.rs                 # TextRenderObject
│   ├── container.rs            # ContainerRenderObject
│   └── text_edit.rs            # TextEditRenderObject
├── focus/                      # Focus tree
│   ├── mod.rs
│   ├── node.rs                 # FocusNodeId, FocusNodeData
│   ├── manager.rs              # FocusManager
│   ├── attachment.rs           # FocusAttachment
│   └── widget.rs               # Focus widget
├── element.rs                  # Element trait, ElementRegistry
├── render_object.rs            # RenderObject trait, RenderObjectRegistry
├── element_context.rs          # ElementContext
├── event_context.rs            # EventContext
├── event_handler.rs            # EventHandler
├── pipeline.rs                 # ThreeTreePipeline
├── reconciler.rs               # Reconciler
├── build_owner.rs              # BuildOwner
├── stateful_widget.rs          # Component, ComponentState, RenderContext, LifecycleContext
├── element_state.rs            # StateStorage
├── layouter.rs                 # Layouter
├── painter.rs                  # Painter
├── hit_test.rs                 # HitTestResult
├── reconcile.rs                # Reconcilable
├── dirty.rs                    # DirtyTracking
├── key.rs                      # Key, GlobalKey, WidgetKey
├── id.rs                       # ElementKey, RenderObjectKey
├── style.rs                    # Style
├── update_result.rs            # UpdateResult
├── child_ops.rs                # ChildOp, ChildOps
├── global_key_registry.rs      # GlobalKeyRegistry
├── inherited_registry.rs       # InheritedRegistry, InheritedMap
├── inherited_widget.rs         # InheritedWidget trait, InheritedElement
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
├── input/                      # Input abstraction
│   ├── mod.rs
│   └── event.rs                # InputEvent enum
├── state/                      # State management
│   ├── mod.rs
│   └── cursor_blink.rs         # CursorBlinkState
├── reactive/                   # Reactive primitives (Signal)
│   └── mod.rs
├── editor.rs                   # Editor (wraps glyphon::Editor)
├── renderer.rs                 # UiBatcher, RenderPipeline
├── text_processor.rs           # Text rendering via glyphon
├── window.rs                   # WindowState, main event loop
├── app.rs                      # VexoApp
└── resource/                   # Embedded resources (fonts)
```

### Critical Data Flows

1. **Application Trait** (`vexo/src/lib.rs`): Apps implement a simple architecture:
   - `State`: Persistent application state
   - `new()`: Creates initial state
   - `view(state, font_system)`: Renders state to a widget tree

2. **Rendering Pipeline**:
   - `Application::view()` → widget tree → `Widget::create_element()` → element tree → `Element::mount()` → render object tree → `RenderObject::layout()` (Taffy) → `RenderObject::paint()` (RenderCommands) → `WgpuBackend.render()`
   - Text is handled separately via glyphon: positioned by Taffy, rendered after geometry via `TextRenderer`

3. **Element Child Management**:
   - All elements manage their own children through `update_child()`
   - `update_child()` handles all four cases: inflate, unmount, update, replace
   - This matches Flutter's design where `updateChild()` is a core method on Element

4. **Input Event Flow**:
   - winit events → `InputEvent::from_winit()` → `ThreeTreePipeline::handle_event()` → element tree
   - Platform-independent input abstraction enables testing without winit

5. **State Management**:
   - `Component` + `Signal` for component-local state
   - `TextEditingController` for text editing state
   - `CursorBlinkState` for cursor animation

### Platform-Specific Initialization

- **Desktop** (`desktop_demo/src/main.rs`): `run_desktop_demo::<State>()` creates winit window, initializes `WindowState::new()`
- **iOS** (`shared_app/src/lib.rs`): `MobileApp` exports methods via UniFFI for Swift integration

## Workspace Dependency Management

Central dependency versions defined in root `Cargo.toml` workspace section; all crates reference via `{ workspace = true }`. Critical versions:
- wgpu 27.0.1 (GPU backend)
- taffy 0.9.1 (layout engine)
- glyphon (git branch: main for text rendering)
- uniffi 0.30.0 (FFI bindings for iOS)

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

## Development Workflow

- Always run `cargo build` after making edits to Rust files, and `cargo test` after implementing features. Never assume tests pass without running them.
- **Never run `cargo run -p desktop_demo` yourself** — you can't interact with the GUI AND your terminal may be on a different display (e.g., non-Retina) producing misleading results. Always ask the user to run it.
- **When debugging GUI bugs, always use the `debugging-gui-with-logs` skill.** Follow its workflow strictly: (1) form hypothesis, (2) add `log::debug!` with a unique prefix, (3) give the user the run command with `RUST_LOG=debug | grep | tee`, (4) read the log evidence, (5) fix root cause. Never skip to theory or try to reason without log evidence first.
- **Never rationalize running the demo.** Commands like `cargo run | grep` still execute the GUI on your display. If you need runtime evidence, instrument and ask the user to run.

## Commit Guidelines

- Do not include "Co-Authored-By: Claude" or similar attribution strings in commit messages

## Key File Locations

- Widget trait definition: `vexo/src/widgets/mod.rs`
- Element trait definition: `vexo/src/element.rs`
- Render object trait: `vexo/src/render_object.rs`
- Three-tree pipeline: `vexo/src/pipeline.rs`
- Application trait definition: `vexo/src/lib.rs`
- WindowState: `vexo/src/window.rs`
- Render backend: `vexo/src/render/wgpu_backend.rs`
- Input events: `vexo/src/input/event.rs`
- Stateful widgets: `vexo/src/stateful_widget.rs` (Component, ComponentState), `vexo/src/reactive/mod.rs` (Signal)
- InheritedWidget trait: `vexo/src/inherited_widget.rs`
- InheritedRegistry: `vexo/src/inherited_registry.rs`
- Theme widget: `vexo/src/widgets/theme.rs`
- Sample application: `shared_app/src/lib.rs`
- iOS wrapper: `shared_app/src/lib.rs`
- Build script: `build_for_ios.sh`

## Web Developer API Mapping

Vexo's public API maps to web framework concepts:

| Vexo | Web analog |
|---|---|
| `Component` trait | React function component / Vue component |
| `ComponentState` trait | React hooks state / Vue reactive state |
| `#[derive(ComponentState)]` | Auto-wires `Signal` fields (like React auto-re-renders) |
| `Signal<T>` | React `useState` / Vue `ref()` |
| `Component::render()` | React render / Vue template |
| `RenderContext` | React render function context |
| `LifecycleContext` | React effect context |
| `on_mount()` | React `useEffect([])` / Vue `onMounted()` |
| `on_update()` | React `useEffect([deps])` / Vue `onUpdated()` |
| `on_unmount()` | React cleanup / Vue `onUnmounted()` |
| `on_tick()` | `requestAnimationFrame` |
| `Column::new()` / `Row::new()` | `<div flex-direction: column/row>` |
| `children![]` macro | JSX children |
| `.on_press()` / `.on_release()` | `onClick` / `onMouseUp` |
| `InheritedWidget` trait | React Context Provider / Vue `provide()` |
| `RenderContext::depend_on_inherited_widget::<V>()` | React `useContext()` / Vue `inject()` |
| `Theme` / `ThemeData` | CSS custom properties / Tailwind theme |

## Three-Tree Architecture

Vexo implements Flutter's three-tree architecture for efficient UI updates. The key design principle is that **all elements manage their own children** through the `update_child()` method.

### Element Child Management

Each element is responsible for its own children:

1. **During mount**: Each element's `mount()` method mounts its children
2. **During rebuild**: Each element's `rebuild()` method uses `update_child()` to reconcile children

This matches Flutter's design where `updateChild()` is a core method on `Element`.

### The `update_child()` Method

The `Element` trait provides a default `update_child()` implementation:

```rust
fn update_child(
    &mut self,
    child: Option<ElementId>,
    new_widget: Option<Box<dyn Widget>>,
    slot: Option<usize>,
    context: &mut ElementContext,
) -> Option<ElementId>
```

This method handles all four cases:
- `(None, Some)` → Inflate new element
- `(Some, None)` → Unmount child
- `(Some, Some)` → Update if `can_update()`, else replace
- `(None, None)` → Do nothing

### Element Types

| Element Type | Children | Child Management |
|--------------|----------|------------------|
| `LeafElement` | None | No children to manage |
| `ContainerElement` | Multiple | Mounts in `mount()`, reconciles in `rebuild()` via `update_child()` |
| `DecoratedContainerElement` | Single | Mounts in `mount()`, reconciles in `rebuild()` via `update_child()` |
| `StatefulElement` | Single (from `render()`) | Mounts in `mount()`, reconciles in `update()` via `update_child()` |
| `GestureDetectorElement` | Single | Mounts in `mount()`, reconciles in `rebuild()` via `update_child()` |
