# DecoratedBox vs DecoratedContainer Split — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `DecoratedBox` widget (decoration-only, true layout proxy) alongside the unchanged `DecoratedContainer`, extract a `paint_style()` helper for DRY decoration painting, and rewire `WidgetExt`'s `.background/.border/.corner_radius/.clip` to use `DecoratedBox`.

**Architecture:** `DecoratedBox` = widget + element + `DecoratedBoxRenderObject` (a true pass-through proxy like `ProxyRenderObject` that additionally paints `Style` against its `computed_bounds`). Decoration painting logic is extracted from `ContainerRenderObject::paint()` into a free `paint_style()` function in `painter.rs`, used by both `ContainerRenderObject` and `DecoratedBoxRenderObject`. `DecoratedContainer` is unchanged.

**Tech Stack:** Rust, Taffy 0.9.1 (layout), wgpu 27.0.1 (rendering), existing `cargo test -p vexo --lib` runner.

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-07-19-decorated-box-split-design.md` — every requirement there must be satisfied.
- **Existing tests must remain green.** All `cargo test -p vexo --lib` and `cargo test -p vexo_uikit` runs must pass before committing.
- **`DecoratedContainer` is unchanged.** No edits to `vexo/src/widgets/decorated_container.rs`. Its existing callers in `vexo_uikit/`, `shared_app/`, and `vexo/src/e2e_test.rs` must keep working without modification.
- **No new dependencies.** Uses only existing crate infrastructure (`ProxyRenderObject` pattern, `ContainerRenderObject`, `RenderObjectElement` trait, `painter.rs`).
- **Clip handling:** `style.clip` is NOT painted inside `paint()` — it's exposed via `RenderObject::clip_bounds()` and the painter pushes `PushClip`/`PopClip` automatically (see `vexo/src/render_objects/container.rs:263-269`). `DecoratedBoxRenderObject` must implement `clip_bounds()` the same way.
- **Public API:** `DecoratedBox` is exported from `vexo/src/lib.rs` at top level (alongside `DecoratedContainer`). `DecoratedBoxRenderObject` is `pub` in `vexo/src/render_objects/mod.rs` but not re-exported at top level (matches `ContainerRenderObject`'s visibility).
- **Commit message style:** `feat(decorated-box): ...`, `refactor(painter): ...`, `test(decorated-box): ...`, `docs(decorated-box): ...` — match existing repo style (see `git log --oneline -20`).

---

## File Structure

| File | Status | Responsibility |
|---|---|---|
| `vexo/src/painter.rs` | Modify | Add `paint_style()` free function; refactor `ContainerRenderObject::paint()` to use it |
| `vexo/src/render_objects/container.rs` | Modify | `ContainerRenderObject::paint()` becomes a one-liner delegating to `paint_style()` |
| `vexo/src/render_objects/decorated_box.rs` | **Create** | `DecoratedBoxRenderObject` + unit tests |
| `vexo/src/render_objects/mod.rs` | Modify | Add `mod decorated_box;` + `pub use decorated_box::DecoratedBoxRenderObject;` |
| `vexo/src/widgets/decorated_box.rs` | **Create** | `DecoratedBox` widget + `DecoratedBoxElement` + unit tests |
| `vexo/src/widgets/mod.rs` | Modify | Add `mod decorated_box;` + `pub use decorated_box::DecoratedBox;`; rewire 4 `WidgetExt` methods; add routing test |
| `vexo/src/lib.rs` | Modify | Re-export `DecoratedBox` at top level |
| `vexo/src/e2e_test.rs` | Modify | Add `test_decorated_box_in_pipeline` + `test_decorated_box_width_propagates_to_child` |

---

## Task 1: Extract `paint_style()` helper

**Files:**
- Modify: `vexo/src/painter.rs`
- Modify: `vexo/src/render_objects/container.rs:145-222` (the existing `paint()` body)

**Interfaces:**
- Produces: `pub(crate) fn paint_style(style: &Style, bounds: Bounds<Logical>, ctx: &mut PaintContext) -> Vec<RenderCommand>` in `vexo/src/painter.rs`. This is consumed by Task 3 (`DecoratedBoxRenderObject::paint()`) and by `ContainerRenderObject::paint()` (modified in this task).

- [ ] **Step 1: Read the existing `ContainerRenderObject::paint()` body**

Read `vexo/src/render_objects/container.rs:145-222`. The body computes `absolute_bounds` from `ctx.absolute_position()` + `bounds`, then emits (in order): shadows → `PushCornerRadius` (if set) → background rect → border rect → `PopCornerRadius` (if set). This is the exact code that will move to `paint_style()`.

- [ ] **Step 2: Add `paint_style()` free function to `painter.rs`**

Open `vexo/src/painter.rs`. At the top of the file, ensure these imports exist (add any missing):

```rust
use crate::core::{Absolute, Bounds, Logical, Position, Relative};
use crate::render::RenderCommand;
use crate::style::Style;
```

(Note: `Absolute`, `Logical`, `Position`, `Relative`, `RenderCommand` are already imported at lines 6-7 — verify before adding.)

Below the `use` statements and above `pub struct Painter;` (line 17), add the free function:

```rust
/// Paint decoration commands (background, border, corner radius, shadows)
/// for a `Style` at the given bounds.
///
/// This is the single source of truth for decoration painting — used by both
/// `ContainerRenderObject::paint()` and `DecoratedBoxRenderObject::paint()`.
/// The caller's `computed_bounds` provide the local bounds; the paint context's
/// `absolute_position()` provides the origin.
///
/// Note: `style.clip` is NOT handled here — it's exposed via
/// `RenderObject::clip_bounds()` and the painter pushes `PushClip`/`PopClip`
/// automatically around the RO's children.
pub(crate) fn paint_style(
    style: &Style,
    bounds: Bounds<Logical>,
    ctx: &mut PaintContext,
) -> Vec<RenderCommand> {
    let pos: Position<Logical, Absolute> = ctx.absolute_position();

    let absolute_bounds = Bounds::new(
        pos.x,
        pos.y,
        pos.x + bounds.width(),
        pos.y + bounds.height(),
    );

    let base_corner_radius = style
        .corner_radius
        .as_ref()
        .map(|cr| cr.radius)
        .unwrap_or(0.0);

    let mut commands = Vec::new();

    // 1. Emit shadows BEFORE fill/border (shadows draw behind everything).
    // Shadows bypass PushCornerRadius context — each shadow Rect carries its
    // own corner_radius field (computed as base + spread).
    // Shadows also bypass style.clip's PushClip — clipping the shadow to the
    // very shape casting it would make it invisible.
    for shadow in &style.shadows {
        if shadow.color.a == 0.0 {
            continue;
        }
        let blur = shadow.blur_radius.max(0.0);
        let pad = blur + shadow.spread_radius;
        let shadow_bounds = Bounds::new(
            absolute_bounds.left + shadow.offset.x - pad,
            absolute_bounds.top + shadow.offset.y - pad,
            absolute_bounds.right + shadow.offset.x + pad,
            absolute_bounds.bottom + shadow.offset.y + pad,
        );
        let shadow_corner_radius = (base_corner_radius + shadow.spread_radius).max(0.0);
        commands.push(RenderCommand::Rect {
            bounds: shadow_bounds,
            fill: shadow.color,
            stroke: None,
            corner_radius: shadow_corner_radius,
            shadow_color: shadow.color.to_array(),
            shadow_blur: blur,
        });
    }

    // 2. Push corner radius if set (affects fill/border only, NOT shadows)
    if let Some(ref cr) = style.corner_radius {
        commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
    }

    // 3. Draw background first (behind child)
    if let Some(bg_color) = style.background {
        commands.push(RenderCommand::rect(absolute_bounds, bg_color));
    }

    // 4. Draw border on top (after background)
    if let Some(ref border) = style.border {
        commands.push(RenderCommand::rect_with_border(
            absolute_bounds,
            crate::core::Color::TRANSPARENT,
            border.color,
            border.width,
        ));
    }

    // 5. Pop corner radius
    if style.corner_radius.is_some() {
        commands.push(RenderCommand::PopCornerRadius);
    }

    commands
}
```

- [ ] **Step 3: Refactor `ContainerRenderObject::paint()` to delegate to `paint_style()`**

In `vexo/src/render_objects/container.rs`, replace the body of `fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand>` (lines 145-222) with:

```rust
fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
    let bounds = match &self.computed_bounds {
        Some(b) => *b,
        None => return vec![],
    };
    crate::painter::paint_style(&self.style, bounds, ctx)
}
```

Add the import at the top of the file if not already present (the file already imports `Bounds`, `Color`, `Position`, `Logical`, `Absolute` — check before adding):

```rust
// No new imports needed — paint_style is reached via crate::painter:: path.
```

- [ ] **Step 4: Run existing container render object tests to verify no regression**

Run: `cargo test -p vexo --lib render_objects::container::tests`
Expected: PASS — all existing tests (`test_container_paint_with_background`, `test_container_paint_with_border`, `test_container_paint_with_background_and_border`, `test_container_paint_with_corner_radius`, `test_container_paint_empty_style`, `test_container_paint_no_style`, `test_container_paint_no_bounds`, etc.) pass unchanged.

- [ ] **Step 5: Run full vexo lib test suite to verify no other regression**

Run: `cargo test -p vexo --lib`
Expected: PASS — all tests pass. The `painter` module is referenced from `container.rs` via `crate::painter::paint_style`, which is `pub(crate)` so the path resolves.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/painter.rs vexo/src/render_objects/container.rs
git commit -m "refactor(painter): extract paint_style() helper from ContainerRenderObject"
```

