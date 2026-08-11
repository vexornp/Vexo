//! In-memory image cache for remote URL fetching.
//!
//! Provides `ImageCache` — a per-process `HashMap<Url, Signal<LoadState>>`
//! that deduplicates fetches: the first `get_or_fetch(url)` call spawns a
//! background thread to fetch + decode the image; subsequent calls for the
//! same URL return the same `Signal` without re-fetching.
//!
//! `NetworkImage` widgets subscribe to the `Signal<LoadState>` via
//! `RenderContext::depend_on_signal`, so only widgets subscribed to a
//! specific URL rebuild when that fetch completes.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use url::Url;

use crate::image_data::ImageData;
use crate::reactive::Signal;
use crate::VexoUserEvent;

/// The load state of a remote image, stored in a `Signal<LoadState>` per URL.
///
/// `Signal<T>` requires `T: PartialEq + Clone + Send + Sync`. `Send + Sync`
/// hold because `ImageData` (`Vec<u8>` + `u32` + `u32`) and `String` are
/// `Send + Sync`. `PartialEq` is derived on both `LoadState` and `ImageData`.
#[derive(Clone, Debug, PartialEq)]
pub enum LoadState {
    /// Fetch is in progress (or hasn't started yet). The widget should show
    /// a placeholder.
    Loading,
    /// Fetch + decode succeeded. The widget should show an `Image`.
    Loaded(ImageData),
    /// Fetch or decode failed. The widget should show an error widget.
    /// The string is a human-readable error message.
    Error(String),
}

/// Errors that can occur during image fetching.
#[derive(Clone, Debug)]
pub enum FetchError {
    /// Network-level failure (DNS, connection refused, HTTP error status, etc.).
    Network(String),
    /// I/O failure while reading the response body.
    Io(String),
    /// Response body exceeded the size cap.
    TooLarge(u64),
    /// Decoding the fetched bytes as an image failed. Set by the cache,
    /// not by the fetcher.
    Decode(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Network(msg) => write!(f, "network error: {}", msg),
            FetchError::Io(msg) => write!(f, "io error: {}", msg),
            FetchError::TooLarge(size) => {
                write!(f, "response too large ({} bytes)", size)
            }
            FetchError::Decode(msg) => write!(f, "decode error: {}", msg),
        }
    }
}

impl std::error::Error for FetchError {}

/// Trait for fetching raw bytes from a URL.
///
/// Implemented by `UreqHttpFetch` in production and `FakeHttpFetch` in tests.
/// The fetcher returns raw bytes; the cache handles decoding via
/// `ImageData::from_bytes`.
pub trait HttpFetch: Send + Sync {
    fn fetch(&self, url: &Url) -> Result<Vec<u8>, FetchError>;
}

/// Trait for waking the render loop when a fetch completes.
///
/// Decouples `ImageCache` from `EventLoopProxy<VexoUserEvent>` so the cache
/// is fully testable without winit. Production uses `WinitImageCacheProxy`;
/// tests use `RecordingProxy`.
pub trait ImageCacheProxy: Send + Sync {
    /// Notify the render loop that the image at `url` has been loaded (or
    /// failed to load). Called from the fetch thread.
    fn send_image_loaded(&self, url: Url);
}

/// Production adapter wrapping winit's `EventLoopProxy`.
///
/// In winit 0.31, `EventLoopProxy` is no longer generic and only exposes
/// `wake_up()`. To deliver the `VexoUserEvent::ImageLoaded` payload to the
/// render loop, the proxy is paired with an `mpsc::Sender<VexoUserEvent>` —
/// the same channel pattern `VexoApp` drains from `proxy_wake_up`.
/// `send_image_loaded` pushes the event through the channel and then wakes
/// the event loop. If the channel receiver has been dropped (app shutting
/// down), the event is silently dropped with a debug log.
pub struct WinitImageCacheProxy {
    proxy: winit::event_loop::EventLoopProxy,
    sender: Sender<VexoUserEvent>,
}

