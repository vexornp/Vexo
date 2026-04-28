# Render Pipeline Refactoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `WindowState::render` method (221 lines) into a clear pipeline with SRP-compliant modules.

**Architecture:** Extract `FrameContext` for shared frame data, `TextProcessor` for text processing, and `RenderPipeline` to orchestrate stages. Each module has a single responsibility and can be tested independently.

**Tech Stack:** Rust, wgpu, glyphon, taffy

---

## File Structure

### New Files
- `vexo/src/frame_context.rs` — Shared read-only frame data
- `vexo/src/text_processor.rs` — Text processing (regular + editor)
- `vexo/src/render_pipeline.rs` — Pipeline orchestration

### Modified Files
- `vexo/src/window.rs` — Remove TextCache, add RenderPipeline, simplify render()
- `vexo/src/lib.rs` — Export new modules

---

## Task 1: Create FrameContext Module

**Files:**
- Create: `vexo/src/frame_context.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Create frame_context.rs with FrameContext struct**

```rust
//! Shared frame context for render pipeline stages.

use crate::core::{Physical, Size, WidgetId};
use crate::layout::LayoutView;
use crate::state::CursorBlinkState;

/// Shared read-only data for render pipeline stages.
pub struct FrameContext<'a> {
    pub scale: crate::core::Scale,
    pub viewport_physical: Size<Physical>,
    pub layout_view: LayoutView<'a>,
    pub focused_widget_id: Option<WidgetId>,
    pub cursor_blink: &'a CursorBlinkState,
}
```

- [ ] **Step 2: Add module export to lib.rs**

Add after line 28 (`mod window;`):

```rust
mod frame_context;
pub use frame_context::FrameContext;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p vexo`
Expected: Success with no errors

- [ ] **Step 4: Commit**

```bash
git add vexo/src/frame_context.rs vexo/src/lib.rs
git commit -m "feat: add FrameContext for shared render frame data"
```

---

## Task 2: Create TextProcessor Module

**Files:**
- Create: `vexo/src/text_processor.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Create text_processor.rs with TextProcessor struct**

