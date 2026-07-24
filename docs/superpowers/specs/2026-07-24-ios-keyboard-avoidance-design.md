# iOS Keyboard Avoidance — Design

**Date:** 2026-07-24
**Topic:** Lift the chat screen's input bar (and, generally, any focused `TextEdit`) above the iOS software keyboard.

## Problem

When a `TextEdit` gains focus on iOS, the software keyboard slides up and covers it. The chat screen's input bar is pinned to the absolute bottom of the window (`shared_app/src/chats/chat_screen.rs`), so it is fully occluded. No other screen has avoidance either; the chat screen is just where it was first noticed.

winit's `Window::safe_area()` reads UIKit's `safeAreaInsets`, which **excludes the keyboard** — so the existing `SafeAreaSource` / `SafeArea` infrastructure is blind to the keyboard. Focus changes already drive `set_ime_allowed(true/false)` on mobile to show/hide the keyboard (`vexo/src/window.rs`), but nothing resizes the UI in response.

## Scope

- Build a reusable keyboard-avoidance primitive in the `vexo` framework (source + widget).
- Apply it in the chat screen so the input bar lifts above the keyboard.
- Any future screen with a focused `TextEdit` gets avoidance for free by wrapping in the widget.
- iOS only. Desktop and Android are no-ops (transparent pass-through).

## Non-goals

- Android IME avoidance (different platform API; out of scope).
- A `Scaffold`-style root wrapper that auto-applies avoidance (apps opt in per-subtree).
- Programmatic scrolling of an arbitrary focused input into view outside a `KeyboardAvoidance` subtree.

## Approach

A new **keyboard-inset path** parallel to the existing safe-area path, kept deliberately distinct:

- **Keyboard inset** = transient bottom occlusion by the software keyboard.
- **Safe area** = persistent device insets (notch / status bar / home indicator).

```
UIKit keyboard notification                 vexo (framework)
─────────────────────────                    ─────────────────
UIResponder.keyboardWillShow/WillHide  ──►  objc2 shim (ios only)
  • end frame (window coords)                  │  converts height → logical px,
  • animation duration                         │  stores target + duration + curve
  • animation curve                            ▼
                                        KeyboardInsetSource  (Arc-atomic, dumb value:
                                          target_height: f32, duration_secs, curve)
                                          │
                        read live during render
                                          ▼
                                   KeyboardAvoidance widget  (Component)
                                   • state tweens current → target via AnimationTicker
                                   • bottom padding = tweened inset
                                   • effective bottom = max(safe_area.bottom, tweened)
                                          │
                                          ▼
                                   chat_screen outer column wrapped in KeyboardAvoidance
                                   → ScrollView (flex_fill) shrinks, input bar lifts
```

**Why this shape:**
- `KeyboardInsetSource` is a pure `Arc`-atomic value (identical design to `SafeAreaSource`) — cheap to clone, lock-free reads from layout/paint. The shim only *writes* it on notifications.
- The widget owns the *animated* value in its own state. Animation stays a widget concern, consistent with every other animated widget in Vexo.
- Desktop/Android: the shim is absent, the source stays at 0, `KeyboardAvoidance` renders zero padding. No behavior change off-iOS, no new dependencies on non-iOS targets.

## Component 1 — `KeyboardInsetSource`

A dumb, `Arc`-atomic value mirroring `SafeAreaSource`'s design. Holds the **target** inset (set instantly by the shim on each notification) plus the last notification's duration and curve, so the widget knows *how* to tween to that target.

**Location:** `vexo/src/core/geometry.rs` (next to `SafeAreaSource`), re-exported from `vexo/src/core/mod.rs` and `vexo/src/lib.rs`.

```rust
#[derive(Clone)]
pub struct KeyboardInsetSource {
    inner: Arc<KeyboardInsetInner>,
}

struct KeyboardInsetInner {
    // Target bottom inset in logical px (0 when keyboard is down/hidden).
    target_height: AtomicU32,
    // Duration of the keyboard's own animation, in seconds.
    // 0.0 means "no animation" (use final value immediately).
    duration_secs: AtomicU32,
    // Keyboard animation curve stored as AtomicU8 (KeyboardCurve discriminant).
    curve: AtomicU8,
}
```