impl WinitImageCacheProxy {
    pub fn new(proxy: winit::event_loop::EventLoopProxy, sender: Sender<VexoUserEvent>) -> Self {
        Self { proxy, sender }
    }
}

impl ImageCacheProxy for WinitImageCacheProxy {
    fn send_image_loaded(&self, url: Url) {
        if let Err(_) = self.sender.send(VexoUserEvent::ImageLoaded(url.clone())) {
            log::debug!("ImageLoaded event dropped (receiver dropped): {}", url);
            return;
        }
        // Wake the event loop so it drains the channel. `wake_up()` is a
        // no-op if the loop has already exited.
        self.proxy.wake_up();
    }
}

/// Per-URL cache entry. The `Signal` is shared between the cache and all
/// subscribed `NetworkImage` widgets.
struct CacheEntry {
    state: Signal<LoadState>,
}

/// In-memory image cache. One instance per `WindowState`, shared via `Arc`.
///
/// Call `get_or_fetch(url)` to get a `Signal<LoadState>` for a URL. On first
/// call, spawns a fetch thread; on subsequent calls, returns the existing
/// signal without re-fetching.
pub struct ImageCache {
    /// URL → cache entry. `Mutex` (not `RefCell`) because fetch threads
    /// are real OS threads that need `Send`-lockable access.
    entries: Mutex<HashMap<Url, Arc<CacheEntry>>>,
    /// Wake-up callback for the render loop.
    proxy: Arc<dyn ImageCacheProxy>,
    /// HTTP fetcher (injected for testability).
    fetcher: Arc<dyn HttpFetch>,
}

impl ImageCache {
    /// Create a new cache. The fetcher and proxy are injected — production
    /// uses `UreqHttpFetch` + `WinitImageCacheProxy`; tests use fakes.
    pub fn new(fetcher: Arc<dyn HttpFetch>, proxy: Arc<dyn ImageCacheProxy>) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            proxy,
            fetcher,
        }
    }

    /// Get the `Signal<LoadState>` for a URL, spawning a fetch if needed.
    ///
    /// On cache miss: inserts a `Loading` entry, spawns a fetch thread,
    /// returns the `Loading` signal.
    ///
    /// On cache hit (any state): returns the existing signal without
    /// re-fetching.
    pub fn get_or_fetch(&self, url: Url) -> Signal<LoadState> {
        // Fast path: URL already in cache.
        {
            let entries = self.entries.lock().unwrap();
            if let Some(entry) = entries.get(&url) {
                return entry.state.clone();
            }
        }

        // Slow path: insert Loading entry, spawn fetch thread.
        let signal = Signal::new(LoadState::Loading);
        let entry = Arc::new(CacheEntry {
            state: signal.clone(),
        });

        // Insert the entry. Check again under lock in case another thread
        // raced ahead of us — if so, discard our entry and return theirs.
        {
            let mut entries = self.entries.lock().unwrap();
            if let Some(existing) = entries.get(&url) {
                return existing.state.clone();
            }
            entries.insert(url.clone(), entry);
        }

        // Spawn fetch thread.
        let fetcher = self.fetcher.clone();
        let proxy = self.proxy.clone();
        let fetch_url = url.clone();
        let fetch_signal = signal.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| fetcher.fetch(&fetch_url)));

            let new_state = match result {
                Ok(Ok(bytes)) => match ImageData::from_bytes(&bytes) {
                    Ok(data) => LoadState::Loaded(data),
                    Err(e) => LoadState::Error(format!("Decode failed: {}", e)),
                },
                Ok(Err(e)) => LoadState::Error(format!("Fetch failed: {}", e)),
                Err(payload) => {
                    let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    LoadState::Error(format!("panic: {}", msg))
                }
            };

            // `set_from` (not `set`) because `LoadState` is `Clone` not `Copy`.
            fetch_signal.set_from(&new_state);

            // Wake the render loop regardless of success/failure — the
            // widget needs to rebuild to show the result.
            proxy.send_image_loaded(fetch_url);
        });

        signal
    }
}

