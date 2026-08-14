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
use vexo::layout::TaffyLayoutEngine;
use vexo::reactive::Signal;
use vexo::render_objects::ImageRenderObject;
use vexo::widgets::{MultiChild, NetworkImage, Offstage, Text, Widget};
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

/// A wrapper Component that re-renders on every update, cascading to its
/// child. Simulates a parent (like NavigationStackView) that rebuilds on
/// every animation frame, causing the child NetworkImage to be updated
/// (new dirty_callback, re-subscribe) multiple times before the fetch
/// completes.
#[derive(Clone)]
struct CascadeParent {
    child_url: url::Url,
    child_key: &'static str,
}

impl vexo::Component for CascadeParent {
    type State = vexo::SimpleState<()>;

    fn render(&self, _state: &mut Self::State, _ctx: &mut vexo::RenderContext) -> Box<dyn Widget> {
        NetworkImage::new(self.child_url.clone())
            .with_key(self.child_key)
            .boxed()
    }
}

/// Reproduces the bug where a NetworkImage's avatar never renders if the
/// parent cascades (re-renders) before the fetch completes.
///
/// Scenario:
/// 1. NetworkImage mounts, subscribes to a Loading signal.
/// 2. Parent cascades multiple times (simulating animation frames during
///    a push transition). Each cascade creates a new dirty_callback and
///    re-subscribes.
/// 3. Fetch completes — signal transitions to Loaded.
/// 4. The subscriber callback should fire, marking the element dirty.
///
/// If the callback doesn't fire (stale weak ref), the element is never
/// rebuilt and the avatar stays in Loading state forever.
#[test]
fn network_image_rebuilds_after_parent_cascade_and_fetch() {
    let url = test_url("https://example.com/cascade-test.png");
    let fetcher = Arc::new(FakeHttpFetch::new());
    fetcher.return_bytes(&url, FakeHttpFetch::red_pixel_png());
    fetcher.with_delay(&url, Duration::from_millis(300));
    let (mut pipeline, cache) = make_pipeline(fetcher);

    // 1. Mount via a CascadeParent so the root element is a StatefulElement
    //    that re-renders on update, cascading to the child NetworkImage.
    let widget = CascadeParent {
        child_url: url.clone(),
        child_key: "cascade-img",
    };
    pipeline.update(widget.boxed());
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();

    // 2. Simulate parent cascade: update the root multiple times.
    //    Each update calls CascadeParent::render() → creates a new
    //    NetworkImage widget → NetworkImage element's update() is called
    //    → new dirty_callback → re-subscribe to signal.
    for _ in 0..5 {
        let widget = CascadeParent {
            child_url: url.clone(),
            child_key: "cascade-img",
        };
        pipeline.update(widget.boxed());
    }

    // 3. Wait for the fetch to complete.
    let signal = cache.get_or_fetch(url.clone());
    let state = wait_until_settled(&signal, Duration::from_secs(2));
    assert!(
        matches!(state, LoadState::Loaded(_)),
        "fetch should complete: {:?}",
        state
    );

    // 4. Drain the dirty channel — the signal's subscriber callback should
    //    have sent the NetworkImage's element_id through the channel.
    pipeline.drain_dirty_to_build_owner();

    // 5. The NetworkImage element should be marked dirty.
    assert!(
        pipeline.has_pending_rebuilds(),
        "NetworkImage should be marked dirty after fetch completes \
         (subscriber callback should fire despite prior cascades)"
    );

    // 6. Pump rebuilds and verify the element count — after rebuild, the
    //    NetworkImage should swap its child from Spacer to Image.
    let elem_count_before = pipeline.element_registry().len();
    pipeline.perform_rebuilds();
    let elem_count_after = pipeline.element_registry().len();

    // Element count should be the same (Spacer→Image is a swap, not add).
    // The key assertion is that has_pending_rebuilds() was true — the
    // dirty callback fired. If it didn't, the avatar would stay in
    // Loading state forever (the bug).
    assert_eq!(
        elem_count_before, elem_count_after,
        "element count should be stable across the Spacer→Image swap"
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
    assert_eq!(
        fetcher_clone.call_count(),
        2,
        "two different URLs should trigger two fetches"
    );
}

/// Reproduces the "avatar never renders after push/pop" bug.
///
/// Scenario:
/// 1. NetworkImage mounts inside an Offstage widget, initially ONSTAGE.
/// 2. Offstage flips to true (user navigates to chat screen — list hidden).
/// 3. Fetch completes while offstage — Signal fires, element is marked dirty.
/// 4. Frame runs: perform_rebuilds (rebuilds NetworkImage, swaps Spacer→Image)
///    + layout (traversal skips offstage subtree — dirty marks wasted).
/// 5. Offstage flips back to false (user pops back to conversation list).
/// 6. Frame runs: perform_rebuilds (nothing dirty) + layout (Offstage dirty).
/// 7. ASSERT: ImageRenderObject should have computed_bounds after step 6.
///
/// If the assertion fails, the layout traversal didn't call layout() on the
/// ImageRenderObject — the child_layout_node invalidation fix is incomplete.
#[test]
fn network_image_offstage_fetch_then_reveal_gets_laid_out() {
    let url = test_url("https://example.com/offstage-avatar.png");
    let fetcher = Arc::new(FakeHttpFetch::new());
    fetcher.return_bytes(&url, FakeHttpFetch::solid_color_png(4, 4, [255, 0, 0, 255]));
    fetcher.with_delay(&url, Duration::from_millis(100));
    let (mut pipeline, cache) = make_pipeline(fetcher);

    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = vexo::resource::new_font_system();
    let size = vexo::core::Size::new(400.0, 600.0);

    // Helper to build the tree: MultiChild → Offstage → NetworkImage
    let build = |offstage: bool| -> Box<dyn Widget> {
        MultiChild::new(
            vec![
                Offstage::new(NetworkImage::new(url.clone()).with_key("avatar"), offstage).boxed(),
            ],
            Layout::column().width_percent(1.0).height_percent(1.0),
        )
        .boxed()
    };

    // ── Step 1: Mount onstage ──
    pipeline.update(build(false));
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    pipeline.layout(size, &mut engine, &mut font_system);

    // ── Step 2: Flip offstage (navigate to chat screen) ──
    pipeline.update(build(true));
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    pipeline.layout(size, &mut engine, &mut font_system);

    // ── Step 3: Wait for fetch to complete ──
    let signal = cache.get_or_fetch(url.clone());
    let state = wait_until_settled(&signal, Duration::from_secs(2));
    assert!(
        matches!(state, LoadState::Loaded(_)),
        "fetch should complete: {:?}",
        state
    );

    // ── Step 4: Frame runs while offstage ──
    // (simulates the ImageLoaded event waking the render loop)
    pipeline.drain_dirty_to_build_owner();
    assert!(
        pipeline.has_pending_rebuilds(),
        "NetworkImage should be marked dirty after fetch completes"
    );
    pipeline.perform_rebuilds();
    pipeline.layout(size, &mut engine, &mut font_system);

    // ── Step 5: Flip back onstage (pop back to conversation list) ──
    pipeline.update(build(false));
    pipeline.drain_dirty_to_build_owner();
    pipeline.perform_rebuilds();
    pipeline.layout(size, &mut engine, &mut font_system);

    // ── Step 6: Assert ImageRenderObject has computed_bounds ──
    // Traverse the RO tree from root to find the ImageRenderObject.
    let ro_reg = pipeline.render_objects();
    fn find_image_ro(
        reg: &vexo::RenderObjectRegistry,
        id: vexo::RenderObjectKey,
    ) -> Option<vexo::RenderObjectKey> {
        let ro = reg.get(id)?;
        if ro.as_any().downcast_ref::<ImageRenderObject>().is_some() {
            return Some(id);
        }
        for child in ro.children() {
            if let Some(found) = find_image_ro(reg, *child) {
                return Some(found);
            }
        }
        None
    }

    let root = pipeline.render_objects().root().expect("root RO exists");
    let image_ro_key = find_image_ro(ro_reg, root).expect("ImageRenderObject should exist");

    let image_ro = ro_reg.get(image_ro_key).unwrap();
    let bounds = image_ro.computed_bounds();
    assert!(
        bounds.is_some(),
        "ImageRenderObject should have computed_bounds after reveal — \
         if None, layout() was never called on it"
    );
    let b = bounds.unwrap();
    assert!(
        b.width() > 0.0 && b.height() > 0.0,
        "ImageRenderObject should have non-zero bounds after reveal, \
         got {}x{}",
        b.width(),
        b.height()
    );
}
