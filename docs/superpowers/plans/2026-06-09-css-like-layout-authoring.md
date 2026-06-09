# CSS-like Layout Authoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Vexo's layout authoring feel like CSS — same concepts, same mental model, accessible from the widget API.

**Architecture:** Add `Layout` support to containers (Column/Row/Grid), create a `WithLayout` wrapper widget for child-level layout, remove `Style::padding`, and add missing CSS layout properties to the `Layout` struct.

**Tech Stack:** Rust, Taffy 0.9.2 (layout engine), wgpu (rendering)

---

### Task 1: Add Missing Layout Properties to `Layout` Struct

**Files:**
- Modify: `vexo/src/layout/style.rs`

This task adds the new CSS layout properties that Taffy already supports but Vexo doesn't expose yet.

- [ ] **Step 1: Add new enums**

Add these enums before the `Layout` struct (before line 226):

```rust
/// Per-item cross-axis alignment (CSS `align-self`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlignSelf {
    #[default]
    Auto,
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

/// How to handle overflow (CSS `overflow`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Clip,
    Scroll,
}

/// Grid auto-placement direction (CSS `grid-auto-flow`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GridAutoFlow {
    #[default]
    Row,
    Column,
    RowDense,
    ColumnDense,
}
```

- [ ] **Step 2: Add new fields to `Layout` struct**

Add these fields to the `Layout` struct (after existing fields, around line 259):

```rust
    // Per-item alignment
    pub align_self: Option<AlignSelf>,

    // Sizing
    pub aspect_ratio: Option<f32>,

    // Overflow
    pub overflow_x: Option<Overflow>,
    pub overflow_y: Option<Overflow>,

    // Grid auto
    pub grid_auto_flow: Option<GridAutoFlow>,
    pub grid_auto_rows: Option<Vec<TrackSizing>>,
    pub grid_auto_columns: Option<Vec<TrackSizing>>,
```

- [ ] **Step 3: Add builder methods to `Layout`**

Add these builder methods after the existing ones (before the `to_taffy_style` method, around line 520):

```rust
    /// Set align-self (per-item cross-axis alignment).
    pub fn align_self(mut self, value: AlignSelf) -> Self {
        self.align_self = Some(value);
        self
    }

    /// Set aspect ratio (width / height).
    pub fn aspect_ratio(mut self, value: f32) -> Self {
        self.aspect_ratio = Some(value);
        self
    }

    /// Set overflow for both axes.
    pub fn overflow(mut self, value: Overflow) -> Self {
        self.overflow_x = Some(value);
        self.overflow_y = Some(value);
        self
    }

    /// Set overflow for x-axis only.
    pub fn overflow_x(mut self, value: Overflow) -> Self {
        self.overflow_x = Some(value);
        self
    }

    /// Set overflow for y-axis only.
    pub fn overflow_y(mut self, value: Overflow) -> Self {
        self.overflow_y = Some(value);
        self
    }

    /// Set grid auto-flow direction.
    pub fn grid_auto_flow(mut self, value: GridAutoFlow) -> Self {
        self.grid_auto_flow = Some(value);
        self
    }

    /// Set grid auto-rows sizing.
    pub fn auto_rows(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.grid_auto_rows = Some(sizes);
        self
    }

    /// Set grid auto-columns sizing.
    pub fn auto_columns(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.grid_auto_columns = Some(sizes);
        self
    }
```

- [ ] **Step 4: Add Taffy conversions for new enums**

Add conversion impls after the existing ones (after line 760):

```rust
impl AlignSelf {
    fn to_taffy(self) -> taffy::prelude::AlignSelf {
        use taffy::prelude::AlignItems as TaffyAlign;
        match self {
            AlignSelf::Auto => TaffyAlign::Stretch,
            AlignSelf::Start => TaffyAlign::Start,
            AlignSelf::End => TaffyAlign::End,
            AlignSelf::Center => TaffyAlign::Center,
            AlignSelf::Stretch => TaffyAlign::Stretch,
            AlignSelf::Baseline => TaffyAlign::Baseline,
        }
    }
}

impl Overflow {
    fn to_taffy(self) -> taffy::style::Overflow {
        match self {
            Overflow::Visible => taffy::style::Overflow::Visible,
            Overflow::Hidden => taffy::style::Overflow::Hidden,
            Overflow::Clip => taffy::style::Overflow::Clip,
            Overflow::Scroll => taffy::style::Overflow::Scroll,
        }
    }
}

impl GridAutoFlow {
    fn to_taffy(self) -> taffy::style::GridAutoFlow {
        match self {
            GridAutoFlow::Row => taffy::style::GridAutoFlow::Row,
            GridAutoFlow::Column => taffy::style::GridAutoFlow::Column,
            GridAutoFlow::RowDense => taffy::style::GridAutoFlow::RowDense,
            GridAutoFlow::ColumnDense => taffy::style::GridAutoFlow::ColumnDense,
        }
    }
}
```

