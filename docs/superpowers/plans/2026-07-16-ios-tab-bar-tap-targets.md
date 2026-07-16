# iOS-Style Tab Bar Tap Targets — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the demo app's tab bar behave like iOS `UITabBar` — each tab owns an equal, fully-tappable fraction of the bar (`bar_width / N` × 49pt) with no dead space between items; icon+label visually centered so the first/last items' content appears inset from the screen edge; fixed 49pt bar height.

**Architecture:** Add a configurable `Layout` field to `GestureDetector` (currently pass-through with a hardcoded `Column + Stretch` layout) so it can take `flex_grow` to claim an equal slot width and `justify(Center)` to center its content vertically. Then rebuild `TabBarView`'s items row as a fixed-height `Flex::row()` of equal-`flex_grow` GestureDetectors instead of a `SpaceBetween`-spread row of content-sized items. Drop the redundant content `.padding(8.0)` in the app's `tab_bar_builder` since the slot now handles vertical centering.

**Tech Stack:** Rust, vexo framework (`vexo/src/widgets/gesture_detector.rs`), vexo_uikit (`vexo_uikit/src/tab_bar.rs`), shared_app (`shared_app/src/app.rs`), Taffy layout engine.

## Global Constraints

- `GestureDetector`'s default layout must remain `Layout::default().flex_direction(Column).align(AlignItems::Stretch)` — every existing `.on_press()` caller (via the `Widget::on_press` trait method on `Box<dyn Widget>`) depends on this default. Backward compatibility is mandatory.
- `GestureDetectorRenderObject::layout()` currently discards `LayoutResult.size` (returns `Size::zero()`); the layouter (`vexo/src/layouter.rs:139`) also discards it. Final node size comes from Taffy's parent-driven resolution, not the RO's reported size. Do not "fix" this — it's the intended pass-through behavior.
- Bar height is hardcoded 49pt (iOS standard). Do not parameterize (YAGNI).
- `SafeArea` wrapping of the tab bar row and the top hairline stay unchanged — only the items-row construction inside `TabBarView::render()` changes.
- Per `CLAUDE.md`: never run `cargo run -p desktop_demo` from the agent session. Runtime verification is done by the user, not the agent.
- Per `CLAUDE.md`: always run `cargo build` after editing Rust files, and `cargo test` after implementing features. Never assume tests pass without running them.
- Per `CLAUDE.md`: do not add comments to code unless asked.

---

## File Structure

- Modify: `vexo/src/widgets/gesture_detector.rs` — add `layout: Layout` field to `GestureDetector` + `GestureDetectorRenderObject`; add inherent `with_layout` builder; thread the layout through `create_render_object`, `Clone`, and `set_widget_from_widget`.
- Modify: `vexo_uikit/src/tab_bar.rs` — rebuild the items row in `TabBarView::render()` as a 49pt `Flex::row()` of equal-`flex_grow` GestureDetectors with `justify(Center)`; add new tests for slot geometry and tap-target coverage.
- Modify: `shared_app/src/app.rs` — drop the redundant `.padding(8.0)` from the `tab_bar_builder`.

No new files.

---

## Task 1: Make `GestureDetector` public and give it a configurable `Layout`

This is the foundation — without it, the tab bar can't make its items take `flex_grow` or center their content. `GestureDetector` is currently `pub(crate)` (`vexo/src/widgets/mod.rs:54`) and the `gesture_detector` module is private (`mod gesture_detector;` at line 9), so `vexo_uikit` cannot construct one directly. This task makes both public (consistent with `WithLayout`, `DecoratedContainer`, etc. — a UI kit building on vexo legitimately needs to construct gesture handlers with custom layouts) AND adds the `Layout` field. Done in isolation so the framework change compiles and tests green before any tab-bar work.

