# Cursor Blink for TextEdit Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a blinking vertical bar cursor to the TextEdit widget that appears when focused, blinks at 800ms intervals, resets on typing, and hides when unfocused.

**Architecture:** Add `CursorBlinkState` to `WindowState` for frame-based timing. Pass blink state through the render pipeline to TextEdit's `draw()` method. Reset blink on keyboard input via `on_event()`.

**Tech Stack:** Rust, wgpu, glyphon, std::time::Instant

---

## File Structure

| File | Responsibility |
|------|----------------|
| `vexo/src/lib.rs` | `CursorBlinkState` struct, integrate into `WindowState`, update render loop and event handling |
| `vexo/src/widgets/mod.rs` | Add `cursor_blink` parameter to `Widget::draw()` trait signature |
| `vexo/src/widgets/text_edit.rs` | Add `cursor_color` field, render cursor in `draw()` when focused and visible |

---

### Task 1: Add CursorBlinkState to WindowState

**Files:**
- Modify: `vexo/src/lib.rs:53-91` (WindowState struct)
- Modify: `vexo/src/lib.rs:315-340` (WindowState::init return)

- [ ] **Step 1: Add CursorBlinkState struct and integrate into WindowState**

Add the `CursorBlinkState` struct and `cursor_blink` field to `WindowState`:

```rust
// Add near the top of lib.rs, after imports (around line 43)
use std::time::Instant;

// Add after WindowState struct definition (around line 91)
/// Tracks cursor blink timing for focused text inputs.
pub struct CursorBlinkState {
    /// Time of last tick (frame start)
    last_update: Instant,
    /// Accumulated milliseconds since last blink toggle
    accumulator_ms: f32,
    /// Whether cursor is currently visible (blink phase)
    visible: bool,
    /// Blink period in milliseconds (800ms default)
    blink_period_ms: f32,
}

impl CursorBlinkState {
    pub fn new() -> Self {
        Self {
            last_update: Instant::now(),
            accumulator_ms: 0.0,
            visible: true,
            blink_period_ms: 800.0,
        }
    }

    /// Call each frame to update blink state based on elapsed time.
    pub fn tick(&mut self) {
        let now = Instant::now();
        let elapsed_ms = (now - self.last_update).as_millis() as f32;
        self.last_update = now;
        self.accumulator_ms += elapsed_ms;

        // Toggle visibility each time we exceed the period
        while self.accumulator_ms >= self.blink_period_ms {
            self.accumulator_ms -= self.blink_period_ms;
            self.visible = !self.visible;
        }
    }

    /// Reset blink to visible state (call on keyboard input).
    pub fn reset(&mut self) {
        self.accumulator_ms = 0.0;
        self.visible = true;
        self.last_update = Instant::now();
    }

    /// Is cursor currently visible?
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}
```

- [ ] **Step 2: Add cursor_blink field to WindowState struct**

Add to `WindowState` struct (around line 90, after `widget_context`):

```rust
    // Cursor blink state (global - only one focused widget at a time)
    cursor_blink: CursorBlinkState,
```

- [ ] **Step 3: Initialize cursor_blink in WindowState::init**

In the `Ok(Self { ... })` block (around line 315-340), add:

```rust
            cursor_blink: CursorBlinkState::new(),
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p vexo`
Expected: Compilation succeeds (CursorBlinkState integrated but not yet used)

---

### Task 2: Pass CursorBlinkState through Widget::draw()

**Files:**
- Modify: `vexo/src/widgets/mod.rs:9-37` (Widget trait)
- Modify: `vexo/src/widgets/mod.rs:45-77` (Box<dyn Widget> impl)
- Modify: `vexo/src/lib.rs:403-410` (root_widget.draw call)

- [ ] **Step 1: Update Widget trait draw() signature**

Modify `Widget::draw()` in `vexo/src/widgets/mod.rs` (lines 18-26):

```rust
    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: taffy::NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    );
```

