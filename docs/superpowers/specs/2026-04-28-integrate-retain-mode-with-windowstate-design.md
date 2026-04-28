# Integrate Retain-Mode with WindowState Design

**Date:** 2026-04-28
**Status:** Design Approved

## Goal

Connect the three-tree retain-mode pipeline to WindowState so applications can opt into retain-mode rendering via `Application::retain_view()`.

## Current State

The infrastructure already exists:
- `Application::retain_view()` returns `Option<Box<dyn retain::Widget>>` (defaults to `None`)
- `WindowState.retain_pipeline: Option<ThreeTreePipeline>` is created
- `WindowState.render_retain()` exists but is incomplete (doesn't process RenderCommands)
- `use_retain_mode: bool` flag exists but is hardcoded to `false`

## What Needs to Be Done

### 1. Process RenderCommands through UiBatcher

The `render_retain()` method gets `Vec<RenderCommand>` from `pipeline.paint()` but doesn't process them. Convert each command to batcher operations:

```rust
for cmd in commands {
    match cmd {
        RenderCommand::Rect { bounds, fill, stroke, corner_radius } => {
            self.batcher.add_rect(bounds, fill, stroke, corner_radius);
        }
        RenderCommand::PushCornerRadius { radius } => {
            self.batcher.push_corner_radius(radius);
        }
        RenderCommand::PopCornerRadius => {
            self.batcher.pop_corner_radius();
        }
        RenderCommand::PushClip { bounds } => {
            // Clip handling
        }
        RenderCommand::PopClip => {
            // Clip handling
        }
        RenderCommand::Text { .. } => {
            // Text handled by glyphon separately
        }
        // ... other commands
    }
}
```

### 2. Enable `use_retain_mode` Configuration

Add a method to enable retain mode at runtime:

```rust
impl<A: Application> WindowState<A> {
    pub fn set_retain_mode(&mut self, enabled: bool) {
        self.use_retain_mode = enabled;
    }
}
```

### 3. Update `view_retain()` to Use Application's Retain View

Change from placeholder to actual application view:

```rust
fn view_retain(&self) -> Option<Box<dyn RetainWidget>> {
    A::retain_view(&self.user_app_state)
}
```

### 4. Wire Up `render()` to Choose Pipeline

Refactor `render()` to dispatch to the correct pipeline:

```rust
pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
    if self.use_retain_mode && self.view_retain().is_some() {
        self.render_retain()
    } else {
        self.render_immediate()
    }
}
```

### 5. Handle Text Rendering

Text commands need special handling through glyphon. The immediate-mode path uses `render_pipeline.collect_text()`. For retain-mode, we need similar text collection from the widget tree.

## Files to Modify

- `vexo/src/window.rs` - Main integration work
- `vexo/src/lib.rs` - Possibly update Application trait docs

## Success Criteria

1. Application can implement `retain_view()` returning retain widgets
2. Setting `use_retain_mode = true` switches to retain pipeline
3. RenderCommands are processed through UiBatcher
4. Rectangles, borders, and corner radius modifiers render correctly
5. Text renders through glyphon
6. Existing immediate-mode apps continue to work unchanged

## Out of Scope

- Event handling in retain-mode (future work)
- State preservation across frames
- Performance optimization
- Full widget migration
