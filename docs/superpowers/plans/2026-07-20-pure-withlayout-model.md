# Pure WithLayout-Only Layout Model — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the macro-generated layout/style modifier methods on widgets with a pure model: `WithLayout` (thin carrier) for single-child layout, `MultiChild` (new widget) for multi-child layout, `DecoratedBox` (thin carrier) for decoration. Both macros (`layout_builder_methods!`, `modifier_methods!`, `modifier_fields!`) are deleted. `Flex`/`Column`/`Row` die, replaced by `MultiChild` + `Layout::column()`/`Layout::row()` constructors.

**Architecture:** All layout is built fluently on `Layout` (which already has builders). All decoration is built fluently on `Style` (which already has builders). `WithLayout::new(child, layout)` and `MultiChild::new(children, layout)` are the only layout entry points. `DecoratedBox::new(child)` + `.style(Style)` is the only decoration entry point. Leaves (`Text`, `Image`, `TextEditContent`) lose their `style`/`layout` fields — decoration/layout via wrappers. `Stack`/`Grid`/`IndexedStack` keep intrinsic default `Layout`s but lose fluent methods; customization via `with_layout(Layout)` setter that replaces the default.

**Tech Stack:** Rust, cargo workspace (`vexo`, `shared_app`, `vexo_uikit`, `desktop_demo` crates).

## Global Constraints

- Workspace dependency versions defined in root `Cargo.toml` — no version changes.
- No deprecation period — old API removed outright once migration completes. Internal codebase, no external consumers.
- Per `CLAUDE.md`: run `cargo build` after Rust edits, `cargo test` after implementing features. Never run `cargo run -p desktop_demo` — ask the user.
- Phased migration (Q9 II): introduce new mechanism first, migrate call sites file-by-file, delete old mechanism last. Each phase compiles independently.
- The `WidgetExt` trait does not exist in the codebase (only in old design docs). The macros (`layout_builder_methods!`, `modifier_methods!`) generate inherent methods on widget structs, not trait methods.
- **Deviation from Q3 (i):** The literal form `WithLayout::new(MultiChild::new(children), layout)` requires multi-child pass-through (a framework architectural change). This plan uses the pragmatic form `MultiChild::new(children, layout)` — `MultiChild` owns its Taffy node and `Layout` directly. `WithLayout` is for single-child layout only. This avoids the architectural change while honoring the design's spirit (Layout built fluently on `Layout`, no macro methods on widgets). See "Design Deviations" at the end.

---

## File Structure

### New files
- `vexo/src/widgets/multi_child.rs` — `MultiChild` widget (multi-child container with user-supplied `Layout`). Uses existing `ContainerElement` + `ContainerRenderObject`.

### Modified files
- `vexo/src/layout/style.rs` — add `Layout::column()`, `Layout::row()`, `Layout::stack()`, `Layout::grid()`, `Layout::display()` constructors.
- `vexo/src/widgets/mod.rs` — export `MultiChild`; delete `Flex`/`Column`/`Row` exports in final phase.
- `vexo/src/widgets/with_layout.rs` — remove `layout_builder_methods!()` call; `WithLayout` becomes thin carrier.
- `vexo/src/widgets/decorated_box.rs` — remove inherent `.background()`/`.border()`/`.corner_radius()`/`.clip()`/`.shadow()`/`.shadows()`/`.style()` methods; add `DecoratedBox::with_style(child, style)` constructor.
- `vexo/src/widgets/text.rs` — remove `style`/`layout` fields + `modifier_methods!()` call; update `TextRenderObject` construction.
- `vexo/src/widgets/image.rs` — same as `text.rs`.
- `vexo/src/widgets/text_edit_content.rs` — same.
- `vexo/src/widgets/container.rs` — delete `Flex`/`Column`/`Row` in final phase.
- `vexo/src/widgets/stack.rs` — remove `layout_builder_methods!()` + decoration methods; add `Stack::with_layout(Layout)`; lose `Style` field.
- `vexo/src/widgets/grid.rs` — same as `stack.rs`.
- `vexo/src/widgets/indexed_stack.rs` — same (keeps `index` field).
- `vexo/src/macros.rs` — delete `layout_builder_methods!`, `modifier_methods!`, `modifier_fields!` macros + their tests; retarget `column!`/`row!`/`children!`.
- `vexo/src/render_objects/container.rs` — no changes (`MultiChild` reuses it).
- `vexo/src/render_objects/text.rs` — remove `style`/`layout` storage from `TextRenderObject`.
- `vexo/src/render_objects/image.rs` — same.
- `vexo/src/render_objects/text_edit.rs` — same.
- Migration: `shared_app/src/**/*.rs`, `vexo_uikit/src/**/*.rs`, `vexo/src/integration_tests.rs`, `vexo/src/focus/integration_tests.rs`, `vexo/src/e2e_test.rs`.

---

## Phase 1: Add `Layout` Constructors (Additive)

### Task 1.1: Add `Layout::display()` setter

**Files:**
- Modify: `vexo/src/layout/style.rs` (inside the `impl Layout` block at line 321, in the "Box Model Builders" section or a new "Display" section)
- Test: `vexo/src/layout/style.rs` (extend `mod tests`)

**Interfaces:**
- Produces: `Layout::display(value: Display) -> Self` — fluent setter for the `display` field. Used by `Layout::grid()` in Task 1.4.

**Why first:** `Layout::grid()` needs to set `display: Grid`. Currently only `grid_layout()` in `widgets/grid.rs:20-24` sets it via direct field access. Adding the setter makes it available as a fluent builder.

- [ ] **Step 1: Write the failing test**

Add to `vexo/src/layout/style.rs` test module (before the closing `}` of `mod tests`):

```rust
    #[test]
    fn test_layout_display_setter() {
        let layout = Layout::default().display(Display::Grid);
        assert_eq!(layout.display, Some(Display::Grid));

        let layout = Layout::default().display(Display::Flex);
        assert_eq!(layout.display, Some(Display::Flex));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib layout::style::tests::test_layout_display_setter`
Expected: compile error — method `display` not found on `Layout`.

- [ ] **Step 3: Add the setter**

In `vexo/src/layout/style.rs`, inside the `impl Layout` block (after the `align_self` method around line 570, in the "Per-Item Alignment Builders" section or a new "Display" section after it):