**Files:**
- Modify: `vexo/src/widgets/mod.rs:9` (make module `pub mod`)
- Modify: `vexo/src/widgets/mod.rs:54` (change `pub(crate) use` to `pub use`)
- Modify: `vexo/src/lib.rs:203-208` (add `GestureDetector` to the `pub use widgets::{...}` list)
- Modify: `vexo/src/widgets/gesture_detector.rs:30-101` (imports, struct, `new`, builders, `Clone`)
- Modify: `vexo/src/widgets/gesture_detector.rs:125-127` (`create_render_object`)
- Modify: `vexo/src/widgets/gesture_detector.rs:177-183` (`set_widget_from_widget`)
- Modify: `vexo/src/widgets/gesture_detector.rs:388-434` (`GestureDetectorRenderObject` struct + `new` + `layout`)
- Test: new tests in `vexo/src/widgets/gesture_detector.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `vexo::layout::Layout` (already in scope at `gesture_detector.rs:36`)
- Produces:
  - `vexo::GestureDetector` — publicly accessible concrete widget type
  - `vexo::widgets::gesture_detector::GestureDetectorRenderObject` — publicly accessible RO type (needed for tab-bar tests to identify slot ROs by downcast)
  - `GestureDetector::with_layout(self, Layout) -> Self` — inherent builder method (shadows `Widget::with_layout` trait method on the concrete type)
  - `GestureDetectorRenderObject::new_with_layout(Layout) -> Self` — constructor used by `create_render_object`
  - `GestureDetectorRenderObject` gains a `layout: Layout` field; `layout()` uses `self.layout` instead of the hardcoded `Layout`

- [ ] **Step 1: Read the current `GestureDetector` widget + render object to confirm exact lines**

Run: `Read vexo/src/widgets/gesture_detector.rs offset=30 limit=10` (imports) and `Read vexo/src/widgets/gesture_detector.rs offset=59 limit=45` (widget struct + impl) and `Read vexo/src/widgets/gesture_detector.rs offset=388 limit=50` (RO struct + impl)

Expected: see the imports already include `Layout` (line 36), the `GestureDetector` struct at line 59 with fields `key/child/on_press/on_release`, `GestureDetectorRenderObject` at line 388 with fields `child/computed_bounds/layout_node` and a hardcoded `Layout::default().flex_direction(Column).align(Stretch)` at line 413.

- [ ] **Step 2: Add `Layout` import to the existing `use crate::layout::{...}` line**

In `vexo/src/widgets/gesture_detector.rs`, the import at line 36 currently reads:

```rust
use crate::layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey};
```

`Layout` is already imported — no change needed. Confirm by reading the line; if `Layout` is missing, add it.

- [ ] **Step 3: Make the `gesture_detector` module public**

In `vexo/src/widgets/mod.rs`, change line 9:

```rust
mod gesture_detector;
```

to:

```rust
pub mod gesture_detector;
```

- [ ] **Step 4: Make the `GestureDetector` re-export public**

In `vexo/src/widgets/mod.rs`, change line 54:

```rust
pub(crate) use gesture_detector::GestureDetector;
```

to:

```rust
pub use gesture_detector::GestureDetector;
```

- [ ] **Step 5: Add `GestureDetector` to the public widget re-export list**

In `vexo/src/lib.rs`, the `pub use widgets::{...}` block at lines 203-208 currently reads:

```rust
pub use widgets::{
    Column, DecoratedContainer, FadeTransition, Flex, FractionalTranslation, Grid, Image,
    IndexedStack, Offstage, Opacity, Positioned, Row, SafeArea, ScrollController, ScrollView,
    SlideDirection, SlideTransition, Stack, Text, TextEdit, TextEditState, TextEditingController,
    Theme, ThemeData, Transform, Widget, WithLayout,
};
```

Replace with (inserting `GestureDetector` alphabetically after `FractionalTranslation`):

```rust
pub use widgets::{
    Column, DecoratedContainer, FadeTransition, Flex, FractionalTranslation, GestureDetector, Grid,
    Image, IndexedStack, Offstage, Opacity, Positioned, Row, SafeArea, ScrollController,
    ScrollView, SlideDirection, SlideTransition, Stack, Text, TextEdit, TextEditState,
    TextEditingController, Theme, ThemeData, Transform, Widget, WithLayout,
};
```

- [ ] **Step 6: Verify the visibility change compiles before touching the struct**

Run: `cargo build -p vexo`
Expected: compiles with no errors. `GestureDetector` is now `vexo::GestureDetector` and `GestureDetectorRenderObject` is now `vexo::widgets::gesture_detector::GestureDetectorRenderObject`.

- [ ] **Step 7: Add `layout: Layout` field to `GestureDetector` struct**

In `vexo/src/widgets/gesture_detector.rs`, replace the struct definition (lines 59-66):

```rust
pub struct GestureDetector {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    /// Callback invoked when pointer is pressed inside the child bounds.
    on_press: Option<Rc<RefCell<dyn FnMut()>>>,
    /// Callback invoked when pointer is released inside the child bounds.
    on_release: Option<Rc<RefCell<dyn FnMut()>>>,
}
```

with:

```rust
pub struct GestureDetector {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    layout: Layout,
    on_press: Option<Rc<RefCell<dyn FnMut()>>>,
    on_release: Option<Rc<RefCell<dyn FnMut()>>>,
}
```

- [ ] **Step 8: Update `GestureDetector::new` to set the default layout**

Replace `new` (lines 70-77):

```rust
pub fn new(child: impl Widget + 'static) -> Self {
    Self {
        key: None,
        child: Box::new(child),
        on_press: None,
        on_release: None,
    }
}
```

with:

```rust
pub fn new(child: impl Widget + 'static) -> Self {
    Self {
        key: None,
        child: Box::new(child),
        layout: Layout::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch),
        on_press: None,
        on_release: None,
    }
}
```

- [ ] **Step 9: Add the inherent `with_layout` builder method**

Immediately after the `with_key` method (after line 83), add:

```rust
/// Set the layout for this GestureDetector.
///
/// Overrides the default `Column + Stretch` layout. Use this when the
/// detector needs to participate in flex sizing (e.g. `flex_grow` to fill
/// a slot) or center its content (`justify(Center)`).
pub fn with_layout(mut self, layout: Layout) -> Self {
    self.layout = layout;
    self
}
```

- [ ] **Step 10: Update `Clone for GestureDetector` to clone `layout`**

Replace the `Clone` impl (lines 103-112):

```rust
impl Clone for GestureDetector {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            on_press: self.on_press.clone(),
            on_release: self.on_release.clone(),
        }
    }
}
```

with:

```rust
impl Clone for GestureDetector {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            layout: self.layout.clone(),
            on_press: self.on_press.clone(),
            on_release: self.on_release.clone(),
        }
    }
}
```

- [ ] **Step 11: Update `create_render_object` to pass the layout**

Replace `create_render_object` (lines 125-127):

```rust
fn create_render_object(&self) -> Box<dyn RenderObject> {
    Box::new(GestureDetectorRenderObject::new())
}
```

with:

```rust
fn create_render_object(&self) -> Box<dyn RenderObject> {
    Box::new(GestureDetectorRenderObject::new_with_layout(self.layout.clone()))
}
```

- [ ] **Step 12: Update `set_widget_from_widget` to copy `layout`**

Replace `set_widget_from_widget` (lines 178-183):

```rust
fn set_widget_from_widget(&mut self, widget: &GestureDetector) {
    self.key = widget.key.clone();
    self.on_press = widget.on_press.clone();
    self.on_release = widget.on_release.clone();
    self.widget = Some(widget.clone_boxed());
}
```

with:

```rust
fn set_widget_from_widget(&mut self, widget: &GestureDetector) {
    self.key = widget.key.clone();
    self.on_press = widget.on_press.clone();
    self.on_release = widget.on_release.clone();
    self.widget = Some(widget.clone_boxed());
}
```

No change needed here — `widget.clone_boxed()` already clones the whole widget including the new `layout` field (via the `Clone` impl updated in Step 10). Confirm by reading; no edit required.

- [ ] **Step 13: Add `layout: Layout` field to `GestureDetectorRenderObject`**

Replace the RO struct (lines 388-392):

```rust
pub struct GestureDetectorRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}
```

with:

```rust
pub struct GestureDetectorRenderObject {
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
    layout: Layout,
}
```

- [ ] **Step 14: Update `GestureDetectorRenderObject::new` and add `new_with_layout`**

Replace the RO impl block opening (lines 394-403):

```rust
impl GestureDetectorRenderObject {
    /// Create a new GestureDetector render object.
    pub fn new() -> Self {
        Self {
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }
}
```

with:

```rust
impl GestureDetectorRenderObject {
    /// Create a new GestureDetector render object with the default layout.
    pub fn new() -> Self {
        Self::new_with_layout(
            Layout::default()
                .flex_direction(FlexDirection::Column)
                .align(AlignItems::Stretch),
        )
    }

