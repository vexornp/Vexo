# Remove Immediate Mode Code — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove all immediate mode UI code from Vexo, leaving only the retain mode (three-tree) architecture.

**Architecture:** Bottom-up removal: delete leaf files (old widget implementations) first, then the infrastructure they depend on (Widget<M> trait, RenderPipeline, etc.), then the Application trait redesign, then WindowState cleanup, then shared_app update, then final cleanup. Each task compiles, passes tests, and leaves the demo working.

**Tech Stack:** Rust, wgpu, taffy, glyphon

---

### Task 1: Remove old widget implementation files

**Files:**
- Delete: `vexo/src/widgets/text.rs`
- Delete: `vexo/src/widgets/column.rs`
- Delete: `vexo/src/widgets/row.rs`
- Delete: `vexo/src/widgets/button.rs`
- Delete: `vexo/src/widgets/text_edit.rs`
- Delete: `vexo/src/widgets/color_widget.rs`
- Delete: `vexo/src/widgets/grid.rs`
- Delete: `vexo/src/widgets/scroll_view.rs`
- Delete: `vexo/src/widgets/map_widget.rs`
- Delete: `vexo/src/widgets/modifiers.rs`
- Modify: `vexo/src/widgets/mod.rs` — remove `mod` declarations and `pub use` re-exports for all deleted files

- [ ] **Step 1: Remove module declarations and re-exports from widgets/mod.rs**

Remove these lines from `vexo/src/widgets/mod.rs`:

```rust
mod button;
mod color_widget;
mod column;
mod grid;
mod map_widget;
mod modifiers;
mod row;
mod scroll_view;
mod text;
mod text_edit;

pub use button::Button;
pub use color_widget::ColorWidget;
pub use column::Column;
pub use grid::Grid;
pub use map_widget::MapWidget;
pub use modifiers::Background;
pub use modifiers::Border;
pub use modifiers::CornerRadius;
pub use modifiers::WidgetExt;
pub use row::Row;
pub use scroll_view::{ScrollView, ScrollState};
pub use text::Text;
pub use text_edit::TextEdit;
```

Keep the `Widget<M>` trait, `WidgetResponse<M>`, `WidgetContext`, and `propagate_pointer_moved_to_containing_child` — other code still references them.

- [ ] **Step 2: Delete the widget implementation files**

```bash
rm vexo/src/widgets/text.rs \
   vexo/src/widgets/column.rs \
   vexo/src/widgets/row.rs \
   vexo/src/widgets/button.rs \
   vexo/src/widgets/text_edit.rs \
   vexo/src/widgets/color_widget.rs \
   vexo/src/widgets/grid.rs \
   vexo/src/widgets/scroll_view.rs \
   vexo/src/widgets/map_widget.rs \
   vexo/src/widgets/modifiers.rs
```

- [ ] **Step 3: Fix broken imports in lib.rs**

In `vexo/src/lib.rs`, remove the `pub use widgets::WidgetExt` line (the trait no longer exists). Also remove `use widgets::Widget` since nothing in lib.rs uses it directly anymore.

- [ ] **Step 4: Fix broken import in window.rs**

In `vexo/src/window.rs`, the import `use crate::widgets::{Column, Widget, WidgetContext, WidgetResponse}` references `Column` from the old widgets. Change to:

```rust
use crate::widgets::{Widget, WidgetContext, WidgetResponse};
```

- [ ] **Step 5: Fix broken imports in shared_app/lib.rs**

In `shared_app/src/lib.rs`, remove `WidgetExt` from the import. The line:

```rust
use vexo::{reactive::StatefulMutable, retain, widgets::Widget, Application, WidgetExt};
```

becomes:

```rust
use vexo::{reactive::StatefulMutable, retain, widgets::Widget, Application};
```

The `view()` function body still uses `vexo::column!`, `vexo::text!`, `vexo::button!`, `vexo::row!`, `vexo::component!` macros and types like `Column`, `Text`, `Button`, `ScrollView`, `Background`, `Border`, `CornerRadius`, `CounterComponent`. These will be fixed in later tasks. For now, the code won't compile — that's expected. We fix compilation in Step 6.

- [ ] **Step 6: Comment out the `view()` implementation in shared_app to allow compilation**