// ============================================================================
// TEST HELPERS
// ============================================================================

pub mod test_helpers {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;

    /// Fetcher that returns canned responses. Panics if asked for a URL
    /// it doesn't know about.
    pub struct FakeHttpFetch {
        responses: Mutex<HashMap<Url, Result<Vec<u8>, FetchError>>>,
        delays: Mutex<HashMap<Url, Duration>>,
        calls: Mutex<Vec<Url>>,
    }

    impl FakeHttpFetch {
        pub fn new() -> Self {
            Self {
                responses: Mutex::new(HashMap::new()),
                delays: Mutex::new(HashMap::new()),
                calls: Mutex::new(Vec::new()),
            }
        }

        /// Register a successful response (raw bytes) for a URL.
        pub fn return_bytes(&self, url: &Url, bytes: Vec<u8>) {
            self.responses
                .lock()
                .unwrap()
                .insert(url.clone(), Ok(bytes));
        }

        /// Register an error response for a URL.
        pub fn return_error(&self, url: &Url, err: FetchError) {
            self.responses.lock().unwrap().insert(url.clone(), Err(err));
        }

        /// Register a delay before the response is returned.
        pub fn with_delay(&self, url: &Url, delay: Duration) {
            self.delays.lock().unwrap().insert(url.clone(), delay);
        }

        /// Panicking variant: fetcher that always panics.
        #[allow(dead_code)]
        pub fn panicking() -> Self {
            let fetcher = Self::new();
            // No responses registered — fetch() will panic with "no response"
            fetcher
        }

        /// Record a call and return the canned response.
        fn record_call(&self, url: &Url) {
            self.calls.lock().unwrap().push(url.clone());
        }

        /// Get the number of times fetch() was called.
        pub fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }

        /// Get the list of URLs that fetch() was called with.
        #[allow(dead_code)]
        pub fn calls(&self) -> Vec<Url> {
            self.calls.lock().unwrap().clone()
        }

        /// Generate a small valid PNG (1x1 red pixel) for testing.
        pub fn red_pixel_png() -> Vec<u8> {
            // Minimal 1x1 red PNG generated by the image crate.
            let img = image::RgbaImage::from_raw(1, 1, vec![255, 0, 0, 255]).unwrap();
            let mut bytes = Vec::new();
            image::DynamicImage::ImageRgba8(img)
                .write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageFormat::Png,
                )
                .unwrap();
            bytes
        }

        /// Generate a larger valid PNG for testing.
        pub fn solid_color_png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
            let pixels: Vec<u8> = rgba.repeat((width * height) as usize);
            let img = image::RgbaImage::from_raw(width, height, pixels).unwrap();
            let mut bytes = Vec::new();
            image::DynamicImage::ImageRgba8(img)
                .write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageFormat::Png,
                )
                .unwrap();
            bytes
        }
    }

    impl HttpFetch for FakeHttpFetch {
        fn fetch(&self, url: &Url) -> Result<Vec<u8>, FetchError> {
            self.record_call(url);

            if let Some(delay) = self.delays.lock().unwrap().get(url) {
                std::thread::sleep(*delay);
            }

            self.responses
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .unwrap_or_else(|| {
                    Err(FetchError::Network(format!(
                        "no canned response for {}",
                        url
                    )))
                })
        }
    }

    /// Fetcher that always panics. Used to test catch_unwind.
    pub struct PanickingFetch;

    impl HttpFetch for PanickingFetch {
        fn fetch(&self, _url: &Url) -> Result<Vec<u8>, FetchError> {
            panic!("PanickingFetch intentionally panicking");
        }
    }

    /// Proxy that records all `send_image_loaded` calls for assertion.
    #[derive(Default)]
    pub struct RecordingProxy {
        calls: Mutex<Vec<Url>>,
    }

    impl RecordingProxy {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn calls(&self) -> Vec<Url> {
            self.calls.lock().unwrap().clone()
        }

        pub fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    impl ImageCacheProxy for RecordingProxy {
        fn send_image_loaded(&self, url: Url) {
            self.calls.lock().unwrap().push(url);
        }
    }

    /// Fetcher that panics if called. Used by `ImageCache::for_test()`
    /// for test contexts that never use `NetworkImage`.
    pub struct NeverFetch;

    impl HttpFetch for NeverFetch {
        fn fetch(&self, url: &Url) -> Result<Vec<u8>, FetchError> {
            panic!(
                "NeverFetch called for URL: {} — test should not trigger image fetching",
                url
            )
        }
    }
}

