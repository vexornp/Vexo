//! iOS keyboard observer — bridges UIKit keyboard notifications to
//! [`KeyboardInsetSource`](crate::core::KeyboardInsetSource).
//!
//! Registers for `UIResponder.keyboardWillShowNotification` and
//! `keyboardWillHideNotification` on the default `NotificationCenter`.
//! On each notification, extracts the keyboard's end-frame height,
//! animation duration, and animation curve from `userInfo`, converts to
//! logical pixels, and writes them into the source via `set_target`.
//!
//! # Thread safety
//!
//! All UIKit calls happen on the main thread. `WindowState` constructs the
//! observer during window init (main thread), and UIKit delivers keyboard
//! notifications on the main thread. The observer holds a clone of the
//! `KeyboardInsetSource` (an `Arc`-atomic — `Send + Sync`) so the closure
//! can write to it from the notification callback without additional
//! marshalling.
//!
//! The observer itself is **not** `Send`: the `Retained<ProtocolObject<dyn
//! NSObjectProtocol>>` tokens returned by `NotificationCenter` are
//! main-thread-affine. `WindowState` is single-threaded on iOS, so this is
//! fine. If a future caller needs to move the observer across threads, a
//! main-thread hop must be added — don't blindly `unsafe impl Send`.

use core::ffi::c_void;
use core::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_foundation::{
    NSDictionary, NSNotification, NSNotificationCenter, NSNumber, NSObject, NSString, NSValue,
};

use crate::core::{KeyboardCurve, KeyboardInsetSource};

/// Keyboard notification / userInfo-key names.
///
/// These are `NS_TYPED_EXTENSIBLE_ENUM` constants declared in UIKit headers
/// (`UIResponder.h`); `objc2-ui-kit` 0.3.2 does not generate typed bindings
/// for them, so we spell their string values directly. `NSNotificationCenter`
/// matches notification names by `isEqualToString:` (value equality, not
/// pointer identity), so a freshly-constructed `NSString` with the right
/// content is equivalent to the UIKit-exported global.
const KEYBOARD_WILL_SHOW: &str = "UIKeyboardWillShowNotification";
const KEYBOARD_WILL_HIDE: &str = "UIKeyboardWillHideNotification";
const KEYBOARD_FRAME_END_KEY: &str = "UIKeyboardFrameEndUserInfoKey";
const KEYBOARD_ANIMATION_DURATION_KEY: &str = "UIKeyboardAnimationDurationUserInfoKey";
const KEYBOARD_ANIMATION_CURVE_KEY: &str = "UIKeyboardAnimationCurveUserInfoKey";

/// Minimal `#[repr(C)]` mirror of CoreGraphics' `CGRect`.
///
/// We only need the height field, and we avoid pulling in the
/// `objc2-core-foundation` crate (which would add a transitive dependency
/// just to call `CGRectValue`). On `aarch64-apple-ios` `CGFloat` is `f64`,
/// so `CGRect` is exactly four `f64`s (32 bytes). We read it out of an
/// `NSValue` via `getValue:size:`.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct CGRect {
    origin_x: f64,
    origin_y: f64,
    size_width: f64,
    size_height: f64,
}

/// Handle to the installed keyboard notification observers.
///
/// Drop to remove the observers from `NotificationCenter`. In practice the
/// observer lives for the window's lifetime (so it's dropped when
/// `WindowState` is dropped).
pub struct KeyboardObserver {
    // Opaque tokens returned by `addObserverForName:object:queue:usingBlock:`.
    // Stored so `Drop` can hand them back to `removeObserver:`.
    show_token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    hide_token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    // Keep a reference to the center so `Drop` can call `removeObserver`.
    center: Retained<NSNotificationCenter>,
}

impl KeyboardObserver {
    /// Install keyboard observers on the default `NotificationCenter`.
    ///
    /// `scale_factor` converts the keyboard frame (physical px, as reported
    /// by UIKit) to logical px. `window_logical_height` caps the reported
    /// height so an iPad stage-manager / slide-over keyboard frame (which can
    /// exceed the window's own height) doesn't push the avoidance padding
    /// beyond the window. Pass `f32::MAX` to disable the cap.
    /// Returns a handle whose `Drop` removes the observers.
    ///
    /// # v1 limitation
    ///
    /// `WindowState` constructs the observer during window init, when the
    /// window's live size isn't yet available. It passes `f32::MAX` (no cap),
    /// preserving pre-clamp behavior. A future improvement should thread the
    /// live window height (e.g., update it on each `SurfaceResized`).
    ///
    /// # Safety (caller contract)
    ///
    /// Must be called on the main thread. UIKit's `NSNotificationCenter`
    /// and the keyboard notifications are main-thread-affine. `WindowState`
    /// upholds this by constructing the observer during window init on the
    /// main thread.
    pub fn install(
        source: KeyboardInsetSource,
        scale_factor: f64,
        window_logical_height: f32,
    ) -> Self {
        let center = NSNotificationCenter::defaultCenter();

        let scale = scale_factor as f32;

        let show_name = NSString::from_str(KEYBOARD_WILL_SHOW);
        let source_for_show = source.clone();
        let show_block = block2::RcBlock::new(move |notif: NonNull<NSNotification>| {
            // SAFETY: UIKit hands us a valid `NSNotification *` for the
            // lifetime of the callback. We only read it on the main thread.
            let notif = unsafe { notif.as_ref() };
            handle_keyboard_notification(
                notif,
                &source_for_show,
                scale,
                window_logical_height,
                /*show=*/ true,
            );
        });
        // SAFETY: `addObserverForName:object:queue:usingBlock:` is marked
        // `#[unsafe(method)]` for thread-safety reasons; we only invoke it
        // on the main thread (see fn doc). Passing `None` for `object` and
        // `queue` matches UIKit's "any sender, post on the queue that posts
        // the notification" semantics — which for keyboard notifications is
        // always the main queue.
        let show_token = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(&show_name),
                None,
                None,
                &show_block,
            )
        };

        let hide_name = NSString::from_str(KEYBOARD_WILL_HIDE);
        let source_for_hide = source.clone();
        let hide_block = block2::RcBlock::new(move |notif: NonNull<NSNotification>| {
            // SAFETY: same as above.
            let notif = unsafe { notif.as_ref() };
            handle_keyboard_notification(
                notif,
                &source_for_hide,
                scale,
                window_logical_height,
                /*show=*/ false,
            );
        });
        // SAFETY: same as the show registration above.
        let hide_token = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(&hide_name),
                None,
                None,
                &hide_block,
            )
        };

        Self {
            show_token,
            hide_token,
            center,
        }
    }
}

