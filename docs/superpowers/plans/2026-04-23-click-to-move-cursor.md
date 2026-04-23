# Click-to-Move-Cursor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move cursor to clicked position when clicking inside the text edit widget.

**Architecture:** Add `set_cursor()` method to `Editor` wrapper, then use `Buffer::hit()` in the `TextEdit::on_event` handler to convert click coordinates to cursor position.

**Tech Stack:** Rust, glyphon/cosmic-text for text editing

---

## File Structure

| File | Responsibility |
|------|----------------|
| `vexo/src/editor.rs` | Add `set_cursor()` method to set cursor from a `Cursor` object |
| `vexo/src/widgets/text_edit.rs` | Add cursor positioning logic in `on_event` handler |

---

### Task 1: Add `set_cursor()` method to Editor

**Files:**
- Modify: `vexo/src/editor.rs:44`

- [ ] **Step 1: Add the `set_cursor()` method**

Add this method to the `Editor` impl block (after `cursor_position()` at line 42):

```rust
    /// Set the cursor position from a Cursor object.
    pub fn set_cursor(&mut self, cursor: glyphon::Cursor) {
        self.raw.set_cursor(cursor);
    }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles without errors

- [ ] **Step 3: Commit**

```bash
git add vexo/src/editor.rs
git commit -m "feat(editor): add set_cursor method for direct cursor positioning"
```

---

### Task 2: Add cursor positioning on click in TextEdit

**Files:**
- Modify: `vexo/src/widgets/text_edit.rs:360-378`

- [ ] **Step 1: Add cursor positioning logic in the focused click handler**

Replace the `PointerButton::Pressed` match arm in the focused case (lines 361-378) with:

```rust
            InputEvent::PointerButton {
                state: ButtonState::Pressed,
                position,
                ..
            } => {
                if bounds_check(position) {
                    // Click inside - retain focus and move cursor to click position
                    if let Some(layout) = layout_view.get_layout(node) {
                        let abs_x = offset.x + layout.x();
                        let abs_y = offset.y + layout.y();

                        // Calculate click position relative to widget
                        let rel_x = position.x - abs_x;
                        let rel_y = position.y - abs_y;

                        // Convert to physical pixels (buffer uses physical coordinates)
                        let scale = widget_context.scale.factor();
                        let phys_x = rel_x * scale;
                        let phys_y = rel_y * scale;

                        // Hit-test to find cursor position
                        let buffer = editor_ref.buffer();
                        if let Some(cursor) = buffer.hit(&mut widget_context.font_system, phys_x, phys_y) {
                            editor_ref.set_cursor(cursor);
                        }
                    }

                    return WidgetResponse {
                        message: None,
                        focus_request: Some(my_id),
                        handled: true,
                        clear_focus: false,
                        cursor: None,
                    };
                }
                // Click outside - don't handle, let framework clear focus
                return WidgetResponse::default();
            }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p vexo`
Expected: Compiles without errors

- [ ] **Step 3: Commit**

```bash
git add vexo/src/widgets/text_edit.rs
git commit -m "feat(text-edit): move cursor to clicked position"
```

---

### Task 3: Manual verification

- [ ] **Step 1: Run the desktop demo**

Run: `cargo run -p desktop_demo`

- [ ] **Step 2: Test cursor positioning**

1. Click inside the text edit widget to focus it
2. Type some text (e.g., "Hello World")
3. Click at different positions in the text
4. Verify the cursor moves to the clicked position
5. Test clicking near line boundaries
6. Test clicking at the start and end of the text

Expected: Cursor moves to the character position closest to each click

---

## Verification Summary

- [ ] `cargo build -p vexo` compiles without errors
- [ ] Manual test: cursor moves to clicked position
- [ ] Edge cases work: line boundaries, empty areas, start/end of text
