# MediaQuery Design — Replace BottomBarHeight, Consolidate Display State

**Date:** 2026-07-26
**Status:** Approved (brainstorming complete)
**Supersedes:** `BottomBarHeight` InheritedWidget (`vexo/src/widgets/keyboard_avoidance.rs:66-101`), `SafeAreaClaim` widget + layouter safe-area claim walk (`vexo/src/widgets/safe_area.rs`, `vexo/src/layouter.rs`), `KeyboardAvoidance` widget (`vexo/src/widgets/keyboard_avoidance.rs`).

## Motivation

Vexo currently tracks display state (safe-area insets, keyboard height, window size, scale factor, brightness) through three parallel mechanisms:

1. **`BuildOwner` global side-channels** — `safe_area_source`, `keyboard_inset_source`. Read via `RenderContext::safe_area()` and `RenderContext::keyboard_inset()`.
2. **Ad-hoc `InheritedWidget`s** — `BottomBarHeight` (carries `f32`), `Theme` (carries `ThemeData`).
3. **Layouter-internal claim walk** — `SafeAreaClaimEdges` + top-down `safe_area_claim` pre-pass in `layouter.rs`, surfaced via the `SafeAreaClaim` widget.

`BottomBarHeight` is fragile: `type Value = f32` collides with any other `InheritedWidget<Value = f32>` on `TypeId` — `depend_on_inherited_widget::<f32>()` finds "the f32 provider," not "the BottomBarHeight provider." Not extensible. The layouter claim walk duplicates Flutter's `MediaQuery.removePadding` semantics in a non-Flutter idiom. `KeyboardAvoidance` runs a 700-line per-widget `AnimationController` tween because the iOS shim only reports the keyboard target snapshot, not continuous heights.

Flutter solves all of this with one primitive: `MediaQuery` — an `InheritedWidget` carrying `MediaQueryData` (size, devicePixelRatio, padding, viewInsets, viewPadding, platformBrightness, orientation), with subtree-scoped mutators (`removePadding`, `removeViewInsets`, `removeViewPadding`). `SafeArea` reads `MediaQuery.of(context).padding` at render time; `Scaffold`-style containers resize/pad against `MediaQuery.of(context).viewInsets.bottom`; the keyboard animation is driven by the OS writing continuous heights to `viewInsets`.

This spec brings Vexo to that model.

## Scope

