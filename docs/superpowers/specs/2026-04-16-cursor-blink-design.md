# Cursor Blink for TextEdit Widget

**Date:** 2026-04-16
**Status:** Approved

## Context

The TextEdit widget currently has focus handling but does not render a cursor at all. Users need visual feedback when a text field is focused and ready for input. A blinking cursor is the standard UI pattern for this.

## Requirements

- **Cursor style:** Vertical bar (`|`)
- **Blink timing:** 800ms period (slow, relaxed pace)
- **Cursor color:** Accent color, configurable via TextEdit property
- **Behavior on typing:** Reset blink cycle — cursor becomes visible immediately, then starts blinking after typing stops
- **Behavior on unfocus:** Hide cursor completely

## Architecture

Add a `CursorBlinkState` struct to `WindowState` that tracks timing and visibility. The blink state updates each frame during render, and resets when keyboard input is received. TextEdit's `draw()` method reads this state to decide whether to render the cursor.

**Key principle:** The blink state is global (one cursor visible at a time since only one widget can be focused), but the TextEdit widget controls rendering.

## Components

### 1. CursorBlinkState (new struct in `vexo/src/lib.rs`)

```rust
struct CursorBlinkState {
    last_update: Instant,
    accumulator_ms: f32,
    visible: bool,
    blink_period_ms: f32,  // 800ms default
}
```

Methods:
- `tick()` — called each frame, updates `visible` based on elapsed time
- `reset()` — called on keyboard input, sets `visible = true` and clears accumulator

### 2. WindowState (modify in `vexo/src/lib.rs`)

- Add `cursor_blink: CursorBlinkState` field
- Initialize in `new()` and `new_with_ios()`

### 3. TextEdit (modify `vexo/src/widgets/text_edit.rs`)

- Add `cursor_color: Color` field (configurable accent color)
- In `draw()`: when focused AND `cursor_blink.visible`, render vertical bar cursor

### 4. Render loop (modify `vexo/src/lib.rs`)

- Call `cursor_blink.tick()` at start of `render()`
- Pass `cursor_blink` reference to widget `draw()` calls

## Data Flow

```
1. Frame Start
   └─> render() calls cursor_blink.tick()
       └─> Measures elapsed time since last frame
       └─> Accumulates time, toggles visible when accumulator >= 800ms

2. Widget Draw
   └─> TextEdit::draw() receives &cursor_blink
       └─> If focused AND cursor_blink.visible:
           └─> Calculate cursor position from glyphon::Editor cursor()
           └─> Draw vertical bar at that position using UiBatcher

3. Keyboard Input
   └─> on_event() receives keyboard event
       └─> If focused and key pressed:
           └─> cursor_blink.reset() → visible = true, accumulator = 0
```

**Cursor position calculation:**
- `glyphon::Editor` already tracks cursor position via its internal buffer
- Use the editor's cursor index to calculate x-offset within the text layout
- Draw a thin vertical rectangle (1-2 logical pixels wide) at that position

## Error Handling

No error states — cursor blink is purely visual and should never fail.

Edge cases:
1. **No font system:** Already handled by existing TextEdit — cursor won't render without text
2. **Zero-size TextEdit:** Bounds check in `draw()` prevents rendering outside widget area
3. **Very fast typing:** Reset on each keystroke keeps cursor visible, accumulator stays near 0
4. **App paused/resumed:** `Instant::now()` handles time gaps correctly — long pause will trigger multiple blink toggles on next frame, settling to correct state

## Testing

Manual verification:
1. Run `cargo run -p desktop_demo`
2. Click on TextEdit to focus — cursor should appear and start blinking
3. Type characters — cursor should stay visible, then start blinking after you stop
4. Click outside TextEdit — cursor should disappear
5. Wait and verify 800ms timing feels right

No unit tests — visual timing behavior is difficult to test in unit tests. Integration tests would require mocking time, which adds complexity for minimal benefit.

## Files to Modify

| File | Changes |
|------|---------|
| `vexo/src/lib.rs` | Add `CursorBlinkState`, modify `WindowState`, update `render()` |
| `vexo/src/widgets/text_edit.rs` | Add `cursor_color` field, implement cursor drawing in `draw()` |
| `vexo/src/widgets/mod.rs` | Update `WidgetContext` or `Widget::draw()` signature if needed |
| `vexo/src/renderer.rs` | May need to add cursor rectangle to `UiBatcher` |

## Implementation Approach

Frame-based timing (Approach A from brainstorming):
- Track elapsed time between frames using `Instant::now()`
- Update blink state during render
- Reset accumulator on keyboard input
- No new threads or event loop modifications required
