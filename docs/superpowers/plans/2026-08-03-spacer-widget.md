# Spacer Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a leaf `Spacer` widget that claims a share of the parent's free space (`flex_grow(1.0)`), then migrate the two `MultiChild::empty(Layout::default().flex_grow(1.0))` call sites in `shared_app/src/chats/chat_screen.rs` to use it.

**Architecture:** A new leaf widget (`Spacer`) reusing `LeafRenderObjectElement` (same pattern as `Text`), backed by a new `SpacerRenderObject` that owns a single Taffy leaf node created with `Layout::default().flex_grow(1.0)`. The render object paints nothing, hits nothing, and has no children. Direction-agnostic — the parent's `flex_direction` decides which axis the spacer grows along.

**Tech Stack:** Rust, vexo three-tree architecture (Widget → Element → RenderObject), Taffy layout engine, glyphon font system (for test fixtures only).

## Global Constraints

- Workspace dep versions (from root `Cargo.toml`): taffy 0.11, glyphon (fork `vexorsis/glyphon`, branch `depth-per-textarea`).
- Embedded `font.ttf` via `crate::resource::file::FONT` is the canonical font fixture for tests — copy the helper from `vexo/src/render_objects/offstage.rs:189-193`.
- Spacer's `flex_grow` is a compile-time `1.0` baked into `SpacerRenderObject`. No configurable parameter, no `pub(crate)` escape hatch. (YAGNI per spec.)
- All new code must be `#[derive(Clone)]` where the widget needs `clone_boxed()`.
- After every Rust edit, run `cargo build -p vexo` (or `-p shared_app` for the migration task) and `cargo test -p vexo` (or the relevant package). Never assume tests pass.
- Run `cargo fmt --all` before each commit.
- Use only `log::debug!` for any diagnostic output (none expected in this feature).

---

### Task 1: SpacerRenderObject — leaf RO with `flex_grow(1.0)`

**Files:**
- Create: `vexo/src/render_objects/spacer.rs`
- Modify: `vexo/src/render_objects/mod.rs` (add `mod spacer;` + `pub use`)

**Interfaces:**
- Consumes: `crate::layout::{Layout, LayoutNodeKey}`, `crate::core::{Bounds, Logical, Point, Size}`, `crate::{HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey}`, `std::any::Any`.
- Produces: `pub struct SpacerRenderObject` with `pub fn new() -> Self`; implements `RenderObject` trait. `layout()` returns a `LayoutResult` whose `node` is the spacer's owned Taffy leaf (created with `Layout::default().flex_grow(1.0)`).

- [ ] **Step 1: Write the failing tests**

Create `vexo/src/render_objects/spacer.rs` with only the test module and a stub struct (so tests compile and fail):