    /// Create a new GestureDetector render object with a specific layout.
    pub fn new_with_layout(layout: Layout) -> Self {
        Self {
            child: None,
            computed_bounds: None,
            layout_node: None,
            layout,
        }
    }
}
```

- [ ] **Step 15: Update `RenderObject::layout` to use `self.layout`**

Replace the `layout` method body (lines 412-434):

```rust
fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
    let layout = Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch);
    match self.layout_node {
        Some(existing) => {
            ctx.engine().set_style(existing, &layout);
            ctx.engine().set_children(existing, child_nodes);
            LayoutResult {
                node: existing,
                size: Size::zero(),
            }
        }
        None => {
            let node = ctx.engine().create_container(&layout, child_nodes);
            self.layout_node = Some(node);
            LayoutResult {
                node,
                size: Size::zero(),
            }
        }
    }
}
```

with:

```rust
fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
    match self.layout_node {
        Some(existing) => {
            ctx.engine().set_style(existing, &self.layout);
            ctx.engine().set_children(existing, child_nodes);
            LayoutResult {
                node: existing,
                size: Size::zero(),
            }
        }
        None => {
            let node = ctx.engine().create_container(&self.layout, child_nodes);
            self.layout_node = Some(node);
            LayoutResult {
                node,
                size: Size::zero(),
            }
        }
    }
}
```

- [ ] **Step 16: Build to verify the framework change compiles**

Run: `cargo build -p vexo`
Expected: compiles with no errors. If errors, fix before proceeding.

- [ ] **Step 17: Write the failing test for the custom-layout GestureDetector**

Add this test to the `#[cfg(test)] mod tests` block in `vexo/src/widgets/gesture_detector.rs` (after the existing `test_gesture_detector_clone` test, before the closing `}` of the mod):

