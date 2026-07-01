use glyphon::{Edit, FontSystem};

pub struct Editor {
    raw: glyphon::Editor<'static>,
    /// The layout width that the buffer should be constrained to.
    /// Set from apply_layout, then automatically re-applied after
    /// every buffer mutation so cursor_position() stays correct
    /// when text wraps.
    layout_width: Option<f32>,
}

impl Editor {
    pub fn new(raw: glyphon::Editor<'static>) -> Self {
        Self {
            raw,
            layout_width: None,
        }
    }

    pub fn buffer(&self) -> &glyphon::Buffer {
        match self.raw.buffer_ref() {
            glyphon::cosmic_text::BufferRef::Owned(buffer) => buffer,
            glyphon::cosmic_text::BufferRef::Borrowed(buffer) => buffer,
            glyphon::cosmic_text::BufferRef::Arc(buffer) => buffer,
        }
    }

    /// Set the layout width constraint for the buffer.
    ///
    /// Called from apply_layout with the computed width. Once set,
    /// the width is automatically re-applied after every mutation
    /// so the buffer's internal line wrapping matches the visual layout.
    pub fn set_layout_width(&mut self, font_system: &mut FontSystem, width: f32) {
        self.layout_width = Some(width);
        self.apply_width_and_shape(font_system);
    }

    /// Re-apply the stored layout width to the buffer and reshape.
    fn apply_width_and_shape(&mut self, font_system: &mut FontSystem) {
        if let Some(width) = self.layout_width {
            self.raw.with_buffer_mut(|buffer| {
                buffer.set_size(font_system, Some(width), None);
            });
            self.raw
                .with_buffer_mut(|buffer| buffer.shape_until_scroll(font_system, true));
        }
    }

    /// Process an editing action (insert, delete, motion, click, etc.).
    ///
    /// After the action, automatically re-applies the layout width constraint
    /// so that cursor_position() returns correct coordinates for wrapped text.
    pub fn action(&mut self, font_system: &mut FontSystem, action: glyphon::Action) {
        self.raw.action(font_system, action);
        self.apply_width_and_shape(font_system);
    }

    /// Replace the entire text content.
    ///
    /// After setting text, automatically re-applies the layout width constraint
    /// so that cursor_position() returns correct coordinates for wrapped text.
    pub fn set_text(
        &mut self,
        font_system: &mut FontSystem,
        text: &str,
        attrs: &glyphon::Attrs,
        shaping: glyphon::Shaping,
    ) {
        self.raw.with_buffer_mut(|buffer| {
            buffer.set_text(font_system, text, attrs, shaping, None);
        });
        self.apply_width_and_shape(font_system);
    }

    /// Get the cursor position in screen coordinates (x, y).
    /// Returns None if the cursor position cannot be determined.
    pub fn cursor_position(&self) -> Option<(i32, i32)> {
        self.raw.cursor_position()
    }

    /// Get the current cursor.
    pub fn cursor(&self) -> glyphon::Cursor {
        self.raw.cursor()
    }

    /// Set the cursor position from a Cursor object.
    pub fn set_cursor(&mut self, cursor: glyphon::Cursor) {
        self.raw.set_cursor(cursor);
    }

    /// Get the current selection.
    pub fn selection(&self) -> glyphon::cosmic_text::Selection {
        self.raw.selection()
    }

    /// Set the current selection.
    pub fn set_selection(&mut self, selection: glyphon::cosmic_text::Selection) {
        self.raw.set_selection(selection);
    }

    /// Get the bounds of the current selection as `(start, end)` cursors.
    /// Returns `None` if there is no selection.
    pub fn selection_bounds(&self) -> Option<(glyphon::Cursor, glyphon::Cursor)> {
        self.raw.selection_bounds()
    }

    /// Copy the selected text, returning it as a `String`.
    /// Returns `None` if there is no selection.
    pub fn copy_selection(&self) -> Option<String> {
        self.raw.copy_selection()
    }

    /// Delete the current selection. Returns `true` if a selection was deleted.
    /// After deletion, automatically re-applies the layout width constraint.
    pub fn delete_selection(&mut self, font_system: &mut FontSystem) -> bool {
        let deleted = self.raw.delete_selection();
        if deleted {
            self.apply_width_and_shape(font_system);
        }
        deleted
    }

    /// Insert a string at the cursor, replacing any current selection.
    /// After insertion, automatically re-applies the layout width constraint.
    pub fn insert_string(&mut self, font_system: &mut FontSystem, text: &str) {
        // cosmic-text's insert_string takes an optional AttrsList; pass None
        // to use default attributes (matches how set_text is used elsewhere).
        self.raw.insert_string(text, None);
        self.apply_width_and_shape(font_system);
    }

    /// Select the entire document.
    ///
    /// Sets the selection anchor to the start of the buffer (line 0, index 0)
    /// and moves the cursor to the end of the last line.
    pub fn select_all(&mut self, font_system: &mut FontSystem) {
        let (last_line, last_index) = self.raw.with_buffer(|buffer| {
            let last_line = buffer.lines.len().saturating_sub(1);
            let last_index = buffer.lines.get(last_line).map_or(0, |l| l.text().len());
            (last_line, last_index)
        });

        self.raw
            .set_selection(glyphon::cosmic_text::Selection::Normal(
                glyphon::Cursor::new(0, 0),
            ));
        self.raw
            .set_cursor(glyphon::Cursor::new(last_line, last_index));
        self.apply_width_and_shape(font_system);
    }
}
