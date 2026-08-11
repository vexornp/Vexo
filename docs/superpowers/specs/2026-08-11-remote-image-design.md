# Remote Image Loading Design Spec

## Problem

Vexo's `Image` widget (`vexo/src/widgets/image.rs:10`) only displays pre-decoded
`ImageData` — typically from `include_bytes!` at compile time. Real apps need to
display images fetched from remote URLs at runtime (user avatars, photo feeds,
illustrations). This spec adds that capability.

## Scope

**v1: Desktop only** (macOS/Linux/Windows). iOS/Android are out of scope — the
HTTP layer is desktop-specific for now, but the framework-core design
(`ImageCache`, `NetworkImage`, `HttpFetch` trait) is platform-agnostic and
extends to mobile when a platform-native fetcher is added later.

## Requirements

- Load and render images from remote URLs (JPEG/PNG).
- `NetworkImage` widget wraps the existing synchronous `Image` widget.
- In-memory per-process cache keyed by URL: one fetch per URL, shared across
  all `NetworkImage` widgets.
- Builder-closure API for loading placeholder and error states.
- Cross-thread wake-up of the desktop render loop when a fetch completes (the
  loop sleeps when idle; without a wake-up, the loaded image would never
  render).

## Non-Goals (v1)

- Disk cache / persistence across app restarts.
- LRU eviction or byte-cap eviction (cache grows unbounded with distinct URLs).
- Fetch retry on failure (errors are terminal; reload requires app restart or a
  new URL).
- Fetch cancellation when all subscribers unmount mid-fetch.
- Configurable HTTP timeouts (uses `ureq` defaults: 30s connect + 30s read).
- Connection pooling across fetches to the same host.
- WebP/GIF support (JPEG/PNG only; add `image` crate features in
  `vexo_http_ureq` to extend).
- Mobile (iOS/Android) HTTP fetcher.

## Future Refinements

- **`depend_on_signal` subscription accumulation.** When a `NetworkImage`
  widget's URL changes (e.g. recycled in a list), the element remains
  subscribed to the *old* URL's `Signal` in addition to the new one, because
  `Signal::add_subscriber` accumulates weak refs and there is no
  clear-subscriptions-on-rebuild mechanism. The stale subscription is harmless
  (the old URL's fetch won't re-fire) but wasteful. A framework-level fix
  (clearing subscriptions on rebuild) is deferred.

---

## Architecture Overview

Three pieces fit together:

```
┌─────────────────────────────────────────────────────────────┐
│  Widget tree (main thread)                                  │
│                                                             │
│  NetworkImage(url)  ──depend_on_signal──►  ImageCache       │
│       │                                        │            │
│       │ render() reads LoadState               │ holds       │
│       ▼                                        ▼            │
│  Image / placeholder / error widget    Signal<LoadState>    │
│                                  per URL, shared             │
│                                       │                     │
└───────────────────────────────────────┼─────────────────────┘
                                        │
                          ┌─────────────┼──────────────┐
                          │             │              │
                   ┌──────▼──────┐  ┌───▼────┐  ┌──────▼───────┐
                   │ Fetch thread│  │ Cache  │  │ ImageCacheProxy│
                   │ (spawned)   │  │ lookup │  │ (winit adapter)│
                   └──────┬──────┘  └────────┘  └──────┬───────┘
                          │                            │
                  ureq GET + decode                    │
                          │                            │
                          ▼                            │
                   signal.set(Loaded)                  │
                          │                            │
                          └────────────► proxy.send_image_loaded(url)
                                                       │
                                      ┌────────────────┘
                                      ▼
                          ApplicationHandler::user_event()
                                      │
                                      ▼
                              window.request_redraw()
                                      │
                                      ▼
                          dirty element rebuilds → renders Image
```

### Key flows

1. **Mount**: `NetworkImage::render()` calls
   `ImageCache::get_or_fetch(url)`. First call for a URL spawns a fetch thread
   and returns `Signal<Loading>`. Subsequent calls (same URL, same or different
   widget) return the same Signal — one fetch, many subscribers.

2. **Completion**: Fetch thread decodes bytes → `ImageData::from_bytes()` →
   `signal.set(Loaded(data))` (notifies all subscribed widgets via the existing
   weak-ref dirty callback mechanism) → `proxy.send_image_loaded(url)` (wakes
   the loop).