---

## Task 2: Create `DecoratedBoxRenderObject`

**Files:**
- Create: `vexo/src/render_objects/decorated_box.rs`
- Modify: `vexo/src/render_objects/mod.rs`

**Interfaces:**
- Consumes: `crate::painter::paint_style` (from Task 1), `crate::layout::{Layout, LayoutNodeKey, LayoutEngine}`, `crate::core::{Bounds, Logical, Point, Size}`, `crate::render_object::{RenderObject, LayoutContext, LayoutResult, HitTestContext, PaintContext, RenderObjectKey}`, `crate::render::RenderCommand`, `crate::style::Style`. Reference for the pass-through pattern: `crate::stateful_widget::ProxyRenderObject` (`vexo/src/stateful_widget.rs:862-977`).
- Produces: `pub struct DecoratedBoxRenderObject` with constructor `pub fn new(style: Style) -> Self` and setter `pub fn set_style(&mut self, style: Style) -> bool`. Implements `RenderObject` trait with `is_pass_through() == true`, `layout()` returning child's Taffy node, `paint()` delegating to `paint_style()`, `clip_bounds()` returning `computed_bounds` when `style.clip` is true.

- [ ] **Step 1: Write failing test for `is_pass_through()` and basic construction**

Create `vexo/src/render_objects/decorated_box.rs` with this content (test-first; the implementation will be added in Step 3):

