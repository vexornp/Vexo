# Click-to-Position Cursor in TextEdit

## Problem

When a TextEdit widget with text is focused, clicking inside it again does not move the blink cursor to the clicked position. The click only requests focus (which is a no-op if already focused). The click coordinates are completely ignored for cursor placement.

## Design

Following Flutter's pattern where `RenderEditable.selectPositionAt()` uses `globalToLocal()` to convert the global pointer position to the render object's local coordinate space before passing to the text layout system.

### Part 1: Hit Test Transform Stack (Flutter's `globalToLocal`)

Flutter builds a transform stack during hit testing so each hit target can convert global coordinates to its own local space. Vexo currently stores only the outermost hit target's absolute bounds in `HitTestResult`. We add the deepest hit target's bounds so `EventContext` can provide `local_position`.

**Changes:**

1. **`HitTestResult`** (`vexo/src/hit_test.rs`):
   - Add `inner_bounds: Option<Bounds<Logical>>` field
   - Add `inner_bounds()` accessor
   - Update constructors (`hit()`, `hit_with_bounds()`, `miss()`, `Default`)
   - In `hit_test_recursive()`, set `inner_bounds` to the current object's absolute bounds on every hit (the last assignment before returning is the deepest)

2. **`EventContext`** (`vexo/src/event_context.rs`):
   - Add `local_position: Point<Logical>` field
   - This is the pointer position in the deepest hit target's local coordinate space
   - Computed as `pointer_position - inner_bounds.origin`
   - Update `new()` and `with_build_owner()` constructors
   - Update all tests

3. **`EventHandler::handle_pointer_event()`** (`vexo/src/event_handler.rs`):
   - Compute `local_position` from `hit_result.inner_bounds()` and `position`
   - Pass it to `EventContext::with_build_owner()`

**Why this works:** The deepest hit target for a TextEdit click is the `TextEditRenderObject`, whose bounds are inside the DecoratedContainer's padding. So `local_position` automatically accounts for padding.

### Part 2: Cursor Positioning on Click

**Changes:**

1. **`TextEditingController`** (`vexo/src/widgets/text_edit.rs`):
   - Add `click_at(x: i32, y: i32, font_system: &mut FontSystem)` method
   - Calls `editor.action(font_system, Action::Click { x, y })` then `shape_as_needed` and `notify()`
   - Parallels existing `move_cursor()`, `insert_char()`, etc.

2. **`TextEditState::on_event()`** (`vexo/src/widgets/text_edit.rs`):
   - When handling `InputEvent::PointerButton { state: Pressed, position, .. }`:
     - Still call `ctx.request_focus(ctx.element_id())`
     - Convert `ctx.local_position` to physical pixels: `physical = local_position * ctx.scale`
     - Call `controller.click_at(physical.x as i32, physical.y as i32, ctx.font_system)`
     - Reset cursor blink via `controller.reset_cursor_blink()` so cursor is immediately visible

3. **`window.rs`** (`vexo/src/window.rs`):
   - After pointer button events on a focused TextEdit, call `pipeline.reset_cursor_blink()` and `pipeline.mark_focus_subtree_needs_paint()` so the cursor repaints immediately at the new position
   - This is handled in `process_input_event()` alongside the existing keyboard-input blink reset

### Coordinate Chain

```
pointer_position (window logical absolute)
→ local_position = pointer_position - inner_bounds.origin (content area logical relative)
→ physical_local = local_position * scale_factor (content area physical relative)
→ Action::Click { x: physical_local.x as i32, y: physical_local.y as i32 }
```

This matches Flutter's chain: `globalToLocal(globalPosition) - paintOffset → TextPainter.getPositionForOffset()`.

### Scale Factor Access

The `EventContext` currently has no access to the DPI scale factor. We need to add it so `TextEditState::on_event()` can convert logical `local_position` to physical pixels for `Action::Click`.

**Option:** Add `scale: Scale` field to `EventContext`, threaded through from `EventHandler` → `ThreeTreePipeline::handle_event()` → `WindowState::process_input_event()`.

### Files Modified

| File | Change |
|------|--------|
| `vexo/src/hit_test.rs` | Add `inner_bounds` to `HitTestResult`, update `hit_test_recursive` |
| `vexo/src/event_context.rs` | Add `local_position` and `scale` fields |
| `vexo/src/event_handler.rs` | Compute `local_position`, pass `scale` to `EventContext` |
| `vexo/src/widgets/text_edit.rs` | Add `click_at()` to controller, update `on_event()` |
| `vexo/src/pipeline.rs` | Thread `scale` through `handle_event()` |
| `vexo/src/window.rs` | Pass `scale` to pipeline, reset blink on click |

### Testing

- Unit test: `TextEditingController::click_at()` positions cursor correctly
- Unit test: `HitTestResult::inner_bounds()` returns deepest target bounds
- Unit test: `EventContext::local_position` computed correctly
- Manual test: click inside focused TextEdit, cursor moves to click position
- Manual test: click inside unfocused TextEdit, cursor moves to click position AND focus is gained
