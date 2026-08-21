//! Native file-picker abstraction.
//!
//! Mirrors the `Clipboard` trait pattern: an object-safe trait with
//! platform-specific backends selected by `default_file_picker()`.
//! Desktop uses `rfd`; iOS/Android use `NoopFilePicker` (returns `None`).

use std::sync::Arc;

/// Maximum file size accepted by the picker. Larger files are rejected
/// with `None` from `pick_file`.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Result of a successful file pick — enough to build a `FileAttachment`.
pub struct PickedFile {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// Object-safe file-picker trait. Implementations must be `Send + Sync`
/// so the trait can be used as `Arc<dyn FilePicker>`.
pub trait FilePicker: Send + Sync {
    /// Open the native file dialog. `on_done` is invoked exactly once:
    /// - `Some(PickedFile)` on confirm
    /// - `None` on cancel, error, or file exceeding `MAX_FILE_BYTES`
    ///
    /// Desktop implementations call `on_done` synchronously (re-entrant into
    /// the caller's stack). iOS calls `on_done` later from the picker
    /// delegate (main thread). Either way, exactly-once delivery.
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>);
}

/// Pure helper for testable size gating. Returns `true` if `len` is within
/// `MAX_FILE_BYTES`. Extracted from `RfdFilePicker` so the boundary is
/// unit-testable without invoking a real OS dialog.
pub fn file_within_limit(len: u64) -> bool {
    len <= MAX_FILE_BYTES
}

/// No-op file picker used on platforms without a native dialog (iOS/Android).
/// `pick_file` always returns `None`. Mirrors `stub_clipboard::StubClipboard`.
pub struct NoopFilePicker;

impl FilePicker for NoopFilePicker {
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>) {
        on_done(None);
    }
}

/// Desktop file picker backed by `rfd` (rust-native file dialog).
/// Blocks the calling thread on `pick_file` — the native modal dialog
/// runs its own message pump so the window stays visually responsive.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct RfdFilePicker;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl FilePicker for RfdFilePicker {
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>) {
        let result = (|| {
            // Deliberately add NO file-type filters. On macOS, `rfd` flattens all
            // filters into a single `NSOpenPanel:setAllowedFileTypes:` array
            // (there is no dropdown), and `"*"` is treated as a literal extension,
            // not a wildcard — so any filter restricts the panel to those
            // extensions only, graying out everything else (e.g. .zip). When
            // `filters` is empty, `rfd` skips `setAllowedFileTypes:` and the panel
            // allows all files. A chat attach button should accept any file type.
            let path = rfd::FileDialog::new().pick_file()?;
            let metadata = std::fs::metadata(&path).ok()?;
            if !file_within_limit(metadata.len()) {
                return None;
            }
            let bytes = std::fs::read(&path).ok()?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            let mime = mime_from_extension(&path);
            Some(PickedFile { name, mime, bytes })
        })();
        on_done(result);
    }
}

/// Pure helper mapping a file extension (lowercase, no leading dot) to a
/// MIME type string. Returns `""` for unknown extensions. Cfg-free so both
/// desktop (`RfdFilePicker`) and iOS (`IosFilePicker`) share one mapping.
pub fn mime_from_extension_str(ext: &str) -> String {
    match ext {
        "png" => "image/png".into(),
        "jpg" | "jpeg" => "image/jpeg".into(),
        "gif" => "image/gif".into(),
        "bmp" => "image/bmp".into(),
        "webp" => "image/webp".into(),
        _ => String::new(),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn mime_from_extension(path: &std::path::Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| mime_from_extension_str(&e.to_lowercase()))
        .unwrap_or_default()
}

/// Construct the platform-default file picker as `Arc<dyn FilePicker>`.
///
/// - Desktop (macOS/Linux/Windows): `RfdFilePicker` (blocks on `rfd`).
/// - iOS/Android: `NoopFilePicker` (always returns `None`).
pub fn default_file_picker() -> Arc<dyn FilePicker> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        Arc::new(RfdFilePicker)
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        Arc::new(NoopFilePicker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_within_limit_accepts_exact_max() {
        assert!(file_within_limit(MAX_FILE_BYTES));
    }

    #[test]
    fn test_file_within_limit_rejects_one_over() {
        assert!(!file_within_limit(MAX_FILE_BYTES + 1));
    }

    #[test]
    fn test_file_within_limit_accepts_zero() {
        assert!(file_within_limit(0));
    }

    #[test]
    fn test_noop_file_picker_returns_none() {
        let picker = NoopFilePicker;
        picker.pick_file(Box::new(|picked| {
            assert!(picked.is_none());
        }));
    }

    #[test]
    fn test_default_file_picker_returns_send_sync_arc() {
        let picker: Arc<dyn FilePicker> = default_file_picker();
        assert!(Arc::strong_count(&picker) >= 1);
    }
}