```rust
//! DecoratedBoxRenderObject: a true pass-through proxy that paints `Style`.
//!
//! Like `ProxyRenderObject`, this render object does NOT own a Taffy node —
//! `layout()` returns the child's node so the grandparent links the
//! grandchild directly. Unlike `ProxyRenderObject`, this RO additionally
//! paints `Style` (background, border, corner radius, shadows) against its
//! `computed_bounds` (which equals the child's bounds, since they share the
//! Taffy node).
//!
//! `is_pass_through() == true` tells the painter / hit-tester to apply the
//! pass-through coordinate correction (subtract `position_in_parent` when
//! recursing into the child) and tells `RenderObjectRegistry::remove()` to
//! skip orphan-node cleanup. See `ProxyRenderObject` docstring in
//! `crate::stateful_widget` for the full rationale.

use crate::core::{Absolute, Bounds, Color, Logical, Point, Position, Relative, Size};
use crate::layout::{Layout, LayoutEngine, LayoutNodeKey, LayoutResult};
use crate::render::RenderCommand;
use crate::render_object::{
    HitTestContext, LayoutContext, PaintContext, RenderObject, RenderObjectKey,
};
use crate::style::Style;

/// Render object for `DecoratedBox`. True pass-through proxy that paints `Style`.
///
/// See module docs for details.
pub struct DecoratedBoxRenderObject {
    child: Option<RenderObjectKey>,
    style: Style,
    computed_bounds: Option<Bounds<Logical>>,
    /// The child's Taffy node, returned to the parent so the grandparent
    /// links the grandchild directly. `None` until `layout()` runs.
    child_layout_node: Option<LayoutNodeKey>,
}

impl DecoratedBoxRenderObject {
    /// Create a new `DecoratedBoxRenderObject` with the given style.
    pub fn new(style: Style) -> Self {
        Self {
            child: None,
            style,
            computed_bounds: None,
            child_layout_node: None,
        }
    }

    /// Set the style, returning `true` if it changed.
    ///
    /// Used by `Widget::update_render_object()` to detect whether a paint
    /// invalidation is needed.
    pub fn set_style(&mut self, style: Style) -> bool {
        if self.style != style {
            self.style = style;
            true
        } else {
            false
        }
    }
}

impl Default for DecoratedBoxRenderObject {
    fn default() -> Self {
        Self::new(Style::default())
    }
}

impl RenderObject for DecoratedBoxRenderObject {
    fn layout(
        &mut self,
        ctx: &mut LayoutContext,
        child_nodes: &[LayoutNodeKey],
    ) -> LayoutResult {
        // Pass-through: return the child's node directly. No intervening
        // container — the grandparent links the grandchild's Taffy node.
        //
        // DecoratedBox always has exactly one child. The defensive `None`
        // case creates a throwaway zero-size leaf to avoid panicking on
        // framework edge cases. Mirrors `ProxyRenderObject::layout()`.
        match child_nodes.first() {
            Some(&child_node) => {
                self.child_layout_node = Some(child_node);
                LayoutResult {
                    node: child_node,
                    size: Size::zero(),
                }
            }
            None => {
                let node = ctx.engine().create_leaf(&Layout::default());
                self.child_layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::zero(),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.child_layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn is_pass_through(&self) -> bool {
        true
    }

    fn paint(&self, ctx: &mut PaintContext) -> Vec<RenderCommand> {
        let bounds = match self.computed_bounds {
            Some(b) => b,
            None => return Vec::new(),
        };
        crate::painter::paint_style(&self.style, bounds, ctx)
    }

    fn hit_test(
        &self,
        position: Point<Logical>,
        _ctx: &HitTestContext,
    ) -> bool {
        match self.computed_bounds {
            Some(bounds) => bounds.contains(&position),
            None => false,
        }
    }

    fn children(&self) -> &[RenderObjectKey] {
        match &self.child {
            Some(child) => std::slice::from_ref(child),
            None => &[],
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn replace_child(&mut self, old: RenderObjectKey, new: RenderObjectKey) {
        if self.child == Some(old) {
            self.child = Some(new);
        }
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.child_layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }

    fn clip_bounds(&self) -> Option<Bounds<Logical>> {
        if self.style.clip {
            self.computed_bounds
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Bounds;
    use crate::layout::{LayoutEngine, TaffyLayoutEngine};
    use crate::style::BoxShadow;

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = include_bytes!("../../font.ttf").to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn test_decorated_box_layout_returns_child_node() {
        let mut ro = DecoratedBoxRenderObject::new(Style::default());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        // Create a child Taffy node the way the pipeline would: by calling
        // engine.create_leaf and passing the key as a child_nodes entry.
        let child_node = ctx.engine().create_leaf(&Layout::default().width(50.0).height(50.0));
        let result = ro.layout(&mut ctx, &[child_node]);

        assert_eq!(result.node, child_node, "layout() must return the child's node");
        assert_eq!(
            ro.layout_node(),
            Some(child_node),
            "layout_node() must return the child's node after layout()"
        );
    }

    #[test]
    fn test_decorated_box_layout_no_child_creates_throwaway_node() {
        let mut ro = DecoratedBoxRenderObject::new(Style::default());
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = ro.layout(&mut ctx, &[]);

        // Should not panic; should return some node and store it.
        assert!(ro.layout_node().is_some());
        assert_eq!(ro.layout_node(), Some(result.node));
    }

    #[test]
    fn test_decorated_box_is_pass_through() {
        let ro = DecoratedBoxRenderObject::new(Style::default());
        assert!(ro.is_pass_through(), "DecoratedBoxRenderObject must be pass-through");
    }

    #[test]
    fn test_decorated_box_paint_no_bounds_returns_empty() {
        let ro = DecoratedBoxRenderObject::new(Style::new().background(Color::RED));
        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert!(cmds.is_empty(), "paint() with no computed_bounds must return empty");
    }

    #[test]
    fn test_decorated_box_paint_with_background_emits_one_command() {
        let mut ro = DecoratedBoxRenderObject::new(Style::new().background(Color::RED));
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert_eq!(cmds.len(), 1, "background only → 1 command");
    }

    #[test]
    fn test_decorated_box_paint_with_background_and_border() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);
        let mut ro = DecoratedBoxRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert_eq!(cmds.len(), 2, "background + border → 2 commands");
    }

    #[test]
    fn test_decorated_box_paint_with_corner_radius() {
        let style = Style::new().background(Color::RED).corner_radius(8.0);
        let mut ro = DecoratedBoxRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert_eq!(cmds.len(), 3, "push radius + background + pop radius → 3 commands");
    }

    #[test]
    fn test_decorated_box_paint_empty_style() {
        let mut ro = DecoratedBoxRenderObject::new(Style::new());
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        assert!(cmds.is_empty(), "empty style → 0 commands");
    }

    #[test]
    fn test_decorated_box_paint_with_shadow() {
        let style = Style::new()
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK).blur(8.0));
        let mut ro = DecoratedBoxRenderObject::new(style);
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));

        let mut commands = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        let cmds = ro.paint(&mut ctx);
        // shadow + background = 2 (no corner radius)
        assert_eq!(cmds.len(), 2, "shadow + background → 2 commands");
    }

    #[test]
    fn test_decorated_box_set_style_change_detection() {
        let mut ro = DecoratedBoxRenderObject::new(Style::default());

        // Setting the same default style → no change
        assert!(!ro.set_style(Style::default()));

        // Setting a different style → change
        let style = Style::new().background(Color::RED);
        assert!(ro.set_style(style.clone()));

        // Setting the same style again → no change
        assert!(!ro.set_style(style));
    }

    #[test]
    fn test_decorated_box_clip_bounds_no_clip() {
        let ro = DecoratedBoxRenderObject::new(Style::new());
        assert!(ro.clip_bounds().is_none(), "no clip → clip_bounds() is None");
    }

    #[test]
    fn test_decorated_box_clip_bounds_with_clip_no_bounds() {
        let ro = DecoratedBoxRenderObject::new(Style::new().clip());
        assert!(
            ro.clip_bounds().is_none(),
            "clip set but no computed_bounds → clip_bounds() is None"
        );
    }

    #[test]
    fn test_decorated_box_clip_bounds_with_clip_and_bounds() {
        let mut ro = DecoratedBoxRenderObject::new(Style::new().clip());
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        assert_eq!(
            ro.clip_bounds(),
            Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0)),
            "clip + bounds → clip_bounds() returns the bounds"
        );
    }

    #[test]
    fn test_decorated_box_hit_test_no_bounds() {
        let ro = DecoratedBoxRenderObject::new(Style::default());
        assert!(
            !ro.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()),
            "no bounds → hit_test false"
        );
    }

    #[test]
    fn test_decorated_box_hit_test_inside_bounds() {
        let mut ro = DecoratedBoxRenderObject::new(Style::default());
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        assert!(
            ro.hit_test(Point::new(10.0, 10.0), &HitTestContext::mock()),
            "point inside bounds → hit_test true"
        );
    }

    #[test]
    fn test_decorated_box_hit_test_outside_bounds() {
        let mut ro = DecoratedBoxRenderObject::new(Style::default());
        ro.computed_bounds = Some(Bounds::from_xywh(0.0, 0.0, 100.0, 50.0));
        assert!(
            !ro.hit_test(Point::new(200.0, 200.0), &HitTestContext::mock()),
            "point outside bounds → hit_test false"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails to compile**

Run: `cargo test -p vexo --lib render_objects::decorated_box`
Expected: FAIL — error `unresolved module decorated_box` because the module isn't registered yet.

- [ ] **Step 3: Register the module**

Open `vexo/src/render_objects/mod.rs`. Add `mod decorated_box;` (alphabetical, after `mod container;` on line 12) and `pub use decorated_box::DecoratedBoxRenderObject;` (after `pub use container::ContainerRenderObject;` on line 22).

The file should look like:

```rust
//! RenderObject implementations for the retain rendering system.
//!
//! RenderObjects are the lowest-level building blocks of the framework.
//! They perform layout and painting, and are managed by elements.

mod container;
mod decorated_box;
mod image;
mod indexed_stack;
mod offstage;
mod opacity;
mod positioned;
mod scroll_view;
mod text;
mod text_edit;

pub use container::ContainerRenderObject;
pub use decorated_box::DecoratedBoxRenderObject;
pub use image::ImageRenderObject;
// ... (rest unchanged)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib render_objects::decorated_box`
Expected: PASS — all 14 tests pass.

- [ ] **Step 5: Run full vexo lib test suite**