```rust
//! Text processing for render pipeline.
//!
//! Handles conversion of text requests and editor requests into
//! glyphon TextArea instances ready for rendering.

use glyphon::{cosmic_text, Buffer, FontSystem, TextArea, TextBounds};

use crate::core::{Logical, Physical, Point, Scale, Size};
use crate::renderer::{EditorRequest, TextRequest};
use crate::text_cache::TextCache;
use crate::widgets::WidgetContext;

/// Processes text requests into TextArea instances for rendering.
pub struct TextProcessor {
    cache: TextCache,
}

impl TextProcessor {
    /// Create a new text processor.
    pub fn new() -> Self {
        Self {
            cache: TextCache::new(),
        }
    }

    /// Process regular text requests into TextArea instances.
    pub fn process_text_requests(
        &mut self,
        font_system: &mut FontSystem,
        requests: Vec<TextRequest>,
        scale: Scale,
        viewport_physical: Size<Physical>,
    ) -> Vec<TextArea> {
        let mut processed_texts: Vec<(Buffer, TextRequest)> = Vec::new();

        for req in requests {
            let buffer = self.cache.get_or_create(font_system, &req);
            processed_texts.push((buffer, req));
        }

        // Periodically evict stale cache entries
        self.cache.evict_stale();

        // Create TextAreas from processed buffers
        processed_texts
            .iter_mut()
            .map(|(buffer, req)| {
                self.create_text_area(buffer, req, scale, viewport_physical)
            })
            .collect()
    }

    /// Process editor requests into TextArea instances.
    pub fn process_editor_requests(
        &mut self,
        widget_context: &mut WidgetContext,
        requests: Vec<EditorRequest>,
        scale: Scale,
    ) -> Vec<TextArea> {
        // Collect owned editor buffers and metadata first to avoid borrow issues
        let mut editor_buffers: Vec<Buffer> = Vec::new();
        let mut editor_meta: Vec<(f32, f32, i32, i32, i32, i32, cosmic_text::Color)> = Vec::new();

        for req in &requests {
            let physical_rect = req.bounds.to_physical(scale);

            let bounds_left: i32 = physical_rect.origin.x.floor() as i32;
            let bounds_top: i32 = physical_rect.origin.y.floor() as i32;
            let bounds_right: i32 =
                (physical_rect.origin.x + physical_rect.size.width).ceil() as i32;
            let bounds_bottom: i32 =
                (physical_rect.origin.y + physical_rect.size.height).ceil() as i32;

            let color_rgba_u8 = cosmic_text::Color::rgba(
                (req.color[0] * 255.0) as u8,
                (req.color[1] * 255.0) as u8,
                (req.color[2] * 255.0) as u8,
                (req.color[3] * 255.0) as u8,
            );

            let editor_ref = widget_context.get_or_create_editor(&req.id, "initial_text");
            let editor = editor_ref.borrow();
            let buf = editor.buffer().clone();
            editor_buffers.push(buf);
            editor_meta.push((
                physical_rect.origin.x,
                physical_rect.origin.y,
                bounds_left,
                bounds_top,
                bounds_right,
                bounds_bottom,
                color_rgba_u8,
            ));
        }

        // Build TextArea instances borrowing from owned editor_buffers
        let mut editor_areas: Vec<TextArea> = Vec::new();
        for (i, buf) in editor_buffers.iter_mut().enumerate() {
            let (left_pos, top_pos, bounds_left, bounds_top, bounds_right, bounds_bottom, color) =
                editor_meta[i];
            buf.shape_until_scroll(&mut widget_context.font_system, true);

            editor_areas.push(TextArea {
                buffer: buf,
                left: left_pos,
                top: top_pos,
                scale: scale.factor(),
                bounds: TextBounds {
                    left: bounds_left,
                    top: bounds_top,
                    right: bounds_right,
                    bottom: bounds_bottom,
                },
                default_color: color,
                custom_glyphs: &[],
            });
        }

        editor_areas
    }

    /// Create a TextArea from a buffer and request.
    fn create_text_area(
        &self,
        buffer: &mut Buffer,
        req: &TextRequest,
        scale: Scale,
        viewport_physical: Size<Physical>,
    ) -> TextArea {
        // Convert logical position to physical
        let physical_pos = req.position.to_physical(scale);

        // Use clip bounds if set, otherwise use screen bounds
        let (bounds_left, bounds_top, bounds_right, bounds_bottom) =
            if req.clip_bounds[2] > 0.0 {
                // Clip bounds are in logical coordinates - convert to physical
                let clip_left = req.clip_bounds[0] * scale.factor();
                let clip_top = req.clip_bounds[1] * scale.factor();
                let clip_right = (req.clip_bounds[0] + req.clip_bounds[2]) * scale.factor();
                let clip_bottom = (req.clip_bounds[1] + req.clip_bounds[3]) * scale.factor();
                (
                    clip_left.floor() as i32,
                    clip_top.floor() as i32,
                    clip_right.ceil() as i32,
                    clip_bottom.ceil() as i32,
                )
            } else {
                // No clipping - use full screen
                (
                    physical_pos.x.floor() as i32,
                    physical_pos.y.floor() as i32,
                    viewport_physical.width_u32() as i32,
                    viewport_physical.height_u32() as i32,
                )
            };

        let color_rgba_u8 = cosmic_text::Color::rgba(
            (req.color[0] * 255.0) as u8,
            (req.color[1] * 255.0) as u8,
            (req.color[2] * 255.0) as u8,
            (req.color[3] * 255.0) as u8,
        );

        TextArea {
            buffer,
            left: physical_pos.x,
            top: physical_pos.y,
            scale: scale.factor(),
            bounds: TextBounds {
                left: bounds_left,
                top: bounds_top,
                right: bounds_right,
                bottom: bounds_bottom,
            },
            default_color: color_rgba_u8,
            custom_glyphs: &[],
        }
    }
}

impl Default for TextProcessor {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Add module export to lib.rs**

Add after `mod text_cache;`:

```rust
mod text_processor;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p vexo`
Expected: Success with no errors

- [ ] **Step 4: Commit**

```bash
git add vexo/src/text_processor.rs vexo/src/lib.rs
git commit -m "feat: add TextProcessor module for text rendering"
```

---

## Task 3: Create RenderPipeline Module

**Files:**
- Create: `vexo/src/render_pipeline.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Create render_pipeline.rs with RenderPipeline struct**

