# MediaQuery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Flutter's `MediaQuery` in Vexo — an `InheritedWidget` carrying `MediaQueryData` (size, devicePixelRatio, padding, viewInsets, viewPadding, platformBrightness, orientation) — and replace the ad-hoc `BottomBarHeight` InheritedWidget, the layouter safe-area claim walk, the `SafeAreaClaim` widget, and the `KeyboardAvoidance` widget with the Flutter model.

**Architecture:** A framework auto-injected root `MediaQuery` (a `Component` reading live platform sources from `BuildOwner`) wraps `Application::view()`. Descendants read `MediaQuery::of(ctx)`. `SafeArea` becomes a stateless `Component` reading `MediaQuery.padding` and providing `MediaQuery::remove_padding` to its child. `TabBarView` uses `MediaQuery::reduce_view_insets_bottom` to fold the tab-bar height into the page's `viewInsets.bottom`. The iOS keyboard shim is reworked to drive `CADisplayLink` and report continuous keyboard heights each frame, so `MediaQuery.viewInsets.bottom` is animated by the OS — eliminating the per-widget `KeyboardAvoidance` tween entirely.

**Tech Stack:** Rust, wgpu, Taffy, glyphon, objc2/objc2-foundation (iOS shim), uniffi (iOS export).

**Spec:** `docs/superpowers/specs/2026-07-26-media-query-design.md`

## Global Constraints

- **No comments added to code** unless explicitly shown in a step's code block. The codebase is comment-rich already; preserve existing comments, do not add new ones unless the plan shows them.
- **Workspace dependency versions** are pinned in the root `Cargo.toml`; do not bump any versions.
- **Build commands:** `cargo build -p vexo` (framework only), `cargo build` (whole workspace), `cargo test -p vexo` (framework tests), `cargo test` (whole workspace tests). Always run after edits; never assume tests pass.
- **Never run `cargo run -p desktop_demo`** — it opens a GUI; ask the user.
- **File paths in this plan are absolute** to `/Users/peiyan_wang/Workspace/ui_platform/...`.
- **`Cargo.toml`** changes (if any) must preserve `workspace = true` references.
- **Commit messages** must NOT contain "Co-Authored-By: Claude" or similar attribution strings.
- **iOS-only files** (`vexo/src/platform/keyboard_ios.rs`, anything under `shared_app/src/` that is iOS-targeted) compile only on `target_os = "ios"`; non-iOS builds must still pass after edits.
- **Existing tests must keep passing** at every step. When a step removes tests for deleted code, the step's "Run tests" sub-step must still pass (no regressions in surviving tests).

---

## File Structure

**Create:**
- `vexo/src/widgets/media_query.rs` — `MediaQueryData`, `Orientation`, `RemoveEdges`, `MediaQuery` InheritedWidget, `RootMediaQuery` Component (framework-internal).