Run: `cargo test -p vexo --lib`
Expected: PASS — no regressions.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/render_objects/decorated_box.rs vexo/src/render_objects/mod.rs
git commit -m "feat(decorated-box): add DecoratedBoxRenderObject (true pass-through proxy that paints Style)"
```

---

## Task 3: Create `DecoratedBox` widget + element

**Files:**
- Create: `vexo/src/widgets/decorated_box.rs`
- Modify: `vexo/src/widgets/mod.rs` (add module declaration + export)

**Interfaces:**
- Consumes: `crate::render_objects::DecoratedBoxRenderObject` (from Task 2), `crate::style::Style`, `crate::core::Color`, `crate::style::BoxShadow`, `crate::widgets::Widget`/`crate::Element`/`crate::RenderObjectElement`/`crate::ElementContext`/`crate::UpdateResult`/etc., `crate::focus::attachment::FocusAttachment`, `crate::input::InputEvent`, `crate::key::WidgetKey`, `crate::id::{ElementKey, RenderObjectKey}`. Reference template: `vexo/src/widgets/decorated_container.rs` (full file) and `vexo/src/widgets/with_layout.rs` (full file) — both show the same element lifecycle pattern.
- Produces: `pub struct DecoratedBox` with `pub fn new(child: impl Widget + 'static) -> Self` and builder methods `.style(Style)`, `.background(Color)`, `.border(Color, f32)`, `.corner_radius(f32)`, `.clip()`, `.shadow(BoxShadow)`, `.shadows(Vec<BoxShadow>)`, `.with_key(impl Into<WidgetKey>)`. Plus accessors `pub fn style_ref(&self) -> &Style` and `pub fn child(&self) -> &dyn Widget`. Implements `Widget` trait; `update_render_object()` returns `PAINT` only (never `LAYOUT`).

- [ ] **Step 1: Write the failing widget unit tests**

Create `vexo/src/widgets/decorated_box.rs` with the test module first (implementation will be added in Step 3). Use this content — note the test bodies call `DecoratedBox::new(...)`, `.background()`, etc., which don't exist yet, so the file won't compile until Step 3:

```rust
//! DecoratedBox widget — decoration only, no layout opinion.
//!
//! `DecoratedBox` is the Vexo equivalent of Flutter's `DecoratedBox`: it
//! paints a `Style` (background, border, corner radius, shadow, clip) around
//! its child without imposing any layout. The wrapper is a true pass-through
//! proxy (`is_pass_through() == true`) — it does NOT own a Taffy node, so
//! the grandparent links the grandchild directly. The child sizes itself
//! naturally; `DecoratedBox` adopts the child's bounds and paints the
//! decoration there.
//!
//! Contrast with [`DecoratedContainer`](crate::DecoratedContainer), which
//! owns both a `Style` and a `Layout` and defaults to
//! `align_self(Start).flex_shrink(0.0)` (size-to-content). Use
//! `DecoratedContainer` when you want decoration + sizing; use `DecoratedBox`
//! when you want decoration only.
//!
//! # Border semantics
//!
//! `DecoratedBox::border(color, width)` does NOT add padding — the border
//! paints over the child's edge pixels (Flutter semantics). If you want the
//! child inset from the border, compose with [`WithLayout`](crate::WithLayout)
//! padding or use [`DecoratedContainer`](crate::DecoratedContainer) whose
//! `border()` adds padding automatically.

use std::any::Any;

use crate::core::Color;
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::key::WidgetKey;
use crate::render_objects::DecoratedBoxRenderObject;
use crate::style::{BoxShadow, Style};
use crate::{
    Element, ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext,
    LayoutResult, PaintContext, RenderObject, RenderObjectKey, UpdateResult, Widget,
};

// ============================================================================
// DecoratedBoxElement
// ============================================================================

/// Element for `DecoratedBox` widget.
///
/// Manages a single child element and updates the render object when the
/// style changes. Structurally identical to `DecoratedContainerElement`
/// minus the layout bookkeeping — `DecoratedBox` has no `Layout` field, so
/// the element never marks layout dirty (only paint).
pub struct DecoratedBoxElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl DecoratedBoxElement {
    /// Create a new `DecoratedBox` element.
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            render_object: None,
            widget: None,
            focus_attachment: None,
        }
    }

    /// Set the widget for this element.
    pub fn set_widget(&mut self, widget: &dyn Widget) {
        self.widget = Some(widget.clone_boxed());
        self.key = widget.key();
    }

    /// Get the child widget from the stored widget.
    fn get_child_widget(&self) -> Option<&dyn Widget> {
        self.widget.as_ref()?.child()
    }
}

impl Default for DecoratedBoxElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for DecoratedBoxElement {
    fn widget(&self) -> Option<&dyn Widget> {
        self.widget.as_deref()
    }

    fn set_widget(&mut self, widget: Box<dyn Widget>) {
        self.widget = Some(widget);
    }

    fn render_object_id(&self) -> Option<RenderObjectKey> {
        self.render_object
    }

    fn set_render_object_id(&mut self, id: Option<RenderObjectKey>) {
        self.render_object = id;
    }

    fn stored_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn set_stored_key(&mut self, key: Option<WidgetKey>) {
        self.key = key;
    }

    fn element_id(&self) -> Option<ElementKey> {
        self.id
    }

    fn set_element_id(&mut self, id: Option<ElementKey>) {
        self.id = id;
    }
}

impl Element for DecoratedBoxElement {
    fn mount(&mut self, context: &mut ElementContext) {
        // Create focus attachment BEFORE mounting child (same rationale as
        // DecoratedContainerElement::mount): the child looks up this element's
        // focus node as its parent when it mounts.
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context
            .focus_manager()
            .create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        // Use RenderObjectElement's default mount for render object creation
        self.mount_render_object(context);

        // Mount single child via child_ops (emit Inflate command).
        if let Some(widget) = &self.widget {
            if let Some(child_widget) = widget.child() {
                context.inflate_child(None, child_widget.clone_boxed());
            }
        }
    }

    fn update(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        self.update_render_object(new_widget, context);
    }

    fn unmount(&mut self, context: &mut ElementContext) {
        self.unmount_render_object(context);
        if let Some(mut attachment) = self.focus_attachment.take() {
            attachment.detach(context.focus_manager());
        }
    }

    fn render_object(&self) -> Option<RenderObjectKey> {
        self.render_object
    }

    fn widget_key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn can_update(&self, widget: &dyn Any) -> bool {
        self.widget
            .as_ref()
            .map(|old| old.as_any().type_id() == widget.type_id())
            .unwrap_or(false)
    }

    fn on_event(
        &mut self,
        _event: &InputEvent,
        _context: &mut EventContext,
        _state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        // DecoratedBox doesn't handle events itself
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        // Downcast and store the new widget
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.widget = Some(*widget);

            // Update the render object with new properties.
            // DecoratedBoxRenderObject::set_style only returns true on
            // actual change, and update_render_object only returns PAINT
            // (never LAYOUT — proxy has no layout).
            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self
                        .widget
                        .as_ref()
                        .unwrap()
                        .update_render_object(ro.as_mut());

                    if result.contains(UpdateResult::PAINT) {
                        context.mark_needs_paint(ro_id);
                    }
                    // LAYOUT is never returned; no mark_needs_layout call.
                }
            }

            // Reconcile single child via child_ops
            let old_child = context.children().first().copied();
            if let Some(child_widget) = self.get_child_widget() {
                match old_child {
                    Some(old_child_key) => {
                        context.update_child(old_child_key, child_widget.clone_boxed());
                    }
                    None => {
                        context.inflate_child(None, child_widget.clone_boxed());
                    }
                }
            } else if let Some(old_child_key) = old_child {
                context.unmount_child(old_child_key);
            }
        }

        // Reparent focus node if parent changed
        if let Some(attachment) = self.focus_attachment.as_ref() {
            let new_parent_id = context.parent_focus_node_id();
            attachment.reparent_to(new_parent_id, context.focus_manager());
        }
    }

    fn child_mounted(
        &mut self,
        _slot: Option<usize>,
        child_ro: Option<RenderObjectKey>,
        context: &mut ElementContext,
    ) {
        if let Some(child_ro_key) = child_ro {
            self.insert_child_render_object(child_ro_key, context);
        }
    }

    fn focus_attachment(&self) -> &Option<FocusAttachment> {
        &self.focus_attachment
    }

    fn focus_attachment_mut(&mut self) -> &mut Option<FocusAttachment> {
        &mut self.focus_attachment
    }
}

// ============================================================================
// DecoratedBox Widget
// ============================================================================

/// A widget that decorates a child with visual styling, with no layout opinion.
///
/// `DecoratedBox` paints a `Style` (background, border, corner radius,
/// shadow, clip) around its child without imposing any layout. The wrapper
/// is a true pass-through proxy — it does NOT own a Taffy node, so the
/// grandparent links the grandchild directly. The child sizes itself
/// naturally; `DecoratedBox` adopts the child's bounds and paints the
/// decoration there.
///
/// Use `DecoratedBox` when you want decoration only. Use
/// [`DecoratedContainer`](crate::DecoratedContainer) when you want
/// decoration + sized layout.
///
/// # Example
///
/// ```ignore
/// DecoratedBox::new(Text::new("Hello"))
///     .background(Color::RED)
///     .border(Color::BLACK, 2.0)
///     .corner_radius(8.0)
/// ```
///
/// # Border semantics
///
/// `DecoratedBox::border(color, width)` does NOT add padding — the border
/// paints over the child's edge pixels (Flutter semantics). If you want the
/// child inset from the border, compose with [`WithLayout`](crate::WithLayout)
/// padding or use [`DecoratedContainer`](crate::DecoratedContainer) whose
/// `border()` adds padding automatically.
pub struct DecoratedBox {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    style: Style,
}

impl DecoratedBox {
    /// Create a new `DecoratedBox` with a child and default (empty) style.
    ///
    /// Unlike `DecoratedContainer::new()`, this does NOT set any
    /// `align_self`/`flex_shrink` defaults — the widget imposes zero layout
    /// opinion. If you want padding/sizing, compose with `WithLayout` or
    /// use `DecoratedContainer`.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            style: Style::default(),
        }
    }

    /// Replace the entire style.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the background color.
    pub fn background(mut self, color: Color) -> Self {
        self.style = self.style.background(color);
        self
    }

    /// Set the border. **Does NOT add padding** (Flutter semantics).
    ///
    /// The border paints over the child's edge pixels. To inset the child
    /// from the border, compose with `WithLayout::padding` or use
    /// `DecoratedContainer::border` which adds padding automatically.
    pub fn border(mut self, color: Color, width: f32) -> Self {
        self.style = self.style.border(color, width);
        self
    }

    /// Set the corner radius.
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.style = self.style.corner_radius(radius);
        self
    }

    /// Clip children to this widget's bounds.
    pub fn clip(mut self) -> Self {
        self.style = self.style.clip();
        self
    }

    /// Add a single shadow.
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.style = self.style.shadow(shadow);
        self
    }

    /// Replace all shadows.
    pub fn shadows(mut self, shadows: Vec<BoxShadow>) -> Self {
        self.style = self.style.shadows(shadows);
        self
    }

    /// Set the key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Get the child widget.
    pub fn child(&self) -> &dyn Widget {
        self.child.as_ref()
    }

    /// Get the style.
    pub fn style_ref(&self) -> &Style {
        &self.style
    }
}

