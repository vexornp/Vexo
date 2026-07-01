//! arboard-backed clipboard for desktop platforms (macOS/Linux/Windows).

use std::sync::Mutex;

use super::clipboard::Clipboard;

/// Wraps an `arboard::Clipboard` behind a `Mutex` so it can be shared as
/// `Arc<dyn Clipboard>`. `arboard::Clipboard` is `Send` but not `Sync`
/// (it holds platform-specific handles), so the Mutex is required.
pub struct ArboardClipboard(Mutex<arboard::Clipboard>);

impl ArboardClipboard {
    /// Create a new clipboard handle backed by arboard.
    ///
    /// Fails if the platform clipboard cannot be acquired (very rare on
    /// desktop; usually means no display server). The caller decides how
    /// to handle this — typically by falling back to a stub.
    pub fn new() -> Result<Self, arboard::Error> {
        Ok(Self(Mutex::new(arboard::Clipboard::new()?)))
    }
}

impl Clipboard for ArboardClipboard {
    fn get_text(&self) -> Option<String> {
        self.0.lock().ok().and_then(|mut c| c.get_text().ok())
    }

    fn set_text(&self, text: &str) {
        if let Ok(mut c) = self.0.lock() {
            let _ = c.set_text(text);
        }
    }
}
