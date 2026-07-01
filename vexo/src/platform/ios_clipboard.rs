//! iOS clipboard backend backed by `UIPasteboard`.
//!
//! This is the mobile counterpart to [`super::arboard_clipboard::ArboardClipboard`].
//! It implements [`Clipboard`] by talking to UIKit's `UIPasteboard` singleton
//! through the `objc2` typed bindings, so copy/cut/paste in widgets like
//! `TextEdit` works on iOS without any Swift/UniFFI glue.
//!
//! # Thread safety
//!
//! `UIPasteboard` calls must happen on the main thread. Every current call
//! site fires from winit's main-loop event dispatch in [`crate::window`], so
//! this invariant holds without extra marshalling. The struct itself stores
//! no state (the system pasteboard is a singleton obtained fresh per call),
//! so it is trivially `Send + Sync` and can be shared as `Arc<dyn Clipboard>`.

use objc2_foundation::NSString;
use objc2_ui_kit::UIPasteboard;

use super::clipboard::Clipboard;

/// Clipboard backend that proxies to the iOS system pasteboard.
///
/// Zero-sized: the `UIPasteboard` singleton is fetched per operation via
/// [`UIPasteboard::generalPasteboard`], so there is nothing to store.
pub struct IosClipboard;

impl Clipboard for IosClipboard {
    fn get_text(&self) -> Option<String> {
        let pasteboard = UIPasteboard::generalPasteboard();
        // SAFETY: `string` is a `#[unsafe(method)]` accessor. The safety
        // contract is about thread-safety; we only invoke it from the main
        // thread (see module docs). The returned `NSString` is retained by
        // `Retained` and converted to a Rust `String` via `Display`.
        let nsstring = unsafe { pasteboard.string() }?;
        Some(nsstring.to_string())
    }

    fn set_text(&self, text: &str) {
        let nsstring = NSString::from_str(text);
        let pasteboard = UIPasteboard::generalPasteboard();
        // SAFETY: `setString:` is a `#[unsafe(method)]` setter. Same
        // main-thread invariant as `string`; called from event dispatch.
        unsafe { pasteboard.setString(Some(&nsstring)) };
    }
}