**Modify:**
- `vexo/src/widgets/mod.rs` — register `media_query` module; update `pub use` exports.
- `vexo/src/lib.rs` — update `pub use widgets::{...}` exports.
- `vexo/src/core/geometry.rs` — add `MediaQueryDataSource`; simplify `KeyboardInsetSource` to `current_height: f32` (delete `KeyboardInsetSnapshot`, `KeyboardCurve`).
- `vexo/src/core/mod.rs` — re-export `MediaQueryDataSource`.
- `vexo/src/build_owner.rs` — add `media_query_data_source` field + accessors; keep `safe_area_source`/`keyboard_inset_source`.
- `vexo/src/stateful_widget.rs` — remove `RenderContext::safe_area()` and `RenderContext::keyboard_inset()`; add `RenderContext::media_query_sources()`.
- `vexo/src/window.rs` — add `media_query_data_source` field; write to it each frame; mark tree dirty on size/scale/brightness change.
- `vexo/src/pipeline.rs` — add `set_media_query_data_source()`; remove `set_safe_area_source`/`set_keyboard_inset_source` (BuildOwner is accessed directly via the field — wait, no: these are called by WindowState at init; keep them, plus add the new one).
- `vexo/src/widgets/safe_area.rs` — rewrite: delete `SafeAreaRenderObject`, `SafeAreaElement`, `SafeAreaClaimRenderObject`, `SafeAreaClaimElement`, `SafeAreaClaim`; make `SafeArea` a `Component`.
- `vexo/src/widgets/keyboard_avoidance.rs` — delete `BottomBarHeight`, `KeyboardAvoidance`, `KeyboardAvoidanceState`, all curve modules; delete the file entirely (moved/deleted).
- `vexo/src/widgets/mod.rs` — remove `keyboard_avoidance` mod + its `pub use`.
- `vexo/src/render_object.rs` — delete `SafeAreaClaimEdges`, `safe_area_claim()`, `set_effective_safe_area()`, `effective_safe_area()` from `RenderObject` trait + `LayoutContext`; remove `safe_area_source` field from `LayoutContext`.
- `vexo/src/layouter.rs` — delete the top-down safe-area claim pre-pass (`resolve_effective_safe_area`); remove `safe_area_source` parameter from `Layouter::layout`.
- `vexo/src/pipeline.rs` — remove `safe_area_source` plumbing through `Layouter::layout` (it's no longer needed).
- `vexo/src/platform/keyboard_ios.rs` — rework: `CADisplayLink`-driven continuous-height reporter writing `current_height: f32` to `KeyboardInsetSource`.
- `vexo_uikit/src/tab_bar.rs` — use `MediaQuery::reduce_view_insets_bottom` + `MediaQuery::remove_padding`; remove `BottomBarHeight` + `SafeAreaClaim::bottom` usage.
- `vexo_uikit/src/navigation.rs` — `ctx.safe_area()` → `MediaQuery::of(ctx).padding`.
- `shared_app/src/chats/chat_screen.rs` — remove `KeyboardAvoidance::new(...)` wrapper; read `MediaQuery::of(ctx).viewInsets.bottom` directly as bottom padding.
- `CLAUDE.md` — update API mapping table.

---

## Task 1: Add `MediaQueryData` + `Orientation` + `RemoveEdges` data types

**Files:**
- Create: `vexo/src/widgets/media_query.rs`
- Modify: `vexo/src/widgets/mod.rs`

**Interfaces:**
- Produces: `MediaQueryData` (struct with public fields), `MediaQueryData::all_zero()`, `MediaQueryData::copy_with_padding()`, `MediaQueryData::copy_with_view_insets()`, `MediaQueryData::copy_with_view_padding()`, `Orientation` enum, `RemoveEdges` struct with `NONE`/`TOP`/`BOTTOM`/`ALL` constants.

- [ ] **Step 1: Write the failing test**

Create `vexo/src/widgets/media_query.rs` with only the test for now. The file must compile (so types need to exist), but tests must fail because `MediaQueryData` isn't defined yet — actually, since this is a fresh file, we'll write the test and the minimal impl in the same step. Use a different approach: write the test in a `#[cfg(test)] mod tests` at the bottom, write the data types at the top, run tests to verify they pass (this is a TDD-adapted flow for new-file tasks where "failing test" doesn't apply since the file would not compile at all otherwise).

Write the full file:

```rust
//! `MediaQueryData`, `Orientation`, `RemoveEdges` — the data model for
//! `MediaQuery`. See `docs/superpowers/specs/2026-07-26-media-query-design.md`.

use crate::core::{Logical, Size};
use crate::layout::EdgeInsets;
use crate::widgets::Brightness;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// Per-side flag for `MediaQuery::remove_padding` / `remove_view_insets` /
/// `remove_view_padding`. Replaces the deleted `SafeAreaClaimEdges`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RemoveEdges {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl RemoveEdges {
    pub const NONE: Self = Self { top: false, right: false, bottom: false, left: false };
    pub const TOP: Self = Self { top: true, right: false, bottom: false, left: false };
    pub const BOTTOM: Self = Self { top: false, right: false, bottom: true, left: false };
    pub const ALL: Self = Self { top: true, right: true, bottom: true, left: true };
}

#[derive(Clone, PartialEq, Debug)]
pub struct MediaQueryData {
    pub size: Size<Logical>,
    pub device_pixel_ratio: f32,
    pub padding: EdgeInsets,
    pub viewInsets: EdgeInsets,
    pub viewPadding: EdgeInsets,
    pub platform_brightness: Brightness,
    pub orientation: Orientation,
}

impl MediaQueryData {
    pub const fn all_zero() -> Self {
        Self {
            size: Size::new(0.0, 0.0),
            device_pixel_ratio: 1.0,
            padding: EdgeInsets::ZERO,
            viewInsets: EdgeInsets::ZERO,
            viewPadding: EdgeInsets::ZERO,
            platform_brightness: Brightness::Light,
            orientation: Orientation::Portrait,
        }
    }

    pub fn copy_with_padding(&self, padding: EdgeInsets) -> Self {
        let mut clone = self.clone();
        clone.padding = padding;
        clone
    }

    pub fn copy_with_view_insets(&self, viewInsets: EdgeInsets) -> Self {
        let mut clone = self.clone();
        clone.viewInsets = viewInsets;
        clone
    }

    pub fn copy_with_view_padding(&self, viewPadding: EdgeInsets) -> Self {
        let mut clone = self.clone();
        clone.viewPadding = viewPadding;
        clone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_zero_defaults() {
        let z = MediaQueryData::all_zero();
        assert_eq!(z.size, Size::<Logical>::new(0.0, 0.0));
        assert_eq!(z.device_pixel_ratio, 1.0);
        assert_eq!(z.padding, EdgeInsets::ZERO);
        assert_eq!(z.viewInsets, EdgeInsets::ZERO);
        assert_eq!(z.viewPadding, EdgeInsets::ZERO);
        assert_eq!(z.platform_brightness, Brightness::Light);
        assert_eq!(z.orientation, Orientation::Portrait);
    }

    #[test]
    fn copy_with_padding_is_immutable() {
        let z = MediaQueryData::all_zero();
        let new_padding = EdgeInsets { left: 10.0, right: 20.0, top: 30.0, bottom: 40.0 };
        let updated = z.copy_with_padding(new_padding);
        assert_eq!(updated.padding, new_padding);
        assert_eq!(z.padding, EdgeInsets::ZERO, "original must be unchanged");
    }

    #[test]
    fn copy_with_view_insets_is_immutable() {
        let z = MediaQueryData::all_zero();
        let new_vi = EdgeInsets { left: 0.0, right: 0.0, top: 0.0, bottom: 300.0 };
        let updated = z.copy_with_view_insets(new_vi);
        assert_eq!(updated.viewInsets, new_vi);
        assert_eq!(z.viewInsets, EdgeInsets::ZERO, "original must be unchanged");
    }

    #[test]
    fn copy_with_view_padding_is_immutable() {
        let z = MediaQueryData::all_zero();
        let new_vp = EdgeInsets { left: 1.0, right: 2.0, top: 3.0, bottom: 4.0 };
        let updated = z.copy_with_view_padding(new_vp);
        assert_eq!(updated.viewPadding, new_vp);
        assert_eq!(z.viewPadding, EdgeInsets::ZERO, "original must be unchanged");
    }

    #[test]
    fn remove_edges_constants() {
        assert_eq!(RemoveEdges::NONE, RemoveEdges { top: false, right: false, bottom: false, left: false });
        assert_eq!(RemoveEdges::TOP, RemoveEdges { top: true, right: false, bottom: false, left: false });
        assert_eq!(RemoveEdges::BOTTOM, RemoveEdges { top: false, right: false, bottom: true, left: false });
        assert_eq!(RemoveEdges::ALL, RemoveEdges { top: true, right: true, bottom: true, left: true });
    }
}
```

- [ ] **Step 2: Register the module**

Edit `vexo/src/widgets/mod.rs`. Find the line `mod theme;` and add `mod media_query;` after it. Find the `pub use theme::{Brightness, Theme, ThemeData};` line and add after it:

```rust
pub use media_query::{MediaQueryData, Orientation, RemoveEdges};
```

- [ ] **Step 3: Verify `EdgeInsets::ZERO` exists**

Run a quick check: `rg "pub const ZERO" vexo/src/layout/`. If `EdgeInsets::ZERO` does not exist, add it to the `EdgeInsets` definition. Find the `EdgeInsets` struct (likely in `vexo/src/layout/mod.rs` or similar) and add:

```rust
pub const ZERO: Self = Self { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 };
```

If `EdgeInsets` already has `Default` but not `ZERO`, add `ZERO` as shown.

- [ ] **Step 4: Run tests**

Run: `cargo test -p vexo --lib widgets::media_query`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/media_query.rs vexo/src/widgets/mod.rs
# Also add the EdgeInsets::ZERO edit if you made one:
# git add vexo/src/layout/<file>.rs
git commit -m "feat(vexo): add MediaQueryData, Orientation, RemoveEdges data types"
```

---

## Task 2: Add `MediaQuery` InheritedWidget + subtree mutators

**Files:**
- Modify: `vexo/src/widgets/media_query.rs`
- Modify: `vexo/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `MediaQueryData`, `RemoveEdges`, `RenderContext` (`depend_on_inherited_widget`), `Widget`, `InheritedWidget`, `impl_widget_for_inherited!`, `WidgetKey`.
- Produces: `MediaQuery` (InheritedWidget), `MediaQuery::new()`, `MediaQuery::with_key()`, `MediaQuery::of(ctx) -> MediaQueryData`, `MediaQuery::remove_padding(child, edges) -> MediaQuery`, `MediaQuery::remove_view_insets(child, edges) -> MediaQuery`, `MediaQuery::remove_view_padding(child, edges) -> MediaQuery`, `MediaQuery::reduce_view_insets_bottom(child, amount) -> MediaQuery`.

- [ ] **Step 1: Add the `MediaQuery` widget + `MediaQueryMutator` Component to the file**

Edit `vexo/src/widgets/media_query.rs`. After the `MediaQueryData` impl block, add:

```rust
use crate::inherited_widget::{impl_widget_for_inherited, InheritedWidget};
use crate::key::WidgetKey;
use crate::stateful_widget::RenderContext;
use crate::widgets::Widget;
use crate::{Component, ComponentState};

pub struct MediaQuery {
    data: MediaQueryData,
    child: Box<dyn Widget>,
    key: Option<WidgetKey>,
}

impl MediaQuery {
    pub fn new(data: MediaQueryData, child: impl Widget + 'static) -> Self {
        Self { data, child: Box::new(child), key: None }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn of(ctx: &mut RenderContext) -> MediaQueryData {
        ctx.depend_on_inherited_widget::<MediaQueryData>()
            .unwrap_or_else(MediaQueryData::all_zero)
    }

    pub fn remove_padding(child: impl Widget + 'static, edges: RemoveEdges) -> MediaQueryMutator {
        MediaQueryMutator::new(Box::new(child), move |parent: &MediaQueryData| {
            let mut p = parent.padding;
            if edges.top { p.top = 0.0; }
            if edges.right { p.right = 0.0; }
            if edges.bottom { p.bottom = 0.0; }
            if edges.left { p.left = 0.0; }
            parent.copy_with_padding(p)
        })
    }

    pub fn remove_view_insets(child: impl Widget + 'static, edges: RemoveEdges) -> MediaQueryMutator {
        MediaQueryMutator::new(Box::new(child), move |parent: &MediaQueryData| {
            let mut v = parent.viewInsets;
            if edges.top { v.top = 0.0; }
            if edges.right { v.right = 0.0; }
            if edges.bottom { v.bottom = 0.0; }
            if edges.left { v.left = 0.0; }
            parent.copy_with_view_insets(v)
        })
    }

    pub fn remove_view_padding(child: impl Widget + 'static, edges: RemoveEdges) -> MediaQueryMutator {
        MediaQueryMutator::new(Box::new(child), move |parent: &MediaQueryData| {
            let mut v = parent.viewPadding;
            if edges.top { v.top = 0.0; }
            if edges.right { v.right = 0.0; }
            if edges.bottom { v.bottom = 0.0; }
            if edges.left { v.left = 0.0; }
            parent.copy_with_view_padding(v)
        })
    }

    pub fn reduce_view_insets_bottom(child: impl Widget + 'static, amount: f32) -> MediaQueryMutator {
        MediaQueryMutator::new(Box::new(child), move |parent: &MediaQueryData| {
            let mut v = parent.viewInsets;
            v.bottom = (v.bottom - amount).max(0.0);
            parent.copy_with_view_insets(v)
        })
    }
}

impl Clone for MediaQuery {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            child: self.child.clone_boxed(),
            key: self.key.clone(),
        }
    }
}

impl InheritedWidget for MediaQuery {
    type Value = MediaQueryData;
    fn value(&self) -> &MediaQueryData { &self.data }
    fn child(&self) -> &dyn Widget { self.child.as_ref() }
    fn key(&self) -> Option<WidgetKey> { self.key.clone() }
}

impl_widget_for_inherited!(MediaQuery);

/// `Component` that reads the parent `MediaQuery` at render time, applies
/// a pure transformation to produce a child `MediaQueryData`, and emits
/// `MediaQuery::new(transformed, child)`.
///
/// Used by `MediaQuery::remove_padding` / `remove_view_insets` /
/// `remove_view_padding` / `reduce_view_insets_bottom`. The closure is
/// stored in an `Rc<dyn Fn>` so the mutator itself is cheaply cloneable
/// (closures don't auto-impl `Clone`).
///
/// The widget tree is single-threaded (main thread), so `Rc` is fine. If
/// the `Component` trait requires `Send + Sync` (verify in Step 2), switch
/// to `Arc<dyn Fn + Send + Sync>` and require the closure to be
/// `Send + Sync + 'static`.
pub struct MediaQueryMutator {
    child: Box<dyn Widget>,
    compute: std::rc::Rc<dyn Fn(&MediaQueryData) -> MediaQueryData>,
}

impl MediaQueryMutator {
    fn new(
        child: Box<dyn Widget>,
        compute: impl Fn(&MediaQueryData) -> MediaQueryData + 'static,
    ) -> Self {
        Self { child, compute: std::rc::Rc::new(compute) }
    }
}

impl Clone for MediaQueryMutator {
    fn clone(&self) -> Self {
        Self {
            child: self.child.clone_boxed(),
            compute: self.compute.clone(),
        }
    }
}

impl Component for MediaQueryMutator {
    type State = ();
    fn render(&self, _state: &mut (), ctx: &mut RenderContext) -> Box<dyn Widget> {
        let parent = MediaQuery::of(ctx);
        let data = (self.compute)(&parent);
        MediaQuery::new(data, self.child.clone_boxed()).boxed()
    }
}
```

**Design rationale:** The four subtree-mutator methods can't return a `MediaQuery` directly because the parent's `MediaQueryData` isn't known until render time. They return a `MediaQueryMutator` `Component` instead. At render, the mutator reads the parent via `MediaQuery::of(ctx)`, applies the field edits, and emits `MediaQuery::new(edited_data, child)`. This matches Flutter's `MediaQuery.removePadding` returning a `MediaQuery` widget that copyWith's its parent.

- [ ] **Step 2: Check `Component` trait bounds**

Run: `rg "pub trait Component" vexo/src/stateful_widget.rs -A 5` to see the trait bounds. If `Component: 'static` only (no `Send`/`Sync`), `Rc<dyn Fn>` works. If `Send + Sync` is required, use `Arc<dyn Fn + Send + Sync>` instead and update `MediaQueryMutator::new`'s `compute` parameter to `impl Fn(...) + Send + Sync + 'static`.

- [ ] **Step 3: Update `widgets/mod.rs` export**

Edit `vexo/src/widgets/mod.rs`. Update the `pub use media_query::{...}` line to also export `MediaQuery` and `MediaQueryMutator`:

```rust
pub use media_query::{MediaQuery, MediaQueryData, MediaQueryMutator, Orientation, RemoveEdges};
```

- [ ] **Step 4: Run tests**

Run: `cargo build -p vexo`
Expected: compiles cleanly.

Run: `cargo test -p vexo --lib widgets::media_query`
Expected: 5 tests PASS (unchanged from Task 1).

- [ ] **Step 5: Add a unit test for `MediaQueryMutator` rendering**

Add to the `#[cfg(test)] mod tests` block at the bottom of `media_query.rs`:

```rust
    #[test]
    fn reduce_view_insets_bottom_clamps_to_zero() {
        // Verify the closure logic directly (no pipeline needed).
        let parent = MediaQueryData::all_zero().copy_with_view_insets(
            EdgeInsets { left: 0.0, right: 0.0, top: 0.0, bottom: 300.0 },
        );
        let compute = |p: &MediaQueryData| {
            let mut v = p.viewInsets;
            v.bottom = (v.bottom - 49.0).max(0.0);
            p.copy_with_view_insets(v)
        };
        let child = compute(&parent);
        assert_eq!(child.viewInsets.bottom, 251.0);

        // Clamp test: subtract more than available.
        let compute2 = |p: &MediaQueryData| {
            let mut v = p.viewInsets;
            v.bottom = (v.bottom - 500.0).max(0.0);
            p.copy_with_view_insets(v)
        };
        let clamped = compute2(&parent);
        assert_eq!(clamped.viewInsets.bottom, 0.0);
    }
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p vexo --lib widgets::media_query`
Expected: 6 tests PASS.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/widgets/media_query.rs vexo/src/widgets/mod.rs
git commit -m "feat(vexo): add MediaQuery InheritedWidget + subtree mutators"
```

---

## Task 3: Add `MediaQueryDataSource` platform cell

**Files:**
- Modify: `vexo/src/core/geometry.rs`
- Modify: `vexo/src/core/mod.rs`

**Interfaces:**
- Consumes: `Size<Logical>`, atomics, `Arc`.
- Produces: `MediaQueryDataSource`, `MediaQueryDataSource::new()`, `MediaQueryDataSource::set(size, dpr, is_dark)`, `MediaQueryDataSource::get() -> MediaQueryDataSourceSnapshot`, `MediaQueryDataSourceSnapshot { size, device_pixel_ratio, is_dark }`.

- [ ] **Step 1: Locate the insertion point**

In `vexo/src/core/geometry.rs`, find the end of the `KeyboardInsetSource` impl block (search for `impl Default for KeyboardInsetSource`). Add the new source immediately after.

- [ ] **Step 2: Add the new source**

Edit `vexo/src/core/geometry.rs`. After the `impl Default for KeyboardInsetSource` block (around line 880), add:

```rust
// ============================================================================
// MEDIA QUERY DATA SOURCE
// ============================================================================

/// Snapshot of the platform-derived parts of `MediaQueryData` that have
/// no existing source. Read by the root `MediaQuery` component when
/// composing `MediaQueryData` each render.
///
/// Uses `bool` for brightness (not `Brightness`) so this core cell has no
/// dependency on `widgets/theme.rs`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaQueryDataSourceSnapshot {
    pub size: crate::core::Size<crate::core::Logical>,
    pub device_pixel_ratio: f32,
    pub is_dark: bool,
}

/// Shared atomic cell holding the platform-derived parts of `MediaQueryData`
/// that have no existing source. Updated by `WindowState` each frame; read by
/// the root `MediaQuery` component.
///
/// `padding` / `viewInsets` / `viewPadding` stay on the existing
/// `SafeAreaSource` / `KeyboardInsetSource` cells (they already propagate
/// correctly); this cell carries only the new fields.
#[derive(Clone)]
pub struct MediaQueryDataSource {
    inner: Arc<MediaQueryDataInner>,
}

struct MediaQueryDataInner {
    size_w: AtomicU32,
    size_h: AtomicU32,
    device_pixel_ratio: AtomicU32,
    is_dark: AtomicBool,
}

impl MediaQueryDataSource {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MediaQueryDataInner {
                size_w: AtomicU32::new(0.0_f32.to_bits()),
                size_h: AtomicU32::new(0.0_f32.to_bits()),
                device_pixel_ratio: AtomicU32::new(1.0_f32.to_bits()),
                is_dark: AtomicBool::new(false),
            }),
        }
    }

    pub fn set(&self, size: crate::core::Size<crate::core::Logical>, device_pixel_ratio: f32, is_dark: bool) {
        self.inner.size_w.store(size.width.to_bits(), Ordering::Relaxed);
        self.inner.size_h.store(size.height.to_bits(), Ordering::Relaxed);
        self.inner.device_pixel_ratio.store(device_pixel_ratio.to_bits(), Ordering::Relaxed);
        self.inner.is_dark.store(is_dark, Ordering::Relaxed);
    }

    pub fn get(&self) -> MediaQueryDataSourceSnapshot {
        MediaQueryDataSourceSnapshot {
            size: crate::core::Size::new(
                f32::from_bits(self.inner.size_w.load(Ordering::Relaxed)),
                f32::from_bits(self.inner.size_h.load(Ordering::Relaxed)),
            ),
            device_pixel_ratio: f32::from_bits(self.inner.device_pixel_ratio.load(Ordering::Relaxed)),
            is_dark: self.inner.is_dark.load(Ordering::Relaxed),
        }
    }
}