The `view()` function in `shared_app/src/lib.rs` (lines 214-270) references all the deleted types. Since we're removing immediate mode entirely, comment out the entire `view()` function body and replace with a minimal stub so the `Application` trait is still satisfied:

```rust
fn view(_state: &Self::State) -> Box<dyn Widget<Self::Message>> {
    // Immediate mode removed — this method will be deleted in Task 8
    unimplemented!("Immediate mode view removed. Use retain_view() instead.")
}
```

Also comment out or remove the `CounterComponent` impl and the `Message::CounterOutput` / `CounterMessage` / `CounterOutput` types that reference `Component`, since those will be deleted in Task 2. For now, just comment them out with a note:

```rust
// TODO: Remove in Task 2 (component system removal)
```

- [ ] **Step 7: Build and test**

```bash
cargo build
cargo test
```

Expected: Both pass. The demo will still work in retain mode (default).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: remove old immediate-mode widget implementations"
```

- [ ] **Step 9: User verification checkpoint**

Run `cargo run -p desktop_demo` and verify retain mode still works. Then confirm to proceed.

---

### Task 2: Remove component system

**Files:**
- Delete: `vexo/src/component/context.rs`
- Delete: `vexo/src/component/storage.rs`
- Delete: `vexo/src/component/widget.rs`
- Delete: `vexo/src/component/mod.rs`
- Delete: `vexo/src/component/` directory
- Modify: `vexo/src/lib.rs` — remove `mod component` and `pub mod component`
- Modify: `vexo/src/state/registry.rs` — remove `ComponentStateStorage` field and methods
- Modify: `shared_app/src/lib.rs` — remove `CounterComponent`, `CounterMessage`, `CounterOutput`

- [ ] **Step 1: Delete the component directory**

```bash
rm -rf vexo/src/component/
```

- [ ] **Step 2: Remove component module from lib.rs**

In `vexo/src/lib.rs`, remove:

```rust
pub mod component;
```

- [ ] **Step 3: Remove ComponentStateStorage from WidgetStateRegistry**

In `vexo/src/state/registry.rs`:
- Remove `use crate::component::storage::ComponentStateStorage;`
- Remove the `component_storage: ComponentStateStorage` field from `WidgetStateRegistry`
- Remove the `component_storage: ComponentStateStorage::new()` from `WidgetStateRegistry::new()`
- Remove the `pub fn component_storage(&mut self) -> &mut ComponentStateStorage` method
- Remove the `pub fn get_or_create_component_state<S>`, `pub fn get_component_state<S>`, `pub fn has_component_state`, and `pub fn remove_component_state` methods
- Remove the `self.component_storage.clear()` call in `WidgetStateRegistry::clear()`

- [ ] **Step 4: Remove component references from WidgetContext**

In `vexo/src/widgets/mod.rs`, remove the `create_component_context` method from `WidgetContext`:

```rust
pub fn create_component_context<'a, M: Clone + std::fmt::Debug + Send>(
    &'a mut self,
    key_path: crate::component::KeyPath,
) -> crate::component::ComponentContext<'a, M> {
    crate::component::ComponentContext::new(
        key_path,
        self.state.component_storage(),
        &mut self.font_system,
        self.scale,
    )
}
```

- [ ] **Step 5: Remove component-related code from shared_app**

In `shared_app/src/lib.rs`:
- Remove the `CounterComponent` struct and its `impl vexo::component::Component for CounterComponent` block
- Remove `CounterMessage` and `CounterOutput` enums
- Remove `Message::CounterOutput(CounterOutput)` variant
- Remove the `Message::CounterOutput(CounterOutput::CountReached(_n))` match arm from `update()`
- Remove the `vexo::component!(...)` call from `view()` (already commented out from Task 1)

- [ ] **Step 6: Build and test**

```bash
cargo build
cargo test
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: remove component system"
```

- [ ] **Step 8: User verification checkpoint**

Run `cargo run -p desktop_demo` and verify retain mode still works.

---

### Task 3: Remove testable module

**Files:**
- Delete: `vexo/src/testable/identifiable.rs`
- Delete: `vexo/src/testable/interact.rs`
- Delete: `vexo/src/testable/layout.rs`
- Delete: `vexo/src/testable/mod.rs`
- Delete: `vexo/src/testable/paint.rs`
- Delete: `vexo/src/testable/` directory
- Modify: `vexo/src/lib.rs` — remove `mod testable` and `pub mod testable`
- Modify: `vexo/src/widgets/mod.rs` — remove `apply_layout` method from `Widget<M>` trait and `use crate::testable::ComputedLayout`
- Modify: `vexo/src/layout/mod.rs` — if it re-exports `ComputedLayout` from testable, fix the re-export

- [ ] **Step 1: Delete the testable directory**

```bash
rm -rf vexo/src/testable/
```

- [ ] **Step 2: Remove testable module from lib.rs**

In `vexo/src/lib.rs`, remove:

```rust
pub mod testable;
```

- [ ] **Step 3: Fix Widget<M> trait — remove testable::ComputedLayout reference**

In `vexo/src/widgets/mod.rs`:
- Remove `use crate::testable::ComputedLayout;` (if present)
- Remove the `apply_layout` method from the `Widget<M>` trait
- Remove the `apply_layout` implementation from `Box<dyn Widget<M>>`
- Remove the `paint` method from the `Widget<M>` trait (it returns `Vec<RenderCommand>` via `PaintContext` — immediate-mode-only)
- Remove the `paint` implementation from `Box<dyn Widget<M>>`

- [ ] **Step 4: Fix layout/mod.rs re-export**

Check if `vexo/src/layout/mod.rs` re-exports `ComputedLayout` from `testable`. If so, check if anything in the `layout` module defines its own `ComputedLayout` or if it was purely a re-export. The layout module has its own `ComputedLayout` in `node.rs` (line 215), so the testable version can be safely removed. If the layout module re-exports from testable, remove that re-export.

- [ ] **Step 5: Build and test**

```bash
cargo build
cargo test
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: remove testable module"
```

- [ ] **Step 7: User verification checkpoint**

---

### Task 4: Remove macros

**Files:**
- Delete: `vexo/src/macros.rs`
- Modify: `vexo/src/lib.rs` — remove `mod macros`

- [ ] **Step 1: Delete the macros file**

```bash
rm vexo/src/macros.rs
```

- [ ] **Step 2: Remove macros module from lib.rs**

In `vexo/src/lib.rs`, remove:

```rust
mod macros;
```

- [ ] **Step 3: Build and test**

```bash
cargo build
cargo test
```

Note: The `view()` in shared_app still references `vexo::column!`, etc. but it's the `unimplemented!()` stub from Task 1, so it won't compile those macro calls.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: remove immediate-mode macros"
```

