//! Typed user events for winit's `EventLoop<UserEvent>`.
//!
//! These events are sent from background threads (e.g. image fetch threads)
//! to wake the render loop via `EventLoopProxy::send_event()`.

use url::Url;

/// User events dispatched through winit's `EventLoop<UserEvent>`.
///
/// Sent from background threads to wake the render loop. Each variant
/// corresponds to a cross-thread notification that requires a frame request.
#[derive(Debug, Clone)]
pub enum VexoUserEvent {
    /// A remote image fetch completed. The payload is the URL that was
    /// fetched; the actual `ImageData` lives in the `ImageCache` and is
    /// read by `NetworkImage::render()` on rebuild. This event only needs
    /// to wake the render loop — the handler calls `request_frame()` and
    /// does not inspect the payload beyond logging.
    ImageLoaded(Url),
}
