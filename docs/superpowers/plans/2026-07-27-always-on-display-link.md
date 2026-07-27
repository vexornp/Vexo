# Always-On Display Link Refactor — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the iOS CADisplayLink always-on and remove the 6 redundant frame-driver paths that accreted around the on-demand model.

**Architecture:** Single-frame-driver per platform. iOS: CADisplayLink fires every vsync → `request_redraw` → `render_retain` (early-returns when idle). Desktop: `about_to_wait` → `poll_idle_frame_drivers()` → `request_frame` for cursor blink / animation ticker.

**Tech Stack:** Rust, objc2 + objc2-quartz-core (CADisplayLink), winit 0.31, #[cfg(target_os = "ios")] platform split.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-27-always-on-display-link-design.md`
- All existing tests must pass: `cargo test -p vexo --lib` (1014), `cargo test -p shared_app --lib` (16), `cargo test -p vexo_uikit --lib` (21)
- iOS build must pass: `cargo build -p vexo --target aarch64-apple-ios`
- Desktop build must pass: `cargo build -p vexo -p vexo_uikit -p shared_app`
- Keep `[KBDBG] notify SHOW/HIDE` logs (keyboard_ios.rs) and `[KBDBG] frame elapsed/gap` + `DONE` logs (window.rs) — they are the A/B verification signal for post-refactor on-device validation
- Remove `[KBDBG] display-link tick` log (display_link_ios.rs) — 60-120Hz noise
- Do NOT remove `last_kb_frame` field — it backs the frame-gap log we're keeping
- `DisplayLink` and `KeyboardObserver` have no unit tests (platform glue); verification is build + existing tests + manual A/B

---

### Task 1: Always-On DisplayLink

Simplify `DisplayLink` to a lifecycle-only handle (start unpaused, no start/stop methods), drop the `Arc<DisplayLink>` sharing with `KeyboardObserver`, and remove `sync_display_link()`. After this task the display link is always-on; the remaining redundant `request_frame` paths stay (harmless — `request_redraw` is idempotent) and are cleaned up in Task 2.

**Files:**
- Modify: `vexo/src/platform/display_link_ios.rs` (remove start/stop/is_running/running, start unpaused, remove tick log, update doc)
- Modify: `vexo/src/platform/keyboard_ios.rs` (drop `display_link` param, drop proactive `start()` calls, update doc)
- Modify: `vexo/src/window.rs:118-134` (field type `Arc<DisplayLink>` → `DisplayLink`, update field doc), `:172-214` (`new()` construction + `install()` call), `:591-606` (remove `sync_display_link` method)
- Modify: `vexo/src/app.rs:123-164` (remove `sync_display_link()` call from `about_to_wait`)

**Interfaces:**
- Consumes: `DisplayLink::new(on_frame: Arc<dyn Fn() + Send + Sync>)` (existing signature, unchanged)
- Produces: `DisplayLink` (no longer `Arc<DisplayLink>`); `KeyboardObserver::install` no longer takes `display_link` parameter

- [ ] **Step 1: Update `display_link_ios.rs` module doc**

In `vexo/src/platform/display_link_ios.rs`, replace lines 21-30 (the `## Proactive start` section) with:

```rust
//! ## Always-on
//!
//! `DisplayLink` starts unpaused in `new()` and is never explicitly stopped.
//! iOS auto-pauses `CADisplayLink` when the app is suspended (home swipe,
//! app switcher) and auto-resumes on foreground return. `Drop` invalidates
//! the link when `WindowState` is torn down. The `is_occluded` early-return
//! in `render_retain` prevents wasted CPU/GPU work when the surface is
//! hidden but the app is still foregrounded (control center, notification
//! shade).
```

- [ ] **Step 2: Remove `AtomicBool` import from `display_link_ios.rs`**

Remove this line (currently line 32):

```rust
use std::sync::atomic::{AtomicBool, Ordering};
```

Keep the `use std::sync::Arc;` line that follows it.

- [ ] **Step 3: Remove `[KBDBG] display-link tick` log from `display_link_ios.rs`**

In the `tick` method (around line 67-83), remove this line:

```rust
            log::debug!("[KBDBG] display-link tick");
```

The method body should become:

```rust
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
            unsafe {
                let boxed = &*(ivars.callback as *mut Box<FrameCallback>);
                (boxed)();
            }
        }
```

- [ ] **Step 4: Update `DisplayLink` struct doc and remove `running` field**

Replace the struct definition + doc (around lines 87-107) with:

```rust
/// Handle to a `CADisplayLink` that drives `window.request_redraw()` each
/// vsync. Created once per window in `WindowState::new`; started unpaused;
/// invalidated in `Drop` when the window is torn down. iOS auto-pauses the
/// link when the app is suspended.
pub struct DisplayLink {
    link: Retained<CADisplayLink>,
    _target: Retained<DisplayLinkTarget>,
    /// Raw pointer to the heap-allocated `Box<Box<FrameCallback>>`. Owned by
    /// this struct; the Objective-C target's ivar holds a borrow of it.
    /// Freed in `Drop` AFTER the display link is invalidated, so `tick` can
    /// never read a dangling pointer.
    callback_raw: *mut Box<FrameCallback>,
}
```

- [ ] **Step 5: Update `DisplayLink::new` to start unpaused**

Replace the `new()` method (around lines 116-153) with:

```rust
    /// Create a display link that calls `on_frame` each vsync. The link
    /// starts unpaused (always-on); iOS auto-pauses when the app is
    /// suspended and auto-resumes on foreground.
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

        // Start unpaused — always-on. iOS auto-pauses when the app is
        // suspended and auto-resumes on foreground. See module doc.
        link.setPaused(false);

        Self {
            link,
            _target: target,
            callback_raw,
        }
    }
```

- [ ] **Step 6: Remove `start`, `stop`, `is_running` methods from `DisplayLink`**

Delete these three methods (around lines 155-177):

```rust
    pub fn start(&self) { ... }
    pub fn stop(&self) { ... }
    pub fn is_running(&self) -> bool { ... }
```

The `Drop` impl stays unchanged.

- [ ] **Step 7: Update `keyboard_ios.rs` module doc**

In `vexo/src/platform/keyboard_ios.rs`, replace lines 7-13 (the proactive-start rationale):

```rust
//! height, animation duration, and animation curve raw value from `userInfo`,
//! reads the current keyboard height (the `from` value), constructs a
//! [`KeyboardAnimation`], and writes it to the animation source. It then
//! proactively starts the shared `CADisplayLink` (a hardware wake source) so
//! the next vsync fires a redraw regardless of winit's CFRunLoop state. This
//! fixes the 163ms cold-start delay that occurred when winit's CFRunLoop was
//! parked at notification time — without the proactive start, the first
//! animation frame would wait for some unrelated system event to wake the
//! run loop.
```

with:

```rust
//! height, animation duration, and animation curve raw value from `userInfo`,
//! reads the current keyboard height (the `from` value), constructs a
//! [`KeyboardAnimation`], and writes it to the animation source. It then
//! requests a redraw so the render loop (`render_retain`) can begin
//! interpolating on the next vsync. The always-on `CADisplayLink` drives
//! frames at vsync rate, so no proactive start is needed from here.
```

- [ ] **Step 8: Remove `DisplayLink` import from `keyboard_ios.rs`**

Delete this line (currently line 30):

```rust
use crate::platform::display_link_ios::DisplayLink;
```

- [ ] **Step 9: Remove `display_link` parameter from `KeyboardObserver::install`**

Change the `install` signature (around lines 54-61) from:

```rust
    pub fn install(
        source: KeyboardInsetSource,
        animation_source: KeyboardAnimationSource,
        _scale_factor: f64,
        window_logical_height: f32,
        request_frame: Arc<dyn Fn() + Send + Sync>,
        display_link: Arc<DisplayLink>,
    ) -> Self {
```

to:

```rust
    pub fn install(
        source: KeyboardInsetSource,
        animation_source: KeyboardAnimationSource,
        _scale_factor: f64,
        window_logical_height: f32,
        request_frame: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
```

- [ ] **Step 10: Remove `dl_for_show` clone and `start()` call from SHOW handler**

In the SHOW block (around lines 64-104):

1. Delete this line:
   ```rust
        let dl_for_show = display_link.clone();
   ```

2. Delete the comment + call block:
   ```rust
            // Proactively start the CADisplayLink BEFORE requesting a frame.
            // The display link is a hardware wake source: once started, it
            // fires on the next vsync (≤16.7ms) and calls request_redraw(),
            // regardless of whether winit's CFRunLoop is currently parked.
            // Without this, the first animation frame would be delayed by up
            // to ~163ms waiting for CFRunLoop to wake from some unrelated
            // system event.
            dl_for_show.start();
   ```

Keep the `request_for_show();` call.

- [ ] **Step 11: Remove `dl_for_hide` clone and `start()` call from HIDE handler**

In the HIDE block (around lines 114-146):