**`KeyboardCurve` enum** (`Default` = `EaseInEaseOut`):
- `Default` — UIKit's default keyboard curve (ease-in-ease-out). The one used in practice.
- `Linear` / `EaseIn` / `EaseOut` — the other UIKit keyboard curves, kept for completeness; the widget's tween applies them via Vexo's existing `CubicBezierCurve` / `Curve` infrastructure.

**API:**
- `KeyboardInsetSource::default()` → all zero (keyboard down).
- `get(&self) -> KeyboardInsetSnapshot { target_height, duration_secs, curve }`.
- `set_target(&self, height: f32, duration_secs: f32, curve: KeyboardCurve)` — called only by the shim.
- `current_target_height(&self) -> f32` — convenience for widgets that skip animation.

**Wire-up:** `WindowState` owns a `KeyboardInsetSource` (alongside `safe_area_source`), passes a clone into `ThreeTreePipeline` via a new `set_keyboard_inset_source(...)` (mirrors the existing `set_safe_area_source`), and the pipeline exposes it to `RenderContext` via `keyboard_inset_source()` (mirrors `safe_area_source()`). `BuildOwner` stores the second source the same way it stores the first.

**Per-frame dirty propagation:** `KeyboardInsetSource` is a dumb `Arc`-atomic with no callback (mirrors `SafeAreaSource`). So `WindowState`'s render loop polls it each frame, exactly as it already polls `safe_area_source`: compare current snapshot to previous; if changed, call `pipeline.mark_all_needs_layout()`. This is what drives the widget to re-render and observe the new target — without polling *inside* the widget.

**Lifecycle:** the source is created before the first frame (like `safe_area_source`) and lives for the window's lifetime. The shim attaches its UIKit observers when `WindowState` is constructed and detaches on drop.

**Why duration + curve on the source (not just height):** the widget needs them to start a tween synchronized with the OS keyboard animation. If the source stored only the height, the widget would tween with a fixed duration/curve and visibly disagree with the keyboard slide.

## Component 2 — objc2 UIKit shim

A `#[cfg(target_os = "ios")]` module that registers UIKit keyboard notifications, extracts the frame/duration/curve, and writes them into the `KeyboardInsetSource`. Mirrors the existing clipboard shim's pattern (objc2 + objc2-ui-kit behind a cfg gate).

**File:** `vexo/src/platform/keyboard_ios.rs`

**Responsibilities:**
1. Register observers for `UIResponder.keyboardWillShowNotification` and `UIResponder.keyboardWillHideNotification` on the shared `NotificationCenter`.
2. On each notification, extract:
   - **End-frame height** from `userInfo[UIResponder.keyboardFrameEndUserInfoKey]` (an `NSValue` wrapping a `CGRect` in window coordinates). Convert to logical px: `height / scale_factor`.
   - **Animation duration** from `userInfo[UIResponder.keyboardAnimationDurationUserInfoKey]` (an `NSNumber` of seconds).
   - **Animation curve** from `userInfo[UIResponder.keyboardAnimationCurveUserInfoKey]` (an `NSNumber`, raw enum 0–3).
3. Compute the target inset:
   - `keyboardWillShow`: `target_height = frame_end_height`.
   - `keyboardWillHide`: `target_height = 0`.
4. Call `source.set_target(height, duration, curve)`.

**Attachment lifecycle:**
- `KeyboardObserver::install(source: KeyboardInsetSource, scale_factor: f64) -> KeyboardObserver` — called once during `WindowState::new()` (iOS only). The returned handle holds the observer tokens.
- `Drop` for `KeyboardObserver` removes the observers from `NotificationCenter` (defensive; on iOS app process lifetime ≈ window lifetime, but clean shutdown matters for tests that construct/tear down a `WindowState`).

**Scale factor handling:** the shim captures `scale_factor` at install time to convert the CGRect (physical px) → logical px. If the scale ever changes (rare on iOS; possible on iPad with stage manager), `WindowState`'s existing scale-change path can tear down and re-install the observer. v1 documents this as a known limitation; re-install is YAGNI until observed in practice.

**Keyboard frame coordinate space:** the reported `CGRect` is in the *window's* coordinate space (origin at the window's top-left). The keyboard always sits at the bottom, so `target_height = rect.size.height` is correct regardless of orientation, *as long as the window fills the screen*. For slide-over / stage-manager (window smaller than screen), `rect.size.height` may exceed the window height. v1 clamps: `target_height = min(rect.size.height, window_logical_height)`. Defensive guard; normal phone case unaffected.

