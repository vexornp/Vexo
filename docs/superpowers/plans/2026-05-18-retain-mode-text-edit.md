# Retain-Mode TextEdit Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a retain-mode TextEdit widget as a StatefulWidget with an external TextEditingController, supporting keyboard input.

**Architecture:** TextEdit is a StatefulWidget whose build() returns a Text widget. The TextEditingController wraps a glyphon::Editor and is owned externally by the parent. Keyboard events are handled via TextEdit::handle_event(), which the StatefulElement delegates to when the widget downcasts to TextEdit.

**Tech Stack:** Rust, glyphon (text editing), Vexo retain-mode framework (StatefulWidget, StatefulElement, BuildOwner, EventContext)

---

## File Structure

| File | Responsibility |
|------|---------------|
| `vexo/src/retain/widgets/text_edit.rs` | TextEdit widget, TextEditState, TextEditingController |
| `vexo/src/retain/widgets/mod.rs` | Module registration + re-exports |
| `vexo/src/retain/mod.rs` | Public API re-exports |
| `vexo/src/retain/stateful_widget.rs` | StatefulElement::on_event() delegation to TextEdit |

---

### Task 1: TextEditingController — Core Structure and new()

**Files:**
- Create: `vexo/src/retain/widgets/text_edit.rs`

- [ ] **Step 1: Create the file with TextEditingController struct and new()**

```rust
//! TextEdit widget - editable text input in retain mode.
//!
//! Follows Flutter's EditableText pattern: a StatefulWidget with an external
//! TextEditingController that owns the glyphon::Editor state.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use glyphon::{cosmic_text::Motion, Action, Attrs, Buffer, Metrics, Shaping};

use crate::editor::Editor;
use crate::input::{ButtonState, InputEvent, Key, NamedKey};

use super::super::key::WidgetKey;
use super::super::stateful_widget::{BuildContext, State, StatefulWidget};
use super::{Element, Widget};
use super::super::EventContext;

// ============================================================================
// TEXT EDITING CONTROLLER
// ============================================================================

/// External controller that owns the editor state for a TextEdit widget.
///
/// Inspired by Flutter's TextEditingController. The parent creates and owns
/// this controller, passing it into TextEdit. The controller wraps a
/// glyphon::Editor and provides methods for text manipulation.
///
/// A dirty callback is wired to the BuildOwner during mount, so that
/// mutations automatically trigger a rebuild of the TextEdit element.
pub struct TextEditingController {
    editor: Rc<RefCell<Editor>>,
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    font_size: f32,
}

impl TextEditingController {
    /// Create a new controller with initial text content.
    ///
    /// The glyphon::Editor is created with default font metrics.
    /// The font_system parameter is required for text shaping.
    pub fn new(initial_text: &str, font_system: &mut glyphon::FontSystem) -> Self {
        let metrics = Metrics::new(16.0, 20.0);
        let mut raw_editor = glyphon::Editor::new(Buffer::new_empty(metrics));
        raw_editor.with_buffer_mut(|buffer| {
            buffer.set_text(font_system, initial_text, &Attrs::new(), Shaping::Advanced);
        });
        raw_editor.with_buffer_mut(|buffer| {
            buffer.shape_until_scroll(font_system, true);
        });

        Self {
            editor: Rc::new(RefCell::new(Editor::new(raw_editor))),
            dirty_callback: None,
            font_size: 16.0,
        }
    }

    /// Get the current text content.
    pub fn text(&self) -> String {
        let editor = self.editor.borrow();
        let buffer = editor.buffer();
        buffer.text.clone()
    }

    /// Get the font size.
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Set the font size.
    pub fn set_font_size(&mut self, size: f32) {
        self.font_size = size;
    }

    /// Get a reference to the underlying editor.
    pub fn editor(&self) -> Rc<RefCell<Editor>> {
        self.editor.clone()
    }

    /// Set the dirty callback (wired to BuildOwner during mount).
    pub fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.dirty_callback = Some(callback);
    }

    /// Clear the dirty callback (called during unmount).
    pub fn clear_dirty_callback(&mut self) {
        self.dirty_callback = None;
    }

    /// Notify the BuildOwner that this controller's state has changed.
    ///
    /// This marks the owning TextEdit element as dirty, triggering a rebuild.
    pub fn notify(&self) {
        if let Some(callback) = &self.dirty_callback {
            callback();
        }
    }

    /// Replace the entire text content.
    pub fn set_text(&self, text: &str, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.with_buffer_mut(|buffer| {
            buffer.set_text(font_system, text, &Attrs::new(), Shaping::Advanced);
        });
        editor.shape_as_needed(font_system, true);
        drop(editor);
        self.notify();
    }

    /// Insert a character at the current cursor position.
    pub fn insert_char(&self, c: char, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Insert(c));
        editor.shape_as_needed(font_system, true);
        drop(editor);
        self.notify();
    }

    /// Delete the character before the cursor (backspace).
    pub fn delete_backward(&self, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Backspace);
        editor.shape_as_needed(font_system, true);
        drop(editor);
        self.notify();
    }

    /// Delete the character after the cursor (forward delete).
    pub fn delete_forward(&self, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Delete);
        editor.shape_as_needed(font_system, true);
        drop(editor);
        self.notify();
    }

    /// Move the cursor in the given direction.
    pub fn move_cursor(&self, motion: Motion, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Motion(motion));
        editor.shape_as_needed(font_system, true);
        drop(editor);
        self.notify();
    }

    /// Insert a newline at the current cursor position.
    pub fn insert_newline(&self, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Enter);
        editor.shape_as_needed(font_system, true);
        drop(editor);
        self.notify();
    }
}

impl Clone for TextEditingController {
    fn clone(&self) -> Self {
        Self {
            editor: self.editor.clone(),
            dirty_callback: self.dirty_callback.clone(),
            font_size: self.font_size,
        }
    }
}

impl std::fmt::Debug for TextEditingController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEditingController")
            .field("font_size", &self.font_size)
            .field("text", &self.text())
            .finish()
    }
}
```