3. **Render**: `user_event` handler calls `request_frame()` → next
   `RedrawRequested` → pipeline drains dirty elements → `NetworkImage::render()`
   now reads `Loaded` from the Signal → builds an `Image` widget with the
   decoded `ImageData`.

### Why the proxy wake-up is needed

`signal.set()` marks the element dirty in the `BuildOwner`, but on desktop the
render loop sleeps when idle (`poll_idle_frame_drivers` at `window.rs:545` only
wakes for cursor-blink or active animations). The `send_event` is the wake-up
kick. On iOS/Android (future), the always-on CADisplayLink already wakes every
vsync, so the proxy is desktop-only in effect but harmless elsewhere.

---

## `ImageCache`

The singleton cache sits in `vexo` (not `shared_app`) so any app can use it.
It's framework infrastructure, like `AnimationTicker`.

### Location & construction

- New module: `vexo/src/image_cache.rs`.
- Exported from `vexo/src/lib.rs` alongside `Image`/`ImageData`.
- Constructed once per `WindowState` (alongside `AnimationTicker`, `clipboard`,
  etc. at `window.rs:143`). Stored as `Arc<ImageCache>` on `WindowState`, cloned
  into the `ThreeTreePipeline` which hands it to `RenderContext`.

### Why `RenderContext` needs access

`NetworkImage::render()` must call `cache.get_or_fetch(url)`. Today
`RenderContext` (`stateful_widget.rs:297`) has no image-cache field. We add one
— same pattern as `build_owner` and `inherited_map` being passed through. The
cache is `Arc`, so the clone is cheap.

This is a narrow, additive change to `RenderContext::new` (one new field) and
its construction sites. Existing call sites that don't use `NetworkImage` pass
`ImageCache::for_test()`.

### Data structures

```rust
pub struct ImageCache {
    entries: Mutex<HashMap<Url, Arc<CacheEntry>>>,
    proxy: Arc<dyn ImageCacheProxy>,
    fetcher: Arc<dyn HttpFetch>,
}

struct CacheEntry {
    state: Signal<LoadState>,
}

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum LoadState {
    Loading,
    Loaded(ImageData),
    Error(String),
}
```

`LoadState` must implement `PartialEq + Clone + Send + Sync` to satisfy
`Signal<T>` and `depend_on_signal` bounds (`stateful_widget.rs:388`). `Send +
Sync` hold because `ImageData` (`Vec<u8>` + `u32` + `u32`) and `String` are
`Send + Sync`. `PartialEq` on `LoadState` derives `PartialEq` on `ImageData` —
which requires adding `#[derive(PartialEq)]` to `ImageData` (see Files section).
The pixel-vec comparison happens only once per fetch completion (on
`Signal::set_from`), not per frame, so the O(n) cost is acceptable.

- `Url` is `url::Url` (parsed once at `get_or_fetch` time; stored as the key so
  identical strings collapse).
- `entries` is `Mutex` not `RefCell` — fetch threads are real OS threads, must
  be `Send`-lockable. Interior mutability mirrors `BuildOwner`'s `RefCell`
  pattern but thread-safe.
- `proxy` is `Arc<dyn ImageCacheProxy>` — always present; production installs a
  winit adapter, tests install a `RecordingProxy`.

### `HttpFetch` trait

```rust
pub trait HttpFetch: Send + Sync {
    fn fetch(&self, url: &Url) -> Result<Vec<u8>, FetchError>;
}
```

Injected at cache construction, not per-call. `WindowState` installs a
`UreqHttpFetch` (real impl) in production; tests install a `FakeHttpFetch`
(returns canned bytes / errors / delays).

### `ImageCacheProxy` trait

```rust
pub trait ImageCacheProxy: Send + Sync {
    fn send_image_loaded(&self, url: Url);
}
```

Decouples the cache from `EventLoopProxy<VexoUserEvent>` so the cache is fully
testable without winit. Production adapter:

```rust
struct WinitImageCacheProxy {
    proxy: EventLoopProxy<VexoUserEvent>,
}
impl ImageCacheProxy for WinitImageCacheProxy {
    fn send_image_loaded(&self, url: Url) {
        let _ = self.proxy.send_event(VexoUserEvent::ImageLoaded(url));
    }
}
```