1. Delete this line:
   ```rust
        let dl_for_hide = display_link.clone();
   ```

2. Delete the comment + call:
   ```rust
            // Proactively start the CADisplayLink — same rationale as SHOW.
            dl_for_hide.start();
   ```

Keep the `request_for_hide();` call.

- [ ] **Step 12: Update `display_link` field in `window.rs`**

Replace the field definition + doc (around lines 118-134) with:

```rust
    /// CADisplayLink driver for vsync-rate animation on iOS. Always-on:
    /// started at construction, never explicitly stopped. iOS auto-pauses
    /// when the app is suspended and auto-resumes on foreground. `Drop`
    /// invalidates the link when `WindowState` is torn down. On desktop
    /// this field doesn't exist.
    #[cfg(target_os = "ios")]
    display_link: crate::platform::display_link_ios::DisplayLink,
```

- [ ] **Step 13: Update `display_link` construction in `window.rs::new()`**

Replace the `#[cfg(target_os = "ios")] let display_link = { ... };` block (around lines 172-185) with:

```rust
        #[cfg(target_os = "ios")]
        let display_link = {
            // CADisplayLink callback: just request a redraw each vsync. The
            // render loop's interpolation driver (in render_retain) advances
            // the keyboard animation one step per vsync. The link is always-on
            // — iOS auto-pauses on background, auto-resumes on foreground.
            let window_for_dl = window.clone();
            let on_frame: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                window_for_dl.request_redraw();
            });
            crate::platform::display_link_ios::DisplayLink::new(on_frame)
        };
```

- [ ] **Step 14: Drop `display_link.clone()` from `KeyboardObserver::install` call in `window.rs::new()`**

In the `#[cfg(target_os = "ios")] let keyboard_observer = { ... };` block, change the `install` call (around lines 207-214) from:

```rust
            Some(crate::platform::keyboard_ios::KeyboardObserver::install(
                keyboard_inset_source.clone(),
                keyboard_animation_source.clone(),
                scale,
                window_logical_height,
                request_frame,
                display_link.clone(),
            ))
```

to:

```rust
            Some(crate::platform::keyboard_ios::KeyboardObserver::install(
                keyboard_inset_source.clone(),
                keyboard_animation_source.clone(),
                scale,
                window_logical_height,
                request_frame,
            ))
```

- [ ] **Step 15: Remove `sync_display_link` method from `window.rs`**

Delete both cfg variants (around lines 591-606):

```rust
    /// Start or stop the CADisplayLink based on whether any animation is
    /// active. Called from `about_to_wait`. On desktop this is a no-op (no
    /// display link field exists).
    #[cfg(target_os = "ios")]
    pub fn sync_display_link(&self) {
        let needs_vsync = self.keyboard_animation_source.has_pending()
            || self.animation_ticker().has_active();
        if needs_vsync {
            self.display_link.start();
        } else {
            self.display_link.stop();
        }
    }

    #[cfg(not(target_os = "ios"))]
    pub fn sync_display_link(&self) {}
```

- [ ] **Step 16: Remove `sync_display_link()` call from `app.rs::about_to_wait`**

In `vexo/src/app.rs`, in `about_to_wait` (around line 162), delete this line:

```rust
            state.sync_display_link();
```

Also delete the 6-line comment block above it (around lines 157-161):

```rust
            // Start/stop the CADisplayLink on iOS based on animation activity.
            // The display link fires at vsync rate (60/120Hz), calling
            // window.request_redraw() each tick. Without it, winit's
            // CFRunLoopTimer throttles to ~15 FPS, making keyboard animations
            // jerky and causing the input bar to finish after the keyboard.
```

Leave the rest of `about_to_wait` intact for now (Task 2 will refactor it further).

- [ ] **Step 17: Build desktop**

Run: `cargo build -p vexo -p vexo_uikit -p shared_app`
Expected: Compiles clean (existing warnings OK, no new warnings, no errors).

- [ ] **Step 18: Build iOS**

Run: `cargo build -p vexo --target aarch64-apple-ios`
Expected: Compiles clean.

- [ ] **Step 19: Run tests**

Run:
```bash
cargo test -p vexo --lib 2>&1 | tail -3
cargo test -p shared_app --lib 2>&1 | tail -3
cargo test -p vexo_uikit --lib 2>&1 | tail -3
```

Expected: 1014 + 16 + 21 = 1051 tests pass, 0 failed.

- [ ] **Step 20: Commit**

