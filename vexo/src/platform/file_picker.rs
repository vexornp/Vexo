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
    /// Open the native file dialog and block until the user confirms or
    /// cancels. Returns `None` on cancel or if the chosen file exceeds
    /// `MAX_FILE_BYTES`.
    fn pick_file(&self) -> Option<PickedFile>;
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
    fn pick_file(&self) -> Option<PickedFile> {
        None
    }
}

/// Desktop file picker backed by `rfd` (rust-native file dialog).
/// Blocks the calling thread on `pick_file` — the native modal dialog
/// runs its own message pump so the window stays visually responsive.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct RfdFilePicker;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl FilePicker for RfdFilePicker {
    fn pick_file(&self) -> Option<PickedFile> {
        let path = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
            .add_filter("All files", &["*"])
            .pick_file()?;

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
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn mime_from_extension(path: &std::path::Path) -> String {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png".into(),
        Some("jpg") | Some("jpeg") => "image/jpeg".into(),
        Some("gif") => "image/gif".into(),
        Some("bmp") => "image/bmp".into(),
        Some("webp") => "image/webp".into(),
        _ => String::new(),
    }
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
        assert!(picker.pick_file().is_none());
    }

    #[test]
    fn test_default_file_picker_returns_send_sync_arc() {
        let picker: Arc<dyn FilePicker> = default_file_picker();
        assert!(Arc::strong_count(&picker) >= 1);
    }
}
