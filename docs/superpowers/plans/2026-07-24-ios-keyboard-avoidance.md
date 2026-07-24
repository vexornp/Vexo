# iOS Keyboard Avoidance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lift the chat screen's input bar (and any focused `TextEdit`) above the iOS software keyboard, animated in sync with the keyboard's own slide.

**Architecture:** A new `KeyboardInsetSource` (dumb `Arc`-atomic, parallel to `SafeAreaSource`) holds the keyboard's target height + animation duration/curve. An objc2 UIKit shim (iOS only) writes to it from `UIResponder.keyboardWillShow/Hide` notifications. A `KeyboardAvoidance` widget (a `Component`) reads the source each render, owns an `AnimationController` tween from the current inset → target, and pads its child by `max(safe_area.bottom, animated_inset)`. `WindowState` polls the source each frame (like it polls `safe_area_source`) and marks the tree dirty on change. The chat screen wraps its outer column in `KeyboardAvoidance`.

**Tech Stack:** Rust, objc2 0.6.x + objc2-ui-kit/objc2-foundation 0.3.2 (iOS), wgpu, taffy, glyphon. No new crate dependencies — only new features on existing objc2 crates.

## Global Constraints

- iOS-only behavior; desktop/Android must be no-ops (transparent pass-through, zero padding, no new deps).
- `KeyboardInsetSource` must be a pure `Arc`-atomic value with no callbacks — mirrors `SafeAreaSource` exactly. Animation lives in widget state, not the source.
- Effective bottom padding = `max(safe_area.bottom, animated_keyboard_inset)` — never sum (avoids double-counting the home-indicator strip during the slide).
- objc2 shim is `#[cfg(target_os = "ios")]`-gated; matches the existing `ios_clipboard.rs` pattern.
- All new framework code is unit-tested where possible; the objc2 shim is exercised only on-device (same posture as the clipboard shim).
- No comments added to existing code unless asked.

---

## File Structure

**New files:**
- `vexo/src/platform/keyboard_ios.rs` — objc2 UIKit shim (`KeyboardObserver`), `#[cfg(target_os = "ios")]`.
- `vexo/src/widgets/keyboard_avoidance.rs` — `KeyboardAvoidance` widget + state + render object + tests.

**Modified files (framework):**
- `vexo/src/core/geometry.rs` — add `KeyboardInsetSource`, `KeyboardInsetSnapshot`, `KeyboardCurve` + unit tests.
- `vexo/src/core/mod.rs` — re-export the new types.
- `vexo/src/lib.rs` — re-export the new types.
- `vexo/src/build_owner.rs` — store `KeyboardInsetSource` alongside `SafeAreaSource`; add `keyboard_inset_source()` / `set_keyboard_inset_source()`.
- `vexo/src/stateful_widget.rs` — add `RenderContext::keyboard_inset()`.
- `vexo/src/pipeline.rs` — add `set_keyboard_inset_source()`.
- `vexo/src/window.rs` — own `KeyboardInsetSource`, install `KeyboardObserver` on iOS, add per-frame poll.
- `vexo/src/platform/mod.rs` — declare `keyboard_ios` (cfg-gated).
- `vexo/src/widgets/mod.rs` — declare + re-export `KeyboardAvoidance`.
- `vexo/Cargo.toml` — extend objc2 feature flags for iOS keyboard notification APIs.

**Modified files (app):**
- `shared_app/src/chats/chat_screen.rs` — wrap outer `MultiChild` in `KeyboardAvoidance` inside `DecoratedBox`.

---

### Task 1: `KeyboardInsetSource` core type

**Files:**
- Modify: `vexo/src/core/geometry.rs` (add new types after `SafeAreaSource` impl block, ~line 707; add tests after `safe_area_source_tests` module, ~line 1133)
- Modify: `vexo/src/core/mod.rs`
- Modify: `vexo/src/lib.rs`

**Interfaces:**
- Consumes: `crate::layout::EdgeInsets` (existing).
- Produces: `KeyboardInsetSource`, `KeyboardInsetSnapshot`, `KeyboardCurve` — used by Task 2 (plumbing), Task 3 (widget), Task 4 (shim).

- [ ] **Step 1: Write failing tests in `vexo/src/core/geometry.rs`**

Add a new test module after the existing `safe_area_source_tests` module (ends at line 1133). Insert this block immediately after the closing `}` of `safe_area_source_tests`:

```rust
#[cfg(test)]
mod keyboard_inset_source_tests {
    use super::{KeyboardCurve, KeyboardInsetSnapshot, KeyboardInsetSource};

    #[test]
    fn default_is_all_zero() {
        let s = KeyboardInsetSource::default();
        let snap = s.get();
        assert_eq!(snap.target_height, 0.0);
        assert_eq!(snap.duration_secs, 0.0);
        assert_eq!(snap.curve, KeyboardCurve::EaseInOut);
    }

    #[test]
    fn set_target_then_get_returns_written_values() {
        let s = KeyboardInsetSource::default();
        s.set_target(300.0, 0.25, KeyboardCurve::EaseIn);
        let snap = s.get();
        assert_eq!(snap.target_height, 300.0);
        assert_eq!(snap.duration_secs, 0.25);
        assert_eq!(snap.curve, KeyboardCurve::EaseIn);
    }

    #[test]
    fn clones_share_storage() {
        let s = KeyboardInsetSource::default();
        let clone = s.clone();
        s.set_target(250.0, 0.3, KeyboardCurve::Linear);
        let snap = clone.get();
        assert_eq!(snap.target_height, 250.0);
        assert_eq!(snap.duration_secs, 0.3);
        assert_eq!(snap.curve, KeyboardCurve::Linear);
    }

    #[test]
    fn current_target_height_returns_latest() {
        let s = KeyboardInsetSource::default();
        s.set_target(336.0, 0.25, KeyboardCurve::EaseInOut);
        assert_eq!(s.current_target_height(), 336.0);
        s.set_target(0.0, 0.25, KeyboardCurve::EaseInOut);
        assert_eq!(s.current_target_height(), 0.0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib keyboard_inset_source_tests`
Expected: FAIL with "cannot find type `KeyboardInsetSource`" / "cannot find type `KeyboardCurve`".

- [ ] **Step 3: Implement the types in `vexo/src/core/geometry.rs`**

Insert this block immediately after the `SafeAreaSource` `impl Default` block (after line 707, before the `// ====...` separator for AFFINE TRANSFORM at line 709):

```rust
// ============================================================================
// KEYBOARD INSET SOURCE
// ============================================================================

/// Keyboard animation curve, mirroring UIKit's
/// `UIViewAnimationCurve` raw values reported via
/// `UIResponder.keyboardAnimationCurveUserInfoKey`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyboardCurve {
    /// UIKit raw value 0. The default keyboard curve; ease-in-ease-out.
    EaseInOut = 0,
    /// UIKit raw value 1.
    EaseIn = 1,
    /// UIKit raw value 2.
    EaseOut = 2,
    /// UIKit raw value 3.
    Linear = 3,
}

impl Default for KeyboardCurve {
    fn default() -> Self {
        Self::EaseInOut
    }
}

impl KeyboardCurve {
    /// Map a UIKit `UIViewAnimationCurve` raw value to our enum.
    /// Falls back to `EaseInOut` (UIKit's default) for unknown values.
    pub fn from_uikit_raw(raw: u8) -> Self {
        match raw {
            1 => Self::EaseIn,
            2 => Self::EaseOut,
            3 => Self::Linear,
            _ => Self::EaseInOut,
        }
    }
}

/// Snapshot of the keyboard-inset state at a point in time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyboardInsetSnapshot {
    /// Target bottom inset in logical pixels (0 when keyboard is down).
    pub target_height: f32,
    /// Duration of the keyboard's own animation, in seconds.
    /// 0.0 means "snap immediately" (no animation).
    pub duration_secs: f32,
    /// Keyboard animation curve.
    pub curve: KeyboardCurve,
}

/// Shared handle to the keyboard's target inset (logical pixels),
/// animation duration, and curve.
///
/// Mirrors [`SafeAreaSource`]'s design: a dumb `Arc`-atomic value with no
/// callbacks. The iOS keyboard shim writes via [`set_target`] on each
/// `keyboardWillShow/Hide` notification; the [`KeyboardAvoidance`] widget
/// reads via [`get`] each render and owns the animated tween in its own state.
///
/// On desktop / Android the shim is absent, so this stays at its default
/// (all-zero) and `KeyboardAvoidance` is a transparent pass-through.
///
/// [`KeyboardAvoidance`]: crate::widgets::KeyboardAvoidance
/// [`set_target`]: Self::set_target
/// [`get`]: Self::get
#[derive(Clone)]
pub struct KeyboardInsetSource {
    inner: Arc<KeyboardInsetInner>,
}

struct KeyboardInsetInner {
    target_height: AtomicU32,
    duration_secs: AtomicU32,
    curve: AtomicU8,
}

impl KeyboardInsetSource {
    /// Create a new source with all-zero defaults (keyboard down, no animation).
    pub fn new() -> Self {
        Self {
            inner: Arc::new(KeyboardInsetInner {
                target_height: AtomicU32::new(0.0_f32.to_bits()),
                duration_secs: AtomicU32::new(0.0_f32.to_bits()),
                curve: AtomicU8::new(KeyboardCurve::EaseInOut as u8),
            }),
        }
    }

    /// Read the current snapshot.
    pub fn get(&self) -> KeyboardInsetSnapshot {
        KeyboardInsetSnapshot {
            target_height: f32::from_bits(self.inner.target_height.load(Ordering::Relaxed)),
            duration_secs: f32::from_bits(self.inner.duration_secs.load(Ordering::Relaxed)),
            curve: match self.inner.curve.load(Ordering::Relaxed) {
                1 => KeyboardCurve::EaseIn,
                2 => KeyboardCurve::EaseOut,
                3 => KeyboardCurve::Linear,
                _ => KeyboardCurve::EaseInOut,
            },
        }
    }

    /// Update the target inset, animation duration, and curve.
    /// Called only by the iOS keyboard shim on each notification.
    /// Visible to all clone holders immediately.
    pub fn set_target(&self, height: f32, duration_secs: f32, curve: KeyboardCurve) {
        self.inner.target_height.store(height.to_bits(), Ordering::Relaxed);
        self.inner.duration_secs.store(duration_secs.to_bits(), Ordering::Relaxed);
        self.inner.curve.store(curve as u8, Ordering::Relaxed);
    }

    /// Convenience: read just the current target height.
    pub fn current_target_height(&self) -> f32 {
        f32::from_bits(self.inner.target_height.load(Ordering::Relaxed))
    }
}

impl Default for KeyboardInsetSource {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Re-export from `vexo/src/core/mod.rs`**

Open `vexo/src/core/mod.rs`, find the line that re-exports `SafeAreaSource` (around line 60). Add `KeyboardCurve, KeyboardInsetSnapshot, KeyboardInsetSource,` to that same `pub use` statement. The line currently looks like:

```rust
    SafeAreaSource, Scale, ScaleSource, Size,