```rust
#[test]
fn test_gesture_detector_with_custom_layout_stores_layout() {
    let layout = Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch)
        .flex_grow(1.0);
    let gd = GestureDetector::new(Text::new("Slot"))
        .with_layout(layout.clone());

    assert_eq!(gd.layout, layout, "with_layout must store the layout");
    assert_eq!(gd.layout.flex_grow, Some(1.0));
    assert_eq!(gd.layout.flex_direction, Some(FlexDirection::Column));
    assert_eq!(gd.layout.align_items, Some(AlignItems::Stretch));
}

#[test]
fn test_gesture_detector_default_layout_is_column_stretch() {
    let gd = GestureDetector::new(Text::new("Default"));
    assert_eq!(gd.layout.flex_direction, Some(FlexDirection::Column));
    assert_eq!(gd.layout.align_items, Some(AlignItems::Stretch));
    assert_eq!(gd.layout.flex_grow, None, "default must not set flex_grow");
}

#[test]
fn test_gesture_detector_clone_preserves_custom_layout() {
    let layout = Layout::default()
        .flex_direction(FlexDirection::Row)
        .flex_grow(2.0);
    let gd = GestureDetector::new(Text::new("Clone Me"))
        .with_layout(layout);
    let cloned = gd.clone();
    assert_eq!(cloned.layout.flex_direction, Some(FlexDirection::Row));
    assert_eq!(cloned.layout.flex_grow, Some(2.0));
}

#[test]
fn test_gesture_detector_render_object_uses_custom_layout() {
    let layout = Layout::default()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch)
        .flex_grow(1.0);
    let ro = GestureDetectorRenderObject::new_with_layout(layout.clone());
    assert_eq!(ro.layout, layout, "RO must store the layout passed to new_with_layout");
    assert_eq!(ro.layout.flex_grow, Some(1.0));
}
```

- [ ] **Step 18: Run the new tests to verify they pass**

Run: `cargo test -p vexo --lib gesture_detector::tests::test_gesture_detector_with_custom_layout gesture_detector::tests::test_gesture_detector_default_layout_is_column_stretch gesture_detector::tests::test_gesture_detector_clone_preserves_custom_layout gesture_detector::tests::test_gesture_detector_render_object_uses_custom_layout -- --nocapture`
Expected: all 4 tests PASS.

- [ ] **Step 19: Run the full gesture_detector test module to verify no regressions**

Run: `cargo test -p vexo --lib gesture_detector -- --nocapture`
Expected: all gesture_detector tests PASS (existing + 4 new).

- [ ] **Step 20: Run the full vexo test suite to verify no framework regressions**

Run: `cargo test -p vexo --lib`
Expected: all tests PASS. If any pre-existing test fails, investigate — the default-layout change should be behavior-preserving, so failures likely indicate the test was depending on pass-through internals in a way that needs checking.

- [ ] **Step 21: Commit**

```bash
git add vexo/src/widgets/gesture_detector.rs vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "feat(gesture_detector): public widget + configurable Layout for flex participation"
```

---

## Task 2: Rebuild `TabBarView`'s items row with equal-width 49pt slots

Now that `GestureDetector` can take `flex_grow` and `justify(Center)`, rebuild the tab bar row to use equal-width slots with a fixed 49pt height.

**Files:**
- Modify: `vexo_uikit/src/tab_bar.rs:14-19` (imports — add `GestureDetector`, `JustifyContent`)
- Modify: `vexo_uikit/src/tab_bar.rs:152-215` (`TabBarView::render` items-row + bar assembly)
- Test: new tests in `vexo_uikit/src/tab_bar.rs:219` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `GestureDetector::new(content).on_press(cb).with_layout(Layout).boxed()` from Task 1
- Produces: `TabBarView::render` now emits a 49pt `Flex::row()` of equal-width tappable slots. No public API change — `TabBarView::new` signature is unchanged.

- [ ] **Step 1: Read the current `TabBarView::render` to confirm exact lines**

Run: `Read vexo_uikit/src/tab_bar.rs offset=14 limit=6` (imports) and `Read vexo_uikit/src/tab_bar.rs offset=149 limit=70` (render body)

Expected: imports at lines 14-19 (`use vexo::layout::{FlexDirection, JustifyContent};` and `use vexo::{Component, ...}`); render body at lines 152-215 with the `Flex::row().layout(Layout::default().justify(JustifyContent::SpaceBetween).width_percent(1.0))` bar at line 166, the `for tab in &self.tabs` loop at 171, the `(self.tab_bar_builder)(tab, is_selected).on_press(move || ctrl.switch_to(tab_clone.clone()))` at 175-176, and the SafeArea/hairline wrapping at 190-203.

- [ ] **Step 2: Update imports — add `GestureDetector` to the `use vexo::{...}` block**

Replace the imports at lines 16-19:

```rust
use vexo::{
    Component, ComponentState, Flex, IndexedStack, Layout, LifecycleContext, RenderContext,
    SafeArea, Text, Theme, Widget,
};
```

with:

```rust
use vexo::{
    Component, ComponentState, Flex, GestureDetector, IndexedStack, Layout, LifecycleContext,
    RenderContext, SafeArea, Text, Theme, Widget,
};
```