Tests use `RecordingProxy` that records calls.

### `get_or_fetch` — the only public method

```rust
impl ImageCache {
    pub fn new(fetcher: Arc<dyn HttpFetch>, proxy: Arc<dyn ImageCacheProxy>) -> Self;
    pub fn get_or_fetch(&self, url: Url) -> Signal<LoadState>;
}
```

On cache miss: insert `Loading` entry, `std::thread::spawn` a closure that
captures `fetcher.clone()`, `url.clone()`, the `Signal` (clone), and a clone of
`self.proxy`. Closure calls `fetcher.fetch(&url)`, decodes via
`ImageData::from_bytes`, sets the signal via `signal.set_from(&state)` (the
`Clone`-bound `set_from` at `reactive/mod.rs:128`, not the `Copy`-bound `set`),
pings the proxy. The fetch+decode is wrapped in `std::panic::catch_unwind` so a
panic in the fetch thread converts to `LoadState::Error` rather than leaving the
signal stuck at `Loading` forever.

On cache hit (`Loading` or `Loaded` or `Error`): return the existing Signal
without spawning. No duplicate fetches per URL.

### Eviction

**No eviction in v1.** The map grows with distinct URLs for the app's lifetime.
This matches the original image-widget spec's trade-off. A real app loading
thousands of unique remote images would need LRU + byte-cap eviction —
explicitly out of scope.

### Stale-fetch handling

No cancellation: if all subscribed widgets unmount mid-fetch, the thread still
completes and the entry stays in the cache (now `Loaded`, no subscribers). The
work was already in flight, and the cached result benefits a future mount.

### Threading model

- `entries` lock is held only briefly: lookup, insert-Loading. Never held during
  fetch or decode.
- `Signal<LoadState>` is `Arc`-backed and `Send + Sync` (existing
  `reactive/mod.rs`), safe to move into the fetch thread.
- `proxy.send_image_loaded` requires `ImageCacheProxy: Send + Sync` (the winit
  adapter wraps `EventLoopProxy` which winit guarantees is `Send + Sync`).
- Decode happens on the fetch thread (off the main thread) — `image::decode` is
  CPU work.

---

## `NetworkImage` widget

The user-facing component. A `Component` (stateful), not a leaf `Widget` — it
needs `RenderContext` to reach the cache and `depend_on_signal` for rebuilds.

### Structure

```rust
pub struct NetworkImage {
    url: Url,
    placeholder: Option<Box<dyn Fn() -> Box<dyn Widget> + Send + Sync>>,
    error: Option<Box<dyn Fn(&str) -> Box<dyn Widget> + Send + Sync>>,
    key: Option<WidgetKey>,
}
```

- `placeholder`: closure called when `LoadState::Loading` (or when no entry
  exists yet). Returns a widget to show while waiting. `None` renders nothing.
- `error`: closure called with the error message string when
  `LoadState::Error`. `None` renders nothing on error.
- Both closures are `Send + Sync` because `NetworkImage` is cloned during
  reconciliation (widgets are rebuilt each frame) and the closures must survive
  the clone — matching `GestureDetector`'s callback bounds at
  `widgets/mod.rs:199`.
- Builder methods: `.placeholder(|| Text::new("Loading…").boxed())`,
  `.error(|e| Text::new(e).boxed())`.
- `with_key(url.as_str())` should be used when `NetworkImage` appears in a list,
  so reconciliation reuses the element when the list reorders.

### State

```rust
#[derive(Default)]
pub struct NetworkImageState;

impl ComponentState for NetworkImageState {}
```

No reactive fields — `depend_on_signal` in `render()` handles subscription
automatically (the existing read-tracking path at `stateful_widget.rs:388`).
`SimpleState<()>` also works; a custom state type is used only if lifecycle
hooks are needed later.

### `render()`

```rust
fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
    let signal = ctx.image_cache().get_or_fetch(self.url.clone());
    let load_state = ctx.depend_on_signal(&signal);

    match load_state {
        LoadState::Loading => self.placeholder.as_ref()
            .map(|f| f())
            .unwrap_or_else(empty_widget),
        LoadState::Loaded(data) => Image::new(data.clone()).boxed(),
        LoadState::Error(msg) => self.error.as_ref()
            .map(|f| f(msg))
            .unwrap_or_else(empty_widget),
    }
}
```

