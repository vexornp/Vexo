use glyphon::{fontdb, FontSystem};
use once_cell::sync::Lazy;
use std::sync::Mutex;

extern crate alloc;

pub static FONT_SYSTEM: Lazy<Mutex<FontSystem>> = Lazy::new(|| {
    let font_data = include_bytes!("../font.ttf").to_vec();
    let binary = fontdb::Source::Binary(alloc::sync::Arc::new(font_data));
    let font_system = glyphon::FontSystem::new_with_fonts([binary]);
    Mutex::new(font_system)
});

pub fn font_system() -> std::sync::MutexGuard<'static, FontSystem> {
    FONT_SYSTEM.lock().unwrap()
}

pub fn with_font_system_mut<T>(f: impl FnOnce(&mut FontSystem) -> T) -> T {
    let mut font_system = FONT_SYSTEM.lock().unwrap();
    f(&mut *font_system)
}