(`JustifyContent` is already imported at line 14 — confirm; if not, add it to the `use vexo::layout::{...}` line.)

- [ ] **Step 3: Rebuild the items row with equal-width slots and fixed 49pt height**

In `vexo_uikit/src/tab_bar.rs`, replace the items-row construction in `TabBarView::render` (lines 166-178):

```rust
let mut bar = Flex::row().layout(
    Layout::default()
        .justify(JustifyContent::SpaceBetween)
        .width_percent(1.0),
);
for tab in &self.tabs {
    let is_selected = *tab == self.controller.current();
    let ctrl = self.controller.clone();
    let tab_clone = tab.clone();
    let item = (self.tab_bar_builder)(tab, is_selected)
        .on_press(move || ctrl.switch_to(tab_clone.clone()));
    bar = bar.push(item);
}
```

with:

```rust
let mut bar = Flex::row()
    .layout(Layout::default().width_percent(1.0))
    .height(49.0);
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
```

Notes:
- `AlignItems` must be in scope. Add it to the `use vexo::layout::{...}` import at line 14 if not present: `use vexo::layout::{AlignItems, FlexDirection, JustifyContent};`
- `Flex::row()` already defaults to `AlignItems::Stretch` (`vexo/src/widgets/container.rs:50`), so each GestureDetector stretches to 49pt height without an explicit `align` on the bar.
- The SafeArea wrapping and hairline (lines 190-203) stay unchanged.

- [ ] **Step 4: Build to verify the tab_bar change compiles**

Run: `cargo build -p vexo_uikit`
Expected: compiles with no errors. Fix any import errors before proceeding.

- [ ] **Step 5: Run the existing tab_bar tests to check for regressions**

Run: `cargo test -p vexo_uikit --lib tab_bar -- --nocapture`
Expected: existing tests pass. `test_tab_bar_top_hairline_paints` uses a 400×600 window and asserts a 390+px-wide hairline at the seam — this still holds (the hairline is unaffected by the items-row change). `test_tab_bar_view_renders_active_page` checks the element count and a switch — still holds.

- [ ] **Step 6: Write the failing test for slot geometry (equal widths, full height)**

Add this test to the `#[cfg(test)] mod tests` block in `vexo_uikit/src/tab_bar.rs` (after `test_tab_bar_view_renders_active_page`):

```rust
#[test]
fn test_tab_bar_items_are_equal_width_full_height_slots() {
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::widgets::gesture_detector::GestureDetectorRenderObject;
    use vexo::ThreeTreePipeline;

    let ctrl = TabController::new(TestTab::A);
    let view = TabBarView::new(
        ctrl,
        vec![TestTab::A, TestTab::B, TestTab::C],
        |tab| match tab {
            TestTab::A => Text::new("Page A").boxed(),
            TestTab::B => Text::new("Page B").boxed(),
            TestTab::C => Text::new("Page C").boxed(),
        },
        |tab, _| match tab {
            TestTab::A => Text::new("A").boxed(),
            TestTab::B => Text::new("B").boxed(),
            TestTab::C => Text::new("C").boxed(),
        },
    );
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(Box::new(view));
    let mut engine = vexo::layout::TaffyLayoutEngine::new();
    let mut font_system = vexo::resource::new_font_system();
    pipeline.layout(
        vexo::core::Size::new(390.0, 600.0),
        &mut engine,
        &mut font_system,
    );

    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("should have root");

    fn find_gd_bounds(
        ro_reg: &vexo::RenderObjectRegistry,
        id: vexo::RenderObjectKey,
        out: &mut Vec<vexo::core::Bounds<vexo::core::Logical>>,
    ) {
        if let Some(ro) = ro_reg.get(id) {
            if ro.as_any()
                .downcast_ref::<GestureDetectorRenderObject>()
                .is_some()
            {
                if let Some(b) = ro.computed_bounds() {
                    out.push(b);
                }
            }
            for &c in ro.children() {
                find_gd_bounds(ro_reg, c, out);
            }
        }
    }

    let mut gd_bounds = Vec::new();
    find_gd_bounds(ro_reg, root, &mut gd_bounds);
    assert_eq!(
        gd_bounds.len(),
        3,
        "expected 3 tab-item GestureDetectors, found {}",
        gd_bounds.len()
    );

    // Sort by left so slot order is A, B, C.
    gd_bounds.sort_by(|a, b| a.left.partial_cmp(&b.left).unwrap());

    // Each slot must be 390/3 = 130 wide and 49 tall.
    for (i, b) in gd_bounds.iter().enumerate() {
        assert!(
            (b.width() - 130.0).abs() < 1.0,
            "slot {} width {} should be ~130 (390/3)",
            i,
            b.width()
        );
        assert!(
            (b.height() - 49.0).abs() < 1.0,
            "slot {} height {} should be ~49",
            i,
            b.height()
        );
        assert!(
            (b.left - (i as f32) * 130.0).abs() < 1.0,
            "slot {} left {} should be ~{}",
            i,
            b.left,
            i * 130
        );
    }

    // No dead space: widths sum to bar width.
    let total: f32 = gd_bounds.iter().map(|b| b.width()).sum();
    assert!(
        (total - 390.0).abs() < 1.0,
        "slot widths sum {} should be ~390 (no dead space)",
        total
    );
}
```

