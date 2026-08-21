//! Shared test infrastructure for `shared_app` test modules.
//!
//! Currently provides `install_test_image_cache`, which wires a
//! `FakeHttpFetch`-backed `ImageCache` into a `ThreeTreePipeline`. The
//! fetcher has no canned responses registered, so any `NetworkImage` in the
//! tree resolves to `LoadState::Error` (rendering an empty `Spacer`). This
//! is sufficient for layout/gesture tests, which assert on bounds and
//! hit-testing, not avatar pixels — the avatar slot is layout-stable at
//! `diameter × diameter` regardless of load state.
//!
//! Tests that need a *loaded* image should call this helper and then
//! register bytes on the returned fetcher (not yet exposed — add when
//! needed).

use std::sync::Arc;
use vexo::image_cache::test_helpers::{FakeHttpFetch, RecordingProxy};
use vexo::ThreeTreePipeline;

/// Install a no-response `FakeHttpFetch` `ImageCache` into `pipeline`.
///
/// `NetworkImage` widgets in the tree will go `Loading → Error → Spacer`.
/// This avoids the panic in `RenderContext::image_cache()` when no cache is
/// installed, while keeping the helper decoupled from seed URLs (no canned
/// responses to keep in sync with `data::seed`).
pub(crate) fn install_test_image_cache(pipeline: &mut ThreeTreePipeline) {
    let cache = Arc::new(vexo::ImageCache::new(
        Arc::new(FakeHttpFetch::new()),
        Arc::new(RecordingProxy::new()),
    ));
    pipeline.set_image_cache(cache);
}

use vexo::platform::file_picker::{FilePicker, NoopFilePicker, PickedFile};

/// Return a no-op `FilePicker` for tests that construct `ChatScreen`
/// directly but don't exercise the attach button. `pick_file()` always
/// returns `None`.
pub(crate) fn test_file_picker() -> std::sync::Arc<dyn FilePicker> {
    std::sync::Arc::new(NoopFilePicker)
}

/// A mock `FilePicker` that returns a canned `PickedFile` on every call.
/// Used by the attach-button test to simulate a user picking a file
/// without opening a real OS dialog.
pub(crate) struct MockFilePicker {
    pub picked: Option<PickedFile>,
}

impl FilePicker for MockFilePicker {
    fn pick_file(&self) -> Option<PickedFile> {
        self.picked.as_ref().map(|p| PickedFile {
            name: p.name.clone(),
            mime: p.mime.clone(),
            bytes: p.bytes.clone(),
        })
    }
}

/// Build a mock picker that returns a canned PNG file.
pub(crate) fn mock_png_picker() -> std::sync::Arc<MockFilePicker> {
    std::sync::Arc::new(MockFilePicker {
        picked: Some(PickedFile {
            name: "test.png".into(),
            mime: "image/png".into(),
            bytes: crate::data::make_avatar_png(50, 150, 250).to_vec(),
        }),
    })
}