```rust
//! Render pipeline orchestration.
//!
//! Coordinates the render stages: layout, geometry, text, and GPU execution.

use glyphon::{FontSystem, TextArea};

use crate::core::{Logical, Point, Size, WidgetId};
use crate::frame_context::FrameContext;
use crate::layout::{LayoutContext, LayoutEngine, LayoutNodeId, LayoutView};
use crate::render::{RenderBackend, RenderError, WgpuBackend};
use crate::renderer::UiBatcher;
use crate::state::CursorBlinkState;
use crate::text_processor::TextProcessor;
use crate::widgets::{Widget, WidgetContext};

/// Output from layout computation stage.
pub struct LayoutOutput<'a> {
    pub layout_view: LayoutView<'a>,
    pub root_node: LayoutNodeId,
}

/// Orchestrates the render pipeline stages.
pub struct RenderPipeline {
    text_processor: TextProcessor,
}

impl RenderPipeline {
    /// Create a new render pipeline.
    pub fn new() -> Self {
        Self {
            text_processor: TextProcessor::new(),
        }
    }

    /// Stage 1: Compute layout for the widget tree.
    pub fn compute_layout<M: Clone + std::fmt::Debug + Send>(
        &mut self,
        widget: &mut dyn Widget<M>,
        layout_engine: &mut dyn LayoutEngine,
        _root_node: LayoutNodeId,
        viewport_logical: Size<Logical>,
        widget_context: &mut WidgetContext,
    ) -> LayoutOutput<'_> {
        // Build layout tree
        let mut layout_ctx = LayoutContext::new(layout_engine);
        let new_root_node = widget.layout(&mut layout_ctx, widget_context);

        // Compute layout
        layout_engine.compute(new_root_node, viewport_logical, &mut widget_context.font_system);

        let layout_view = LayoutView::new(layout_engine.as_ref());

        LayoutOutput {
            layout_view,
            root_node: new_root_node,
        }
    }

    /// Stage 2: Generate geometry from widget tree.
    pub fn generate_geometry<M: Clone + std::fmt::Debug + Send>(
        &mut self,
        widget: &dyn Widget<M>,
        batcher: &mut UiBatcher,
        root_node: LayoutNodeId,
        ctx: &FrameContext,
        widget_context: &mut WidgetContext,
    ) {
        widget.draw(
            &ctx.layout_view,
            root_node,
            batcher,
            Point::new(0.0, 0.0),
            ctx.focused_widget_id,
            ctx.cursor_blink,
            widget_context,
        );
    }

    /// Stage 3: Collect text areas from batcher.
    pub fn collect_text(
        &mut self,
        batcher: &mut UiBatcher,
        widget_context: &mut WidgetContext,
        scale: crate::core::Scale,
        viewport_physical: Size<crate::core::Physical>,
    ) -> Vec<TextArea> {
        // Process regular text requests
        let text_requests = std::mem::take(&mut batcher.text_requests);
        let mut text_areas = self.text_processor.process_text_requests(
            &mut widget_context.font_system,
            text_requests,
            scale,
            viewport_physical,
        );

        // Process editor requests
        let editor_requests = std::mem::take(&mut batcher.editor_requests);
        let editor_areas = self.text_processor.process_editor_requests(
            widget_context,
            editor_requests,
            scale,
        );

        // Combine text areas
        text_areas.extend(editor_areas);
        text_areas
    }

    /// Stage 4: Execute GPU render.
    pub fn execute_render(
        &mut self,
        backend: &mut WgpuBackend,
        batcher: &UiBatcher,
        text_areas: Vec<TextArea>,
        widget_context: &mut WidgetContext,
    ) -> Result<(), RenderError> {
        // Upload geometry
        backend.upload_geometry(batcher);

        // Prepare text
        backend.prepare_text(&mut widget_context.font_system, text_areas);

        // Execute render pass
        let instance_count = batcher.quad_instances.len();
        backend.execute_render_pass(instance_count)?;

        Ok(())
    }
}

impl Default for RenderPipeline {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Add module export to lib.rs**

Add after `mod frame_context;`:

```rust
mod render_pipeline;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p vexo`
Expected: Success with no errors

- [ ] **Step 4: Commit**

```bash
git add vexo/src/render_pipeline.rs vexo/src/lib.rs
git commit -m "feat: add RenderPipeline for stage orchestration"
```

---

## Task 4: Update WindowState to use RenderPipeline

**Files:**
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Update imports in window.rs**

Replace the imports at the top of `vexo/src/window.rs`:

```rust
use glyphon::{cosmic_text, TextBounds};
use std::sync::Arc;