Note: `TestTab` currently only has `A` and `B` variants (see `vexo_uikit/src/tab_bar.rs:224-228`). Add a `C` variant in Step 7. Also `GestureDetectorRenderObject` must be reachable — check it's `pub` (it is, per `vexo/src/widgets/gesture_detector.rs:388`).

- [ ] **Step 7: Add `TestTab::C` variant for the 3-slot test**

In `vexo_uikit/src/tab_bar.rs`, find the `TestTab` enum (around line 224):

```rust
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
enum TestTab {
    A,
    B,
}
```

Replace with:

```rust
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
enum TestTab {
    A,
    B,
    C,
}
```

- [ ] **Step 8: Run the slot-geometry test to verify it passes**

Run: `cargo test -p vexo_uikit --lib tab_bar::tests::test_tab_bar_items_are_equal_width_full_height_slots -- --nocapture`
Expected: PASS. If it fails on bounds not being set, the layout pass may not have applied — confirm `pipeline.layout` was called (it is, in the test).

- [ ] **Step 9: Write the failing test for the bigger tap target (tapping between icons still selects the slot)**

Add this test to the same `#[cfg(test)] mod tests` block:

```rust
#[test]
fn test_tab_bar_tap_between_icons_selects_slot() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use vexo::animation::AnimationTicker;
    use vexo::input::{ButtonState, InputEvent, PointerButton};
    use vexo::platform::stub_clipboard::StubClipboard;
    use vexo::ThreeTreePipeline;

    let ctrl = TabController::new(TestTab::B);
    let ctrl_for_view = ctrl.clone();
    let view = TabBarView::new(
        ctrl_for_view,
        vec![TestTab::A, TestTab::B, TestTab::C],
        |tab| match tab {
            TestTab::A => Text::new("Page A").boxed(),
            TestTab::B => Text::new("Page B").boxed(),
            TestTab::C => Text::new("Page C").boxed(),
        },
        |tab, _| match tab {
            TestTab::A => Text::new("A").boxed(),
            TestTab::B => Text::new("B").boxed(),
            TestTab::C => Text::new("C").boxed(),
        },
    );
    let mut pipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.update(Box::new(view));
    let mut engine = vexo::layout::TaffyLayoutEngine::new();
    let mut font_system = vexo::resource::new_font_system();
    pipeline.layout(
        vexo::core::Size::new(390.0, 600.0),
        &mut engine,
        &mut font_system,
    );

    // Find the y of the bar (top of the first GestureDetector slot).
    let ro_reg = pipeline.render_objects();
    let root = ro_reg.root().expect("should have root");
    use vexo::widgets::gesture_detector::GestureDetectorRenderObject;
    fn find_first_gd_top(
        ro_reg: &vexo::RenderObjectRegistry,
        id: vexo::RenderObjectKey,
    ) -> Option<f32> {
        if let Some(ro) = ro_reg.get(id) {
            if ro.as_any()
                .downcast_ref::<GestureDetectorRenderObject>()
                .is_some()
            {
                return ro.computed_bounds().map(|b| b.top);
            }
            for &c in ro.children() {
                if let Some(t) = find_first_gd_top(ro_reg, c) {
                    return Some(t);
                }
            }
        }
        None
    }
    let bar_top = find_first_gd_top(ro_reg, root).expect("should find a GestureDetector");
    let tap_y = bar_top + 24.5; // middle of the 49pt slot

    // Tap at x=110 (inside slot 0's 0..130 range, but well off the "A" icon
    // which is centered at x~65). Before this change, this position was in
    // dead space between the SpaceBetween-spread items and did nothing.
    let tap_x = 110.0;
    let event = InputEvent::PointerButton {
        position: vexo::core::Point::new(tap_x, tap_y),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    let clipboard: Arc<dyn vexo::platform::Clipboard> = Arc::new(StubClipboard);
    pipeline.handle_event(
        vexo::core::Point::new(tap_x, tap_y),
        &event,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &vexo::core::ScaleSource::default(),
        &clipboard,
    );

    assert_eq!(
        ctrl.current(),
        TestTab::A,
        "tapping at x=110 (slot 0, off-icon) must select tab A"
    );

    // Symmetric check: tap at x=250 (inside slot 2's 260..390 range, off-icon)
    // must select tab C.
    let tap_x = 250.0;
    let event = InputEvent::PointerButton {
        position: vexo::core::Point::new(tap_x, tap_y),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    pipeline.handle_event(
        vexo::core::Point::new(tap_x, tap_y),
        &event,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &vexo::core::ScaleSource::default(),
        &clipboard,
    );
    assert_eq!(
        ctrl.current(),
        TestTab::C,
        "tapping at x=250 (slot 1/2 boundary region) must select the slot containing x=250"
    );
}
```