```rust
//! Render object for Spacer — a leaf that claims a share of free space.
//!
//! `layout()` creates a Taffy leaf with `Layout::default().flex_grow(1.0)`.
//! `paint()` emits nothing, `hit_test()` returns false, `children()` is empty.
//! Direction-agnostic: the parent's `flex_direction` decides which axis the
//! spacer grows along, which is why the layout uses `Layout::default()` (not
//! `Layout::row()` / `Layout::column()`).

use std::any::Any;

use crate::core::{Bounds, Logical, Point, Size};
use crate::layout::{Layout, LayoutNodeKey};
use crate::{
    HitTestContext, LayoutContext, LayoutResult, PaintContext, RenderObject, RenderObjectKey,
};

pub struct SpacerRenderObject; // stub

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutEngine, TaffyLayoutEngine};

    fn create_test_font_system() -> glyphon::FontSystem {
        let font_data = crate::resource::file::FONT.to_vec();
        let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        glyphon::FontSystem::new_with_fonts([binary])
    }

    #[test]
    fn spacer_layout_creates_node() {
        let mut ro = SpacerRenderObject::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();
        let mut ctx = LayoutContext::new(&mut engine, &mut font_system);

        let result = ro.layout(&mut ctx, &[]);

        assert!(ro.layout_node().is_some());
        assert_eq!(ro.layout_node(), Some(result.node));
    }

    #[test]
    fn spacer_layout_node_uses_flex_grow_one() {
        // Behavioral assertion: when placed in a 200px-wide row with an 80px
        // fixed sibling, the spacer absorbs the leftover 120px.
        let mut ro = SpacerRenderObject::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let spacer_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[]).node
        };

        let bubble_node = engine.create_leaf(&Layout::default().width(80.0).height(20.0));
        let row = engine.create_container(
            &Layout::row().width(200.0).height(20.0),
            &[spacer_node, bubble_node],
        );

        engine.compute(row, Size::new(200.0, 20.0), &mut font_system);

        let spacer_layout = engine.get_layout(spacer_node).expect("spacer has layout");
        assert_eq!(spacer_layout.x(), 0.0);
        assert_eq!(spacer_layout.width(), 120.0);

        let bubble_layout = engine.get_layout(bubble_node).expect("bubble has layout");
        assert_eq!(bubble_layout.x(), 120.0);
        assert_eq!(bubble_layout.width(), 80.0);
    }

    #[test]
    fn spacer_paint_is_empty() {
        let ro = SpacerRenderObject::new();
        let mut commands: Vec<crate::render::RenderCommand> = Vec::new();
        let mut ctx = PaintContext::new(&mut commands);
        assert!(ro.paint(&mut ctx).is_empty());
    }

    #[test]
    fn spacer_hit_test_returns_false() {
        let ro = SpacerRenderObject::new();
        assert!(!ro.hit_test(Point::new(0.0, 0.0), &HitTestContext::mock()));
    }

    #[test]
    fn spacer_children_is_empty() {
        let ro = SpacerRenderObject::new();
        assert_eq!(ro.children(), &[] as &[RenderObjectKey]);
    }

    #[test]
    fn spacer_apply_layout_populates_bounds() {
        let mut ro = SpacerRenderObject::new();
        let mut engine = TaffyLayoutEngine::new();
        let mut font_system = create_test_font_system();

        let spacer_node = {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.layout(&mut ctx, &[]).node
        };

        // Compute against a 100x50 box so the spacer fills it.
        engine.compute(spacer_node, Size::new(100.0, 50.0), &mut font_system);

        {
            let mut ctx = LayoutContext::new(&mut engine, &mut font_system);
            ro.apply_layout(&mut ctx);
        }

        let bounds = ro.computed_bounds().expect("apply_layout populates bounds");
        assert_eq!(bounds.width(), 100.0);
        assert_eq!(bounds.height(), 50.0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail (compile error)**

Run: `cargo test -p vexo --lib spacer::tests`
Expected: FAIL — `SpacerRenderObject` has no `new()` / `layout()` / etc.

- [ ] **Step 3: Implement `SpacerRenderObject`**

Replace the stub with the full implementation. Keep the same `use` imports and the test module unchanged.

```rust
pub struct SpacerRenderObject {
    owned_node: Option<LayoutNodeKey>,
    computed_bounds: Option<Bounds<Logical>>,
}

impl SpacerRenderObject {
    pub fn new() -> Self {
        Self {
            owned_node: None,
            computed_bounds: None,
        }
    }
}

