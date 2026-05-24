# TextEdit Cursor Display Design

## Problem

The retain-mode `TextEdit` widget currently shows no cursor when focused. The legacy immediate-mode `TextEdit` draws a blinking vertical bar via `renderer.add_rect()`, but the retain-mode pipeline (`ThreeTreePipeline`) has no equivalent rendering path — no cursor render command, no blink state, and no way to compute cursor position at paint time.

## Goal

Show a blinking accent-blue vertical cursor (2px wide, line-height tall) inside the focused retain-mode `TextEdit`, matching the legacy behavior and standard text editor UX.

## Requirements

- Cursor visible only when the TextEdit element is focused
- Cursor blinks on/off at 800ms intervals (matching legacy `CursorBlinkState`)
- Cursor resets to visible on keyboard input (matching legacy reset-on-typing behavior)
- Cursor color: accent blue RGB(0.3, 0.67, 0.97) (matching legacy `cursor_color`)
- Cursor width: 2px (matching legacy behavior)
- Cursor height: line height from the editor buffer at the cursor position
- Cursor position: derived from `glyphon::Editor::cursor_position()` relative to the text buffer, converted to absolute window coordinates

## Architecture

Following Flutter's `RenderEditable` pattern: a single render object paints both text content and cursor in one `paint()` call, giving correct z-order (text first, cursor on top).

### Element/Render Tree Structure

**Before:**
```
StatefulElement<TextEdit>
  → ProxyRenderObject (no paint)
    → DecoratedContainerElement
      → DecoratedContainerRenderObject (border, background, padding)
        → LeafElement<Text>
          → TextRenderObject (text only, no cursor)
```

**After:**
```
StatefulElement<TextEdit>
  → ProxyRenderObject (no paint, unchanged)
    → DecoratedContainerElement
      → DecoratedContainerRenderObject (border, background, padding)
        → LeafElement<TextEditContent>  ← new thin widget
          → TextEditRenderObject (text + cursor)
```

`TextEdit::build()` returns `DecoratedContainer(TextEditContent(...))` instead of `DecoratedContainer(Text(...))`.

### New Components

#### 1. `TextEditContent` Widget

A thin leaf widget that carries the data `TextEditRenderObject` needs:

```rust
struct TextEditContent {
    controller: TextEditingController,
    font_size: f32,
    is_focused: bool,
    cursor_blink_visible: bool,
}
```

Implements `Widget` with `create_render_object()` returning `Box::new(TextEditRenderObject::new(...))`.

#### 2. `TextEditRenderObject`

A leaf render object (like `TextRenderObject`) that paints both text and cursor:

```rust
struct TextEditRenderObject {
    // Text rendering (same as TextRenderObject)
    content: String,
    font_size: f32,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,

    // Cursor rendering
    controller: Rc<RefCell<TextEditingControllerInner>>,
    is_focused: bool,
    cursor_blink_visible: bool,
}
```

**Paint logic:**
1. If `computed_bounds` is `None`, return empty
2. Get absolute position from `ctx.absolute_position()`
3. Emit `RenderCommand::Text { content, position, font_size, color: BLACK, max_width }`
4. If `is_focused && cursor_blink_visible`:
   - Borrow `controller`, get `cursor_position()` from inner `Editor` → returns `Option<(i32, i32)>` relative to buffer
   - Get line height from `controller.editor.buffer().metrics().line_height`
   - Convert relative cursor position to absolute: `abs_x = pos.x + absolute_position.x`, `abs_y = pos.y + absolute_position.y`
   - Emit `RenderCommand::Caret { position: Point::new(abs_x, abs_y), height: line_height, color: ACCENT_BLUE }`

**Layout logic:** identical to `TextRenderObject` — creates a Taffy leaf node with text measurement context.

#### 3. `RenderCommand::Caret` Variant

```rust
Caret {
    position: Point<Logical>,  // top-left of the cursor bar in absolute coordinates
    height: f32,                // line height
    color: Color,               // accent blue
}
```

#### 4. `TextEditingController::cursor_position()`

New method on `TextEditingController` (delegating to inner `Editor`):

```rust
pub fn cursor_position(&self) -> Option<(i32, i32)> {
    self.inner.borrow().cursor_position()
}
```

#### 5. `TextEditingController::line_height()`

New method to expose the buffer's line height for cursor height:

```rust
pub fn line_height(&self) -> f32 {
    self.inner.borrow().buffer().metrics().line_height
}
```