- [ ] **Step 5: User verification checkpoint**

---

### Task 5: Remove immediate-mode rendering infrastructure

**Files:**
- Delete: `vexo/src/render_pipeline.rs`
- Delete: `vexo/src/frame_context.rs`
- Modify: `vexo/src/lib.rs` — remove `mod render_pipeline`, `mod frame_context`, `pub use frame_context::FrameContext`
- Modify: `vexo/src/window.rs` — remove `use crate::render_pipeline::RenderPipeline`, `use crate::frame_context::FrameContext`, the `render_pipeline` field, and the `render_immediate()` method
- Modify: `vexo/src/text_processor.rs` — change `WidgetContext` parameter to accept `(&mut FontSystem, &WidgetStateRegistry)` instead (decouple from WidgetContext)

Note: `QuadInstance` is also used by `wgpu_backend.rs` and `UiBatcher` — it stays as part of the GPU rendering layer.

- [ ] **Step 1: Delete render_pipeline.rs and frame_context.rs**

```bash
rm vexo/src/render_pipeline.rs vexo/src/frame_context.rs
```

- [ ] **Step 2: Remove module declarations from lib.rs**

In `vexo/src/lib.rs`, remove:

```rust
mod frame_context;
mod render_pipeline;
```

and:

```rust
pub use frame_context::FrameContext;
```

- [ ] **Step 3: Remove RenderPipeline and FrameContext from window.rs**

In `vexo/src/window.rs`:
- Remove `use crate::frame_context::FrameContext;`
- Remove `use crate::render_pipeline::RenderPipeline;`
- Remove `render_pipeline: RenderPipeline` field from `WindowState`
- Remove `render_pipeline: RenderPipeline::new()` from the constructor
- Remove the entire `render_immediate()` method

- [ ] **Step 4: Fix text_processor.rs — decouple from WidgetContext**

The `process_editor_requests` method takes `&mut WidgetContext`. It only needs `&mut FontSystem` and the editor lookup. Since we're removing `WidgetStateRegistry` in Task 6, we need to think ahead.

