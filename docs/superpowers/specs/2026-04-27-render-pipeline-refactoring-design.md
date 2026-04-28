# Render Pipeline Refactoring Design

**Date:** 2026-04-27
**Goal:** Refactor `WindowState::render` method (221 lines, 12 responsibilities) into a clear pipeline with SRP-compliant modules.

## Context

The `WindowState::render` method in `vexo/src/window.rs` has grown to 221 lines with at least 12 distinct responsibilities:

1. Redraw request & backend check
2. Frame timing (cursor blink)
3. View generation
4. State clearing
5. Layout computation
6. Geometry generation
7. Viewport update
8. Text processing (regular) — ~53 lines
9. Text processing (editor) — ~68 lines
10. GPU upload
11. Text preparation
12. Render pass

Recent commits show a clear refactoring pattern: extract focused modules (`CursorBlinkState`, `TextCache`, `WindowState` moved to `window.rs`). This design continues that pattern.

## Design Goals

- **Testability** — Extract units that can be tested in isolation
- **Readability** — Make the render method easy to scan and understand
- **Maintainability** — Reduce blast radius of changes

## Architecture

### New Modules

```
vexo/src/
├── window.rs           # WindowState (simplified render method)
├── frame_context.rs    # FrameContext — shared read-only frame data
├── text_processor.rs   # TextProcessor — text + editor processing
└── render_pipeline.rs  # RenderPipeline — stage orchestration
```

### FrameContext

Holds shared read-only data that multiple pipeline stages need.

```rust
pub struct FrameContext<'a> {
    pub scale: f32,
    pub viewport_physical: Size<Physical>,
    pub layout_view: LayoutView<'a>,
    pub focused_widget_id: Option<WidgetId>,
    pub cursor_blink: &'a CursorBlinkState,
}
```

**Location:** `vexo/src/frame_context.rs`

### TextProcessor

Encapsulates all text processing logic — converting text_requests and editor_requests into `Vec<TextArea>`.

```rust
pub struct TextProcessor {
    cache: TextCache,
}

impl TextProcessor {
    pub fn new(font_system: &mut FontSystem) -> Self;

    pub fn process_text_requests(
        &mut self,
        requests: Vec<TextRequest>,
        font_system: &mut FontSystem,
        scale: f32,
    ) -> Vec<TextArea>;

    pub fn process_editor_requests(
        &mut self,
        requests: Vec<EditorRequest>,
        widget_context: &mut WidgetContext,
        scale: f32,
    );

    pub fn collect(
        &mut self,
        batcher: &UiBatcher,
        widget_context: &mut WidgetContext,
        scale: f32,
    ) -> Vec<TextArea>;
}
```

**Key behaviors:**
- Owns `TextCache` internally (moves it out of `WindowState`)
- `collect()` drains both request types from batcher and returns combined `Vec<TextArea>`
- Handles coordinate conversion (logical → physical) internally
- Evicts stale cache entries each frame

**Location:** `vexo/src/text_processor.rs`

### RenderPipeline

Orchestrates the render stages with clear method boundaries.

```rust
pub struct RenderPipeline {
    text_processor: TextProcessor,
}

impl RenderPipeline {
    pub fn new(font_system: &mut FontSystem) -> Self;

    pub fn compute_layout<M>(
        &mut self,
        widget: &mut dyn Widget<M>,
        layout_engine: &mut dyn LayoutEngine,
        root_node: LayoutNodeId,
        viewport_logical: Size<Logical>,
    ) -> LayoutOutput;

    pub fn generate_geometry<M>(
        &mut self,
        widget: &dyn Widget<M>,
        batcher: &mut UiBatcher,
        ctx: &FrameContext,
        layout_output: &LayoutOutput,
    );

    pub fn collect_text(
        &mut self,
        batcher: &mut UiBatcher,
        widget_context: &mut WidgetContext,
        ctx: &FrameContext,
    ) -> Vec<TextArea>;

    pub fn execute_render(
        &mut self,
        backend: &mut WgpuBackend,
        batcher: &UiBatcher,
        text_areas: Vec<TextArea>,
        widget_context: &mut WidgetContext,
    ) -> Result<(), RenderError>;
}
```

**Location:** `vexo/src/render_pipeline.rs`

### LayoutOutput

Carries layout computation results to subsequent stages.

```rust
pub struct LayoutOutput<'a> {
    pub layout_view: LayoutView<'a>,
}
```

### WindowState Changes

**Removed fields:**
- `text_cache: TextCache` — moved to `TextProcessor`

**Added fields:**
- `render_pipeline: RenderPipeline`

**Simplified render method (~50 lines):**

```rust
pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
    // 1. Redraw request & backend check
    if let Some(win) = &self.window {
        win.request_redraw();
    }
    if !self.backend.is_ready() {
        return Ok(());
    }

    // 2. Frame timing
    self.cursor_blink.tick();

    // 3. View generation
    let mut widget = self.view();

    // 4. Compute layout
    let viewport_logical = self.viewport_logical();
    let layout_output = self.render_pipeline.compute_layout(
        &mut *widget,
        self.layout_engine.as_mut(),
        self.root_node_id,
        viewport_logical,
    );

    // 5. Clear state
    self.layout_engine.clear();
    self.batcher.clear();

    // 6. Build frame context
    let ctx = FrameContext {
        scale: self.scale(),
        viewport_physical: self.viewport_physical(),
        layout_view: layout_output.layout_view,
        focused_widget_id: self.focused_widget_id,
        cursor_blink: &self.cursor_blink,
    };

    // 7. Generate geometry
    self.render_pipeline.generate_geometry(
        &*widget,
        &mut self.batcher,
        &ctx,
        &layout_output,
    );

    // 8. Update viewport
    self.backend.resize(self.viewport_physical());

    // 9. Collect text
    let text_areas = self.render_pipeline.collect_text(
        &mut self.batcher,
        &mut self.widget_context,
        &ctx,
    );

    // 10. Execute render
    self.render_pipeline.execute_render(
        &mut self.backend,
        &self.batcher,
        text_areas,
        &mut self.widget_context,
    ).map_err(|e| e.into())
}
```

## Testing Strategy

### TextProcessor Tests
- Test text processing logic without GPU
- Create `TextProcessor` with mock `FontSystem`
- Feed synthetic `TextRequest` / `EditorRequest`
- Verify `TextArea` output coordinates and bounds

### RenderPipeline Stage Tests
- `compute_layout()` — Use `MockLayoutEngine`
- `generate_geometry()` — Use mock batcher, verify commands
- `collect_text()` — Covered by TextProcessor tests
- `execute_render()` — Use `MockBackend`

### Integration Test
- Full pipeline with mock backend
- Verify `MockBackend` captured expected render commands

**Test locations:** Inline `#[cfg(test)]` modules in each source file.

## File Changes Summary

### New Files
- `vexo/src/frame_context.rs`
- `vexo/src/text_processor.rs`
- `vexo/src/render_pipeline.rs`

### Modified Files
- `vexo/src/window.rs` — Remove TextCache, add RenderPipeline, simplify render()
- `vexo/src/lib.rs` — Export new modules

## Verification

1. `cargo build -p vexo` — Verify compilation after each file change
2. `cargo test -p vexo` — Run all tests after refactoring complete
3. `cargo run -p desktop_demo` — Verify desktop demo renders correctly
