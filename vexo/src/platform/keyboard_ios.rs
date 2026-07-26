//! iOS keyboard observer — bridges UIKit keyboard notifications to
//! [`KeyboardAnimationSource`](crate::core::KeyboardAnimationSource).
//!
//! On `keyboardWillShow/Hide`, the observer captures the keyboard's end-frame
//! height, animation duration, and animation curve raw value from `userInfo`,
//! reads the current keyboard height (the `from` value), constructs a
//! [`KeyboardAnimation`], and writes it to the animation source. It then
//! requests a frame so the render loop (`WindowState::render_retain()`)
//! starts interpolating `KeyboardInsetSource.current_height` each vsync.
//!
//! The interpolation itself runs in the render loop, not here — the render
//! loop is already `CADisplayLink`-driven on iOS (via winit), so the timing
//! is vsync-accurate without installing a separate display link.

use core::ffi::c_void;
use core::ptr::NonNull;
use std::sync::Arc;
use std::time::Instant;

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_foundation::{
    NSDictionary, NSNotification, NSNotificationCenter, NSNumber, NSObject, NSString, NSValue,
};

use crate::core::{KeyboardAnimation, KeyboardAnimationSource, KeyboardInsetSource};

const KEYBOARD_WILL_SHOW: &str = "UIKeyboardWillShowNotification";
const KEYBOARD_WILL_HIDE: &str = "UIKeyboardWillHideNotification";
const KEYBOARD_FRAME_END_KEY: &str = "UIKeyboardFrameEndUserInfoKey";
const KEYBOARD_ANIMATION_DURATION_KEY: &str = "UIKeyboardAnimationDurationUserInfoKey";
const KEYBOARD_ANIMATION_CURVE_KEY: &str = "UIKeyboardAnimationCurveUserInfoKey";

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct CGRect {
    origin_x: f64,
    origin_y: f64,
    size_width: f64,
    size_height: f64,
}

pub struct KeyboardObserver {
    show_token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    hide_token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    center: Retained<NSNotificationCenter>,
}

impl KeyboardObserver {
    pub fn install(
        source: KeyboardInsetSource,
        animation_source: KeyboardAnimationSource,
        _scale_factor: f64,
        window_logical_height: f32,
        request_frame: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let center = NSNotificationCenter::defaultCenter();

        let show_name = NSString::from_str(KEYBOARD_WILL_SHOW);
        let source_for_show = source.clone();
        let anim_for_show = animation_source.clone();
        let request_for_show = request_frame.clone();
        let window_h_for_show = window_logical_height;
        let show_block = block2::RcBlock::new(move |notif: NonNull<NSNotification>| {
            let notif = unsafe { notif.as_ref() };
            let target_height = extract_target_height(notif, window_h_for_show);
            let duration_secs = extract_duration(notif);
            let curve_raw = extract_curve_raw(notif);
            let from = source_for_show.get();
            let animation = KeyboardAnimation {
                from,
                target: target_height,
                duration_secs,
                start: Instant::now(),
                curve_raw,
            };
            if duration_secs <= 0.0 {
                source_for_show.set(target_height);
            } else {
                anim_for_show.set(animation);
            }
            request_for_show();
        });
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
        let anim_for_hide = animation_source.clone();
        let request_for_hide = request_frame.clone();
        let hide_block = block2::RcBlock::new(move |notif: NonNull<NSNotification>| {
            let notif = unsafe { notif.as_ref() };
            let _ = extract_target_height(notif, window_logical_height);
            let duration_secs = extract_duration(notif);
            let curve_raw = extract_curve_raw(notif);
            let from = source_for_hide.get();
            let animation = KeyboardAnimation {
                from,
                target: 0.0,
                duration_secs,
                start: Instant::now(),
                curve_raw,
            };
            if duration_secs <= 0.0 {
                source_for_hide.set(0.0);
            } else {
                anim_for_hide.set(animation);
            }
            request_for_hide();
        });
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
        unsafe {
            self.center.removeObserver(self.show_token.as_ref());
            self.center.removeObserver(self.hide_token.as_ref());
        }
    }
}

fn extract_target_height(notif: &NSNotification, window_logical_height: f32) -> f32 {
    let user_info: Option<Retained<NSDictionary>> = notif.userInfo();
    let user_info = match user_info {
        Some(ui) => ui,
        None => return 0.0,
    };
    let user_info: &NSDictionary<NSString, NSObject> =
        unsafe { user_info.cast_unchecked::<NSString, NSObject>() };
    let frame_key = NSString::from_str(KEYBOARD_FRAME_END_KEY);
    let frame_value: Option<Retained<NSObject>> = user_info.objectForKey(&frame_key);
    match frame_value {
        Some(obj) => {
            let value: Retained<NSValue> = match obj.downcast::<NSValue>() {
                Ok(v) => v,
                Err(_) => return 0.0,
            };
            let mut rect = CGRect::default();
            let size = core::mem::size_of::<CGRect>();
            unsafe {
                value.getValue_size(
                    NonNull::from(&mut rect).cast::<c_void>(),
                    size as objc2_foundation::NSUInteger,
                );
            }
            let height_logical = rect.size_height as f32;
            height_logical.min(window_logical_height).max(0.0)
        }
        None => 0.0,
    }
}

fn extract_duration(notif: &NSNotification) -> f32 {
    let user_info: Option<Retained<NSDictionary>> = notif.userInfo();
    let user_info = match user_info {
        Some(ui) => ui,
        None => return 0.25,
    };
    let user_info: &NSDictionary<NSString, NSObject> =
        unsafe { user_info.cast_unchecked::<NSString, NSObject>() };
    let duration_key = NSString::from_str(KEYBOARD_ANIMATION_DURATION_KEY);
    user_info
        .objectForKey(&duration_key)
        .and_then(|obj| obj.downcast::<NSNumber>().ok())
        .map(|n| n.as_f32())
        .unwrap_or(0.25)
}

fn extract_curve_raw(notif: &NSNotification) -> u8 {
    let user_info: Option<Retained<NSDictionary>> = notif.userInfo();
    let user_info = match user_info {
        Some(ui) => ui,
        None => return 0,
    };
    let user_info: &NSDictionary<NSString, NSObject> =
        unsafe { user_info.cast_unchecked::<NSString, NSObject>() };
    let curve_key = NSString::from_str(KEYBOARD_ANIMATION_CURVE_KEY);
    user_info
        .objectForKey(&curve_key)
        .and_then(|obj| obj.downcast::<NSNumber>().ok())
        .map(|n| n.as_u8())
        .unwrap_or(0)
}