```bash
git add vexo/src/platform/display_link_ios.rs vexo/src/platform/keyboard_ios.rs vexo/src/window.rs vexo/src/app.rs
git commit -m "$(cat <<'EOF'
refactor(vexo): always-on CADisplayLink on iOS

Start the display link unpaused in DisplayLink::new and never explicitly
stop it. iOS auto-pauses on background and auto-resumes on foreground;
Drop invalidates the link when the window is torn down.

Removes: DisplayLink::start/stop/is_running, the Arc<DisplayLink> sharing
with KeyboardObserver, the proactive start() calls in keyboard SHOW/HIDE
handlers, and sync_display_link(). The 163ms cold-start fix is preserved
because the link is now always firing — no start-up latency.

The remaining redundant request_frame paths (about_to_wait checks,
render_retain self-rearm) are harmless (request_redraw is idempotent)
and will be cleaned up in a follow-up.
EOF
)"
```

---

### Task 2: Frame-Loop Cleanup

Remove the now-redundant `request_frame` paths and centralize the desktop frame drivers into `poll_idle_frame_drivers()`. After this task, each platform has exactly one frame driver: iOS = CADisplayLink, desktop = `poll_idle_frame_drivers()`.

**Files:**
- Modify: `vexo/src/window.rs:553-589` (remove `keyboard_inset_changed` method), `:591-606` (add `poll_idle_frame_drivers` where `sync_display_link` was), `:929-941` (remove render_retain self-rearm blocks)
- Modify: `vexo/src/app.rs:123-164` (replace `about_to_wait` body with `poll_idle_frame_drivers()` call)

**Interfaces:**
- Consumes: `WindowState::check_cursor_blink(&mut self) -> bool`, `WindowState::animation_ticker(&self) -> &Arc<AnimationTicker>` (existing accessors)
- Produces: `WindowState::poll_idle_frame_drivers(&mut self)` — pub, called from `about_to_wait`

- [ ] **Step 1: Remove `keyboard_inset_changed` method from `window.rs`**

Delete the entire method + its doc comment (around lines 553-589):

```rust
    /// Check if the keyboard animation source has pending params (i.e. an
    /// animation is active or queued).
    ///
    /// Read-only. This is for `about_to_wait` to detect a pending animation
    /// that landed **during** render (after the interpolation poll ran) and
    /// break what would otherwise be a render-loop deadlock.
    ///
    /// ## The deadlock (dismiss only)
    ///
    /// When the user taps outside a focused TextEdit, focus clears during
    /// `perform_rebuilds()`, and the focus-change block calls
    /// `set_ime_allowed(false)`. UIKit fires `keyboardWillHide` — often
    /// synchronously — writing animation params to the source. But the
    /// interpolation poll at the top of `render_retain()` already ran, so
    /// this frame misses the params.
    ///
    /// Then `about_to_wait` runs. Its two existing frame drivers are both
    /// dead: `check_cursor_blink()` is false (TextEdit just unfocused), and
    /// `animation_ticker().has_active()` is false (the interpolation hasn't
    /// started — the poll missed the params). No frame is requested. The OS
    /// keyboard keeps sliding down (GPU-driven, independent of our render
    /// loop); our input view freezes. Eventually some stray event wakes the
    /// loop, the poll picks up the params, the interpolation starts — but
    /// the keyboard is already gone, so the input view animates down after
    /// the keyboard disappeared.
    ///
    /// For **show** this doesn't deadlock: cursor blink turns **on**
    /// (TextEdit focused), so `check_cursor_blink()` keeps the loop alive
    /// until the poll catches up and the interpolation starts.
    ///
    /// This method breaks the dismiss deadlock by giving `about_to_wait` a
    /// third reason to request a frame: the animation source has pending
    /// params. The next `render_retain()` poll then interpolates and writes
    /// `current_height`.
    pub fn keyboard_inset_changed(&self) -> bool {
        self.keyboard_animation_source.has_pending()
    }
```