Wait — at x=250, the slot boundaries are 0..130 (A), 130..260 (B), 260..390 (C). x=250 is inside slot 1 (B), not slot 2. Fix the assertion: tapping at x=250 selects B.

Correct the second assertion block:

```rust
    // Tap at x=250 (inside slot 1's 130..260 range, off-icon).
    // Must select tab B (the slot owning x=250).
    let tap_x = 250.0;
    let event = InputEvent::PointerButton {
        position: vexo::core::Point::new(tap_x, tap_y),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
    };
    pipeline.handle_event(
        vexo::core::Point::new(tap_x, tap_y),
        &event,
        vexo::input::Modifiers::default(),
        &mut font_system,
        &vexo::core::ScaleSource::default(),
        &clipboard,
    );
    assert_eq!(
        ctrl.current(),
        TestTab::B,
        "tapping at x=250 (slot 1, off-icon) must select tab B"
    );
```

Use the corrected assertion in the test.

- [ ] **Step 10: Run the tap-target test to verify it passes**

Run: `cargo test -p vexo_uikit --lib tab_bar::tests::test_tab_bar_tap_between_icons_selects_slot -- --nocapture`
Expected: PASS. If it fails with "current is still B" after the first tap, the hit-test isn't reaching the GestureDetector — check that `find_first_gd_top` returned the right y (the bar should be near the bottom of the 600px window).

- [ ] **Step 11: Run the full tab_bar test module to verify all tests pass together**

Run: `cargo test -p vexo_uikit --lib tab_bar -- --nocapture`
Expected: all tests PASS (existing 5 + 2 new).

- [ ] **Step 12: Run the full vexo_uikit test suite**

Run: `cargo test -p vexo_uikit --lib`
Expected: all tests PASS.

- [ ] **Step 13: Commit**

```bash
git add vexo_uikit/src/tab_bar.rs
git commit -m "feat(tab_bar): equal-width 49pt slots with full-slot tap targets"
```

---

## Task 3: Drop redundant content padding in the demo app's `tab_bar_builder`

The 49pt slot + `justify(Center)` now handles vertical centering; the old `.padding(8.0)` would double-pad. This is a tiny change but must be done after Task 2 so the slot is already providing centering.

**Files:**
- Modify: `shared_app/src/app.rs:55-73` (the `tab_bar_builder` closure)

**Interfaces:**
- Consumes: `TabBarView` from Task 2 (now with 49pt slots + `justify(Center)`)
- Produces: the demo app's tab items render without redundant padding, centered vertically by the slot.

- [ ] **Step 1: Read the current `tab_bar_builder` to confirm exact lines**

Run: `Read shared_app/src/app.rs offset=55 limit=20`

Expected: see the closure at lines 55-73 with `Column::new().gap(2.0).align(AlignItems::Center).push(Icon...).push(Text...).boxed().padding(8.0)`.

- [ ] **Step 2: Drop the trailing `.padding(8.0)`**

In `shared_app/src/app.rs`, replace the closure body (lines 55-73):

```rust
            |tab, is_selected| {
                let (icon, label) = match tab {
                    ImTab::Chats => (Icons::Comment, "Chats"),
                    ImTab::Contacts => (Icons::User, "Contacts"),
                    ImTab::Me => (Icons::Gear, "Me"),
                };
                let color = if is_selected {
                    Color::rgb(0.0, 0.5, 1.0)
                } else {
                    Color::rgb(0.5, 0.5, 0.5)
                };
                Column::new()
                    .gap(2.0)
                    .align(AlignItems::Center)
                    .push(Icon::new(icon).with_size(22.0).with_color(color))
                    .push(Text::new(label).with_font_size(11.0).with_color(color))
                    .boxed()
                    .padding(8.0)
            },
```

with:

```rust
            |tab, is_selected| {
                let (icon, label) = match tab {
                    ImTab::Chats => (Icons::Comment, "Chats"),
                    ImTab::Contacts => (Icons::User, "Contacts"),
                    ImTab::Me => (Icons::Gear, "Me"),
                };
                let color = if is_selected {
                    Color::rgb(0.0, 0.5, 1.0)
                } else {
                    Color::rgb(0.5, 0.5, 0.5)
                };
                Column::new()
                    .gap(2.0)
                    .align(AlignItems::Center)
                    .push(Icon::new(icon).with_size(22.0).with_color(color))
                    .push(Text::new(label).with_font_size(11.0).with_color(color))
                    .boxed()
            },
```

- [ ] **Step 3: Build the demo app to verify it compiles**

Run: `cargo build -p shared_app`
Expected: compiles with no errors.

- [ ] **Step 4: Run shared_app tests to verify no regressions**

Run: `cargo test -p shared_app`
Expected: all tests PASS. (If shared_app has integration tests that asserted on the old padding, update them — but none are expected to assert on tab-item padding.)

- [ ] **Step 5: Commit**