- `ctx.image_cache()` — new `RenderContext` accessor, returns `&ImageCache`.
- `ctx.depend_on_signal(&signal)` — establishes the rebuild edge. Every
  `signal.set` (Loading→Loaded transition on the fetch thread) marks this
  element dirty → rebuild → re-render reads `Loaded`.
- `data.clone()` — `ImageData` is `Clone` (vec of pixels). The clone happens
  once per rebuild after load, not per frame (once `Loaded`, the signal value is
  stable, `Signal::set` no-ops on equal values per `reactive/mod.rs:117`).
- The widget rebuilds exactly twice per URL: once on mount (`Loading`), once on
  completion (`Loaded`). After that, no rebuilds unless the URL changes.

### `should_rebuild`

Default `true` — `NetworkImage` is not in a hot path like keyboard animation or
scroll. No override needed.

### URL changes

When the parent rebuilds `NetworkImage` with a different URL (e.g. recycled in a
list), `render()` calls `get_or_fetch(self.url)` with the *new* URL, returning
the new URL's signal. The old URL's signal subscription does not actively
expire — see "Future Refinements" above.

---

## winit `EventLoop<UserEvent>` plumbing (C2)

The most invasive change. winit's `EventLoop` gains a typed user event so the
fetch thread can wake the loop via `proxy.send_event()`.

### `VexoUserEvent` enum

```rust
// vexo/src/user_event.rs
#[derive(Debug, Clone)]
pub enum VexoUserEvent {
    /// A remote image fetch completed. The payload is the URL that was fetched;
    /// the actual ImageData lives in the ImageCache and is read by
    /// NetworkImage::render() on rebuild. This event only needs to wake the
    /// render loop — the handler calls request_frame() and does not inspect the
    /// payload beyond logging.
    ImageLoaded(Url),
}
```

Extensible — future cross-thread wake-ups (async font loading, future network
operations) add variants here.

### `EventLoop` construction — two sites

Only two construction sites exist in the codebase (no separate `run_ios`; iOS
calls `run_desktop_demo` via `shared_app/src/app.rs:185`):

1. **`vexo/src/lib.rs:369`** `run_desktop_demo`:
   ```rust
   // Before:
   let event_loop = EventLoop::new()?;
   // After:
   let event_loop: EventLoop<VexoUserEvent> = EventLoop::with_user_event().build()?;
   ```

2. **`vexo/src/lib.rs:424`** `run_android_demo`:
   ```rust
   // Before:
   let event_loop = EventLoop::builder().with_android_app(app).build()?;
   // After:
   let event_loop = EventLoop::<VexoUserEvent>::with_user_event()
       .with_android_app(app)
       .build()?;
   ```

### Proxy ownership flow

```
run_desktop_demo:
  event_loop = EventLoop::<VexoUserEvent>::with_user_event().build()?
  proxy = event_loop.create_proxy()
  image_cache = Arc::new(ImageCache::new(
      fetcher,                              // app-supplied
      Arc::new(WinitImageCacheProxy::new(proxy)),
  ))
  VexoApp::new(image_cache)               // mpsc channel removed

VexoApp:
  holds Arc<ImageCache>
  try_init_framework_state → WindowState::new(window, image_cache.clone())

WindowState::new:
  receives Arc<ImageCache>
  passes to ThreeTreePipeline
  pipeline threads to RenderContext::image_cache()
```

`EventLoopProxy` is `Clone`, but we create it once and install into the
singleton cache.

### `VexoApp` changes

```rust
pub struct VexoApp<A: Application + 'static> {
    image_cache: Arc<ImageCache>,
    windows: HashMap<WindowId, WindowState<A>>,
}
```

- `VexoApp::new` signature changes from `new(&EventLoop, Receiver<KeyBindingAction>, Sender<KeyBindingAction>)` to `new(Arc<ImageCache>)` (mpsc channel + event loop ref removed, image cache added).
- Implements `ApplicationHandler<VexoUserEvent>` instead of `ApplicationHandler`.
- New `user_event` handler:
  ```rust
  fn user_event(&mut self, _event_loop: &dyn ActiveEventLoop, event: VexoUserEvent) {
      match event {
          VexoUserEvent::ImageLoaded(url) => {
              log::debug!("ImageLoaded event: {}", url);
              for state in self.windows.values_mut() {
                  state.request_frame();
              }
          }
      }
  }
  ```
