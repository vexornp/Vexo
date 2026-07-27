//! CADisplayLink driver for vsync-rate animation on iOS.
//!
//! winit 0.31's UIKit backend drives the event loop via `CFRunLoopTimer`
//! (not `CADisplayLink`), which iOS throttles to ~15 FPS in
//! `ControlFlow::Wait` and ~15 FPS even in `ControlFlow::Poll` (software
//! timers are coalesced/throttled for power). This makes keyboard animations
//! visibly jerky: only 4-5 frames land during a 383ms animation, and the
//! final "snap" frame arrives up to 75ms after the OS keyboard finished.
//!
//! `CADisplayLink` is a Core Animation timer that fires once per display
//! refresh (60/120Hz), synced to the vsync. It is the canonical iOS way to
//! drive animations. This module wraps it in a Rust-friendly handle that
//! starts/stops the display link while animations are active.
//!
//! The display link callback calls `window.request_redraw()`, which queues a
//! `RedrawRequested` event. The render loop's interpolation driver (in
//! `WindowState::render_retain`) then advances the keyboard animation one
//! step per vsync, giving smooth 60/120 FPS motion that matches the OS
//! keyboard.

use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{define_class, msg_send, AnyThread, ClassType, DefinedClass};
use objc2_foundation::{NSRunLoop, NSRunLoopCommonModes};
use objc2_quartz_core::CADisplayLink;

/// Type-erased per-frame callback. Stored on the heap as a double-box so the
/// ivar holds a thin pointer (objc2 ivars must be `Encode`; raw `*mut ()` is).
type FrameCallback = Arc<dyn Fn() + Send + Sync>;

/// Ivars for the Objective-C target class. Holds a thin pointer to a
/// heap-allocated `Box<Box<dyn Fn()>>`.
#[repr(C)]
struct DisplayLinkTargetIvars {
    callback: *mut std::ffi::c_void,
}

unsafe impl objc2::Encode for DisplayLinkTargetIvars {
    const ENCODING: objc2::Encoding = <*mut std::ffi::c_void>::ENCODING;
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "VexoDisplayLinkTarget"]
    #[ivars = DisplayLinkTargetIvars]
    struct DisplayLinkTarget;

    impl DisplayLinkTarget {
        /// CADisplayLink selector. Called by Core Animation once per vsync
        /// while the link is running (not paused). Reads the callback pointer
        /// from ivars and invokes it.
        #[unsafe(method(tick:))]
        fn tick(&self, _sender: Option<&CADisplayLink>) {
            let ivars = self.ivars();
            if ivars.callback.is_null() {
                return;
            }
            // SAFETY: `callback` is a `*mut Box<FrameCallback>` set in
            // `DisplayLink::new` and kept alive by the `DisplayLink` struct
            // (freed in `Drop` only after `invalidate()` guarantees no more
            // `tick` calls). The `DisplayLink` outlives the target because it
            // owns the `_target` Retained. The callback is `Send + Sync` so
            // calling from the main-thread display-link callback is safe.
            log::debug!("[KBDBG] display-link tick");
            unsafe {
                let boxed = &*(ivars.callback as *mut Box<FrameCallback>);
                (boxed)();
            }
        }
    }
);

/// Handle to a `CADisplayLink` that drives `window.request_redraw()` each
/// vsync while running. Created once per window; started/stopped as
/// animations come and go.
pub struct DisplayLink {
    link: Retained<CADisplayLink>,
    _target: Retained<DisplayLinkTarget>,
    /// Raw pointer to the heap-allocated `Box<Box<FrameCallback>>`. Owned by
    /// this struct; the Objective-C target's ivar holds a borrow of it.
    /// Freed in `Drop` AFTER the display link is invalidated, so `tick` can
    /// never read a dangling pointer.
    callback_raw: *mut Box<FrameCallback>,
    running: bool,
}

// SAFETY: `CADisplayLink` and its target must be used on the main thread
// (CADisplayLink is registered on the main run loop). The callback is
// `Send + Sync`. In practice `DisplayLink` is only touched from the main
// thread (winit event loop), so this impl is conservative.
unsafe impl Send for DisplayLink {}
unsafe impl Sync for DisplayLink {}

impl DisplayLink {
    /// Create a display link that calls `on_frame` each vsync while running.
    /// Starts paused; call `start()` to begin firing.
    pub fn new(on_frame: Arc<dyn Fn() + Send + Sync>) -> Self {
        // Double-box: outer Box gives us a stable thin pointer to store in
        // the ivar; inner Arc holds the trait object. The outer Box is kept
        // alive for the lifetime of the `DisplayLink` (freed in `Drop`).
        let callback: Box<FrameCallback> = Box::new(on_frame);
        let callback_raw: *mut Box<FrameCallback> = Box::into_raw(Box::new(callback));

        let target = DisplayLinkTarget::alloc().set_ivars(DisplayLinkTargetIvars {
            callback: callback_raw as *mut std::ffi::c_void,
        });
        // SAFETY: Call NSObject's init on the allocated target.
        let target: Retained<DisplayLinkTarget> = unsafe { msg_send![super(target), init] };

        // SAFETY: `target` is a valid NSObject; `tick:` is a valid selector
        // defined on `DisplayLinkTarget`.
        let link: Retained<CADisplayLink> =
            unsafe { CADisplayLink::displayLinkWithTarget_selector(&**target, objc2::sel!(tick:)) };

        // Register on the main run loop for common modes (default + tracking)
        // so the link fires during scroll/touch tracking too.
        let runloop = NSRunLoop::mainRunLoop();
        unsafe {
            link.addToRunLoop_forMode(&runloop, &NSRunLoopCommonModes);
        }

        // Start paused.
        link.setPaused(true);

        Self {
            link,
            _target: target,
            callback_raw,
            running: false,
        }
    }

    /// Begin firing the display link each vsync. Idempotent.
    pub fn start(&mut self) {
        if !self.running {
            self.link.setPaused(false);
            self.running = true;
        }
    }

    /// Pause the display link. Idempotent.
    pub fn stop(&mut self) {
        if self.running {
            self.link.setPaused(true);
            self.running = false;
        }
    }

    /// Whether the link is currently firing.
    pub fn is_running(&self) -> bool {
        self.running
    }
}

impl Drop for DisplayLink {
    fn drop(&mut self) {
        // Invalidate FIRST to remove the display link from the run loop and
        // guarantee no further `tick` callbacks will fire. We're on the main
        // thread (DisplayLink is owned by WindowState in the winit event
        // loop), and `tick` also runs on the main thread, so there's no
        // concurrent callback to worry about.
        self.link.invalidate();
        // SAFETY: After `invalidate`, `tick` will never be called again, so
        // the callback pointer is safe to reclaim.
        unsafe {
            drop(Box::from_raw(self.callback_raw));
        }
    }
}
