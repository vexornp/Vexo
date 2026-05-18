# TextEdit Widget in Retain Mode — Design Spec

## Context

The Vexo framework has a retain-mode three-tree architecture (Widget/Element/RenderObject) with a `StatefulWidget` pattern. The existing immediate-mode `TextEdit` widget (`vexo/src/widgets/text_edit.rs`) manages a `glyphon::Editor` via `WidgetStateRegistry`, but this approach doesn't fit the retain-mode architecture where elements own their own state via `StateStorage`.

We need a retain-mode TextEdit that follows Flutter's `EditableText` pattern: a `StatefulWidget` with an external controller that owns the editor state, enabling parent widgets to read/write text and listen for changes.

## Scope

**Initial version supports keyboard input only.** No cursor rendering, no mouse selection, no IME composition. These can be added incrementally in later iterations.

## Architecture

```
TextEdit (StatefulWidget)
  └── TextEditState (State + Default)
        └── build() → Text widget (displays controller.text())
```

The `TextEditingController` is owned externally by the parent and passed into `TextEdit`. It wraps a `glyphon::Editor` and provides a dirty callback wired to the `BuildOwner` for triggering rebuilds.

## Components

### 1. TextEditingController

**File:** `vexo/src/retain/widgets/text_edit.rs`

```rust
pub struct TextEditingController {
    editor: Rc<RefCell<Editor>>,       // glyphon::Editor wrapper
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    font_size: f32,                    // default 16.0
}
```

**Methods:**
- `new(initial_text: &str)` → creates glyphon Editor with initial content
- `text()` → `String` — reads current buffer text
- `set_text(text: &str)` — replaces buffer content, calls `notify()`
- `insert_char(c: char)` — inserts at cursor via `Action::Insert`, calls `notify()`
- `delete_backward()` — backspace via `Action::Backspace`, calls `notify()`
- `move_cursor(direction: Motion)` — arrow key via `Action::Motion`, calls `notify()`
- `set_dirty_callback(callback: Arc<dyn Fn() + Send + Sync>)` — wires to BuildOwner
- `notify()` — calls dirty callback if set, marking owning element for rebuild
- `editor()` → `Rc<RefCell<Editor>>` — direct access for advanced operations

**Lifecycle:**
- Parent creates controller before building widget tree
- Passed into `TextEdit::new(controller)`
- On `StatefulElement::mount()`, the dirty callback is wired to the BuildOwner
- On `StatefulElement::unmount()`, the callback is disconnected
- Controller survives across widget rebuilds (external ownership)

### 2. TextEdit Widget

**File:** `vexo/src/retain/widgets/text_edit.rs`

```rust
#[derive(Clone)]
pub struct TextEdit {
    controller: TextEditingController,
    key: Option<WidgetKey>,
}
```

**Methods:**
- `new(controller: TextEditingController)` → creates widget
- `with_key(key)` → sets widget key

**Widget trait:** Implemented via blanket `impl<W: StatefulWidget + Clone + 'static> Widget for W`.

### 3. TextEditState

**File:** `vexo/src/retain/widgets/text_edit.rs`

```rust
pub struct TextEditState;
```

Implements `State` (no-op lifecycle hooks) and `Default`.

**StatefulWidget impl:**
```rust
impl StatefulWidget for TextEdit {
    type State = TextEditState;

    fn build(&self, _state: &mut TextEditState, _ctx: &mut BuildContext) -> Box<dyn Widget> {
        Box::new(Text::new(self.controller.text()).with_font_size(self.controller.font_size()))
    }
}
```

### 4. Event Handling

Keyboard events flow through the existing `EventContext` → `Element::on_event()` pipeline. The `StatefulElement` for TextEdit handles events by:

1. Checking if the element is focused (via `EventContext::is_focused()`)
2. For `InputEvent::Keyboard` with `ButtonState::Pressed`:
   - `Key::Character` → `controller.insert_char(c)` for each non-control char
   - `Key::Named(NamedKey::Backspace)` → `controller.delete_backward()`
   - `Key::Named(NamedKey::ArrowLeft/Right/Up/Down)` → `controller.move_cursor(Motion::Left/Right/Up/Down)`
   - `Key::Named(NamedKey::Home/End)` → `controller.move_cursor(Motion::Home/End)`
   - `Key::Named(NamedKey::Enter)` → `controller.insert_char('\n')`
   - `Key::Named(NamedKey::Escape)` → clear focus
3. Each mutation calls `controller.notify()`, which marks the element dirty
4. The pipeline performs a targeted rebuild on the next frame

**Implementation approach:** The `StatefulElement` already stores the widget (`self.widget`). When `StatefulElement::on_event()` is called with a keyboard event and the element is focused, it checks if the widget is a `TextEdit` (via `as_any().downcast_ref::<TextEdit>()`). If so, it delegates the event to the widget's controller directly:

```rust
// In StatefulElement::on_event()
if let Some(text_edit) = self.widget.as_any().downcast_ref::<TextEdit>() {
    return text_edit.handle_event(event, ctx);
}
```

This avoids modifying the `State` trait. The `TextEdit` widget itself has a `handle_event()` method that operates on its controller.

**TextEdit::handle_event():**
```rust
impl TextEdit {
    pub fn handle_event(&self, event: &InputEvent, _ctx: &mut EventContext) -> Option<Box<dyn Any>> {
        if let InputEvent::Keyboard { key, state: ButtonState::Pressed, .. } = event {
            match key {
                Key::Character(c) => { self.controller.insert_char(c.chars().next().unwrap()); }
                Key::Named(NamedKey::Backspace) => { self.controller.delete_backward(); }
                Key::Named(NamedKey::ArrowLeft) => { self.controller.move_cursor(Motion::Left); }
                // ... other keys
                _ => {}
            }
        }
        None
    }
}
```

**StatefulElement change:** In its `on_event()` impl, after checking focus, attempt to downcast the widget to `TextEdit`. If successful, delegate to `TextEdit::handle_event()`. Otherwise, delegate to child element as before.

This approach keeps event handling logic on the widget (which owns the controller) rather than on the state (which has no access to the controller). It follows the principle that the widget knows how to handle its own events.

**Future improvement:** The downcast approach creates a dependency from `StatefulElement` to `TextEdit`. A cleaner long-term solution would be a `KeyboardHandler` trait that any widget can implement, with `StatefulElement` checking for trait implementation instead of concrete types. This can be introduced when a second keyboard-handling widget is needed.

## File Changes

### New files
- `vexo/src/retain/widgets/text_edit.rs` — TextEdit, TextEditState, TextEditingController

### Modified files
- `vexo/src/retain/widgets/mod.rs` — add `mod text_edit;` and `pub use text_edit::{TextEdit, TextEditState, TextEditingController};`
- `vexo/src/retain/mod.rs` — add `TextEdit` and `TextEditingController` to public re-exports
- `vexo/src/retain/stateful_widget.rs` — update `StatefulElement::on_event()` to delegate keyboard events to TextEdit when focused

### No changes to
- Element system (StatefulElement already supports the pattern)
- Render pipeline (TextEdit delegates to Text widget)
- Event system (keyboard events already flow through EventContext)

## Verification

1. **Unit tests** for TextEditingController:
   - `new()` creates editor with initial text
   - `text()` returns current content
   - `set_text()` updates content and triggers notify
   - `insert_char()` inserts at cursor
   - `delete_backward()` deletes before cursor
   - `move_cursor()` moves cursor position

2. **Unit tests** for TextEdit widget:
   - `build()` returns a Text widget with current content
   - Widget implements Clone correctly
   - StatefulWidget trait is satisfied

3. **Integration test:**
   - Create a TextEdit with a controller
   - Reconcile into pipeline
   - Simulate keyboard events
   - Verify controller text updates
   - Verify rebuild produces correct Text widget

4. **Build verification:** `cargo build -p vexo` and `cargo test -p vexo` pass