```rust
    /// Set the display mode (block, flex, grid, none).
    ///
    /// Default is `Display::Block`. Set to `Display::Flex` for flexbox layout,
    /// `Display::Grid` for CSS Grid layout.
    pub fn display(mut self, value: Display) -> Self {
        self.display = Some(value);
        self
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo --lib layout::style::tests::test_layout_display_setter`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/layout/style.rs
git commit -m "feat: add Layout::display() fluent setter"
```

---

### Task 1.2: Add `Layout::column()` and `Layout::row()` constructors

**Files:**
- Modify: `vexo/src/layout/style.rs` (inside the `impl Layout` block, in the "Convenience Methods" section around line 631)
- Test: `vexo/src/layout/style.rs`

**Interfaces:**
- Produces:
  - `Layout::column() -> Self` — returns `Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch)`. Matches current `column_layout()` in `widgets/container.rs:43-47`.
  - `Layout::row() -> Self` — returns `Layout::default().flex_direction(FlexDirection::Row).align(AlignItems::Stretch)`. Matches current `row_layout()` in `widgets/container.rs:50-54`.
- Used by: `MultiChild` construction (Task 2.1), call-site migration (Phase 3).

- [ ] **Step 1: Write the failing test**

Add to `vexo/src/layout/style.rs` test module:

```rust
    #[test]
    fn test_layout_column_constructor() {
        let layout = Layout::column();
        assert_eq!(layout.flex_direction, Some(FlexDirection::Column));
        assert_eq!(layout.align_items, Some(AlignItems::Stretch));
        // Other fields stay at default
        assert!(layout.gap.is_none());
        assert!(layout.padding.is_none());
    }

    #[test]
    fn test_layout_row_constructor() {
        let layout = Layout::row();
        assert_eq!(layout.flex_direction, Some(FlexDirection::Row));
        assert_eq!(layout.align_items, Some(AlignItems::Stretch));
        assert!(layout.gap.is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib layout::style::tests::test_layout_column_constructor layout::style::tests::test_layout_row_constructor`
Expected: compile error — `Layout::column` / `Layout::row` do not exist.

- [ ] **Step 3: Add the constructors**

In `vexo/src/layout/style.rs`, inside the `impl Layout` "Convenience Methods" section (after `fill()` around line 633):

```rust
    /// Create a column flex layout: `flex_direction: Column` + `align_items: Stretch`.
    ///
    /// This is the Vexo equivalent of CSS `display: flex; flex-direction: column`.
    /// Children stretch to fill the cross-axis (width) by default.
    pub fn column() -> Self {
        Self::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
    }

    /// Create a row flex layout: `flex_direction: Row` + `align_items: Stretch`.
    ///
    /// This is the Vexo equivalent of CSS `display: flex; flex-direction: row`.
    pub fn row() -> Self {
        Self::default()
            .flex_direction(FlexDirection::Row)
            .align(AlignItems::Stretch)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib layout::style::tests::test_layout_column_constructor layout::style::tests::test_layout_row_constructor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/layout/style.rs
git commit -m "feat: add Layout::column() and Layout::row() constructors"
```

---

### Task 1.3: Add `Layout::stack()` constructor

**Files:**
- Modify: `vexo/src/layout/style.rs`
- Test: `vexo/src/layout/style.rs`

**Interfaces:**
- Produces: `Layout::stack() -> Self` — returns `Layout::default().flex_direction(FlexDirection::Column).align(AlignItems::Stretch).width_percent(1.0).height_percent(1.0).min_height(0.0)`. Matches `stack_layout()` in `widgets/stack.rs:51-58`.
- Used by: `Stack::new()` default (Phase 5), call-site migration.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn test_layout_stack_constructor() {
        let layout = Layout::stack();
        assert_eq!(layout.flex_direction, Some(FlexDirection::Column));
        assert_eq!(layout.align_items, Some(AlignItems::Stretch));
        assert_eq!(layout.width, Some(Dimension::Percent(1.0)));
        assert_eq!(layout.height, Some(Dimension::Percent(1.0)));
        assert_eq!(layout.min_height, Some(Dimension::Length(0.0)));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib layout::style::tests::test_layout_stack_constructor`
Expected: compile error.

- [ ] **Step 3: Add the constructor**

In `vexo/src/layout/style.rs`, after `Layout::row()`:

```rust
    /// Create a Stack layout: column + stretch + fills parent + `min_height: 0`.
    ///
    /// `min_height(0.0)` allows the stack to shrink below its content's
    /// min-content when the parent is shorter, matching CSS block layout
    /// semantics where `min-height: auto` is `0`.
    pub fn stack() -> Self {
        Self::default()
            .flex_direction(FlexDirection::Column)
            .align(AlignItems::Stretch)
            .width_percent(1.0)
            .height_percent(1.0)
            .min_height(0.0)
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo --lib layout::style::tests::test_layout_stack_constructor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/layout/style.rs
git commit -m "feat: add Layout::stack() constructor"
```

---

### Task 1.4: Add `Layout::grid()` constructor

**Files:**
- Modify: `vexo/src/layout/style.rs`
- Test: `vexo/src/layout/style.rs`

**Interfaces:**
- Produces: `Layout::grid() -> Self` — returns `Layout::default().display(Display::Grid)`. Matches `grid_layout()` in `widgets/grid.rs:20-24`.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn test_layout_grid_constructor() {
        let layout = Layout::grid();
        assert_eq!(layout.display, Some(Display::Grid));
        // Other fields stay at default
        assert!(layout.gap.is_none());
        assert!(layout.grid_template_columns.is_none());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib layout::style::tests::test_layout_grid_constructor`
Expected: compile error.

- [ ] **Step 3: Add the constructor**

In `vexo/src/layout/style.rs`, after `Layout::stack()`:

```rust
    /// Create a Grid layout: `display: Grid`.
    ///
    /// Use `.columns(...)` / `.rows(...)` to set the grid template.
    pub fn grid() -> Self {
        Self::default().display(Display::Grid)
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo --lib layout::style::tests::test_layout_grid_constructor`
Expected: PASS.

- [ ] **Step 5: Run full test suite to verify no regressions**

Run: `cargo test -p vexo --lib`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/layout/style.rs
git commit -m "feat: add Layout::grid() constructor"
```

---

## Phase 2: Add `MultiChild` Widget (Additive)

### Task 2.1: Create `MultiChild` widget

**Files:**
- Create: `vexo/src/widgets/multi_child.rs`
- Modify: `vexo/src/widgets/mod.rs` (add `mod multi_child;` + `pub use multi_child::MultiChild;`)

**Interfaces:**
- Produces: `MultiChild` widget with:
  - `MultiChild::new(children: Vec<Box<dyn Widget>>, layout: Layout) -> Self`
  - `MultiChild::with_key(self, key) -> Self`
  - `MultiChild::with_layout(self, layout: Layout) -> Self` — replaces the layout
  - `MultiChild::children(&self) -> &[Box<dyn Widget>]`
  - `MultiChild::layout_ref(&self) -> &Layout`
- Uses: existing `ContainerElement` (`vexo/src/elements/container.rs`) + `ContainerRenderObject` (`vexo/src/render_objects/container.rs`). No new element/render object types.
- Consumes: `Layout::column()` / `Layout::row()` from Task 1.2.

**Why reuses ContainerElement/ContainerRenderObject:** `MultiChild` is structurally identical to `Flex` — a multi-child container with a `Layout`. The only difference is no `Style` field (decoration goes on `DecoratedBox`). `ContainerRenderObject::new(layout)` (without style) already exists at `render_objects/container.rs:40`.

- [ ] **Step 1: Write the failing test**

Create `vexo/src/widgets/multi_child.rs` with the test module first:

```rust
//! MultiChild widget — a multi-child container with a user-supplied `Layout`.
//!
//! `MultiChild` is the Vexo replacement for `Flex`/`Column`/`Row`. It holds
//! N children and applies a `Layout` (flexbox, grid, or block) to them.
//! Unlike the old `Flex`, it has no `Style` field — decoration goes on
//! `DecoratedBox`.
//!
//! # Example
//!
//! ```ignore
//! use vexo::{MultiChild, Layout, Text};
//!
//! MultiChild::new(
//!     vec![Text::new("A").boxed(), Text::new("B").boxed()],
//!     Layout::column().gap(16.0),
//! )
//! ```

use super::container::ChildPush;
use super::{Element, Widget};
use crate::key::WidgetKey;
use crate::layout::Layout;
use crate::render_objects::ContainerRenderObject;
use crate::{RenderObject, UpdateResult};

/// A multi-child container with a user-supplied `Layout`.
///
/// The replacement for `Flex`/`Column`/`Row`. Pass a `Layout::column()`,
/// `Layout::row()`, `Layout::grid()`, or `Layout::default()` (block) to
/// control how children are arranged. For decoration (background, border,
/// etc.), wrap in `DecoratedBox`.
pub struct MultiChild {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
}

impl MultiChild {
    /// Create a new `MultiChild` with the given children and layout.
    pub fn new(children: Vec<Box<dyn Widget>>, layout: Layout) -> Self {
        Self {
            key: None,
            children,
            layout,
        }
    }

    /// Create an empty `MultiChild` with the given layout; add children via `.push()`.
    pub fn empty(layout: Layout) -> Self {
        Self {
            key: None,
            children: Vec::new(),
            layout,
        }
    }

    /// Set the widget key.
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Replace the layout.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Add a child widget.
    ///
    /// Accepts any `impl Widget` or `Option<Box<dyn Widget>>` (for conditional children).
    pub fn push(mut self, child: impl ChildPush + 'static) -> Self {
        child.push_into(&mut self.children);
        self
    }

    /// Get the children.
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// Get the layout.
    pub fn layout_ref(&self) -> &Layout {
        &self.layout
    }
}

impl Default for MultiChild {
    fn default() -> Self {
        Self::empty(Layout::default())
    }
}

impl Clone for MultiChild {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            children: self.children.iter().map(|c| c.clone_boxed()).collect(),
            layout: self.layout.clone(),
        }
    }
}

impl Widget for MultiChild {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{FlexDirection, Layout};
    use crate::Text;

    #[test]
    fn test_multi_child_new_with_children() {
        let mc = MultiChild::new(
            vec![Text::new("A").boxed(), Text::new("B").boxed()],
            Layout::column(),
        );
        assert_eq!(mc.children().len(), 2);
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Column));
    }

    #[test]
    fn test_multi_child_empty_then_push() {
        let mc = MultiChild::empty(Layout::column())
            .push(Text::new("A"))
            .push(Text::new("B"));
        assert_eq!(mc.children().len(), 2);
    }

    #[test]
    fn test_multi_child_with_key() {
        let mc = MultiChild::empty(Layout::column()).with_key("my-mc");
        assert_eq!(mc.key(), Some(WidgetKey::Local(crate::Key::new("my-mc"))));
    }

    #[test]
    fn test_multi_child_with_layout_replaces() {
        let mc = MultiChild::empty(Layout::column()).with_layout(Layout::row().gap(8.0));
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Row));
        assert!(mc.layout_ref().gap.is_some());
    }

    #[test]
    fn test_multi_child_clone() {
        let mc = MultiChild::new(
            vec![Text::new("A").boxed()],
            Layout::column().gap(16.0),
        );
        let cloned = mc.clone();
        assert_eq!(cloned.children().len(), 1);
        assert!(cloned.layout_ref().gap.is_some());
    }

    #[test]
    fn test_multi_child_creates_container_render_object() {
        let mc = MultiChild::empty(Layout::column());
        let ro = mc.create_render_object();
        assert!(ro.as_any().downcast_ref::<ContainerRenderObject>().is_some());
    }

    #[test]
    fn test_multi_child_update_render_object_layout_change() {
        let mc1 = MultiChild::empty(Layout::default().padding(10.0));
        let mc2 = MultiChild::empty(Layout::default().padding(20.0));
        let mut ro = ContainerRenderObject::new(Layout::default().padding(10.0));
        assert_eq!(mc1.update_render_object(&mut ro), UpdateResult::NONE);
        assert!(mc2.update_render_object(&mut ro).contains(UpdateResult::LAYOUT));
    }
}
```

- [ ] **Step 2: Register the module**

In `vexo/src/widgets/mod.rs`, add after `mod container;` (line 6):

```rust
mod multi_child;
```

And in the public exports, add after `pub use container::{ChildPush, Column, Flex, Row};` (line 38):

```rust
pub use multi_child::MultiChild;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p vexo --lib widgets::multi_child`
Expected: all 7 tests PASS.

- [ ] **Step 4: Run full test suite to verify no regressions**

Run: `cargo test -p vexo --lib`
Expected: all tests pass (new module is additive).

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/multi_child.rs vexo/src/widgets/mod.rs
git commit -m "feat: add MultiChild widget (replacement for Flex/Column/Row)"
```