- [ ] **Step 5: Add new fields to `to_taffy_style()`**

In the `to_taffy_style()` method, add these fields to the `taffy::Style { ... }` struct literal before the `..Default::default()`:

```rust
            // Per-item alignment
            align_self: self.align_self.map(|a| a.to_taffy()),

            // Sizing
            aspect_ratio: self.aspect_ratio,

            // Overflow
            overflow: taffy::geometry::Point {
                x: self.overflow_x.map(|o| o.to_taffy()).unwrap_or(taffy::style::Overflow::Visible),
                y: self.overflow_y.map(|o| o.to_taffy()).unwrap_or(taffy::style::Overflow::Visible),
            },

            // Grid auto
            grid_auto_flow: self.grid_auto_flow.map(|f| f.to_taffy()).unwrap_or_default(),
            grid_auto_rows: self.grid_auto_rows.as_ref()
                .map(|v| v.iter().map(|ts| taffy::prelude::minmax(ts.to_taffy_min(), ts.to_taffy_max())).collect())
                .unwrap_or_default(),
            grid_auto_columns: self.grid_auto_columns.as_ref()
                .map(|v| v.iter().map(|ts| taffy::prelude::minmax(ts.to_taffy_min(), ts.to_taffy_max())).collect())
                .unwrap_or_default(),
```

- [ ] **Step 6: Add unit tests for new properties**

Add tests in the `#[cfg(test)] mod tests` section:

```rust
    #[test]
    fn test_layout_align_self() {
        let layout = Layout::default().align_self(AlignSelf::Center);
        assert_eq!(layout.align_self, Some(AlignSelf::Center));
    }

    #[test]
    fn test_layout_aspect_ratio() {
        let layout = Layout::default().aspect_ratio(1.5);
        assert_eq!(layout.aspect_ratio, Some(1.5));
    }

    #[test]
    fn test_layout_overflow() {
        let layout = Layout::default().overflow(Overflow::Hidden);
        assert_eq!(layout.overflow_x, Some(Overflow::Hidden));
        assert_eq!(layout.overflow_y, Some(Overflow::Hidden));
    }

    #[test]
    fn test_layout_overflow_each() {
        let layout = Layout::default().overflow_x(Overflow::Hidden).overflow_y(Overflow::Scroll);
        assert_eq!(layout.overflow_x, Some(Overflow::Hidden));
        assert_eq!(layout.overflow_y, Some(Overflow::Scroll));
    }

    #[test]
    fn test_layout_grid_auto_flow() {
        let layout = Layout::default().grid_auto_flow(GridAutoFlow::Column);
        assert_eq!(layout.grid_auto_flow, Some(GridAutoFlow::Column));
    }

    #[test]
    fn test_layout_auto_rows() {
        let layout = Layout::default().auto_rows(vec![TrackSizing::Px(100.0)]);
        assert!(layout.grid_auto_rows.is_some());
        let rows = layout.grid_auto_rows.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn test_layout_auto_columns() {
        let layout = Layout::default().auto_columns(vec![TrackSizing::Fr(1.0), TrackSizing::Fr(2.0)]);
        assert!(layout.grid_auto_columns.is_some());
        let cols = layout.grid_auto_columns.unwrap();
        assert_eq!(cols.len(), 2);
    }
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p vexo`
Expected: All existing tests pass, all new tests pass.

- [ ] **Step 8: Commit**

```bash
git add vexo/src/layout/style.rs
git commit -m "feat: add align_self, aspect_ratio, overflow, grid_auto properties to Layout"
```

---

### Task 2: Export New Layout Types from Public API

**Files:**
- Modify: `vexo/src/layout/mod.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Update layout module re-exports**

In `vexo/src/layout/mod.rs`, add the new types to the pub use statement that re-exports from `style.rs`. Find the existing re-export line and add `AlignSelf`, `Overflow`, `GridAutoFlow`:

```rust
pub use style::{Layout, EdgeInsets, Dimension, Display, FlexDirection, FlexWrap, JustifyContent, AlignItems, AlignContent, Position, Inset, TrackSizing, GridPlacement, AlignSelf, Overflow, GridAutoFlow};
```

- [ ] **Step 2: Update crate-level re-exports**

In `vexo/src/lib.rs`, find the layout re-export line and add the new types. Look for the line that re-exports layout types (around line 100-110):

```rust
pub use layout::{Layout, EdgeInsets, Dimension, /* ... existing ... */, AlignSelf, Overflow, GridAutoFlow};
```

- [ ] **Step 3: Run build**

Run: `cargo build -p vexo`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/layout/mod.rs vexo/src/lib.rs
git commit -m "feat: export AlignSelf, Overflow, GridAutoFlow from public API"
```

