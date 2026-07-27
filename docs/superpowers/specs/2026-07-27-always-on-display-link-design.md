# Always-On Display Link Refactor

**Date:** 2026-07-27
**Status:** Approved (brainstormed)
**Branch:** `feat/ios-keyboard-avoidance` (follow-up to `a15b136`)

## Problem

The current iOS frame driver has accreted complexity. The display link is on-demand:
`sync_display_link()` starts/stops it based on animation activity, the keyboard observer
proactively calls `start()` from notification handlers to fix a 163ms cold-start bug, and
six redundant `request_frame()` paths exist across `about_to_wait` and `render_retain` to
keep frames flowing during cursor blink, keyboard animation, and ticker animations. Three
of those paths are dead on desktop; all six are redundant on iOS once the display link is
always-on.

The TODO comment on `WindowState::display_link` (`vexo/src/window.rs:126`) already
identifies the fix: start the display link at construction, never stop it, trust iOS to
auto-pause on background. This refactor executes that fix and removes the now-redundant
frame-driver paths.

## Goal

Single-frame-driver model: each platform has exactly one frame driver, with no redundancy.

```
iOS:     CADisplayLink (always-on) → request_redraw → RedrawRequested → render_retain
Desktop: about_to_wait → poll_idle_frame_drivers() → request_frame → RedrawRequested → render_retain
```

## Non-Goals

- ProMotion 120Hz support via `preferredFrameRateRange`. Default rate (60Hz, 80Hz on
  ProMotion) is sufficient for keyboard sync. A future TODO will track this.
- Listening to `UIApplication` lifecycle notifications. iOS auto-pauses `CADisplayLink`
  when the app is suspended; no explicit pause/resume wiring is needed.
- Removing the remaining `[KBDBG]` logs. Those are the A/B verification signal for this
  refactor and will be cleaned up in a follow-up commit after on-device verification.

## Architecture

### DisplayLink lifecycle

- Created in `WindowState::new`, started immediately (not paused).
- Never explicitly stopped. iOS auto-pauses when the app is suspended; auto-resumes on
  foreground. The existing `is_occluded` early-return in `render_retain` prevents wasted
  CPU/GPU work when the surface is hidden but the app is still foregrounded (control
  center, notification shade).
- `Drop` calls `link.invalidate()` as today — happens when `WindowState` drops.

### Frame flow on iOS (after refactor)

1. CADisplayLink fires every vsync (60Hz default; 80Hz ProMotion)
2. Callback calls `window.request_redraw()`
3. winit dispatches `RedrawRequested`
4. `render_retain` runs; early-returns if
   `!needs_redraw && !has_dirty && !needs_reconcile && !kb_active` (cheap; no GPU work)
5. Repeat next vsync

### Frame flow on desktop (unchanged)

1. `about_to_wait` polls cursor blink + animation ticker via `poll_idle_frame_drivers()`
2. If either is active, `request_frame()`
3. winit dispatches `RedrawRequested`
4. `render_retain` runs

### Key invariant

On iOS, `render_retain` is invoked every vsync whether or not work is needed. The
early-return path makes idle frames cheap (no GPU work, no layout, no paint). This
matches Flutter's model.

## Code Changes

### `vexo/src/platform/display_link_ios.rs`

Simplify to a lifecycle-only handle:

- Remove `running: AtomicBool` field
- Remove `start()`, `stop()`, `is_running()` methods
- Remove `AtomicBool` import
- Module doc: replace "Proactive start" section with "Always-on" section explaining iOS
  auto-pauses on background and the `Drop` invalidation
- `new(on_frame)`: after creating the link, call `link.setPaused(false)` instead of
  `true` (start unpaused)
- Remove `[KBDBG] display-link tick` log line in `tick` (60-120Hz noise — unusable in
  console)
- Keep `Send+Sync` impls (harmless; safety comment is accurate)

### `vexo/src/platform/keyboard_ios.rs`

- Remove `display_link: Arc<DisplayLink>` parameter from `KeyboardObserver::install()`
- Remove `dl_for_show = display_link.clone()` / `dl_for_hide = display_link.clone()`
- Remove `dl_for_show.start()` / `dl_for_hide.start()` calls and their comments
- Update module doc: delete the "proactive start" rationale paragraph; replace with a
  one-liner noting the always-on display link drives frames
- Keep the `[KBDBG] notify SHOW/HIDE` logs — these are the A/B verification signal

### `vexo/src/window.rs`

- **Field**: `display_link: Arc<DisplayLink>` → `display_link: DisplayLink`
- **Field doc**: replace the `TODO(future)` comment with a short always-on explanation
- **`new()`**: drop the `Arc::new(...)` wrapping; `display_link` is constructed started
  (DisplayLink::new starts unpaused now). Drop `display_link.clone()` from
  `KeyboardObserver::install()` call.