#[cfg(test)]
impl ImageCache {
    /// Create a test-only cache with a `NeverFetch` fetcher and
    /// `RecordingProxy`. For tests that don't use `NetworkImage` but need
    /// an `ImageCache` to exist.
    pub fn for_test() -> Arc<ImageCache> {
        use test_helpers::*;
        Arc::new(ImageCache::new(
            Arc::new(NeverFetch),
            Arc::new(RecordingProxy::new()),
        ))
    }
}

// ============================================================================
// UNIT TESTS (Layer 1)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;
    use std::thread;
    use std::time::Duration;

    /// Helper: poll a signal until it's no longer `Loading`, with a timeout.
    /// Returns the final `LoadState`.
    fn wait_until_settled(signal: &Signal<LoadState>, timeout: Duration) -> LoadState {
        let start = std::time::Instant::now();
        loop {
            let state = signal.get_cloned();
            match state {
                LoadState::Loading => {
                    if start.elapsed() > timeout {
                        panic!("Signal did not settle within {:?}", timeout);
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                _ => return state,
            }
        }
    }

    fn test_url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn cache_miss_spawns_fetch_and_returns_loading() {
        let url = test_url("https://example.com/image.png");
        let fetcher = FakeHttpFetch::new();
        fetcher.return_bytes(&url, FakeHttpFetch::red_pixel_png());
        let cache = ImageCache::new(Arc::new(fetcher), Arc::new(RecordingProxy::new()));

        let signal = cache.get_or_fetch(url.clone());

        assert_eq!(signal.get_cloned(), LoadState::Loading);
    }

    #[test]
    fn cache_hit_returns_existing_signal_without_fetch() {
        let url = test_url("https://example.com/image.png");
        let fetcher = Arc::new(FakeHttpFetch::new());
        fetcher.return_bytes(&url, FakeHttpFetch::red_pixel_png());
        let cache = ImageCache::new(fetcher.clone(), Arc::new(RecordingProxy::new()));

        let signal1 = cache.get_or_fetch(url.clone());
        // Wait for fetch to complete.
        wait_until_settled(&signal1, Duration::from_secs(1));

        let signal2 = cache.get_or_fetch(url.clone());

        // Both signals should be the same (Arc identity on the inner SignalInner).
        assert_eq!(
            signal1.get_cloned(),
            signal2.get_cloned(),
            "cache hit should return the same signal value"
        );
        // Fetcher should have been called exactly once.
        assert_eq!(
            fetcher.call_count(),
            1,
            "fetcher should not be called again on cache hit"
        );
    }

    #[test]
    fn fetch_success_sets_loaded() {
        let url = test_url("https://example.com/image.png");
        let fetcher = FakeHttpFetch::new();
        fetcher.return_bytes(&url, FakeHttpFetch::solid_color_png(4, 3, [0, 255, 0, 255]));
        let cache = ImageCache::new(Arc::new(fetcher), Arc::new(RecordingProxy::new()));

        let signal = cache.get_or_fetch(url);
        let state = wait_until_settled(&signal, Duration::from_secs(1));

        match state {
            LoadState::Loaded(data) => {
                assert_eq!(data.width, 4);
                assert_eq!(data.height, 3);
            }
            _ => panic!("expected Loaded, got {:?}", state),
        }
    }

    #[test]
    fn fetch_failure_sets_error() {
        let url = test_url("https://example.com/missing.png");
        let fetcher = FakeHttpFetch::new();
        fetcher.return_error(&url, FetchError::Network("connection refused".into()));
        let cache = ImageCache::new(Arc::new(fetcher), Arc::new(RecordingProxy::new()));

        let signal = cache.get_or_fetch(url);
        let state = wait_until_settled(&signal, Duration::from_secs(1));

        match state {
            LoadState::Error(msg) => {
                assert!(
                    msg.contains("connection refused"),
                    "error message should contain the cause: {}",
                    msg
                );
            }
            _ => panic!("expected Error, got {:?}", state),
        }
    }

    #[test]
    fn decode_failure_sets_error() {
        let url = test_url("https://example.com/garbage.bin");
        let fetcher = FakeHttpFetch::new();
        fetcher.return_bytes(&url, b"this is not an image".to_vec());
        let cache = ImageCache::new(Arc::new(fetcher), Arc::new(RecordingProxy::new()));

        let signal = cache.get_or_fetch(url);
        let state = wait_until_settled(&signal, Duration::from_secs(1));

        match state {
            LoadState::Error(msg) => {
                assert!(
                    msg.contains("Decode"),
                    "error message should mention decode: {}",
                    msg
                );
            }
            _ => panic!("expected Error, got {:?}", state),
        }
    }

    #[test]
    fn concurrent_get_or_fetch_same_url_single_fetch() {
        let url = test_url("https://example.com/shared.png");
        let fetcher = Arc::new(FakeHttpFetch::new());
        fetcher.return_bytes(&url, FakeHttpFetch::red_pixel_png());
        fetcher.with_delay(&url, Duration::from_millis(50));
        let cache = Arc::new(ImageCache::new(
            fetcher.clone(),
            Arc::new(RecordingProxy::new()),
        ));

        let cache1 = cache.clone();
        let cache2 = cache.clone();
        let url1 = url.clone();
        let url2 = url.clone();

        let h1 = thread::spawn(move || cache1.get_or_fetch(url1));
        let h2 = thread::spawn(move || cache2.get_or_fetch(url2));

        let signal1 = h1.join().unwrap();
        let signal2 = h2.join().unwrap();

        // Both should settle to Loaded.
        let state1 = wait_until_settled(&signal1, Duration::from_secs(2));
        let state2 = wait_until_settled(&signal2, Duration::from_secs(2));

        assert!(matches!(state1, LoadState::Loaded(_)));
        assert!(matches!(state2, LoadState::Loaded(_)));

        // Fetcher called exactly once despite concurrent calls.
        assert_eq!(
            fetcher.call_count(),
            1,
            "concurrent get_or_fetch for same URL should only fetch once"
        );
    }

    #[test]
    fn fetch_thread_panic_sets_error() {
        let url = test_url("https://example.com/panic.png");
        let cache = ImageCache::new(Arc::new(PanickingFetch), Arc::new(RecordingProxy::new()));

        let signal = cache.get_or_fetch(url);
        let state = wait_until_settled(&signal, Duration::from_secs(1));

        match state {
            LoadState::Error(msg) => {
                assert!(
                    msg.contains("panic"),
                    "error message should mention panic: {}",
                    msg
                );
            }
            _ => panic!("expected Error, got {:?}", state),
        }
    }

    #[test]
    fn proxy_send_image_loaded_on_completion() {
        let url = test_url("https://example.com/proxy-test.png");
        let fetcher = FakeHttpFetch::new();
        fetcher.return_bytes(&url, FakeHttpFetch::red_pixel_png());
        let proxy = Arc::new(RecordingProxy::new());
        let cache = ImageCache::new(Arc::new(fetcher), proxy.clone());

        let signal = cache.get_or_fetch(url.clone());
        wait_until_settled(&signal, Duration::from_secs(1));

        assert_eq!(
            proxy.call_count(),
            1,
            "proxy should be called once on fetch completion"
        );
        assert_eq!(proxy.calls(), vec![url]);
    }
}