use winit::{
    event_loop::ActiveEventLoop, keyboard::KeyCode, window::Window,
};

use crate::core::{Logical, Physical, Point, Scale, Size, WidgetId};
use crate::frame_context::FrameContext;
use crate::input::{CursorIcon, InputEvent};
use crate::layout::{LayoutContext, LayoutEngine, LayoutNodeId, LayoutView, TaffyLayoutEngine};
use crate::render::{RenderBackend, WgpuBackend};
use crate::render_pipeline::RenderPipeline;
use crate::renderer::TextRequest;
use crate::state::CursorBlinkState;
use crate::widgets::{Column, Widget, WidgetContext};
use crate::Application;
```

- [ ] **Step 2: Replace text_cache field with render_pipeline in WindowState struct**

In the `WindowState` struct (lines 18-44), replace:

```rust
    // Text buffer cache to avoid recreating/shaping every frame
    text_cache: TextCache,
```

With:

```rust
    // Render pipeline for orchestrating render stages
    render_pipeline: RenderPipeline,
```

- [ ] **Step 3: Update WindowState::new() to initialize RenderPipeline**

In the `new()` method (lines 63-88), replace:

```rust
            text_cache: TextCache::new(),
```

With:

```rust
            render_pipeline: RenderPipeline::new(),
```

- [ ] **Step 4: Verify compilation**

Run: `cargo build -p vexo`
Expected: Success (render method still uses old code, but struct is updated)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/window.rs
git commit -m "refactor: replace TextCache with RenderPipeline in WindowState"
```

---

## Task 5: Refactor render method to use RenderPipeline

**Files:**
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Replace the entire render method with the refactored version**

