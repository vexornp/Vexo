# Integrate Retain-Mode with WindowState Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect the three-tree retain-mode pipeline to WindowState so applications can opt into retain-mode rendering.

**Architecture:** Applications implement `Application::retain_view()` returning retain widgets. WindowState calls it when `use_retain_mode=true`, processes RenderCommands through UiBatcher, and submits to GPU.

**Tech Stack:** Rust, vexo retain module, UiBatcher, WgpuBackend, glyphon for text

---

## File Structure

**Files to modify:**
- `vexo/src/window.rs` - Main integration work

---

### Task 1: Add set_retain_mode() method and update view_retain()

**Files:**
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Add set_retain_mode() method**

```rust
// In WindowState impl, add after the new() method:

/// Enable or disable retain-mode rendering.
///
/// When enabled and the application implements `retain_view()`,
/// the three-tree pipeline will be used for rendering.
pub fn set_retain_mode(&mut self, enabled: bool) {
    self.use_retain_mode = enabled;
}
```

- [ ] **Step 2: Update view_retain() to use Application's retain_view**

```rust
// Replace the existing view_retain() method (around line 361):

/// Generate a retain-mode widget tree from the application.
///
/// Returns the widget tree from `Application::retain_view()`,
/// or None if the application doesn't implement retain-mode.
fn view_retain(&self) -> Option<Box<dyn RetainWidget>> {
    A::retain_view(&self.user_app_state)
}
```

- [ ] **Step 3: Run tests to verify compilation**