Looking at the code: `process_editor_requests` calls `widget_context.get_or_create_editor(&req.id, "initial_text")` which delegates to `WidgetStateRegistry`. In the retain mode path (window.rs `render_retain()`), `RenderCommand::Editor` requests are processed via `batcher.editor_requests` which get collected by `collect_text()` → `process_editor_requests()`.

Since the retain path already handles text editing via `TextEditingController` and `TextEditRenderObject` (which renders via `RenderCommand::Caret` and `RenderCommand::Editor`), and `RenderCommand::Editor` needs the editor state...

The simplest approach: refactor `text_processor.rs` `process_editor_requests` to accept `(&mut FontSystem, &mut EditorState)` where `EditorState` is the `state/editor.rs` type that manages `EditorRef`s. This keeps the editor lookup working without `WidgetContext`.

However, looking at the retain pipeline code in `window.rs`, the `render_retain()` method processes `RenderCommand::Editor` by pushing to `batcher.editor_requests`, and then `collect_text()` calls `process_editor_requests()` which needs to find the editor by ID.

Actually, looking more carefully at `render_retain()`: it doesn't call `process_editor_requests` directly. It calls `self.render_pipeline.collect_text()` which internally calls `process_editor_requests`. But we just deleted `render_pipeline`!

Looking at `window.rs` `render_retain()` line 602-615: it uses `self.render_pipeline.collect_text()` and `self.render_pipeline.execute_render()`. So `RenderPipeline` IS used by the retain path too — it provides `collect_text` and `execute_render`.

**Revised approach:** We cannot delete `RenderPipeline` entirely because the retain path depends on it. Instead, we remove only the immediate-mode-specific methods (`compute_layout` and `generate_geometry` which take `Widget<M>`) and keep the text/rendering methods that both paths share.

- [ ] **Step 5 (revised): Keep RenderPipeline but remove immediate-mode methods**

In `vexo/src/render_pipeline.rs`:
- Remove the `compute_layout` method (takes `&mut dyn Widget<M>`)
- Remove the `generate_geometry` method (takes `&dyn Widget<M>`)
- Keep `collect_text`, `execute_render`, and `CombinedPreparedText`
- Remove `use crate::widgets::{Widget, WidgetContext};` — `collect_text` and `execute_render` still need `WidgetContext` for now (fixed in Task 6)