---

### Task 2.2: Retarget `children!` macro to return `Vec<Box<dyn Widget>>`

**Files:**
- Modify: `vexo/src/macros.rs:71-80` (the `children!` macro)

**Interfaces:**
- Produces: `children![a, b, c]` expands to `Vec<Box<dyn Widget>>` (currently it pushes onto a parent widget).
- Used by: `MultiChild::new(children![a, b, c], Layout::column())`.
- **Breaking change for existing `children!` call sites** — they currently use `children![parent, a, b, c]` form. Must migrate simultaneously.

**Why before Phase 3 migration:** The retargeted `children!` is the sugar for building `MultiChild`'s children vec. Migration of `column![]`/`row![]` (which currently use `Flex::column().push(...)`) needs `children!` to return a Vec.

- [ ] **Step 1: Find all existing `children!` call sites**

Run: `rg -n "children!\[" vexo/src shared_app/src vexo_uikit/src`
Expected: list of call sites. Each must be migrated to the new form.

- [ ] **Step 2: Migrate existing `children!` call sites to explicit `.push()` chains**

For each call site `children![parent, a, b, c]`, rewrite as `parent.push(a).push(b).push(c)`. This is a mechanical transformation. Example:

Before:
```rust
children![Flex::column().gap(16.0),
    Text::new("Title"),
    Text::new("Body"),
]
```

After:
```rust
Flex::column().gap(16.0)
    .push(Text::new("Title"))
    .push(Text::new("Body"))
```

Run: `cargo build -p vexo -p shared_app -p vexo_uikit`
Expected: compiles (old `children!` still works, call sites no longer use it).

- [ ] **Step 3: Update the `children!` macro**

In `vexo/src/macros.rs`, replace the `children!` macro (lines 57-80) with:

```rust
/// Build a `Vec<Box<dyn Widget>>` from child expressions.
///
/// Each child must implement `ChildPush` (any `impl Widget` or
/// `Option<Box<dyn Widget>>` for conditional children). The resulting
/// `Vec` is typically passed to `MultiChild::new(children, layout)`.
///
/// # Example
///
/// ```ignore
/// MultiChild::new(children![Text::new("A"), Text::new("B")], Layout::column())
/// ```
#[macro_export]
macro_rules! children {
    ($($child:expr),* $(,)?) => {{
        let mut __vexo_children: Vec<::std::boxed::Box<dyn $crate::Widget>> = Vec::new();
        $(
            $crate::widgets::ChildPush::push_into($child, &mut __vexo_children);
        )*
        __vexo_children
    }};
}
```

- [ ] **Step 4: Update `children!` tests in `macros.rs`**

In `vexo/src/macros.rs` test module, replace the existing `children_macro_*` tests (around line 633-688) with:

```rust
    #[test]
    fn children_macro_builds_vec() {
        let kids: Vec<Box<dyn crate::Widget>> = children![
            crate::Text::new("A"),
            crate::Text::new("B"),
            crate::Text::new("C"),
        ];
        assert_eq!(kids.len(), 3);
    }

    #[test]
    fn children_macro_single_child() {
        let kids: Vec<Box<dyn crate::Widget>> = children![crate::Text::new("Only"),];
        assert_eq!(kids.len(), 1);
    }

    #[test]
    fn children_macro_no_children() {
        let kids: Vec<Box<dyn crate::Widget>> = children![];
        assert_eq!(kids.len(), 0);
    }

    #[test]
    fn children_macro_with_multi_child() {
        use crate::layout::Layout;
        let mc = crate::MultiChild::new(
            children![crate::Text::new("A"), crate::Text::new("B")],
            Layout::column().gap(16.0),
        );
        assert_eq!(mc.children().len(), 2);
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p vexo --lib macros::tests`
Expected: all macro tests pass.

- [ ] **Step 6: Run full build to verify no regressions**

Run: `cargo build -p vexo -p shared_app -p vexo_uikit`
Expected: compiles cleanly.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/macros.rs
git commit -m "refactor: retarget children! macro to return Vec<Box<dyn Widget>>"
```

---

## Phase 3: Migrate Call Sites (File-by-File)

This phase migrates ~177 call sites across `shared_app`, `vexo_uikit`, and `vexo` integration tests. The old API (`Flex::column()`, `.padding()`, `.background()`, `column![]`, `row![]`) still works — both old and new forms coexist during this phase. Migrate file-by-file, commit after each file compiles + tests pass.

### Migration Patterns

Use these patterns to transform call sites:

**Pattern A: `Flex::column()` / `Flex::row()` / `Column::new()` / `Row::new()` → `MultiChild`**

Before:
```rust
Flex::column().gap(16.0).push(Text::new("A")).push(Text::new("B"))
```

After:
```rust
MultiChild::new(
    children![Text::new("A"), Text::new("B")],
    Layout::column().gap(16.0),
)
```

Before:
```rust
Column::new().gap(16.0).padding(8.0)
    .push(Text::new("Title"))
    .push(Text::new("Body"))
```

After:
```rust
MultiChild::new(
    children![Text::new("Title"), Text::new("Body")],
    Layout::column().gap(16.0).padding(8.0),
)
```

Before:
```rust
let mut col = Flex::column().gap(8.0);
col = col.push(child1);
if show { col = col.push(child2.boxed()); }
col
```

After:
```rust
let mut kids: Vec<Box<dyn Widget>> = vec![child1.boxed()];
if show { kids.push(child2.boxed()); }
MultiChild::new(kids, Layout::column().gap(8.0))
```

**Pattern B: `column![a, b, c]` / `row![a, b, c]` → `MultiChild`**

Before:
```rust
column![Text::new("A"), Text::new("B")]
```

After:
```rust
MultiChild::new(children![Text::new("A"), Text::new("B")], Layout::column())
```

**Pattern C: `.padding(8.0)` / `.width(100.0)` / `.flex_grow(1.0)` etc. on widgets → `WithLayout` wrapper**

Before:
```rust
Text::new("Hi").padding(8.0).background(Color::RED)
```

After:
```rust
DecoratedBox::new(
    WithLayout::new(Text::new("Hi"), Layout::default().padding(8.0))
).style(Style::default().background(Color::RED))
```

Note: order matters. Outer = painted last = visually on top. `DecoratedBox` wraps `WithLayout` wraps `Text` → background paints over the padded area.

Before (just layout):
```rust
Text::new("Hi").padding(8.0)
```

After:
```rust
WithLayout::new(Text::new("Hi"), Layout::default().padding(8.0))
```

Before (flex_grow on a child of a flex):
```rust
Flex::column().push(Text::new("A").flex_grow(1.0))
```

After:
```rust
MultiChild::new(
    children![WithLayout::new(Text::new("A"), Layout::default().flex_grow(1.0))],
    Layout::column(),
)
```

Before (flex_fill):
```rust
ScrollView::new(column).flex_fill()
```

After:
```rust
WithLayout::new(ScrollView::new(column), Layout::flex_fill())
```

**Pattern D: `.background(Color::RED)` / `.border(...)` / `.corner_radius(...)` / `.clip()` on widgets → `DecoratedBox` wrapper**

Before:
```rust
Text::new("Hi").background(Color::RED).border(Color::BLACK, 1.0).corner_radius(8.0)
```

After:
```rust
DecoratedBox::new(Text::new("Hi"))
    .style(Style::default().background(Color::RED).border(Color::BLACK, 1.0).corner_radius(8.0))
```

Note: `DecoratedBox`'s `.style(Style)` method and inherent `.background()` etc. still exist at this phase (removed in Phase 5). Prefer `.style(Style::default()...)` form to prepare for Phase 5.

**Pattern E: `.background(...)` on `Flex`/`Stack`/`Grid` → `DecoratedBox` wrapper**

Before:
```rust
Flex::column().gap(16.0).background(Color::RED).push(...)
```

After:
```rust
DecoratedBox::new(
    MultiChild::new(children![...], Layout::column().gap(16.0))
).style(Style::default().background(Color::RED))
```

**Pattern F: `GestureDetector::new(content).with_layout(layout)` → unchanged**

`GestureDetector` has an **inherent** `with_layout` method at `gesture_detector.rs:103` that sets `self.layout` directly (no wrapping). Call sites that use it are already safe and do NOT migrate. Verify each `GestureDetector.with_layout(...)` call site resolves to the inherent method (not a trait default — there is no trait default anymore, so this is automatic).

---

### Task 3.1: Migrate `shared_app` call sites

**Files (migrate one file at a time, commit after each):**
- `shared_app/src/widgets/avatar.rs`
- `shared_app/src/me/profile_screen.rs`
- `shared_app/src/chats/conversation_list.rs`
- `shared_app/src/chats/chat_screen.rs`
- `shared_app/src/contacts/contacts_screen.rs`
- Any other `shared_app/src/**/*.rs` file using old patterns (run `rg -n "\.(padding|background|flex_grow|flex_fill|margin|width|height|border|corner_radius|clip|gap)\(" shared_app/src` after each file to track progress).

**Procedure per file:**

- [ ] **Step 1: Identify all migration sites in the file**

Run: `rg -n "Flex::|Column::|Row::|column!\[|row!\[|\.(padding|background|border|corner_radius|clip|gap|flex_grow|flex_fill|width|height|margin|align|justify|inset|absolute)\(" <file>`

- [ ] **Step 2: Apply the migration patterns above to each site**

Transform each call site per the patterns. Use `Layout::default()...` for layout construction, `Style::default()...` for style construction.

- [ ] **Step 3: Add necessary imports**

Each migrated file needs:
```rust
use vexo::{MultiChild, WithLayout, DecoratedBox, Layout, Style, ChildPush};
use vexo::children;  // if using children! macro
```

Remove unused imports (`Flex`, `Column`, `Row`, `column!`, `row!` if the macro is removed).

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p shared_app`
Expected: compiles cleanly.

- [ ] **Step 5: Run tests**

Run: `cargo test -p shared_app`
Expected: all tests pass (if any).

- [ ] **Step 6: Commit**

```bash
git add <file>
git commit -m "refactor: migrate <file> to MultiChild/WithLayout/DecoratedBox model"
```

Repeat for each file in the list.

---

### Task 3.2: Migrate `vexo_uikit` call sites

**Files (migrate one file at a time):**
- `vexo_uikit/src/button.rs`
- `vexo_uikit/src/navigation.rs`
- `vexo_uikit/src/tab_bar.rs`
- Any other `vexo_uikit/src/**/*.rs` file using old patterns.

**Procedure:** Same as Task 3.1, per file.

- [ ] **Step 1-N:** Apply the same procedure as Task 3.1 per file.

Run: `cargo build -p vexo_uikit && cargo test -p vexo_uikit` after each file.

---

### Task 3.3: Migrate `vexo` integration tests and e2e tests

**Files:**
- `vexo/src/integration_tests.rs`
- `vexo/src/focus/integration_tests.rs`
- `vexo/src/e2e_test.rs`
- `vexo/src/passthrough_integration.rs` (if it exists and uses old patterns)

**Procedure:** Same as Task 3.1.

- [ ] **Step 1-N:** Apply the same procedure per file.

Run: `cargo test -p vexo` after each file.

**Special note for `e2e_test.rs:807`:** The comment referencing "WidgetExt sizing bug" is historical — the `WidgetExt` trait doesn't exist. Update the comment to reference the new model if the test itself migrates.

---

### Task 3.4: Verify no old-pattern call sites remain

- [ ] **Step 1: Search for remaining old-pattern usage**

Run:
```bash
rg -n "Flex::column|Flex::row|Flex::new|Column::new|Row::new|column!\[|row!\[" vexo/src shared_app/src vexo_uikit/src
```
Expected: no matches (or only matches in `vexo/src/widgets/container.rs` which is deleted in Phase 8).

- [ ] **Step 2: Search for remaining macro-method usage on widgets**

Run:
```bash
rg -n "Text::new\(.*\)\.(padding|background|border|corner_radius|clip|margin|width|height|flex_grow|flex_fill|gap|align|justify|inset|absolute)" vexo/src shared_app/src vexo_uikit/src
```
Expected: no matches (except in widget test files that will be updated in Phases 4-5).

- [ ] **Step 3: Build and test the full workspace**

Run: `cargo build && cargo test`
Expected: everything compiles and passes.

- [ ] **Step 4: Commit any remaining fixes**

```bash
git add -A
git commit -m "chore: verify no old-pattern layout call sites remain"
```

---

## Phase 4: Make `WithLayout` Thin

### Task 4.1: Remove `layout_builder_methods!()` from `WithLayout`

**Files:**
- Modify: `vexo/src/widgets/with_layout.rs:19` (remove `use crate::layout_builder_methods;`)
- Modify: `vexo/src/widgets/with_layout.rs:290-292` (remove the `impl WithLayout { layout_builder_methods!(); }` block)
- Modify: `vexo/src/widgets/with_layout.rs:14-18` (remove the now-unused layout imports guarded by `#[allow(unused_imports)]`)
- Test: `vexo/src/widgets/with_layout.rs` (update tests that call `.gap()` etc. on `WithLayout`)

**Interfaces:**
- Removes: ~30 inherent methods from `WithLayout` (`padding`, `gap`, `flex_grow`, etc.).
- Keeps: `WithLayout::new(child, layout)`, `with_key(key)`, `layout_ref()`.
- **Precondition:** All call sites must already pass `Layout` to `WithLayout::new(...)` directly (completed in Phase 3). No call site should do `WithLayout::new(...).padding(8.0)`.

- [ ] **Step 1: Verify no call site uses `WithLayout` fluent methods**

Run: `rg -n "WithLayout::new\(.*\)\." vexo/src shared_app/src vexo_uikit/src`
Inspect each match — none should call layout methods (`.padding()`, `.gap()`, etc.). They may call `.boxed()` or `.with_key()` which remain.

- [ ] **Step 2: Remove the macro invocation**

In `vexo/src/widgets/with_layout.rs`, delete lines 290-292:

```rust
impl WithLayout {
    layout_builder_methods!();
}
```

Also delete line 19: `use crate::layout_builder_methods;`

- [ ] **Step 3: Remove now-unused layout imports**

In `vexo/src/widgets/with_layout.rs`, lines 14-18 are guarded by `#[allow(unused_imports)]`. Remove the entire block if no longer needed:

```rust
#[allow(unused_imports)]
use crate::layout::{
    AlignContent, AlignItems, AlignSelf, Dimension, EdgeInsets, FlexDirection, FlexWrap, Inset,
    JustifyContent, Layout, LayoutNodeKey, Overflow, Position,
};
```

Keep only `Layout, LayoutNodeKey` (still used by `WithLayout`'s fields and methods):

```rust
use crate::layout::{Layout, LayoutNodeKey};
```

- [ ] **Step 4: Update `with_layout.rs` tests**

In `vexo/src/widgets/with_layout.rs` test module, update `test_with_layout_gap_preserves_padding` (line 470-474) — it currently calls `.gap(4.0)` on `WithLayout`. Rewrite to pass `Layout` directly:

```rust
    #[test]
    fn test_with_layout_gap_preserves_padding() {
        let w = WithLayout::new(
            Text::new("Hello"),
            Layout::default().padding(10.0).gap(4.0),
        );
        assert!(w.layout_ref().padding.is_some());
        assert!(w.layout_ref().gap.is_some());
    }
```

- [ ] **Step 5: Build and test**

Run: `cargo build -p vexo && cargo test -p vexo --lib widgets::with_layout`
Expected: compiles + tests pass.

- [ ] **Step 6: Run full workspace tests**

Run: `cargo build && cargo test`
Expected: everything passes.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/widgets/with_layout.rs
git commit -m "refactor: make WithLayout a thin carrier (remove layout_builder_methods!)"
```

---

## Phase 5: Make `DecoratedBox` Thin

### Task 5.1: Remove inherent style methods from `DecoratedBox`

**Files:**
- Modify: `vexo/src/widgets/decorated_box.rs:296-355` (remove `.style()`, `.background()`, `.border()`, `.corner_radius()`, `.clip()`, `.shadow()`, `.shadows()` methods)
- Add: `DecoratedBox::with_style(child, style)` constructor (replaces `DecoratedBox::new(child).style(style)` pattern)
- Test: `vexo/src/widgets/decorated_box.rs` (update tests)

**Interfaces:**
- Removes: inherent style-setting methods on `DecoratedBox`.
- Keeps: `DecoratedBox::new(child)`, `with_key(key)`, `child()`, `style_ref()`.
- Adds: `DecoratedBox::with_style(child, style) -> Self` — constructor that takes a pre-built `Style`.
- **Precondition:** All call sites must already use `.style(Style::default()...)` form (from Phase 3 migration Pattern D). This task replaces `.style(style)` with `with_style(child, style)` at the constructor level.

- [ ] **Step 1: Verify all call sites use `.style(Style::default()...)` form**

Run: `rg -n "DecoratedBox::new\(.*\)\.(background|border|corner_radius|clip|shadow|shadows|style)" vexo/src shared_app/src vexo_uikit/src`
Expected: no matches — all call sites should already use `.style(Style::default()...)` or be migrated to `DecoratedBox::with_style(child, style)`.

If any call sites still use `.background()` etc. on `DecoratedBox`, migrate them first:
- `DecoratedBox::new(child).background(RED)` → `DecoratedBox::with_style(child, Style::default().background(RED))`
- `DecoratedBox::new(child).style(Style::default()...)` → `DecoratedBox::with_style(child, Style::default()...)`

- [ ] **Step 2: Add `with_style` constructor**

In `vexo/src/widgets/decorated_box.rs`, in the `impl DecoratedBox` block (after `new` around line 294):

```rust
    /// Create a new `DecoratedBox` with a child and a pre-built `Style`.
    ///
    /// This is the primary constructor. Build the `Style` fluently:
    /// `DecoratedBox::with_style(child, Style::default().background(RED).border(BLACK, 1.0))`.
    pub fn with_style(child: impl Widget + 'static, style: Style) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            style,
        }
    }
```

- [ ] **Step 3: Remove the inherent style methods**

In `vexo/src/widgets/decorated_box.rs`, delete the methods at lines 296-339:
- `pub fn style(mut self, style: Style) -> Self`
- `pub fn background(mut self, color: Color) -> Self`
- `pub fn border(mut self, color: Color, width: f32) -> Self`
- `pub fn corner_radius(mut self, radius: f32) -> Self`
- `pub fn clip(mut self) -> Self`
- `pub fn shadow(mut self, shadow: BoxShadow) -> Self`
- `pub fn shadows(mut self, shadows: Vec<BoxShadow>) -> Self`

Keep: `new`, `with_style`, `with_key`, `child`, `style_ref`.

- [ ] **Step 4: Update `decorated_box.rs` tests**

In the test module, replace any test that calls `.background()` etc. on `DecoratedBox` with `with_style` form. Example:

Before:
```rust
let w = DecoratedBox::new(Text::new("Hi")).background(Color::RED);
```

After:
```rust
let w = DecoratedBox::with_style(Text::new("Hi"), Style::default().background(Color::RED));
```

- [ ] **Step 5: Build and test**

Run: `cargo build -p vexo && cargo test -p vexo --lib widgets::decorated_box`
Expected: compiles + tests pass.

- [ ] **Step 6: Run full workspace tests**

Run: `cargo build && cargo test`
Expected: everything passes.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/widgets/decorated_box.rs
git commit -m "refactor: make DecoratedBox a thin carrier (remove inherent style methods)"
```

---

## Phase 6: Strip `style`/`layout` Fields from Leaves

### Task 6.1: Strip `style`/`layout` from `Text`

**Files:**
- Modify: `vexo/src/widgets/text.rs` (remove `style`/`layout` fields, `modifier_methods!()` call, `Style`/`Layout` imports)
- Modify: `vexo/src/render_objects/text.rs` (remove `style`/`layout` storage + setters from `TextRenderObject`)
- Test: both files
- **Precondition:** No call site uses `Text::new(...).padding(...)` etc. (migrated in Phase 3 to `WithLayout::new(Text::new(...), ...)`).

- [ ] **Step 1: Verify no call site uses `Text`'s modifier methods**

Run: `rg -n "Text::new\(.*\)\.(padding|background|border|corner_radius|clip|margin|width|height|flex_grow|flex_fill|gap|align|justify|inset|absolute)" vexo/src shared_app/src vexo_uikit/src`
Expected: no matches outside `vexo/src/widgets/text.rs` test module.

- [ ] **Step 2: Remove `style`/`layout` fields from `Text`**

In `vexo/src/widgets/text.rs`:

Remove from the struct (lines 14-26):
```rust
    style: Style,
    layout: Layout,
```

Remove from `new()` (lines 31-39):
```rust
            style: Style::default(),
            layout: Layout::default(),
```

Remove from `Clone` impl (lines 100-110):
```rust
            style: self.style.clone(),
            layout: self.layout.clone(),
```

Remove `use crate::modifier_methods;` (line 10) and `use crate::style::Style;` (line 11) and `use crate::layout::Layout;` (line 9).

Remove the entire `impl Text { modifier_methods!(); }` block (line 96-97, including the `impl` wrapper).

- [ ] **Step 3: Update `Text::create_render_object`**

In `vexo/src/widgets/text.rs`, update `create_render_object` (lines 124-133) to not pass style/layout:

```rust
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(
            TextRenderObject::new(&self.content)
                .with_font_size(self.font_size)
                .with_color(self.color)
                .with_font_family(self.font_family.clone()),
        )
    }
```

- [ ] **Step 4: Update `Text::update_render_object`**

Remove the `set_style` and `set_layout` calls (lines 157-162):

```rust
    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(text_ro) = render_object
            .as_any_mut()
            .downcast_mut::<TextRenderObject>()
        {
            let mut result = UpdateResult::NONE;
            if text_ro.set_content(&self.content) {
                result |= UpdateResult::LAYOUT;
            }
            if text_ro.set_font_size(self.font_size) {
                result |= UpdateResult::LAYOUT;
            }
            if text_ro.set_color(self.color) {
                result |= UpdateResult::PAINT;
            }
            if text_ro.set_font_family(self.font_family.clone()) {
                result |= UpdateResult::LAYOUT;
            }
            result
        } else {
            UpdateResult::ALL
        }
    }
```

- [ ] **Step 5: Remove `style`/`layout` from `TextRenderObject`**

In `vexo/src/render_objects/text.rs`, remove the `style`/`layout` fields, their setters (`set_style`, `set_layout`), and the `with_style`/`with_layout` builder methods. Update `layout()` to use `Layout::default()` internally (the render object still needs a Taffy leaf node for text measurement).

- [ ] **Step 6: Update `text.rs` tests**

Remove tests that exercise `Text::new(...).padding(...)` / `.background(...)` etc. (lines 207-237, 263-271, 296-298, 319-326). These patterns are now invalid — decoration/layout goes on wrappers.

- [ ] **Step 7: Build and test**

Run: `cargo build && cargo test -p vexo --lib widgets::text`
Expected: compiles + tests pass.

- [ ] **Step 8: Commit**

```bash
git add vexo/src/widgets/text.rs vexo/src/render_objects/text.rs
git commit -m "refactor: strip style/layout fields from Text (decoration via DecoratedBox, layout via WithLayout)"
```

---

### Task 6.2: Strip `style`/`layout` from `Image`

**Files:**
- Modify: `vexo/src/widgets/image.rs`
- Modify: `vexo/src/render_objects/image.rs`
- Test: both files

**Procedure:** Same as Task 6.1, but for `Image`. Remove `style`/`layout` fields, `modifier_methods!()` call, and the `set_style`/`set_layout` calls in `update_render_object`. Update `ImageRenderObject` to not store style/layout.

- [ ] **Steps 1-8:** Mirror Task 6.1's procedure for `Image`. The `avatar.rs` widget in `shared_app` (already migrated in Phase 3) uses `WithLayout`/`DecoratedBox` wrappers instead of `Image::new(...).width(...).corner_radius(...)`.

Run: `cargo build && cargo test -p vexo --lib widgets::image`
Commit: `refactor: strip style/layout fields from Image`

---

### Task 6.3: Strip `style`/`layout` from `TextEditContent`

**Files:**
- Modify: `vexo/src/widgets/text_edit_content.rs`
- Modify: `vexo/src/render_objects/text_edit.rs`
- Test: both files

**Procedure:** Same as Task 6.1, but for `TextEditContent`. This is the leaf widget produced by `TextEdit`'s build method.

- [ ] **Steps 1-8:** Mirror Task 6.1's procedure.

Run: `cargo build && cargo test -p vexo --lib widgets::text_edit_content`
Commit: `refactor: strip style/layout fields from TextEditContent`

---

## Phase 7: Update `Stack`/`Grid`/`IndexedStack`

### Task 7.1: Update `Stack` — remove macro calls, add `with_layout`

**Files:**
- Modify: `vexo/src/widgets/stack.rs`
- Test: `vexo/src/widgets/stack.rs`

**Interfaces:**
- Removes: `layout_builder_methods!()` call (line 104), inherent decoration methods (`.background()`, `.border()`, `.corner_radius()`, `.clip()` at lines 106-125).
- Removes: `Style` field (line 68) — decoration goes on `DecoratedBox`.
- Keeps: `Layout` field with `stack_layout()` default (intrinsic).
- Adds: `Stack::with_layout(layout)` — replaces the default `Layout`.
- Changes: `create_render_object` uses `ContainerRenderObject::new(self.layout.clone())` (no style).

- [ ] **Step 1: Remove `Style` field and decoration methods**

In `vexo/src/widgets/stack.rs`:
- Remove `style: Style` from the struct (line 68).
- Remove `style: Style::default()` from `new()` (line 78).
- Remove `style: self.style.clone()` from `Clone` impl (line 139).
- Remove the entire `impl Stack { layout_builder_methods!(); ... }` block (lines 103-125).
- Remove `use crate::layout_builder_methods;` (line 37) and `use crate::style::Style;` (line 38).
- Remove `use crate::core::Color;` (line 36, now unused).

- [ ] **Step 2: Add `with_layout` method**

In the `impl Stack` block (after `layout()` method around line 100), the existing `layout(self, layout)` method already replaces the layout. **Keep it as `with_layout` for naming consistency with `MultiChild`** — or keep `layout()` if you prefer the existing name. Decision: keep `layout()` to avoid breaking any call sites that use `Stack::new().layout(...)`.

Actually, rename to `with_layout` for consistency with `MultiChild::with_layout`:

```rust
    /// Replace the layout.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
```

Remove the old `pub fn layout(mut self, layout: Layout) -> Self` (line 97-100).

- [ ] **Step 3: Update `create_render_object`**

Change `create_render_object` (line 155-160) to use `ContainerRenderObject::new` (no style):

```rust
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(ContainerRenderObject::new(self.layout.clone()))
    }
```

- [ ] **Step 4: Update `update_render_object`**

Remove the `set_style` call (line 176):

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

- [ ] **Step 5: Update tests**

Remove `test_stack_background` (line 229-232). Update any test that calls `.background()` etc. on `Stack` to use `DecoratedBox::with_style(Stack::new(...), Style::default().background(...))`.

- [ ] **Step 6: Build and test**

Run: `cargo build -p vexo && cargo test -p vexo --lib widgets::stack`
Expected: compiles + tests pass.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/widgets/stack.rs
git commit -m "refactor: strip Style field and macro methods from Stack"
```

---

### Task 7.2: Update `Grid` — remove macro calls, add `with_layout`

**Files:**
- Modify: `vexo/src/widgets/grid.rs`
- Test: `vexo/src/widgets/grid.rs`

**Procedure:** Same as Task 7.1, but for `Grid`. The grid-specific methods (`.columns()`, `.rows()`, `.grid_column()`, `.grid_row()`, `.grid_auto_flow()`, `.auto_rows()`, `.auto_columns()` at lines 99-133) are removed — users call these on `Layout` directly (`Layout::grid().columns(...)`).

- [ ] **Step 1: Remove `Style` field, `layout_builder_methods!()` call, decoration methods, grid-specific methods**

In `vexo/src/widgets/grid.rs`:
- Remove `style: Style` field, `style: Style::default()` from `new()`, `style: self.style.clone()` from `Clone`.
- Remove the entire `impl Grid { layout_builder_methods!(); ... }` block (lines 74-133).
- Remove `use crate::layout_builder_methods;` and `use crate::style::Style;`.
- Remove `use crate::core::Color;`.

- [ ] **Step 2: Add `with_layout` method**

```rust
    /// Replace the layout.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
```

Remove the old `pub fn layout(mut self, layout: Layout) -> Self` (line 55-58).

- [ ] **Step 3: Update `create_render_object` and `update_render_object`**

Same as Task 7.1 Steps 3-4 — use `ContainerRenderObject::new(self.layout.clone())`, remove `set_style` call.

- [ ] **Step 4: Update tests**

Remove `test_grid_modifier_background_returns_self`, `test_grid_modifier_chain`. Update `test_grid_gap_preserves_display` etc. to use `Grid::new().with_layout(Layout::grid().gap(12.0))` instead of `Grid::new().gap(12.0)`.

- [ ] **Step 5: Build and test**

Run: `cargo build -p vexo && cargo test -p vexo --lib widgets::grid`
Expected: compiles + tests pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/grid.rs
git commit -m "refactor: strip Style field and macro methods from Grid"
```

---

### Task 7.3: Update `IndexedStack` — remove macro calls, add `with_layout`

**Files:**
- Modify: `vexo/src/widgets/indexed_stack.rs`
- Test: `vexo/src/widgets/indexed_stack.rs`

**Procedure:** Same as Task 7.1, but for `IndexedStack`. **Keeps the `index: usize` field** (truly intrinsic). The `IndexedStackRenderObject` already handles `index` — no change to that.

- [ ] **Step 1: Remove `Style` field, `layout_builder_methods!()` call, decoration methods**

In `vexo/src/widgets/indexed_stack.rs`:
- Remove `style: Style` field (line 69).
- Remove `style: Style::default()` from `new()` (line 80).
- Remove `style: self.style.clone()` from `Clone` impl (line 145).
- Remove the `impl IndexedStack { layout_builder_methods!(); ... }` block (lines 108-130).
- Remove `use crate::layout_builder_methods;` and `use crate::style::Style;`.

- [ ] **Step 2: Add `with_layout` method**

```rust
    /// Replace the layout.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
```

- [ ] **Step 3: Update `create_render_object`**

Change `IndexedStackRenderObject::new_with_style(self.index, self.layout.clone(), self.style.clone())` (line 162-166) to `IndexedStackRenderObject::new(self.index, self.layout.clone())`. Update `IndexedStackRenderObject` to remove the `Style` parameter (or use a `new_with_style` with `Style::default()`).

- [ ] **Step 4: Update `update_render_object`**

Remove the `set_style` call (line 184):

```rust
    fn update_render_object(&self, render_object: &mut dyn RenderObject) -> UpdateResult {
        if let Some(ro) = render_object
            .as_any_mut()
            .downcast_mut::<IndexedStackRenderObject>()
        {
            let index_changed = ro.set_index(self.index);
            let layout_changed = ro.set_layout(self.layout.clone());
            if index_changed || layout_changed {
                UpdateResult::LAYOUT
            } else {
                UpdateResult::NONE
            }
        } else {
            UpdateResult::ALL
        }
    }
```

- [ ] **Step 5: Build and test**

Run: `cargo build -p vexo && cargo test -p vexo --lib widgets::indexed_stack`
Expected: compiles + tests pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/src/widgets/indexed_stack.rs
git commit -m "refactor: strip Style field and macro methods from IndexedStack"
```

---

## Phase 8: Delete `Flex`/`Column`/`Row` and Old Macros

### Task 8.1: Delete `Flex`/`Column`/`Row` types

**Files:**
- Modify: `vexo/src/widgets/container.rs` — delete `Flex`, `Column`, `Row`, `column_layout()`, `row_layout()`. Keep `ChildPush` trait (used by `MultiChild`).
- Modify: `vexo/src/widgets/mod.rs` — remove `pub use container::{Column, Flex, Row};` (keep `ChildPush`).

**Precondition:** All call sites migrated to `MultiChild` (Phase 3 complete). No remaining references to `Flex`/`Column`/`Row` outside `container.rs`.

- [ ] **Step 1: Verify no references to `Flex`/`Column`/`Row` remain**

Run: `rg -n "\bFlex\b|\bColumn\b|\bRow\b" vexo/src shared_app/src vexo_uikit/src | grep -v "container.rs" | grep -v "mod.rs"`
Expected: no matches (or only matches in comments/docs that should be updated).

- [ ] **Step 2: Delete `Flex`/`Column`/`Row` from `container.rs`**

In `vexo/src/widgets/container.rs`, delete:
- `column_layout()` function (lines 43-47).
- `row_layout()` function (lines 50-54).
- `Flex` struct (lines 60-65) and all its `impl` blocks (lines 67-219).
- `Column` struct (line 230) and its `impl` (lines 232-239).
- `Row` struct (line 250) and its `impl` (lines 252-259).
- The entire `#[cfg(test)] mod tests` block (lines 261-404) — tests for `Flex`/`Column`/`Row` are obsolete.

Keep:
- `ChildPush` trait (lines 12-28) — still used by `MultiChild::push`.
- The `use` imports for `ChildPush`'s dependencies.

- [ ] **Step 3: Update `mod.rs` exports**

In `vexo/src/widgets/mod.rs`, change line 38:

```rust
pub use container::{ChildPush, Column, Flex, Row};
```

to:

```rust
pub use container::ChildPush;
```

- [ ] **Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: compiles + all tests pass.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/container.rs vexo/src/widgets/mod.rs
git commit -m "refactor: delete Flex/Column/Row types (replaced by MultiChild)"
```

---

### Task 8.2: Delete `column!`/`row!` macros

**Files:**
- Modify: `vexo/src/macros.rs` (delete `column!` and `row!` macros, lines 3-37)

- [ ] **Step 1: Verify no call sites use `column![]`/`row![]`**

Run: `rg -n "column!\[|row!\[" vexo/src shared_app/src vexo_uikit/src`
Expected: no matches.

- [ ] **Step 2: Delete the macros**

In `vexo/src/macros.rs`, delete the `column!` macro (lines 3-19) and the `row!` macro (lines 21-37). Also delete the `grid!` macro (lines 39-55) if it's unused — check first:

Run: `rg -n "grid!\[" vexo/src shared_app/src vexo_uikit/src`

If unused, delete it too. If used, migrate call sites to `MultiChild::new(children![...], Layout::grid())` first.

- [ ] **Step 3: Build and test**

Run: `cargo build && cargo test`
Expected: compiles + tests pass.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/macros.rs
git commit -m "refactor: delete column!/row!/grid! macros (replaced by MultiChild + children!)"
```

---

### Task 8.3: Delete `layout_builder_methods!`/`modifier_methods!`/`modifier_fields!` macros

**Files:**
- Modify: `vexo/src/macros.rs` (delete the three macro definitions + their tests)

**Precondition:** No remaining invocations of these macros (verified in Phases 4-7).

- [ ] **Step 1: Verify no invocations remain**

Run: `rg -n "layout_builder_methods!|modifier_methods!|modifier_fields!" vexo/src`
Expected: only matches in `macros.rs` itself (the definitions).

- [ ] **Step 2: Delete the macro definitions**

In `vexo/src/macros.rs`, delete:
- `layout_builder_methods!` macro (lines 82-261).
- `modifier_fields!` macro (lines 263-284).
- `modifier_methods!` macro (lines 286-424).
- The entire `#[cfg(test)] mod tests` block (lines 426-689) — tests for the deleted macros. Keep only the `children_macro_*` tests (from Task 2.2).

- [ ] **Step 3: Build and test**

Run: `cargo build && cargo test -p vexo --lib macros`
Expected: compiles + tests pass (only `children_macro_*` tests remain).

- [ ] **Step 4: Run full workspace tests**

Run: `cargo build && cargo test`
Expected: everything passes.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/macros.rs
git commit -m "refactor: delete layout_builder_methods!/modifier_methods!/modifier_fields! macros"
```

---

## Phase 9: Final Verification

### Task 9.1: Full workspace build and test

- [ ] **Step 1: Clean build**

Run: `cargo clean && cargo build`
Expected: compiles cleanly with no warnings (or only pre-existing warnings).

- [ ] **Step 2: Full test suite**

Run: `cargo test`
Expected: all tests pass across all crates.

- [ ] **Step 3: Verify no old API remnants**

Run:
```bash
rg -n "Flex::column|Flex::row|Flex::new|Column::new|Row::new" vexo/src shared_app/src vexo_uikit/src
rg -n "layout_builder_methods!|modifier_methods!|modifier_fields!" vexo/src
rg -n "column!\[|row!\[" vexo/src shared_app/src vexo_uikit/src
rg -n "\.(padding|background|border|corner_radius|clip|gap|flex_grow|flex_fill)\(" vexo/src/widgets/text.rs vexo/src/widgets/image.rs vexo/src/widgets/text_edit_content.rs
```
Expected: no matches (all old API is gone).

- [ ] **Step 4: Commit any final cleanup**

```bash
git add -A
git commit -m "chore: final cleanup after pure WithLayout migration"
```

---

### Task 9.2: Update CLAUDE.md and documentation

**Files:**
- Modify: `CLAUDE.md` (update the "Web Developer API Mapping" table and any references to `Flex`/`Column`/`Row`/`Container`).
- Modify: any design docs in `docs/superpowers/specs/` that reference the old API (add a note pointing to the new model, don't rewrite history).

- [ ] **Step 1: Update CLAUDE.md API mapping table**

Replace entries:
- `Column::new()` / `Row::new()` → `MultiChild::new(children, Layout::column())` / `Layout::row()`
- `Container` (if mentioned) → `DecoratedBox::with_style(WithLayout::new(child, layout), style)`
- `.padding()` / `.background()` on widgets → `WithLayout::new(child, Layout::default().padding(...))` / `DecoratedBox::with_style(child, Style::default().background(...))`

- [ ] **Step 2: Add a note to relevant design docs**

In `docs/superpowers/specs/2026-07-20-remove-widgetext-layout-methods-design.md`, add a section at the top noting the design has been superseded by the "Pure WithLayout-Only" model implemented in this plan. Link to this plan.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md docs/superpowers/specs/
git commit -m "docs: update CLAUDE.md and design docs for pure WithLayout model"
```

---

## Design Deviations

### Deviation 1: `MultiChild` takes `Layout` directly (not wrapped in `WithLayout`)

**Q3 (i) specified:** `WithLayout::new(MultiChild::new([...]), Layout::column().gap(16.0))`.

**This plan implements:** `MultiChild::new(children, Layout::column().gap(16.0))`.

**Reason:** The Q3 (i) form requires multi-child pass-through — `MultiChild` would have no Taffy node, and the wrapping `WithLayout`'s Taffy node would directly parent the grandchildren. This is a significant architectural change to the layouter (`vexo/src/layouter.rs:152-155`) and `RenderObject::layout()` signature (returns single `LayoutResult`, can't represent multi-child pass-through).