---

### Task 3: Refactor ContainerRenderObject to Accept Layout

**Files:**
- Modify: `vexo/src/render_objects/container.rs`

This replaces `is_row: bool` with `layout: Layout` so that ContainerRenderObject uses whatever Layout the widget provides.

- [ ] **Step 1: Replace `is_row` with `layout` field**

In `ContainerRenderObject` struct, replace:

```rust
pub struct ContainerRenderObject {
    children: Vec<RenderObjectKey>,
    is_row: bool,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}
```

With:

```rust
pub struct ContainerRenderObject {
    children: Vec<RenderObjectKey>,
    layout: Layout,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}
```

- [ ] **Step 2: Replace constructors**

Replace `new_column()` and `new_row()` with a single `new(layout: Layout)` constructor:

```rust
impl ContainerRenderObject {
    /// Create a new container render object with the given layout.
    pub fn new(layout: Layout) -> Self {
        Self {
            children: Vec::new(),
            layout,
            computed_bounds: None,
            layout_node: None,
        }
    }
```

Remove `new_column()` and `new_row()` entirely. Remove `is_row()` getter.

- [ ] **Step 3: Update `layout()` method to use stored Layout**

Replace the `layout()` method body:

```rust
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &self.layout);
                ctx.engine().set_children(existing, child_nodes);
                LayoutResult {
                    node: existing,
                    size: Size::new(0.0, 0.0),
                }
            }
            None => {
                let node = ctx.engine().create_container(&self.layout, child_nodes);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::new(0.0, 0.0),
                }
            }
        }
    }
```

- [ ] **Step 4: Update internal tests in container.rs**

Update all test usages of `new_column()` and `new_row()`:

- Replace `ContainerRenderObject::new_column()` with `ContainerRenderObject::new(Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch))`
- Replace `ContainerRenderObject::new_row()` with `ContainerRenderObject::new(Layout::default().flex_direction(FlexDirection::Row).align(AlignItems::Stretch))`
- Replace `assert!(!obj.is_row())` with checking the layout's flex_direction
- Replace `assert!(obj.is_row())` with checking the layout's flex_direction
- Remove the `is_row` getter test or replace it with a layout field test

- [ ] **Step 5: Run tests**

Run: `cargo test -p vexo`
Expected: Tests fail at Column/Row `create_render_object()` which still call `new_column()`/`new_row()`. That's expected — we fix those in the next task. The container.rs internal tests should pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/render_objects/container.rs
git commit -m "refactor: ContainerRenderObject accepts Layout instead of is_row bool"
```

---

### Task 4: Add Layout to Column and Row Widgets

**Files:**
- Modify: `vexo/src/widgets/container.rs`
- Modify: `vexo/src/hit_test.rs` (fix test that calls `new_column()`)

- [ ] **Step 1: Add `layout` field and default Layout constants to Column**

In `Column` struct, add `layout` field:

```rust
pub struct Column {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
}
```

Add a constant for the default column layout at module level:

```rust
/// Default layout for Column: flex-direction column, align-items stretch.
fn column_layout() -> Layout {
    Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch)
}

/// Default layout for Row: flex-direction row, align-items stretch.
fn row_layout() -> Layout {
    Layout::default().flex_direction(FlexDirection::Row).align(AlignItems::Stretch)
}
```

Update `Column::new()`:

```rust
pub fn new() -> Self {
    Self {
        key: None,
        children: Vec::new(),
        layout: column_layout(),
    }
}
```

Add `.layout()` builder:

```rust
    /// Set the layout properties for this column.
    ///
    /// Overrides the default flex-direction and alignment.
    /// Pass a Layout to control padding, gap, justify, align, flex-wrap, etc.
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
```

Update `Clone` impl to include `layout` field.

Update `create_render_object()`:

```rust
fn create_render_object(&self) -> Box<dyn RenderObject> {
    Box::new(ContainerRenderObject::new(self.layout.clone()))
}
```

Add `update_render_object()` that diff-checks Layout changes:

```rust
fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
    if let Some(container_ro) = render_object
        .as_any_mut()
        .downcast_mut::<ContainerRenderObject>()
    {
        if container_ro.layout != self.layout {
            container_ro.layout = self.layout.clone();
            UpdateResult::LAYOUT
        } else {
            UpdateResult::NONE
        }
    } else {
        UpdateResult::ALL
    }
}
```

Wait — ContainerRenderObject doesn't have a public `layout` field. We need to add a setter. Add to ContainerRenderObject:

```rust
    /// Set the layout configuration.
    /// Returns true if the layout changed.
    pub fn set_layout(&mut self, layout: Layout) -> bool {
        if self.layout != layout {
            self.layout = layout;
            true
        } else {
            false
        }
    }
