# Click-to-Move-Cursor in TextEdit Widget

## Context

The text edit widget currently handles pointer clicks only for focus management — clicking inside the widget requests or retains focus, but does not move the cursor to the clicked position. This is a missing feature that users expect from any text editor.

## Goal

When the user clicks inside the text edit widget while it is focused, move the cursor to the character position closest to the click location.

## Design

### Architecture

**Files to modify:**
- `vexo/src/widgets/text_edit.rs` — Add cursor positioning logic in `on_event` handler
- `vexo/src/editor.rs` — Add `set_cursor()` method to set cursor position

### Data Flow

On `PointerButton::Pressed` while the widget is focused:

1. **Calculate relative click position:**
   - Click position (logical coordinates) from `event.position`
   - Widget origin from `computed_layout.bounds.origin`
   - Relative position = `event.position - widget_origin`

2. **Convert to buffer coordinates:**
   - Buffer uses physical pixels
   - Multiply relative position by `widget_ctx.scale`

3. **Hit-test to get cursor position:**
   - Get buffer via `editor_ref.buffer()`
   - Call `buffer.hit(&mut font_system, x, y)` → returns `Option<Cursor>`

4. **Set cursor in editor:**
   - If hit returns a cursor, call `editor_ref.set_cursor(cursor)`

### New Method in Editor

```rust
pub fn set_cursor(&self, cursor: glyphon::Cursor) {
    self.0.set_cursor(cursor);
}
```

### Error Handling

| Edge Case | Handling |
|-----------|----------|
| Click outside buffer bounds | `Buffer::hit()` returns `None`, cursor stays in place |
| Empty buffer | `Buffer::hit()` handles gracefully, returns cursor at origin |
| No computed layout | Check `computed_layout.is_some()`, skip if unavailable |
| Scale factor | Convert logical to physical using `widget_ctx.scale` |

### Implementation Location

In `text_edit.rs`, modify the `PointerButton::Pressed` handler for the focused case (around line 361-378). Currently it only retains focus — add cursor positioning logic there.

## Verification

1. Run `cargo run -p desktop_demo`
2. Click inside text edit widget at various positions
3. Verify cursor moves to expected character position
4. Test edge cases: line boundaries, line start/end, empty areas