- The payload URL is unused beyond logging — dirty elements already know what to
  rebuild from the Signal. The event is purely a wake-up kick.
- Multi-window: iterate all windows and request frames.

### Dead scaffolding removed

The existing `proxy_wake_up` handler (`app.rs:109`) and its `KeyBindingAction`
mpsc channel (`app.rs:22-24`, `lib.rs:370-386`) are dead scaffolding replaced by
the typed `user_event`. The spec removes:
- The `KeyBindingAction` enum.
- The mpsc channel (`sender`/`receiver` fields on `VexoApp`, the `mpsc::channel`
  in `run_desktop_demo`).
- The `proxy_wake_up` method.
- The commented-out spawn thread in `lib.rs:376-385`.

### `run_*` signature change

Both `run_desktop_demo` and `run_android_demo` gain a parameter:
`image_fetcher: Arc<dyn HttpFetch>`. The app crate supplies the concrete
fetcher; the framework constructs the cache (with proxy + fetcher).

```rust
pub fn run_desktop_demo<A: Application + 'static>(
    image_fetcher: Arc<dyn HttpFetch>,
) -> Result<(), Box<dyn Error>>
```

---

## HTTP client & dependencies

### Crate choice: `ureq`

- Blocking API, no async runtime. Matches the "spawn thread + blocking call"
  model exactly — no `tokio` needed.
- Pure-Rust TLS via `rustls` (no OpenSSL system dependency, builds clean on
  macOS/Linux/Windows).
- Small dependency footprint.
- Mature, maintained.

### Dependency placement: separate `vexo_http_ureq` crate

```
vexo_http_ureq/
  Cargo.toml    — depends on vexo (for HttpFetch trait), ureq, url
  src/lib.rs    — UreqHttpFetch struct, impl HttpFetch
```

**Why separate, not inside `vexo`?** `vexo` must build for iOS, Android, and
desktop. `ureq` + `rustls` add weight and build time to every platform, even
ones that won't use the ureq fetcher (mobile will eventually use
platform-native HTTP). Keeping `ureq` in a separate desktop-optional crate means:
- `vexo` stays HTTP-implementation-agnostic — only defines the `HttpFetch`
  trait.
- `vexo_http_ureq` is a desktop-only dependency. `shared_app` opts in.
- Future `vexo_http_ios` / `vexo_http_android` crates implement the same trait
  with platform-native clients, no changes to `vexo`.

### Workspace `Cargo.toml` additions

```toml
[workspace.dependencies]
url = { version = "2", features = ["serde"] }
ureq = { version = "2", features = ["tls"] }
```

`url` is needed by `vexo` itself (for `Url` in `ImageCache` and
`VexoUserEvent`). `ureq` is workspace-dep'd for version centralization but only
pulled in by `vexo_http_ureq`.

### `vexo/Cargo.toml`

```toml
url = { workspace = true }
```

`vexo` does **not** depend on `ureq`.

### `UreqHttpFetch` implementation

```rust
pub struct UreqHttpFetch;

impl HttpFetch for UreqHttpFetch {
    fn fetch(&self, url: &Url) -> Result<Vec<u8>, FetchError> {
        let response = ureq::get(url.as_str())
            .call()
            .map_err(|e| FetchError::Network(e.to_string()))?;

        const MAX_BYTES: u64 = 10 * 1024 * 1024;
        let content_length = response.header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok());
        if let Some(len) = content_length {
            if len > MAX_BYTES {
                return Err(FetchError::TooLarge(len));
            }
        }

        let mut bytes = Vec::new();
        response.into_reader()
            .take(MAX_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|e| FetchError::Io(e.to_string()))?;

        Ok(bytes)
    }
}
```

- Stateless struct. A future optimization would hold an `ureq::Agent` for
  connection reuse; v1 uses the simple stateless form.