This method is now dead: the always-on display link guarantees frames flow every vsync, so the dismiss deadlock no longer applies (the next vsync's `render_retain` poll picks up the pending params).

- [ ] **Step 2: Add `poll_idle_frame_drivers` to `window.rs`**

Add this method where `sync_display_link` used to live (the spot is now empty after Task 1 Step 15):

```rust
    /// Poll platform-specific idle frame drivers. On desktop, this checks
    /// cursor blink and animation ticker and requests a frame if either is
    /// active. On iOS, the always-on CADisplayLink drives frames at vsync
    /// rate, so there's nothing to poll — this is a no-op.
    #[cfg(not(target_os = "ios"))]
    pub fn poll_idle_frame_drivers(&mut self) {
        if self.check_cursor_blink() {
            self.request_frame();
        }
        if self.animation_ticker().has_active() {
            self.request_frame();
        }
    }

    #[cfg(target_os = "ios")]
    pub fn poll_idle_frame_drivers(&mut self) {}
```

- [ ] **Step 3: Remove render_retain self-rearm blocks from `window.rs`**

At the end of `render_retain` (around lines 929-941), delete these two blocks:

```rust
        // 14. If a TextEdit is focused, keep the event loop alive so
        //     about_to_wait fires and can check cursor blink timing.
        //     request_redraw() is idempotent; the next render_retain() will
        //     early-return if nothing is dirty (blink hasn't toggled yet).
        if self.three_tree_pipeline.focused_element().is_some() {
            self.request_frame();
        }

        // Keep the frame loop alive while animations are active so that
        // tick() continues to fire each frame.
        if self.animation_ticker.has_active() {
            self.request_frame();
        }
```

The method should now end with:

```rust
        // 13. Execute render
        self.text_pipeline
            .execute_render(
                &mut self.backend,
                &self.frame_builder,
                prepared_text,
                &mut self.font_system,
            )?;

        Ok(())
    }
}
```

On iOS the always-on display link drives frames; on desktop `poll_idle_frame_drivers()` (called from `about_to_wait`) covers cursor blink and animation ticker.

- [ ] **Step 4: Simplify `about_to_wait` in `app.rs`**

Replace the entire `about_to_wait` method body (around lines 123-164) with:

```rust
    fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
        for state in self.windows.values_mut() {
            state.poll_idle_frame_drivers();
        }
    }
```

This removes:
- The `check_cursor_blink()` → `request_frame()` inline call (now inside desktop's `poll_idle_frame_drivers`)
- The `animation_ticker().has_active()` → `request_frame()` inline call (now inside desktop's `poll_idle_frame_drivers`)
- The `keyboard_inset_changed()` → `request_frame()` call (dead — display link drives frames on iOS)
- All associated comment blocks (the dismiss-deadlock analysis, the iOS CADisplayLink comment)

- [ ] **Step 5: Build desktop**

Run: `cargo build -p vexo -p vexo_uikit -p shared_app`
Expected: Compiles clean (existing warnings OK, no new warnings).

- [ ] **Step 6: Build iOS**

Run: `cargo build -p vexo --target aarch64-apple-ios`
Expected: Compiles clean.

- [ ] **Step 7: Run tests**

Run:
```bash
cargo test -p vexo --lib 2>&1 | tail -3
cargo test -p shared_app --lib 2>&1 | tail -3
cargo test -p vexo_uikit --lib 2>&1 | tail -3
```

Expected: 1014 + 16 + 21 = 1051 tests pass, 0 failed.

- [ ] **Step 8: Commit**

```bash
git add vexo/src/window.rs vexo/src/app.rs
git commit -m "$(cat <<'EOF'
refactor(vexo): single-frame-driver model, remove redundant request_frame paths

With the display link always-on (Task 1), six request_frame paths became
redundant. Centralize the desktop drivers into poll_idle_frame_drivers()
and delete the rest:

- about_to_wait: drop inline cursor-blink + ticker + keyboard_inset_changed
  checks; call poll_idle_frame_drivers() instead (no-op on iOS)
- render_retain: drop self-rearm for focused TextEdit and active animations
- window.rs: drop keyboard_inset_changed method (dead — the dismiss
  deadlock it fixed no longer applies because the display link guarantees
  frames flow every vsync)

Each platform now has exactly one frame driver: iOS = CADisplayLink,
desktop = poll_idle_frame_drivers. Matches Flutter's model.
EOF
)"
```

---

## Post-Implementation: Manual A/B Verification

After both tasks land, the user runs the iOS demo from Xcode and captures `[KBDBG]` logs. Compare against pre-refactor baseline:

1. **First-frame delay** — `notify SHOW` → first `frame` log. Baseline ~6ms. Regression threshold: >16ms.
2. **Frame gaps** — `gap=` in `frame` logs. Baseline ~17-18ms. Regression: ~67ms (CFRunLoopTimer driving).
3. **SHOW DONE** — `elapsed=` in `DONE` log. Baseline ~387ms. Regression: drift >20ms.
4. **Visual sync** — input bar moves in lockstep with OS keyboard.

If all four match, no regression. Proceed to remove remaining `[KBDBG]` logs in a follow-up cleanup commit (out of scope for this plan).