**Desktop / Android:** the file is `#[cfg(target_os = "ios")]`-gated; `WindowState` calls `KeyboardObserver::install(...)` only inside `#[cfg(target_os = "ios")]`. On other platforms `KeyboardInsetSource` exists but stays at its default (0), so `KeyboardAvoidance` renders zero padding. No new dependencies on non-iOS targets.

## Component 3 — `KeyboardAvoidance` widget

An opt-in `Component` that lifts its child above the keyboard. Its state owns a tween from the current inset → target inset, driven by `AnimationTicker`, synchronized to the keyboard's own duration/curve.

**File:** `vexo/src/widgets/keyboard_avoidance.rs` (alongside `safe_area.rs`), re-exported from `vexo/src/widgets/mod.rs` and `vexo/src/lib.rs`.

**Behavior:**
- Reads `KeyboardInsetSource` live each render (cheap `Arc`-atomic read, like `SafeAreaSource`).
- When the **target** changes (detected in `render()` by comparing the source's current snapshot to the state's `last_seen_target` field), the state starts a tween:
  - `from` = current animated inset
  - `to` = new target height
  - `duration` = source's `duration_secs`
  - `curve` = source's `KeyboardCurve` (mapped to Vexo's `Curve` / `CubicBezierCurve`)
  - Advance the tween via `LifecycleContext::on_tick()` (Vexo's `requestAnimationFrame` equivalent) while it's active; on each tick, write the animated value into a `Signal<f32>` so the element rebuilds and the render object re-pads.
  - When the tween completes (or `duration_secs == 0.0`), snap to `to` and stop ticking.
- Effective bottom padding each render = `max(safe_area_bottom, animated_inset)`:
  - Keyboard down → `safe_area_bottom` (clears home indicator).
  - Keyboard up → `animated_inset` (keyboard subsumes home indicator; `animated_inset ≥ safe_area_bottom` because the keyboard covers that region).
  - Using `max` rather than sum avoids double-counting the home-indicator strip during the slide.
- Top/left/right padding: zero (keyboard only occludes the bottom). The widget is *only* about keyboard avoidance; notch/status-bar avoidance remains `SafeArea`'s job — the two compose.

**Structure (widget tree produced by `render`):**

```
KeyboardAvoidance(child)
  └─ ContainerRenderObject { Layout: column, padding: (0, 0, 0, effective_bottom) }
       └─ child
```

The render object is a thin `ContainerRenderObject` whose `Layout` is rebuilt whenever the animated inset changes (mirroring how `SafeAreaRenderObject` rebuilds its layout from live insets each `layout()`). `flex_grow(1.0)` + `min_height(0.0)` so it fills its parent and can shrink inside a `TabBarView`.

**Re-render triggers:**
1. Shim writes a new target → `WindowState`'s per-frame poll detects the change → `pipeline.mark_all_needs_layout()` → element is marked dirty → widget's `render()` re-runs.
2. In `render()`, the state compares the source's current snapshot to `last_seen_target`; if different, it starts/retargets a tween and updates `last_seen_target`.
3. Tween ticks (via `on_tick`) → `Signal::set(animated_value)` → element marked dirty → `render()` re-runs → render object gets new padding → layout re-runs.

**Edge cases:**
- **New target mid-tween:** start a fresh tween from the *current animated value* to the new target (no jump). This is what feels right when the user toggles between keyboards of different heights (e.g. emoji → predictive).
- **`duration_secs == 0.0`:** snap immediately, no ticker registration (handles `WillHide` sometimes carrying duration 0 on quick dismissals).
- **Unmount during tween:** `on_unmount` removes the ticker callback (existing `LifecycleContext` API).

**Why a `Component` with state (not a leaf render-object widget):** the animation needs a place to live across renders, and Vexo's pattern is "stateful concerns live in `ComponentState` + `Signal`." A render-object-only widget would have no clean place to own the tween/ticker handle.

**Desktop/Android:** source stays at 0, no tween ever starts, effective padding is `max(safe_area.bottom, 0) = 0` on desktop (safe area is zero there) → transparent pass-through.

## Component 4 — Chat screen integration

Wrap the chat screen's outer column in `KeyboardAvoidance`. Minimal, surgical change to `shared_app/src/chats/chat_screen.rs`.

**Current structure** (`render`):
```
DecoratedBox(background)
  └─ MultiChild(column, flex_grow(1), min_height(0))
       ├─ WithLayout(ScrollView, flex_fill)   ← message list
       └─ input_bar                            ← pinned to bottom, but under keyboard
```

**Proposed structure:**
```
DecoratedBox(background)
  └─ KeyboardAvoidance                         ← new wrapper
       └─ MultiChild(column, flex_grow(1), min_height(0))
            ├─ WithLayout(ScrollView, flex_fill)
            └─ input_bar
```

`KeyboardAvoidance` is a `flex_grow(1.0)` + `min_height(0.0)` column with bottom padding = `max(safe_area.bottom, animated_keyboard_inset)`. The inner `MultiChild` fills it. When the keyboard rises:
- The outer column's bottom padding grows → its content area shrinks from the bottom.
- `ScrollView` (flex_fill) absorbs the shrink → message list reflows.
- `input_bar` (no flex-grow, fixed height) stays glued to the new bottom edge → sits just above the keyboard.

**Why outside `DecoratedBox`?** The background should fill the whole screen (including the region behind the keyboard, so no flash of background-color gap during the slide). `KeyboardAvoidance` only insets its *child*, not the background. Putting it inside `DecoratedBox` would shrink the background to exclude the keyboard region — visually wrong.

**Why outside the inner `MultiChild`?** The bottom padding must apply to the *column's* content box (so both the scroll view and input bar lift together). If `KeyboardAvoidance` were between the scroll view and input bar, only one would move.

**Existing test impact:**
- `test_chat_screen_renders_messages`: still passes — pipeline element count grows by a constant (the `KeyboardAvoidance` element + its container render object), the assertion (`> 4`) still holds.
- `test_chat_screen_input_bar_pinned_to_bottom_with_few_messages`: currently asserts `input_bottom >= 599.0` for a 600px-tall view with zero messages and no keyboard. With `KeyboardAvoidance` and no keyboard (source = 0) and desktop safe-area (0), effective bottom padding = 0, so the input bar still pins to 600. The assertion still holds. (On a real device the safe-area bottom inset would push it up by ~34px — but the test runs on desktop, where safe area is 0, so no change.)

**No other changes to `chat_screen.rs`:** the input bar builder, message bubbles, scroll controller, send closure — all untouched. The widget composition change is a single wrapping.

## Testing strategy

Layered: unit tests for the pure pieces, integration tests for the tween + layout behavior, and an explicit acknowledgment that the UIKit shim is exercised only on-device.

**1. `KeyboardInsetSource` unit tests** (`vexo/src/core/geometry.rs`, alongside `SafeAreaSource` tests):
- `default()` returns all-zero snapshot.
- `set_target(...)` then `get()` returns the written values.
- Clones share storage: `set_target` on one is visible to the other (mirrors the existing `SafeAreaSource` shared-storage test).
- `current_target_height()` returns the latest written height.

**2. `KeyboardAvoidance` widget tests** (`vexo/src/widgets/keyboard_avoidance.rs`):
- *No keyboard, desktop safe-area (0):* effective bottom padding = 0; child renders at full height. Transparent pass-through.
- *No keyboard, mobile safe-area (bottom = 34):* effective bottom padding = 34; child shrinks by 34.
- *Keyboard up, no animation (`duration_secs == 0`):* `set_target(300, 0, Default)` → widget snaps to 300; effective padding = `max(34, 300)` = 300.
- *Keyboard up, with animation:* set target with `duration_secs = 0.25`; advance the `AnimationTicker` by half the duration; assert the animated inset is approximately halfway between 0 and 300 (within tolerance); advance to completion; assert exactly 300.
- *Mid-tween retarget:* start a 0→300 tween; at 25% advance, retarget to 0 (`keyboardWillHide`); assert the new tween starts from the ~75 value, not from 0 or 300.
- *Effective padding uses `max`:* keyboard down, safe-area bottom 34 → padding 34; keyboard up to 300 → padding 300; keyboard down to 0 → padding 34 (never below safe-area bottom).

**3. Layout integration test** (in the same file, using `ThreeTreePipeline` + `TaffyLayoutEngine` like the chat screen's existing test):
- Mount `KeyboardAvoidance(child)` in a 400×800 frame.
- Snap keyboard to 300px, layout, assert the child's computed bounds height = 500 (800 − 300) and its bottom edge = 500.
- Retarget to 0, layout, assert child height = 800.

**4. Chat screen regression** — no new test; verify the two existing tests still pass under the new wrapping (covered in Component 4). The framework tests cover avoidance behavior; the chat screen test only needs to confirm the wrapping didn't break the pinned-bottom invariant.

**5. UIKit shim — not unit-tested directly.** Same posture as the clipboard shim: the shim is a thin adapter from UIKit notifications to `KeyboardInsetSource::set_target`. Its contract is tested via the source tests (#1) and the widget tests (#2, #3) that simulate `set_target` calls. On-device validation is the only way to exercise the real notification path; this is called out as a known limitation.

**Test helper:** a small `TestKeyboardSource` helper (or just `KeyboardInsetSource::default()` + direct `set_target` calls in tests) — no mock trait needed, since the source is a concrete `Arc`-atomic value, not a trait object. This keeps the test surface concrete.

## Error handling & edge cases

**1. Scale factor changes mid-session (iPad stage manager).** Shim captures scale at install. If it changes, the cached conversion is stale. v1: documented limitation; `WindowState`'s existing scale-change path can re-install the observer if observed in practice. Not in scope for this spec.

**2. Multiple `KeyboardAvoidance` widgets mounted simultaneously.** Each owns its own tween but reads the same shared source. They animate in lockstep (same target/duration/curve), so visually consistent. No coordination needed. Cost: N tween registrations for N widgets — fine in practice (typically 1).

**3. Source updated while widget is unmounted.** No tween runs (no state). On next mount, `on_mount` reads the current target and sets `animated_value = current_target` and `last_seen_target = current_target` (snap, no tween) — the keyboard is already up (or down), animating would be wrong. Animation only happens *in response to* a change observed while mounted.

**4. `duration_secs == 0.0` on `keyboardWillShow`.** Snap, no tween. (Sometimes happens for programmatic focus.) Covered in Component 3.

**5. Orientation change during animation.** UIKit fires a new `keyboardWillShow` with updated frame; the widget retargets from current animated value (Component 3's mid-tween retarget). No special handling.

**6. Window smaller than screen (slide-over).** Clamp `target_height = min(rect.size.height, window_logical_height)` (Component 2). Defensive; normal phone case unaffected.

**7. Desktop/Android.** Shim absent; source stays at 0; `KeyboardAvoidance` is a transparent pass-through. No behavior change, no new deps, no cfg-gated call sites beyond `WindowState`'s `#[cfg(target_os = "ios")]` install.

## File touch-list

Framework (`vexo/`):
- `src/core/geometry.rs` — add `KeyboardInsetSource`, `KeyboardInsetSnapshot`, `KeyboardCurve` + unit tests.
- `src/core/mod.rs` — re-export the new types.
- `src/lib.rs` — re-export the new types.
- `src/platform/keyboard_ios.rs` (new, `#[cfg(target_os = "ios")]`) — objc2 UIKit shim + `KeyboardObserver`.
- `src/platform/mod.rs` — declare `keyboard_ios` (cfg-gated).
- `src/window.rs` — own `KeyboardInsetSource`, install `KeyboardObserver` on iOS, pass source to pipeline, add per-frame poll (compare snapshot to previous, `mark_all_needs_layout()` on change) next to the existing `safe_area_source` poll.
- `src/pipeline.rs` — `set_keyboard_inset_source(...)`, expose via `RenderContext`.
- `src/build_owner.rs` — store `KeyboardInsetSource` alongside `SafeAreaSource`.
- `src/stateful_widget.rs` — `RenderContext::keyboard_inset_source()` accessor.
- `src/widgets/keyboard_avoidance.rs` (new) — `KeyboardAvoidance` widget + tests.
- `src/widgets/mod.rs` — re-export `KeyboardAvoidance`.
- `Cargo.toml` — extend `objc2-ui-kit` features with the notification/`UIResponder`/`NSValue`/`NSNumber` selectors needed (or add `objc2-foundation` features for `NSDictionary`/`NSNumber` if not already present).

App (`shared_app/`):
- `src/chats/chat_screen.rs` — wrap the outer `MultiChild` in `KeyboardAvoidance` inside `DecoratedBox`.