Replace lines 101-322 in `vexo/src/window.rs` with:

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
        let mut new_root_widget = self.view();

        // 4. Clear state
        self.layout_engine.clear();
        self.batcher.clear();

        // 5. Compute layout
        let scale = self.widget_context.scale;
        let logical_width = self.backend.width() as f32 / scale.factor();
        let logical_height = self.backend.height() as f32 / scale.factor();
        let logical_size = Size::<Logical>::new(logical_width, logical_height);

        self.batcher.set_screen_size(logical_size);

        let layout_output = self.render_pipeline.compute_layout(
            &mut *new_root_widget,
            self.layout_engine.as_mut(),
            self.root_node_id,
            logical_size,
            &mut self.widget_context,
        );

        self.root_widget = new_root_widget;
        self.root_node_id = layout_output.root_node;

        // 6. Build frame context
        let physical_size =
            Size::<Physical>::new(self.backend.width() as f32, self.backend.height() as f32);

        let ctx = FrameContext {
            scale,
            viewport_physical: physical_size,
            layout_view: layout_output.layout_view,
            focused_widget_id: self.focused_widget_id,
            cursor_blink: &self.cursor_blink,
        };

        // 7. Generate geometry
        self.render_pipeline.generate_geometry(
            &*self.root_widget,
            &mut self.batcher,
            self.root_node_id,
            &ctx,
            &mut self.widget_context,
        );

        // 8. Update viewport
        self.backend.update_viewport(physical_size);

        // 9. Collect text
        let text_areas = self.render_pipeline.collect_text(
            &mut self.batcher,
            &mut self.widget_context,
            scale,
            physical_size,
        );

        // 10. Execute render
        self.render_pipeline
            .execute_render(
                &mut self.backend,
                &self.batcher,
                text_areas,
                &mut self.widget_context,
            )
            .map_err(|e| match e {
                crate::render::RenderError::SurfaceNotConfigured => wgpu::SurfaceError::Lost,
                crate::render::RenderError::AcquireFailed(_) => wgpu::SurfaceError::Lost,
                crate::render::RenderError::TextPrepareFailed(_) => wgpu::SurfaceError::Lost,
                crate::render::RenderError::GpuError(_) => wgpu::SurfaceError::Lost,
            })?;

        Ok(())
    }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p vexo`
Expected: May have errors about `root_node()` method - check if it exists

- [ ] **Step 3: Fix any compilation errors**

The code already uses `layout_output.root_node` from the LayoutOutput struct defined in Task 3. No additional changes needed.

- [ ] **Step 4: Verify compilation**

Run: `cargo build -p vexo`
Expected: Success with no errors

- [ ] **Step 5: Commit**

```bash
git add vexo/src/window.rs
git commit -m "refactor: simplify render method using RenderPipeline"
```

---

## Task 6: Remove unused imports from window.rs

**Files:**
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Remove unused imports**

Remove these imports that are no longer needed in `vexo/src/window.rs`:

```rust
use glyphon::{cosmic_text, TextBounds};
use crate::renderer::TextRequest;
```

The imports should now be:

```rust
use std::sync::Arc;

use winit::{
    event_loop::ActiveEventLoop, keyboard::KeyCode, window::Window,
};

use crate::core::{Logical, Physical, Point, Scale, Size, WidgetId};
use crate::frame_context::FrameContext;
use crate::input::{CursorIcon, InputEvent};
use crate::layout::{LayoutContext, LayoutEngine, LayoutNodeId, LayoutView, TaffyLayoutEngine};
use crate::render::{RenderBackend, WgpuBackend};
use crate::render_pipeline::RenderPipeline;
use crate::state::CursorBlinkState;
use crate::widgets::{Column, Widget, WidgetContext};
use crate::Application;
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p vexo`
Expected: Success with no errors

- [ ] **Step 3: Commit**

```bash
git add vexo/src/window.rs
git commit -m "refactor: remove unused imports from window.rs"
```

---

## Task 7: Run tests and verify desktop demo

**Files:**
- None (verification only)

- [ ] **Step 1: Run all tests**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 2: Run desktop demo**

Run: `cargo run -p desktop_demo`
Expected: Window opens and renders correctly

- [ ] **Step 3: Commit any fixes if needed**

If any issues were found and fixed:

```bash
git add -A
git commit -m "fix: resolve issues found during testing"
```

---

## Task 8: Final cleanup and documentation

**Files:**
- Modify: `vexo/src/text_processor.rs`
- Modify: `vexo/src/render_pipeline.rs`

- [ ] **Step 1: Add doc comments to public APIs**

Ensure all public methods have doc comments. Verify existing comments are adequate.

- [ ] **Step 2: Run final build and test**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: Success

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "docs: add documentation for render pipeline modules"
```

---

## Verification Summary

After completing all tasks:

1. `cargo build -p vexo` — Compiles successfully
2. `cargo test -p vexo` — All tests pass
3. `cargo run -p desktop_demo` — Desktop demo renders correctly
4. `WindowState::render` is ~50 lines instead of ~220 lines
5. Each module has a single responsibility:
   - `FrameContext` — Shared frame data
   - `TextProcessor` — Text processing
   - `RenderPipeline` — Stage orchestration