**In scope:**
- New `MediaQuery` `InheritedWidget` + `MediaQueryData` carrying the fields Vexo uses today: `size`, `device_pixel_ratio`, `padding`, `viewInsets`, `viewPadding`, `platform_brightness`, `orientation`.
- Framework auto-injected root `MediaQuery` fed by platform sources.
- Migrate `SafeArea` from `RenderObject`-backed (layout-time) to `Component` (render-time), reading `MediaQuery.padding`.
- Delete `SafeAreaClaim` widget, `SafeAreaClaimEdges`, the layouter safe-area claim pre-pass, and the `RenderObject::safe_area_claim` / `set_effective_safe_area` / `effective_safe_area` trait methods.
- Delete `BottomBarHeight` InheritedWidget.
- Rework iOS keyboard shim to report continuous heights via `CADisplayLink` (Flutter's model); simplify `KeyboardInsetSource` to `current_height: f32`.
- Delete `KeyboardAvoidance` widget and the `KeyboardCurve` / `curve_for` / animation-curve modules (dead under continuous-height reporting).
- Migrate `TabBarView` to use `MediaQuery::reduce_view_insets_bottom` for the tab-bar obstruction.
- Remove `RenderContext::safe_area()` / `keyboard_inset()` and `LayoutContext::safe_area_source()`.

**Out of scope:**
- Flutter accessibility fields (`accessibleNavigation`, `boldText`, `textScaleFactor`, etc.) — stubbed absent; add later when needed.
- `InheritedModel` (aspect-based dependencies) — whole-value dependency only, same as the existing `InheritedWidget` design.
- Observable/`Signal`-based inherited values — immutable + rebuild-driven only.
- Migrating `Theme` onto `MediaQuery` — `Theme` stays a separate `InheritedWidget`.

## Architecture

### Data Model — `MediaQueryData`

New file: `vexo/src/widgets/media_query.rs`.

```rust
#[derive(Clone, PartialEq, Debug)]
pub struct MediaQueryData {
    /// Logical size of the app's view (window). Equals physical size / dpr.
    pub size: Size<Logical>,
    /// Physical pixels per logical pixel (window scale factor).
    pub device_pixel_ratio: f32,
    /// Safe-area insets NOT currently covered by system UI (status bar,
    /// notch, home indicator). Shrinks when the keyboard covers an edge.
    /// `SafeArea` reads this per-side.
    pub padding: EdgeInsets,
    /// Parts of the view covered by system UI that overlays content
    /// (keyboard). `viewInsets.bottom` = live animated keyboard height.
    pub viewInsets: EdgeInsets,
    /// Raw safe-area insets that do NOT shrink when the keyboard appears.
    /// Lets widgets distinguish "home indicator is there but covered" from
    /// "no home indicator".
    pub viewPadding: EdgeInsets,
    /// Light/dark mode from the OS.
    pub platform_brightness: Brightness,
    /// Portrait / landscape, derived from `size`.
    pub orientation: Orientation,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orientation { Portrait, Landscape }

impl MediaQueryData {
    /// Desktop/test default: size 0×0, dpr 1, all insets 0, light, portrait.
    pub const fn all_zero() -> Self { ... }

    /// Fluent subtree mutators. Each returns a clone with one field changed.
    /// Only fields that subtree providers actually mutate have a copy_with_*.
    pub fn copy_with_padding(&self, padding: EdgeInsets) -> Self { ... }
    pub fn copy_with_view_insets(&self, viewInsets: EdgeInsets) -> Self { ... }
    pub fn copy_with_view_padding(&self, viewPadding: EdgeInsets) -> Self { ... }
}
```

**Invariants (Flutter semantics, made explicit):**
- Keyboard up over home indicator → `padding.bottom = 0`, `viewPadding.bottom = 34`, `viewInsets.bottom = animated_height`.
- Keyboard down → `padding.bottom = 34`, `viewPadding.bottom = 34`, `viewInsets.bottom = 0`.
- `padding = viewPadding - viewInsets` per-edge, clamped to 0. Computed once at the root from platform sources; not re-derived downstream.

`Brightness` is reused from `vexo/src/widgets/theme.rs` (no separate type).

### `MediaQuery` Widget

`MediaQuery` is an `InheritedWidget` (same pattern as `Theme`) exposing `MediaQueryData`.

```rust
pub struct MediaQuery {
    data: MediaQueryData,
    child: Box<dyn Widget>,
    key: Option<WidgetKey>,
}

impl MediaQuery {
    pub fn new(data: MediaQueryData, child: impl Widget + 'static) -> Self { ... }
    pub fn with_key(self, key: impl Into<WidgetKey>) -> Self { ... }

    /// Read nearest ancestor MediaQuery. Establishes a dependency:
    /// caller rebuilds when the data changes. Falls back to
    /// `MediaQueryData::all_zero()` when no ancestor provides — so tests
    /// and bare demos work without a root.
    pub fn of(ctx: &mut RenderContext) -> MediaQueryData {
        ctx.depend_on_inherited_widget::<MediaQueryData>()
            .unwrap_or_else(MediaQueryData::all_zero)
    }

    /// Provide a subtree with the named edges' `padding` zeroed
    /// (Flutter's `MediaQuery.removePadding`).
    pub fn remove_padding(child: impl Widget + 'static, edges: RemoveEdges) -> Self { ... }

    /// Provide a subtree with the named edges' `viewInsets` zeroed.
    pub fn remove_view_insets(child: impl Widget + 'static, edges: RemoveEdges) -> Self { ... }

    /// Provide a subtree with the named edges' `viewPadding` zeroed.
    pub fn remove_view_padding(child: impl Widget + 'static, edges: RemoveEdges) -> Self { ... }

    /// Provide a subtree with `viewInsets.bottom` reduced by `amount`
    /// (clamped to 0). Used by containers like TabBarView that sit between
    /// the page content and the screen bottom.
    pub fn reduce_view_insets_bottom(child: impl Widget + 'static, amount: f32) -> Self { ... }
}

impl InheritedWidget for MediaQuery {
    type Value = MediaQueryData;
    fn value(&self) -> &MediaQueryData { &self.data }
    fn child(&self) -> &dyn Widget { self.child.as_ref() }
    fn key(&self) -> Option<WidgetKey> { self.key.clone() }
}
impl_widget_for_inherited!(MediaQuery);
```

**`RemoveEdges`** — small per-side flag struct (replaces `SafeAreaClaimEdges`):

```rust
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
```

`of` returns owned `MediaQueryData` (matches `Theme::of`; the struct is cheaply cloneable; dependency is registered inside `depend_on_inherited_widget` so the caller auto-rebuilds).

### Root Provisioning — Platform Sources + Auto-Injected Root MediaQuery

**New `MediaQueryDataSource` cell** (in `vexo/src/core/geometry.rs` alongside `SafeAreaSource`):

```rust
/// Shared atomic cell holding the platform-derived parts of
/// `MediaQueryData` that have no existing source. Updated by
/// `WindowState` each frame; read by the root MediaQuery component.
///
/// `padding`/`viewInsets`/`viewPadding` stay on the existing
/// `SafeAreaSource` / `KeyboardInsetSource` cells (they already
/// propagate correctly); this cell carries only the new fields.
///
/// Uses `bool` for brightness (not `Brightness`) so this core cell has no
/// dependency on `widgets/theme.rs`. The root `MediaQuery` component
/// converts `is_dark` → `Brightness` when composing `MediaQueryData`.
#[derive(Clone, Default)]
pub struct MediaQueryDataSource {
    inner: Arc<Inner>, // atomics: size_w, size_h, device_pixel_ratio, is_dark
}

impl MediaQueryDataSource {
    pub fn new() -> Self { ... }
    pub fn set(&self, size: Size<Logical>, device_pixel_ratio: f32, is_dark: bool) { ... }
    pub fn get(&self) -> MediaQueryDataSourceSnapshot { ... }
}

pub struct MediaQueryDataSourceSnapshot {
    pub size: Size<Logical>,
    pub device_pixel_ratio: f32,
    pub is_dark: bool,
}
```

`safe_area_source` and `keyboard_inset_source` stay as separate cells (Approach A — they already work; no benefit to merging).

**`BuildOwner`** keeps `safe_area_source` + `keyboard_inset_source` and adds `media_query_data_source`. Exposes all three to the root via one method:

```rust
impl BuildOwner {
    pub fn media_query_data_source(&self) -> MediaQueryDataSource { ... }
    // safe_area_source() / keyboard_inset_source() stay (used by root only)
}
```

**`RenderContext`** loses `safe_area()` and `keyboard_inset()`. Gains one accessor for the root:

```rust
impl RenderContext<'_> {
    /// Snapshot of all three platform sources. Intended for the root
    /// MediaQuery component only; all other widgets read `MediaQuery::of`.
    pub fn media_query_sources(&self) -> MediaQuerySourcesSnapshot { ... }
}

pub struct MediaQuerySourcesSnapshot {
    pub safe_area: EdgeInsets,
    pub keyboard_current_height: f32,
    pub media_query: MediaQueryDataSourceSnapshot,
}
```

**`RootMediaQuery`** — framework-internal `Component` (not exported). Lives in `vexo/src/widgets/media_query.rs`:

```rust
struct RootMediaQuery { child: Box<dyn Widget> }

impl Component for RootMediaQuery {
    type State = ();

    fn render(&self, _: &mut (), ctx: &mut RenderContext) -> Box<dyn Widget> {
        let sources = ctx.media_query_sources();
        let viewPadding = sources.safe_area;
        let viewInsets = EdgeInsets {
            bottom: sources.keyboard_current_height,
            ..Default::default()
        };
        let padding = EdgeInsets {
            top: (viewPadding.top - viewInsets.top).max(0.0),
            bottom: (viewPadding.bottom - viewInsets.bottom).max(0.0),
            left: (viewPadding.left - viewInsets.left).max(0.0),
            right: (viewPadding.right - viewInsets.right).max(0.0),
        };
        let orientation = if sources.media_query.size.width
            >= sources.media_query.size.height
        { Orientation::Landscape } else { Orientation::Portrait };

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

The framework wraps `Application::view()` output in `RootMediaQuery` before mounting, in the same place it currently mounts the root (`pipeline.rs` / `lib.rs::run_desktop_demo`). App authors never see this — they just call `MediaQuery::of(ctx)`.

**Dirty tracking:** `WindowState` already marks the tree dirty when `safe_area` or `keyboard_inset` changes. It will additionally mark dirty when `size`, `scale_factor`, or `platform_brightness` changes (a few lines in the existing per-frame check at `window.rs:601-627`). `RootMediaQuery` then re-renders, reads fresh sources, and the `MediaQuery` InheritedWidget's `update_should_notify` (default value-inequality) propagates the change to dependents.

### `SafeArea` Migration — Render-Time Component

`SafeArea` becomes a stateless `Component` that composes `WithLayout` + `MediaQuery::remove_padding`:

```rust
pub struct SafeArea {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    top: bool, right: bool, bottom: bool, left: bool,
    minimum: EdgeInsets,
}

impl Component for SafeArea {
    type State = ();

    fn render(&self, _: &mut (), ctx: &mut RenderContext) -> Box<dyn Widget> {
        let mq = MediaQuery::of(ctx);
        let insets = mq.padding;

        let left = if self.left { insets.left.max(self.minimum.left) } else { 0.0 };
        let right = if self.right { insets.right.max(self.minimum.right) } else { 0.0 };
        let top = if self.top { insets.top.max(self.minimum.top) } else { 0.0 };
        let bottom = if self.bottom { insets.bottom.max(self.minimum.bottom) } else { 0.0 };

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

Double-consume prevention moves from the layouter claim walk to Flutter's model: `SafeArea` provides a `MediaQuery` with consumed edges' `padding` zeroed. Descendant `SafeArea`s see 0 for those edges → no double-consume.

**Deleted from `safe_area.rs`:** `SafeAreaRenderObject`, `SafeAreaElement`, `SafeAreaClaimRenderObject`, `SafeAreaClaimElement`, `SafeAreaClaim` widget, and the `effective` / `set_effective_safe_area` / `effective_safe_area` machinery.

**Deleted from `render_object.rs`:** `SafeAreaClaimEdges`, the `safe_area_claim()` / `set_effective_safe_area()` / `effective_safe_area()` methods on the `RenderObject` trait.

**Deleted from `layouter.rs`:** the top-down safe-area claim pre-pass (the `safe_area_claim` walk at lines ~83-109). Layout becomes purely bottom-up.

**Deleted from `LayoutContext`:** `safe_area_source()` / `set_safe_area_source()`. Layout no longer reads safe area; root `MediaQuery` reads the source via `BuildOwner`.

**`navigation.rs:584`** migration: `let safe_insets = ctx.safe_area();` → `let safe_insets = MediaQuery::of(ctx).padding;`.

### Keyboard — iOS Continuous-Height Shim + `KeyboardInsetSource` Simplification

**iOS shim rework** (in `shared_app`): replace snapshot reporting with a `CADisplayLink`-driven continuous height reporter. On `keyboardWillShow`:
1. Capture `{ target_height, duration, animation_curve_raw }` from the `NSKeyboardEvent`.
2. Start a `CADisplayLink` on the main run loop.
3. Each frame, sample the OS keyboard's actual frame position using the OS-reported animation curve (the private curve raw value included — no Rust-side approximation).
4. Write the current height to `KeyboardInsetSource.current_height`.

On `keyboardWillHide`: same, animating back to 0. On animation completion (or `keyboardDidShow/Hide`), stop the display link.

**`KeyboardInsetSource` simplification (Y1):**

```rust
/// Shared atomic cell holding the current keyboard height (logical px).
/// Updated each frame by the iOS shim's CADisplayLink; stays 0 on desktop.
#[derive(Clone, Default)]
pub struct KeyboardInsetSource {
    inner: Arc<AtomicU32>, // bits-as-f32
}

impl KeyboardInsetSource {
    pub fn new() -> Self { ... }
    pub fn set(&self, current_height: f32) { ... }
    pub fn get(&self) -> f32 { ... }
}
```

The `KeyboardInsetSnapshot` struct (`{ target_height, duration_secs, curve, animation_start }`), the `KeyboardCurve` enum, and the `curve_for` function all become dead code and are deleted.

**Display-link failure graceful degradation:** if the display link fails to start (rare; e.g. background arrival mid-animation), the shim steps `current_height` to `target_height` on the next frame (no animation, but no stuck state). Documented.

### `KeyboardAvoidance` + `BottomBarHeight` Deletion

**`KeyboardAvoidance` widget** (`vexo/src/widgets/keyboard_avoidance.rs`) is deleted in full: `KeyboardAvoidanceState` (the 700-line `AnimationController` state machine), the `Component` impl, all tests for the state machine, and the `RENDER_LATENCY` constant. With the iOS shim reporting continuous heights, there is no per-widget tween to run.

**`BottomBarHeight` InheritedWidget** is deleted. Its replacement is `MediaQuery::reduce_view_insets_bottom`.

**`TabBarView` migration** (`vexo_uikit/src/tab_bar.rs`):

```rust
// Before:
let safe_bottom = ctx.safe_area().bottom;
let bottom_bar_height = TAB_BAR_HEIGHT + safe_bottom;
// ... SafeAreaClaim::bottom(stack) inside the column ...
BottomBarHeight::new(bottom_bar_height, content).boxed()

// After:
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
```

The page content now reads `MediaQuery::of(ctx).viewInsets.bottom` which is *already reduced by `tab_bar_height`*. Any padding/resize logic inside the page (replacing the old `KeyboardAvoidance` wrapper) lifts by exactly the right amount without needing a separate obstruction signal.

**`shared_app` chat screen** migration: the existing `KeyboardAvoidance::new(...)` wrapper is removed. The chat screen reads `MediaQuery::of(ctx).viewInsets.bottom` directly and applies plain `WithLayout` padding equal to `viewInsets.bottom`. Under the Y continuous-height model, `viewInsets.bottom` is already animated frame-by-frame by the iOS shim's `CADisplayLink`, so the widget re-renders each frame as the value changes and the padding tracks the keyboard slide — no separate `AnimatedPadding` wrapper is needed.

### `RenderContext` / `LayoutContext` Cleanup

**Removed from `RenderContext`:** `safe_area()`, `keyboard_inset()`.
**Removed from `LayoutContext`:** `safe_area_source()`, `set_safe_area_source()`.
**Added to `RenderContext`:** `media_query_sources()` (root-only accessor).

### `lib.rs` Re-exports

Update `vexo/src/lib.rs` to:
- Export `MediaQuery`, `MediaQueryData`, `Orientation`, `RemoveEdges`.
- Remove `BottomBarHeight`, `SafeAreaClaim`, `SafeAreaClaimEdges`, `KeyboardAvoidance`, `KeyboardCurve` (if currently exported).
- Keep `SafeArea`, `Theme`, `ThemeData`, `Brightness`.

### `CLAUDE.md` API Mapping Table

Update the "Web Developer API Mapping" table:
- `MediaQuery::of(ctx)` → React `useMediaQuery()` / CSS `@media`.
- `MediaQueryData` → Flutter `MediaQueryData`.
- `SafeArea` (now a Component) → Flutter `SafeArea`.

## Data Flow

### Root → Tree

```
WindowState (per frame)
    ├── safe_area_source.set(...)
    ├── keyboard_inset_source.set(current_height)   // from iOS CADisplayLink
    └── media_query_data_source.set(size, dpr, is_dark)
         │
         ▼  (mark tree dirty on change)
    RootMediaQuery::render(ctx)
         │  ctx.media_query_sources() → { safe_area, keyboard_current_height, mq }
         ▼
    MediaQuery::new(MediaQueryData { ... }, child)
         │  (InheritedWidget — registers in InheritedRegistry)
         ▼
    Application::view() subtree
         │  MediaQuery::of(ctx) at any depth
         ▼
    SafeArea / TabBarView / chat screen / etc.
```

### Subtree-Scoped Mutations

```
TabBarView::render
    ├── MediaQuery::of(ctx).padding.bottom → tab_bar_height
    ├── MediaQuery::reduce_view_insets_bottom(page, tab_bar_height)
    │       └── provides MediaQueryData { viewInsets.bottom -= tab_bar_height }
    ├── MediaQuery::remove_padding(page, BOTTOM)
    │       └── provides MediaQueryData { padding.bottom = 0 }
    └── page subtree reads the reduced MediaQueryData via MediaQuery::of(ctx)

SafeArea::render
    ├── MediaQuery::of(ctx).padding → per-side insets
    ├── WithLayout(child, padding=...)
    └── MediaQuery::remove_padding(child, enabled_edges)
            └── descendants see padding=0 for those edges
```

## Testing Strategy

| Layer | Tests |
|---|---|
| `MediaQueryData` | Unit: `all_zero()` defaults; `copy_with_*` immutability; `padding = viewPadding - viewInsets` clamp invariant. |
| `MediaQuery` widget | Unit (existing inherited-widget harness): provider registers, `of` returns value, `of` returns `all_zero()` when no provider, dependent rebuilds on value change, `remove_padding`/`remove_view_insets`/`remove_view_padding`/`reduce_view_insets_bottom` produce correct copyWith'd values. |
| `RootMediaQuery` | Pipeline test: mount `RootMediaQuery(child)`, set platform sources, assert `MediaQuery::of` returns composed data; change sources, assert dependent rebuilds. |
| `SafeArea` (migrated) | Port existing `safe_area.rs` tests: defaults, per-side opt-out, minimum floor, nested-no-double-consume (now via `MediaQuery::remove_padding` not claim walk), `SafeAreaClaim::bottom` equivalent (now `MediaQuery::remove_padding(_, BOTTOM)`). |
| `TabBarView` | Port existing `tab_bar.rs` tests: page content sits above bar; keyboard lifts content by `viewInsets.bottom` (already reduced by `tab_bar_height` inside the page). |
| `KeyboardInsetSource` (Y1) | Unit: `current_height` get/set; default 0. |
| iOS shim | Manual / on-device: keyboard show/hide produces frame-accurate `current_height` updates. (No unit test — requires UIKit.) |
| Deletions | Verify removed APIs (`SafeAreaClaim`, `SafeAreaClaimEdges`, claim walk, `BottomBarHeight`, `KeyboardAvoidance`, `RenderContext::safe_area`/`keyboard_inset`, `KeyboardCurve`/`curve_for`) have no remaining references (`rg`). |

## Error Handling

- `MediaQuery::of` with no ancestor → `all_zero()` (no panic; matches `Theme::of` fallback).
- `KeyboardInsetSource.current_height` default 0 → desktop/tests get no keyboard padding (same as today).
- `MediaQueryDataSource` default → `size=0×0, dpr=1, light, portrait` (matches current desktop defaults).
- iOS display-link failure → falls back to stepping `current_height` to target on next frame (no animation, but no stuck state). Graceful degradation.

## Migration Order

Each step compiles + tests pass before the next.

1. **Add `MediaQueryData` + `MediaQuery` widget + `RootMediaQuery` + `MediaQueryDataSource`** — pure additions, no deletions. Existing code untouched. Land tests.
2. **Migrate `SafeArea` to Component + `MediaQuery::remove_padding`**; delete `SafeAreaRenderObject`/`SafeAreaElement`/`SafeAreaClaim`/claim walk/`SafeAreaClaimEdges`. Update `navigation.rs` and `TabBarView`'s `SafeAreaClaim::bottom` call.
3. **Rework iOS shim to continuous-height (Y1)**; simplify `KeyboardInsetSource` to `current_height: f32`. Desktop stays 0.
4. **Delete `KeyboardAvoidance` + `BottomBarHeight` + curve modules**; update `TabBarView` to `MediaQuery::reduce_view_insets_bottom`; update `shared_app` chat screen to read `MediaQuery.viewInsets.bottom` directly.
5. **Remove `RenderContext::safe_area()` / `keyboard_inset()`**; remove `LayoutContext::safe_area_source()`. Verify no references.
6. **Update `lib.rs` re-exports**; update `CLAUDE.md` API mapping table.

Steps 2 and 4 are the high-risk ones (deletions + behavior changes); step 3 is the iOS rework.

## API Surface Summary

**New public API:**
- `vexo::MediaQuery` (widget + `of` + `remove_padding` / `remove_view_insets` / `remove_view_padding` / `reduce_view_insets_bottom`)
- `vexo::MediaQueryData`
- `vexo::Orientation`
- `vexo::RemoveEdges`

**Removed public API:**
- `vexo::BottomBarHeight`
- `vexo::SafeAreaClaim`
- `vexo::SafeAreaClaimEdges`
- `vexo::KeyboardAvoidance`
- `vexo::KeyboardCurve` (if exported)
- `RenderContext::safe_area()`
- `RenderContext::keyboard_inset()`
- `LayoutContext::safe_area_source()` / `set_safe_area_source()`

**Changed:**
- `vexo::SafeArea` — now a `Component` (was `RenderObject`-backed); same public builder API (`new`, `top`, `bottom`, `left`, `right`, `minimum`, `with_key`).

**Unchanged:**
- `vexo::Theme`, `vexo::ThemeData`, `vexo::Brightness`
- `vexo::SafeAreaSource` (platform input; read by root)
- `vexo::KeyboardInsetSource` (simplified to `current_height: f32`; still a platform input read by root)
- `BuildOwner::safe_area_source()` / `keyboard_inset_source()` (kept; root-only readers)