- `MAX_BYTES` cap (10MB) prevents unbounded memory on malicious/large responses.
  Enforced twice: via `Content-Length` header (fast-fail) and via
  `take(MAX_BYTES)` on the reader (handles servers that lie about length).
- No timeout in v1 — `ureq` defaults to 30s connect + 30s read.

### `FetchError`

```rust
#[derive(Clone, Debug)]
pub enum FetchError {
    Network(String),
    Io(String),
    TooLarge(u64),
    Decode(String),  // set by the cache after ImageData::from_bytes fails
}
```

`Decode` is populated by the cache, not the fetcher — the fetcher returns raw
bytes; the cache decodes via `ImageData::from_bytes`. If decode fails, the cache
sets `Signal<LoadState>::Error(...)` with the decode message.

### Image format support

The `image` crate is a workspace dep with `default-features = false,
features = ["jpeg"]` at the workspace level (`Cargo.toml:66`), and `shared_app`
adds `features = ["png"]`. Cargo features are additive, so the final build has
both JPEG and PNG. **v1: JPEG + PNG only.** Unsupported formats surface as
`LoadState::Error("Decode failed: ...")`. To add WebP/GIF later, add the
corresponding `image` crate features to `vexo_http_ureq/Cargo.toml`.

### Dependency flow

```
vexo: defines HttpFetch trait, ImageCache, NetworkImage, VexoUserEvent
  └─ depends on: url

vexo_http_ureq: production desktop fetcher
  └─ depends on: vexo, ureq, url

shared_app: wires UreqHttpFetch into run_desktop_demo
  └─ depends on: vexo, vexo_http_ureq

desktop_demo / android_demo / VexoDemo: unchanged except run_* signature
```

---

## Error handling

| Error | Behavior | User-facing |
|---|---|---|
| Network failure | `LoadState::Error`, sticky | Error widget shown |
| Decode failure (bad bytes / unsupported format) | `LoadState::Error`, sticky | Error widget shown |
| Response too large (>10MB) | `LoadState::Error`, sticky | Error widget shown |
| Proxy send fails (app shutting down) | `log::debug!`, no propagation | None (app exiting) |
| Fetch thread panic (e.g. ureq internal) | `catch_unwind` → `LoadState::Error` | Error widget shown |

### Fetch errors → `LoadState::Error`

The cache's fetch thread maps both fetch and decode failures into
`LoadState::Error(String)`. The widget's `error` closure receives the string.

### No retry (v1)

Errors are terminal. An `Error` entry is cached just like a `Loaded` entry —
re-subscribing to the same URL shows the error immediately without re-fetching.
This prevents a tight retry loop if the server is down and list scrolling
repeatedly triggers subscriptions. A future "retry on explicit user action" API
(e.g. `NetworkImage::retry_on_tap(true)`) could layer on top without changing
the cache.

### Proxy wake-up failure

`EventLoopProxy::send_event` returns `Result<(), EventLoopClosed<VexoUserEvent>>`.
This fails only if the event loop has been dropped (app shutting down). The
adapter logs at `debug` and drops the event. No panic, no propagation.

### Fetch thread panic

The fetch+decode closure is wrapped in `std::panic::catch_unwind`. A panic
converts to `LoadState::Error(format!("panic: {}", payload))`. Without this, a
panic would leave the signal at `Loading` forever — the widget would show a
placeholder indefinitely with no indication of failure.

### Threading panics (pre-existing)

`Signal::set` acquires a `Mutex` (`reactive/mod.rs:117`). If a subscriber
callback panics, the `Mutex` is poisoned. The existing `Signal` code uses
`.lock().unwrap()` without poison recovery. This is pre-existing behavior;
`NetworkImage` doesn't change it. The dirty callbacks are framework-installed
and don't panic in practice.

---

## Testing

Three layers, bottom-up.

### Layer 1: `ImageCache` unit tests (no HTTP, no winit)

```rust
struct FakeHttpFetch {
    responses: Mutex<HashMap<Url, Result<Vec<u8>, FetchError>>>,
    delays: Mutex<HashMap<Url, Duration>>,
    calls: Mutex<Vec<Url>>,
}

struct RecordingProxy {
    calls: Mutex<Vec<Url>>,
}
```

**Tests:**

1. `cache_miss_spawns_fetch_and_returns_loading` — `get_or_fetch` for a new URL
   returns `Loading`. Fetcher called once.