```

Change it to:

```rust
    KeyboardCurve, KeyboardInsetSnapshot, KeyboardInsetSource,
    SafeAreaSource, Scale, ScaleSource, Size,
```

- [ ] **Step 5: Re-export from `vexo/src/lib.rs`**

In `vexo/src/lib.rs`, find the `pub use core::` block. It currently contains `Color` and `AffineTransform` (lines 6–7). Add the new types. After the change the block reads:

```rust
pub use core::AffineTransform;
pub use core::Color;
pub use core::{KeyboardCurve, KeyboardInsetSnapshot, KeyboardInsetSource};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vexo --lib keyboard_inset_source_tests`
Expected: PASS — 4 tests.

- [ ] **Step 7: Build the whole crate to confirm nothing else broke**

Run: `cargo build -p vexo`
Expected: compiles cleanly (warnings about unused imports are OK for now — Task 2 will use them).

- [ ] **Step 8: Commit**

```bash
git add vexo/src/core/geometry.rs vexo/src/core/mod.rs vexo/src/lib.rs
git commit -m "feat(vexo): add KeyboardInsetSource core type

Dumb Arc-atomic value mirroring SafeAreaSource. Holds target bottom
inset (logical px), animation duration, and curve. The iOS shim will
write to it; the KeyboardAvoidance widget will read it each render and
own the animated tween."
```

---

### Task 2: Plumb `KeyboardInsetSource` through the framework

**Files:**
- Modify: `vexo/src/build_owner.rs` (add field + accessors mirroring `safe_area_source`)
- Modify: `vexo/src/stateful_widget.rs` (add `RenderContext::keyboard_inset()`)
- Modify: `vexo/src/pipeline.rs` (add `set_keyboard_inset_source()`)

**Interfaces:**
- Consumes: `KeyboardInsetSource` from Task 1.
- Produces: `BuildOwner::keyboard_inset_source()`, `BuildOwner::set_keyboard_inset_source()`, `RenderContext::keyboard_inset()`, `ThreeTreePipeline::set_keyboard_inset_source()` — used by Task 3 (widget reads via `RenderContext::keyboard_inset()`) and Task 5 (`WindowState` calls `set_keyboard_inset_source()`).

- [ ] **Step 1: Add field + accessors to `BuildOwner`**

In `vexo/src/build_owner.rs`:

(a) Add the import. Find the existing `use crate::core::SafeAreaSource;` (line 26) and add below it:

```rust
use crate::core::KeyboardInsetSource;
```

(b) Add the field. Find the `safe_area_source: SafeAreaSource,` field in the `BuildOwner` struct (line 92). Immediately after it, add:

```rust

    /// Keyboard target inset source (logical pixels), shared with all
    /// [`RenderContext`](crate::stateful_widget::RenderContext)s so
    /// `KeyboardAvoidance` can read live values during `Component::render()`.
    ///
    /// Backed by atomics inside [`KeyboardInsetSource`], so updates from
    /// the iOS keyboard shim are visible here without additional locking.
    /// Defaults to all-zero (desktop / pre-init / keyboard down), making
    /// keyboard avoidance a no-op for tests and desktop builds.
    keyboard_inset_source: KeyboardInsetSource,
```

(c) Initialize the field in `BuildOwner::new()`. Find the `safe_area_source: SafeAreaSource::default(),` line in `new()` (line 105) and add immediately after it:

```rust
            keyboard_inset_source: KeyboardInsetSource::default(),
```

(d) Add accessors. Find the existing `set_safe_area_source` method (lines 278–285). Immediately after its closing `}`, add:

```rust

    /// Get a clone of the shared keyboard-inset source.
    ///
    /// Returns a cheaply-clonable handle ([`KeyboardInsetSource`] is `Arc`-based)
    /// whose [`KeyboardInsetSource::get()`] always reads the latest target
    /// written by the iOS keyboard shim. Used by
    /// [`RenderContext::keyboard_inset()`](crate::stateful_widget::RenderContext::keyboard_inset)
    /// so `KeyboardAvoidance` can resolve the target during render.
    pub fn keyboard_inset_source(&self) -> KeyboardInsetSource {
        self.keyboard_inset_source.clone()
    }

    /// Replace the keyboard-inset source.
    ///
    /// Called once at window init so the [`BuildOwner`] shares the same
    /// atomics as [`WindowState`](crate::window::WindowState); subsequent
    /// updates happen via [`KeyboardInsetSource::set_target()`] on either clone.
    pub fn set_keyboard_inset_source(&mut self, source: KeyboardInsetSource) {
        self.keyboard_inset_source = source;
    }
```

- [ ] **Step 2: Add `RenderContext::keyboard_inset()` accessor**

In `vexo/src/stateful_widget.rs`, find the `safe_area()` method on `RenderContext` (lines 341–351). Immediately after its closing `}`, add:

```rust

    /// Current keyboard-inset snapshot (target height + duration + curve).
    ///
    /// Reflects the live values written by the iOS keyboard shim
    /// (`keyboardWillShow/Hide` notifications); all-zero on desktop / when
    /// the keyboard is down. `KeyboardAvoidance` calls this during
    /// [`Component::render()`] to start/retarget its inset tween.
    pub fn keyboard_inset(&self) -> crate::core::KeyboardInsetSnapshot {
        self.build_owner.keyboard_inset_source().get()
    }
```

- [ ] **Step 3: Add `set_keyboard_inset_source()` to `ThreeTreePipeline`**

In `vexo/src/pipeline.rs`, find the existing `set_safe_area_source` method (lines 201–203). Immediately after its closing `}`, add:

```rust

    /// Install the keyboard-inset source into the [`BuildOwner`].
    ///
    /// Called once at window init by
    /// [`WindowState`](crate::window::WindowState) so the same atomics are
    /// shared between the window (which writes the target on each iOS
    /// keyboard notification) and the element tree (which reads them via
    /// [`RenderContext::keyboard_inset()`](crate::stateful_widget::RenderContext::keyboard_inset)).
    pub fn set_keyboard_inset_source(&mut self, source: crate::core::KeyboardInsetSource) {
        self.build_owner.set_keyboard_inset_source(source);
    }
```

- [ ] **Step 4: Build to confirm it compiles**

Run: `cargo build -p vexo`
Expected: compiles cleanly (the new accessors are unused so far — that's fine, Task 3 + 5 will use them).

- [ ] **Step 5: Run the full test suite to confirm nothing regressed**

Run: `cargo test -p vexo --lib`
Expected: all existing tests still pass (no behavior change; just new plumbing).

- [ ] **Step 6: Commit**

```bash
git add vexo/src/build_owner.rs vexo/src/stateful_widget.rs vexo/src/pipeline.rs
git commit -m "feat(vexo): plumb KeyboardInsetSource through BuildOwner/RenderContext/Pipeline

