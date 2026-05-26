# Integrate FontSystem into TextEdit for Keyboard Handling

## Context

The retain-mode `TextEdit::handle_event()` has keyboard event routing logic (backspace, delete, arrow keys, character input) but all actual editor mutations are commented out because `EventContext` doesn't provide `FontSystem` access. The `cosmic_text::Editor` requires `&mut FontSystem` for operations like `insert`, `delete`, and cursor movement. This design adds FontSystem to EventContext so TextEdit can handle keyboard events end-to-end.

## Approach

Add `font_system: &'a mut glyphon::FontSystem` to `EventContext`, following the same pattern as `LayoutContext` (which already has this field). Thread FontSystem through the event dispatch path from WindowState down to TextEdit.

## Threading Path

```
WindowState::handle_window_event()
  └─ &mut self.widget_context (contains FontSystem)
      └─ ThreeTreePipeline::handle_event()
          └─ EventHandler::dispatch_event()
              └─ EventContext::new(..., font_system)  ← NEW
                  └─ StatefulElement::on_event()
                      └─ TextEdit::handle_event()
                          └─ editor.insert(ctx.font_system, ...)
                          └─ editor.delete(ctx.font_system, ...)
```

## Changes

### 1. EventContext (`vexo/src/retain/event_context.rs`)
- Add `font_system: &'a mut glyphon::FontSystem` field
- Update `new()` to accept and store it

### 2. EventHandler (`vexo/src/retain/event_handler.rs`)
- Update `dispatch_event()` to receive `&mut glyphon::FontSystem`
- Pass it when constructing `EventContext`

### 3. ThreeTreePipeline (`vexo/src/retain/pipeline.rs`)
- Update `handle_event()` to accept `&mut glyphon::FontSystem`
- Pass it to `EventHandler::dispatch_event()`

### 4. WindowState (`vexo/src/window.rs`)
- Pass `&mut self.widget_context.font_system` to `pipeline.handle_event()`

### 5. TextEdit widget (`vexo/src/retain/widgets/text_edit.rs`)
- Uncomment and fix editor mutation calls in `handle_event()`:
  - Character input: `self.editor.insert(ctx.font_system, &cursor, ch)`
  - Backspace: `self.editor.delete(ctx.font_system, &cursor, Selection::none())`
  - Delete forward: same with forward direction
  - Cursor movement: `self.editor.action(ctx.font_system, ...)` where needed

### 6. Tests
- Update any test code constructing `EventContext` to provide a `FontSystem`

## Verification

1. `cargo build` — no compilation errors
2. `cargo test` — all tests pass
3. `cargo run -p desktop_demo` — type in the TextEdit field, verify:
   - Characters appear when typed
   - Backspace deletes characters
   - Arrow keys move cursor
   - Focus tracking works (click to focus, click elsewhere to unfocus)