impl Clone for DecoratedBox {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            style: self.style.clone(),
        }
    }
}

impl Widget for DecoratedBox {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = DecoratedBoxElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(DecoratedBoxRenderObject::new(self.style.clone()))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(decorated_ro) = render_object
            .as_any_mut()
            .downcast_mut::<DecoratedBoxRenderObject>()
        {
            // Style change is paint-only — DecoratedBoxRenderObject is a
            // true pass-through proxy with no layout node.
            if decorated_ro.set_style(self.style.clone()) {
                UpdateResult::PAINT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GlobalKey, Key, Text};

    #[test]
    fn test_decorated_box_creation() {
        let widget = DecoratedBox::new(Text::new("Hello"));
        assert!(widget.key().is_none());
        assert_eq!(widget.style_ref(), &Style::default());
    }

    #[test]
    fn test_decorated_box_with_key_local() {
        let widget = DecoratedBox::new(Text::new("Hello")).with_key("my-box");
        assert_eq!(
            widget.key(),
            Some(WidgetKey::Local(Key::new("my-box")))
        );
    }

    #[test]
    fn test_decorated_box_with_key_global() {
        let global_key = GlobalKey::new();
        let widget = DecoratedBox::new(Text::new("Hello")).with_key(global_key.clone());
        assert_eq!(widget.key(), Some(WidgetKey::Global(global_key)));
    }

    #[test]
    fn test_decorated_box_style_builder_chain() {
        let widget = DecoratedBox::new(Text::new("Hello"))
            .background(Color::RED)
            .border(Color::BLACK, 2.0)
            .corner_radius(8.0)
            .clip();
        assert_eq!(widget.style_ref().background, Some(Color::RED));
        assert_eq!(widget.style_ref().border.as_ref().unwrap().width, 2.0);
        assert_eq!(widget.style_ref().corner_radius.as_ref().unwrap().radius, 8.0);
        assert!(widget.style_ref().clip);
    }

    #[test]
    fn test_decorated_box_shadow_builder() {
        let widget = DecoratedBox::new(Text::new("Hi"))
            .shadow(BoxShadow::new(Color::BLACK).blur(8.0));
        assert_eq!(widget.style_ref().shadows.len(), 1);
        assert_eq!(widget.style_ref().shadows[0].blur_radius, 8.0);
    }

    #[test]
    fn test_decorated_box_shadows_builder() {
        let widget = DecoratedBox::new(Text::new("Hi")).shadows(vec![
            BoxShadow::new(Color::BLACK),
            BoxShadow::new(Color::RED),
        ]);
        assert_eq!(widget.style_ref().shadows.len(), 2);
    }

    #[test]
    fn test_decorated_box_shadow_preserves_background() {
        let widget = DecoratedBox::new(Text::new("Hi"))
            .background(Color::WHITE)
            .shadow(BoxShadow::new(Color::BLACK));
        assert_eq!(widget.style_ref().background, Some(Color::WHITE));
        assert_eq!(widget.style_ref().shadows.len(), 1);
    }

    #[test]
    fn test_decorated_box_style_replaces_everything() {
        let widget = DecoratedBox::new(Text::new("Hello"))
            .background(Color::RED)
            .style(Style::new().border(Color::BLACK, 1.0));
        // .style() replaces the entire Style, so background is lost
        assert_eq!(widget.style_ref().background, None);
        assert!(widget.style_ref().border.is_some());
    }

    #[test]
    fn test_decorated_box_border_does_not_add_padding() {
        // Regression guard: DecoratedBox has no Layout field at all, so it
        // cannot add padding. This is the semantic difference from
        // DecoratedContainer::border(), which adds padding equal to border
        // width. Verifying the field doesn't exist is implicit in the type
        // signature; this test instead verifies the border is set without
        // any layout side effect by checking the style only.
        let widget = DecoratedBox::new(Text::new("Hello")).border(Color::BLACK, 2.0);
        assert_eq!(widget.style_ref().border.as_ref().unwrap().width, 2.0);
        // No layout field to check — compilation itself proves there's no
        // padding side effect.
    }

    #[test]
    fn test_decorated_box_render_object_is_pass_through() {
        let widget = DecoratedBox::new(Text::new("Hello"));
        let ro = widget.create_render_object();
        assert!(
            ro.is_pass_through(),
            "DecoratedBox's render object must be pass-through"
        );
    }

    #[test]
    fn test_decorated_box_render_object_creation() {
        let widget = DecoratedBox::new(Text::new("Hello")).background(Color::RED);
        let ro = widget.create_render_object();
        assert!(ro
            .as_any()
            .downcast_ref::<DecoratedBoxRenderObject>()
            .is_some());
    }

    #[test]
    fn test_decorated_box_update_render_object_returns_paint_only() {
        let widget_red = DecoratedBox::new(Text::new("Hi")).background(Color::RED);
        let mut ro = widget_red.create_render_object();

        // Same style → NONE
        let result = widget_red.update_render_object(ro.as_mut());
        assert_eq!(result, UpdateResult::NONE);

        // Different style → PAINT only (never LAYOUT)
        let widget_blue = DecoratedBox::new(Text::new("Hi")).background(Color::BLUE);
        let result = widget_blue.update_render_object(ro.as_mut());
        assert!(result.contains(UpdateResult::PAINT));
        assert!(
            !result.contains(UpdateResult::LAYOUT),
            "DecoratedBox must never return LAYOUT (proxy has no layout)"
        );
    }

    #[test]
    fn test_decorated_box_can_update_same_type() {
        let w1 = DecoratedBox::new(Text::new("Hi")).background(Color::RED);
        let w2 = DecoratedBox::new(Text::new("Hi")).background(Color::BLUE);
        // Element stores the widget and checks type_id equality.
        let mut elem = DecoratedBoxElement::new();
        elem.set_widget(&w1);
        assert!(
            elem.can_update(w2.as_any()),
            "two DecoratedBox widgets must be able to update each other"
        );
    }

    #[test]
    fn test_decorated_box_cannot_update_different_type() {
        // DecoratedBox and DecoratedContainer have distinct type_ids, so the
        // reconciler cannot accidentally reconcile one into the other.
        use crate::DecoratedContainer;
        let w1 = DecoratedBox::new(Text::new("Hi"));
        let w2 = DecoratedContainer::new(Text::new("Hi"));
        let mut elem = DecoratedBoxElement::new();
        elem.set_widget(&w1);
        assert!(
            !elem.can_update(w2.as_any()),
            "DecoratedBox must not be able to update a DecoratedContainer"
        );
    }

    #[test]
    fn test_decorated_box_element_default() {
        let elem = DecoratedBoxElement::default();
        assert!(elem.widget().is_none());
        assert!(elem.render_object_id().is_none());
    }

    #[test]
    fn test_decorated_box_clone_preserves_fields() {
        let widget = DecoratedBox::new(Text::new("Hi"))
            .background(Color::RED)
            .with_key("cloned");
        let cloned = widget.clone();
        assert_eq!(cloned.key(), widget.key());
        assert_eq!(cloned.style_ref(), widget.style_ref());
    }
}
```

- [ ] **Step 2: Run test to verify it fails to compile**

Run: `cargo test -p vexo --lib widgets::decorated_box`
Expected: FAIL — error `unresolved module decorated_box` because the module isn't registered yet.

- [ ] **Step 3: Register the module and export**

Open `vexo/src/widgets/mod.rs`. Add `mod decorated_box;` (alphabetical, after `mod decorated_container;` on line 7) and `pub use decorated_box::DecoratedBox;` (after `pub use decorated_container::DecoratedContainer;` on line 52).

The relevant section should look like:

```rust
mod container;
mod decorated_box;
mod decorated_container;
mod fractional_translation;
// ...

pub use decorated_box::DecoratedBox;
pub use decorated_container::DecoratedContainer;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib widgets::decorated_box`
Expected: PASS — all 16 tests pass.

- [ ] **Step 5: Run full vexo lib test suite**

Run: `cargo test -p vexo --lib`
Expected: PASS — no regressions. `DecoratedContainer` tests still pass (unchanged).

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/decorated_box.rs vexo/src/widgets/mod.rs
git commit -m "feat(decorated-box): add DecoratedBox widget and DecoratedBoxElement"
```

---

## Task 4: Re-export `DecoratedBox` at top level

**Files:**
- Modify: `vexo/src/lib.rs:207-212`

**Interfaces:**
- Produces: `vexo::DecoratedBox` available as a top-level public export (alongside `vexo::DecoratedContainer`).

- [ ] **Step 1: Add `DecoratedBox` to the top-level re-export**

Open `vexo/src/lib.rs`. Find the `pub use widgets::{ ... }` block at lines 207-212:

```rust
pub use widgets::{
    Column, DecoratedContainer, FadeTransition, Flex, FractionalTranslation, GestureDetector, Grid,
    Image, IndexedStack, Offstage, Opacity, Positioned, Row, SafeArea, SafeAreaClaim,
    ScrollController, ScrollView, SlideDirection, SlideTransition, Stack, Text, TextEdit,
    TextEditState, TextEditingController, Theme, ThemeData, Transform, Widget, WithLayout,
};
```

Insert `DecoratedBox,` immediately before `DecoratedContainer,` (alphabetical):

```rust
pub use widgets::{
    Column, DecoratedBox, DecoratedContainer, FadeTransition, Flex, FractionalTranslation,
    GestureDetector, Grid, Image, IndexedStack, Offstage, Opacity, Positioned, Row, SafeArea,
    SafeAreaClaim, ScrollController, ScrollView, SlideDirection, SlideTransition, Stack, Text,
    TextEdit, TextEditState, TextEditingController, Theme, ThemeData, Transform, Widget,
    WithLayout,
};
```

- [ ] **Step 2: Verify the re-export compiles**

Run: `cargo build -p vexo`
Expected: PASS — no errors. (`cargo build` is sufficient here; no test logic to verify.)

- [ ] **Step 3: Commit**

```bash
git add vexo/src/lib.rs
git commit -m "feat(decorated-box): re-export DecoratedBox at top level"
```

---

## Task 5: Rewire `WidgetExt` decoration modifiers to use `DecoratedBox`

**Files:**
- Modify: `vexo/src/widgets/mod.rs:196-224` (the four `WidgetExt` methods)
- Modify: `vexo/src/widgets/mod.rs` test module (add one routing test)

**Interfaces:**
- Consumes: `crate::widgets::DecoratedBox` (from Task 3, already imported into `widgets/mod.rs` in Task 3 Step 3).
- Produces: `WidgetExt::background/border/corner_radius/clip` now wrap in `DecoratedBox` instead of `DecoratedContainer`. Behavior change for future `Box<dyn Widget>.background(RED)` callers: the wrapper no longer imposes `align_self(Start).flex_shrink(0.0)`.

- [ ] **Step 1: Write the failing routing test**

Open `vexo/src/widgets/mod.rs`. Find the `#[cfg(test)] mod tests { ... }` block (starts around line 414). Add this test at the end of the module (before the closing `}`):

```rust
    #[test]
    fn test_widget_ext_background_wraps_in_decorated_box() {
        // `Text` has an inherent `.background()`, so to trigger WidgetExt
        // (which fires on `Box<dyn Widget>` / `Self: Sized + 'static` only
        // when no inherent method matches), we box the Text first. The
        // outer `.background()` then resolves to WidgetExt.
        let widget: Box<dyn Widget> = Text::new("x").boxed().background(Color::RED);
        let outer = widget
            .as_any()
            .downcast_ref::<DecoratedBox>()
            .expect("WidgetExt::background should wrap in DecoratedBox, not DecoratedContainer");
        assert_eq!(outer.style_ref().background, Some(Color::RED));
    }

    #[test]
    fn test_widget_ext_clip_wraps_in_decorated_box() {
        let widget: Box<dyn Widget> = Text::new("x").boxed().clip();
        let outer = widget
            .as_any()
            .downcast_ref::<DecoratedBox>()
            .expect("WidgetExt::clip should wrap in DecoratedBox");
        assert!(outer.style_ref().clip);
    }

    #[test]
    fn test_widget_ext_border_wraps_in_decorated_box() {
        let widget: Box<dyn Widget> = Text::new("x").boxed().border(Color::BLACK, 2.0);
        let outer = widget
            .as_any()
            .downcast_ref::<DecoratedBox>()
            .expect("WidgetExt::border should wrap in DecoratedBox");
        assert_eq!(outer.style_ref().border.as_ref().unwrap().width, 2.0);
    }

    #[test]
    fn test_widget_ext_corner_radius_wraps_in_decorated_box() {
        let widget: Box<dyn Widget> = Text::new("x").boxed().corner_radius(8.0);
        let outer = widget
            .as_any()
            .downcast_ref::<DecoratedBox>()
            .expect("WidgetExt::corner_radius should wrap in DecoratedBox");
        assert_eq!(outer.style_ref().corner_radius.as_ref().unwrap().radius, 8.0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib widgets::tests::test_widget_ext_background_wraps_in_decorated_box`
Expected: FAIL — `downcast_ref::<DecoratedBox>()` returns `None` because the current `WidgetExt::background` wraps in `DecoratedContainer`, not `DecoratedBox`. The `expect(...)` message will fire.

- [ ] **Step 3: Rewire the four `WidgetExt` methods**

Open `vexo/src/widgets/mod.rs`. Find the "Decoration modifiers" block at lines 196-224:

```rust
    // Decoration modifiers (fallback: wrap in DecoratedContainer)

    fn background(self, color: Color) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedContainer::new(self).background(color))
    }

    fn border(self, color: Color, width: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedContainer::new(self).border(color, width))
    }

    fn corner_radius(self, radius: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedContainer::new(self).corner_radius(radius))
    }

    fn clip(self) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedContainer::new(self).clip())
    }