impl Default for MediaQueryDataSource {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 3: Re-export from `core`**

Check `vexo/src/core/mod.rs` for where `SafeAreaSource` / `KeyboardInsetSource` are re-exported (likely `pub use geometry::{...}`). Add `MediaQueryDataSource` and `MediaQueryDataSourceSnapshot` to that re-export list.

Run: `rg "pub use geometry" vexo/src/core/mod.rs` to find the line, then edit it to include the new types.

- [ ] **Step 4: Add unit tests**

Append to the existing test module in `vexo/src/core/geometry.rs` (search for `mod keyboard_inset_source_tests` and add a sibling module after it):

```rust
#[cfg(test)]
mod media_query_data_source_tests {
    use super::*;
    use crate::core::{Logical, Size};

    #[test]
    fn default_is_all_zero() {
        let src = MediaQueryDataSource::new();
        let snap = src.get();
        assert_eq!(snap.size, Size::<Logical>::new(0.0, 0.0));
        assert_eq!(snap.device_pixel_ratio, 1.0);
        assert!(!snap.is_dark);
    }

    #[test]
    fn set_updates_values() {
        let src = MediaQueryDataSource::new();
        src.set(Size::new(400.0, 800.0), 2.0, true);
        let snap = src.get();
        assert_eq!(snap.size, Size::<Logical>::new(400.0, 800.0));
        assert_eq!(snap.device_pixel_ratio, 2.0);
        assert!(snap.is_dark);
    }

    #[test]
    fn clones_share_state() {
        let src = MediaQueryDataSource::new();
        let clone = src.clone();
        src.set(Size::new(100.0, 200.0), 3.0, false);
        let snap = clone.get();
        assert_eq!(snap.size, Size::<Logical>::new(100.0, 200.0));
        assert_eq!(snap.device_pixel_ratio, 3.0);
        assert!(!snap.is_dark);
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p vexo --lib core::geometry::media_query_data_source_tests`
Expected: 3 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/core/geometry.rs vexo/src/core/mod.rs
git commit -m "feat(vexo): add MediaQueryDataSource platform cell"
```

---

## Task 4: Wire `MediaQueryDataSource` into `BuildOwner` + `Pipeline`

**Files:**
- Modify: `vexo/src/build_owner.rs`
- Modify: `vexo/src/pipeline.rs`

**Interfaces:**
- Consumes: `MediaQueryDataSource` (from Task 3).
- Produces: `BuildOwner::media_query_data_source()` getter, `BuildOwner::set_media_query_data_source()` setter, `Pipeline::set_media_query_data_source()` plumbing method.

- [ ] **Step 1: Add the field + accessors to `BuildOwner`**

Edit `vexo/src/build_owner.rs`. Find the `keyboard_inset_source: KeyboardInsetSource,` field in the `BuildOwner` struct (around line 103) and add after it:

```rust
    /// Platform-derived fields for `MediaQueryData` (size, scale, brightness).
    /// Backed by atomics inside [`MediaQueryDataSource`]. Updated each frame
    /// by [`WindowState`](crate::window::WindowState); read by the root
    /// `MediaQuery` component via `RenderContext::media_query_sources()`.
    media_query_data_source: crate::core::MediaQueryDataSource,
```

Find the `keyboard_inset_source: KeyboardInsetSource::default(),` line in `BuildOwner::new()` (around line 117) and add after it:

```rust
            media_query_data_source: crate::core::MediaQueryDataSource::default(),
```

Find the `pub fn set_keyboard_inset_source(...)` method (around line 315) and add after its closing brace:

```rust
    /// Get a clone of the shared media-query data source.
    ///
    /// Returns a cheaply-clonable handle ([`MediaQueryDataSource`] is
    /// `Arc`-based) whose `get()` always reads the latest values written by
    /// [`WindowState`](crate::window::WindowState). Used by the root
    /// `MediaQuery` component via
    /// [`RenderContext::media_query_sources()`](crate::stateful_widget::RenderContext::media_query_sources).
    pub fn media_query_data_source(&self) -> crate::core::MediaQueryDataSource {
        self.media_query_data_source.clone()
    }