2. `cache_hit_returns_existing_signal_without_fetch` — second `get_or_fetch` for
   the same URL returns the same `Signal` (Arc identity). Fetcher not called
   again.
3. `fetch_success_sets_loaded` — fake fetcher returns valid PNG bytes. Poll
   signal until `Loaded`. Assert `ImageData` dimensions.
4. `fetch_failure_sets_error` — fake returns `FetchError::Network`. Poll until
   `Error`. Assert message.
5. `decode_failure_sets_error` — fake returns garbage bytes. Poll until `Error`.
   Assert message contains "Decode".
6. `concurrent_get_or_fetch_same_url_single_fetch` — two threads call
   `get_or_fetch` simultaneously. Fetcher called exactly once. Both receive the
   same `Signal`.
7. `fetch_thread_panic_sets_error` — fake fetcher panics. Poll until `Error`
   (catch_unwind converts it). Assert message contains "panic".
8. `proxy_send_image_loaded_on_completion` — `RecordingProxy` records the URL on
   successful fetch.

### Layer 2: `NetworkImage` widget tests (no HTTP, no real cache)

```rust
struct StubCache {
    signals: Mutex<HashMap<Url, Signal<LoadState>>>,
}
// get_or_fetch returns a pre-made signal for the URL
```

**Tests:**

1. `loading_state_renders_placeholder` — stub returns `Loading`. `render()`
   produces placeholder widget. Verify by downcast.
2. `loading_state_renders_nothing_when_no_placeholder` — no placeholder closure.
   Empty widget.
3. `loaded_state_renders_image` — stub returns `Loaded(test_data)`. `render()`
   produces `Image`. Verify by downcast.
4. `error_state_renders_error_widget` — stub returns `Error("...")`. Error
   closure's widget produced.
5. `error_state_renders_nothing_when_no_error_closure` — no error closure. Empty
   widget.
6. `depend_on_signal_establishes_rebuild_edge` — full `StatefulElement` mount +
   `signal.set(Loaded)` + `pipeline.perform_rebuilds`. Verify element rebuilt
   and rendered widget changed from placeholder to `Image`.

### Layer 3: Integration test (real cache, real pipeline, fake HTTP)

`vexo/tests/network_image_integration.rs`. No winit `EventLoop` — drives frames
manually via `pipeline.render_retain()`, matching `vexo/src/integration_tests.rs`.

**Tests:**

1. `network_image_loads_and_renders_after_fetch` — happy path: Loading → fetch
   completes → Loaded → `RenderCommand::Image` appears with valid `image_key`.
2. `network_image_error_renders_error_widget` — fetch fails → `Error` → no
   `Image` render command.
3. `two_network_images_same_url_single_fetch` — two widgets, same URL.
   `FakeHttpFetch` called once. Both transition to `Loaded`.
4. `network_image_url_change_refetches` — widget rebuilds with URL A (loads),
   then URL B. URL B fetches and loads.

### What's NOT tested

- **Real HTTP requests** — `ureq` against a live server is flaky in CI. A smoke
  test could live in `vexo_http_ureq` marked `#[ignore]` (run manually). Not
  required for v1.
- **`EventLoopProxy` wake-up** — requires a real `EventLoop` + display server.
  The `RecordingProxy` in Layer 1 verifies the cache calls
  `send_image_loaded`; the `user_event` handler is a one-liner verified by
  inspection.
- **GPU image rendering** — already covered by existing `ImageRenderObject`
  tests (`render_objects/image.rs:162-287`). `NetworkImage` just produces an
  `Image` widget; the GPU path is unchanged.

---

## Files to create/modify

### New files

| File | Purpose |
|---|---|
| `vexo/src/image_cache.rs` | `ImageCache`, `CacheEntry`, `LoadState`, `HttpFetch` trait, `ImageCacheProxy` trait, `FetchError`, `WinitImageCacheProxy` |
| `vexo/src/widgets/network_image.rs` | `NetworkImage` component, `NetworkImageState` |
| `vexo/src/user_event.rs` | `VexoUserEvent` enum |
| `vexo_http_ureq/Cargo.toml` | Crate manifest |
| `vexo_http_ureq/src/lib.rs` | `UreqHttpFetch` production impl |
| `vexo/tests/network_image_integration.rs` | Layer 3 integration tests |
| `docs/superpowers/specs/2026-08-11-remote-image-design.md` | This design doc |