```

Replace `DecoratedContainer` with `DecoratedBox` in all four bodies, and update the section comment to reflect the new semantics:

```rust
    // Decoration modifiers (wrap in DecoratedBox — pass-through, no layout
    // opinion. Use .with_layout(Layout::default().padding(...)) or
    // DecoratedContainer directly if you need layout alongside decoration.)

    fn background(self, color: Color) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedBox::new(self).background(color))
    }

    fn border(self, color: Color, width: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedBox::new(self).border(color, width))
    }

    fn corner_radius(self, radius: f32) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedBox::new(self).corner_radius(radius))
    }

    fn clip(self) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(DecoratedBox::new(self).clip())
    }
```

`DecoratedBox` is already imported into `widgets/mod.rs` (added in Task 3 Step 3 via `pub use decorated_box::DecoratedBox;`). `DecoratedContainer` is still used elsewhere in the file? Let me check: `DecoratedContainer` is referenced only in these four methods and in `widgets/mod.rs` tests at line 609 (`Text::new("Click").background(Color::RED).padding(8.0).on_press(|| {})` — uses inherent `Text::background`, not `WidgetExt`). So the `DecoratedContainer` import at line 52 is still needed for any external re-export through this module; leave it.

- [ ] **Step 4: Run the new routing tests to verify they pass**

Run: `cargo test -p vexo --lib widgets::tests::test_widget_ext_`
Expected: PASS — all 4 routing tests pass.

- [ ] **Step 5: Run the existing `test_widget_trait_on_press_chain` test**

Run: `cargo test -p vexo --lib widgets::tests::test_widget_trait_on_press_chain`
Expected: PASS — this test uses `Text::new("Click").background(Color::RED).padding(8.0).on_press(||{})`. `Text` has inherent `.background()` (via `modifier_methods!()`), so this still resolves to `Text::background`, returns `Text` (not `Box<dyn Widget>`), and `.padding(8.0)` resolves to inherent `Text::padding`. Then `.on_press()` wraps the `Text` in `GestureDetector`. No change in behavior — the `WidgetExt` rewire doesn't affect this chain.

- [ ] **Step 6: Run full vexo lib + vexo_uikit test suites to verify no regression**

Run: `cargo test -p vexo --lib && cargo test -p vexo_uikit`
Expected: PASS — all tests pass. Per the spec's audit (see spec's "Audit of existing `WidgetExt` decoration call sites" table), zero existing callers go through `WidgetExt` for these methods; all use inherent methods on concrete widget types. So the rewire is forward-looking only and changes no existing behavior.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/widgets/mod.rs
git commit -m "feat(decorated-box): rewire WidgetExt decoration modifiers to use DecoratedBox

Fixes the latent sizing bug where .background(RED) on a Box<dyn Widget>
would silently impose align_self(Start).flex_shrink(0.0) on the wrapped
widget, breaking fill chains. DecoratedBox is a true pass-through proxy
with no layout opinion. Zero behavior change for existing callers — all
use inherent methods on concrete widget types."
```