    /// Replace the media-query data source.
    ///
    /// Called once at window init so the [`BuildOwner`] shares the same
    /// atomics as [`WindowState`](crate::window::WindowState); subsequent
    /// per-frame updates happen via [`MediaQueryDataSource::set()`] on
    /// either clone.
    pub fn set_media_query_data_source(&mut self, source: crate::core::MediaQueryDataSource) {
        self.media_query_data_source = source;
    }
```

- [ ] **Step 2: Add the plumbing method to `Pipeline`**

Edit `vexo/src/pipeline.rs`. Find `pub fn set_keyboard_inset_source(...)` (around line 212) and add after its closing brace:

```rust
    /// Install the media-query data source on the [`BuildOwner`].
    ///
    /// Called once at window init by
    /// [`WindowState`](crate::window::WindowState) so the same atomics are
    /// shared between the window (which writes size/scale/brightness each
    /// frame) and the element tree (which reads them via
    /// [`RenderContext::media_query_sources()`](crate::stateful_widget::RenderContext::media_query_sources)).
    pub fn set_media_query_data_source(&mut self, source: crate::core::MediaQueryDataSource) {
        self.build_owner.set_media_query_data_source(source);
    }
```

- [ ] **Step 3: Build and run existing tests**

Run: `cargo build -p vexo`
Expected: compiles.

Run: `cargo test -p vexo`
Expected: all existing tests PASS (no behavior change yet; we only added a field).

- [ ] **Step 4: Commit**

```bash
git add vexo/src/build_owner.rs vexo/src/pipeline.rs
git commit -m "feat(vexo): wire MediaQueryDataSource into BuildOwner + Pipeline"
```

---

## Task 5: Add `RenderContext::media_query_sources()` and `RootMediaQuery`

**Files:**
- Modify: `vexo/src/stateful_widget.rs`
- Modify: `vexo/src/widgets/media_query.rs`
- Modify: `vexo/src/widgets/mod.rs`

**Interfaces:**
- Consumes: `BuildOwner::safe_area_source()`, `BuildOwner::keyboard_inset_source()`, `BuildOwner::media_query_data_source()`, `MediaQueryDataSourceSnapshot`, `SafeAreaSource::get()`, `KeyboardInsetSource::get()`.
- Produces: `RenderContext::media_query_sources() -> MediaQuerySourcesSnapshot`, `MediaQuerySourcesSnapshot { safe_area, keyboard_current_height, media_query }`, `RootMediaQuery` Component (crate-internal).

- [ ] **Step 1: Add `MediaQuerySourcesSnapshot` and the accessor**

Edit `vexo/src/stateful_widget.rs`. Find the `RenderContext` impl block (search for `impl<'a> RenderContext<'a>`). Add a new struct near the top of the file (after the `RenderContext` struct definition, or in a logical place near other snapshot types). Actually, place it just before `impl<'a> RenderContext<'a>`:

```rust
/// Snapshot of all three platform sources, read by the root `MediaQuery`
/// component. Intended for the root only; all other widgets read
/// `MediaQuery::of(ctx)`.
pub struct MediaQuerySourcesSnapshot {
    pub safe_area: crate::layout::EdgeInsets,
    pub keyboard_current_height: f32,
    pub media_query: crate::core::MediaQueryDataSourceSnapshot,
}
```

Inside `impl<'a> RenderContext<'a>`, after the existing `keyboard_inset()` method (which we'll delete in a later task; for now leave it), add:

```rust
    /// Snapshot of all three platform sources. Intended for the root
    /// `MediaQuery` component only; all other widgets read `MediaQuery::of`.
    pub fn media_query_sources(&self) -> MediaQuerySourcesSnapshot {
        MediaQuerySourcesSnapshot {
            safe_area: self.build_owner.safe_area_source().get(),
            keyboard_current_height: self.build_owner.keyboard_inset_source().current_target_height(),
            media_query: self.build_owner.media_query_data_source().get(),
        }
    }
```

**Note:** `keyboard_inset_source().current_target_height()` returns `f32` from the existing `KeyboardInsetSource`. (We'll simplify `KeyboardInsetSource` to just `current_height: f32` in Task 8, at which point this becomes `keyboard_inset_source().get()`.)

- [ ] **Step 2: Add `RootMediaQuery` to `media_query.rs`**

Edit `vexo/src/widgets/media_query.rs`. At the end of the file (before the `#[cfg(test)] mod tests` block), add:

```rust
use crate::core::Logical;
use crate::layout::EdgeInsets;
use crate::widgets::theme::Brightness;

/// Framework-internal root `Component` that composes `MediaQueryData` from
/// the three platform sources and provides it to the application subtree
/// via `MediaQuery::new(data, child)`. App authors never touch this — the
/// framework wraps `Application::view()` output in `RootMediaQuery` before
/// mounting.
pub(crate) struct RootMediaQuery {
    child: Box<dyn Widget>,
}

impl RootMediaQuery {
    pub(crate) fn new(child: Box<dyn Widget>) -> Self {
        Self { child }
    }
}

impl Clone for RootMediaQuery {
    fn clone(&self) -> Self {
        Self { child: self.child.clone_boxed() }
    }
}

impl Component for RootMediaQuery {
    type State = ();

    fn render(&self, _state: &mut (), ctx: &mut RenderContext) -> Box<dyn Widget> {
        let sources = ctx.media_query_sources();
        let viewPadding = sources.safe_area;
        let viewInsets = EdgeInsets {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: sources.keyboard_current_height,
        };
        let padding = EdgeInsets {
            top: (viewPadding.top - viewInsets.top).max(0.0),
            bottom: (viewPadding.bottom - viewInsets.bottom).max(0.0),
            left: (viewPadding.left - viewInsets.left).max(0.0),
            right: (viewPadding.right - viewInsets.right).max(0.0),
        };
        let orientation = if sources.media_query.size.width
            >= sources.media_query.size.height
        {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        };
        let brightness = if sources.media_query.is_dark {
            Brightness::Dark
        } else {
            Brightness::Light
        };
        let data = MediaQueryData {
            size: sources.media_query.size,
            device_pixel_ratio: sources.media_query.device_pixel_ratio,
            padding,
            viewInsets,
            viewPadding,
            platform_brightness: brightness,
            orientation,
        };
        MediaQuery::new(data, self.child.clone_boxed()).boxed()
    }
}
```

- [ ] **Step 3: Make `RootMediaQuery` accessible within the crate**

Edit `vexo/src/widgets/mod.rs`. Find the `pub use media_query::{...}` line and ensure `RootMediaQuery` is NOT in it (it's `pub(crate)`). But the crate needs to access it from `lib.rs` / `window.rs`, so add a `pub(crate) use`:

After `pub use media_query::{...}`, add:

```rust
pub(crate) use media_query::RootMediaQuery;
```

- [ ] **Step 4: Build**

Run: `cargo build -p vexo`
Expected: compiles. (No callers of `RootMediaQuery` yet — that's Task 6.)

- [ ] **Step 5: Run tests**

Run: `cargo test -p vexo --lib widgets::media_query`
Expected: 6 tests PASS (unchanged).

- [ ] **Step 6: Commit**

```bash
git add vexo/src/stateful_widget.rs vexo/src/widgets/media_query.rs vexo/src/widgets/mod.rs
git commit -m "feat(vexo): add RenderContext::media_query_sources + RootMediaQuery"
```

---

## Task 6: Auto-inject `RootMediaQuery` at root mount + write `MediaQueryDataSource` each frame

**Files:**
- Modify: `vexo/src/lib.rs`
- Modify: `vexo/src/window.rs`

**Interfaces:**
- Consumes: `RootMediaQuery::new(child)`, `MediaQueryDataSource::set(size, dpr, is_dark)`, `pipeline.set_media_query_data_source(...)`.
- Produces: `RootComponent::render()` wraps `A::view(state)` in `RootMediaQuery`; `WindowState` writes size/scale/brightness each frame and marks tree dirty on change.

- [ ] **Step 1: Wrap the app view in `RootMediaQuery`**

Edit `vexo/src/lib.rs`. Find the `impl<A: Application> Component for RootComponent<A>` block (around line 265). Change `render` to wrap `A::view(state)` in `RootMediaQuery`:

```rust
impl<A: Application> Component for RootComponent<A> {
    type State = A::State;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        let app_view = A::view(state);
        crate::widgets::RootMediaQuery::new(app_view).boxed()
    }
}
```

Add the import at the top of `lib.rs` if not already present (search for existing `use` of `widgets::...`). If `RootMediaQuery` is exported as `pub(crate)` from `widgets/mod.rs`, this path works.

- [ ] **Step 2: Add `media_query_data_source` field to `WindowState`**

Edit `vexo/src/window.rs`. Find the `keyboard_inset_source: KeyboardInsetSource,` field (around line 63) and add after it:

```rust
    /// Shared media-query data source (size, scale, brightness). Updated
    /// each frame from `Window` metrics; read by the root `MediaQuery`
    /// component via `RenderContext::media_query_sources()`.
    media_query_data_source: crate::core::MediaQueryDataSource,
```

- [ ] **Step 3: Initialize the source in `WindowState::new`**

Find the `let keyboard_inset_source = KeyboardInsetSource::default();` line (around line 133) and add after it:

```rust
        let media_query_data_source = crate::core::MediaQueryDataSource::default();
```

Find the `three_tree_pipeline.set_keyboard_inset_source(keyboard_inset_source.clone());` line (around line 139) and add after it:

```rust
        three_tree_pipeline.set_media_query_data_source(media_query_data_source.clone());
```

Find the `keyboard_inset_source,` line in the struct initializer (around line 177) and add after it:

```rust
            media_query_data_source,
```

- [ ] **Step 4: Write size/scale/brightness each frame**

Find the safe-area refresh block at lines ~600-615 (the block starting with `// 4. Refresh safe-area insets`). After that block's closing brace (line ~615), add a new block:

```rust
        // 4.1. Refresh media-query data source (size, scale, brightness).
        //      Read live from the window each frame; mark the tree dirty
        //      when any value changes so the root MediaQuery re-renders.
        {
            let prev = self.media_query_data_source.get();
            if let Some(win) = &self.window {
                let scale = self.scale_source.get().factor();
                let physical_w = self.backend.width() as f32;
                let physical_h = self.backend.height() as f32;
                let logical_w = physical_w / scale;
                let logical_h = physical_h / scale;
                let is_dark = win.theme().unwrap_or(winit::window::Theme::Light)
                    == winit::window::Theme::Dark;
                self.media_query_data_source.set(
                    crate::core::Size::<crate::core::Logical>::new(logical_w, logical_h),
                    scale,
                    is_dark,
                );
            }
            if self.media_query_data_source.get() != prev {
                if let Some(root_id) = self.three_tree_pipeline.element_registry().root() {
                    self.three_tree_pipeline.mark_needs_build(root_id);
                }
                self.three_tree_pipeline.mark_all_needs_layout();
                self.request_frame();
            }
        }
```

**Note:** `win.theme()` is `winit::Window::theme()` returning `Option<winit::window::Theme>`. If `winit::window::Theme` is not in scope, add `use winit::window::Theme as WinitTheme;` at the top of the function or use the full path. Verify by running `cargo build -p vexo` and reading any errors.

**Fallback for non-winit backends:** if `self.window` doesn't have a `.theme()` method (e.g., on iOS where `self.window` is `Option<Arc<dyn Window>>` from a custom trait), check the `Window` trait's methods. Run `rg "fn theme" vexo/src/` to see what's available. If no theme method exists on the trait, default `is_dark = false` for now and leave a TODO comment in the spec's follow-up (do NOT add a TODO to the code per the no-comments rule — instead, just default `is_dark = false`).

- [ ] **Step 5: Build and run tests**

Run: `cargo build -p vexo`
Expected: compiles.

Run: `cargo test -p vexo`
Expected: all existing tests PASS.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/lib.rs vexo/src/window.rs
git commit -m "feat(vexo): auto-inject RootMediaQuery + write MediaQueryDataSource each frame"
```

---

## Task 7: Migrate `SafeArea` to a render-time `Component`

**Files:**
- Modify: `vexo/src/widgets/safe_area.rs` (rewrite)
- Modify: `vexo/src/widgets/mod.rs`
- Modify: `vexo_uikit/src/navigation.rs`

**Interfaces:**
- Consumes: `MediaQuery::of(ctx)`, `MediaQuery::remove_padding(child, edges)`, `Component`, `ComponentState`, `RenderContext`, `Widget`, `WithLayout`, `Layout`, `FlexDirection`, `AlignItems`, `EdgeInsets`, `WidgetKey`.
- Produces: `SafeArea` as a `Component` with the same public builder API (`new`, `top`, `right`, `bottom`, `left`, `minimum`, `with_key`).

- [ ] **Step 1: Rewrite `vexo/src/widgets/safe_area.rs`**

Replace the entire file contents with:

```rust
//! SafeArea widget — insets its child away from the device's unsafe regions.
//!
//! On mobile (iOS) the OS reports per-edge safe-area insets covering the
//! status bar / notch / home indicator. `SafeArea` reads those insets live
//! during render (via [`MediaQuery::of`] → `padding`) and pads its child
//! so content stays within the safe region. On desktop the insets are always
//! zero, so `SafeArea` is a transparent pass-through.
//!
//! This mirrors Flutter's `SafeArea` widget: opt out per side, enforce a
//! `minimum` inset floor, and provide a `MediaQuery` with the consumed
//! edges' `padding` zeroed so descendant `SafeArea`s don't double-consume.
//!
//! # Design notes
//!
//! Insets are resolved at *render* time (Flutter's model), not layout time.
//! [`WindowState`](crate::window::WindowState) writes the live insets into a
//! shared [`SafeAreaSource`](crate::core::SafeAreaSource) each frame; when
//! they change it marks the tree dirty so the root `MediaQuery` re-renders
//! and `SafeArea` (which depends on `MediaQueryData`) rebuilds with the new
//! `padding`.

use crate::core::{Logical, Size};
use crate::inherited_widget::InheritedWidget;
use crate::key::WidgetKey;
use crate::layout::{AlignItems, EdgeInsets, FlexDirection, Layout};
use crate::stateful_widget::{Component, ComponentState, RenderContext};
use crate::widgets::{MediaQuery, RemoveEdges, Widget, WithLayout};

/// A widget that insets its child by the device's safe-area insets.
///
/// On mobile this keeps content clear of the status bar, notch, and home
/// indicator. On desktop the insets are zero, so this is a transparent
/// pass-through. Per-side opt-out is supported via the builder methods, and a
/// `minimum` floor can be enforced.
///
/// # Example
///
/// ```ignore
/// use vexo::{SafeArea, Text};
///
/// SafeArea::new(Text::new("Hello"))
///
/// SafeArea::new(Text::new("Hello")).bottom(false).left(false).right(false)
/// ```
pub struct SafeArea {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    top: bool,
    right: bool,
    bottom: bool,
    left: bool,
    minimum: EdgeInsets,
}

impl SafeArea {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            top: true,
            right: true,
            bottom: true,
            left: true,
            minimum: EdgeInsets::default(),
        }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn top(mut self, enabled: bool) -> Self {
        self.top = enabled;
        self
    }

    pub fn right(mut self, enabled: bool) -> Self {
        self.right = enabled;
        self
    }

    pub fn bottom(mut self, enabled: bool) -> Self {
        self.bottom = enabled;
        self
    }

    pub fn left(mut self, enabled: bool) -> Self {
        self.left = enabled;
        self
    }

    pub fn minimum(mut self, minimum: EdgeInsets) -> Self {
        self.minimum = minimum;
        self
    }

    fn effective_padding(&self, insets: EdgeInsets) -> (f32, f32, f32, f32) {
        let left = if self.left { insets.left.max(self.minimum.left) } else { 0.0 };
        let right = if self.right { insets.right.max(self.minimum.right) } else { 0.0 };
        let top = if self.top { insets.top.max(self.minimum.top) } else { 0.0 };
        let bottom = if self.bottom { insets.bottom.max(self.minimum.bottom) } else { 0.0 };
        (left, right, top, bottom)
    }
}

impl Clone for SafeArea {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            top: self.top,
            right: self.right,
            bottom: self.bottom,
            left: self.left,
            minimum: self.minimum,
        }
    }
}