- [ ] **Step 2: Update Box<dyn Widget> implementation**

Modify the `impl<M> Widget<M> for Box<dyn Widget<M>>` draw() method (lines 54-64):

```rust
    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: taffy::NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
        (**self).draw(taffy, node, renderer, offset, focused_id, cursor_blink, ctx)
    }
```

- [ ] **Step 3: Update root_widget.draw() call in render()**

Modify `vexo/src/lib.rs` around line 403-410:

```rust
        // 1. DRAW RECTANGLES: Generate geometry data
        self.root_widget.draw(
            &mut self.taffy,
            self.root_node_id,
            &mut self.batcher,
            crate::utils::Point::new(0.0, 0.0),
            self.focused_widget_id,
            &self.cursor_blink,
            &mut self.widget_context,
        );
```

- [ ] **Step 4: Update all other Widget implementations to accept new parameter**

Update these widget files to add `cursor_blink: &crate::CursorBlinkState` parameter and prefix with `_` to suppress unused warning:

- `vexo/src/widgets/button.rs` - add `_cursor_blink: &crate::CursorBlinkState,` after `focused_id`
- `vexo/src/widgets/text.rs` - add `_cursor_blink: &crate::CursorBlinkState,` after `focused_id`
- `vexo/src/widgets/color_widget.rs` - add `_cursor_blink: &crate::CursorBlinkState,` after `focused_id`
- `vexo/src/widgets/containers.rs` - update both Row and Column draw() methods, pass cursor_blink to children

For containers.rs, the child draw call becomes:
```rust
                child.draw(taffy, child_node, renderer, child_offset, focused_id, cursor_blink, ctx);
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p vexo`
Expected: Compilation succeeds

---

### Task 3: Add cursor_color field to TextEdit

**Files:**
- Modify: `vexo/src/widgets/text_edit.rs:13-19` (TextEdit struct)
- Modify: `vexo/src/widgets/text_edit.rs:22-30` (TextEdit::new)

- [ ] **Step 1: Add cursor_color field to TextEdit struct**

Modify struct definition (lines 13-19):

```rust
pub struct TextEdit {
    pub editor_id: String,
    pub initial_text: String,
    pub swash_cache: SwashCache,
    pub text_color: Color,
    pub cursor_color: Color,
    pub key: Option<String>,
}
```

- [ ] **Step 2: Initialize cursor_color in TextEdit::new()**

Modify `new()` function (lines 22-30):

```rust
    pub fn new(id: impl Into<String>, initial_text: impl Into<String>) -> Self {
        Self {
            editor_id: id.into(),
            initial_text: initial_text.into(),
            swash_cache: SwashCache::new(),
            text_color: Color::WHITE,
            cursor_color: Color::new(0.3, 0.67, 0.97, 1.0), // Accent blue
            key: None,
        }
    }
```

- [ ] **Step 3: Add with_cursor_color builder method**

Add after `with_key` method (around line 36):

```rust
    pub fn with_cursor_color(mut self, color: Color) -> Self {
        self.cursor_color = color;
        self
    }
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p vexo`
Expected: Compilation succeeds

---

### Task 4: Implement cursor rendering in TextEdit::draw()

**Files:**
- Modify: `vexo/src/widgets/text_edit.rs:54-93` (TextEdit::draw)

- [ ] **Step 1: Update draw() signature and render cursor**

Replace the entire `draw()` method implementation:

