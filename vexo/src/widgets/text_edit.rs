//! TextEdit widget - editable text input.
//!
//! Follows Flutter's EditableText pattern: a Component with an external
//! TextEditingController that owns the glyphon::Editor state.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::animation::AnimationTicker;
use glyphon::{cosmic_text::Motion, Action, Attrs, Buffer, Edit, Metrics, Shaping};

use crate::editor::Editor;
use crate::input::{ButtonState, InputEvent, Key, MouseCursor, NamedKey, SystemCursorKind};

use super::super::key::WidgetKey;
use super::super::stateful_widget::{Component, ComponentState, LifecycleContext, RenderContext};
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
/// necessary because `LifecycleContext::widget()` returns `&dyn Any` (immutable),
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
            buffer.set_text(
                font_system,
                initial_text,
                &Attrs::new(),
                Shaping::Advanced,
                None,
            );
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
        editor.set_text(font_system, text, &Attrs::new(), Shaping::Advanced);
        drop(editor);
        self.notify();
    }

    /// Insert a character at the current cursor position.
    pub fn insert_char(&self, c: char, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Insert(c));
        drop(editor);
        self.notify();
    }

    /// Delete the character before the cursor (backspace).
    pub fn delete_backward(&self, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Backspace);
        drop(editor);
        self.notify();
    }

    /// Delete the character after the cursor (forward delete).
    pub fn delete_forward(&self, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Delete);
        drop(editor);
        self.notify();
    }

    /// Move the cursor in the given direction.
    pub fn move_cursor(&self, motion: Motion, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Motion(motion));
        drop(editor);
        self.notify();
    }

    /// Insert a newline at the current cursor position.
    pub fn insert_newline(&self, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.action(font_system, Action::Enter);
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
        drop(editor);
        self.notify();
    }

    /// Move the cursor, optionally extending the current selection.
    ///
    /// When `shift` is `true` and there is no existing selection, the current
    /// cursor position becomes the selection anchor (via `Selection::Normal`).
    /// The cursor is then moved by `motion`. `cosmic_text` computes
    /// `selection_bounds()` from the (anchor, cursor) pair, so the visible
    /// highlight automatically tracks the new cursor.
    ///
    /// When `shift` is `false`, any existing selection is cleared first.
    pub fn move_cursor_with_selection(
        &self,
        motion: Motion,
        shift: bool,
        font_system: &mut glyphon::FontSystem,
    ) {
        let mut editor = self.editor.borrow_mut();
        if shift {
            // Anchor at the current cursor if no selection exists yet.
            if matches!(editor.selection(), glyphon::cosmic_text::Selection::None) {
                let anchor = editor.cursor();
                editor.set_selection(glyphon::cosmic_text::Selection::Normal(anchor));
            }
        } else {
            editor.set_selection(glyphon::cosmic_text::Selection::None);
        }
        editor.action(font_system, Action::Motion(motion));
        drop(editor);
        self.notify();
    }

    /// Copy the currently selected text. Returns `None` if nothing is selected.
    pub fn copy(&self) -> Option<String> {
        self.editor.borrow().copy_selection()
    }

    /// Delete the current selection and return its text.
    /// Returns `None` if there was no selection.
    pub fn cut(&self, font_system: &mut glyphon::FontSystem) -> Option<String> {
        let text = self.editor.borrow().copy_selection();
        if text.is_some() {
            let mut editor = self.editor.borrow_mut();
            editor.delete_selection(font_system);
            drop(editor);
            self.notify();
        }
        text
    }

    /// Paste text at the cursor, replacing any current selection.
    pub fn paste(&self, text: &str, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        // delete_selection is a no-op if there is no selection, so this is safe
        // to call unconditionally — it handles both "replace selection" and
        // "insert at cursor" cases.
        editor.delete_selection(font_system);
        editor.insert_string(font_system, text);
        drop(editor);
        self.notify();
    }

    /// Select the entire document.
    pub fn select_all(&self, font_system: &mut glyphon::FontSystem) {
        let mut editor = self.editor.borrow_mut();
        editor.select_all(font_system);
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
/// to satisfy the Component pattern.
pub struct TextEditState;

impl Default for TextEditState {
    fn default() -> Self {
        Self
    }
}

impl ComponentState for TextEditState {
    /// Wire the TextEditingController's dirty callback during initialization.
    ///
    /// Equivalent to Flutter's `initState()` where EditableTextState subscribes
    /// to `widget.controller.addListener(_didChangeTextEditingValue)`.
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
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
    fn on_update(&mut self, old_widget: &dyn Any, ctx: &mut LifecycleContext) {
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
    fn on_unmount(&mut self, ctx: &mut LifecycleContext) {
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
                // Account for vertical centering offset: the editor's coordinate system
                // starts at the text origin, not the box top-left.
                let local = ctx.local_position();
                let scale = ctx.scale();
                let text_height = {
                    let editor = text_edit.controller.editor();
                    let editor = editor.borrow();
                    let mut h = 0.0f32;
                    for run in editor.buffer().layout_runs() {
                        h = h.max(run.line_top + run.line_height);
                    }
                    if h == 0.0 {
                        text_edit.controller.font_size() * 1.2
                    } else {
                        h
                    }
                };
                let vertical_offset = ((ctx.bounds.height() - text_height) / 2.0).max(0.0);
                let adjusted_y = local.y - vertical_offset;
                let physical_x = (local.x * scale.factor()) as i32;
                let physical_y = (adjusted_y * scale.factor()) as i32;
                text_edit
                    .controller
                    .click_at(physical_x, physical_y, ctx.font_system);

                Some(Box::new(()))
            }

            InputEvent::Keyboard {
                key,
                state: ButtonState::Pressed,
                text,
                ..
            } => {
                // Use ctx.modifiers as the single source of truth — it is kept
                // in sync by WindowState and threaded through EventHandler.
                let modifiers = ctx.modifiers;
                let cmd = modifiers.is_command();
                let shift = modifiers.shift;

                match key {
                    Key::Named(NamedKey::ArrowLeft) => {
                        text_edit.controller.move_cursor_with_selection(
                            Motion::Left,
                            shift,
                            ctx.font_system,
                        );
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        text_edit.controller.move_cursor_with_selection(
                            Motion::Right,
                            shift,
                            ctx.font_system,
                        );
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        text_edit.controller.move_cursor_with_selection(
                            Motion::Up,
                            shift,
                            ctx.font_system,
                        );
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        text_edit.controller.move_cursor_with_selection(
                            Motion::Down,
                            shift,
                            ctx.font_system,
                        );
                    }
                    Key::Named(NamedKey::Home) => {
                        text_edit.controller.move_cursor_with_selection(
                            Motion::Home,
                            shift,
                            ctx.font_system,
                        );
                    }
                    Key::Named(NamedKey::End) => {
                        text_edit.controller.move_cursor_with_selection(
                            Motion::End,
                            shift,
                            ctx.font_system,
                        );
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
                    Key::Character(ch) => {
                        if cmd {
                            // Platform-native clipboard shortcuts.
                            // Match case-insensitively so both Ctrl+C and Ctrl+Shift+C work.
                            match ch.to_lowercase().as_str() {
                                "a" => {
                                    text_edit.controller.select_all(ctx.font_system);
                                }
                                "c" => {
                                    if let Some(s) = text_edit.controller.copy() {
                                        ctx.clipboard.set_text(&s);
                                    }
                                }
                                "x" => {
                                    if let Some(s) = text_edit.controller.cut(ctx.font_system) {
                                        ctx.clipboard.set_text(&s);
                                    }
                                }
                                "v" => {
                                    if let Some(s) = ctx.clipboard.get_text() {
                                        text_edit.controller.paste(&s, ctx.font_system);
                                    }
                                }
                                _ => {
                                    // Other command+letter combos: no-op (suppress insertion).
                                }
                            }
                        } else if !modifiers.control {
                            // No command/Ctrl modifier: insert the character.
                            // (On non-macOS, control is the command key — handled above.
                            // On macOS, bare Ctrl is rare for typing; this guard is a no-op
                            // there since `cmd` already covered the Cmd case.)
                            if let Some(text) = text {
                                for c in text.chars() {
                                    if c == '\n' {
                                        // On iOS the software keyboard's Return
                                        // key arrives as insertText:@"\n", i.e.
                                        // a Character with text "\n". Route it
                                        // to Action::Enter so multi-line input
                                        // works. (Desktop Enter arrives as
                                        // NamedKey::Enter and never hits this
                                        // branch, so this is a no-op there.)
                                        text_edit.controller.insert_newline(ctx.font_system);
                                    } else if c.is_control() {
                                        continue;
                                    } else {
                                        text_edit.controller.insert_char(c, ctx.font_system);
                                    }
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
/// TextEdit is a Component that displays editable text content.
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
/// render() returns a DecoratedContainer wrapping a TextEditContent widget, with
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
}

impl Component for TextEdit {
    type State = TextEditState;

    fn render(&self, _state: &mut TextEditState, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let is_focused = ctx.is_focused();

        let border_color = if is_focused {
            crate::core::Color::rgb(0.2, 0.4, 0.8)
        } else {
            crate::core::Color::rgb(0.6, 0.6, 0.6)
        };

        let border_width = if is_focused { 2.0 } else { 1.0 };

        super::TextEditContent::new(self.controller.text(), self.controller.editor())
            .with_font_size(self.controller.font_size())
            .with_focused(is_focused)
            .with_cursor_blink_visible(false)
            .background(crate::core::Color::WHITE)
            .border(border_color, border_width)
            .corner_radius(4.0)
            .padding(8.0)
            .cursor(MouseCursor::System(SystemCursorKind::Text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_clipboard() -> std::sync::Arc<dyn crate::platform::Clipboard> {
        std::sync::Arc::new(crate::platform::stub_clipboard::StubClipboard)
    }

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

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(text_edit));

        // Should have elements in the tree
        assert!(pipeline.element_registry().root().is_some());
        // StatefulElement + MouseRegion + TextEditContent = 3 elements
        assert_eq!(pipeline.element_registry().len(), 3);
    }

    #[test]
    fn test_text_edit_layout_in_pipeline() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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

    use crate::core::{Logical, Point, ScaleSource};
    use crate::input::{ButtonState, InputEvent, Modifiers, PointerButton};

    #[test]
    fn test_text_edit_click_inside_focuses() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let text_edit = TextEdit::new(controller.clone());

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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
            &ScaleSource::default(),
            &test_clipboard(),
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

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
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
            &ScaleSource::default(),
            &test_clipboard(),
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
            &ScaleSource::default(),
            &test_clipboard(),
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

        // Put TextEdit inside a Flex::column(), like the real app does
        let column = crate::Flex::column()
            .push(crate::Text::new("Title"))
            .push(text_edit);

        let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
        pipeline.reconcile(Box::new(column));

        let mut engine = TaffyLayoutEngine::new();
        pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut fs);

        // No element should be focused initially
        assert!(pipeline.focused_element().is_none());

        // Find the TextEdit's StatefulElement by walking the tree
        let root = pipeline.element_registry().root().unwrap();
        let children = pipeline.element_registry().children(root).to_vec();
        // Flex has 2 children: Text and TextEdit
        assert_eq!(children.len(), 2, "Flex should have 2 children");
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
            &ScaleSource::default(),
            &test_clipboard(),
        );

        // The TextEdit's StatefulElement should now be focused
        let focused = pipeline.focused_element();
        assert!(
            focused.is_some(),
            "TextEdit should be focused after clicking inside it (when inside a Flex::column())"
        );
        assert_eq!(
            focused,
            Some(text_edit_element_id),
            "The focused element should be the TextEdit's StatefulElement"
        );
    }

    // ========================================================================
    // Selection + clipboard tests
    // ========================================================================

    #[test]
    fn test_shift_arrow_extends_selection() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        // Move cursor to start
        controller.move_cursor(Motion::Home, &mut fs);

        // Shift+Right should select 1 char and extend the selection
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);
        let copied = controller.copy();
        assert_eq!(
            copied.as_deref(),
            Some("H"),
            "Shift+Right should select 'H'"
        );

        // Another Shift+Right extends the selection to 2 chars
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);
        let copied = controller.copy();
        assert_eq!(
            copied.as_deref(),
            Some("He"),
            "Second Shift+Right should extend to 'He'"
        );
    }

    #[test]
    fn test_arrow_without_shift_clears_selection() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        controller.move_cursor(Motion::Home, &mut fs);

        // Select "He" with shift+Right x2
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);
        assert_eq!(controller.copy().as_deref(), Some("He"));

        // Plain Right (no shift) should clear the selection
        controller.move_cursor_with_selection(Motion::Right, false, &mut fs);
        assert!(
            controller.copy().is_none(),
            "Selection should be cleared after non-shift arrow"
        );
    }

    #[test]
    fn test_select_all() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello World", &mut fs);
        controller.select_all(&mut fs);
        let copied = controller.copy();
        assert_eq!(
            copied.as_deref(),
            Some("Hello World"),
            "select_all should select entire text"
        );
    }

    #[test]
    fn test_copy_returns_selected_text() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        controller.move_cursor(Motion::Home, &mut fs);
        // Select first 3 chars
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);
        assert_eq!(controller.copy().as_deref(), Some("Hel"));
        // Copy does not modify text
        assert_eq!(controller.text(), "Hello");
    }

    #[test]
    fn test_cut_deletes_selection() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        controller.move_cursor(Motion::Home, &mut fs);
        // Select first 2 chars
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);

        let cut_text = controller.cut(&mut fs);
        assert_eq!(cut_text.as_deref(), Some("He"));
        assert_eq!(
            controller.text(),
            "llo",
            "Cut should remove the selected text"
        );
    }

    #[test]
    fn test_paste_replaces_selection() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        controller.move_cursor(Motion::Home, &mut fs);
        // Select "Hel"
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);
        controller.move_cursor_with_selection(Motion::Right, true, &mut fs);

        controller.paste("XYZ", &mut fs);
        assert_eq!(
            controller.text(),
            "XYZlo",
            "Paste should replace the selection"
        );
    }

    #[test]
    fn test_paste_without_selection_inserts_at_cursor() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("ab", &mut fs);
        // Cursor is at start by default
        controller.paste("XY", &mut fs);
        assert_eq!(
            controller.text(),
            "XYab",
            "Paste without selection should insert at cursor"
        );
    }

    #[test]
    fn test_paste_multiline() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        controller.move_cursor(Motion::End, &mut fs);
        controller.paste("\nWorld", &mut fs);
        assert_eq!(
            controller.text(),
            "Hello\nWorld",
            "Paste should handle newlines"
        );
    }

    #[test]
    fn test_copy_returns_none_without_selection() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        assert!(
            controller.copy().is_none(),
            "copy() with no selection should return None"
        );
    }

    #[test]
    fn test_cut_returns_none_without_selection() {
        let mut fs = create_test_font_system();
        let controller = TextEditingController::new("Hello", &mut fs);
        let result = controller.cut(&mut fs);
        assert!(
            result.is_none(),
            "cut() with no selection should return None"
        );
        assert_eq!(
            controller.text(),
            "Hello",
            "cut() with no selection should not modify text"
        );
    }
}
