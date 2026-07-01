//! Platform service abstractions.
//!
//! Currently provides a `Clipboard` trait with pluggable backends.
//! The framework obtains a backend via [`default_clipboard`], which
//! selects arboard on desktop and UIPasteboard on iOS.

#[cfg(not(target_os = "ios"))]
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
pub fn default_clipboard() -> Arc<dyn Clipboard> {
    #[cfg(not(target_os = "ios"))]
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
}
