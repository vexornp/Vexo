# Click-to-Focus/Unfocus for Retain-Mode TextEdit

## Problem

The retain-mode `TextEdit` widget has no visual focus behavior. Clicking inside it should grant focus (enabling keyboard input), and clicking outside should remove focus. The framework plumbing for focus already exists, but the TextEdit doesn't visually respond to focus state.

## Existing Infrastructure

The focus plumbing is already in place:

- **Click inside**: `StatefulElement::on_event()` calls `context.request_focus(id)` when pointer is inside bounds and pressed (stateful_widget.rs:496-508)
- **Click outside**: `EventHandler::handle_pointer_event()` clears `focused_element = None` when no element handles a pointer press (event_handler.rs:162-171)
- **Keyboard dispatch**: `EventHandler::handle_keyboard_event()` dispatches to the focused element (event_handler.rs:186-217)

What's missing: visual feedback and cursor rendering when focused.

## Design

### 1. Pass Focus State to BuildContext

`BuildContext` currently has no focus information. Add `focused_element: Option<ElementKey>` so that `StatefulWidget::build()` can conditionally render based on focus.

**File**: `vexo/src/retain/stateful_widget.rs`

- Add `focused_element: Option<ElementKey>` field to `BuildContext`
- In `StatefulElement::build_child_widget()`, pass the element's own ID as focused if it matches the pipeline's focused element. Since `build_child_widget()` doesn't have access to the pipeline's focus state, we need to thread it through.

**Constraint**: `build_child_widget()` is called from `mount()`, `update()`, and `rebuild_from_state()`, all of which receive `ElementContext`. We need focus state available in `ElementContext` or passed separately.

**Chosen approach**: Add `focused_element: Option<ElementKey>` to `ElementContext`. The pipeline sets this before calling element lifecycle methods. `BuildContext` is a thin wrapper that reads from `ElementContext`, so `build()` can check focus via `ctx.is_focused(element_id)`.

### 2. TextEdit Visual Focus Indicator

**File**: `vexo/src/retain/widgets/text_edit.rs`

`TextEdit::build()` currently returns a bare `Text` widget. Change it to return a `DecoratedContainer` wrapping the `Text`, with:
- **Focused**: blue border (`Color::rgb(0.2, 0.4, 0.8)`), 2px width
- **Unfocused**: gray border (`Color::rgb(0.6, 0.6, 0.6)`), 1px width
- Both states: light background, padding, corner radius

The `build()` method checks `ctx.is_focused(self.element_id)` to determine which style to apply.

### 3. Cursor Rendering When Focused

**File**: `vexo/src/retain/widgets/text_edit.rs`

When focused, the TextEdit should show a blinking text cursor. The `TextEditingController` owns a `glyphon::Editor` which manages cursor position. Add a `RenderCommand::Cursor` variant or use the existing text rendering infrastructure to show the cursor.

**Approach**: Add a `cursor_visible: bool` field to the `TextRenderObject` (or pass it through `PaintContext`). When the TextEdit is focused, the render command stream includes a cursor rect at the editor's cursor position. Cursor blink is managed by a timer in the pipeline or window state.

**Simplification for MVP**: Show a static cursor (no blink) when focused. Blink can be added later.

### 4. Demo App Integration

**File**: `shared_app/src/lib.rs`

Add a `retain::TextEdit` to `retain_view()`:

```rust
fn retain_view(_state: &Self::State) -> Option<Box<dyn retain::Widget>> {
    // TextEdit controller needs to persist across frames.
    // Use a static or thread-local for the controller.
    Some(Box::new(
        retain::Column::new()
            .push(retain::Text::new("Retain Mode Demo"))
            .push(RetainCounter { label: "Stateful Counter".to_string() })
            .push(retain::TextEdit::new(controller)),
    ))
}
```

The `TextEditingController` must persist across frames (it owns the editor state). Since `retain_view()` is called each frame, the controller needs to be stored outside the function — either in `State` or as a static/thread-local.

**Chosen approach**: Store the `TextEditingController` in the app's `State` struct. Add a `FontSystem` reference or store the controller lazily.

### Data Flow

```
Click inside TextEdit bounds:
  winit event → InputEvent::PointerButton
  → EventHandler::handle_pointer_event()
  → hit_test finds TextEdit's render object path
  → bubbles to StatefulElement<TextEdit>::on_event()
  → context.is_pointer_inside() = true
  → context.request_focus(id)
  → EventHandler sets focused_element = Some(id)
  → Next frame: StatefulElement::build() sees is_focused
  → Returns DecoratedContainer(blue border) + Text(with cursor)

Click outside TextEdit:
  winit event → InputEvent::PointerButton
  → EventHandler::handle_pointer_event()
  → hit_test misses TextEdit (or no element handles event)
  → focused_element = None
  → Next frame: StatefulElement::build() sees !is_focused
  → Returns DecoratedContainer(gray border) + Text(no cursor)
```

### Files Changed

| File | Change |
|------|--------|
| `vexo/src/retain/element_context.rs` | Add `focused_element` field |
| `vexo/src/retain/stateful_widget.rs` | Pass focus to BuildContext; TextEdit build uses focus for styling |
| `vexo/src/retain/widgets/text_edit.rs` | `build()` returns DecoratedContainer with focus-dependent border; cursor rendering |
| `shared_app/src/lib.rs` | Add TextEdit to `retain_view()` with persistent controller |

### Out of Scope

- Cursor blink animation (static cursor for MVP)
- Tab focus traversal
- Focus scopes
- Multi-line cursor positioning via click (click-to-cursor-position)
- Text selection
