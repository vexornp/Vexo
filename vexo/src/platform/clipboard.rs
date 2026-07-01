//! Clipboard abstraction.
//!
//! Object-safe trait so the framework can talk to any clipboard backend
//! (arboard on desktop, a stub on iOS, or a mock in tests) without widgets
//! knowing the concrete implementation.

/// A clipboard that can read and write text.
///
/// Implementations must be `Send + Sync` so the trait can be used as
/// `Arc<dyn Clipboard>`. Backends that are not natively `Sync` (e.g. arboard)
/// should wrap their handle in a `Mutex`.
pub trait Clipboard: Send + Sync {
    /// Get the current clipboard contents as text, if any.
    fn get_text(&self) -> Option<String>;

    /// Set the clipboard contents to the given text.
    fn set_text(&self, text: &str);
}