```rust
    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
        use crate::utils::{Logical, Point, Rect, Size};

        let layout = taffy.layout(node).unwrap();
        let pos: Point<Logical> = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );
        let size: Size<Logical> = Size::new(layout.size.width, layout.size.height);

        // Debug border
        let debug_color = crate::Color::RED;
        renderer.add_rect(pos.to_array(), size.to_array(), crate::Color::BLACK, debug_color, 1.0, 0.0);

        let editor_arc = ctx.get_or_create_editor(&self.editor_id, &self.initial_text);
        let mut editor_ref = editor_arc.borrow_mut();

        editor_ref.set_size(&mut ctx.font_system, size.width, size.height);
        editor_ref.shape_as_needed(&mut ctx.font_system, true);

        renderer.add_editor_request(
            &self.editor_id,
            Rect::new(pos, size),
        );

        // Render cursor if focused and visible
        let my_id = WidgetId::from_key(&self.editor_id);
        let is_focused = focused_id == Some(my_id);

        if is_focused && cursor_blink.is_visible() {
            // Get cursor position from the editor buffer
            let buffer = editor_ref.buffer();

            // Get cursor line and index
            let cursor_line = buffer.lines[buffer.cursor.line as usize].as_ref().unwrap();
            let cursor_x_index = buffer.cursor.index as usize;

            // Calculate cursor X position by measuring text up to cursor
            // The cursor is positioned after the character at cursor.index
            let mut cursor_x = 0.0f32;
            for (i, glyph) in cursor_line.glyphs().iter().enumerate() {
                if i >= cursor_x_index {
                    break;
                }
                cursor_x += glyph.w;
            }

            // Cursor Y position based on line
            let line_height = size.height / buffer.lines.len() as f32;
            let cursor_y = pos.y + (buffer.cursor.line as f32) * line_height;

            // Draw vertical bar cursor (2 logical pixels wide)
            let cursor_width = 2.0;
            let cursor_height = line_height;

            renderer.add_rect(
                [pos.x + cursor_x, cursor_y],
                [cursor_width, cursor_height],
                self.cursor_color,
                crate::Color::TRANSPARENT, // No border
                0.0, // No border width
                0.0, // No corner radius
            );
        }
    }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p vexo`
Expected: Compilation succeeds

---

### Task 5: Integrate blink timing into render loop and event handling

**Files:**
- Modify: `vexo/src/lib.rs:366-410` (render method)
- Modify: `vexo/src/lib.rs:659-698` (handle_window_event)

- [ ] **Step 1: Add cursor_blink.tick() at start of render()**

In `WindowState::render()` method, add after the early return check (around line 374):

```rust
        // Update cursor blink state
        self.cursor_blink.tick();
```

- [ ] **Step 2: Reset cursor blink on keyboard input**

In `handle_window_event()`, modify the section after `widget_response` is obtained (around line 673). Add reset when keyboard input is handled:

```rust
        // Reset cursor blink on keyboard input
        if widget_response.handled {
            if let WindowEvent::KeyboardInput { .. } = event {
                self.cursor_blink.reset();
            }
        }
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p vexo`
Expected: Compilation succeeds

---

### Task 6: Build and test

**Files:**
- Test: Run desktop demo

- [ ] **Step 1: Build the project**

Run: `cargo build -p desktop_demo`
Expected: Build succeeds without errors

- [ ] **Step 2: Run desktop demo and verify cursor behavior**

Run: `cargo run -p desktop_demo`

Manual verification checklist:
1. Click on TextEdit widget to focus — cursor should appear
2. Wait 800ms — cursor should blink (disappear, then reappear)
3. Type characters — cursor should stay visible, then start blinking after you stop
4. Click outside TextEdit — cursor should disappear
5. Verify cursor is a vertical bar with accent blue color

- [ ] **Step 3: Commit the changes**

```bash
git add -A
git commit -m "feat(widgets): add blinking cursor to TextEdit widget

- Add CursorBlinkState for frame-based blink timing (800ms period)
- Pass blink state through Widget::draw() trait
- Render vertical bar cursor in TextEdit when focused
- Reset blink on keyboard input for immediate visibility"
```

---

## Verification

After all tasks complete:
1. `cargo build -p desktop_demo` succeeds
2. Running the demo shows blinking cursor in TextEdit
3. Cursor resets to visible on typing
4. Cursor hides when unfocused