---

## Task 6: Add integration tests

**Files:**
- Modify: `vexo/src/e2e_test.rs` (extend with two new tests)

**Interfaces:**
- Consumes: `crate::widgets::{DecoratedBox, DecoratedContainer, Transform}` (already imported at `e2e_test.rs:9` — `DecoratedBox` is added via the new top-level re-export from Task 4, but the existing import line uses `crate::widgets::{DecoratedContainer, Transform}` so we need to add `DecoratedBox` to that import list), `crate::{Flex, Text, ThreeTreePipeline, Widget}`, `crate::core::{Color, Size}`, `crate::layout::TaffyLayoutEngine`, `crate::animation::AnimationTicker`. Reference: existing `test_decorated_container_widget_in_pipeline` at `e2e_test.rs:125-212`.

- [ ] **Step 1: Update the import to include `DecoratedBox`**

Open `vexo/src/e2e_test.rs`. Find line 9:

```rust
use crate::widgets::{DecoratedContainer, Transform};
```

Change to:

```rust
use crate::widgets::{DecoratedBox, DecoratedContainer, Transform};
```

- [ ] **Step 2: Write the first integration test — `test_decorated_box_in_pipeline`**

Append this test at the end of `vexo/src/e2e_test.rs` (after the last existing `#[test]` fn):

```rust
/// Test DecoratedBox widget in the pipeline.
///
/// Mirrors `test_decorated_container_widget_in_pipeline` (line 125) but
/// verifies the pass-through proxy semantics:
/// 1. The render object is `is_pass_through() == true`.
/// 2. The child (Text) render object's Taffy node is linked directly to
///    the DecoratedBox's parent — no intervening Taffy node.
/// 3. Background/border/corner-radius commands appear in the paint output.
#[test]
fn test_decorated_box_in_pipeline() {
    use crate::render::RenderCommand;

    // Create a widget tree: DecoratedBox wrapping a Text.
    let widget = DecoratedBox::new(Text::new("Hello"))
        .background(Color::RED)
        .border(Color::BLACK, 2.0)
        .corner_radius(8.0);

    // Create pipeline and reconcile.
    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(Box::new(widget));

    // Should have created elements and render objects.
    assert!(
        pipeline.render_objects().len() >= 1,
        "Should have at least root render object"
    );

    // === Verify render tree structure ===
    let root_ro = pipeline
        .render_objects()
        .root()
        .expect("should have root render object");
    let root_obj = pipeline
        .render_objects()
        .get(root_ro)
        .expect("root render object should exist");

    // DecoratedBoxRenderObject must be pass-through.
    assert!(
        root_obj.is_pass_through(),
        "DecoratedBox's render object must be pass-through"
    );

    // DecoratedBox render object should have the Text render object as its
    // single child.
    let children = root_obj.children();
    assert_eq!(
        children.len(),
        1,
        "DecoratedBox render object should have exactly one child"
    );

    let child_ro_id = children[0];
    let child_obj = pipeline
        .render_objects()
        .get(child_ro_id)
        .expect("child render object should exist");
    assert_eq!(
        child_obj.children().len(),
        0,
        "Text render object should be a leaf"
    );

    // === Layout ===
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    // === Paint ===
    let commands = pipeline.paint();

    // DecoratedBox should produce commands for background + border, plus
    // PushCornerRadius/PopCornerRadius for the corner radius.
    // Order: PushCornerRadius, background Rect, border Rect, PopCornerRadius.
    assert!(
        commands.len() >= 4,
        "DecoratedBox should produce at least 4 commands (push radius + bg + border + pop radius), got {}",
        commands.len()
    );

    // Verify the render commands include a rect command (the background fill).
    let has_rect = commands
        .iter()
        .any(|cmd| matches!(cmd, RenderCommand::Rect { .. }));
    assert!(
        has_rect,
        "Commands should include a Rect command for background fill"
    );

    // Verify PushCornerRadius / PopCornerRadius are present.
    let has_push = commands
        .iter()
        .any(|cmd| matches!(cmd, RenderCommand::PushCornerRadius { .. }));
    let has_pop = commands
        .iter()
        .any(|cmd| matches!(cmd, RenderCommand::PopCornerRadius));
    assert!(has_push, "Should have PushCornerRadius command");
    assert!(has_pop, "Should have PopCornerRadius command");
}

/// Test that DecoratedBox passes width constraints through to its child.
///
/// Regression guard for the latent WidgetExt sizing bug: when a widget
/// is wrapped in a decoration proxy, the parent's definite width must
/// propagate to the child (so e.g. text wraps at that width). The
/// `DecoratedBox` proxy shares the child's Taffy node, so the parent
/// (Column with align: Stretch) stretches the *child* directly — no
/// intervening "size to content" node breaking the fill chain.
///
/// Mirrors `test_passthrough_opacity_child_receives_grandparent_width`
/// in `vexo/src/passthrough_integration.rs:63` but going through the
/// full pipeline (widget → element → render object).
#[test]
fn test_decorated_box_width_propagates_to_child() {
    use crate::layout::{AlignItems, FlexDirection, Layout};

    // Column (width=300) > DecoratedBox(no layout) > Container(width unset, height=40).
    // The Container is the "child" whose width we read back. If DecoratedBox
    // were NOT a true pass-through, the Container would size to its intrinsic
    // width (0) instead of stretching to 300.
    let child = crate::Flex::column()
        .layout(Layout::default().height(40.0))
        .boxed();
    let widget = crate::Flex::column()
        .layout(
            Layout::default()
                .flex_direction(FlexDirection::Column)
                .align(AlignItems::Stretch)
                .width(300.0)
                .height(200.0),
        )
        .push(DecoratedBox::new(child).background(Color::RED))
        .boxed();

    let mut pipeline: ThreeTreePipeline = ThreeTreePipeline::new(Arc::new(AnimationTicker::new()));
    pipeline.reconcile(widget);

    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();
    pipeline.layout(Size::new(800.0, 600.0), &mut engine, &mut font_system);

    // Render tree: Flex(column, 300x200) → DecoratedBox → Flex(column, height=40)
    let root_ro = pipeline
        .render_objects()
        .root()
        .expect("should have root render object");
    let root_obj = pipeline
        .render_objects()
        .get(root_ro)
        .expect("root render object should exist");

    // Root's child is the DecoratedBox RO.
    let decorated_box_ro = root_obj.children()[0];
    let decorated_box_obj = pipeline
        .render_objects()
        .get(decorated_box_ro)
        .expect("DecoratedBox render object should exist");
    assert!(
        decorated_box_obj.is_pass_through(),
        "DecoratedBox render object must be pass-through"
    );

    // DecoratedBox's child is the inner Flex RO.
    let inner_flex_ro = decorated_box_obj.children()[0];
    let inner_flex_obj = pipeline
        .render_objects()
        .get(inner_flex_ro)
        .expect("inner Flex render object should exist");
    let inner_bounds = inner_flex_obj
        .computed_bounds()
        .expect("inner Flex should have computed bounds after layout");

    // The inner Flex has no explicit width, but the parent Column has
    // align: Stretch and width=300. With a true pass-through proxy in
    // between, the stretch propagates to the inner Flex and it fills
    // the 300px width.
    assert_eq!(
        inner_bounds.width(),
        300.0,
        "DecoratedBox (true pass-through) must let parent's width propagate to child. Got {}",
        inner_bounds.width()
    );
}
```