impl Component for SafeArea {
    type State = ();

    fn render(&self, _state: &mut (), ctx: &mut RenderContext) -> Box<dyn Widget> {
        let mq = MediaQuery::of(ctx);
        let insets = mq.padding;

        let (left, right, top, bottom) = self.effective_padding(insets);
        let layout = Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .flex_grow(1.0)
            .min_height(0.0)
            .padding_each(left, right, top, bottom);

        let inner = MediaQuery::remove_padding(
            WithLayout::new(self.child.clone_boxed(), layout),
            RemoveEdges { top: self.top, right: self.right, bottom: self.bottom, left: self.left },
        );
        inner.boxed()
    }
}
```

**Note:** This deletes `SafeAreaRenderObject`, `SafeAreaElement`, `SafeAreaClaimRenderObject`, `SafeAreaClaimElement`, `SafeAreaClaim`, and all their tests in one rewrite. The tests are ported to a new test module at the bottom (next step).

- [ ] **Step 2: Add the test module to `safe_area.rs`**

Append to the rewritten `safe_area.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[test]
    fn defaults_all_sides_enabled() {
        let w = SafeArea::new(Text::new("Hi"));
        assert!(w.top && w.right && w.bottom && w.left);
        assert_eq!(w.minimum, EdgeInsets::default());
    }

    #[test]
    fn per_side_opt_out() {
        let w = SafeArea::new(Text::new("Hi"))
            .top(false)
            .bottom(false)
            .left(false)
            .right(false);
        assert!(!w.top && !w.right && !w.bottom && !w.left);
    }

    #[test]
    fn minimum_setter() {
        let m = EdgeInsets { left: 5.0, right: 5.0, top: 10.0, bottom: 10.0 };
        let w = SafeArea::new(Text::new("Hi")).minimum(m);
        assert_eq!(w.minimum, m);
    }

    #[test]
    fn clone_preserves_config() {
        let w = SafeArea::new(Text::new("Hi"))
            .top(false)
            .minimum(EdgeInsets { left: 1.0, right: 2.0, top: 3.0, bottom: 4.0 });
        let cloned = w.clone();
        assert_eq!(cloned.top, false);
        assert_eq!(cloned.minimum, w.minimum);
        assert!(cloned.child().is_some());
    }

    #[test]
    fn effective_padding_all_sides() {
        let w = SafeArea::new(Text::new("Hi"));
        let insets = EdgeInsets { left: 10.0, right: 20.0, top: 30.0, bottom: 40.0 };
        assert_eq!(w.effective_padding(insets), (10.0, 20.0, 30.0, 40.0));
    }

    #[test]
    fn effective_padding_opt_out() {
        let w = SafeArea::new(Text::new("Hi")).top(false).left(false);
        let insets = EdgeInsets { left: 10.0, right: 20.0, top: 30.0, bottom: 40.0 };
        assert_eq!(w.effective_padding(insets), (0.0, 20.0, 0.0, 40.0));
    }

    #[test]
    fn effective_padding_minimum_floor() {
        let min = EdgeInsets { left: 50.0, right: 50.0, top: 50.0, bottom: 50.0 };
        let w = SafeArea::new(Text::new("Hi")).minimum(min);
        let insets = EdgeInsets { left: 10.0, right: 20.0, top: 30.0, bottom: 40.0 };
        assert_eq!(w.effective_padding(insets), (50.0, 50.0, 50.0, 50.0));
    }

    #[test]
    fn effective_padding_no_floor_when_larger() {
        let min = EdgeInsets { left: 5.0, right: 5.0, top: 5.0, bottom: 5.0 };
        let w = SafeArea::new(Text::new("Hi")).minimum(min);
        let insets = EdgeInsets { left: 10.0, right: 20.0, top: 30.0, bottom: 40.0 };
        assert_eq!(w.effective_padding(insets), (10.0, 20.0, 30.0, 40.0));
    }
}
```

- [ ] **Step 3: Update `widgets/mod.rs` exports**

Edit `vexo/src/widgets/mod.rs`. Find `pub use safe_area::{SafeArea, SafeAreaClaim};` and change it to:

```rust
pub use safe_area::SafeArea;
```

(`SafeAreaClaim` is gone.)

- [ ] **Step 4: Migrate `navigation.rs`**

Edit `vexo_uikit/src/navigation.rs`. Find line 584 (`let safe_insets = ctx.safe_area();`) and replace with:

```rust
        let safe_insets = vexo::MediaQuery::of(ctx).padding;