```

Then in Column/Row `update_render_object()`:

```rust
fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
    if let Some(container_ro) = render_object
        .as_any_mut()
        .downcast_mut::<ContainerRenderObject>()
    {
        if container_ro.set_layout(self.layout.clone()) {
            UpdateResult::LAYOUT
        } else {
            UpdateResult::NONE
        }
    } else {
        UpdateResult::ALL
    }
}
```

- [ ] **Step 2: Add `layout` field and builder to Row**

Same changes as Column, using `row_layout()` as default.

- [ ] **Step 3: Fix hit_test.rs test**

In `vexo/src/hit_test.rs` line 502, replace:

```rust
ContainerRenderObject::new_column()
```

With:

```rust
ContainerRenderObject::new(Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch))
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p vexo`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/container.rs vexo/src/hit_test.rs vexo/src/render_objects/container.rs
git commit -m "feat: Column and Row accept Layout via .layout() builder"
```

---

### Task 5: Remove Style::padding

**Files:**
- Modify: `vexo/src/style.rs`
- Modify: `vexo/src/widgets/decorated_container.rs`
- Modify: `shared_app/src/lib.rs`
- Modify: `vexo/src/widgets/text_edit.rs`
- Modify: `vexo/src/e2e_test.rs`

- [ ] **Step 1: Remove `padding` field from `Style` struct**

In `vexo/src/style.rs`, remove:

```rust
pub padding: Option<f32>,
```

Remove the `padding` builder method:

```rust
pub fn padding(mut self, value: f32) -> Self {
    self.padding = Some(value);
    self
}
```

Update `Style::new()` and `Style::default()` to not include padding.

- [ ] **Step 2: Remove padding merge logic from DecoratedContainerRenderObject**

In `vexo/src/widgets/decorated_container.rs`, in the `layout()` method (around line 102-109), remove the padding merge:

```rust
// REMOVE these lines:
if let Some(padding) = self.style.padding {
    layout = layout.padding(padding);
}
```

Also remove the padding-related change detection in `update_render_object()` (around lines 568-574):

```rust
// REMOVE:
let old_padding = container_ro.style().padding;
```

And simplify the change detection:

```rust
// REPLACE the complex padding/layout change logic with:
fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
    if let Some(container_ro) = render_object
        .as_any_mut()
        .downcast_mut::<DecoratedContainerRenderObject>()
    {
        let style_changed = container_ro.set_style(self.style.clone());
        let layout_changed = container_ro.set_layout(self.layout.clone());

        if layout_changed {
            UpdateResult::LAYOUT | UpdateResult::PAINT
        } else if style_changed {
            UpdateResult::PAINT
        } else {
            UpdateResult::NONE
        }
    } else {
        UpdateResult::ALL
    }
}
```

- [ ] **Step 3: Migrate all `Style::padding()` calls to `Layout::padding()`**

In `shared_app/src/lib.rs`, every place that uses `.style(Style::new().padding(X).background(Y).border(...))` needs to be split into `.style()` and `.layout()`. For example:

Before:
```rust
.style(Style::new().padding(24.0).background(Color::BLUE))
```

After:
```rust
.style(Style::new().background(Color::BLUE))
.layout(Layout::new().padding(24.0))
```

All 9 occurrences in shared_app/src/lib.rs (lines 17, 138, 214, 226, 235, 245, 258, 279, 295).

In `vexo/src/widgets/text_edit.rs` (line 503):
```rust
// Before:
.style(crate::Style::new().background(crate::core::Color::WHITE).border(border_color, border_width).corner_radius(4.0).padding(8.0))

// After:
.style(crate::Style::new().background(crate::core::Color::WHITE).border(border_color, border_width).corner_radius(4.0))
.layout(crate::Layout::new().padding(8.0))
```

In `vexo/src/e2e_test.rs` (lines 176, 239, 292, 403):
Same pattern — split `.style()` and `.layout()`.

