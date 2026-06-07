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
            buffer.set_text(font_system, text, attrs, shaping);
        });
        self.apply_width_and_shape(font_system);
    }

    /// Get the cursor position in screen coordinates (x, y).
    /// Returns None if the cursor position cannot be determined.
    pub fn cursor_position(&self) -> Option<(i32, i32)> {
        self.raw.cursor_position()
    }

    /// Set the cursor position from a Cursor object.
    pub fn set_cursor(&mut self, cursor: glyphon::Cursor) {
        self.raw.set_cursor(cursor);
    }
}