### Pipeline Changes

#### `ThreeTreePipeline` owns `CursorBlinkState`

```rust
pub struct ThreeTreePipeline {
    // ... existing fields ...
    cursor_blink: CursorBlinkState,
}
```

- `tick_cursor_blink()` called at the start of each frame (matching legacy `Window` behavior)
- `reset_cursor_blink()` called when a keyboard event is dispatched to a focused element

#### Passing focus/blink state to `TextEditRenderObject`

Before the paint traversal, the pipeline injects focus and blink state:

1. The pipeline resolves the focused element via `FocusManager::primary_focus_element()`
2. It walks the render object tree and for each `TextEditRenderObject`, sets:
   - `is_focused = true` if the owning element is the focused element, `false` otherwise
   - `cursor_blink_visible = self.cursor_blink.is_visible()`
3. This happens in a new method `prepare_cursor_state()` called between layout and paint

This keeps focus/blink state out of `PaintContext` (avoiding unnecessary exposure to every render object) and avoids the need for `paint_after_children`.

#### `CommandProcessor` handles `Caret`

`process_commands` matches `RenderCommand::Caret` and calls `batcher.add_rect()` with:
- bounds: `from_xywh(position.x, position.y, 2.0, height)`
- fill: `color`
- no stroke, no corner radius

## Data Flow

```
Frame start:
  pipeline.tick_cursor_blink()

Event handling (if keyboard event):
  pipeline.reset_cursor_blink()

Reconcile/rebuild:
  TextEdit::build() → DecoratedContainer(TextEditContent { controller, font_size, is_focused, cursor_blink_visible })

Layout:
  TextEditRenderObject::layout() → Taffy leaf with text measurement (same as TextRenderObject)

Pre-paint:
  pipeline.prepare_cursor_state() → sets is_focused + cursor_blink_visible on each TextEditRenderObject

Paint:
  TextEditRenderObject::paint() → [RenderCommand::Text, RenderCommand::Caret?]
  CommandProcessor::process_commands() → batcher.add_text(), batcher.add_rect()

GPU render:
  text via glyphon TextRenderer
  cursor rect via quad shader (existing rect rendering)
```

## Cursor Position Computation

`glyphon::Editor::cursor_position()` returns `Option<(i32, i32)>` — pixel coordinates relative to the buffer origin. To convert to absolute window coordinates:

```
abs_cursor_x = cursor_pos.0 as f32 + absolute_position.x
abs_cursor_y = cursor_pos.1 as f32 + absolute_position.y
```

Where `absolute_position` comes from `PaintContext::absolute_position()` (the top-left of the render object in window coordinates, already computed by the paint traversal).

## Files to Modify/Create

| Action | File | Description |
|--------|------|-------------|
| Create | `vexo/src/retain/render_objects/text_edit.rs` | `TextEditRenderObject` implementation |
| Create | `vexo/src/retain/widgets/text_edit_content.rs` | `TextEditContent` leaf widget |
| Modify | `vexo/src/retain/widgets/text_edit.rs` | `TextEdit::build()` returns `DecoratedContainer(TextEditContent(...))`; add `cursor_position()` and `line_height()` to `TextEditingController` |
| Modify | `vexo/src/render/command.rs` | Add `RenderCommand::Caret` variant |
| Modify | `vexo/src/render/command_processor.rs` | Handle `Caret` → `batcher.add_rect()` |
| Modify | `vexo/src/retain/render_objects/mod.rs` | Register `TextEditRenderObject` |
| Modify | `vexo/src/retain/widgets/mod.rs` | Register `TextEditContent` widget |
| Modify | `vexo/src/retain/pipeline.rs` | Add `CursorBlinkState`, `tick_cursor_blink()`, `reset_cursor_blink()`, `prepare_cursor_state()` |

## Testing

1. **Unit test**: `TextEditRenderObject::paint()` emits `Caret` when focused + blink visible
2. **Unit test**: `TextEditRenderObject::paint()` omits `Caret` when not focused
3. **Unit test**: `TextEditRenderObject::paint()` omits `Caret` when focused but blink not visible
4. **Unit test**: `CommandProcessor` converts `Caret` to a 2px-wide rect
5. **Integration test**: `TextEditingController::cursor_position()` returns `Some` after inserting text
6. **Manual test**: Run `desktop_demo`, click TextEdit, verify cursor appears, blinks, resets on typing