The pragmatic form (`MultiChild` owns its Taffy node + Layout) avoids the architectural change while honoring the design's spirit:
- `Layout` is built fluently on `Layout` (no macro methods).
- `MultiChild` has no fluent methods — just `new`, `with_key`, `with_layout`, `push`, `children`, `layout_ref`.
- `WithLayout` is for single-child layout only (padding/sizing on leaves).
- No `Style` on `MultiChild` — decoration via `DecoratedBox`.

**Cost:** `MultiChild` has a `Layout` field, making it structurally similar to the old `Flex`. The difference is no macro-generated methods, no `Style` field, and `Layout` is built externally via `Layout::column()` etc.

**If the user wants the literal Q3 (i) form:** Add a follow-up plan to extend the layouter for multi-child pass-through. The extension requires:
1. `MultiChildRenderObject::is_pass_through() == true`.
2. `MultiChildRenderObject::layout()` returns no node (requires changing `RenderObject::layout()` to return `Option<LayoutResult>`).
3. The layouter flattens pass-through ROs when collecting child nodes (recurse into their children instead of using their `layout_node()`).

---

## Self-Review

### Spec coverage
- Q1 (i) Column/Row/Flex die → Task 8.1 ✓
- Q2 Single config-bag WithLayout → Task 4.1 (WithLayout thin) + Task 2.1 (MultiChild) ✓
- Q3 (i) MultiChild replaces them → Task 2.1 ✓ (with Deviation 1)
- Q4 (α) children! retargeted → Task 2.2 ✓
- Q5 (iv) WithLayout thin carrier → Task 4.1 ✓
- Q6 (II) intrinsic-on-container → Task 7.1-7.3 (Stack/Grid/IndexedStack keep Layout field, lose methods) ✓
- Q7 (I) flex_grow stays in Layout → no change needed (Layout already has flex_grow) ✓
- Q8 (III) DecoratedBox thin carrier → Task 5.1 ✓
- Q9 (II) phased migration → Phase 3 ✓
- Q10 (I) column!/row! die, children! returns MultiChild → Task 2.2 + Task 8.2 ✓

### Placeholder scan
- No "TBD", "TODO", "implement later" in the plan.
- All code blocks contain actual code.
- Migration patterns (Phase 3) provide concrete before/after examples, not abstract instructions.

### Type consistency
- `MultiChild::new(children: Vec<Box<dyn Widget>>, layout: Layout)` — used consistently in Phase 3 patterns.
- `WithLayout::new(child, layout)` — used consistently.
- `DecoratedBox::with_style(child, style)` — introduced in Task 5.1, used in Phase 3 patterns.
- `Layout::column()` / `Layout::row()` / `Layout::stack()` / `Layout::grid()` — introduced in Phase 1, used throughout.
- `children!` returns `Vec<Box<dyn Widget>>` — used in `MultiChild::new(children![...], ...)`.

### Gaps
- `Grid`'s `grid_layout()` function (lines 20-24 of grid.rs) is replaced by `Layout::grid()` (Task 1.4). The `grid_layout()` function is removed in Task 7.2.
- `IndexedStackRenderObject::new_with_style` (used in indexed_stack.rs:162) needs a `new(index, layout)` variant without style. Task 7.3 Step 3 handles this.
- The `Layouter` extension for multi-child pass-through (Deviation 1) is explicitly out of scope for this plan.