- [ ] **Step 6: Keep FrameContext but mark as dead code (it's only used by generate_geometry)**

Actually, `FrameContext` is only constructed in `render_immediate()` and only used by `generate_geometry()`. Both are now deleted. So we CAN delete `frame_context.rs`.

But wait — let me verify that `FrameContext` is used anywhere else:

```bash
grep -rn "FrameContext" --include="*.rs" vexo/src/ | grep -v "target/"
```

It's only referenced in `render_pipeline.rs` and `window.rs` (both in the immediate mode path). Safe to delete.

- [ ] **Step 7: Build and test**

```bash
cargo build
cargo test
```

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: remove immediate-mode render pipeline methods and FrameContext"
```

- [ ] **Step 9: User verification checkpoint**

---

### Task 6: Remove old state management

**Files:**
- Delete: `vexo/src/state/registry.rs`
- Delete: `vexo/src/state/editor.rs`
- Delete: `vexo/src/state/focus.rs`
- Modify: `vexo/src/state/mod.rs` — remove `mod editor`, `mod focus`, `mod registry` and their re-exports
- Modify: `vexo/src/widgets/mod.rs` — remove `WidgetContext` (inline font_system/scale/cursor_pos into WindowState)
- Modify: `vexo/src/text_processor.rs` — change signature to accept `(&mut FontSystem, &HashMap<String, EditorRef>)` or similar
- Modify: `vexo/src/window.rs` — replace `widget_context: WidgetContext` with inline fields; remove immediate-mode event handling; update all `self.widget_context.X` references

This is the most complex task because `WidgetContext` is widely used. The approach:

1. Remove `WidgetStateRegistry`, `EditorRef`, `FocusState` — these are only used by the immediate mode path
2. Delete `WidgetContext` and move its fields (`font_system`, `scale`, `cursor_pos`) directly into `WindowState`
3. Fix `text_processor.rs` to not depend on `WidgetContext`

But wait — `EditorRef` is used by the retain mode `TextEditingController`. Let me check:

- [ ] **Step 1: Verify EditorRef usage in retain mode**

Check if `TextEditingController` uses `EditorRef` or the `vexo::editor::Editor` directly. Looking at the code from earlier exploration:
- `vexo/src/retain/widgets/text_edit.rs` imports `crate::editor::Editor`
- `vexo/src/retain/widgets/text_edit_content.rs` imports `crate::editor::Editor`
- `vexo/src/retain/render_objects/text_edit.rs` imports `crate::editor::Editor`

These all use `vexo::editor::Editor` directly, not `EditorRef` (which is `Rc<RefCell<Editor>>`). The `TextEditingController` likely wraps an `Editor` directly or uses `glyphon::Editor`.

So `state/editor.rs` (`EditorRef` / `EditorState`) can be safely removed — the retain mode uses `vexo::editor::Editor` directly.

- [ ] **Step 2: Delete the immediate-mode state files**

```bash
rm vexo/src/state/registry.rs vexo/src/state/editor.rs vexo/src/state/focus.rs
```

- [ ] **Step 3: Update state/mod.rs**

Remove module declarations and re-exports:

```rust
pub mod cursor_blink;
// Removed: editor, focus, registry

pub use cursor_blink::CursorBlinkState;
// Removed: EditorRef, FocusState, WidgetStateRegistry
```

- [ ] **Step 4: Remove WidgetContext from widgets/mod.rs**

Remove the entire `WidgetContext` struct and its impl block. Also remove `use crate::state::WidgetStateRegistry`.

- [ ] **Step 5: Add font_system, scale, cursor_pos fields directly to WindowState**

In `vexo/src/window.rs`:
- Remove `use crate::widgets::{..., WidgetContext, ...}`
- Remove `pub widget_context: WidgetContext` field
- Add these fields directly:
  ```rust
  font_system: glyphon::FontSystem,
  scale: crate::core::Scale,
  cursor_pos: crate::core::Point<crate::core::Physical>,
  ```
- Update the constructor to initialize these fields (copy the FontSystem init code from `WidgetContext::new()`)
- Replace all `self.widget_context.font_system` with `self.font_system`
- Replace all `self.widget_context.scale` with `self.scale`
- Replace all `self.widget_context.cursor_pos` with `self.cursor_pos`

- [ ] **Step 6: Fix text_processor.rs**

`process_editor_requests` currently takes `&mut WidgetContext`. It calls `widget_context.get_or_create_editor()` to look up editors. Since `WidgetStateRegistry` is gone, we need a different approach.

Looking at the retain mode `render_retain()` flow: `RenderCommand::Editor` pushes `EditorRequest { id, bounds, color }` to `batcher.editor_requests`. Then `collect_text` → `process_editor_requests` looks up the editor by ID.

In the retain mode, editors are managed by `TextEditingController`, not `WidgetStateRegistry`. The `TextEditingController` holds a `glyphon::Editor` directly. The `RenderCommand::Editor` is only emitted by old-style immediate-mode TextEdit widgets. The retain-mode `TextEditRenderObject` emits `RenderCommand::Caret` and uses the editor's buffer directly for text rendering, not `RenderCommand::Editor`.

So `RenderCommand::Editor` is actually only used by the immediate mode path! Let me verify:

```bash
grep -rn "RenderCommand::Editor" --include="*.rs" vexo/src/ | grep -v "target/"
```

If `RenderCommand::Editor` is only emitted by old widget code (which we already deleted), then we can remove the `Editor` variant from `RenderCommand`, remove `process_editor_requests` from `TextProcessor`, and remove `EditorRequest` from `UiBatcher`.

- [ ] **Step 7: Remove RenderCommand::Editor and process_editor_requests**

In `vexo/src/render/command.rs`, remove the `Editor { id, bounds, color }` variant.

In `vexo/src/renderer.rs`, remove `EditorRequest` struct and `editor_requests` field from `UiBatcher`. Remove the `push` to `editor_requests` in the `add_editor_request` method. Remove `process_editor_requests` calls.

In `vexo/src/text_processor.rs`, remove the `process_editor_requests` method entirely.

In `vexo/src/window.rs`, remove the `RenderCommand::Editor` match arm in `render_retain()`.

- [ ] **Step 8: Update text_processor.rs collect_text**

Now `collect_text` only calls `process_text_requests` which takes `(&mut FontSystem, Vec<TextRequest>, Scale, Size<Physical>)`. It doesn't need `WidgetContext` anymore. Update the signature:

```rust
pub fn collect_text(
    &mut self,
    batcher: &mut UiBatcher,
    font_system: &mut FontSystem,
    scale: Scale,
    viewport_physical: Size<Physical>,
) -> CombinedPreparedText
```

And in `window.rs`, pass `&mut self.font_system` instead of `&mut self.widget_context`.

- [ ] **Step 9: Update execute_render**

`execute_render` takes `&mut WidgetContext`. It only needs `&mut FontSystem`. Update signature:

```rust
pub fn execute_render(
    &mut self,
    backend: &mut WgpuBackend,
    batcher: &UiBatcher,
    prepared_text: CombinedPreparedText,
    font_system: &mut FontSystem,
) -> Result<(), RenderError>
```

- [ ] **Step 10: Build and test**

```bash
cargo build
cargo test
```

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "refactor: remove immediate-mode state management and WidgetContext"
```

- [ ] **Step 12: User verification checkpoint**

---

### Task 7: Remove old Widget<M> trait and remaining widgets/mod.rs code

**Files:**
- Modify: `vexo/src/widgets/mod.rs` — remove `Widget<M>` trait, `WidgetResponse<M>`, `propagate_pointer_moved_to_containing_child`
- If `widgets/mod.rs` is now empty, delete it and the directory
- Modify: `vexo/src/lib.rs` — remove `use widgets::Widget`, `pub mod widgets`
- Modify: `vexo/src/window.rs` — remove all remaining references to `Widget`, `WidgetResponse`, immediate-mode event handling methods

- [ ] **Step 1: Remove Widget<M> trait and WidgetResponse from widgets/mod.rs**

In `vexo/src/widgets/mod.rs`, remove:
- The `Widget<M>` trait definition and its `Box<dyn Widget<M>>` impl
- The `WidgetResponse<M>` struct and its `Default` impl
- The `propagate_pointer_moved_to_containing_child` function

If nothing remains in the file, delete it entirely along with the directory.

- [ ] **Step 2: Remove widgets module from lib.rs**

In `vexo/src/lib.rs`, remove:

```rust
mod widgets;
pub mod widgets;
```

- [ ] **Step 3: Fix window.rs — remove immediate-mode event path**

In `vexo/src/window.rs`:
- Remove `use crate::widgets::{Column, Widget, WidgetContext, WidgetResponse};`
- Remove `root_widget: Box<dyn Widget<A::Message>>` field
- Remove `root_node_id: LayoutNodeKey` field
- Remove `focused_widget_id: Option<WidgetId>` field
- Remove the immediate-mode code in `new()` that initializes `root_widget` and `root_node_id`
- Remove the `process_input_event()` method (immediate mode event path)
- Remove the `handle_widget_response()` method
- Remove the `update_cursor()` method
- Remove the immediate-mode branch in `process_input_event()` (the `if self.use_retain_mode` check becomes unconditional)
- Simplify `process_input_event()` to only call `process_input_event_retain()`
- Remove the `view()` method that calls `A::view()`

- [ ] **Step 4: Simplify render() — remove immediate-mode branch**

In `window.rs`, `render()` currently checks `use_retain_mode` and dispatches. Remove the flag check and `render_immediate()` path. The method becomes just `render_retain()`.

- [ ] **Step 5: Build and test**

```bash
cargo build
cargo test
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: remove Widget<M> trait and immediate-mode event handling"
```

- [ ] **Step 7: User verification checkpoint**

---

### Task 8: Redesign Application trait (retain-only)

**Files:**
- Modify: `vexo/src/lib.rs` — redesign `Application` trait
- Modify: `vexo/src/window.rs` — update to use new trait
- Modify: `vexo/src/app.rs` — update `VexoApp` to use new trait bounds
- Modify: `shared_app/src/lib.rs` — implement new trait

The new `Application` trait removes `Message`, `view()`, and `update()`, making `retain_view()` the only view method (renamed to just `view()`).

- [ ] **Step 1: Redesign Application trait in lib.rs**

Replace the current trait:

```rust
pub trait Application: Sized + 'static {
    type Message: Clone + std::fmt::Debug + Send;
    type State;

    fn new() -> Self::State;
    fn update(state: &mut Self::State, message: Self::Message);
    fn view(state: &Self::State) -> Box<dyn Widget<Self::Message>>;

    fn retain_view(state: &mut Self::State, font_system: &mut glyphon::FontSystem) -> Option<Box<dyn retain::Widget>> {
        let _ = (state, font_system);
        None
    }
}
```

With:

```rust
pub trait Application: Sized + 'static {
    type State;

    fn new() -> Self::State;
    fn view(state: &mut Self::State, font_system: &mut glyphon::FontSystem) -> Box<dyn retain::Widget>;
}
```

- [ ] **Step 2: Update WindowState in window.rs**

- Remove `user_app_state: A::State` (it's still needed, but the type changes since `A` no longer has `Message`)
- Actually, `WindowState<A>` still needs `user_app_state: A::State` — that's fine
- Remove the `update()` method (no more message handling)
- Update `view_retain()` to call `A::view()` (renamed from `retain_view()`)
- Remove the `view()` method

- [ ] **Step 3: Update VexoApp in app.rs**

Read `vexo/src/app.rs` and update the `VexoApp` struct and impl to use the new `Application` trait (no `Message` type parameter).

- [ ] **Step 4: Update shared_app**

In `shared_app/src/lib.rs`:
- Remove `Message` enum entirely
- Remove `view()` method
- Remove `update()` method
- Implement the new `Application` trait with only `new()` and `view()`
- Move the `retain_view()` body into `view()`

- [ ] **Step 5: Build and test**

```bash
cargo build
cargo test
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: redesign Application trait as retain-only"
```

- [ ] **Step 7: User verification checkpoint**

---

### Task 9: Clean up WindowState

**Files:**
- Modify: `vexo/src/window.rs` — remove `use_retain_mode` toggle, 'R' key handler, dead code

- [ ] **Step 1: Remove use_retain_mode and toggle**

In `window.rs`:
- Remove `use_retain_mode: bool` field
- Remove `set_retain_mode()` method
- Remove `toggle_retain_mode()` method
- Remove the 'R' key handler in `handle_window_event()`

- [ ] **Step 2: Remove dead code annotations**

Remove any `#[allow(dead_code)]` annotations that were silencing warnings about unused immediate-mode fields/methods.

- [ ] **Step 3: Clean up WindowState::new()**

The constructor no longer needs to create a `root_widget` or `root_node_id`. Clean up the async constructor accordingly.

- [ ] **Step 4: Build and test**

```bash
cargo build
cargo test
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: clean up WindowState, remove retain-mode toggle"
```

- [ ] **Step 6: User verification checkpoint**

---

### Task 10: Final cleanup

**Files:**
- Modify: `vexo/src/lib.rs` — remove all dead re-exports
- Search and destroy any remaining immediate-mode references across the entire codebase

- [ ] **Step 1: Clean up lib.rs public API**

In `vexo/src/lib.rs`, remove any re-exports that reference deleted types:
- `pub use renderer::UiBatcher;` — keep (still used by retain path)
- `pub use state::CursorBlinkState;` — keep (still used)
- `pub use window::WindowState;` — keep
- Remove `pub use layout::AlignItems;` if nothing uses it
- Remove any other dead re-exports

- [ ] **Step 2: Search for any remaining immediate-mode references**

```bash
grep -rn "Widget<M>\|WidgetResponse\|WidgetContext\|WidgetExt\|FrameContext\|RenderPipeline\|WidgetStateRegistry\|EditorRef\|FocusState\|propagate_pointer_moved\|component!\|column!\|text!\|button!\|row!\|grid!\|color_widget!\|text_edit!" --include="*.rs" vexo/src/ shared_app/src/ desktop_demo/src/ | grep -v "target/" | grep -v ".superpowers/"
```

Fix any remaining references.

- [ ] **Step 3: Remove any orphaned use of `resource` module**

Check if `vexo/src/resource/` is still needed. If it was only used for embedded fonts by `WidgetContext::new()`, and we moved that init code to `WindowState::new()`, then the resource module stays (it's still used).

- [ ] **Step 4: Full build, test, and clippy**

```bash
cargo build
cargo test
cargo clippy -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: final cleanup of immediate-mode remnants"
```

- [ ] **Step 6: Final user verification checkpoint**

Run `cargo run -p desktop_demo` and confirm everything works correctly in retain mode.