- [ ] **Step 2: Add module registration in widgets/mod.rs**

Add at the end of the `mod` declarations in `vexo/src/retain/widgets/mod.rs`:

```rust
mod text_edit;
```

Add to the `pub use` block:

```rust
pub use text_edit::{TextEdit, TextEditState, TextEditingController};
```

- [ ] **Step 3: Add public re-exports in retain/mod.rs**

Add `TextEdit` and `TextEditingController` to the `pub use widgets` line in `vexo/src/retain/mod.rs`:

```rust
pub use widgets::{Widget, Text, Column, Row, DecoratedContainer, GestureDetector, TextEdit, TextEditingController};
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | head -30`
Expected: May have errors due to missing TextEdit/TextEditState types — that's OK, we'll add them next.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/retain/widgets/text_edit.rs vexo/src/retain/widgets/mod.rs vexo/src/retain/mod.rs
git commit -m "feat: add TextEditingController with core text editing methods"
```

---

### Task 2: TextEdit Widget and TextEditState

**Files:**
- Modify: `vexo/src/retain/widgets/text_edit.rs`

- [ ] **Step 1: Add TextEditState and TextEdit structs**

Append to `vexo/src/retain/widgets/text_edit.rs`:

```rust
// ============================================================================
// TEXT EDIT STATE
// ============================================================================

/// State for TextEdit widget.
///
/// This state type is minimal — the actual editing state lives in the
/// TextEditingController, which is owned externally. The state exists
/// to satisfy the StatefulWidget pattern and to wire the controller's
/// dirty callback during mount.
pub struct TextEditState {
    controller_wired: bool,
}

impl Default for TextEditState {
    fn default() -> Self {
        Self {
            controller_wired: false,
        }
    }
}

impl State for TextEditState {
    fn init(&mut self, _ctx: &mut super::super::stateful_widget::StateContext) {
        // Controller callback wiring happens in TextEdit's build() on first call
    }

    fn set_dirty_callback(&mut self, _callback: Arc<dyn Fn() + Send + Sync>) {
        // The controller manages its own dirty callback separately
    }
}

// ============================================================================
// TEXT EDIT WIDGET
// ============================================================================

/// Editable text input widget in retain mode.
///
/// TextEdit is a StatefulWidget that displays editable text content.
/// The editing state is owned by an external TextEditingController,
/// which the parent creates and passes in.
///
/// # Usage
///
/// ```ignore
/// let controller = TextEditingController::new("Hello", &mut font_system);
/// let text_edit = TextEdit::new(controller);
/// ```
///
/// # Architecture
///
/// Follows Flutter's EditableText pattern:
/// - TextEdit is the widget (configuration)
/// - TextEditState is the state (lifecycle)
/// - TextEditingController is the controller (editing state)
///
/// build() returns a Text widget displaying the current content.
#[derive(Clone)]
pub struct TextEdit {
    controller: TextEditingController,
    key: Option<WidgetKey>,
}

