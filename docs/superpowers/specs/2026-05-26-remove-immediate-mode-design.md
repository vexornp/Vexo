# Remove Immediate Mode Code — Design Spec

## Goal

Remove all immediate mode UI code from Vexo, leaving only the retain mode (three-tree) architecture. The work is broken into incremental tasks where each task compiles, passes tests, and leaves the desktop demo working in retain mode.

## Removal Scope

### Old widget implementations (entire files)
- `vexo/src/widgets/text.rs`
- `vexo/src/widgets/column.rs`
- `vexo/src/widgets/row.rs`
- `vexo/src/widgets/button.rs`
- `vexo/src/widgets/text_edit.rs`
- `vexo/src/widgets/color_widget.rs`
- `vexo/src/widgets/grid.rs`
- `vexo/src/widgets/scroll_view.rs`
- `vexo/src/widgets/map_widget.rs`
- `vexo/src/widgets/modifiers.rs`

### Old widget infrastructure
- `vexo/src/widgets/mod.rs` — `Widget<M>` trait, `WidgetResponse<M>`, `WidgetContext`, `WidgetExt<M>`, `propagate_pointer_moved_to_containing_child()`

### Component system (entire directory)
- `vexo/src/component/` — `Component` trait, `ComponentWidget`, `ComponentContext`, `ComponentStateStorage`

### Testable module (entire directory)
- `vexo/src/testable/` — `Identifiable`, `Layout`, `Paint`, `Interact` traits

### Immediate-mode rendering infrastructure
- `vexo/src/render_pipeline.rs` — `RenderPipeline`
- `vexo/src/frame_context.rs` — `FrameContext`
- `vexo/src/quad_instance.rs` — `QuadInstance`
- `vexo/src/macros.rs` — `column!`, `text!`, `button!`, `component!` macros

### Old state management
- `vexo/src/state/registry.rs` — `WidgetStateRegistry`
- `vexo/src/state/editor.rs` — `Editor` / `EditorRef`
- `vexo/src/state/focus.rs` — `FocusState`

### Application trait redesign
- Remove `Message` type parameter
- Remove `view()` method
- Remove `update()` method
- Make `retain_view()` the primary (and only) view method

### WindowState cleanup
- Remove immediate mode rendering path and all immediate-mode-only fields
- Remove `use_retain_mode` toggle

### shared_app update
- Remove `Message` enum, `CounterMessage`, `CounterOutput`, `CounterComponent`
- Implement retain-only `Application` trait

## Task Breakdown

Each task must: compile (`cargo build`), pass tests (`cargo test`), and leave the demo working in retain mode.

### Task 1: Remove old widget implementations from widgets/mod.rs

- Remove `mod` declarations and re-exports for: `text`, `column`, `row`, `button`, `text_edit`, `color_widget`, `grid`, `scroll_view`, `map_widget`, `modifiers`
- Delete the corresponding source files
- Keep `Widget<M>` trait, `WidgetResponse<M>`, `WidgetContext` in `widgets/mod.rs` for now (other code still references them)
- Fix any broken imports in `lib.rs` and other files
- **Checkpoint:** Build + test + demo

### Task 2: Remove component system

- Delete `vexo/src/component/` directory
- Remove `mod component` from `lib.rs`
- Remove any re-exports of component types
- Fix broken imports
- **Checkpoint:** Build + test + demo

### Task 3: Remove testable module

- Delete `vexo/src/testable/` directory
- Remove `mod testable` from `lib.rs`
- Remove any re-exports of testable types
- Fix broken imports
- **Checkpoint:** Build + test + demo

### Task 4: Remove macros

- Delete `vexo/src/macros.rs`
- Remove `mod macros` from `lib.rs`
- **Checkpoint:** Build + test + demo

### Task 5: Remove immediate-mode rendering infrastructure

- Delete `vexo/src/render_pipeline.rs`
- Delete `vexo/src/frame_context.rs`
- Delete `vexo/src/quad_instance.rs`
- Remove module declarations and re-exports from `lib.rs`
- Fix broken imports in `window.rs` and elsewhere
- **Checkpoint:** Build + test + demo

### Task 6: Remove old state management

- Delete `vexo/src/state/registry.rs`, `vexo/src/state/editor.rs`, `vexo/src/state/focus.rs`
- Remove re-exports from `state/mod.rs` and `lib.rs`
- Fix broken imports (likely in `window.rs`, `widgets/mod.rs`)
- **Checkpoint:** Build + test + demo

### Task 7: Remove old Widget<M> trait and WidgetContext

- Remove `Widget<M>`, `WidgetResponse<M>`, `WidgetContext`, `WidgetExt<M>`, `propagate_pointer_moved_to_containing_child()` from `widgets/mod.rs`
- If `widgets/mod.rs` is now empty, delete it and the `widgets` directory
- Fix all broken imports across the codebase
- **Checkpoint:** Build + test + demo

### Task 8: Redesign Application trait (retain-only)

- Remove `Message` type parameter from `Application` trait
- Remove `view()` method
- Remove `update()` method
- Rename `retain_view()` to `view()`
- Update `WindowState` to call the simplified `view()`
- Update all implementors
- **Checkpoint:** Build + test + demo

### Task 9: Clean up WindowState

- Remove immediate-mode-only fields: `root_widget`, `root_node_id`, `focused_widget_id`, `widget_context`
- Remove immediate-mode-only methods: `render_immediate()`, `process_input_event()`, `handle_widget_response()`, `update_cursor()`
- Remove `use_retain_mode` flag and `toggle_retain_mode()`
- Remove 'R' key toggle in event handling
- Remove `view()` method (if still present)
- **Checkpoint:** Build + test + demo

### Task 10: Update shared_app to retain-only Application

- Remove `Message` enum, `CounterMessage`, `CounterOutput`, `CounterComponent`
- Remove `view()` and `update()` implementations
- Implement the simplified retain-only `Application` trait
- **Checkpoint:** Build + test + demo

### Task 11: Final cleanup

- Remove any dead imports, unused fields, or orphaned code
- Clean up `lib.rs` public API (remove all immediate-mode re-exports)
- Remove any `#[allow(dead_code)]` or TODO comments related to immediate mode
- Verify no references to removed types remain
- **Checkpoint:** Final build + test + demo

## What Stays

- **Three-tree pipeline:** `ThreeTreePipeline`, `Element` trait, `ElementRegistry`
- **Retain-mode widgets:** `Text`, `TextEditContent`, `Column`, `Row`, `DecoratedContainer`, `GestureDetector`
- **Stateful widgets:** `StatefulElement`, `StatefulMutable`
- **Render objects:** `RenderObject`, `RenderObjectRegistry`
- **Layout:** `LayoutEngine`, `TaffyLayoutEngine`, `LayoutNode`, `ComputedLayout`
- **Rendering:** `RenderCommand`, `RenderBackend`, `WgpuBackend`, `MockBackend`, `UiBatcher`
- **Input:** `InputEvent` and related types
- **Core types:** `WidgetId`, `Color`, `Point`, `Size`, `Rect`, `Scale`
- **Cursor/blink:** `CursorBlinkState`