- [ ] **Step 3: Run the new integration tests**

Run: `cargo test -p vexo --lib e2e_test::test_decorated_box_in_pipeline`
Run: `cargo test -p vexo --lib e2e_test::test_decorated_box_width_propagates_to_child`
Expected: PASS — both tests pass.

If `test_decorated_box_width_propagates_to_child` fails with a width other than 300.0, the pass-through is not working. Check that `DecoratedBoxRenderObject::is_pass_through()` returns `true` (Task 2) and that `layout()` returns the child's node directly (Task 2). Do not "fix" by adding a layout to `DecoratedBox` — the whole point is no layout opinion.

- [ ] **Step 4: Run the full vexo lib test suite**

Run: `cargo test -p vexo --lib`
Expected: PASS — all tests pass, including the existing `test_decorated_container_widget_in_pipeline` (unchanged) and all `passthrough_integration` tests.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/e2e_test.rs
git commit -m "test(decorated-box): add pipeline integration tests

test_decorated_box_in_pipeline: verifies render tree structure,
pass-through flag, and paint commands.

test_decorated_box_width_propagates_to_child: regression guard for
the latent WidgetExt sizing bug — parent Column's width must reach
the child through the DecoratedBox proxy."
```

---

## Task 7: Final verification

**Files:**
- None modified.

- [ ] **Step 1: Run the entire workspace test suite**

Run: `cargo test --workspace`
Expected: PASS — all tests pass across `vexo`, `shared_app`, `vexo_uikit`, and `desktop_demo`.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS — no warnings. If clippy fires on `DecoratedBoxRenderObject` or `DecoratedBox`, address the specific lint (common ones: `too_many_arguments`, `module_inception` — neither expected here).

- [ ] **Step 3: Build for iOS (optional, only if iOS toolchain installed)**

Run: `./build_for_ios.sh`
Expected: PASS — iOS lib + Swift bindings generate successfully. This verifies the UniFFI exports still work and there's no platform-specific regression.

If the iOS toolchain is not installed, skip this step and note it in the final report.

- [ ] **Step 4: Verify no `DecoratedContainer` callers were accidentally modified**

Run: `git diff main -- vexo/src/widgets/decorated_container.rs`
Expected: empty — no changes to `DecoratedContainer`. (If `git diff main` doesn't work because we're on `main`, use `git diff HEAD~7 -- vexo/src/widgets/decorated_container.rs` to compare against the commit before this plan started, which was `2d38e25` per the spec commit.)

- [ ] **Step 5: Verify the spec's "Existing tests that must still pass" section**

Run each of these and confirm PASS:
- `cargo test -p vexo --lib widgets::decorated_container::tests` (DecoratedContainer unchanged)
- `cargo test -p vexo_uikit --test navigation_animation_tests` (downcasts to DecoratedContainer still work)
- `cargo test -p vexo_uikit --test button_render_tests` (downcasts to DecoratedContainer still work)
- `cargo test -p vexo --lib e2e_test::test_decorated_container_widget_in_pipeline` (unchanged)

Expected: PASS for all.

- [ ] **Step 6: Final commit (if any cleanup was needed)**

If steps 1-5 required any cleanup edits, commit them:

```bash
git add -A
git commit -m "chore(decorated-box): final cleanup after verification"
```

Otherwise, no commit needed — the implementation is complete.

---

## Self-Review Notes

This plan was reviewed against the spec (`docs/superpowers/specs/2026-07-19-decorated-box-split-design.md`) with the following checks:

**1. Spec coverage:**
- "Add `DecoratedBox` widget" → Task 3
- "Keep `DecoratedContainer` unchanged" → Task 7 Step 4 verifies this
- "Single source of truth for decoration painting" → Task 1 extracts `paint_style()`, Task 2 uses it
- "Fix `WidgetExt` sizing bug" → Task 5 rewires the 4 methods
- "True pass-through (Option B)" → Task 2 `is_pass_through() == true`
- "`border()` does NOT add padding" → Task 3 doc + `test_decorated_box_border_does_not_add_padding`
- "WidgetExt routing + audit" → Task 5 + audit table in spec
- All 8 widget unit tests from spec → Task 3
- All 3 render object tests from spec → Task 2
- Both integration tests from spec → Task 6
- WidgetExt routing test from spec → Task 5
- "Existing tests that must still pass" → Task 7 Step 5

**2. Placeholder scan:** No `TBD`, `TODO`, or "implement later". Every step has the actual code or command. The `...` in some file listings denotes unchanged surrounding code, not placeholders.

**3. Type consistency:**
- `DecoratedBoxRenderObject::new(style: Style)` — used consistently in Task 2 (definition), Task 3 (`create_render_object` calls `DecoratedBoxRenderObject::new(self.style.clone())`).
- `set_style(&mut self, style: Style) -> bool` — used consistently in Task 2 (definition), Task 3 (`update_render_object` calls `decorated_ro.set_style(self.style.clone())`).
- `paint_style(style: &Style, bounds: Bounds<Logical>, ctx: &mut PaintContext) -> Vec<RenderCommand>` — used consistently in Task 1 (definition), Task 2 (`paint()` body), Task 1 Step 3 (`ContainerRenderObject::paint()` body).
- `DecoratedBox::new(child: impl Widget + 'static)` — used consistently in Task 3 (definition), Task 5 (`WidgetExt::background` calls `DecoratedBox::new(self).background(color)`), Task 6 (`DecoratedBox::new(Text::new("Hello"))`).

**4. Scope check:** Single focused change, no decomposition needed.
