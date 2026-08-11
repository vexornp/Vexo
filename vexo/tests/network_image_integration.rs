//! Layer 3 integration tests for NetworkImage.
//!
//! These tests exercise the full path: NetworkImage widget → ImageCache
//! (with FakeHttpFetch) → ThreeTreePipeline reconciliation → Signal state.
//!
//! No winit EventLoop — frames are driven manually via pipeline.update().
//! No layout/paint — assertions are on Signal<LoadState> transitions and
//! FakeHttpFetch call counts, which don't require a render backend.

use std::sync::Arc;
use std::time::Duration;

use vexo::animation::AnimationTicker;
use vexo::image_cache::test_helpers::{FakeHttpFetch, RecordingProxy};
use vexo::image_cache::{FetchError, ImageCache, LoadState};
use vexo::reactive::Signal;
use vexo::widgets::{MultiChild, NetworkImage, Text, Widget};
use vexo::{Layout, ThreeTreePipeline};

/// Poll a signal until it's no longer Loading, with a timeout.
fn wait_until_settled(signal: &Signal<LoadState>, timeout: Duration) -> LoadState {
    let start = std::time::Instant::now();
    loop {
        let state = signal.get_cloned();
        match state {
            LoadState::Loading => {
                if start.elapsed() > timeout {
                    panic!("Signal did not settle within {:?}", timeout);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => return state,
        }
    }
}

fn test_url(s: &str) -> url::Url {
    url::Url::parse(s).unwrap()
}

/// Create a pipeline with an ImageCache backed by FakeHttpFetch.
fn make_pipeline(fetcher: Arc<FakeHttpFetch>) -> (ThreeTreePipeline, Arc<ImageCache>) {
    let cache = Arc::new(ImageCache::new(fetcher, Arc::new(RecordingProxy::new())));
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.set_image_cache(cache.clone());
    (pipeline, cache)
}

#[test]
fn network_image_loads_and_renders_after_fetch() {
    let url = test_url("https://example.com/avatar.png");
    let fetcher = Arc::new(FakeHttpFetch::new());
    fetcher.return_bytes(&url, FakeHttpFetch::solid_color_png(4, 4, [255, 0, 0, 255]));
    let (mut pipeline, cache) = make_pipeline(fetcher);

    let widget = NetworkImage::new(url.clone()).placeholder(|| Text::new("Loading").boxed());

    // Mount the widget — this calls render() which calls cache.get_or_fetch(url),
    // spawning a fetch thread.
    pipeline.update(widget.boxed());

    // The cache should now have a Loading entry for this URL.
    let signal = cache.get_or_fetch(url.clone());
    assert_eq!(signal.get_cloned(), LoadState::Loading);

    // Wait for the fetch thread to complete.
    let state = wait_until_settled(&signal, Duration::from_secs(2));
    assert!(
        matches!(state, LoadState::Loaded(_)),
        "expected Loaded, got {:?}",
        state
    );
}

#[test]
fn network_image_error_renders_error_state() {
    let url = test_url("https://example.com/broken.png");
    let fetcher = Arc::new(FakeHttpFetch::new());
    fetcher.return_error(&url, FetchError::Network("404 not found".into()));
    let (mut pipeline, cache) = make_pipeline(fetcher);

    let widget = NetworkImage::new(url.clone()).error(|_e| Text::new("Error").boxed());

    pipeline.update(widget.boxed());

    let signal = cache.get_or_fetch(url.clone());
    let state = wait_until_settled(&signal, Duration::from_secs(2));
    assert!(
        matches!(state, LoadState::Error(_)),
        "expected Error, got {:?}",
        state
    );
}

#[test]
fn two_network_images_same_url_single_fetch() {
    let url = test_url("https://example.com/shared.png");
    let fetcher = Arc::new(FakeHttpFetch::new());
    fetcher.return_bytes(&url, FakeHttpFetch::red_pixel_png());
    let fetcher_clone = fetcher.clone();
    let (mut pipeline, _cache) = make_pipeline(fetcher);

    // Mount two NetworkImage widgets with the same URL in a column.
    let widget = MultiChild::new(
        vec![
            NetworkImage::new(url.clone()).boxed(),
            NetworkImage::new(url.clone()).boxed(),
        ],
        Layout::column(),
    );

    pipeline.update(widget.boxed());

    // Wait a moment for the fetch thread.
    std::thread::sleep(Duration::from_millis(200));

    // The fetcher should have been called exactly once — the second
    // NetworkImage's render() hits the cache and gets the existing signal.
    assert_eq!(
        fetcher_clone.call_count(),
        1,
        "two NetworkImage widgets with the same URL should only trigger one fetch"
    );
}

#[test]
fn network_image_url_change_refetches() {
    let url_a = test_url("https://example.com/image-a.png");
    let url_b = test_url("https://example.com/image-b.png");
    let fetcher = Arc::new(FakeHttpFetch::new());
    fetcher.return_bytes(&url_a, FakeHttpFetch::red_pixel_png());
    fetcher.return_bytes(
        &url_b,
        FakeHttpFetch::solid_color_png(2, 2, [0, 0, 255, 255]),
    );
    let fetcher_clone = fetcher.clone();
    let (mut pipeline, cache) = make_pipeline(fetcher);

    // Mount with URL A.
    let widget_a = NetworkImage::new(url_a.clone()).placeholder(|| Text::new("Loading A").boxed());
    pipeline.update(widget_a.boxed());

    let signal_a = cache.get_or_fetch(url_a.clone());
    let state_a = wait_until_settled(&signal_a, Duration::from_secs(2));
    assert!(matches!(state_a, LoadState::Loaded(_)));
    assert_eq!(fetcher_clone.call_count(), 1);

    // Re-mount with URL B (simulates URL change via parent rebuild).
    let widget_b = NetworkImage::new(url_b.clone()).placeholder(|| Text::new("Loading B").boxed());
    pipeline.update(widget_b.boxed());

    let signal_b = cache.get_or_fetch(url_b.clone());
    let state_b = wait_until_settled(&signal_b, Duration::from_secs(2));
    assert!(matches!(state_b, LoadState::Loaded(_)));

    // Two fetches total: one for A, one for B.
    assert_eq!(fetcher_clone.call_count(), 2);
}