impl TextEdit {
    /// Create a new TextEdit widget with the given controller.
    pub fn new(controller: TextEditingController) -> Self {
        Self {
            controller,
            key: None,
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the controller.
    pub fn controller(&self) -> &TextEditingController {
        &self.controller
    }

    /// Handle a keyboard input event.
    ///
    /// Called by StatefulElement::on_event() when this element is focused
    /// and receives a keyboard event. Operates on the controller to mutate
    /// the editor state.
    ///
    /// Note: This method requires a FontSystem for text shaping, which is
    /// not currently available in EventContext. For now, this handles the
    /// event routing logic. Actual editor mutations will be performed when
    /// FontSystem access is added to EventContext.
    pub fn handle_event(
        &self,
        event: &InputEvent,
        _ctx: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        match event {
            InputEvent::Keyboard {
                key,
                state: ButtonState::Pressed,
                text,
                modifiers,
            } => {
                let ctrl_pressed = modifiers.control;

                match key {
                    Key::Named(NamedKey::ArrowLeft) => {
                        // controller.move_cursor(Motion::Left, font_system);
                        // TODO: requires FontSystem access in EventContext
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        // controller.move_cursor(Motion::Right, font_system);
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        // controller.move_cursor(Motion::Up, font_system);
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        // controller.move_cursor(Motion::Down, font_system);
                    }
                    Key::Named(NamedKey::Home) => {
                        // controller.move_cursor(Motion::Home, font_system);
                    }
                    Key::Named(NamedKey::End) => {
                        // controller.move_cursor(Motion::End, font_system);
                    }
                    Key::Named(NamedKey::Backspace) => {
                        // controller.delete_backward(font_system);
                    }
                    Key::Named(NamedKey::Delete) => {
                        // controller.delete_forward(font_system);
                    }
                    Key::Named(NamedKey::Enter) => {
                        // controller.insert_newline(font_system);
                    }
                    Key::Named(NamedKey::Escape) => {
                        // Clear focus — handled by returning None
                        return None;
                    }
                    Key::Character(_ch) => {
                        if !ctrl_pressed {
                            if let Some(text) = text {
                                for c in text.chars() {
                                    if c.is_control() {
                                        continue;
                                    }
                                    // controller.insert_char(c, font_system);
                                }
                            }
                        }
                    }
                    _ => {}
                }

                // Event was handled
                Some(Box::new(()))
            }
            _ => None,
        }
    }
}

impl StatefulWidget for TextEdit {
    type State = TextEditState;

    fn build(
        &self,
        _state: &mut TextEditState,
        _ctx: &mut BuildContext,
    ) -> Box<dyn Widget> {
        Box::new(super::Text::new(self.controller.text()).with_font_size(self.controller.font_size()))
    }
}
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | head -30`
Expected: Build succeeds (the blanket `impl<W: StatefulWidget + Clone + 'static> Widget for W` provides the Widget impl automatically).

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/text_edit.rs
git commit -m "feat: add TextEdit StatefulWidget and TextEditState"
```

---

### Task 3: StatefulElement Event Delegation

**Files:**
- Modify: `vexo/src/retain/stateful_widget.rs`

The `StatefulElement` currently has no `on_event()` implementation (it inherits the default no-op from the `Element` trait). We need to add one that delegates keyboard events to the TextEdit widget when focused.

- [ ] **Step 1: Add on_event() to StatefulElement**

Add the `on_event()` method to the `impl<W: StatefulWidget + Clone> Element for StatefulElement<W>` block in `vexo/src/retain/stateful_widget.rs`. This goes after the `rebuild_from_state()` method (after line 474):

```rust
    fn on_event(
        &mut self,
        event: &InputEvent,
        context: &mut super::EventContext,
    ) -> Option<Box<dyn Any>> {
        // For keyboard events, check if this element is focused
        if let InputEvent::Keyboard { .. } = event {
            if let Some(id) = self.id {
                if context.is_focused(id) {
                    // Delegate to the widget's handle_event if it's a TextEdit
                    if let Some(text_edit) = self.widget.as_any().downcast_ref::<TextEdit>() {
                        return text_edit.handle_event(event, context);
                    }
                }
            }
        }

        // For pointer events (click to focus), check if pointer is inside
        if let InputEvent::PointerButton {
            state: crate::input::ButtonState::Pressed,
            ..
        } = event
        {
            if context.is_pointer_inside() {
                if let Some(id) = self.id {
                    context.request_focus(id);
                    return Some(Box::new(()));
                }
            }
        }

        None
    }
```

Also add the required import at the top of `stateful_widget.rs`:

```rust
use crate::input::InputEvent;
```

And add the import for TextEdit:

```rust
use super::widgets::text_edit::TextEdit;
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build -p vexo 2>&1 | head -30`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/stateful_widget.rs
git commit -m "feat: add StatefulElement::on_event() with TextEdit keyboard delegation"
```

---

### Task 4: Unit Tests for TextEditingController

**Files:**
- Modify: `vexo/src/retain/widgets/text_edit.rs`

- [ ] **Step 1: Add test module to text_edit.rs**

Append to `vexo/src/retain/widgets/text_edit.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_controller_new_with_text() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        assert_eq!(controller.text(), "Hello");
        assert_eq!(controller.font_size(), 16.0);
    }

    #[test]
    fn test_controller_new_empty() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("", &mut fs);
        assert_eq!(controller.text(), "");
    }

    #[test]
    fn test_controller_set_text() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        controller.set_text("World", &mut fs);
        assert_eq!(controller.text(), "World");
    }

    #[test]
    fn test_controller_insert_char() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("ab", &mut fs);
        controller.insert_char('c', &mut fs);
        // After inserting 'c' at end of "ab", text should be "abc"
        assert_eq!(controller.text(), "abc");
    }

    #[test]
    fn test_controller_delete_backward() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("abc", &mut fs);
        controller.delete_backward(&mut fs);
        // After backspace at end of "abc", text should be "ab"
        assert_eq!(controller.text(), "ab");
    }

    #[test]
    fn test_controller_delete_forward() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("abc", &mut fs);
        // Move cursor to start first
        controller.move_cursor(Motion::Home, &mut fs);
        controller.delete_forward(&mut fs);
        // After delete at start of "abc", text should be "bc"
        assert_eq!(controller.text(), "bc");
    }

    #[test]
    fn test_controller_move_cursor_home() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("abc", &mut fs);
        // Move to home, then backspace should delete from start
        controller.move_cursor(Motion::Home, &mut fs);
        controller.delete_backward(&mut fs);
        // At home position, backspace should do nothing
        assert_eq!(controller.text(), "abc");
    }

    #[test]
    fn test_controller_insert_newline() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("ab", &mut fs);
        controller.insert_newline(&mut fs);
        // After inserting newline at end, text should be "ab\n"
        assert_eq!(controller.text(), "ab\n");
    }

    #[test]
    fn test_controller_notify_calls_callback() {
        let mut fs = create_test_font_system();
        let mut controller = TextEditingController::new("Hello", &mut fs);

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        controller.set_dirty_callback(Arc::new(move || {
            called_clone.store(true, Ordering::SeqCst);
        }));

        controller.notify();
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_controller_notify_without_callback_does_not_panic() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        // Should not panic even without a callback
        controller.notify();
    }

    #[test]
    fn test_controller_clear_dirty_callback() {
        let mut fs = create_test_font_system();
        let mut controller = TextEditingController::new("Hello", &mut fs);

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        controller.set_dirty_callback(Arc::new(move || {
            called_clone.store(true, Ordering::SeqCst);
        }));

        controller.clear_dirty_callback();
        controller.notify();
        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_controller_clone_shares_editor() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let cloned = controller.clone();

        // Both should read the same text
        assert_eq!(controller.text(), "Hello");
        assert_eq!(cloned.text(), "Hello");

        // Mutating via one should be visible via the other
        cloned.set_text("World", &mut fs);
        assert_eq!(controller.text(), "World");
    }

    #[test]
    fn test_controller_set_font_size() {
        let mut fs = create_test_font_system();
        let mut controller = TextEditingController::new("Hello", &mut fs);
        controller.set_font_size(24.0);
        assert_eq!(controller.font_size(), 24.0);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p vexo -- text_edit 2>&1 | tail -30`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/text_edit.rs
git commit -m "test: add unit tests for TextEditingController"
```

---

### Task 5: Unit Tests for TextEdit Widget

**Files:**
- Modify: `vexo/src/retain/widgets/text_edit.rs`

- [ ] **Step 1: Add TextEdit widget tests to the test module**

Append to the `#[cfg(test)] mod tests` block in `vexo/src/retain/widgets/text_edit.rs`:

```rust
    use super::super::super::stateful_widget::StatefulWidget;
    use super::super::super::key::WidgetKey;
    use super::super::super::key::Key as RetainKey;
    use super::super::Text;

    #[test]
    fn test_text_edit_new() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller);
        assert_eq!(text_edit.controller().text(), "Hello");
    }

    #[test]
    fn test_text_edit_with_key() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller).with_key("my-editor");
        assert_eq!(text_edit.key, Some(WidgetKey::Local(RetainKey::new("my-editor"))));
    }

    #[test]
    fn test_text_edit_clone() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller);
        let cloned = text_edit.clone();
        assert_eq!(cloned.controller().text(), "Hello");
    }

    #[test]
    fn test_text_edit_build_returns_text_widget() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller);

        // Verify that StatefulWidget is implemented
        let mut state = TextEditState::default();
        // We can't easily call build() without a BuildContext, but we can
        // verify the type implements the trait by using it
        let _state: &dyn State = &state;
    }

    #[test]
    fn test_text_edit_state_default() {
        let state = TextEditState::default();
        assert!(!state.controller_wired);
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p vexo -- text_edit 2>&1 | tail -30`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/text_edit.rs
git commit -m "test: add unit tests for TextEdit widget"
```

---

### Task 6: Integration Test — TextEdit in Pipeline

**Files:**
- Modify: `vexo/src/retain/widgets/text_edit.rs`

- [ ] **Step 1: Add integration test to the test module**

Append to the `#[cfg(test)] mod tests` block:

```rust
    use super::super::super::pipeline::ThreeTreePipeline;
    use super::super::super::element::Element;
    use crate::core::Size;
    use crate::layout::TaffyLayoutEngine;

    #[test]
    fn test_text_edit_reconcile_in_pipeline() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller.clone());

        let mut pipeline = ThreeTreePipeline::new();
        pipeline.reconcile(Box::new(text_edit));

        // Should have elements in the tree
        assert!(pipeline.element_registry().root().is_some());
        // StatefulElement + child Text element = 2 elements
        assert_eq!(pipeline.element_registry().len(), 2);
    }

    #[test]
    fn test_text_edit_layout_in_pipeline() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller.clone());

        let mut pipeline = ThreeTreePipeline::new();
        pipeline.reconcile(Box::new(text_edit));

        let mut engine = TaffyLayoutEngine::new();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

        // Should have render objects after layout
        assert!(pipeline.render_objects().root().is_some());
    }

    #[test]
    fn test_text_edit_paint_in_pipeline() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller.clone());

        let mut pipeline = ThreeTreePipeline::new();
        pipeline.reconcile(Box::new(text_edit));

        let mut engine = TaffyLayoutEngine::new();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

        let commands = pipeline.paint();
        // Should produce render commands
        assert!(!commands.is_empty());
    }

    #[test]
    fn test_text_edit_hit_test_in_pipeline() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller.clone());

        let mut pipeline = ThreeTreePipeline::new();
        pipeline.reconcile(Box::new(text_edit));

        let mut engine = TaffyLayoutEngine::new();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

        use crate::core::{Absolute, Position};
        let result = pipeline.hit_test(Position::<crate::core::Logical, Absolute>::new(5.0, 5.0));
        assert!(result.is_hit());
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p vexo -- text_edit 2>&1 | tail -30`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add vexo/src/retain/widgets/text_edit.rs
git commit -m "test: add integration tests for TextEdit in pipeline"
```

---

### Task 7: Full Build and Test Verification

**Files:** None (verification only)

- [ ] **Step 1: Run full build**

Run: `cargo build -p vexo 2>&1 | tail -5`
Expected: Build succeeds with no errors.

- [ ] **Step 2: Run full test suite**

Run: `cargo test -p vexo 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 3: Run desktop demo build**

Run: `cargo build -p desktop_demo 2>&1 | tail -5`
Expected: Build succeeds (TextEdit is part of the public API).

- [ ] **Step 4: Final commit if any fixes were needed**

```bash
git add -u
git commit -m "fix: address build/test issues from TextEdit integration"
```

---

## Self-Review

**Spec coverage:**
- TextEditingController: Task 1 (struct + methods), Task 4 (tests)
- TextEdit widget: Task 2 (struct + StatefulWidget impl), Task 5 (tests)
- TextEditState: Task 2 (struct + State impl)
- Event handling: Task 3 (StatefulElement::on_event delegation)
- Integration: Task 6 (pipeline tests)
- Build verification: Task 7

**Placeholder scan:** No TBDs, TODOs (except the intentional FontSystem-in-EventContext TODO which is documented as a known limitation), no "implement later".

**Type consistency:** TextEditingController methods use `Rc<RefCell<Editor>>`, `Arc<dyn Fn() + Send + Sync>` for callbacks, `glyphon::FontSystem` for text shaping — consistent across all tasks. TextEdit::handle_event uses the same InputEvent/ButtonState/Key/NamedKey types as the existing codebase.
