use glyphon::{Edit, FontSystem};

pub struct Editor {
    raw: glyphon::Editor<'static>,
}

fn buffer_from_editor<'a, 'b>(
    editor: &'a impl glyphon::cosmic_text::Edit<'b>,
) -> &'a glyphon::cosmic_text::Buffer
where
    'b: 'a,
{
    match editor.buffer_ref() {
        glyphon::cosmic_text::BufferRef::Owned(buffer) => buffer,
        glyphon::cosmic_text::BufferRef::Borrowed(buffer) => buffer,
        glyphon::cosmic_text::BufferRef::Arc(buffer) => buffer,
    }
}

impl Editor {
    pub fn new(raw: glyphon::Editor<'static>) -> Self {
        Self { raw }
    }

    pub fn buffer(&self) -> &glyphon::Buffer {
        buffer_from_editor(&self.raw)
    }

    pub fn set_size(&mut self, font_system: &mut FontSystem, width: f32, height: f32) {
        // println!("Editor set_size: {}, {}", width, height);
        self.raw.with_buffer_mut(|buffer| {
            buffer.set_size(font_system, Some(width), Some(height));
        });
    }

    pub fn action(&mut self, font_system: &mut FontSystem, action: glyphon::Action) {
        println!("Editor action: {:?}", action);
        self.raw.action(font_system, action);
    }

    pub fn shape_as_needed(&mut self, font_system: &mut FontSystem, prune: bool) {
        self.raw
            .with_buffer_mut(|buffer| buffer.shape_until_scroll(font_system, prune));
    }
}
