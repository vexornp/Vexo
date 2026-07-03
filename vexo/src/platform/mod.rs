//! Platform service abstractions.
//!
//! Currently provides a `Clipboard` trait with pluggable backends.
//! The framework obtains a backend via [`default_clipboard`], which
//! selects arboard on desktop, UIPasteboard on iOS, and a stub on
//! Android (a real JNI `ClipboardManager` backend is deferred).

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod arboard_clipboard;
pub mod clipboard;
#[cfg(target_os = "ios")]
pub mod ios_clipboard;
pub mod stub_clipboard;
pub use clipboard::Clipboard;

use std::sync::Arc;

/// Construct the platform-default clipboard backend as `Arc<dyn Clipboard>`.
///
/// - On desktop (macOS/Linux/Windows): uses [`arboard_clipboard::ArboardClipboard`].
///   If arboard cannot acquire the system clipboard (rare; e.g. headless CI),
///   falls back to a [`stub_clipboard::StubClipboard`].
/// - On iOS: uses [`ios_clipboard::IosClipboard`], which proxies to
///   `UIPasteboard` via `objc2`.
/// - On Android: uses [`stub_clipboard::StubClipboard`] for now. A real
///   backend talking to `android.content.ClipboardManager` via JNI is
///   deferred (see ROADMAP).
pub fn default_clipboard() -> Arc<dyn Clipboard> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        match arboard_clipboard::ArboardClipboard::new() {
            Ok(c) => Arc::new(c),
            Err(e) => {
                log::warn!("arboard clipboard init failed ({e:?}); using stub");
                Arc::new(stub_clipboard::StubClipboard)
            }
        }
    }
    #[cfg(target_os = "ios")]
    {
        Arc::new(ios_clipboard::IosClipboard)
    }
    #[cfg(target_os = "android")]
    {
        Arc::new(stub_clipboard::StubClipboard)
    }
}
