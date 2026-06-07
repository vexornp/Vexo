//! TextEdit widget - editable text input in retain mode.
//!
//! Follows Flutter's EditableText pattern: a StatefulWidget with an external
//! TextEditingController that owns the glyphon::Editor state.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use glyphon::{cosmic_text::Motion, Action, Attrs, Buffer, Edit, Metrics, Shaping};

use crate::editor::Editor;
use crate::input::{ButtonState, InputEvent, Key, MouseCursor, NamedKey, SystemCursorKind};

use super::super::key::WidgetKey;
use super::super::stateful_widget::{BuildContext, State, StateContext, StatefulWidget};
use super::super::EventContext;
use super::Widget;

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
///
/// The dirty callback uses `RefCell` for interior mutability, allowing
/// it to be set/cleared through a shared reference (`&self`). This is
/// necessary because `StateContext::widget()` returns `&dyn Any` (immutable),
/// but controller wiring needs to modify the callback.
pub struct TextEditingController {
    editor: Rc<RefCell<Editor>>,
    dirty_callback: RefCell<Option<Arc<dyn Fn() + Send + Sync>>>,
    font_size: f32,
}

impl TextEditingController {
    /// Create a new controller with initial text content.
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
            dirty_callback: RefCell::new(None),
            font_size: 16.0,
        }
    }

    /// Get the current text content.
    ///
    /// Reconstructs text from the buffer's lines, joining with newlines
    /// since cosmic_text stores each paragraph as a separate BufferLine.
    pub fn text(&self) -> String {
        let editor = self.editor.borrow();
        let buffer = editor.buffer();
        buffer
            .lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get the cursor position in buffer-relative coordinates.
    pub fn cursor_position(&self) -> Option<(i32, i32)> {
        let editor = self.editor.borrow();
        editor.cursor_position()
    }

    /// Get the line height from the editor buffer metrics.
    pub fn line_height(&self) -> f32 {
        let editor = self.editor.borrow();
        editor.buffer().metrics().line_height
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
    pub fn set_dirty_callback(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.dirty_callback.borrow_mut() = Some(callback);
    }

    /// Clear the dirty callback (called during unmount).
    pub fn clear_dirty_callback(&self) {
        *self.dirty_callback.borrow_mut() = None;
    }

    /// Notify the BuildOwner that this controller's state has changed.
    pub fn notify(&self) {
        if let Some(callback) = self.dirty_callback.borrow().as_ref() {
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

    /// Position the cursor at the given buffer-relative pixel coordinates.
    ///
    /// Converts the click location to a cursor position using glyphon's
    /// `Action::Click`. The x and y are in physical pixels relative to the
    /// text buffer's top-left corner.
    pub fn click_at(&self, x: i32, y: i32, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Click { x, y });
        editor.shape_as_needed(font_system, true);
        drop(editor);
        self.notify();
    }
}

impl Clone for TextEditingController {
    fn clone(&self) -> Self {
        Self {
            editor: self.editor.clone(),
            dirty_callback: RefCell::new(self.dirty_callback.borrow().clone()),
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

// ============================================================================
// TEXT EDIT STATE
// ============================================================================

/// State for TextEdit widget.
///
/// This state type is minimal — the actual editing state lives in the
/// TextEditingController, which is owned externally. The state exists
/// to satisfy the StatefulWidget pattern.
pub struct TextEditState;

impl Default for TextEditState {
    fn default() -> Self {
        Self
    }
}

impl State for TextEditState {
    /// Wire the TextEditingController's dirty callback during initialization.
    ///
    /// Equivalent to Flutter's `initState()` where EditableTextState subscribes
    /// to `widget.controller.addListener(_didChangeTextEditingValue)`.
    fn init(&mut self, ctx: &mut StateContext) {
        if let Some(text_edit) = ctx.widget().downcast_ref::<TextEdit>() {
            text_edit
                .controller
                .set_dirty_callback(ctx.dirty_callback());
        }
    }

    /// Re-wire the controller when the widget configuration changes.
    ///
    /// Equivalent to Flutter's `didUpdateWidget()` where EditableTextState
    /// unsubscribes from the old controller and subscribes to the new one.
    fn did_update_widget(&mut self, old_widget: &dyn Any, ctx: &mut StateContext) {
        let old_te = old_widget.downcast_ref::<TextEdit>();
        let new_te = ctx.widget().downcast_ref::<TextEdit>();
        if let (Some(old), Some(new)) = (old_te, new_te) {
            if !Rc::ptr_eq(&old.controller.editor, &new.controller.editor) {
                old.controller.clear_dirty_callback();
                new.controller.set_dirty_callback(ctx.dirty_callback());
            }
        }
    }

    /// Unwire the controller's dirty callback during unmount.
    ///
    /// Equivalent to Flutter's `dispose()` where EditableTextState unsubscribes
    /// from `widget.controller.removeListener(_didChangeTextEditingValue)`.
    fn dispose(&mut self, ctx: &mut StateContext) {
        if let Some(text_edit) = ctx.widget().downcast_ref::<TextEdit>() {
            text_edit.controller.clear_dirty_callback();
        }
    }

    /// Handle input events for TextEdit.
    ///
    /// - Pointer press: request focus for this element (click-to-focus),
    ///   matching Flutter's EditableText behavior.
    /// - Keyboard input: forward to the TextEdit controller for text editing.
    fn on_event(
        &mut self,
        widget: &dyn Any,
        event: &InputEvent,
        ctx: &mut EventContext,
    ) -> Option<Box<dyn Any>> {
        let text_edit = match widget.downcast_ref::<TextEdit>() {
            Some(te) => te,
            None => return None,
        };

        match event {
            InputEvent::PointerButton {
                state: ButtonState::Pressed,
                ..
            } => {
                ctx.request_focus(ctx.element_id());

                // Position cursor at click location (Flutter's selectPositionAt pattern)
                let local = ctx.local_position();
                let scale = ctx.scale();
                let physical_x = (local.x * scale.factor()) as i32;
                let physical_y = (local.y * scale.factor()) as i32;
                text_edit.controller.click_at(physical_x, physical_y, ctx.font_system);

                Some(Box::new(()))
            }

            InputEvent::Keyboard {
                key,
                state: ButtonState::Pressed,
                text,
                modifiers,
            } => {
                let ctrl_pressed = modifiers.control;

                match key {
                    Key::Named(NamedKey::ArrowLeft) => {
                        text_edit
                            .controller
                            .move_cursor(Motion::Left, ctx.font_system);
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        text_edit
                            .controller
                            .move_cursor(Motion::Right, ctx.font_system);
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        text_edit
                            .controller
                            .move_cursor(Motion::Up, ctx.font_system);
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        text_edit
                            .controller
                            .move_cursor(Motion::Down, ctx.font_system);
                    }
                    Key::Named(NamedKey::Home) => {
                        text_edit
                            .controller
                            .move_cursor(Motion::Home, ctx.font_system);
                    }
                    Key::Named(NamedKey::End) => {
                        text_edit
                            .controller
                            .move_cursor(Motion::End, ctx.font_system);
                    }
                    Key::Named(NamedKey::Backspace) => {
                        text_edit.controller.delete_backward(ctx.font_system);
                    }
                    Key::Named(NamedKey::Delete) => {
                        text_edit.controller.delete_forward(ctx.font_system);
                    }
                    Key::Named(NamedKey::Enter) => {
                        text_edit.controller.insert_newline(ctx.font_system);
                    }
                    Key::Named(NamedKey::Escape) => {
                        return None;
                    }
                    Key::Character(_ch) => {
                        if !ctrl_pressed {
                            if let Some(text) = text {
                                for c in text.chars() {
                                    if c.is_control() {
                                        continue;
                                    }
                                    text_edit.controller.insert_char(c, ctx.font_system);
                                }
                            }
                        }
                    }
                    _ => {}
                }

                Some(Box::new(()))
            }

            _ => None,
        }
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
/// build() returns a DecoratedContainer wrapping a TextEditContent widget, with
/// focus-dependent border styling.
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

    /// Handle an input event.
    ///
    /// Called by StatefulElement::on_event() for:
    /// - Keyboard events when this element is focused
    /// - Pointer button press events when pointer is inside (click-to-focus)
    pub fn handle_event(&self, event: &InputEvent, ctx: &mut EventContext) -> Option<Box<dyn Any>> {
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
                        self.controller.move_cursor(Motion::Left, ctx.font_system);
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        self.controller.move_cursor(Motion::Right, ctx.font_system);
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        self.controller.move_cursor(Motion::Up, ctx.font_system);
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        self.controller.move_cursor(Motion::Down, ctx.font_system);
                    }
                    Key::Named(NamedKey::Home) => {
                        self.controller.move_cursor(Motion::Home, ctx.font_system);
                    }
                    Key::Named(NamedKey::End) => {
                        self.controller.move_cursor(Motion::End, ctx.font_system);
                    }
                    Key::Named(NamedKey::Backspace) => {
                        self.controller.delete_backward(ctx.font_system);
                    }
                    Key::Named(NamedKey::Delete) => {
                        self.controller.delete_forward(ctx.font_system);
                    }
                    Key::Named(NamedKey::Enter) => {
                        self.controller.insert_newline(ctx.font_system);
                    }
                    Key::Named(NamedKey::Escape) => {
                        return None;
                    }
                    Key::Character(_ch) => {
                        if !ctrl_pressed {
                            if let Some(text) = text {
                                for c in text.chars() {
                                    if c.is_control() {
                                        continue;
                                    }
                                    self.controller.insert_char(c, ctx.font_system);
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

    fn build(&self, _state: &mut TextEditState, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let is_focused = ctx.is_focused();

        let border_color = if is_focused {
            crate::core::Color::rgb(0.2, 0.4, 0.8) // Blue border when focused
        } else {
            crate::core::Color::rgb(0.6, 0.6, 0.6) // Gray border when unfocused
        };

        let border_width = if is_focused { 2.0 } else { 1.0 };

        let style = crate::Style::new()
            .background(crate::core::Color::WHITE)
            .border(border_color, border_width)
            .corner_radius(4.0)
            .padding(8.0);

        Box::new(
            crate::MouseRegion::new(
                crate::DecoratedContainer::new(
                    super::TextEditContent::new(self.controller.text(), self.controller.editor())
                        .with_font_size(self.controller.font_size())
                        .with_focused(is_focused)
                        .with_cursor_blink_visible(false),
                )
                .style(style),
            )
            .cursor(MouseCursor::System(SystemCursorKind::Text)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    // ========================================================================
    // TextEditingController tests
    // ========================================================================

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
        // Move cursor to end, then insert 'c' at end
        controller.move_cursor(Motion::End, &mut fs);
        controller.insert_char('c', &mut fs);
        assert_eq!(controller.text(), "abc");
    }

    #[test]
    fn test_controller_delete_backward() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("abc", &mut fs);
        // Move cursor to end, then backspace deletes 'c'
        controller.move_cursor(Motion::End, &mut fs);
        controller.delete_backward(&mut fs);
        assert_eq!(controller.text(), "ab");
    }

    #[test]
    fn test_controller_delete_forward() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("abc", &mut fs);
        // Move cursor to start, then forward-delete removes 'a'
        controller.move_cursor(Motion::Home, &mut fs);
        controller.delete_forward(&mut fs);
        assert_eq!(controller.text(), "bc");
    }

    #[test]
    fn test_controller_move_cursor_home() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("abc", &mut fs);
        // Move cursor to end first, then back to home
        controller.move_cursor(Motion::End, &mut fs);
        controller.move_cursor(Motion::Home, &mut fs);
        // Now at start, backspace should do nothing
        controller.delete_backward(&mut fs);
        assert_eq!(controller.text(), "abc");
    }

    #[test]
    fn test_controller_insert_newline() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("ab", &mut fs);
        // Move cursor to end, then insert newline appends it
        controller.move_cursor(Motion::End, &mut fs);
        controller.insert_newline(&mut fs);
        assert_eq!(controller.text(), "ab\n");
    }

    #[test]
    fn test_controller_notify_calls_callback() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        controller.set_dirty_callback(Arc::new(move || {
            called_clone.store(true, Ordering::SeqCst);
        }));
        controller.notify();
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_controller_notify_without_callback() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        // Should not panic when no callback is set
        controller.notify();
    }

    #[test]
    fn test_controller_clear_dirty_callback() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
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
        let clone = controller.clone();
        // Mutate via the clone
        clone.set_text("World", &mut fs);
        // Original should see the change because they share the same Rc<RefCell<Editor>>
        assert_eq!(controller.text(), "World");
    }

    #[test]
    fn test_controller_set_font_size() {
        let mut fs = create_test_font_system();
        let mut controller = TextEditingController::new("Hello", &mut fs);
        controller.set_font_size(24.0);
        assert_eq!(controller.font_size(), 24.0);
    }

    #[test]
    fn test_controller_cursor_position() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let pos = controller.cursor_position();
        assert!(
            pos.is_some(),
            "cursor_position should return Some after text is set"
        );
    }

    #[test]
    fn test_controller_line_height() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let lh = controller.line_height();
        assert!(lh > 0.0, "line_height should be positive, got {}", lh);
    }

    // ========================================================================
    // TextEdit widget tests
    // ========================================================================

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
        assert!(text_edit.key.is_some());
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
    fn test_text_edit_state_default() {
        let state = TextEditState::default();
        // TextEditState is a unit struct with no fields to assert on,
        // but we verify it can be constructed.
        let _ = state;
    }

    // ========================================================================
    // TextEdit pipeline integration tests
    // ========================================================================

    use crate::core::Size;
    use crate::layout::TaffyLayoutEngine;
    use crate::ThreeTreePipeline;

    #[test]
    fn test_text_edit_reconcile_in_pipeline() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller.clone());

        let mut pipeline = ThreeTreePipeline::new();
        pipeline.reconcile(Box::new(text_edit));

        // Should have elements in the tree
        assert!(pipeline.element_registry().root().is_some());
        // StatefulElement + MouseRegion + DecoratedContainer + child TextEditContent element = 4 elements
        assert_eq!(pipeline.element_registry().len(), 4);
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

    // ========================================================================
    // Focus integration tests
    // ========================================================================

    use crate::core::{Logical, Point, Scale};
    use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};

    #[test]
    fn test_text_edit_click_inside_focuses() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller.clone());

        let mut pipeline = ThreeTreePipeline::new();
        pipeline.reconcile(Box::new(text_edit));

        let mut engine = TaffyLayoutEngine::new();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

        // No element should be focused initially
        assert!(pipeline.focused_element().is_none());

        // Simulate a pointer press inside the TextEdit bounds
        let event = InputEvent::PointerButton {
            position: Point::<Logical>::new(10.0, 10.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::<Logical>::new(10.0, 10.0),
            &event,
            Modifiers::default(),
            &mut fs,
            Scale::default(),
        );

        // The TextEdit's StatefulElement should now be focused
        let focused = pipeline.focused_element();
        assert!(
            focused.is_some(),
            "TextEdit should be focused after clicking inside it"
        );

        // The focused element should be the root StatefulElement
        let root = pipeline.element_registry().root().unwrap();
        assert_eq!(
            focused,
            Some(root),
            "The focused element should be the root StatefulElement"
        );
    }

    #[test]
    fn test_text_edit_click_outside_unfocuses() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller.clone());

        let mut pipeline = ThreeTreePipeline::new();
        pipeline.reconcile(Box::new(text_edit));

        let mut engine = TaffyLayoutEngine::new();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

        // First, click inside to focus the TextEdit
        let click_inside = InputEvent::PointerButton {
            position: Point::<Logical>::new(10.0, 10.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::<Logical>::new(10.0, 10.0),
            &click_inside,
            Modifiers::default(),
            &mut fs,
            Scale::default(),
        );

        // Verify TextEdit is focused
        assert!(
            pipeline.focused_element().is_some(),
            "TextEdit should be focused after clicking inside"
        );

        // Now click outside the viewport bounds (root fills 800x600 now)
        let click_outside = InputEvent::PointerButton {
            position: Point::<Logical>::new(900.0, 700.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::<Logical>::new(900.0, 700.0),
            &click_outside,
            Modifiers::default(),
            &mut fs,
            Scale::default(),
        );

        // Focus should be cleared because no element handled the click
        assert!(
            pipeline.focused_element().is_none(),
            "Focus should be cleared after clicking outside the TextEdit"
        );
    }

    #[test]
    fn test_text_edit_in_column_click_focuses() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller.clone());

        // Put TextEdit inside a Column, like the real app does
        let column = crate::Column::new()
            .push(crate::Text::new("Title"))
            .push(text_edit);

        let mut pipeline = ThreeTreePipeline::new();
        pipeline.reconcile(Box::new(column));

        let mut engine = TaffyLayoutEngine::new();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

        // No element should be focused initially
        assert!(pipeline.focused_element().is_none());

        // Find the TextEdit's StatefulElement by walking the tree
        let root = pipeline.element_registry().root().unwrap();
        let children = pipeline.element_registry().children(root).to_vec();
        // Column has 2 children: Text and TextEdit
        assert_eq!(children.len(), 2, "Column should have 2 children");
        let text_edit_element_id = children[1]; // TextEdit is the second child

        // Click inside the TextEdit area (below the title text)
        // The title is roughly 29px tall, so clicking at y=30 should be
        // inside the TextEdit's DecoratedContainer.
        let event = InputEvent::PointerButton {
            position: Point::<Logical>::new(10.0, 30.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
        };
        pipeline.handle_event(
            Point::<Logical>::new(10.0, 30.0),
            &event,
            Modifiers::default(),
            &mut fs,
            Scale::default(),
        );

        // The TextEdit's StatefulElement should now be focused
        let focused = pipeline.focused_element();
        assert!(
            focused.is_some(),
            "TextEdit should be focused after clicking inside it (when inside a Column)"
        );
        assert_eq!(
            focused,
            Some(text_edit_element_id),
            "The focused element should be the TextEdit's StatefulElement"
        );
    }
}