### Modified files — framework core (`vexo`)

| File | Change |
|---|---|
| `vexo/Cargo.toml` | Add `url = { workspace = true }` |
| `vexo/src/image_data.rs` | Add `PartialEq` to the `#[derive(...)]` on `ImageData` (needed for `Signal<LoadState>` bounds). |
| `vexo/src/lib.rs` | `run_desktop_demo` / `run_android_demo`: `EventLoop::with_user_event()`, construct proxy + cache, pass to `VexoApp::new`. Remove dead `KeyBindingAction` mpsc scaffolding (lines 370-386). Add `image_fetcher: Arc<dyn HttpFetch>` parameter to both `run_*` functions. |
| `vexo/src/app.rs` | `VexoApp<A>`: add `image_cache: Arc<ImageCache>` field. `VexoApp::new` gains `image_cache` param. Implement `ApplicationHandler<VexoUserEvent>`. Add `user_event()` handler calling `request_frame`. Remove `proxy_wake_up` + `KeyBindingAction` channel draining. Pass `image_cache` to `WindowState::new`. |
| `vexo/src/window.rs` | `WindowState::new` gains `image_cache: Arc<ImageCache>` param, stores it, passes to `ThreeTreePipeline`. |
| `vexo/src/pipeline.rs` | `ThreeTreePipeline`: store `Arc<ImageCache>`, thread to `RenderContext`. |
| `vexo/src/stateful_widget.rs` | `RenderContext` gains `image_cache: &'a ImageCache` field. `RenderContext::new` gains param. Add `RenderContext::image_cache()` accessor. Update all `RenderContext::new` call sites. |
| `vexo/src/widgets/mod.rs` | Add `mod network_image;` + `pub use network_image::NetworkImage;` |
| `Cargo.toml` (workspace) | Add `url`, `ureq` to `[workspace.dependencies]` |

### Modified files — app crates

| File | Change |
|---|---|
| `shared_app/Cargo.toml` | Add `vexo_http_ureq = { path = "../vexo_http_ureq" }` |
| `shared_app/src/app.rs` | `MobileApp::start_app` / `Application` wiring: construct `Arc::new(UreqHttpFetch)`, pass to `run_desktop_demo`. |
| `desktop_demo/src/main.rs` | Pass `Arc::new(UreqHttpFetch)` to `run_desktop_demo` (or delegate to `shared_app`). |
| `android_demo/src/lib.rs` | Pass `Arc::new(UreqHttpFetch)` to `run_android_demo`. |

### `RenderContext::new` call sites (12 total)

Mechanical updates to add the `image_cache` parameter:

- `vexo/src/stateful_widget.rs` (7 sites: lines 560, 1421, 1432, 1443, 1491, 1511, 1534)
- `vexo/src/widgets/memo.rs` (2 sites: lines 257, 289)
- `vexo_uikit/tests/navigation_stack_tests.rs` (line 212)
- `vexo_uikit/tests/navigation_animation_tests.rs` (line 40)
- `vexo_uikit/tests/button_render_tests.rs` (line 19)

Tests that don't touch `NetworkImage` use `ImageCache::for_test()`:

```rust
#[cfg(test)]
impl ImageCache {
    pub fn for_test() -> Arc<ImageCache> {
        Arc::new(ImageCache::new(
            Arc::new(NeverFetch),  // panics if called
            Arc::new(RecordingProxy::default()),
        ))
    }
}
```

### Public API surface

Exported from `vexo`:

```rust
pub use image_cache::{ImageCache, HttpFetch, ImageCacheProxy, LoadState, FetchError};
pub use widgets::network_image::NetworkImage;
pub use user_event::VexoUserEvent;
```

Exported from `vexo_http_ureq`:

```rust
pub use UreqHttpFetch;
```

### Demo in `shared_app`

Add a `NetworkImage` to the `Me` profile screen header
(`shared_app/src/me/profile_screen.rs:405`), pointing at a placeholder avatar
URL. Isolated demo spot that doesn't affect the chat flow. The existing
embedded-byte avatars stay to prove both paths work.