impl Drop for KeyboardObserver {
    fn drop(&mut self) {
        // SAFETY: `Drop` runs on the main thread (the observer is
        // main-thread-affine — see module docs). `removeObserver:` only
        // needs the observer pointer to match a previously-registered
        // observer; both tokens were returned by the matching
        // `addObserverForName:...` calls on the same center.
        unsafe {
            self.center.removeObserver(self.show_token.as_ref());
            self.center.removeObserver(self.hide_token.as_ref());
        }
    }
}

/// Extract keyboard frame / duration / curve from a notification's `userInfo`
/// and write them into the source.
///
/// - `show == true`: target height = frame end height (clamped to
///   `[0, window_logical_height]` — the upper bound prevents iPad
///   stage-manager / slide-over frames, which can exceed the window's own
///   height, from over-padding the avoidance widget).
/// - `show == false`: target height = 0 (keyboard dismissing).
fn handle_keyboard_notification(
    notif: &NSNotification,
    source: &KeyboardInsetSource,
    _scale_factor: f32,
    window_logical_height: f32,
    show: bool,
) {
    let user_info: Option<Retained<NSDictionary>> = notif.userInfo();
    let user_info = match user_info {
        Some(ui) => ui,
        None => return,
    };

    // SAFETY: UIKit's keyboard `userInfo` is `NSDictionary<NSString, NSObject>`;
    // the cast is a sound reinterpretation of the generic parameters.
    let user_info: &NSDictionary<NSString, NSObject> =
        unsafe { user_info.cast_unchecked::<NSString, NSObject>() };

    // --- Target height ---
    let target_height = if show {
        let frame_key = NSString::from_str(KEYBOARD_FRAME_END_KEY);
        let frame_value: Option<Retained<NSObject>> = user_info.objectForKey(&frame_key);
        match frame_value {
            Some(obj) => {
                // `obj` is an `NSValue` wrapping a `CGRect`.
                let value: Retained<NSValue> = match obj.downcast::<NSValue>() {
                    Ok(v) => v,
                    Err(_) => return,
                };
                // Read the CGRect via `getValue:size:`. We pass a 32-byte
                // buffer (4 × f64) and the size; UIKit writes the CGRect's
                // fields into it. On aarch64-apple-ios `CGFloat` is `f64`.
                let mut rect = CGRect::default();
                let size = core::mem::size_of::<CGRect>();
                // SAFETY: `getValue:size:` writes `size` bytes into the
                // provided buffer; our buffer is exactly `CGRect`-sized and
                // properly aligned for `f64`. The call is `#[unsafe(method)]`
                // for thread-safety; we're on the main thread.
                unsafe {
                    value.getValue_size(
                        NonNull::new_unchecked(&mut rect as *mut CGRect as *mut c_void),
                        size as objc2_foundation::NSUInteger,
                    );
                }
                let height_pts = rect.size_height as f32;
                // UIKit's keyboardFrameEndUserInfoKey returns a CGRect in the
                // window's coordinate space, which on iOS is in POINTS (logical
                // px), NOT physical px. Do NOT divide by scale_factor — the
                // height is already in logical px.
                let height_logical = height_pts;
                // Defensive: never report a negative height (can happen if
                // the keyboard frame is off-screen in slide-over / stage
                // manager configurations), and never exceed the window's
                // own logical height (on iPad with stage manager /
                // slide-over the keyboard frame can be taller than the
                // app window). `window_logical_height == f32::MAX` is the
                // "no cap" sentinel used when the live height is unknown.
                height_logical.min(window_logical_height).max(0.0)
            }
            None => return,
        }
    } else {
        0.0
    };

    // --- Animation duration (seconds) ---
    let duration_key = NSString::from_str(KEYBOARD_ANIMATION_DURATION_KEY);
    let duration_secs: f32 = user_info
        .objectForKey(&duration_key)
        .and_then(|obj| obj.downcast::<NSNumber>().ok())
        .map(|n| n.as_f32())
        .unwrap_or(0.25); // UIKit default if missing

    // --- Animation curve (raw u8) ---
    let curve_key = NSString::from_str(KEYBOARD_ANIMATION_CURVE_KEY);
    let curve_raw: u8 = user_info
        .objectForKey(&curve_key)
        .and_then(|obj| obj.downcast::<NSNumber>().ok())
        .map(|n| n.as_u8())
        .unwrap_or(0); // EaseInOut is UIKit's default
    let curve = KeyboardCurve::from_uikit_raw(curve_raw);

    log::debug!(
        "[KBD_AVOID] notification: show={} target_height={:.1} duration={:.3} curve={:?}",
        show,
        target_height,
        duration_secs,
        curve
    );
    source.set_target(target_height, duration_secs, curve);
}
