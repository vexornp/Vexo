# Retain-Mode TextEdit Cursor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a blinking accent-blue vertical cursor in the focused retain-mode TextEdit widget.

**Architecture:** Following Flutter's `RenderEditable` pattern, a single `TextEditRenderObject` leaf paints both text content and cursor in one `paint()` call. The `ThreeTreePipeline` owns `CursorBlinkState` and injects focus/blink state into render objects before each paint traversal. A new `RenderCommand::Caret` variant carries cursor position/height/color to the command processor, which draws it as a 2px-wide rect.

**Tech Stack:** Rust, glyphon (text/cursor position), Taffy (layout), existing UiBatcher (rect rendering)

---

### Task 1: Add `RenderCommand::Caret` variant

**Files:**
- Modify: `vexo/src/render/command.rs:25-91`

- [ ] **Step 1: Add the `Caret` variant to the `RenderCommand` enum**

In `vexo/src/render/command.rs`, add the `Caret` variant after the `Text` variant:

```rust
/// Draw a text cursor (caret) at a position.
Caret {
    /// Top-left position of the cursor bar in logical coordinates.
    position: Point<Logical>,
    /// Height of the cursor bar (line height).
    height: f32,
    /// Cursor color.
    color: Color,
},
```

- [ ] **Step 2: Add a `caret()` convenience constructor**

Add after the `editor()` method in the `impl RenderCommand` block:

```rust
/// Create a caret (cursor) command.
pub fn caret(position: Point<Logical>, height: f32, color: Color) -> Self {
    Self::Caret {
        position,
        height,
        color,
    }
}
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | head -30`
Expected: Build fails in `command_processor.rs` because the match is non-exhaustive (missing `Caret` arm). This is expected — we'll fix it in Task 2.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/render/command.rs
git commit -m "feat: add RenderCommand::Caret variant for cursor rendering"
```

---

### Task 2: Handle `Caret` in `CommandProcessor`

**Files:**
- Modify: `vexo/src/render/command_processor.rs:30-101`

- [ ] **Step 1: Write the failing test**

In `vexo/src/render/command_processor.rs`, add to the `tests` module:

```rust
#[test]
fn test_process_caret_command() {
    let mut batcher = UiBatcher::new();
    let cursor_color = Color::rgb(0.3, 0.67, 0.97);
    let commands = vec![RenderCommand::caret(
        Point::new(50.0, 10.0),
        20.0,
        cursor_color,
    )];

    process_commands(&commands, &mut batcher, Point::new(0.0, 0.0));

    // Caret should be rendered as a 2px-wide rect
    assert_eq!(batcher.quad_instances.len(), 1);
    let quad = &batcher.quad_instances[0];
    assert_eq!(quad.position, [50.0, 10.0]);
    assert_eq!(quad.size, [2.0, 20.0]);
    assert_eq!(quad.color, cursor_color.to_array());
}