```bash
git add shared_app/src/app.rs
git commit -m "refactor(shared_app): drop redundant tab-item padding (slot now centers)"
```

---

## Task 4: Final verification and runtime handoff

Verify the whole change end-to-end before handing off to the user for runtime confirmation.

**Files:** none modified.

- [ ] **Step 1: Build the entire workspace**

Run: `cargo build --workspace`
Expected: compiles with no errors.

- [ ] **Step 2: Run the entire workspace test suite**

Run: `cargo test --workspace`
Expected: all tests PASS. If any test outside vexo/vexo_uikit/shared_app fails, investigate — but the change is scoped to these three crates.

- [ ] **Step 3: Run clippy on the changed crates**

Run: `cargo clippy -p vexo -p vexo_uikit -p shared_app -- -D warnings`
Expected: no warnings. If warnings appear, fix them (common: unused import after removing `JustifyContent` from tab_bar — but `JustifyContent` is still used in the new bar layout via `.justify(JustifyContent::Center)`, so it should still be needed; if not, remove the unused import).

- [ ] **Step 4: Verify the spec's requirements are met by the tests**

Confirm by re-reading the test bodies:
- ✅ Bigger click area (no dead space): `test_tab_bar_items_are_equal_width_full_height_slots` asserts widths sum to 390.
- ✅ Full-slot tap target: `test_tab_bar_tap_between_icons_selects_slot` asserts tapping off-icon positions still selects the correct slot.
- ✅ Fixed 49pt height: `test_tab_bar_items_are_equal_width_full_height_slots` asserts each slot is 49pt tall.
- ✅ Visual edge gap (content centered): emerges from `align(Center)` on the content Column + `justify(Center)` on the slot — not directly asserted in a unit test (it's a visual property), but verified at runtime in Step 5.

- [ ] **Step 5: Hand off to the user for runtime verification**

Per `CLAUDE.md`, do NOT run `cargo run -p desktop_demo` from the agent session. Instead, present the user with the run command and what to check:

**Message to user:**

> The tab bar change is complete and all unit tests pass. Please run the demo to verify visually:
>
> ```
> cargo run -p desktop_demo
> ```
>
> Check:
> 1. Tab bar is 49pt tall (shorter than before).
> 2. Three tabs are evenly spaced — each owns an equal third of the bar width.
> 3. The Chats/Contacts/Me icons+labels appear centered within each slot, with visible breathing room from the left/right screen edges.
> 4. **Tap between two icons** (in the dead space that used to do nothing) — the nearest tab should now select. Tap at the very left/right edge of the window — the first/last tab should still select.
> 5. Switching tabs still works as before.

Wait for the user's confirmation. If they report issues, debug per the `debugging-gui-with-logs` skill (add `log::debug!` with a unique prefix, give them a `RUST_LOG=debug | grep | tee` command, read the log, fix root cause). Do not theorize without log evidence.

- [ ] **Step 6: Final commit (if any fixups were needed from runtime feedback)**

If runtime feedback required changes, commit them:

```bash
git add -A
git commit -m "fix(tab_bar): <describe fix from runtime feedback>"
```

Otherwise, no commit — the work is complete at Step 5.

---

## Self-Review Notes

**Spec coverage:**
- ✅ Framework change (GestureDetector public + Layout field) — Task 1
- ✅ Tab bar equal-width 49pt slots — Task 2
- ✅ Drop redundant content padding — Task 3
- ✅ Framework unit test (custom layout) — Task 1 Step 17
- ✅ Tab bar slot-geometry test — Task 2 Step 6
- ✅ Tab bar tap-target test — Task 2 Step 9
- ✅ Hairline still paints — covered by existing `test_tab_bar_top_hairline_paints` (Task 2 Step 5)
- ✅ Active page renders — covered by existing `test_tab_bar_view_renders_active_page` (Task 2 Step 5)
- ✅ Runtime verification — Task 4 Step 5

**Placeholder scan:** No TBDs, TODOs, or "add error handling" — every step has complete code.

**Type consistency:**
- `GestureDetector` made public — Task 1 Steps 3-5; used in Task 2 Step 2 (import) + Step 3 (construction). ✓
- `GestureDetectorRenderObject` accessible at `vexo::widgets::gesture_detector::GestureDetectorRenderObject` — Task 1 Step 3 (pub mod); used in Task 2 Steps 6 + 9 tests. ✓
- `GestureDetector::with_layout(Layout) -> Self` — defined Task 1 Step 9, used Task 2 Step 3. ✓
- `GestureDetectorRenderObject::new_with_layout(Layout) -> Self` — defined Task 1 Step 14, used Task 1 Step 11. ✓
- `GestureDetectorRenderObject.layout` field — added Task 1 Step 13, read Task 1 Step 15 + Step 17 test. ✓
- `TestTab::C` — added Task 2 Step 7, used Task 2 Steps 6 + 9. ✓

**Scope check:** Single focused change across three crates, one feature. No decomposition needed.