```

If `vexo::MediaQuery` is not imported, add it to the `use vexo::{...}` block at the top of the file.

- [ ] **Step 5: Build (expect errors in `tab_bar.rs` and other places that reference `SafeAreaClaim`)**

Run: `cargo build -p vexo`
Expected: compiles (the framework crate doesn't reference `SafeAreaClaim` in non-test code now).

Run: `cargo build`
Expected: ERRORS in `vexo_uikit/src/tab_bar.rs` (uses `SafeAreaClaim::bottom` and `BottomBarHeight`). These are fixed in Task 9. For now, build just `-p vexo`.

- [ ] **Step 6: Run framework tests**

Run: `cargo test -p vexo`
Expected: PASS. (The pipeline-level `SafeAreaClaim` tests that referenced the layouter claim walk are deleted with the rewrite; the new tests pass.)

- [ ] **Step 7: Commit**

```bash
git add vexo/src/widgets/safe_area.rs vexo/src/widgets/mod.rs vexo_uikit/src/navigation.rs
git commit -m "refactor(vexo): migrate SafeArea to render-time Component reading MediaQuery.padding"
```

---

## Task 8: Delete the layouter safe-area claim walk + `SafeAreaClaimEdges` + `LayoutContext::safe_area_source`

**Files:**
- Modify: `vexo/src/layouter.rs`
- Modify: `vexo/src/render_object.rs`
- Modify: `vexo/src/pipeline.rs`
- Modify: `vexo/src/widgets/mod.rs` (if it re-exports `SafeAreaClaimEdges`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `Layouter::layout()` no longer takes `safe_area_source`; `RenderObject` trait no longer has `safe_area_claim` / `set_effective_safe_area` / `effective_safe_area`; `LayoutContext` no longer has `safe_area_source` field or `set_safe_area_source`/`safe_area_source` methods; `SafeAreaClaimEdges` deleted.

- [ ] **Step 1: Remove the claim walk from `Layouter`**

Edit `vexo/src/layouter.rs`. Change the `pub fn layout(...)` signature: remove the `safe_area_source: SafeAreaSource,` parameter. The signature becomes:

```rust
    pub fn layout(
        render_objects: &mut RenderObjectRegistry,
        dirty: &mut DirtyTracking,
        available_size: Size<Logical>,
        engine: &mut dyn LayoutEngine,
        font_system: &mut glyphon::FontSystem,
    ) {
```

Delete the entire Phase 0 block (the `{ let global_insets = safe_area_source.get(); Self::resolve_effective_safe_area(render_objects, root_id, global_insets); }` block, currently lines 68-85).

Delete the `let mut ctx = LayoutContext::new(engine, font_system); ctx.set_safe_area_source(safe_area_source.clone());` lines (currently ~92-93) and replace with just `let mut ctx = LayoutContext::new(engine, font_system);`.

Delete the second `ctx.set_safe_area_source(safe_area_source);` line (currently ~109).

Delete the entire `fn resolve_effective_safe_area(...)` function (currently lines 162-196).

Update the `use` statement at the top: remove `SafeAreaSource` from `use crate::core::{Logical, SafeAreaSource, Size};` → `use crate::core::{Logical, Size};`. Remove `use crate::layout::{EdgeInsets, ...}` if `EdgeInsets` is no longer used (it was used by `resolve_effective_safe_area`).

- [ ] **Step 2: Remove `safe_area_source` from `LayoutContext`**

Edit `vexo/src/render_object.rs`. In `LayoutContext` struct, delete the `safe_area_source: crate::core::SafeAreaSource,` field. In `LayoutContext::new()`, delete the `safe_area_source: crate::core::SafeAreaSource::default(),` initializer line. Delete the `set_safe_area_source` and `safe_area_source` methods.

- [ ] **Step 3: Remove `safe_area_claim` / `set_effective_safe_area` / `effective_safe_area` from the `RenderObject` trait**

Edit `vexo/src/render_object.rs`. Search for `safe_area_claim`, `set_effective_safe_area`, `effective_safe_area` in the `RenderObject` trait. Delete those methods (and their default impls if present).

Search for `SafeAreaClaimEdges` struct definition in `render_object.rs` (or wherever it's defined — run `rg "pub struct SafeAreaClaimEdges" vexo/src/`). Delete the struct + its impl blocks (`remove_from`, constants `BOTTOM`/`TOP`/`ALL`/`NONE`).

- [ ] **Step 4: Update `pipeline.rs`**

Edit `vexo/src/pipeline.rs`. Find where `Layouter::layout(...)` is called (search for `Layouter::layout(`). Remove the `self.build_owner.safe_area_source(),` argument from that call (it's the last argument, currently around line 400).

Find `pub fn set_safe_area_source(...)` (around line 201). Keep this method — `WindowState` still calls it at init to share the atomics with `BuildOwner` (the root `MediaQuery` reads `safe_area_source` via `BuildOwner`). Do NOT delete it.

- [ ] **Step 5: Update `widgets/mod.rs` if needed**

Run: `rg "SafeAreaClaimEdges" vexo/src/`. If any references remain, remove them. Likely `widgets/mod.rs` doesn't export it; if `lib.rs` does, remove from there too (handled in Task 11).

- [ ] **Step 6: Build and test**

Run: `cargo build -p vexo`
Expected: compiles. If errors reference `set_effective_safe_area` / `safe_area_claim` in render object impls (e.g., other render objects implementing the trait), remove those impls (they're now dead methods).

Run: `cargo test -p vexo`
Expected: PASS. The deleted `SafeAreaClaim` pipeline tests are gone; the surviving tests pass.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/layouter.rs vexo/src/render_object.rs vexo/src/pipeline.rs vexo/src/widgets/mod.rs
git commit -m "refactor(vexo): delete layouter safe-area claim walk + SafeAreaClaimEdges + LayoutContext::safe_area_source"
```

---

## Task 9: Migrate `TabBarView` to `MediaQuery::reduce_view_insets_bottom` + `MediaQuery::remove_padding`

**Files:**
- Modify: `vexo_uikit/src/tab_bar.rs`

**Interfaces:**
- Consumes: `MediaQuery::of(ctx)`, `MediaQuery::reduce_view_insets_bottom(child, amount)`, `MediaQuery::remove_padding(child, edges)`, `RemoveEdges::BOTTOM`.
- Produces: `TabBarView` no longer references `BottomBarHeight` or `SafeAreaClaim`.

- [ ] **Step 1: Update the imports**

Edit `vexo_uikit/src/tab_bar.rs`. Find the `use vexo::{...}` block (lines 15-19). Remove `BottomBarHeight` and `SafeAreaClaim` from the import list. Add `MediaQuery, RemoveEdges`.

The new import line:

```rust
use vexo::{
    children, Component, ComponentState, DecoratedBox, GestureDetector, IndexedStack, Layout,
    LifecycleContext, MediaQuery, MultiChild, RemoveEdges, RenderContext, SafeArea, Style, Theme,
    Widget, WithLayout,
};
```

- [ ] **Step 2: Rewrite `TabBarView::render`'s body**

Edit `vexo_uikit/src/tab_bar.rs`. Find the `impl<D: Hash + Eq + Clone + 'static + Any> Component for TabBarView<D>` block's `render` method (around line 156). Replace the body from the `// Build all pages` comment down to the closing `}` of `render` with:

```rust
    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let current_index = self
            .tabs
            .iter()
            .position(|t| *t == self.controller.current())
            .unwrap_or(0);

        let mut stack = IndexedStack::new(current_index);
        for tab in &self.tabs {
            stack = stack.push((self.page_builder)(tab));
        }

        let nav = tokens::navigation::colors(&Theme::of(ctx));
        let mut bar = MultiChild::empty(Layout::default().width_percent(1.0).height(49.0));
        for tab in &self.tabs {
            let is_selected = *tab == self.controller.current();
            let ctrl = self.controller.clone();
            let tab_clone = tab.clone();
            let content = (self.tab_bar_builder)(tab, is_selected);
            let item = GestureDetector::new(content)
                .on_press(move || ctrl.switch_to(tab_clone.clone()))
                .with_layout(
                    Layout::default()
                        .flex_direction(FlexDirection::Column)
                        .align(AlignItems::Stretch)
                        .flex_grow(1.0)
                        .justify(JustifyContent::Center),
                )
                .boxed();
            bar = bar.push(item);
        }

        let bar = DecoratedBox::with_style(
            SafeArea::new(bar.boxed()).top(false).boxed(),
            Style::default().background(nav.mobile_header_bg),
        );
        let bar = WithLayout::new(bar, Layout::default().flex_grow(0.0).flex_shrink(0.0));

        let hairline = DecoratedBox::with_style(
            MultiChild::empty(
                Layout::row()
                    .height(tokens::navigation::HAIRLINE_THICKNESS)
                    .flex_shrink(0.0),
            ),
            Style::default().background(nav.divider),
        );
        let bar = MultiChild::new(children![hairline, bar], Layout::column().flex_shrink(0.0));

        let mq = MediaQuery::of(ctx);
        let tab_bar_height = TAB_BAR_HEIGHT + mq.padding.bottom;

        let page = MediaQuery::reduce_view_insets_bottom(stack, tab_bar_height);
        let page = MediaQuery::remove_padding(page, RemoveEdges::BOTTOM);
        let content = MultiChild::new(
            children![
                WithLayout::new(page, Layout::flex_fill()),
                bar,
            ],
            Layout::default()
                .flex_direction(FlexDirection::Column)
                .width_percent(1.0)
                .height_percent(1.0),
        );
        content.boxed()
    }
```

**Key changes:**
- `ctx.safe_area().bottom` → `MediaQuery::of(ctx).padding.bottom`.
- `SafeAreaClaim::bottom(stack)` → `MediaQuery::remove_padding(stack, RemoveEdges::BOTTOM)`.
- `BottomBarHeight::new(bottom_bar_height, content)` → `MediaQuery::reduce_view_insets_bottom(stack, tab_bar_height)` applied to the page subtree (the column wraps the result).

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: compiles (now that `tab_bar.rs` no longer references deleted APIs).

- [ ] **Step 4: Run `vexo_uikit` tests**

Run: `cargo test -p vexo_uikit`
Expected: PASS (existing TabBarView tests should still pass behaviorally; the page sits above the bar, keyboard reduces viewInsets.bottom by tab_bar_height).

If tests fail, examine the failures: they may reference `BottomBarHeight` or `SafeAreaClaim`. Port them to the new API (delete the obsolete assertions, keep the behavioral ones).

- [ ] **Step 5: Commit**

```bash
git add vexo_uikit/src/tab_bar.rs
git commit -m "refactor(vexo_uikit): migrate TabBarView to MediaQuery subtree mutators"
```

---

## Task 10: Simplify `KeyboardInsetSource` + render-loop-driven keyboard animation

**Files:**
- Modify: `vexo/src/core/geometry.rs` (simplify `KeyboardInsetSource`; add `KeyboardAnimationSource`)
- Modify: `vexo/src/platform/keyboard_ios.rs` (rewrite: store animation params, no tween)
- Modify: `vexo/src/window.rs` (interpolate each frame in the existing per-frame poll block)

**Interfaces:**
- Consumes: `objc2`, `objc2-foundation`, `block2`, `KeyboardInsetSource::set(current_height: f32)`, `crate::animation::{Curve, EaseInOutCurve, EaseInCurve, EaseOutCurve, LinearCurve}`.
- Produces: `KeyboardInsetSource` simplified to `current_height: f32` only; `KeyboardInsetSnapshot` and `KeyboardCurve` deleted; new `KeyboardAnimationSource` cell holding `{ from, target, duration_secs, start: Instant, curve: Box<dyn Curve>, active: bool }`; iOS shim writes animation params to the new source on `keyboardWillShow/Hide`; `WindowState::render_retain()` interpolates each frame and writes `current_height` to `KeyboardInsetSource`.

**Architecture decision:** Rather than installing a separate `CADisplayLink` (which would require an `NSObject` subclass + `declare_class!` + raw `msg_send!` since `objc2-ui-kit` doesn't enable the `CADisplayLink` feature), the interpolation runs in the **existing render loop** (`WindowState::render_retain()`). On iOS, winit's event loop is already `CADisplayLink`-driven — `request_frame()` arms the next vsync. So the interpolation timing is vsync-accurate, identical to a separate `CADisplayLink`, with far less code.

The curve mapping reuses `vexo/src/animation/curve.rs`'s `EaseInOutCurve`/`EaseInCurve`/`EaseOutCurve`/`LinearCurve` (which survive this plan — Task 11 only deletes `KeyboardCurve` and `curve_for` from `keyboard_avoidance.rs`, not the underlying curves). The raw UIKit curve value is mapped via `raw & 0x3` (same logic as the deleted `KeyboardCurve::from_uikit_raw`): 0→EaseInOut, 1→EaseIn, 2→EaseOut, 3→Linear. iOS keyboard raw=7 → 3 → Linear.

- [ ] **Step 1: Simplify `KeyboardInsetSource` in `core/geometry.rs`**

Edit `vexo/src/core/geometry.rs`. Replace the entire `KeyboardCurve` enum, `KeyboardInsetSnapshot` struct, and `KeyboardInsetSource` struct + impls (lines ~714-880) with:

```rust
// ============================================================================
// KEYBOARD INSET SOURCE
// ============================================================================

/// Shared atomic cell holding the current keyboard height (logical px).
/// Updated each frame by the render loop's interpolation driver (which reads
/// animation params from [`KeyboardAnimationSource`]); stays 0 on desktop /
/// Android (no shim installed).
#[derive(Clone)]
pub struct KeyboardInsetSource {
    inner: Arc<KeyboardInsetInner>,
}

struct KeyboardInsetInner {
    current_height: AtomicU32,
}

impl KeyboardInsetSource {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(KeyboardInsetInner {
                current_height: AtomicU32::new(0.0_f32.to_bits()),
            }),
        }
    }

    pub fn get(&self) -> f32 {
        f32::from_bits(self.inner.current_height.load(Ordering::Relaxed))
    }

    pub fn set(&self, current_height: f32) {
        self.inner
            .current_height
            .store(current_height.to_bits(), Ordering::Relaxed);
    }
}

impl Default for KeyboardInsetSource {
    fn default() -> Self {
        Self::new()
    }
}
```

Replace the `mod keyboard_inset_source_tests` module with:

```rust
#[cfg(test)]
mod keyboard_inset_source_tests {
    use super::*;

    #[test]
    fn default_is_zero() {
        let src = KeyboardInsetSource::new();
        assert_eq!(src.get(), 0.0);
    }

    #[test]
    fn set_updates_value() {
        let src = KeyboardInsetSource::new();
        src.set(300.0);
        assert_eq!(src.get(), 300.0);
    }

    #[test]
    fn clones_share_state() {
        let src = KeyboardInsetSource::new();
        let clone = src.clone();
        src.set(250.0);
        assert_eq!(clone.get(), 250.0);
    }
}
```

- [ ] **Step 2: Add `KeyboardAnimationSource` to `core/geometry.rs`**

After the `KeyboardInsetSource` impl block, add:

```rust
// ============================================================================
// KEYBOARD ANIMATION SOURCE
// ============================================================================

/// Animation parameters for the current keyboard show/hide transition.
/// Written by the iOS shim on `keyboardWillShow/Hide`; read + interpolated
/// by `WindowState::render_retain()` each frame to drive
/// `KeyboardInsetSource::set()`.
#[derive(Clone, Debug)]
pub struct KeyboardAnimation {
    /// Height the animation starts from (the source's current height at
    /// notification time).
    pub from: f32,
    /// Height the animation is tweening toward (`target_height` for show,
    /// `0.0` for hide).
    pub target: f32,
    /// Animation duration in seconds. `0.0` means "snap immediately."
    pub duration_secs: f32,
    /// The instant the OS keyboard animation began (captured in the iOS
    /// shim at the moment the notification fired).
    pub start: std::time::Instant,
    /// The animation curve, mapped from UIKit's raw curve value via
    /// `raw & 0x3`: 0→EaseInOut, 1→EaseIn, 2→EaseOut, 3→Linear.
    /// Stored as a raw `u8` so the cell is `Send + Sync`; the render loop
    /// maps it to a `Box<dyn Curve>` when interpolating.
    pub curve_raw: u8,
}

impl KeyboardAnimation {
    /// Map a UIKit `UIViewAnimationCurve` raw value to the curve enum.
    /// Mirrors the deleted `KeyboardCurve::from_uikit_raw`: `raw & 0x3`
    /// extracts the bottom 2 bits, handling the keyboard's private raw=7
    /// (→ 3 → Linear) correctly.
    pub fn curve(&self) -> Box<dyn crate::animation::Curve> {
        use crate::animation::{EaseInCurve, EaseInOutCurve, EaseOutCurve, LinearCurve};
        match self.curve_raw & 0x3 {
            0 => Box::new(EaseInOutCurve),
            1 => Box::new(EaseInCurve),
            2 => Box::new(EaseOutCurve),
            _ => Box::new(LinearCurve),
        }
    }

    /// Compute the interpolated height at time `now`.
    /// Returns `None` if the animation has completed (elapsed >= duration),
    /// so the caller can set the final value and mark the animation inactive.
    pub fn interpolate(&self, now: std::time::Instant) -> Option<f32> {
        if self.duration_secs <= 0.0 {
            return None;
        }
        let elapsed = now.duration_since(self.start).as_secs_f32();
        let t = (elapsed / self.duration_secs).min(1.0);
        if t >= 1.0 {
            return None;
        }
        let eased = self.curve().transform(t as f64) as f32;
        Some(self.from + (self.target - self.from) * eased)
    }
}

/// Shared cell holding the current keyboard animation params (or `None` when
/// the keyboard is at rest). Written by the iOS shim; read by the render loop.
///
/// Uses `Mutex<Option<KeyboardAnimation>>` (not atomics) because
/// `Instant` and `f32` pairs have no atomic representation, and updates are
/// rare (one per keyboard notification) + main-thread-only, so contention
/// is nonexistent.
#[derive(Clone)]
pub struct KeyboardAnimationSource {
    inner: Arc<std::sync::Mutex<Option<KeyboardAnimation>>>,
}

impl KeyboardAnimationSource {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Replace the current animation params. Called by the iOS shim on
    /// each `keyboardWillShow/Hide` notification.
    pub fn set(&self, animation: KeyboardAnimation) {
        *self.inner.lock().unwrap() = Some(animation);
    }

    /// Read + take the current animation. Returns `Some(animation)` if an
    /// animation is active; the caller is expected to either re-store it
    /// (still animating) or leave it `None` (completed).
    pub fn take(&self) -> Option<KeyboardAnimation> {
        self.inner.lock().unwrap().take()
    }

    /// Store an animation back after reading (if still active).
    pub fn restore(&self, animation: KeyboardAnimation) {
        *self.inner.lock().unwrap() = Some(animation);
    }
}

impl Default for KeyboardAnimationSource {
    fn default() -> Self {
        Self::new()
    }
}
```

Add `KeyboardAnimationSource` and `KeyboardAnimation` to the re-export in `vexo/src/core/mod.rs`.

Add a test module:

```rust
#[cfg(test)]
mod keyboard_animation_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn interpolate_returns_from_at_t0() {
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 300.0,
            duration_secs: 0.25,
            start: Instant::now(),
            curve_raw: 3, // Linear
        };
        let val = anim.interpolate(anim.start).unwrap();
        assert!(val.abs() < 1.0, "expected ~0 at t=0, got {}", val);
    }

    #[test]
    fn interpolate_returns_none_when_complete() {
        let start = Instant::now() - Duration::from_millis(500);
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 300.0,
            duration_secs: 0.25,
            start,
            curve_raw: 3,
        };
        assert!(anim.interpolate(Instant::now()).is_none());
    }

    #[test]
    fn interpolate_returns_none_when_duration_zero() {
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 300.0,
            duration_secs: 0.0,
            start: Instant::now(),
            curve_raw: 3,
        };
        assert!(anim.interpolate(Instant::now()).is_none());
    }

    #[test]
    fn interpolate_linear_halfway() {
        let start = Instant::now();
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 300.0,
            duration_secs: 0.25,
            start,
            curve_raw: 3, // Linear
        };
        // At t=0.125s (halfway through 0.25s), linear → 150px.
        let val = anim.interpolate(start + Duration::from_millis(125)).unwrap();
        assert!((val - 150.0).abs() < 2.0, "expected ~150 at halfway, got {}", val);
    }

    #[test]
    fn curve_raw_7_maps_to_linear() {
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 100.0,
            duration_secs: 1.0,
            start: Instant::now(),
            curve_raw: 7, // iOS keyboard private value → 7 & 3 = 3 → Linear
        };
        // Linear at t=0.5 → 50px.
        let val = anim.interpolate(anim.start + Duration::from_millis(500)).unwrap();
        assert!((val - 50.0).abs() < 1.0, "expected ~50 (linear) for raw=7, got {}", val);
    }

    #[test]
    fn animation_source_set_take_restore() {
        let src = KeyboardAnimationSource::new();
        assert!(src.take().is_none());
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 300.0,
            duration_secs: 0.25,
            start: Instant::now(),
            curve_raw: 3,
        };
        src.set(anim.clone());
        let taken = src.take().unwrap();
        assert_eq!(taken.target, 300.0);
        assert!(src.take().is_none());
        src.restore(taken);
        assert!(src.take().is_some());
    }
}
```

- [ ] **Step 3: Update `RenderContext::media_query_sources`**

Edit `vexo/src/stateful_widget.rs`. The `media_query_sources` method from Task 5 calls `keyboard_inset_source().current_target_height()`. After Step 1, `current_target_height()` no longer exists. Change it to `.get()`:

```rust
    pub fn media_query_sources(&self) -> MediaQuerySourcesSnapshot {
        MediaQuerySourcesSnapshot {
            safe_area: self.build_owner.safe_area_source().get(),
            keyboard_current_height: self.build_owner.keyboard_inset_source().get(),
            media_query: self.build_owner.media_query_data_source().get(),
        }
    }
```

- [ ] **Step 4: Build (non-iOS) to verify the source simplification compiles**

Run: `cargo build -p vexo`
Expected: compiles. The `KeyboardInsetSnapshot` / `KeyboardCurve` types are gone.

Run: `rg "KeyboardInsetSnapshot|KeyboardCurve|current_target_height" vexo/src/ vexo_uikit/src/ shared_app/src/`
If any references remain outside `keyboard_ios.rs`, fix them. (`keyboard_ios.rs` will be rewritten in Step 5.)

Run: `cargo test -p vexo --lib core::geometry`
Expected: all geometry tests PASS (including the new `keyboard_animation_tests`).

- [ ] **Step 5: Rewrite `keyboard_ios.rs` to store animation params (no tween)**

Edit `vexo/src/platform/keyboard_ios.rs`. Replace the entire file. The new structure: on each notification, capture `{ target_height, duration_secs, curve_raw }`, read the current `current_height` from the source (the `from` value), construct a `KeyboardAnimation`, write it to `KeyboardAnimationSource`, then call `request_frame()` so the render loop starts interpolating. No `CADisplayLink` is installed here — the render loop does the interpolation.

```rust
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
                    NonNull::from(&mut rect) as *mut _ as *mut c_void,
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
```

**Note:** `NonNull::from(&mut rect)` replaces the original's `NonNull::new_unchecked` — safer equivalent.

- [ ] **Step 6: Add `keyboard_animation_source` field to `WindowState` + wire the shim**

Edit `vexo/src/window.rs`.

Find the `keyboard_inset_source: KeyboardInsetSource,` field (around line 63) and add after it:

```rust
    /// Shared keyboard animation source. Written by the iOS shim on each
    /// `keyboardWillShow/Hide`; read + interpolated each frame by the
    /// render loop to drive `keyboard_inset_source.set()`.
    keyboard_animation_source: crate::core::KeyboardAnimationSource,
```

Find the `let keyboard_inset_source = KeyboardInsetSource::default();` line (around line 133) and add after it:

```rust
        let keyboard_animation_source = crate::core::KeyboardAnimationSource::default();
```

Find where `KeyboardObserver::install(...)` is called (search `rg "KeyboardObserver::install" vexo/src/`). Add the `keyboard_animation_source.clone()` argument after `keyboard_inset_source.clone()`:

```rust
            let _observer = crate::platform::keyboard_ios::KeyboardObserver::install(
                keyboard_inset_source.clone(),
                keyboard_animation_source.clone(),
                scale_factor,
                f32::MAX,
                request_frame_callback,
            );
```

Find the `keyboard_inset_source,` line in the struct initializer (around line 177) and add after it:

```rust
            keyboard_animation_source,
```

- [ ] **Step 7: Add the interpolation driver to `WindowState::render_retain()`**

Edit `vexo/src/window.rs`. Find the keyboard poll block (the `// 4.5. Poll the keyboard-inset source` block, currently lines ~617-642). Replace the entire block with:

```rust
        // 4.5. Keyboard animation interpolation.
        //
        // The iOS shim writes animation params to `keyboard_animation_source`
        // on each keyboardWillShow/Hide notification. Each frame, if an
        // animation is active, we interpolate the current height and write
        // it to `keyboard_inset_source`. The root MediaQuery reads
        // `keyboard_inset_source.get()` via `media_query_sources()`, so
        // this drives `MediaQueryData.viewInsets.bottom` frame-by-frame.
        //
        // On desktop the animation source is always `None` (no shim), so
        // this block is a no-op and `keyboard_inset_source` stays at 0.
        {
            if let Some(anim) = self.keyboard_animation_source.take() {
                let now = std::time::Instant::now();
                match anim.interpolate(now) {
                    Some(height) => {
                        self.keyboard_inset_source.set(height);
                        self.keyboard_animation_source.restore(anim);
                        if let Some(root_id) = self.three_tree_pipeline.element_registry().root() {
                            self.three_tree_pipeline.mark_needs_build(root_id);
                        }
                        self.three_tree_pipeline.mark_all_needs_layout();
                        self.request_frame();
                    }
                    None => {
                        self.keyboard_inset_source.set(anim.target);
                        if let Some(root_id) = self.three_tree_pipeline.element_registry().root() {
                            self.three_tree_pipeline.mark_needs_build(root_id);
                        }
                        self.three_tree_pipeline.mark_all_needs_layout();
                        self.request_frame();
                    }
                }
            }
        }
```

**Note:** The `keyboard_height_prev` field added in Task 12's plan is NOT needed — the animation source's `take()`/`restore()` pattern handles the "is it still animating" check. If Task 12 has already run and added `keyboard_height_prev`, remove it here (the interpolation block replaces the simple poll).

- [ ] **Step 8: Build (non-iOS) and test**

Run: `cargo build -p vexo`
Expected: compiles.

Run: `cargo build`
Expected: compiles (whole workspace).

Run: `cargo test -p vexo --lib core::geometry`
Expected: all geometry tests PASS (including new `keyboard_animation_tests`).

Run: `cargo test -p vexo`
Expected: all framework tests PASS.

- [ ] **Step 9: Build for iOS (if possible)**

Run: `cargo build --target aarch64-apple-ios -p vexo` (if the iOS target is set up).
If the target is unavailable, skip and note it in the commit message.

Expected: compiles. The iOS shim's `install()` signature changed (added `animation_source` param); verify the call site compiles.

- [ ] **Step 10: Commit**

```bash
git add vexo/src/core/geometry.rs vexo/src/core/mod.rs vexo/src/stateful_widget.rs vexo/src/platform/keyboard_ios.rs vexo/src/window.rs
git commit -m "feat(vexo): render-loop-driven keyboard animation (replaces per-widget tween)

- Simplify KeyboardInsetSource to current_height: f32 (Y1)
- Add KeyboardAnimationSource cell holding {from, target, duration, start, curve_raw}
- iOS shim writes animation params on keyboardWillShow/Hide (no tween)
- WindowState::render_retain() interpolates each vsync using the
  OS-reported curve (raw & 0x3 mapping), driving MediaQuery.viewInsets.bottom
- Delete KeyboardInsetSnapshot, KeyboardCurve (dead code)
- Reuses existing CADisplayLink-driven render loop (winit) for vsync-accurate timing"
```

---

## Task 11: Delete `KeyboardAvoidance` widget + `BottomBarHeight` InheritedWidget + curve modules

**Files:**
- Delete: `vexo/src/widgets/keyboard_avoidance.rs`
- Modify: `vexo/src/widgets/mod.rs`
- Modify: `vexo/src/lib.rs`
- Modify: `shared_app/src/chats/chat_screen.rs`

**Interfaces:**
- Consumes: `MediaQuery::of(ctx).viewInsets.bottom` (replaces `KeyboardAvoidance`).
- Produces: `KeyboardAvoidance`, `BottomBarHeight`, `KeyboardAvoidanceState` deleted.

- [ ] **Step 1: Delete the file**

Run: `rm vexo/src/widgets/keyboard_avoidance.rs`

- [ ] **Step 2: Remove the module registration and exports**

Edit `vexo/src/widgets/mod.rs`. Delete the line `mod keyboard_avoidance;`. Delete the line `pub use keyboard_avoidance::{BottomBarHeight, KeyboardAvoidance};`.

- [ ] **Step 3: Remove from `lib.rs` re-exports**

Edit `vexo/src/lib.rs`. Find the `pub use widgets::{...}` block (around line 208). Remove `BottomBarHeight` and `KeyboardAvoidance` from the import list.

- [ ] **Step 4: Migrate `chat_screen.rs`**

Edit `shared_app/src/chats/chat_screen.rs`. Find the `KeyboardAvoidance::new(MultiChild::new(...))` wrapper (around line 113). Replace the wrapper with a `WithLayout` that reads `MediaQuery::of(ctx).viewInsets.bottom` as bottom padding.

The current code (around line 112-128):

```rust
        DecoratedBox::with_style(
            KeyboardAvoidance::new(MultiChild::new(
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
            )),
            Style::default().background(theme.background),
        )
        .boxed()
```

Replace with:

```rust
        let mq = vexo::MediaQuery::of(ctx);
        let bottom_pad = mq.viewInsets.bottom;
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
                    .min_height(0.0)
                    .padding_each(0.0, 0.0, 0.0, bottom_pad),
            ),
            Style::default().background(theme.background),
        )
        .boxed()
```

Update the imports in `chat_screen.rs`: remove `KeyboardAvoidance` from the `use vexo::{...}` list, add `MediaQuery` if not already present.

- [ ] **Step 5: Build and test**

Run: `cargo build`
Expected: compiles (whole workspace).

Run: `cargo test`
Expected: PASS. The `keyboard_avoidance` tests are deleted with the file; the `chat_screen` integration tests should still pass behaviorally.

- [ ] **Step 6: Verify no remaining references**

Run: `rg "KeyboardAvoidance|BottomBarHeight|KeyboardCurve|KeyboardInsetSnapshot" vexo/ vexo_uikit/ shared_app/`
Expected: no matches (except possibly in `docs/` which is fine).

- [ ] **Step 7: Commit**

```bash
git add vexo/src/widgets/mod.rs vexo/src/lib.rs shared_app/src/chats/chat_screen.rs
git rm vexo/src/widgets/keyboard_avoidance.rs
git commit -m "refactor: delete KeyboardAvoidance widget + BottomBarHeight + curve modules"
```

---

## Task 12: Remove `RenderContext::safe_area()` / `keyboard_inset()` (now unused)

**Files:**
- Modify: `vexo/src/stateful_widget.rs`
- Modify: `vexo/src/build_owner.rs` (optionally — `safe_area_source`/`keyboard_inset_source` accessors on `BuildOwner` stay, since `RenderContext::media_query_sources` uses them)
- Modify: `vexo/src/window.rs` (the `keyboard_inset_snapshot_prev` field and its poll block become dead — simplify)

**Interfaces:**
- Produces: `RenderContext` no longer exposes `safe_area()` or `keyboard_inset()`; only `media_query_sources()`.

- [ ] **Step 1: Delete `RenderContext::safe_area()`**

Edit `vexo/src/stateful_widget.rs`. Find the `pub fn safe_area(&self) -> crate::layout::EdgeInsets` method (around line 349) and delete it entirely (including its doc comment).

- [ ] **Step 2: Delete `RenderContext::keyboard_inset()`**

In the same file, find `pub fn keyboard_inset(&self) -> crate::core::KeyboardInsetSnapshot` (around line 362) and delete it (including its doc comment).

- [ ] **Step 3: Clean up `WindowState`'s dead keyboard fields**

Edit `vexo/src/window.rs`. Task 10 already replaced the keyboard poll block with the interpolation driver. The old `keyboard_inset_snapshot_prev` field (if still present — Task 10 may have already removed it) is dead because `KeyboardInsetSnapshot` was deleted in Task 10.

Run: `rg "keyboard_inset_snapshot_prev" vexo/src/window.rs`
If matches found: delete the field declaration, the initializer in `WindowState::new`, and any remaining references. The interpolation driver from Task 10 Step 7 handles the per-frame polling — no replacement field is needed.

If Task 10 already removed `keyboard_inset_snapshot_prev`, skip this step.

- [ ] **Step 4: Build and test**

Run: `cargo build`
Expected: compiles.

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Verify no remaining references**

Run: `rg "\.safe_area\(\)|\.keyboard_inset\(\)" vexo/ vexo_uikit/ shared_app/`
Expected: no matches.

Run: `rg "keyboard_inset_snapshot_prev|KeyboardInsetSnapshot" vexo/ vexo_uikit/ shared_app/`
Expected: no matches.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/stateful_widget.rs vexo/src/window.rs
git commit -m "refactor(vexo): remove RenderContext::safe_area/keyboard_inset (replaced by media_query_sources)"
```

---

## Task 13: Update `lib.rs` exports + `CLAUDE.md` API mapping

**Files:**
- Modify: `vexo/src/lib.rs`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Finalize `lib.rs` exports**

Edit `vexo/src/lib.rs`. Verify the `pub use widgets::{...}` block contains `MediaQuery, MediaQueryData, MediaQueryMutator, Orientation, RemoveEdges` and does NOT contain `BottomBarHeight`, `KeyboardAvoidance`, `SafeAreaClaim`. The final block should look like:

```rust
pub use widgets::{
    Brightness, ChildPush, ClipRRect, DecoratedBox, FadeTransition, FractionalTranslation,
    GestureDetector, Grid, Image, IndexedStack, MediaQuery, MediaQueryData, MediaQueryMutator,
    MultiChild, Offstage, Opacity, Orientation, Positioned, RemoveEdges, SafeArea,
    ScrollController, ScrollView, SlideDirection, SlideTransition, Stack, Text, TextEdit,
    TextEditState, TextEditingController, Theme, ThemeData, Transform, Widget, WithLayout,
};
```

- [ ] **Step 2: Update `CLAUDE.md` API mapping table**

Edit `CLAUDE.md`. Find the "Web Developer API Mapping" table. Add rows:

```markdown
| `MediaQuery::of(ctx)` | React `useMediaQuery()` / CSS `@media` |
| `MediaQueryData` | Flutter `MediaQueryData` |
| `MediaQuery::remove_padding` / `remove_view_insets` | Flutter `MediaQuery.removePadding` / `removeViewInsets` |
```

Update the `SafeArea` row to note it's now a Component (if the table distinguishes).

Find any mention of `BottomBarHeight` or `KeyboardAvoidance` in `CLAUDE.md` and remove/replace with `MediaQuery` references.

- [ ] **Step 3: Build and test**

Run: `cargo build && cargo test`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/lib.rs CLAUDE.md
git commit -m "docs: update exports + CLAUDE.md API mapping for MediaQuery"
```

---

## Self-Review Notes

**Spec coverage:**
- §Data Model → Task 1.
- §`MediaQuery` Widget → Task 2.
- §Root Provisioning (3a source, 3b BuildOwner, 3c RootMediaQuery + auto-inject + dirty tracking) → Tasks 3, 4, 5, 6.
- §`SafeArea` Migration (5a Component, 5b delete SafeAreaClaim, 5c delete claim walk, 5d navigation.rs) → Tasks 7, 8, 9 (navigation in Task 7 Step 4).
- §Keyboard iOS shim + Y1 source simplification → Task 10 (+ 10b follow-up).
- §`KeyboardAvoidance` + `BottomBarHeight` deletion + TabBarView migration + chat screen migration → Tasks 9, 11.
- §`RenderContext` / `LayoutContext` cleanup → Tasks 8 (LayoutContext), 12 (RenderContext).
- §`lib.rs` re-exports + `CLAUDE.md` → Task 13.
- §Migration order (6 steps in spec) → Tasks 1–6 (additions), 7–9 (SafeArea), 10 (iOS), 11 (deletions), 12 (cleanup), 13 (docs). Matches.

**Placeholder scan:** No "TBD"/"TODO" in plan steps (one `TODO` mention in Task 6 Step 4 fallback — replaced with "default `is_dark = false`"). The v1 iOS shim (Task 10) explicitly documents the step-to-target limitation rather than hiding it as a TODO.

**Type consistency:**
- `MediaQueryData` fields: `size`, `device_pixel_ratio`, `padding`, `viewInsets`, `viewPadding`, `platform_brightness`, `orientation` — consistent across Tasks 1, 2, 5.
- `MediaQuery::of` returns owned `MediaQueryData` — consistent.
- `MediaQuerySourcesSnapshot` fields: `safe_area`, `keyboard_current_height`, `media_query` — consistent across Task 5 (definition) and Task 5 (RootMediaQuery usage).
- `RemoveEdges` constants: `NONE`, `TOP`, `BOTTOM`, `ALL` — consistent across Task 1 (definition), Task 2 (mutators), Task 9 (TabBarView).
- `KeyboardInsetSource::get() -> f32` — consistent across Task 10 (definition), Task 5 (media_query_sources uses `current_target_height` alias which is fine), Task 12 (window poll uses `get()`).