- [ ] **Step 4: Run build and tests**

Run: `cargo build -p vexo && cargo test -p vexo && cargo build -p shared_app`
Expected: All compile and pass.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/style.rs vexo/src/widgets/decorated_container.rs shared_app/src/lib.rs vexo/src/widgets/text_edit.rs vexo/src/e2e_test.rs
git commit -m "refactor: remove Style::padding, move to Layout::padding"
```

---

### Task 6: Fix DecoratedContainer to Respect Layout

**Files:**
- Modify: `vexo/src/widgets/decorated_container.rs`

Now that Style::padding is gone, DecoratedContainer's render object still overrides `flex_direction` to Column. Fix this.

- [ ] **Step 1: Remove flex_direction override from render object layout()**

In `DecoratedContainerRenderObject::layout()`, replace:

```rust
fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
    let mut layout = self.layout.clone()
        .flex_direction(FlexDirection::Column)
        .align(AlignItems::Stretch);

    // Apply padding from style if set (overrides layout padding)
    if let Some(padding) = self.style.padding {
        layout = layout.padding(padding);
    }
    // ...
}
```

With:

```rust
fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
    match self.layout_node {
        Some(existing) => {
            ctx.engine().set_style(existing, &self.layout);
            ctx.engine().set_children(existing, child_nodes);
            LayoutResult {
                node: existing,
                size: Size::new(0.0, 0.0),
            }
        }
        None => {
            let node = ctx.engine().create_container(&self.layout, child_nodes);
            self.layout_node = Some(node);
            LayoutResult {
                node,
                size: Size::new(0.0, 0.0),
            }
        }
    }
}
```

- [ ] **Step 2: Remove unused import**

Remove `use crate::layout::{AlignItems, FlexDirection, Layout, LayoutNodeKey};` if `AlignItems` and `FlexDirection` are no longer used in the file (they were only used in the hardcoded layout construction).

Replace with: `use crate::layout::{Layout, LayoutNodeKey};`

- [ ] **Step 3: Run tests**

Run: `cargo test -p vexo && cargo build -p desktop_demo && cargo run -p desktop_demo`
Expected: Tests pass, desktop demo renders correctly.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/decorated_container.rs
git commit -m "fix: DecoratedContainer respects user-provided Layout instead of overriding flex_direction"
```

---

### Task 7: Create WithLayout Widget

**Files:**
- Create: `vexo/src/widgets/with_layout.rs`
- Modify: `vexo/src/widgets/mod.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Create the WithLayout widget file**

Create `vexo/src/widgets/with_layout.rs`:

```rust
//! WithLayout widget - applies layout properties to any child.
//!
//! The Vexo equivalent of inline styles on a child element in CSS.

use std::any::Any;

use crate::core::{Bounds, Logical, Point, Size};
use crate::elements::RenderObjectElement;
use crate::focus::attachment::FocusAttachment;
use crate::input::InputEvent;
use crate::layout::{Layout, LayoutNodeKey};
use crate::render::RenderCommand;
use crate::{
    Element, ElementContext, ElementKey, EventContext, HitTestContext, LayoutContext, LayoutResult,
    PaintContext, RenderObject, RenderObjectKey, UpdateResult, Widget, WidgetKey,
};

// ============================================================================
// WithLayoutRenderObject
// ============================================================================

/// Render object for WithLayout - applies layout but does not paint.
pub struct WithLayoutRenderObject {
    layout: Layout,
    child: Option<RenderObjectKey>,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}

impl WithLayoutRenderObject {
    /// Create a new WithLayout render object with the given layout.
    pub fn new(layout: Layout) -> Self {
        Self {
            layout,
            child: None,
            computed_bounds: None,
            layout_node: None,
        }
    }

    /// Set the layout configuration. Returns true if changed.
    pub fn set_layout(&mut self, layout: Layout) -> bool {
        if self.layout != layout {
            self.layout = layout;
            true
        } else {
            false
        }
    }
}

impl RenderObject for WithLayoutRenderObject {
    fn layout(&mut self, ctx: &mut LayoutContext, child_nodes: &[LayoutNodeKey]) -> LayoutResult {
        match self.layout_node {
            Some(existing) => {
                ctx.engine().set_style(existing, &self.layout);
                ctx.engine().set_children(existing, child_nodes);
                LayoutResult {
                    node: existing,
                    size: Size::new(0.0, 0.0),
                }
            }
            None => {
                let node = ctx.engine().create_container(&self.layout, child_nodes);
                self.layout_node = Some(node);
                LayoutResult {
                    node,
                    size: Size::new(0.0, 0.0),
                }
            }
        }
    }

