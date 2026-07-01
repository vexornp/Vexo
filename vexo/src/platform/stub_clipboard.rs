//! Stub clipboard backend used on platforms where no native clipboard is
//! available (currently iOS).
//!
//! All operations are no-ops: `get_text` always returns `None` and
//! `set_text` discards its input. This lets the rest of the framework
//! (and its keyboard shortcuts) compile and run on iOS without a real
//! clipboard; a native UIPasteboard-backed implementation can be added later.

use super::clipboard::Clipboard;

pub struct StubClipboard;

impl Clipboard for StubClipboard {
    fn get_text(&self) -> Option<String> {
        None
    }

    fn set_text(&self, _text: &str) {}
}