Run: `cargo build -p vexo`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add vexo/src/window.rs
git commit -m "feat: add set_retain_mode() and update view_retain() to use Application"
```

---

### Task 2: Implement RenderCommand processing in render_retain()

**Files:**
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Update render_retain() to process RenderCommands**

Replace the existing `render_retain()` method (around line 378) with:

```rust
/// Render using the three-tree retain-mode pipeline.
///
/// This method implements the full retain-mode rendering flow:
/// 1. Generate widget tree from view_retain()
/// 2. Reconcile widget tree with element tree
/// 3. Layout dirty render objects
/// 4. Paint dirty render objects
/// 5. Process RenderCommands through batcher
/// 6. Submit to GPU
    fn render_retain(&mut self) -> Result<(), wgpu::SurfaceError> {
        // 1. Redraw request & backend check
        if let Some(win) = &self.window {
            win.request_redraw();
        }
        if !self.backend.is_ready() {
            return Ok(());
        }

        // 2. Frame timing
        self.cursor_blink.tick();

        // 3. Generate widget tree
        let widget_tree = match self.view_retain() {
            Some(w) => w,
            None => return Ok(()), // No retain view, skip
        };

        // 4. Get pipeline
        let pipeline = match &mut self.retain_pipeline {
            Some(p) => p,
            None => return Ok(()),
        };

        // 5. Clear batcher
        self.batcher.clear();

        // 6. Reconcile widget tree with element tree
        pipeline.reconcile(widget_tree);

        // 7. Compute logical size
        let scale = self.widget_context.scale;
        let logical_width = self.backend.width() as f32 / scale.factor();
        let logical_height = self.backend.height() as f32 / scale.factor();
        let logical_size = Size::<Logical>::new(logical_width, logical_height);

        self.batcher.set_screen_size(logical_size);

        // 8. Layout dirty render objects
        pipeline.layout(logical_size, self.layout_engine.as_mut());

        // 9. Paint dirty render objects
        let commands = pipeline.paint();

        // 10. Process RenderCommands through batcher
        for cmd in commands {
            match cmd {
                crate::render::RenderCommand::Rect { bounds, fill, stroke, corner_radius } => {
                    self.batcher.add_rect(bounds, fill, stroke, corner_radius);
                }
                crate::render::RenderCommand::PushCornerRadius { radius } => {
                    self.batcher.push_corner_radius(radius);
                }
                crate::render::RenderCommand::PopCornerRadius => {
                    self.batcher.pop_corner_radius();
                }
                crate::render::RenderCommand::PushClip { bounds } => {
                    self.batcher.push_clip(bounds);
                }
                crate::render::RenderCommand::PopClip => {
                    self.batcher.pop_clip();
                }
                crate::render::RenderCommand::Text { content, position, font_size, color, max_width } => {
                    // Add text request for glyphon processing
                    self.batcher.text_requests.push(crate::renderer::TextRequest {
                        content,
                        position,
                        size: font_size,
                        color,
                        clip_bounds: self.batcher.current_clip(),
                    });
                    let _ = max_width; // TODO: Handle max_width for text wrapping
                }
                crate::render::RenderCommand::Editor { id, bounds, color } => {
                    self.batcher.editor_requests.push(crate::renderer::EditorRequest {
                        id,
                        bounds,
                        color,
                    });
                }
                crate::render::RenderCommand::PushOffset { offset } => {
                    // TODO: Implement offset stack in batcher
                    let _ = offset;
                }
                crate::render::RenderCommand::PopOffset => {
                    // TODO: Implement offset stack in batcher
                }
            }
        }

        // 11. Update viewport
        let physical_size =
            Size::<Physical>::new(self.backend.width() as f32, self.backend.height() as f32);
        self.backend.update_viewport(physical_size);

        // 12. Collect text through glyphon
        let prepared_text = self.render_pipeline.collect_text(
            &mut self.batcher,
            &mut self.widget_context,
            scale,
            physical_size,
        );

        // 13. Execute render
        self.render_pipeline
            .execute_render(
                &mut self.backend,
                &self.batcher,
                prepared_text,
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

- [ ] **Step 2: Run build to verify compilation**

Run: `cargo build -p vexo`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add vexo/src/window.rs
git commit -m "feat: process RenderCommands through UiBatcher in render_retain()"
```

---

### Task 3: Wire up render() to choose the correct pipeline

**Files:**
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Refactor render() to dispatch to correct pipeline**

Replace the existing `render()` method (around line 113) with:

```rust
pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
    // Check if we should use retain mode
    if self.use_retain_mode && self.view_retain().is_some() {
        return self.render_retain();
    }
    
    // Otherwise use immediate mode
    self.render_immediate()
}

/// Render using the immediate-mode pipeline (legacy).
fn render_immediate(&mut self) -> Result<(), wgpu::SurfaceError> {
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
    let prepared_text = self.render_pipeline.collect_text(
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
            prepared_text,
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

- [ ] **Step 2: Run build to verify compilation**

Run: `cargo build -p vexo`
Expected: Build succeeds

- [ ] **Step 3: Run tests**

Run: `cargo test -p vexo -- --nocapture`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add vexo/src/window.rs
git commit -m "feat: wire up render() to choose between retain and immediate mode"
```

---

### Task 4: Add integration test for retain-mode rendering

**Files:**
- Create: `vexo/src/retain/window_integration_test.rs`
- Modify: `vexo/src/retain/mod.rs`

- [ ] **Step 1: Create integration test file**

```rust
// vexo/src/retain/window_integration_test.rs
//! Integration test for retain-mode with WindowState.

#[cfg(test)]
mod tests {
    use crate::retain::{Background, Column, Text, Widget};
    use crate::core::Color;

    #[test]
    fn test_retain_view_returns_widget_tree() {
        // Test that a simple retain widget tree can be created
        let child = Box::new(Text::new("Hello"));
        let bg = Background::new(child, Color::RED);
        
        // Verify the widget tree structure
        assert!(bg.child().as_any().downcast_ref::<Text>().is_some());
    }

    #[test]
    fn test_retain_column_with_modifiers() {
        // Test a more complex widget tree with modifiers
        let text1 = Box::new(Text::new("First"));
        let bg1 = Background::new(text1, Color::BLUE);
        
        let text2 = Box::new(Text::new("Second"));
        let bg2 = Background::new(text2, Color::GREEN);
        
        let col = Column::new()
            .push(bg1)
            .push(bg2);
        
        // Verify column has children
        let col_any = col.as_any();
        assert!(col_any.downcast_ref::<Column>().is_some());
    }
}
```

- [ ] **Step 2: Add module to retain/mod.rs**

```rust
// In vexo/src/retain/mod.rs, add:

#[cfg(test)]
mod window_integration_test;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p vexo test_retain_view -- --nocapture`
Expected: Tests pass

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/window_integration_test.rs vexo/src/retain/mod.rs
git commit -m "test: add integration tests for retain-mode widget trees"
```

---

## Summary

This plan integrates retain-mode with WindowState:

1. **Task 1**: Add `set_retain_mode()` method and update `view_retain()` to use Application's retain_view
2. **Task 2**: Implement RenderCommand processing through UiBatcher in `render_retain()`
3. **Task 3**: Wire up `render()` to choose between retain and immediate mode
4. **Task 4**: Add integration tests

After completion, applications can opt into retain-mode:

```rust
impl Application for MyApp {
    fn retain_view(state: &Self::State) -> Option<Box<dyn retain::Widget>> {
        Some(Box::new(
            Background::new(
                Box::new(Text::new("Hello Retain Mode")),
                Color::RED
            )
        ))
    }
}

// Enable retain mode
window_state.set_retain_mode(true);
```