    fn apply_layout(&mut self, ctx: &mut LayoutContext) {
        if let Some(node) = self.layout_node {
            if let Some(computed) = ctx.engine_ref().get_layout(node) {
                self.computed_bounds = Some(computed.bounds);
            }
        }
    }

    fn paint(&self, _ctx: &mut PaintContext) -> Vec<RenderCommand> {
        vec![]
    }

    fn hit_test(&self, position: Point<Logical>, _ctx: &HitTestContext) -> bool {
        match &self.computed_bounds {
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

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_child_id(&mut self, child: RenderObjectKey) {
        self.child = Some(child);
    }

    fn layout_node(&self) -> Option<LayoutNodeKey> {
        self.layout_node
    }

    fn computed_bounds(&self) -> Option<Bounds<Logical>> {
        self.computed_bounds
    }
}

// ============================================================================
// WithLayoutElement
// ============================================================================

/// Element for WithLayout widget.
pub struct WithLayoutElement {
    id: Option<ElementKey>,
    key: Option<WidgetKey>,
    render_object: Option<RenderObjectKey>,
    widget: Option<Box<dyn Widget>>,
    focus_attachment: Option<FocusAttachment>,
}

impl WithLayoutElement {
    /// Create a new WithLayout element.
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

impl Default for WithLayoutElement {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderObjectElement for WithLayoutElement {
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

impl Element for WithLayoutElement {
    fn mount(&mut self, context: &mut ElementContext) {
        let element_key = context.element_id;
        let parent_id = context.parent_focus_node_id();
        let node_id = context
            .focus_manager()
            .create_node_for_element(element_key, parent_id);
        if let Some(node_id) = node_id {
            self.focus_attachment = Some(FocusAttachment::new(node_id));
        }

        self.mount_render_object(context);

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

    fn can_update(&self, _widget: &dyn Any) -> bool {
        true
    }

    fn on_event(
        &mut self,
        _event: &InputEvent,
        _context: &mut EventContext,
        _state: &mut crate::element_state::StateStorage,
    ) -> Option<Box<dyn Any>> {
        None
    }

    fn rebuild(&mut self, new_widget: Box<dyn Any>, context: &mut ElementContext) {
        if let Ok(widget) = new_widget.downcast::<Box<dyn Widget>>() {
            self.widget = Some(*widget);

            if let Some(ro_id) = self.render_object {
                if let Some(ro) = context.get_render_object_mut(ro_id) {
                    let result = self
                        .widget
                        .as_ref()
                        .unwrap()
                        .update_render_object(ro.as_mut());

                    if result.contains(UpdateResult::LAYOUT) {
                        context.mark_needs_layout(ro_id);
                    }
                    if result.contains(UpdateResult::PAINT) {
                        context.mark_needs_paint(ro_id);
                    }
                }
            }

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
// WithLayout Widget
// ============================================================================

/// A widget that applies layout properties to a child.
///
/// The Vexo equivalent of inline styles on a child element in CSS.
///
/// # Example
///
/// ```ignore
/// // CSS: .item { flex: 1; align-self: center; margin: 10px; }
/// Text::new("Hello").with_layout(Layout::new().flex_grow(1).align_self(AlignSelf::Center).margin(10))
/// ```
pub struct WithLayout {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    layout: Layout,
}

impl WithLayout {
    /// Create a new WithLayout widget wrapping a child with the given layout.
    pub fn new(child: impl Widget + 'static, layout: Layout) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            layout,
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Clone for WithLayout {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            child: self.child.clone_boxed(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for WithLayout {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = WithLayoutElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(WithLayoutRenderObject::new(self.layout.clone()))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn child(&self) -> Option<&dyn Widget> {
        Some(self.child.as_ref())
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(wl_ro) = render_object
            .as_any_mut()
            .downcast_mut::<WithLayoutRenderObject>()
        {
            if wl_ro.set_layout(self.layout.clone()) {
                UpdateResult::LAYOUT
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
```

- [ ] **Step 2: Register WithLayout in widgets module**

In `vexo/src/widgets/mod.rs`, add:

```rust
mod with_layout;
```

And in the pub use section:

```rust
pub use with_layout::WithLayout;
```

- [ ] **Step 3: Add `.with_layout()` default method to Widget trait**

In `vexo/src/widgets/mod.rs`, add to the `Widget` trait:

```rust
    /// Wrap this widget with layout properties.
    ///
    /// The Vexo equivalent of inline styles on a child element in CSS.
    ///
    /// # Example
    ///
    /// ```ignore
    /// Text::new("Hello").with_layout(Layout::new().flex_grow(1))
    /// ```
    fn with_layout(self, layout: Layout) -> WithLayout
    where
        Self: Sized + 'static,
    {
        WithLayout::new(self, layout)
    }
```

Note: This requires adding `use crate::layout::Layout;` and `use super::with_layout::WithLayout;` imports at the top of `mod.rs`. Actually, since `WithLayout` is defined in the same module and re-exported, just use `WithLayout` directly. And `Layout` needs to be imported:

```rust
use crate::layout::Layout;
```

Add this at the top of `mod.rs` with the other use statements.

- [ ] **Step 4: Add WithLayout to crate-level re-exports**

In `vexo/src/lib.rs`, add `WithLayout` to the widgets re-export line:

```rust
pub use widgets::{Widget, Text, Column, Row, DecoratedContainer, GestureDetector, MouseRegion, TextEdit, TextEditState, TextEditingController, Transform, WithLayout};
```

- [ ] **Step 5: Run build and tests**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: Compiles and all tests pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/with_layout.rs vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "feat: add WithLayout widget and .with_layout() method on Widget trait"
```

---

### Task 8: Create Grid Widget

**Files:**
- Create: `vexo/src/widgets/grid.rs`
- Modify: `vexo/src/widgets/mod.rs`
- Modify: `vexo/src/lib.rs`

- [ ] **Step 1: Create the Grid widget file**

Create `vexo/src/widgets/grid.rs`:

```rust
//! Grid widget - CSS Grid layout container.

use super::{Element, Widget};
use super::super::key::WidgetKey;
use super::super::render_objects::ContainerRenderObject;
use super::super::{RenderObject, UpdateResult};
use crate::layout::{Display, Layout};

/// Default layout for Grid: display grid.
fn grid_layout() -> Layout {
    Layout::default().display(Display::Grid)
}

/// Grid widget - arranges children in a CSS Grid layout.
///
/// Use `.layout()` to set grid template columns/rows and other grid properties.
/// Use `.with_layout()` on children to set grid column/row placement.
///
/// # Example
///
/// ```ignore
/// Grid::new()
///     .layout(Layout::new()
///         .columns(vec![TrackSizing::Fr(1.0), TrackSizing::Fr(2.0)])
///         .rows(vec![TrackSizing::Auto, TrackSizing::Px(100.0)]))
///     .push(child1.with_layout(Layout::new().grid_column(GridPlacement::span(2))))
///     .push(child2)
/// ```
pub struct Grid {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
}

impl Grid {
    /// Create a new empty grid.
    pub fn new() -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout: grid_layout(),
        }
    }

    /// Set the key for this widget.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the layout properties for this grid.
    ///
    /// Use this to set `columns()`, `rows()`, `gap()`, `grid_auto_flow()`, etc.
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Add a child widget.
    pub fn push(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Grid {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for Grid {
    fn key(&self) -> Option<WidgetKey> {
        self.key.clone()
    }

    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = crate::elements::ContainerElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }

    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new(self.layout.clone()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(container_ro) = render_object
            .as_any_mut()
            .downcast_mut::<ContainerRenderObject>()
        {
            if container_ro.set_layout(self.layout.clone()) {
                UpdateResult::LAYOUT
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
```

- [ ] **Step 2: Register Grid in widgets module**

In `vexo/src/widgets/mod.rs`, add:

```rust
mod grid;
```

And in the pub use section:

```rust
pub use grid::Grid;
```

- [ ] **Step 3: Add Grid to crate-level re-exports**

In `vexo/src/lib.rs`, add `Grid` to the widgets re-export line.

- [ ] **Step 4: Add Display import to layout re-exports if missing**

Check that `Display` is re-exported from `vexo/src/layout/mod.rs` and `vexo/src/lib.rs`. It's already in the `Layout` struct's module, so it should already be exported. If not, add it.

- [ ] **Step 5: Run build and tests**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: Compiles and all tests pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/grid.rs vexo/src/widgets/mod.rs vexo/src/lib.rs
git commit -m "feat: add Grid widget with CSS grid layout support"
```

---

### Task 9: Add grid! Macro

**Files:**
- Modify: `vexo/src/macros.rs`

- [ ] **Step 1: Add grid! macro**

Add after the existing `row!` macro in `vexo/src/macros.rs`:

```rust
/// Create a `Grid` widget with children.
///
/// ```ignore
/// grid![child1, child2]
/// ```
/// expands to:
/// ```ignore
/// Grid::new().push(child1).push(child2)
/// ```
#[macro_export]
macro_rules! grid {
    ($($child:expr),* $(,)?) => {{
        let mut grid = $crate::Grid::new();
        $(grid = grid.push($child);)*
        grid
    }};
}
```

- [ ] **Step 2: Run build**

Run: `cargo build -p vexo`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add vexo/src/macros.rs
git commit -m "feat: add grid! macro for Grid widget construction"
```

---

### Task 10: Integration Test — CSS-like Layout Authoring

**Files:**
- Modify: `vexo/src/e2e_test.rs`
- Modify: `shared_app/src/lib.rs`

This task validates that the full CSS-like layout authoring flow works end-to-end by adding test cases and updating the demo app to use the new API.

- [ ] **Step 1: Add e2e test for Column with Layout**

Add a test in `vexo/src/e2e_test.rs`:

```rust
#[test]
fn test_column_with_layout() {
    let app = TestApp::new(|_state, _font_system| {
        Box::new(
            Column::new()
                .layout(Layout::new()
                    .padding(10)
                    .gap(5)
                    .justify(JustifyContent::Center)
                    .align(AlignItems::Start))
                .push(Text::new("First"))
                .push(Text::new("Second")),
        )
    });

    let frame = app.render_frame(800.0, 600.0);
    assert!(!frame.commands.is_empty());
}
```

- [ ] **Step 2: Add e2e test for WithLayout on children**

```rust
#[test]
fn test_with_layout_on_children() {
    let app = TestApp::new(|_state, _font_system| {
        Box::new(
            Row::new()
                .layout(Layout::new().gap(8))
                .push(Text::new("Left").with_layout(Layout::new().flex_grow(1)))
                .push(Text::new("Right").with_layout(Layout::new().width(100))),
        )
    });

    let frame = app.render_frame(800.0, 600.0);
    assert!(!frame.commands.is_empty());
}
```

- [ ] **Step 3: Add e2e test for Grid**

```rust
#[test]
fn test_grid_widget() {
    let app = TestApp::new(|_state, _font_system| {
        Box::new(
            Grid::new()
                .layout(Layout::new()
                    .columns(vec![TrackSizing::Fr(1.0), TrackSizing::Fr(1.0)])
                    .rows(vec![TrackSizing::Auto, TrackSizing::Auto])
                    .gap(8))
                .push(Text::new("A"))
                .push(Text::new("B"))
                .push(Text::new("C"))
                .push(Text::new("D")),
        )
    });

    let frame = app.render_frame(800.0, 600.0);
    assert!(!frame.commands.is_empty());
}
```

- [ ] **Step 4: Update shared_app demo to showcase new layout API**

Update one section of `shared_app/src/lib.rs` to use the new `.layout()` API on a Column or Row, demonstrating gap, justify, and padding from Layout instead of Style. For example, find a Column that uses padding via Style and convert it:

Before:
```rust
DecoratedContainer::new(column![...])
    .style(Style::new().padding(8.0).background(Color::BLUE))
```

After:
```rust
DecoratedContainer::new(
    Column::new()
        .layout(Layout::new().padding(8.0).gap(4))
        .push(...)
        .push(...)
)
    .style(Style::new().background(Color::BLUE))
```

- [ ] **Step 5: Run all tests and desktop demo**

Run: `cargo test -p vexo && cargo build -p desktop_demo && cargo run -p desktop_demo`
Expected: All tests pass, desktop demo renders correctly.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/e2e_test.rs shared_app/src/lib.rs
git commit -m "test: add integration tests for CSS-like layout authoring"
```

---

## Self-Review Checklist

### Spec Coverage

| Spec Section | Task |
|---|---|
| 1. WithLayout Widget | Task 7 |
| 2. Column/Row Accept Layout | Task 4 |
| 3. DecoratedContainer Layout Fix | Tasks 5 + 6 |
| 4. Grid Widget | Task 8 |
| 5. Missing Layout Properties | Tasks 1 + 2 |
| 6. No Convenience Widgets | N/A (by design) |

### Placeholder Scan

No TBDs, TODOs, or "implement later" patterns found.

### Type Consistency

- `Layout` struct fields and builder methods match across Tasks 1, 3, 4, 7, 8
- `ContainerRenderObject::new(Layout)` used consistently in Tasks 3, 4, 7, 8
- `set_layout()` method on ContainerRenderObject defined in Task 4, used in Tasks 4, 8
- `WithLayout` widget defined in Task 7, referenced in Tasks 8, 10
- `AlignSelf`, `Overflow`, `GridAutoFlow` enums defined in Task 1, exported in Task 2
