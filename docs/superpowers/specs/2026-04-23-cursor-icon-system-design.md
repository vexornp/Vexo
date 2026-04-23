# Cursor Icon System Design

**Date:** 2026-04-23
**Status:** Approved

## Context

The Vexo UI framework currently tracks mouse position but has no cursor icon handling. The mouse pointer always shows the default arrow, even when hovering over text input fields where an I-beam cursor would be expected. This creates a poor user experience for text editing.

## Goal

Implement a general cursor icon system that allows any widget to request cursor changes when the pointer hovers over it, with the text edit widget specifically showing an I-beam (vertical bar) cursor.

## Design

### 1. Core Types

**File:** `vexo/src/input/event.rs`

Add a `CursorIcon` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorIcon {
    #[default]
    Default,   // Standard arrow
    Pointer,   // Hand (for clickable elements)
    Text,      // I-beam (for text input)
    Crosshair, // Precision selection
    Move,      // Move/drag
    NotAllowed, // Action not allowed
    ResizeHorizontal,
    ResizeVertical,
}
```

This mirrors winit's cursor types but lives in the platform-agnostic input module.

### 2. WidgetResponse Extension

**File:** `vexo/src/widgets/mod.rs`

Add `cursor` field to `WidgetResponse<M>`:

```rust
pub struct WidgetResponse<M> {
    pub message: Option<M>,
    pub focus_request: Option<WidgetId>,
    pub cursor: Option<CursorIcon>,  // NEW
}
```

Widgets return cursor requests in their `on_event` handler. `None` means "no opinion" - use parent's or default.

### 3. InteractionResponse Extension (Separated Traits)

**File:** `vexo/src/testable/interact.rs`

Add cursor to `InteractionResponse`:

```rust
pub struct InteractionResponse<M> {
    pub message: Option<M>,
    pub focus_request: Option<WidgetId>,
    pub cursor: Option<CursorIcon>,  // NEW
}
```

This ensures unit tests can verify cursor behavior without GPU/window.

### 4. Hover Detection & Cursor Resolution

**File:** `vexo/src/lib.rs`

During `PointerMoved` events:

1. Traverse widget tree from root with pointer position
2. Collect cursor requests from widgets containing the pointer
3. Apply the topmost (last in traversal) non-`None` cursor
4. Call `window.set_cursor()` if cursor changed

**Implementation approach:** Reuse the existing `on_event` mechanism. When processing `PointerMoved`, widgets can check if the pointer is inside their bounds and return a cursor request.

### 5. WindowState Integration

**File:** `vexo/src/lib.rs`

Add cursor state tracking:

```rust
pub struct WindowState {
    // ... existing fields ...
    current_cursor: CursorIcon,
}
```

On cursor change:
```rust
if cursor != self.current_cursor {
    self.current_cursor = cursor;
    self.window.set_cursor(winit_cursor_from_icon(cursor));
}
```

### 6. Widget Implementations

**TextEdit** (`vexo/src/widgets/text_edit.rs`):
- In `on_event`, when pointer is inside bounds, return `cursor: Some(CursorIcon::Text)`

**Button** (`vexo/src/widgets/button.rs`):
- In `on_event`, when pointer is inside bounds, return `cursor: Some(CursorIcon::Pointer)`

**Container widgets** (Row, Column):
- Propagate cursor from children - if a child returns a cursor request, pass it up

### 7. Winit Cursor Mapping

**File:** `vexo/src/lib.rs`

Map `CursorIcon` to winit's `Cursor`:

```rust
fn winit_cursor_from_icon(icon: CursorIcon) -> winit::window::Cursor {
    match icon {
        CursorIcon::Default => winit::window::Cursor::default(),
        CursorIcon::Pointer => winit::window::Cursor::Pointer,
        CursorIcon::Text => winit::window::Cursor::Text,
        CursorIcon::Crosshair => winit::window::Cursor::Crosshair,
        CursorIcon::Move => winit::window::Cursor::Move,
        CursorIcon::NotAllowed => winit::window::Cursor::NotAllowed,
        CursorIcon::ResizeHorizontal => winit::window::Cursor::ResizeHorizontal,
        CursorIcon::ResizeVertical => winit::window::Cursor::ResizeVertical,
    }
}
```

## Files to Modify

| File | Changes |
|------|---------|
| `vexo/src/input/event.rs` | Add `CursorIcon` enum |
| `vexo/src/widgets/mod.rs` | Add `cursor` field to `WidgetResponse` |
| `vexo/src/testable/interact.rs` | Add `cursor` field to `InteractionResponse` |
| `vexo/src/lib.rs` | Add `current_cursor` to `WindowState`, cursor resolution logic, winit mapping |
| `vexo/src/widgets/text_edit.rs` | Return `CursorIcon::Text` on hover |
| `vexo/src/widgets/button.rs` | Return `CursorIcon::Pointer` on hover |

## Testing

1. **Unit tests:** Verify `InteractionResponse` cursor field is set correctly for text edit and button widgets
2. **Manual test:** Run desktop demo, hover over text edit area, verify I-beam cursor appears
3. **Manual test:** Hover over button, verify pointer cursor appears
4. **Manual test:** Move cursor outside widgets, verify default cursor restored

## Out of Scope

- Custom cursor images (only standard cursors)
- Cursor visibility toggle (always visible)
- Animation on cursor change