impl Default for SpacerRenderObject {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObject for SpacerRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, _child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        let spacer_layout = Layout::default().flex_grow(1.0);
        let node = match self.owned_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &spacer_layout);
                ctx.engine().set_children(existing, &[]);
                existing
            }
            None => {
                let node = ctx.engine().create_container(&spacer_layout, &[]);
                self.owned_node = Some(node);
                node
            }
        };
        LayoutResult {
            node,
            size: Size::zero(),
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.owned_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<crate::render::RenderCommand> {
        vec![]
    }

    fn hit_test(&self, _position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        false
    }

    fn children(&self) -> &[RenderObjectKey] {
        &[]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.owned_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}
```

- [ ] **Step 4: Register the module in `vexo/src/render_objects/mod.rs`**

In `vexo/src/render_objects/mod.rs`, add `mod spacer;` to the `mod` block (alphabetical, between `positioned` and `proxy`) and `pub use spacer::SpacerRenderObject;` to the `pub use` block (alphabetical, between `PositionedRenderObject` and `ProxyRenderObject`).

The two blocks should read:

```rust
mod clip_rrect;
mod container;
mod decorated_box;
mod image;
mod indexed_stack;
mod offstage;
mod opacity;
mod positioned;
mod proxy;
mod scroll_view;
mod spacer;
mod text;
mod text_edit;
```

```rust
pub use clip_rrect::ClipRRectRenderObject;
pub use container::ContainerRenderObject;
pub use decorated_box::DecoratedBoxRenderObject;
pub use image::ImageRenderObject;
pub use indexed_stack::IndexedStackRenderObject;
pub use offstage::OffstageRenderObject;
pub use opacity::OpacityRenderObject;
pub use positioned::{PositionedInsets, PositionedRenderObject};
pub use proxy::ProxyRenderObject;
pub use scroll_view::ScrollViewRenderObject;
pub use spacer::SpacerRenderObject;
pub use text::TextRenderObject;
pub use text_edit::TextEditRenderObject;
```

- [ ] **Step 5: Verify the API surface compiles**

Run: `cargo build -p vexo`
Expected: BUILD SUCCEEDS. (If `PaintContext::new` / `HitTestContext::new` constructor signatures differ from what the tests assume, fix the test setup to match — look at how other render object tests in `vexo/src/render_objects/*.rs` construct these contexts. The implementation itself must not change.)

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p vexo --lib render_objects::spacer::tests`
Expected: PASS — all 6 tests green.

If `spacer_paint_is_empty` or `spacer_hit_test_returns_false` fail because the `PaintContext`/`HitTestContext` constructors take different arguments, inspect existing tests in `vexo/src/render_objects/` (e.g. `text.rs`, `offstage.rs`) for the right construction pattern and adjust the test setup only. Do not weaken the assertions.

- [ ] **Step 7: Format and commit**

```bash
cargo fmt --all
git add vexo/src/render_objects/spacer.rs vexo/src/render_objects/mod.rs
git commit -m "feat(vexo): add SpacerRenderObject with flex_grow(1.0) leaf node"
```

---

### Task 2: Spacer widget — leaf widget reusing `LeafRenderObjectElement`

**Files:**
- Create: `vexo/src/widgets/spacer.rs`
- Modify: `vexo/src/widgets/mod.rs` (add `mod spacer;` + `pub use spacer::Spacer;`)
- Modify: `vexo/src/lib.rs` (add `Spacer` to the `pub use widgets::{...}` list)

**Interfaces:**
- Consumes: `crate::elements::LeafElement`, `crate::render_objects::SpacerRenderObject`, `crate::{Element, RenderObject, UpdateResult, Widget, WidgetKey}`, `crate::key::WidgetKey`, `std::any::Any`.
- Produces: `pub struct Spacer` with `pub fn new() -> Self` and `pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self`. Implements `Widget`. `create_element()` returns a `LeafElement` wired with `set_widget(self)`; `create_render_object()` returns `Box::new(SpacerRenderObject::new())`; `update_render_object()` returns `UpdateResult::NONE`.

- [ ] **Step 1: Write the failing tests**

Create `vexo/src/widgets/spacer.rs` with stub struct and tests:

```rust
//! Spacer widget — a leaf that claims a share of the parent's free space.
//!
//! Drop-in replacement for `MultiChild::empty(Layout::default().flex_grow(1.0))`
//! when used as a flexible spacer inside a `row!` / `column!`. Paints nothing,
//! hits nothing, has no children. Backed by `SpacerRenderObject`.
//!
//! See `docs/superpowers/specs/2026-08-03-spacer-widget-design.md`.

use std::any::Any;

use crate::elements::LeafElement;
use crate::key::WidgetKey;
use crate::render_objects::SpacerRenderObject;
use crate::{Element, RenderObject, UpdateResult, Widget};

pub struct Spacer; // stub

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Key;

    #[test]
    fn spacer_new_creates_spacer_render_object() {
        let w = Spacer::new();
        let ro = w.create_render_object();
        assert!(ro.as_any().downcast_ref::<SpacerRenderObject>().is_some());
    }

    #[test]
    fn spacer_update_render_object_returns_none() {
        let w = Spacer::new();
        let mut ro = SpacerRenderObject::new();
        let result = w.update_render_object(&mut ro);
        assert_eq!(result, UpdateResult::NONE);
    }

    #[test]
    fn spacer_with_key_round_trips() {
        let w = Spacer::new().with_key("my-spacer");
        assert_eq!(
            w.key(),
            Some(WidgetKey::Local(Key::new("my-spacer")))
        );
    }

    #[test]
    fn spacer_default_is_same_as_new() {
        let _w1 = Spacer::new();
        let _w2 = Spacer::default();
        // If this compiles, `Default` is wired up correctly.
    }

    #[test]
    fn spacer_create_element_is_leaf() {
        let w = Spacer::new();
        let _elem = w.create_element();
        // No assertion on internals — LeafElement is opaque from the widget
        // module. The test confirms `create_element` does not panic.
    }
}
```

- [ ] **Step 2: Run tests to verify they fail (compile error)**

Run: `cargo test -p vexo --lib widgets::spacer::tests`
Expected: FAIL — `Spacer` has no `new()` / `with_key()` / `default()` / `update_render_object()` / etc.

- [ ] **Step 3: Implement `Spacer`**

Replace the stub with the full implementation:

```rust
pub struct Spacer {
    key: Option<WidgetKey>,
}

impl Spacer {
    pub fn new() -> Self {
        Self { key: None }
    }

    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Default for Spacer {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Spacer {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
        }
    }
}

impl Widget for Spacer {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = LeafElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(SpacerRenderObject::new())
    }

    fn update_render_object(&self, _render_object: &mut dyn RenderObject) -> UpdateResult {
        UpdateResult::NONE
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_boxed(&self) -> Box<dyn Widget> {
        Box::new(self.clone())
    }
}
```

- [ ] **Step 4: Register the module in `vexo/src/widgets/mod.rs`**

In `vexo/src/widgets/mod.rs`, add `mod spacer;` to the `mod` block (alphabetical, between `shared` and `stack`) and `pub use spacer::Spacer;` to the public API block (alphabetical, between `Shared` and `Stack`).

The `mod` block (currently lines 6-32) gains `mod spacer;` after `mod shared;`:

```rust
mod shared;
mod spacer;
mod stack;
```

The public `pub use` block (currently lines 41-56) gains `Spacer` after `Shared`:

```rust
pub use shared::Shared;
pub use spacer::Spacer;
pub use stack::Stack;
```

- [ ] **Step 5: Re-export from `vexo/src/lib.rs`**

In `vexo/src/lib.rs` (lines 213-219), add `Spacer` to the `pub use widgets::{...}` list. Alphabetical slot is between `SlideTransition` and `Stack`. The block becomes:

```rust
pub use widgets::{
    Brightness, ChildPush, ClipRRect, DecoratedBox, FadeTransition, FractionalTranslation,
    GestureDetector, Grid, Image, IndexedStack, MediaQuery, MediaQueryData, MediaQueryMutator,
    MultiChild, Offstage, Opacity, Orientation, Positioned, RemoveEdges, SafeArea, ScrollController,
    ScrollView, Shared, SlideDirection, SlideTransition, Spacer, Stack, Text, TextEdit, TextEditState,
    TextEditingController, Theme, ThemeData, Transform, Widget, WithLayout,
};
```

- [ ] **Step 6: Build the crate**

Run: `cargo build -p vexo`
Expected: BUILD SUCCEEDS.

- [ ] **Step 7: Run widget tests to verify they pass**

Run: `cargo test -p vexo --lib widgets::spacer::tests`
Expected: PASS — all 5 tests green.

- [ ] **Step 8: Run the full vexo test suite to catch regressions**

Run: `cargo test -p vexo`
Expected: PASS — no regressions in existing tests.

- [ ] **Step 9: Format and commit**

```bash
cargo fmt --all
git add vexo/src/widgets/spacer.rs vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "feat(vexo): add Spacer leaf widget backed by SpacerRenderObject"
```

---

### Task 3: Integration test — `Spacer` inside `row!` pushes sibling to the right

**Files:**
- Create: `vexo/tests/spacer.rs`

**Interfaces:**
- Consumes: `vexo::Spacer`, `vexo::Layout`, `vexo::layout::TaffyLayoutEngine` (via `vexo::layout::LayoutEngine` trait), `vexo::render_objects::SpacerRenderObject`, `vexo::core::Size`.
- Produces: nothing (test-only file).

- [ ] **Step 1: Write the failing test**

Create `vexo/tests/spacer.rs`:

```rust
//! End-to-end test for Spacer: verifies that a `Spacer` placed before a
//! fixed-width sibling inside a row container absorbs the leftover space
//! and pushes the sibling to the right edge.
//!
//! Mirrors the `chat_screen.rs` use case this widget was introduced to
//! replace (`MultiChild::empty(Layout::default().flex_grow(1.0))`).

use vexo::Size;
use vexo::layout::{Layout, LayoutEngine, TaffyLayoutEngine};
use vexo::render_objects::SpacerRenderObject;
use vexo::RenderObject;

fn create_test_font_system() -> glyphon::FontSystem {
    let font_data = vexo::resource::file::FONT.to_vec();
    let binary = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
    glyphon::FontSystem::new_with_fonts([binary])
}

#[test]
fn spacer_in_row_pushes_sibling_to_right_edge() {
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();

    // Spacer render object — created exactly the way `Spacer::new()` creates it.
    let mut spacer_ro = SpacerRenderObject::new();
    let spacer_node = {
        let mut ctx = vexo::LayoutContext::new(&mut engine, &mut font_system);
        spacer_ro.layout(&mut ctx, &[]).node
    };

    // Fixed-width sibling simulating the chat bubble.
    let bubble_node = engine.create_leaf(&Layout::default().width(80.0).height(20.0));

    // Row container 200px wide, 20px tall, holding [spacer, bubble].
    let row = engine.create_container(
        &Layout::row().width(200.0).height(20.0),
        &[spacer_node, bubble_node],
    );

    engine.compute(row, Size::new(200.0, 20.0), &mut font_system);

    let spacer_layout = engine.get_layout(spacer_node).expect("spacer has layout");
    let bubble_layout = engine.get_layout(bubble_node).expect("bubble has layout");

    // Spacer absorbs leftover width: 200 - 80 = 120.
    assert_eq!(spacer_layout.x(), 0.0);
    assert_eq!(spacer_layout.width(), 120.0);
    assert_eq!(spacer_layout.height(), 20.0);

    // Bubble is pushed to the right edge.
    assert_eq!(bubble_layout.x(), 120.0);
    assert_eq!(bubble_layout.width(), 80.0);
    assert_eq!(bubble_layout.height(), 20.0);

    // Total width adds up to the parent width.
    assert_eq!(spacer_layout.width() + bubble_layout.width(), 200.0);
}

#[test]
fn two_spacers_split_free_space_evenly() {
    let mut engine = TaffyLayoutEngine::new();
    let mut font_system = create_test_font_system();

    let mut spacer_a = SpacerRenderObject::new();
    let mut spacer_b = SpacerRenderObject::new();

    let spacer_a_node = {
        let mut ctx = vexo::LayoutContext::new(&mut engine, &mut font_system);
        spacer_a.layout(&mut ctx, &[]).node
    };
    let spacer_b_node = {
        let mut ctx = vexo::LayoutContext::new(&mut engine, &mut font_system);
        spacer_b.layout(&mut ctx, &[]).node
    };

    let bubble_node = engine.create_leaf(&Layout::default().width(50.0).height(20.0));

    let row = engine.create_container(
        &Layout::row().width(200.0).height(20.0),
        &[spacer_a_node, bubble_node, spacer_b_node],
    );

    engine.compute(row, Size::new(200.0, 20.0), &mut font_system);

    let a_layout = engine.get_layout(spacer_a_node).expect("spacer A has layout");
    let b_layout = engine.get_layout(spacer_b_node).expect("spacer B has layout");
    let bubble_layout = engine.get_layout(bubble_node).expect("bubble has layout");

    // Free space = 200 - 50 = 150, split evenly = 75 each.
    assert_eq!(a_layout.width(), 75.0);
    assert_eq!(b_layout.width(), 75.0);
    assert_eq!(bubble_layout.width(), 50.0);

    // Layout left-to-right: A at 0, bubble at 75, B at 125.
    assert_eq!(a_layout.x(), 0.0);
    assert_eq!(bubble_layout.x(), 75.0);
    assert_eq!(b_layout.x(), 125.0);
}
```

- [ ] **Step 2: Run the integration tests to verify they pass**

Run: `cargo test -p vexo --test spacer`
Expected: PASS — both tests green.

If `LayoutContext` is not re-exported at the `vexo::` crate root, change the test to use the full path `vexo::layout::LayoutContext` and re-run. Likewise for `Size` — if `vexo::core::Size` is not public, use `vexo::Size` (it is re-exported per `vexo/src/lib.rs:9`). Adjust imports to whatever compiles; do not weaken the assertions.

If `RenderBackend` / `RenderObject` imports are unused, remove them — Rust will warn. Keep only the imports the test actually uses.

- [ ] **Step 3: Format and commit**

```bash
cargo fmt --all
git add vexo/tests/spacer.rs
git commit -m "test(vexo): add Spacer integration tests for row layout distribution"
```

---

### Task 4: Migrate `chat_screen.rs` call sites to `Spacer::new()`

**Files:**
- Modify: `shared_app/src/chats/chat_screen.rs` (lines 8, 205, 216)

**Interfaces:**
- Consumes: `vexo::Spacer` (added in Task 2).
- Produces: nothing — pure refactor.

- [ ] **Step 1: Read the current imports and call sites**

Run: `rg -n "MultiChild" shared_app/src/chats/chat_screen.rs`
Expected output (3 matches):
```
shared_app/src/chats/chat_screen.rs:8:    FlexDirection, Image, ImageData, Key, Layout, LifecycleContext, MultiChild, RenderContext,
shared_app/src/chats/chat_screen.rs:205:            MultiChild::empty(Layout::default().flex_grow(1.0)),
shared_app/src/chats/chat_screen.rs:216:            MultiChild::empty(Layout::default().flex_grow(1.0)),
```

This confirms `MultiChild` is referenced only by these two call sites (lines 205 and 216). The import on line 8 will become unused after migration.

- [ ] **Step 2: Replace call site 1 (line 205)**

In `shared_app/src/chats/chat_screen.rs`, change:

```rust
        row! {
            MultiChild::empty(Layout::default().flex_grow(1.0)),
            bubble,
            me_avatar,
        }
```

to:

```rust
        row! {
            Spacer::new(),
            bubble,
            me_avatar,
        }
```

- [ ] **Step 3: Replace call site 2 (line 216)**

In the same file, change:

```rust
        row! {
            them_avatar,
            bubble,
            MultiChild::empty(Layout::default().flex_grow(1.0)),
        }
```

to:

```rust
        row! {
            them_avatar,
            bubble,
            Spacer::new(),
        }
```

- [ ] **Step 4: Update imports on line 8**

On line 8, remove `MultiChild` from the import list (it is now unused), and add `Spacer`. Verify whether `Layout` is still used elsewhere in the file:

Run: `rg -n "\bLayout::" shared_app/src/chats/chat_screen.rs`

If `Layout::` has no other matches in the file, also remove `Layout` from the import list. If it does, keep `Layout`.

The new line 8 (assuming `Layout` is still used elsewhere — verify first) becomes:

```rust
    FlexDirection, Image, ImageData, Key, Layout, LifecycleContext, RenderContext,
```

Add `Spacer` on whichever line of the import block makes the alphabetical / existing order consistent. If the import block is a single line, add `Spacer` in alphabetical position between `Shared`-equivalents and `Stack`-equivalents (or wherever the existing list ordering would put it — match the file's existing style, not strict alphabetical if the file isn't strict).

- [ ] **Step 5: Build shared_app**

Run: `cargo build -p shared_app`
Expected: BUILD SUCCEEDS with no warnings about unused imports.

If there is an unused-import warning for `Layout` (or any other symbol), remove that symbol from the import list and re-run. If there is a missing-import error for `Spacer`, ensure the import edit landed correctly.

- [ ] **Step 6: Run shared_app tests**

Run: `cargo test -p shared_app`
Expected: PASS — no regressions.

- [ ] **Step 7: Build the whole workspace to catch any cross-crate breakage**

Run: `cargo build --workspace`
Expected: BUILD SUCCEEDS.

- [ ] **Step 8: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: PASS — all tests green, including the new `vexo` Spacer tests and the migrated `shared_app` chat tests.

- [ ] **Step 9: Format and commit**

```bash
cargo fmt --all
git add shared_app/src/chats/chat_screen.rs
git commit -m "refactor(shared_app): use Spacer::new() instead of MultiChild::empty in chat_screen"
```

---

## Self-Review Notes

**Spec coverage:**
- Widget `Spacer` with `new()` + `with_key()` → Task 2.
- Reuses `LeafRenderObjectElement` → Task 2 Step 3.
- `SpacerRenderObject` with `Layout::default().flex_grow(1.0)` leaf, empty `paint()`/`hit_test()`/`children()` → Task 1 Step 3.
- `update_render_object` returns `NONE` → Task 2 Step 3 + test `spacer_update_render_object_returns_none`.
- Direction-agnostic (only `flex_grow`, no `flex_direction`) → baked into `SpacerRenderObject::layout()`. Verified behaviorally by Task 3 (spacer grows horizontally inside a row because the *parent* sets `flex_direction: row`).
- Multiple spacers split evenly → Task 3 `two_spacers_split_free_space_evenly`.
- Exports in `widgets/mod.rs`, `render_objects/mod.rs`, `lib.rs` → Tasks 1 Step 4, 2 Steps 4-5.
- Migration of `chat_screen.rs:205,216` → Task 4.
- Removal of unused `MultiChild` import → Task 4 Step 4.
- Out of scope items (configurable flex, full layout passthrough, `spacer!` macro) → not implemented, as specified.

**Placeholder scan:** No TBDs, no "similar to Task N", all code shown inline.

**Type consistency:** `SpacerRenderObject::new()` signature consistent across Task 1 (definition), Task 2 (call site in `create_render_object`), Task 3 (test instantiation). `Spacer::new()` consistent across Task 2 (definition) and Task 4 (call sites). `Layout::default().flex_grow(1.0)` consistent across spec, Task 1 implementation, and Task 3 test (where `Layout::row().width(200.0).height(20.0)` is the *parent's* layout, not the spacer's — this is correct).