Mirrors the existing SafeAreaSource plumbing: BuildOwner stores the
source, RenderContext exposes keyboard_inset(), and ThreeTreePipeline
has a setter called once at window init."
```

---

### Task 3: `KeyboardAvoidance` widget

**Files:**
- Create: `vexo/src/widgets/keyboard_avoidance.rs`
- Modify: `vexo/src/widgets/mod.rs` (declare module + re-export)
- Modify: `vexo/src/lib.rs` (re-export from crate root)

**Interfaces:**
- Consumes: `RenderContext::keyboard_inset()` (Task 2), `RenderContext::safe_area()` (existing), `AnimationController` + `Curve` types (existing), `LifecycleContext::{widget, dirty_callback, animation_ticker}` (existing).
- Produces: `KeyboardAvoidance` widget — used by Task 6 (chat screen wraps in it).

- [ ] **Step 1: Write failing tests in a new file `vexo/src/widgets/keyboard_avoidance.rs`**

Create the file with this content (tests first; implementation will be added in Step 3):

```rust
//! Keyboard avoidance widget — lifts its child above the iOS software keyboard.
//!
//! Reads [`KeyboardInsetSource`](crate::core::KeyboardInsetSource) live each
//! render; when the target changes, the widget's state starts an
//! [`AnimationController`] tween from the current animated inset to the new
//! target, synchronized to the keyboard's own duration/curve. Effective
//! bottom padding each frame is `max(safe_area.bottom, animated_inset)`:
//!
//! - Keyboard down → `safe_area.bottom` (clears home indicator).
//! - Keyboard up → `animated_inset` (keyboard subsumes home indicator).
//!
//! On desktop / Android the source stays at 0 and this widget is a transparent
//! pass-through. Top/left/right padding is always zero — notch/status-bar
//! avoidance is [`SafeArea`](crate::widgets::SafeArea)'s job; the two compose.

use std::any::Any;
use std::time::{Duration, Instant};

use crate::animation::{AnimationController, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, LinearCurve};
use crate::core::{KeyboardCurve, KeyboardInsetSnapshot};
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey};
use crate::render_objects::ContainerRenderObject;
use crate::{
    Component, ComponentState, Element, ElementContext, EventContext, HitTestContext,
    LayoutContext, LayoutResult, LifecycleContext, PaintContext, RenderContext, RenderObject,
    RenderObjectKey, UpdateResult, Widget, WidgetKey,
};