#[test]
fn test_process_caret_with_offset() {
    let mut batcher = UiBatcher::new();
    let cursor_color = Color::rgb(0.3, 0.67, 0.97);
    let commands = vec![RenderCommand::caret(
        Point::new(10.0, 5.0),
        20.0,
        cursor_color,
    )];

    process_commands(&commands, &mut batcher, Point::new(100.0, 50.0));

    let quad = &batcher.quad_instances[0];
    assert_eq!(quad.position, [110.0, 55.0]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_process_caret_command 2>&1 | tail -10`
Expected: FAIL — `Caret` arm missing in match.

- [ ] **Step 3: Add the `Caret` arm to `process_commands`**

In `vexo/src/render/command_processor.rs`, add inside the `match cmd` block, after the `Editor` arm:

```rust
RenderCommand::Caret {
    position,
    height,
    color,
} => {
    let pos = Point::new(
        position.x + current_offset.x,
        position.y + current_offset.y,
    );
    let bounds = Bounds::from_xywh(pos.x, pos.y, 2.0, *height);
    batcher.add_rect(bounds, *color, None, 0.0);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_process_caret 2>&1 | tail -10`
Expected: PASS (both `test_process_caret_command` and `test_process_caret_with_offset`)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/render/command_processor.rs
git commit -m "feat: handle RenderCommand::Caret in CommandProcessor as 2px rect"
```

---

### Task 3: Add `cursor_position()` and `line_height()` to `TextEditingController`

**Files:**
- Modify: `vexo/src/retain/widgets/text_edit.rs:39-160`

- [ ] **Step 1: Write the failing test**

In `vexo/src/retain/widgets/text_edit.rs`, add to the `tests` module:

```rust
#[test]
fn test_controller_cursor_position() {
    let mut fs = create_test_font_system();
    let controller = TextEditingController::new("Hello", &mut fs);
    // After initialization, cursor should be at some position
    let pos = controller.cursor_position();
    // cursor_position returns Option<(i32, i32)>
    // The cursor exists after text is set, so it should be Some
    assert!(pos.is_some(), "cursor_position should return Some after text is set");
}

#[test]
fn test_controller_line_height() {
    let mut fs = create_test_font_system();
    let controller = TextEditingController::new("Hello", &mut fs);
    let lh = controller.line_height();
    // Line height should be positive (metrics are 16.0 font_size, 20.0 line_height)
    assert!(lh > 0.0, "line_height should be positive, got {}", lh);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo test_controller_cursor_position 2>&1 | tail -10`
Expected: FAIL — method `cursor_position` not found on `TextEditingController`.

- [ ] **Step 3: Implement the two methods**

In `vexo/src/retain/widgets/text_edit.rs`, add to the `impl TextEditingController` block, after the `editor()` method (around line 86):

```rust
/// Get the cursor position in buffer-relative coordinates.
///
/// Returns `Some((x, y))` in pixels relative to the buffer origin,
/// or `None` if the cursor position cannot be determined.
pub fn cursor_position(&self) -> Option<(i32, i32)> {
    self.editor.borrow().cursor_position()
}

/// Get the line height from the editor buffer metrics.
///
/// Used to determine the cursor bar height.
pub fn line_height(&self) -> f32 {
    self.editor.borrow().buffer().metrics().line_height
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo test_controller_cursor_position test_controller_line_height 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/widgets/text_edit.rs
git commit -m "feat: add cursor_position() and line_height() to TextEditingController"
```

---

### Task 4: Create `TextEditRenderObject`

**Files:**
- Create: `vexo/src/retain/render_objects/text_edit.rs`
- Modify: `vexo/src/retain/render_objects/mod.rs`

- [ ] **Step 1: Write the `TextEditRenderObject` implementation**

Create `vexo/src/retain/render_objects/text_edit.rs`:

```rust
//! TextEditRenderObject - renders text content and cursor for TextEdit.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position, Size};
use crate::layout::{Layout, LayoutNodeKey, MeasureContext, TextMeasureContext};
use crate::render::RenderCommand;
use crate::retain::{HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject};
use crate::editor::Editor;

/// Accent blue cursor color, matching legacy TextEdit.
const CURSOR_COLOR: Color = Color::rgb(0.3, 0.67, 0.97);

/// Cursor bar width in logical pixels.
const CURSOR_WIDTH: f32 = 2.0;

/// RenderObject for TextEdit content (text + cursor).
///
/// Following Flutter's RenderEditable pattern, this single leaf render object
/// paints both the text content and the blinking cursor in one `paint()` call.
/// Text is painted first, cursor second, giving correct z-order.
pub struct TextEditRenderObject {
    // Text rendering (same as TextRenderObject)
    content: String,
    font_size: f32,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,

    // Cursor rendering
    editor: Rc<RefCell<Editor>>,
    is_focused: bool,
    cursor_blink_visible: bool,
}

impl TextEditRenderObject {
    /// Create a new TextEditRenderObject.
    pub fn new(
        content: &str,
        font_size: f32,
        editor: Rc<RefCell<Editor>>,
    ) -> Self {
        Self {
            content: content.to_string(),
            font_size,
            computed_bounds: None,
            layout_node: None,
            editor,
            is_focused: false,
            cursor_blink_visible: false,
        }
    }

    /// Set the text content. Returns true if changed.
    pub fn set_content(&mut self, content: &str) -> bool {
        let changed = self.content != content;
        if changed {
            self.content = content.to_string();
        }
        changed
    }

    /// Set the font size. Returns true if changed.
    pub fn set_font_size(&mut self, size: f32) -> bool {
        if (self.font_size - size).abs() > f32::EPSILON {
            self.font_size = size;
            true
        } else {
            false
        }
    }

    /// Set whether this TextEdit is focused.
    pub fn set_focused(&mut self, focused: bool) {
        self.is_focused = focused;
    }

    /// Set whether the cursor blink is currently visible.
    pub fn set_cursor_blink_visible(&mut self, visible: bool) {
        self.cursor_blink_visible = visible;
    }

    /// Get the text content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the font size.
    pub fn font_size(&self) -> f32 {
        self.font_size
    }
}

impl RenderObject for TextEditRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let measure_ctx = MeasureContext::Text(TextMeasureContext {
            content: self.content.clone(),
            font_size: self.font_size,
            line_height: 1.2,
        });

        let node = ctx.engine().create_leaf_with_context(
            &Layout::default(),
            measure_ctx,
        );

        self.layout_node = Some(node);

        LayoutResult {
            node,
            size: Size::new(0.0, 0.0),
        }
    }

    fn apply_layout(&mut self, ctx: &LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        let bounds = match &self.computed_bounds {
            Some(b) => b,
            None => return vec![],
        };

        let pos: Position<Logical, Absolute> = ctx.absolute_position();
        let mut commands = Vec::new();

        // 1. Paint text content
        commands.push(RenderCommand::Text {
            content: self.content.clone(),
            position: pos.to_point(),
            font_size: self.font_size,
            color: Color::BLACK,
            max_width: Some(bounds.width()),
        });

        // 2. Paint cursor if focused and blink visible
        if self.is_focused && self.cursor_blink_visible {
            let editor = self.editor.borrow();
            if let Some((cursor_x, cursor_y)) = editor.cursor_position() {
                let line_height = editor.buffer().metrics().line_height;
                let abs_x = cursor_x as f32 + pos.x;
                let abs_y = cursor_y as f32 + pos.y;
                commands.push(RenderCommand::Caret {
                    position: Point::new(abs_x, abs_y),
                    height: line_height,
                    color: CURSOR_COLOR,
                });
            }
        }

        commands
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}
```

- [ ] **Step 2: Register the module in `mod.rs`**

In `vexo/src/retain/render_objects/mod.rs`, add `text_edit` module and re-export:

```rust
mod text;
mod text_edit;
mod container;

pub use text::TextRenderObject;
pub use text_edit::TextEditRenderObject;
pub use container::ContainerRenderObject;
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: PASS (builds successfully)

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/render_objects/text_edit.rs vexo/src/retain/render_objects/mod.rs
git commit -m "feat: add TextEditRenderObject with text + cursor painting"
```

---

### Task 5: Create `TextEditContent` widget

**Files:**
- Create: `vexo/src/retain/widgets/text_edit_content.rs`
- Modify: `vexo/src/retain/widgets/mod.rs`

- [ ] **Step 1: Write the `TextEditContent` widget**

Create `vexo/src/retain/widgets/text_edit_content.rs`:

```rust
//! TextEditContent widget - leaf widget that creates TextEditRenderObject.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use super::{Element, Widget};
use super::super::key::WidgetKey;
use super::super::RenderObject;
use super::super::render_objects::TextEditRenderObject;
use super::super::UpdateResult;
use crate::editor::Editor;

/// Leaf widget that creates a TextEditRenderObject.
///
/// This widget is produced by `TextEdit::build()` as the child of
/// `DecoratedContainer`. It carries the data the render object needs:
/// text content, font size, editor reference, focus state, and blink state.
pub struct TextEditContent {
    key: Option<WidgetKey>,
    content: String,
    font_size: f32,
    editor: Rc<RefCell<Editor>>,
    is_focused: bool,
    cursor_blink_visible: bool,
}

impl TextEditContent {
    /// Create a new TextEditContent widget.
    pub fn new(
        content: String,
        font_size: f32,
        editor: Rc<RefCell<Editor>>,
        is_focused: bool,
        cursor_blink_visible: bool,
    ) -> Self {
        Self {
            key: None,
            content,
            font_size,
            editor,
            is_focused,
            cursor_blink_visible,
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Clone for TextEditContent {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content: self.content.clone(),
            font_size: self.font_size,
            editor: self.editor.clone(),
            is_focused: self.is_focused,
            cursor_blink_visible: self.cursor_blink_visible,
        }
    }
}

impl Widget for TextEditContent {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::retain::elements::LeafElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        let mut ro = TextEditRenderObject::new(
            &self.content,
            self.font_size,
            self.editor.clone(),
        );
        ro.set_focused(self.is_focused);
        ro.set_cursor_blink_visible(self.cursor_blink_visible);
        Box::new(ro)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(ro) = render_object.as_any_mut().downcast_mut::<TextEditRenderObject>() {
            let mut result = UpdateResult::NONE;
            if ro.set_content(&self.content) {
                result |= UpdateResult::LAYOUT | UpdateResult::PAINT;
            }
            if ro.set_font_size(self.font_size) {
                result |= UpdateResult::LAYOUT | UpdateResult::PAINT;
            }
            // Focus and blink state changes require repaint
            ro.set_focused(self.is_focused);
            ro.set_cursor_blink_visible(self.cursor_blink_visible);
            result |= UpdateResult::PAINT;
            result
        } else {
            UpdateResult::ALL
        }
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}
```

- [ ] **Step 2: Register the module in `widgets/mod.rs`**

In `vexo/src/retain/widgets/mod.rs`, add the `text_edit_content` module and re-export:

Add `mod text_edit_content;` after `mod text_edit;`

Add `pub use text_edit_content::TextEditContent;` after the `pub use text_edit::...` line.

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/widgets/text_edit_content.rs vexo/src/retain/widgets/mod.rs
git commit -m "feat: add TextEditContent leaf widget for TextEditRenderObject"
```

---

### Task 6: Update `TextEdit::build()` to use `TextEditContent`

**Files:**
- Modify: `vexo/src/retain/widgets/text_edit.rs:338-369`

- [ ] **Step 1: Update `TextEdit::build()` to return `DecoratedContainer(TextEditContent)`**

Replace the `build()` method body (lines 341-368) with:

```rust
fn build(
    &self,
    _state: &mut TextEditState,
    ctx: &mut BuildContext,
) -> Box<dyn Widget> {
    let is_focused = ctx.is_focused();

    let border_color = if is_focused {
        crate::core::Color::rgb(0.2, 0.4, 0.8) // Blue border when focused
    } else {
        crate::core::Color::rgb(0.6, 0.6, 0.6) // Gray border when unfocused
    };

    let border_width = if is_focused { 2.0 } else { 1.0 };

    let style = crate::retain::Style::new()
        .background(crate::core::Color::WHITE)
        .border(border_color, border_width)
        .corner_radius(4.0)
        .padding(8.0);

    Box::new(
        crate::retain::DecoratedContainer::new(
            Box::new(super::TextEditContent::new(
                self.controller.text(),
                self.controller.font_size(),
                self.controller.editor(),
                is_focused,
                false, // cursor_blink_visible — updated by pipeline before paint
            ))
        )
        .style(style)
    )
}
```

- [ ] **Step 2: Build and run tests to verify**

Run: `cargo test -p vexo 2>&1 | tail -20`
Expected: All existing tests pass. The `test_text_edit_reconcile_in_pipeline` test may need updating since the element count changes (TextEditContent replaces Text as the leaf child, but element count stays the same: StatefulElement + DecoratedContainer + LeafElement = 3).

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/text_edit.rs
git commit -m "feat: TextEdit::build() uses TextEditContent instead of Text"
```

---

### Task 7: Add `CursorBlinkState` to `ThreeTreePipeline`

**Files:**
- Modify: `vexo/src/retain/pipeline.rs`

- [ ] **Step 1: Add `CursorBlinkState` field to `ThreeTreePipeline`**

In `vexo/src/retain/pipeline.rs`, add the import and field:

Add import at top:
```rust
use crate::state::CursorBlinkState;
```

Add field to `ThreeTreePipeline` struct (after `needs_full_reconcile`):
```rust
/// Cursor blink state for text editing cursors.
cursor_blink: CursorBlinkState,
```

Initialize in `new()`:
```rust
cursor_blink: CursorBlinkState::new(),
```

- [ ] **Step 2: Add public accessor methods**

Add to `impl ThreeTreePipeline`:

```rust
/// Tick the cursor blink state. Call once per frame.
pub fn tick_cursor_blink(&mut self) {
    self.cursor_blink.tick();
}

/// Reset cursor blink to visible. Call on keyboard input.
pub fn reset_cursor_blink(&mut self) {
    self.cursor_blink.reset();
}

/// Check if the cursor blink is currently visible.
pub fn cursor_blink_visible(&self) -> bool {
    self.cursor_blink.is_visible()
}
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/pipeline.rs
git commit -m "feat: add CursorBlinkState to ThreeTreePipeline"
```

---

### Task 8: Add `prepare_cursor_state()` to pipeline

**Files:**
- Modify: `vexo/src/retain/pipeline.rs`

- [ ] **Step 1: Implement `prepare_cursor_state()`**

Add to `impl ThreeTreePipeline`:

```rust
/// Inject focus and cursor blink state into TextEditRenderObjects.
///
/// Called between layout and paint. Walks the render object tree,
/// finds TextEditRenderObject instances, and sets their focus/blink state.
/// This avoids adding these fields to PaintContext (which every render
/// object would see).
pub fn prepare_cursor_state(&mut self) {
    let focused_element = self.focus_manager.primary_focus_element();
    let blink_visible = self.cursor_blink.is_visible();

    // Walk all render objects and update TextEditRenderObject instances
    for (_, ro) in self.render_objects.iter_mut() {
        if let Some(text_edit_ro) = ro.as_any_mut().downcast_mut::<crate::retain::render_objects::TextEditRenderObject>() {
            // Check if this render object's owning element is focused
            // For now, set is_focused based on whether any element is focused
            // The precise element matching will be done via the element_map
            text_edit_ro.set_focused(false); // Will be set correctly below
            text_edit_ro.set_cursor_blink_visible(blink_visible);
        }
    }

    // Set is_focused on the specific TextEditRenderObject that belongs
    // to the focused element
    if let Some(focused_key) = focused_element {
        // Find the render object for the focused element
        // Walk the element tree to find the render object
        if let Some(ro_key) = self.element_registry.render_object_for(focused_key) {
            if let Some(ro) = self.render_objects.get_mut(ro_key) {
                // The focused element's render object is a ProxyRenderObject (StatefulElement).
                // We need to find the TextEditRenderObject in its subtree.
                // Walk children to find it.
                Self::set_cursor_focus_in_subtree(&mut self.render_objects, ro_key, blink_visible);
            }
        }
    }
}

/// Recursively walk a render object subtree to find and focus TextEditRenderObjects.
fn set_cursor_focus_in_subtree(
    render_objects: &mut RenderObjectRegistry,
    root: RenderObjectKey,
    blink_visible: bool,
) {
    if let Some(ro) = render_objects.get_mut(root) {
        if let Some(text_edit_ro) = ro.as_any_mut().downcast_mut::<crate::retain::render_objects::TextEditRenderObject>() {
            text_edit_ro.set_focused(true);
            text_edit_ro.set_cursor_blink_visible(blink_visible);
            return; // Found it, no need to go deeper
        }

        // Recurse into children
        let children: Vec<_> = render_objects.get(root)
            .map(|r| r.children().to_vec())
            .unwrap_or_default();
        for child in children {
            Self::set_cursor_focus_in_subtree(render_objects, child, blink_visible);
        }
    }
}
```

- [ ] **Step 2: Add `iter_mut()` to `RenderObjectRegistry` and `render_object_for()` to `ElementRegistry`**

Check if these methods exist. If not, add them.

For `RenderObjectRegistry::iter_mut()` in `vexo/src/retain/render_object.rs`:
```rust
/// Iterate mutably over all render objects.
pub fn iter_mut(&mut self) -> impl Iterator<Item = (RenderObjectKey, &mut Box<dyn RenderObject>)> {
    self.objects.iter_mut()
}
```

For `ElementRegistry::render_object_for()` — check if it exists. If not, add it in `vexo/src/retain/element.rs` or the element registry module. This method maps an `ElementKey` to its `RenderObjectKey`.

- [ ] **Step 3: Build and fix compilation errors**

Run: `cargo build -p vexo 2>&1 | tail -20`
Expected: May have compilation errors if `iter_mut()` or `render_object_for()` don't exist. Fix as needed.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/retain/pipeline.rs vexo/src/retain/render_object.rs
git commit -m "feat: add prepare_cursor_state() to inject focus/blink into TextEditRenderObjects"
```

---

### Task 9: Wire cursor blink into the frame loop

**Files:**
- Modify: `vexo/src/window.rs`

- [ ] **Step 1: Call `pipeline.tick_cursor_blink()` in `render_retain()`**

In `vexo/src/window.rs`, in the `render_retain()` method, the existing `self.cursor_blink.tick()` call at line 472 already ticks the legacy blink state. Add a corresponding tick for the pipeline:

After `self.cursor_blink.tick()`, add:
```rust
if let Some(ref mut pipeline) = self.retain_pipeline {
    pipeline.tick_cursor_blink();
}
```

- [ ] **Step 2: Call `pipeline.reset_cursor_blink()` on keyboard events in retain mode**

In `vexo/src/window.rs`, in `process_input_event_retain()`, after the `pipeline.handle_event()` call, add cursor blink reset when the event was a keyboard event that was handled:

```rust
if matches!(input_event, InputEvent::Keyboard { .. }) && result.is_some() {
    if let Some(ref mut pipeline) = self.retain_pipeline {
        pipeline.reset_cursor_blink();
    }
}
```

- [ ] **Step 3: Call `pipeline.prepare_cursor_state()` before `pipeline.paint()`**

In `render_retain()`, before the `pipeline.paint()` call, add:
```rust
pipeline.prepare_cursor_state();
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/window.rs
git commit -m "feat: wire cursor blink tick/reset/prepare into retain frame loop"
```

---

### Task 10: Write unit tests for `TextEditRenderObject` cursor painting

**Files:**
- Modify: `vexo/src/retain/render_objects/text_edit.rs`

- [ ] **Step 1: Write test: paint emits Caret when focused + blink visible**

Add to the test module in `vexo/src/retain/render_objects/text_edit.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Layout, LayoutEngine, TaffyLayoutEngine};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    fn create_test_editor(text: &str) -> Rc<RefCell<Editor>> {
        let mut fs = create_test_font_system();
        let metrics = glyphon::Metrics::new(16.0, 20.0);
        let mut raw_editor = glyphon::Editor::new(glyphon::Buffer::new_empty(metrics));
        raw_editor.with_buffer_mut(|buffer| {
            buffer.set_text(&mut fs, text, &glyphon::Attrs::new(), glyphon::Shaping::Advanced);
        });
        raw_editor.with_buffer_mut(|buffer| {
            buffer.shape_until_scroll(&mut fs, true);
        });
        Rc::new(RefCell::new(Editor::new(raw_editor)))
    }

    #[test]
    fn test_paint_emits_caret_when_focused_and_blink_visible() {
        let editor = create_test_editor("Hello");
        let mut ro = TextEditRenderObject::new("Hello", 16.0, editor);
        ro.set_focused(true);
        ro.set_cursor_blink_visible(true);

        // Layout the render object so it has computed_bounds
        let mut engine = TaffyLayoutEngine::new();
        let mut fs = create_test_font_system();
        {
            let mut ctx = LayoutContext::new(&mut engine, &mut fs);
            let _ = ro.layout(&mut ctx, &[]);
        }
        let root = engine.create_leaf(&Layout::default());
        engine.compute(root, Size::new(200.0, 50.0), &mut fs);
        {
            let ctx = LayoutContext::new(&mut engine, &mut fs);
            ro.apply_layout(&ctx);
        }

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = ro.paint(&mut ctx);

        // Should have Text command + Caret command
        assert!(result.len() >= 1, "Should emit at least Text command");
        assert!(result.iter().any(|cmd| matches!(cmd, RenderCommand::Text { .. })), "Should emit Text command");
        assert!(result.iter().any(|cmd| matches!(cmd, RenderCommand::Caret { .. })), "Should emit Caret command when focused and blink visible");
    }

    #[test]
    fn test_paint_omits_caret_when_not_focused() {
        let editor = create_test_editor("Hello");
        let mut ro = TextEditRenderObject::new("Hello", 16.0, editor);
        ro.set_focused(false);
        ro.set_cursor_blink_visible(true);

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = ro.paint(&mut ctx);

        // Without computed_bounds, paint returns empty
        // But even if it had bounds, no Caret should appear
        assert!(result.iter().all(|cmd| !matches!(cmd, RenderCommand::Caret { .. })), "Should not emit Caret when not focused");
    }

    #[test]
    fn test_paint_omits_caret_when_blink_not_visible() {
        let editor = create_test_editor("Hello");
        let mut ro = TextEditRenderObject::new("Hello", 16.0, editor);
        ro.set_focused(true);
        ro.set_cursor_blink_visible(false);

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let result = ro.paint(&mut ctx);

        assert!(result.iter().all(|cmd| !matches!(cmd, RenderCommand::Caret { .. })), "Should not emit Caret when blink not visible");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p vexo test_paint 2>&1 | tail -20`
Expected: PASS (all three cursor paint tests)

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/render_objects/text_edit.rs
git commit -m "test: add TextEditRenderObject cursor painting unit tests"
```

---

### Task 11: Run full test suite and manual verification

**Files:** None (verification only)

- [ ] **Step 1: Run full test suite**

Run: `cargo test -p vexo 2>&1 | tail -30`
Expected: All tests pass.

- [ ] **Step 2: Build desktop demo**

Run: `cargo build -p desktop_demo 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 3: Manual verification**

Run: `cargo run -p desktop_demo`
Verify:
1. Click inside the TextEdit field — cursor should appear
2. Cursor should blink on/off at ~800ms intervals
3. Type a character — cursor should immediately become visible (reset), then resume blinking
4. Click outside the TextEdit — cursor should disappear
5. Text should still render correctly inside the TextEdit

- [ ] **Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: address issues found during manual verification"
```