- **Remove `sync_display_link()`** entirely (both iOS impl and desktop stub)
- **Keep `last_kb_frame` field** — still used by the `[KBDBG] frame gap` log, which is
  one of the A/B verification logs we're keeping
- **`render_retain` end**: remove both self-re-arm blocks:
  ```rust
  if self.three_tree_pipeline.focused_element().is_some() {
      self.request_frame();
  }
  if self.animation_ticker.has_active() {
      self.request_frame();
  }
  ```
  On iOS the display link drives frames; on desktop `poll_idle_frame_drivers()` covers
  cursor blink and animation ticker.

### `vexo/src/app.rs` (`about_to_wait`)

Refactor to extract the platform split into one helper. Current:

```rust
fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
    for state in self.windows.values_mut() {
        if state.check_cursor_blink() { state.request_frame(); }
        if state.animation_ticker().has_active() { state.request_frame(); }
        if state.keyboard_inset_changed() { state.request_frame(); }
        state.sync_display_link();
    }
}
```

After:

```rust
fn about_to_wait(&mut self, _event_loop: &dyn ActiveEventLoop) {
    for state in self.windows.values_mut() {
        state.poll_idle_frame_drivers();
    }
}
```

With the helper on `WindowState`:

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

What's removed from `about_to_wait`:

- `keyboard_inset_changed()` check (path 3) — dead on desktop, redundant on iOS
- `sync_display_link()` call (path 4) — gone entirely

The two desktop drivers (cursor blink, animation ticker) get wrapped in the helper. The
iOS no-op helper makes the "single frame driver per platform" model explicit at one point.

## What Goes Away

- The 6 redundant `request_frame` paths enumerated in the brainstorming session:
  1. `about_to_wait`: `check_cursor_blink()` → request_frame (iOS only; redundant)
  2. `about_to_wait`: `animation_ticker().has_active()` → request_frame (iOS only; redundant)
  3. `about_to_wait`: `keyboard_inset_changed()` → request_frame (dead on desktop, redundant on iOS)
  4. `about_to_wait`: `sync_display_link()` call (gone entirely)
  5. `render_retain` end: `if focused_element().is_some() { request_frame() }` (iOS redundant; desktop covered by path 1)
  6. `render_retain` end: `if animation_ticker.has_active() { request_frame() }` (iOS redundant; desktop covered by path 2)
- `sync_display_link()` method (both iOS impl and desktop stub)
- `Arc<DisplayLink>` sharing with `KeyboardObserver`
- `DisplayLink::start/stop/is_running/running` field
- Proactive `dl.start()` in keyboard SHOW/HIDE handlers
- The `[KBDBG] display-link tick` log (60-120Hz noise)
- The `TODO(future)` comment on the `display_link` field (it's no longer future work)

## Testing & Verification

### Automated tests

No new unit tests. The refactor is a behavioral simplification — same observable
behavior, fewer code paths. Existing tests cover what matters:

- `cargo test -p vexo --lib` (1014 tests) — verifies no regressions in core framework
- `cargo test -p shared_app --lib` (16 tests) — verifies chat app still works
- `cargo test -p vexo_uikit --lib` (21 tests) — verifies KeyboardAvoider + tab bar

The `DisplayLink` and `KeyboardObserver` modules have no existing tests (they're
platform glue, hard to unit-test without a real iOS runtime). Adding mock-based tests
would be a separate effort and out of scope.

### Build verification

```bash
cargo build -p vexo -p vexo_uikit -p shared_app    # Desktop build
cargo build -p vexo --target aarch64-apple-ios      # iOS build
```

Both must compile clean (existing warnings OK, no new warnings).

### Manual A/B verification (on device)

Run the iOS demo from Xcode before and after the refactor, capturing `[KBDBG]` logs.
Comparison criteria:

1. **First-frame delay** — time from `notify SHOW` to first `frame` log. Should match
   (~6ms). If it regresses to >16ms, the always-on display link isn't firing on the
   first vsync after notification.
2. **Frame gaps** — `gap=` values in `frame` logs. Should be ~17-18ms (vsync rate). If
   gaps jump to ~67ms, winit's CFRunLoopTimer is driving frames again (display link not
   firing).
3. **SHOW DONE timing** — `elapsed=` in `DONE` log. Should be ~387ms (4ms late). If it
   drifts significantly, frames are being dropped.
4. **Visual sync** — input bar should move in lockstep with OS keyboard, same as before.

If all four match, no regression. The remaining `[KBDBG]` logs are then removed in a
follow-up cleanup commit.

### Rollback plan

If the refactor regresses, revert the single commit. The old on-demand architecture is
preserved in git history. No data migration, no state to recover.