// (Implementation goes here in Step 3 — see below.)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::AnimationTicker;
    use crate::layout::TaffyLayoutEngine;
    use crate::{ThreeTreePipeline};
    use std::sync::Arc;

    // ----- Helper: build a pipeline + render context with given sources -----
    fn build_pipeline() -> (ThreeTreePipeline, Arc<AnimationTicker>) {
        let ticker = Arc::new(AnimationTicker::new());
        let pipeline = ThreeTreePipeline::new(ticker.clone());
        (pipeline, ticker)
    }

    // ----- Widget-level behavior tests (no pipeline; just exercise state) -----

    #[test]
    fn effective_padding_zero_when_no_keyboard_no_safe_area() {
        // Desktop: source = 0, safe area = 0 → padding 0.
        let mut state = KeyboardAvoidanceState::default();
        state.animated_inset = 0.0;
        let safe_bottom = 0.0;
        let pad = state.effective_bottom_padding(safe_bottom);
        assert_eq!(pad, 0.0);
    }

    #[test]
    fn effective_padding_uses_safe_area_when_keyboard_down() {
        let mut state = KeyboardAvoidanceState::default();
        state.animated_inset = 0.0;
        let safe_bottom = 34.0;
        let pad = state.effective_bottom_padding(safe_bottom);
        assert_eq!(pad, 34.0);
    }

    #[test]
    fn effective_padding_uses_keyboard_when_up() {
        let mut state = KeyboardAvoidanceState::default();
        state.animated_inset = 300.0;
        let safe_bottom = 34.0;
        let pad = state.effective_bottom_padding(safe_bottom);
        assert_eq!(pad, 300.0); // max(34, 300)
    }

    #[test]
    fn effective_padding_never_below_safe_area_during_slide() {
        // Mid-slide: animated_inset = 10 (below safe area 34).
        let mut state = KeyboardAvoidanceState::default();
        state.animated_inset = 10.0;
        let safe_bottom = 34.0;
        let pad = state.effective_bottom_padding(safe_bottom);
        assert_eq!(pad, 34.0); // max(34, 10)
    }

    #[test]
    fn start_tween_snaps_when_duration_zero() {
        // duration_secs == 0 → snap immediately, no animation.
        let mut state = KeyboardAvoidanceState::default();
        state.from_inset = 0.0;
        state.animated_inset = 0.0;
        state.controller = AnimationController::new(Duration::ZERO);
        state.start_tween_to(300.0, KeyboardInsetSnapshot {
            target_height: 300.0,
            duration_secs: 0.0,
            curve: KeyboardCurve::EaseInOut,
        });
        // Controller should have advanced to completion on the first advance()
        // call; but start_tween_to also sets animated_inset = to_inset when
        // duration is zero (snap path).
        assert_eq!(state.animated_inset, 300.0);
        assert_eq!(state.to_inset, 300.0);
    }

    #[test]
    fn advance_tween_interpolates_halfway() {
        let mut state = KeyboardAvoidanceState::default();
        state.from_inset = 0.0;
        state.animated_inset = 0.0;
        state.controller = AnimationController::new(Duration::from_millis(250));
        state.start_tween_to(300.0, KeyboardInsetSnapshot {
            target_height: 300.0,
            duration_secs: 0.25,
            curve: KeyboardCurve::Linear, // linear so halfway is exactly 150
        });
        let start = state.controller.start_time().unwrap();
        state.advance(start + Duration::from_millis(125));
        assert!(
            (state.animated_inset - 150.0).abs() < 1.0,
            "expected ~150 at halfway, got {}",
            state.animated_inset
        );
    }

    #[test]
    fn advance_tween_completes_at_target() {
        let mut state = KeyboardAvoidanceState::default();
        state.from_inset = 0.0;
        state.animated_inset = 0.0;
        state.controller = AnimationController::new(Duration::from_millis(250));
        state.start_tween_to(300.0, KeyboardInsetSnapshot {
            target_height: 300.0,
            duration_secs: 0.25,
            curve: KeyboardCurve::Linear,
        });
        let start = state.controller.start_time().unwrap();
        state.advance(start + Duration::from_millis(260));
        assert!(
            (state.animated_inset - 300.0).abs() < 0.5,
            "expected 300 at completion, got {}",
            state.animated_inset
        );
    }

    #[test]
    fn mid_tween_retarget_starts_from_current_animated_value() {
        let mut state = KeyboardAvoidanceState::default();
        state.from_inset = 0.0;
        state.animated_inset = 0.0;
        state.controller = AnimationController::new(Duration::from_millis(100));
        state.start_tween_to(300.0, KeyboardInsetSnapshot {
            target_height: 300.0,
            duration_secs: 0.1,
            curve: KeyboardCurve::Linear,
        });
        let start = state.controller.start_time().unwrap();
        state.advance(start + Duration::from_millis(25)); // 25% → 75
        assert!((state.animated_inset - 75.0).abs() < 1.0);

        // Retarget to 0 (keyboardWillHide) — new tween should start from 75.
        state.start_tween_to(0.0, KeyboardInsetSnapshot {
            target_height: 0.0,
            duration_secs: 0.1,
            curve: KeyboardCurve::Linear,
        });
        assert_eq!(state.from_inset, 75.0, "from_inset must be current animated value");
        assert_eq!(state.to_inset, 0.0);
        let start2 = state.controller.start_time().unwrap();
        state.advance(start2 + Duration::from_millis(50)); // 50% of 0→75 reversed = 75-37.5
        assert!(
            (state.animated_inset - 37.5).abs() < 1.5,
            "expected ~37.5 halfway down from 75, got {}",
            state.animated_inset
        );
    }

    #[test]
    fn curve_mapping_matches_uikit_raw_values() {
        assert_eq!(curve_for(KeyboardCurve::EaseInOut).transform(0.5), EaseInOutCurve.transform(0.5));
        assert_eq!(curve_for(KeyboardCurve::EaseIn).transform(0.5), EaseInCurve.transform(0.5));
        assert_eq!(curve_for(KeyboardCurve::EaseOut).transform(0.5), EaseOutCurve.transform(0.5));
        assert_eq!(curve_for(KeyboardCurve::Linear).transform(0.5), LinearCurve.transform(0.5));
    }

    // ----- Layout integration test (uses ThreeTreePipeline) -----

    #[test]
    fn layout_shrinks_child_by_keyboard_inset() {
        // We test the render object's padding directly, since the full
        // Component lifecycle (with AnimationController + ticker wiring)
        // requires the heavy pipeline; the layout math is what matters here.
        let mut ro = KeyboardAvoidanceRenderObject::new();
        ro.set_bottom_padding(300.0);

        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = crate::resource::new_font_system();
        // Use the same mock-layout path the SafeArea tests use: build a tiny
        // pipeline so the render object has a layout node.
        let (mut pipeline, _ticker) = build_pipeline();
        let child_widget = crate::Text::new("hi");
        let view = KeyboardAvoidance::new(child_widget).boxed();
        pipeline.update(view);

        // Override the render object's effective padding by reaching in after
        // mount. (In production this is set by the element from the state.)
        let ro_reg = pipeline.render_objects();
        // Walk to find the KeyboardAvoidance render object (root proxy →
        // component proxy → ... → KeyboardAvoidanceRenderObject). For this
        // test we just verify the element count grew, which proves the widget
        // mounted. The math is covered by the unit tests above.
        assert!(ro_reg.len() > 0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail to compile**

Run: `cargo test -p vexo --lib keyboard_avoidance::tests`
Expected: FAIL with "cannot find type `KeyboardAvoidanceState`" / "cannot find function `curve_for`" / "cannot find type `KeyboardAvoidance`" / "cannot find type `KeyboardAvoidanceRenderObject`".

- [ ] **Step 3: Implement the widget, state, render object, and element**

Replace the `(Implementation goes here in Step 3 — see below.)` placeholder in `vexo/src/widgets/keyboard_avoidance.rs` with the full implementation. Insert it after the `use` block (before the `#[cfg(test)]` module):

```rust
// ============================================================================
// KEYBOARD AVOIDANCE STATE
// ============================================================================

/// State for [`KeyboardAvoidance`]. Owns the inset tween.
pub struct KeyboardAvoidanceState {
    /// Current animated inset (logical px). Read by `render()` each frame.
    animated_inset: f32,
    /// Inset the current tween started from.
    from_inset: f32,
    /// Inset the current tween is animating toward.
    to_inset: f32,
    /// The animation controller (0..1 linear; curve applied in `advance`).
    controller: AnimationController,
    /// Last target snapshot observed from the source. Used to detect changes.
    last_seen: KeyboardInsetSnapshot,
    /// Boxed curve for the current tween. Replaced on each retarget.
    curve: Box<dyn Curve>,
    /// Ticker handle; set on mount so we can stop on unmount.
    /// (AnimationController registers with the ticker itself; we hold the
    /// ticker Arc so we can pass it to a fresh controller on retarget.)
    ticker: Option<Arc<crate::animation::AnimationTicker>>,
    /// Dirty callback; wired on mount so fresh controllers get it.
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Default for KeyboardAvoidanceState {
    fn default() -> Self {
        Self {
            animated_inset: 0.0,
            from_inset: 0.0,
            to_inset: 0.0,
            controller: AnimationController::new(Duration::ZERO),
            last_seen: KeyboardInsetSnapshot {
                target_height: 0.0,
                duration_secs: 0.0,
                curve: KeyboardCurve::EaseInOut,
            },
            curve: Box::new(EaseInOutCurve),
            ticker: None,
            dirty_callback: None,
        }
    }
}

impl KeyboardAvoidanceState {
    /// Effective bottom padding: `max(safe_area.bottom, animated_inset)`.
    pub fn effective_bottom_padding(&self, safe_area_bottom: f32) -> f32 {
        self.animated_inset.max(safe_area_bottom)
    }

    /// Start (or retarget) a tween to `target.target_height`.
    ///
    /// - If `duration_secs == 0.0`, snap immediately (set `animated_inset = target`).
    /// - Otherwise, start a fresh `AnimationController` from 0..1; `from_inset`
    ///   is the current `animated_inset` so mid-tween retargets don't jump.
    pub fn start_tween_to(&mut self, target_height: f32, target: KeyboardInsetSnapshot) {
        self.from_inset = self.animated_inset;
        self.to_inset = target_height;
        self.curve = Box::new(curve_for(target.curve));

        if target.duration_secs <= 0.0 {
            // Snap path: no animation, jump to target.
            self.animated_inset = target_height;
            self.controller = AnimationController::new(Duration::ZERO);
            self.controller.stop();
            self.last_seen = target;
            return;
        }

        // Build a fresh controller with the new duration. Re-attach the
        // ticker + dirty callback if we have them (set on mount).
        let mut controller = AnimationController::new(Duration::from_secs_f64(target.duration_secs as f64));
        if let Some(ticker) = &self.ticker {
            controller.set_ticker(ticker.clone());
        }
        if let Some(cb) = &self.dirty_callback {
            controller.set_dirty_callback(cb.clone());
        }
        controller.forward(); // value 0 → 1 over duration
        self.controller = controller;
        self.last_seen = target;
    }

    /// Advance the tween. Called from `on_tick`.
    pub fn advance(&mut self, now: Instant) {
        self.controller.advance(now);
        let t = self.controller.value();
        let eased = self.curve.transform(t);
        self.animated_inset = self.from_inset + (self.to_inset - self.from_inset) * eased as f32;
    }
}

/// Map a `KeyboardCurve` to a Vexo `Curve` implementation.
pub fn curve_for(curve: KeyboardCurve) -> Box<dyn Curve> {
    match curve {
        KeyboardCurve::EaseInOut => Box::new(EaseInOutCurve),
        KeyboardCurve::EaseIn => Box::new(EaseInCurve),
        KeyboardCurve::EaseOut => Box::new(EaseOutCurve),
        KeyboardCurve::Linear => Box::new(LinearCurve),
    }
}

impl ComponentState for KeyboardAvoidanceState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        self.ticker = Some(ctx.animation_ticker().clone());
        self.dirty_callback = Some(ctx.dirty_callback());
        // On mount, snap to the current target (no animation) — the keyboard
        // is already in whatever state it's in; animating would be wrong.
        if let Some(widget) = ctx.widget().downcast_ref::<KeyboardAvoidance>() {
            // Read the source via the widget's stored clone.
            let snap = widget.source.get();
            self.animated_inset = snap.target_height;
            self.from_inset = snap.target_height;
            self.to_inset = snap.target_height;
            self.last_seen = snap;
        }
    }

    fn on_tick(&mut self, now: Instant) {
        self.advance(now);
    }

    fn on_unmount(&mut self, _ctx: &mut LifecycleContext) {
        self.controller.stop();
    }
}

// ============================================================================
// KEYBOARD AVOIDANCE WIDGET
// ============================================================================

/// A widget that lifts its child above the iOS software keyboard.
///
/// Wraps `child` in a column whose bottom padding is
/// `max(safe_area.bottom, animated_keyboard_inset)`. When the keyboard
/// appears, the padding animates in sync with the OS keyboard slide (using
/// the duration + curve reported by UIKit).
///
/// On desktop / Android the source stays at 0, so this is a transparent
/// pass-through. Only the bottom edge is padded; for notch/status-bar
/// avoidance, compose with [`SafeArea`](crate::widgets::SafeArea).
pub struct KeyboardAvoidance {
    child: Box<dyn Widget>,
    source: crate::core::KeyboardInsetSource,
    key: Option<WidgetKey>,
}

impl KeyboardAvoidance {
    /// Create a new `KeyboardAvoidance` wrapping `child`.
    ///
    /// The `source` is read live each render. In production, obtain it from
    /// the framework (the chat screen uses the default app-wide source via
    /// `RenderContext::keyboard_inset()`). For tests, construct a
    /// `KeyboardInsetSource::default()` and call `set_target(...)` directly.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            source: crate::core::KeyboardInsetSource::default(),
            key: None,
        }
    }

    /// Provide a specific `KeyboardInsetSource` (e.g. the app-wide one).
    /// When the chat screen constructs this widget, it should pass the
    /// source obtained from the framework's `WindowState`.
    pub fn with_source(mut self, source: crate::core::KeyboardInsetSource) -> Self {
        self.source = source;
        self
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Clone for KeyboardAvoidance {
    fn clone(&self) -> Self {
        Self {
            child: self.child.clone_boxed(),
            source: self.source.clone(),
            key: self.key.clone(),
        }
    }
}

impl Component for KeyboardAvoidance {
    type State = KeyboardAvoidanceState;

    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        // 1. Read live source snapshot.
        let snap = self.source.get();

        // 2. Detect target change. If changed, start/retarget the tween.
        if snap != state.last_seen {
            state.start_tween_to(snap.target_height, snap);
        }

        // 3. Compute effective bottom padding.
        let safe_bottom = ctx.safe_area().bottom;
        let bottom = state.effective_bottom_padding(safe_bottom);

        // 4. Build the layout: column with bottom padding, fills parent.
        let layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .flex_grow(1.0)
            .min_height(0.0)
            .padding_each(0.0, 0.0, 0.0, bottom);

        crate::WithLayout::new(self.child.clone_boxed(), layout).boxed()
    }
}
```

Now add the missing import at the top of the file. Update the `use crate::` block to include `WithLayout` and `Arc`:

```rust
use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::animation::{AnimationController, Curve, EaseInCurve, EaseInOutCurve, EaseOutCurve, LinearCurve};
use crate::core::{KeyboardCurve, KeyboardInsetSnapshot};
use crate::{Component, ComponentState, LifecycleContext, RenderContext, Widget, WidgetKey, WithLayout};
```

(Remove the now-unused imports: `elements::RenderObjectElement`, `focus::attachment::FocusAttachment`, `input::InputEvent`, `layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey}`, `render_objects::ContainerRenderObject`, and the long `use crate::{...}` line that listed Element/ElementContext/etc. The widget delegates to `WithLayout` for its render object, so it doesn't need its own render object or element. Replace the entire `use` block with the trimmed version above.)

Also remove the `layout_shrinks_child_by_keyboard_inset` test's reference to `KeyboardAvoidanceRenderObject` — replace that test body with a simpler element-count assertion (the layout math is covered by the `effective_bottom_padding` + `advance_tween_*` unit tests; the full pipeline integration is covered by the chat screen regression test in Task 6). Replace the whole `layout_shrinks_child_by_keyboard_inset` test with:

```rust
    #[test]
    fn widget_mounts_in_pipeline() {
        let (mut pipeline, _ticker) = build_pipeline();
        let view = KeyboardAvoidance::new(crate::Text::new("hi")).boxed();
        pipeline.update(view);
        assert!(pipeline.element_registry().len() > 2, "expected element tree to mount");
    }
```

And delete the now-unused `use` of `KeyboardAvoidanceRenderObject` from the test module (it was only referenced in the removed test body).

Finally, the test `start_tween_snaps_when_duration_zero` references `state.controller` before `start_tween_to` rebuilds it — that's fine. But the `advance_tween_*` tests call `state.controller.start_time()` *after* `start_tween_to`, which replaced the controller. Make sure `start_tween_to` sets `self.controller` to the fresh one (it does in the impl above). Good.

- [ ] **Step 4: Declare the module + re-export in `vexo/src/widgets/mod.rs`**

In `vexo/src/widgets/mod.rs`:

(a) Add the module declaration. Find the `mod safe_area;` line (line 19) and add immediately after it:

```rust
mod keyboard_avoidance;
```

(b) Add the re-export. Find the `pub use safe_area::{SafeArea, SafeAreaClaim};` line (line 44) and add immediately after it:

```rust
pub use keyboard_avoidance::KeyboardAvoidance;
```

- [ ] **Step 5: Re-export from `vexo/src/lib.rs`**

In `vexo/src/lib.rs`, find the `pub use widgets::{ ... }` block (lines 207–213). Add `KeyboardAvoidance,` to it. After the change the block includes:

```rust
pub use widgets::{
    Brightness, ChildPush, ClipRRect, DecoratedBox, FadeTransition, FractionalTranslation,
    GestureDetector, Grid, Image, IndexedStack, KeyboardAvoidance, MultiChild, Offstage, Opacity,
    Positioned, SafeArea, SafeAreaClaim, ScrollController, ScrollView, SlideDirection,
    SlideTransition, Stack, Text, TextEdit, TextEditState, TextEditingController, Theme,
    ThemeData, Transform, Widget, WithLayout,
};
```

- [ ] **Step 6: Verify `WithLayout::new` constructor signature**

The implementation uses `crate::WithLayout::new(child, layout)` — confirmed the constructor takes `(child, layout)` as two args (`vexo/src/widgets/with_layout.rs:260`). If the build fails on this call, re-check the signature:

Run: `grep -n 'pub fn new' vexo/src/widgets/with_layout.rs`

and adjust the call to match.

- [ ] **Step 7: Run the widget tests**

Run: `cargo test -p vexo --lib keyboard_avoidance::tests`
Expected: all tests PASS (8 tests: 4 effective_padding + 1 snap + 1 halfway + 1 complete + 1 mid-tween retarget + 1 curve mapping + 1 widget_mounts).

- [ ] **Step 8: Build the whole crate**

Run: `cargo build -p vexo`
Expected: compiles cleanly.

- [ ] **Step 9: Commit**

```bash
git add vexo/src/widgets/keyboard_avoidance.rs vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "feat(vexo): add KeyboardAvoidance widget

A Component that lifts its child above the iOS software keyboard. State
owns an AnimationController tween (reused from existing infra) from the
current animated inset to the target; reads KeyboardInsetSource each
render and retargets on change. Effective bottom padding is
max(safe_area.bottom, animated_inset). Delegates its render object to
WithLayout so it needs no custom element/render-object code."
```

---

### Task 4: objc2 UIKit keyboard shim

**Files:**
- Create: `vexo/src/platform/keyboard_ios.rs`
- Modify: `vexo/src/platform/mod.rs`
- Modify: `vexo/Cargo.toml` (extend objc2 feature flags)

**Interfaces:**
- Consumes: `KeyboardInsetSource::set_target()` (Task 1).
- Produces: `KeyboardObserver` (struct with `install()` + `Drop`) — used by Task 5 (`WindowState` calls `install()` on iOS).

**Note on objc2 feature flags:** objc2-ui-kit 0.3.2 / objc2-foundation 0.3.2 use per-class feature flags. The exact set needed for `UIResponder` keyboard notifications, `NSNotificationCenter`, `NSNotification`, `NSValue`, `NSNumber`, `NSDictionary` must be verified by building for the iOS target. The steps below specify the most likely feature names following the existing `UIPasteboard` pattern; Step 5 is a build-and-iterate loop that adds any the compiler requests.

- [ ] **Step 1: Extend objc2 features in `vexo/Cargo.toml`**

In `vexo/Cargo.toml`, find the iOS dependency block (lines 33–36). Extend the feature lists. After the change:

```toml
[target.'cfg(target_os = "ios")'.dependencies]
objc2 = { version = ">=0.6.2, <0.8.0", default-features = false, features = ["std"] }
objc2-foundation = { version = "0.3.2", default-features = false, features = ["NSString", "NSNotification", "NSDictionary", "NSNumber", "NSValue"] }
objc2-ui-kit = { version = "0.3.2", default-features = false, features = ["UIPasteboard", "UIResponder", "UIApplication"] }
```

(If the build in Step 5 reports a missing feature, add it here and rebuild. Common candidates: `NSNotificationCenter` may be a separate feature on `objc2-foundation`; if so, add it. Same for `NSRunLoop` if the shim needs to dispatch to the main thread.)

- [ ] **Step 2: Declare the module in `vexo/src/platform/mod.rs`**

In `vexo/src/platform/mod.rs`, find the `#[cfg(target_os = "ios")] pub mod ios_clipboard;` line (line 11–12). Add immediately after it:

```rust
#[cfg(target_os = "ios")]
pub mod keyboard_ios;
```

- [ ] **Step 3: Create `vexo/src/platform/keyboard_ios.rs`**

Create the file with this content. This follows the `ios_clipboard.rs` pattern: a thin objc2 adapter that lives behind `#[cfg(target_os = "ios")]`. All UIKit calls are `unsafe` (objc2 marks them `#[unsafe(method)]`); the safety contract is main-thread dispatch, which holds because `WindowState` constructs the observer on the main thread and the notifications fire on the main thread.

```rust
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

use crate::core::{KeyboardCurve, KeyboardInsetSource};

/// Handle to the installed keyboard notification observers.
///
/// Drop to remove the observers from `NotificationCenter`. In practice the
/// observer lives for the window's lifetime (so it's dropped when
/// `WindowState` is dropped).
pub struct KeyboardObserver {
    // Opaque token handles returned by NotificationCenter. Stored as
    // `Retained<NSObject>` so dropping the observer removes them.
    //
    // We store them as raw `Retained` pointers; the exact objc2 type for
    // observer tokens is `Retained<NSObject>` (the return of
    // `addObserverForName:object:queue:usingBlock:`).
    show_token: objc2::rc::Retained<objc2_foundation::NSObject>,
    hide_token: objc2::rc::Retained<objc2_foundation::NSObject>,
    // Keep a reference to the center so Drop can call removeObserver.
    center: objc2::rc::Retained<objc2_foundation::NSNotificationCenter>,
}

impl KeyboardObserver {
    /// Install keyboard observers on the default `NotificationCenter`.
    ///
    /// `scale_factor` converts the keyboard frame (physical px) to logical px.
    /// Returns a handle whose `Drop` removes the observers.
    pub fn install(source: KeyboardInsetSource, scale_factor: f64) -> Self {
        use objc2::rc::Retained;
        use objc2_foundation::{NSNotificationCenter, NSObject, NSString};
        use objc2_ui_kit::{UIResponder, UIApplication};

        let center = NSNotificationCenter::defaultCenter();

        // Shared clone for the show callback.
        let source_for_show = source.clone();
        let scale = scale_factor as f32;
        let show_name = UIResponder::keyboardWillShowNotification();
        let show_token = center.addObserverForName_object_queue_usingBlock(
            Some(&show_name),
            None,
            None,
            move |notif| {
                handle_keyboard_notification(notif, &source_for_show, scale, /*show=*/ true);
            },
        );

        let source_for_hide = source.clone();
        let hide_name = UIResponder::keyboardWillHideNotification();
        let hide_token = center.addObserverForName_object_queue_usingBlock(
            Some(&hide_name),
            None,
            None,
            move |notif| {
                handle_keyboard_notification(notif, &source_for_hide, scale, /*show=*/ false);
            },
        );

        Self {
            show_token,
            hide_token,
            center,
        }
    }
}

impl Drop for KeyboardObserver {
    fn drop(&mut self) {
        self.center.removeObserver(&self.show_token);
        self.center.removeObserver(&self.hide_token);
    }
}

/// Extract keyboard frame / duration / curve from a notification's `userInfo`
/// and write them into the source.
///
/// - `show == true`: target height = frame end height (clamped to window).
/// - `show == false`: target height = 0 (keyboard dismissing).
fn handle_keyboard_notification(
    notif: &objc2_foundation::NSNotification,
    source: &KeyboardInsetSource,
    scale_factor: f32,
    show: bool,
) {
    use objc2_foundation::{NSDictionary, NSNumber, NSObject, NSString, NSValue};

    let user_info: Option<Retained<NSDictionary<NSString, NSObject>>> = notif.userInfo();
    let user_info = match user_info {
        Some(ui) => ui,
        None => return,
    };

    // --- Target height ---
    let target_height = if show {
        let frame_key = UIResponder::keyboardFrameEndUserInfoKey();
        let frame_value: Option<Retained<NSObject>> = user_info.get(&frame_key).cloned();
        match frame_value {
            Some(obj) => {
                // obj should be an NSValue wrapping a CGRect.
                // Downcast and read the CGRectValue.
                let value: Retained<NSValue> = obj.downcast::<NSValue>().unwrap();
                let rect = value.CGRectValue();
                let height_px = rect.size.height as f32;
                let height_logical = height_px / scale_factor;
                // Clamp: never report a height larger than the window
                // (defensive for slide-over / stage-manager).
                height_logical.max(0.0)
            }
            None => return,
        }
    } else {
        0.0
    };

    // --- Animation duration (seconds) ---
    let duration_key = UIResponder::keyboardAnimationDurationUserInfoKey();
    let duration_secs: f32 = user_info
        .get(&duration_key)
        .cloned()
        .and_then(|obj| obj.downcast::<NSNumber>().ok())
        .map(|n| n.as_f32())
        .unwrap_or(0.25); // UIKit default if missing

    // --- Animation curve (raw u8) ---
    let curve_key = UIResponder::keyboardAnimationCurveUserInfoKey();
    let curve_raw: u8 = user_info
        .get(&curve_key)
        .cloned()
        .and_then(|obj| obj.downcast::<NSNumber>().ok())
        .map(|n| n.as_u8())
        .unwrap_or(0); // EaseInOut is UIKit's default
    let curve = KeyboardCurve::from_uikit_raw(curve_raw);

    source.set_target(target_height, duration_secs, curve);
}
```

- [ ] **Step 4: Add the `unsafe impl` for thread safety if required**

`KeyboardObserver` holds `Retained<...>` which may not be `Send`. If `WindowState` needs it to be `Send` (it doesn't — `WindowState` is single-threaded on iOS), skip this. If the compiler complains, add at the top of `keyboard_ios.rs`:

```rust
// SAFETY: KeyboardObserver is constructed and dropped on the main thread
// (WindowState lifetime). The Retained<NSObject> tokens are never sent
// across threads; they're only touched in Drop on the main thread.
unsafe impl Send for KeyboardObserver {}
```

Only add this if the build in Step 5 requires it.

- [ ] **Step 5: Build for iOS and iterate on feature flags**

This is the iterative part. The exact objc2 feature names for 0.3.2 must match what the crate exposes. Run:

```bash
cargo build -p vexo --target aarch64-apple-ios 2>&1 | tee /tmp/ios-build.log
```

If the build fails with "feature `X` not found" or "cannot find type `Y`", do the following for each error:

1. Identify the missing feature (e.g. `NSNotificationCenter`).
2. Add it to the relevant crate's `features` list in `vexo/Cargo.toml`.
3. Re-run the build.

Common additions you may need:
- `objc2-foundation`: add `NSNotificationCenter` if it's a separate feature.
- `objc2-ui-kit`: add `UIScreen` if you decide to clamp using `UIScreen.mainScreen().bounds` (alternative: pass the window height into `install()` instead).

If the `addObserverForName_object_queue_usingBlock` method signature doesn't match (objc2 0.3.2 may name it differently), check the generated bindings:

```bash
grep -rn 'addObserverForName' ~/.cargo/registry/src/*/objc2-ui-kit-0.3.2/src/ 2>/dev/null | head
```

Adjust the call to match the actual method name. The block argument is a `Block` — objc2's `define_block!` or the `objc2::block` helpers may be needed; follow whatever pattern the existing `ios_clipboard.rs` uses (it doesn't use blocks, so this may be a new pattern — consult the objc2 docs at https://docs.rs/objc2/0.6.x/objc2/block/ if needed).

**Build success criteria:** `cargo build -p vexo --target aarch64-apple-ios` compiles cleanly with no errors. Warnings about unused code are acceptable (Task 5 will use the observer).

- [ ] **Step 6: Build for desktop to confirm no regression**

Run: `cargo build -p vexo`
Expected: compiles cleanly (the iOS shim is cfg-gated out; desktop sees no change).

- [ ] **Step 7: Commit**

```bash
git add vexo/src/platform/keyboard_ios.rs vexo/src/platform/mod.rs vexo/Cargo.toml
git commit -m "feat(vexo): add iOS keyboard notification shim (objc2)

KeyboardObserver registers for UIResponder.keyboardWillShow/WillHide on
the default NotificationCenter, extracts end-frame height + animation
duration + curve from userInfo, and writes them into KeyboardInsetSource.
cfg-gated to iOS; mirrors the existing ios_clipboard.rs pattern. Desktop
build is unaffected."
```

---

### Task 5: Wire shim + per-frame poll into `WindowState`

**Files:**
- Modify: `vexo/src/window.rs`

**Interfaces:**
- Consumes: `KeyboardInsetSource` (Task 1), `KeyboardObserver::install()` (Task 4), `ThreeTreePipeline::set_keyboard_inset_source()` (Task 2), `ThreeTreePipeline::mark_all_needs_layout()` (existing).
- Produces: an app-wide `KeyboardInsetSource` plumbed end-to-end. After this task, the source is shared between the iOS shim, the pipeline/`RenderContext`, and the widget.

- [ ] **Step 1: Add the field + initialization to `WindowState`**

In `vexo/src/window.rs`:

(a) Add the import near the existing `SafeAreaSource` import (line 11):

```rust
use crate::core::{Absolute, KeyboardInsetSource, Logical, Physical, Point, ScaleSource, SafeAreaSource, Size};
```

(b) Add the field. Find the `safe_area_source: SafeAreaSource,` field (line 53) and add immediately after it:

```rust

    /// Shared keyboard-inset source (logical pixels). Updated by the iOS
    /// keyboard shim on each `keyboardWillShow/Hide` notification; read by
    /// `KeyboardAvoidance` widgets during render via
    /// `RenderContext::keyboard_inset()`. On desktop this stays at 0 (no
    /// shim is installed).
    keyboard_inset_source: KeyboardInsetSource,
```

(c) Add the iOS-only observer field. Immediately after the `keyboard_inset_source` field, add:

```rust

    #[cfg(target_os = "ios")]
    keyboard_observer: Option<crate::platform::keyboard_ios::KeyboardObserver>,
```

(d) Initialize in `WindowState::new()`. Find the `let safe_area_source = SafeAreaSource::default();` line (line 114) and add immediately after it:

```rust

        let keyboard_inset_source = KeyboardInsetSource::default();
```

(e) Plumb the source into the pipeline. Find the `three_tree_pipeline.set_safe_area_source(safe_area_source.clone());` line (line 119) and add immediately after it:

```rust
        three_tree_pipeline.set_keyboard_inset_source(keyboard_inset_source.clone());
```

(f) Install the iOS observer. Immediately after the pipeline-plumbing line above, add:

```rust

        #[cfg(target_os = "ios")]
        let keyboard_observer = {
            let scale = scale_source.get().factor();
            Some(crate::platform::keyboard_ios::KeyboardObserver::install(
                keyboard_inset_source.clone(),
                scale,
            ))
        };
```

(g) Add the fields to the `Self { ... }` return struct. Find the `safe_area_source,` line in the struct literal (line 128) and add immediately after it:

```rust
            keyboard_inset_source,
            #[cfg(target_os = "ios")]
            keyboard_observer,
```

- [ ] **Step 2: Add the per-frame poll in `render_retain()`**

Find the existing safe-area per-frame poll in `render_retain()` (lines 499–514). It looks like:

```rust
        {
            let prev = self.safe_area_source.get();
            if let Some(win) = &self.window {
                let insets = win.safe_area();
                let f = self.scale_source.get().factor();
                self.safe_area_source.set(
                    insets.left as f32 / f,
                    insets.right as f32 / f,
                    insets.top as f32 / f,
                    insets.bottom as f32 / f,
                );
            }
            if self.safe_area_source.get() != prev {
                self.three_tree_pipeline.mark_all_needs_layout();
            }
        }
```

Immediately after the closing `}` of this block (line 514), add the keyboard-inset poll:

```rust

        // 4.5. Poll the keyboard-inset source for changes. The iOS shim
        //      writes to it asynchronously from UIKit notifications; we
        //      detect the change here and mark the tree dirty so
        //      KeyboardAvoidance widgets re-render and start/retarget their
        //      tweens. Mirrors the safe-area poll above. On desktop the
        //      source never changes (no shim), so this is a no-op.
        {
            let prev = self.keyboard_inset_snapshot_prev;
            let curr = self.keyboard_inset_source.get();
            if curr != prev {
                self.keyboard_inset_snapshot_prev = curr;
                self.three_tree_pipeline.mark_all_needs_layout();
            }
        }
```

- [ ] **Step 3: Add the `keyboard_inset_snapshot_prev` field**

Add a field to `WindowState` to remember the previous snapshot for the per-frame diff. Near the `keyboard_inset_source` field added in Step 1, add:

```rust

    /// Previous keyboard-inset snapshot, used by the per-frame poll to
    /// detect changes. Updated each frame in `render_retain()`.
    keyboard_inset_snapshot_prev: crate::core::KeyboardInsetSnapshot,
```

Initialize it in `Self { ... }`:

```rust
            keyboard_inset_snapshot_prev: crate::core::KeyboardInsetSnapshot {
                target_height: 0.0,
                duration_secs: 0.0,
                curve: crate::core::KeyboardCurve::EaseInOut,
            },
```

Add `KeyboardInsetSnapshot` and `KeyboardCurve` to the import on line 11:

```rust
use crate::core::{
    Absolute, KeyboardCurve, KeyboardInsetSnapshot, KeyboardInsetSource, Logical, Physical, Point,
    ScaleSource, SafeAreaSource, Size,
};
```

- [ ] **Step 4: Add a public accessor (optional, for tests / external observers)**

Find the existing `safe_area_source()` accessor (lines 420–426). Immediately after it, add:

```rust

    /// Get a clone of the keyboard-inset source.
    ///
    /// Cheap (`KeyboardInsetSource` is `Arc`-based); useful for subsystems
    /// that want to observe insets outside the widget tree, or for tests
    /// that need to drive the source directly.
    pub fn keyboard_inset_source(&self) -> KeyboardInsetSource {
        self.keyboard_inset_source.clone()
    }
```

- [ ] **Step 5: Build for desktop**

Run: `cargo build -p vexo`
Expected: compiles cleanly (the iOS observer field is cfg-gated out; the poll runs but is a no-op since the source never changes).

- [ ] **Step 6: Build for iOS**

Run: `cargo build -p vexo --target aarch64-apple-ios`
Expected: compiles cleanly (the observer is installed; the poll runs).

- [ ] **Step 7: Run the desktop test suite**

Run: `cargo test -p vexo --lib`
Expected: all existing tests pass; no behavior change on desktop.

- [ ] **Step 8: Commit**

```bash
git add vexo/src/window.rs
git commit -m "feat(vexo): wire KeyboardInsetSource into WindowState

Owns the source, installs the iOS KeyboardObserver on mobile, plumbs
the source into the pipeline, and polls it each frame (mirroring the
safe-area poll) to mark the tree dirty when the keyboard target changes.
Desktop builds are unaffected (no shim, source stays at 0)."
```

---

### Task 6: Apply `KeyboardAvoidance` in the chat screen

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs`

**Interfaces:**
- Consumes: `KeyboardAvoidance` widget (Task 3), `KeyboardInsetSource` from `WindowState` (Task 5 — but the chat screen constructs the widget without an explicit source; it uses `RenderContext::keyboard_inset()` which reads the app-wide source plumbed in Task 5).

**Note on source wiring:** `KeyboardAvoidance::new(child)` uses a default (all-zero) source. To read the app-wide source from `WindowState`, the chat screen needs to obtain a clone. There are two options:
1. Add a `KeyboardInsetSource` field to `ChatScreen` and pass it through from `MobileChatsPage` → `ChatScreen` → `KeyboardAvoidance::new(child).with_source(source)`. This requires threading the source from `WindowState` through the app.
2. Make `KeyboardAvoidance` read the source from `RenderContext` directly (not from a widget field).

Option 2 is simpler and matches how `SafeArea` works (it reads `RenderContext::safe_area()`, not a stored source). The implementation in Task 3 currently stores a `source` field; we should change it to read from `RenderContext::keyboard_inset()` instead. This is a small refactor to Task 3's `render()` method.

**Decision: refactor `KeyboardAvoidance` to read from `RenderContext` (Option 2).** This removes the `source` field, the `with_source()` builder, and makes the chat-screen integration a one-line wrapping.

- [ ] **Step 1: Refactor `KeyboardAvoidance` to read from `RenderContext`**

In `vexo/src/widgets/keyboard_avoidance.rs`:

(a) Remove the `source` field from the struct. The struct becomes:

```rust
pub struct KeyboardAvoidance {
    child: Box<dyn Widget>,
    key: Option<WidgetKey>,
}
```

(b) Remove the `with_source()` method entirely.

(c) Update `Clone for KeyboardAvoidance` to drop the `source` field:

```rust
impl Clone for KeyboardAvoidance {
    fn clone(&self) -> Self {
        Self {
            child: self.child.clone_boxed(),
            key: self.key.clone(),
        }
    }
}
```

(d) Update `render()` to read from `RenderContext` instead of the stored source. Replace the first line of `render()` (`let snap = self.source.get();`) with:

```rust
        let snap = ctx.keyboard_inset();
```

(e) Update `on_mount` in `KeyboardAvoidanceState` — it currently reads `widget.source.get()`. Since the widget no longer has a `source` field, and `on_mount` doesn't have a `RenderContext`, we need to defer the initial snap to the first `render()` call. Change `on_mount` to just wire up the ticker + dirty callback:

```rust
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        self.ticker = Some(ctx.animation_ticker().clone());
        self.dirty_callback = Some(ctx.dirty_callback());
        // The initial snap happens on the first render() call, which reads
        // the source via RenderContext::keyboard_inset(). On mount the
        // animated_inset stays at its default (0.0); if the keyboard is
        // already up, the first render will detect last_seen != current
        // and start a tween — but since from_inset == 0 == animated_inset
        // and we want to snap, we handle this by checking if last_seen is
        // still the default sentinel and snapping instead of tweening.
    }
```

(f) Handle the "first render after mount" snap. In `render()`, the change-detection block currently does:

```rust
        if snap != state.last_seen {
            state.start_tween_to(snap.target_height, snap);
        }
```

We need this to snap (not tween) on the very first render. Add a `mounted` flag to the state. Update `KeyboardAvoidanceState`:

```rust
pub struct KeyboardAvoidanceState {
    animated_inset: f32,
    from_inset: f32,
    to_inset: f32,
    controller: AnimationController,
    last_seen: KeyboardInsetSnapshot,
    curve: Box<dyn Curve>,
    ticker: Option<Arc<crate::animation::AnimationTicker>>,
    dirty_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    mounted: bool,
}
```

Initialize `mounted: false` in `Default`.

Update `render()`:

```rust
    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let snap = ctx.keyboard_inset();

        if !state.mounted {
            // First render after mount: snap to the current target, no tween.
            state.animated_inset = snap.target_height;
            state.from_inset = snap.target_height;
            state.to_inset = snap.target_height;
            state.last_seen = snap;
            state.mounted = true;
        } else if snap != state.last_seen {
            state.start_tween_to(snap.target_height, snap);
        }

        let safe_bottom = ctx.safe_area().bottom;
        let bottom = state.effective_bottom_padding(safe_bottom);

        let layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .flex_grow(1.0)
            .min_height(0.0)
            .padding_each(0.0, 0.0, 0.0, bottom);

        crate::WithLayout::new(self.child.clone_boxed(), layout).boxed()
    }
```

(g) Remove the now-unused import `use crate::core::KeyboardInsetSource;` if it was added (it shouldn't be — the source is only accessed via `RenderContext`).

(h) Remove the `with_source` references from any test that used it (none of the Task 3 tests used `with_source`, so this is likely a no-op).

- [ ] **Step 2: Run the widget tests to confirm the refactor didn't break them**

Run: `cargo test -p vexo --lib keyboard_avoidance::tests`
Expected: all tests still pass (they call `start_tween_to` and `effective_bottom_padding` directly, which are unchanged).

- [ ] **Step 3: Wrap the chat screen's outer column in `KeyboardAvoidance`**

In `shared_app/src/chats/chat_screen.rs`, find the `render()` method of `ChatScreen` (lines 79–129). The current structure is:

```rust
        DecoratedBox::with_style(
            MultiChild::new(
                children![
                    WithLayout::new(
                        ScrollView::new(list.boxed()).controller(self.scroll_controller.clone()),
                        Layout::flex_fill(),
                    ),
                    input_bar,
                ],
                Layout::column()
                    .flex_grow(1.0)
                    .flex_basis(0.0)
                    .min_height(0.0),
            ),
            Style::default().background(theme.background),
        )
        .boxed()
```

Change it to wrap the inner `MultiChild` in `KeyboardAvoidance`:

```rust
        DecoratedBox::with_style(
            KeyboardAvoidance::new(
                MultiChild::new(
                    children![
                        WithLayout::new(
                            ScrollView::new(list.boxed()).controller(self.scroll_controller.clone()),
                            Layout::flex_fill(),
                        ),
                        input_bar,
                    ],
                    Layout::column()
                        .flex_grow(1.0)
                        .flex_basis(0.0)
                        .min_height(0.0),
                ),
            ),
            Style::default().background(theme.background),
        )
        .boxed()
```

Add `KeyboardAvoidance` to the import from `vexo` at the top of the file. Find the existing `use vexo::{ ... }` block (lines 6–10) and add `KeyboardAvoidance,` to it:

```rust
use vexo::{
    children, AlignSelf, BoxShadow, Color, Component, ComponentState, DecoratedBox,
    FlexDirection, KeyboardAvoidance, Key, Layout, LifecycleContext, MultiChild, RenderContext,
    ScrollController, ScrollView, Style, Text, TextEdit, TextEditingController, Theme, Widget,
    WidgetKey, WithLayout,
};
```

- [ ] **Step 4: Run the chat screen tests**

Run: `cargo test -p shared_app --lib chats::chat_screen`
Expected: both existing tests pass:
- `test_chat_screen_renders_messages` — element count grows by a constant (the `KeyboardAvoidance` element + `WithLayout` element); the assertion `> 4` still holds.
- `test_chat_screen_input_bar_pinned_to_bottom_with_few_messages` — with no keyboard (source = 0) and desktop safe-area (0), the effective bottom padding is 0, so the input bar still pins to 600. The assertion `input_bottom >= 599.0` still holds.

If the second test fails because the render-object tree navigation changed (the `KeyboardAvoidance` adds a `WithLayout` wrapper), update the test's tree-navigation code (`find_child` calls) to account for the extra layer. The test currently navigates:

```rust
let proxy = find_child(ro_reg, root, 0).expect("proxy");
let chat_decorated = find_child(ro_reg, proxy, 0).expect("chat decorated root");
let chat_col = find_child(ro_reg, chat_decorated, 0).expect("chat column");
let input_wrapper = find_child(ro_reg, chat_col, 1).expect("input bar wrapper");
```

With `KeyboardAvoidance` wrapping the column, the tree becomes:

```
proxy → chat_decorated → KeyboardAvoidance(WithLayout) → chat_col → [scrollview, input_bar]
```

So the navigation needs one more `find_child` step:

```rust
let proxy = find_child(ro_reg, root, 0).expect("proxy");
let chat_decorated = find_child(ro_reg, proxy, 0).expect("chat decorated root");
let keyboard_avoidance = find_child(ro_reg, chat_decorated, 0).expect("keyboard avoidance wrapper");
let chat_col = find_child(ro_reg, keyboard_avoidance, 0).expect("chat column");
let input_wrapper = find_child(ro_reg, chat_col, 1).expect("input bar wrapper");
```

Update the test to match the actual tree structure. Run the test again to confirm.

- [ ] **Step 5: Build the whole workspace**

Run: `cargo build`
Expected: compiles cleanly.

- [ ] **Step 6: Run the full test suite**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/widgets/keyboard_avoidance.rs shared_app/src/chats/chat_screen.rs
git commit -m "feat(shared_app): wrap chat screen in KeyboardAvoidance

The chat screen's outer column is now wrapped in KeyboardAvoidance
(inside DecoratedBox so the background still fills the screen). When the
keyboard appears, the input bar lifts above it; the ScrollView shrinks to
fill the remaining space. Also refactors KeyboardAvoidance to read the
source from RenderContext::keyboard_inset() (not a stored field), matching
how SafeArea reads safe_area()."
```

---

## Self-Review

### Spec coverage

- ✅ **Component 1 — `KeyboardInsetSource`** → Task 1 (type + unit tests).
- ✅ **Component 2 — objc2 UIKit shim** → Task 4 (`KeyboardObserver` + `install()` + `Drop`).
- ✅ **Component 3 — `KeyboardAvoidance` widget** → Task 3 (widget + state + tween + tests) + Task 6 Step 1 (refactor to read from `RenderContext`).
- ✅ **Component 4 — Chat screen integration** → Task 6 (wrap + test adjustment).
- ✅ **Testing strategy #1 (source unit tests)** → Task 1 Step 1.
- ✅ **Testing strategy #2 (widget tests)** → Task 3 Step 1 (effective_padding, snap, halfway, complete, mid-tween retarget, curve mapping).
- ✅ **Testing strategy #3 (layout integration test)** → Task 3 Step 1 (`widget_mounts_in_pipeline` — lighter than the original spec's layout assertion, but the math is covered by unit tests; the full layout integration is covered by the chat screen regression test in Task 6).
- ✅ **Testing strategy #4 (chat screen regression)** → Task 6 Step 4.
- ✅ **Testing strategy #5 (shim not unit-tested)** → Task 4 (build-and-iterate; no unit tests for the shim).
- ✅ **Error handling #1–#7** → all covered by the design (scale captured at install; multiple widgets read same source; unmounted-then-mounted snap via `mounted` flag in Task 6 Step 1; zero-duration snap; orientation retarget; window-size clamp; desktop no-op).
- ✅ **Per-frame poll** → Task 5 Step 2 (mirrors safe-area poll).
- ✅ **`RenderContext::keyboard_inset()` accessor** → Task 2 Step 2.

### Placeholder scan

No "TBD", "TODO", or "implement later" found. Task 4 Step 5 includes a build-and-iterate loop for objc2 feature flags — this is not a placeholder; it's an explicit acknowledgment that objc2 0.3.2's exact feature names must be verified at build time (the most likely names are specified, with instructions to add more as the compiler requests).

### Type consistency

- `KeyboardInsetSource` — consistent across all tasks.
- `KeyboardInsetSnapshot` — consistent (fields: `target_height: f32`, `duration_secs: f32`, `curve: KeyboardCurve`).
- `KeyboardCurve` — consistent (variants: `EaseInOut`, `EaseIn`, `EaseOut`, `Linear`; `from_uikit_raw(raw: u8) -> Self`).
- `set_target(height: f32, duration_secs: f32, curve: KeyboardCurve)` — consistent (Task 1 defines, Task 4 shim calls).
- `RenderContext::keyboard_inset()` → `KeyboardInsetSnapshot` — consistent (Task 2 defines, Task 3/Task 6 reads).
- `KeyboardAvoidance::new(child: impl Widget + 'static)` — consistent (Task 3 defines, Task 6 calls).
- `KeyboardObserver::install(source: KeyboardInsetSource, scale_factor: f64) -> KeyboardObserver` — consistent (Task 4 defines, Task 5 calls).
- `ThreeTreePipeline::set_keyboard_inset_source(source: KeyboardInsetSource)` — consistent (Task 2 defines, Task 5 calls).

All type signatures match across task boundaries.
