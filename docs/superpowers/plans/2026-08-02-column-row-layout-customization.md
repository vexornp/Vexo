# `column!`/`row!` Layout Customization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 45 fluent builder methods to `MultiChild` that mirror `Layout`'s instance builders, enabling `column! { A, B }.gap(8.0).padding(12.0)` and `MultiChild::empty(Layout::column()).flex_shrink(0.0)`.

**Architecture:** A single `macro_rules!` (`impl_layout_passthrough!`) inside `vexo/src/widgets/multi_child.rs` generates 45 one-line methods that delegate to the corresponding `Layout` builder, mutating `self.layout` in place and returning `self`. The `column!`/`row!` macros are unchanged — they already return `MultiChild`, so postfix chaining just works. Missing layout types are re-exported at the `vexo` crate root so external call sites can name them.

**Tech Stack:** Rust, `macro_rules!`, Vexo's existing `Layout` / `MultiChild` types. No new dependencies. No proc-macro changes.

**Spec:** `docs/superpowers/specs/2026-08-02-column-row-layout-customization-design.md`

## Global Constraints

- All changes are purely additive — no existing public API may change.
- The `column!`/`row!` macros in `vexo/vexo_macros/src/lib.rs` are NOT modified.
- `Layout` (`vexo/src/layout/style.rs`) is NOT modified.
- Generated method names and signatures must match `Layout`'s instance builders exactly (`vexo/src/layout/style.rs:365-683`) — same name, same arg order, same arg types.
- Methods **modify** `self.layout` (preserving other fields), distinct from the existing `with_layout()` which **replaces** the entire layout.
- Every step ends with `cargo build` or `cargo test` passing before committing.
- Commit message style: lowercase `feat(scope):` / `test(scope):` / `refactor(scope):` (match recent commits like `c0fee48 feat(conversation_list): ...`).

---

### Task 1: Add `impl_layout_passthrough!` macro, 45 fluent methods, re-exports, and unit tests

**Files:**
- Modify: `vexo/src/widgets/multi_child.rs` (add imports, macro definition, macro invocation, 9 unit tests)
- Modify: `vexo/src/lib.rs:58-61` (add missing layout type re-exports to crate root)

**Interfaces:**
- Consumes: `Layout`'s 45 instance builders (`vexo/src/layout/style.rs:365-683`) — `gap`, `padding`, `flex_shrink`, `justify`, `absolute`, `columns`, `overflow`, etc. Each takes `(mut self, ...) -> Self`.
- Produces: 45 inherent methods on `MultiChild` with the same names/signatures, each taking `(mut self, ...) -> Self` and delegating to `self.layout.$method(...)`.

- [ ] **Step 1: Write the 9 failing unit tests**

Open `vexo/src/widgets/multi_child.rs`. Find the `#[cfg(test)] mod tests` block (starts at line 147). The existing imports are:

```rust
use super::*;
use crate::layout::{FlexDirection, Layout};
use crate::Text;
```

Replace those three lines with the expanded imports (only types actually referenced by the new tests — `Logical`, `AlignItems`, `FlexWrap` are NOT used directly because types are inferred):

```rust
use super::*;
use crate::core::Size;
use crate::layout::{
    FlexDirection, JustifyContent, Layout, Overflow, Position, TrackSizing,
};
use crate::Text;
```

Then append these 9 tests at the end of the `tests` mod (after the existing `test_multi_child_update_render_object_layout_change` test, before the closing `}`):

```rust
    #[test]
    fn fluent_gap_preserves_column_direction() {
        let mc = MultiChild::empty(Layout::column()).gap(8.0);
        assert_eq!(mc.layout_ref().gap, Some(Size::new(8.0, 8.0)));
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Column));
    }

    #[test]
    fn fluent_padding_preserves_row_direction() {
        let mc = MultiChild::empty(Layout::row()).padding(12.0);
        let p = mc.layout_ref().padding.unwrap();
        assert_eq!(p.top, 12.0);
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Row));
    }

    #[test]
    fn fluent_flex_shrink_preserves_direction() {
        let mc = MultiChild::empty(Layout::row()).flex_shrink(0.0);
        assert_eq!(mc.layout_ref().flex_shrink, Some(0.0));
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Row));
    }

    #[test]
    fn fluent_justify_overrides_default() {
        let mc = MultiChild::empty(Layout::column()).justify(JustifyContent::SpaceBetween);
        assert_eq!(mc.layout_ref().justify_content, Some(JustifyContent::SpaceBetween));
    }

    #[test]
    fn fluent_columns_sets_grid_template() {
        let mc = MultiChild::empty(Layout::grid()).columns(vec![TrackSizing::Fr(1.0), TrackSizing::Fr(2.0)]);
        let cols = mc.layout_ref().grid_template_columns.as_ref().unwrap();
        assert_eq!(cols.len(), 2);
    }

    #[test]
    fn fluent_absolute_top_sets_position_and_inset() {
        let mc = MultiChild::empty(Layout::default()).absolute().top(10.0);
        assert_eq!(mc.layout_ref().position, Some(Position::Absolute));
        assert_eq!(mc.layout_ref().inset.unwrap().top, Some(10.0));
    }

    #[test]
    fn fluent_overflow_sets_both_axes() {
        let mc = MultiChild::empty(Layout::default()).overflow(Overflow::Hidden);
        assert_eq!(mc.layout_ref().overflow_x, Some(Overflow::Hidden));
        assert_eq!(mc.layout_ref().overflow_y, Some(Overflow::Hidden));
    }

    #[test]
    fn fluent_chaining_sets_all_three() {
        let mc = MultiChild::empty(Layout::column())
            .gap(8.0)
            .padding(12.0)
            .flex_shrink(0.0);
        assert_eq!(mc.layout_ref().gap, Some(Size::new(8.0, 8.0)));
        assert!(mc.layout_ref().padding.is_some());
        assert_eq!(mc.layout_ref().flex_shrink, Some(0.0));
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Column));
    }

    #[test]
    fn fluent_flex_direction_overrides_column_macro_default() {
        // column! sets FlexDirection::Column; calling .flex_direction(Row) overrides it.
        // No error — methods are low-level setters, user intent honored.
        let mc = crate::column! { Text::new("A") }.flex_direction(FlexDirection::Row);
        assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Row));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vexo --lib widgets::multi_child::tests 2>&1 | head -40`
Expected: COMPILE ERROR — `no method named 'gap' found for struct 'MultiChild'` (and similar for the other 8 methods). This confirms the tests exercise the not-yet-existing API.

- [ ] **Step 3: Add imports to `multi_child.rs` (non-test section)**

In `vexo/src/widgets/multi_child.rs`, the current imports (lines 19-24) are:

```rust
use super::container::ChildPush;
use super::{Element, Widget};
use crate::key::WidgetKey;
use crate::layout::Layout;
use crate::render_objects::ContainerRenderObject;
use crate::{RenderObject, UpdateResult};
```

Replace the single `use crate::layout::Layout;` line with the expanded layout imports plus the core types:

```rust
use crate::core::{Logical, Size};
use crate::layout::{
    AlignContent, AlignItems, AlignSelf, Display, FlexDirection, FlexWrap, GridAutoFlow,
    GridPlacement, JustifyContent, Layout, Overflow, Position, TrackSizing,
};
```

(The full block becomes:)

```rust
use super::container::ChildPush;
use super::{Element, Widget};
use crate::core::{Logical, Size};
use crate::key::WidgetKey;
use crate::layout::{
    AlignContent, AlignItems, AlignSelf, Display, FlexDirection, FlexWrap, GridAutoFlow,
    GridPlacement, JustifyContent, Layout, Overflow, Position, TrackSizing,
};
use crate::render_objects::ContainerRenderObject;
use crate::{RenderObject, UpdateResult};
```

- [ ] **Step 4: Add the `impl_layout_passthrough!` macro definition**

In `vexo/src/widgets/multi_child.rs`, immediately BEFORE the `impl MultiChild {` block (i.e., after the `MultiChild` struct definition's closing `}` and its `impl Default` / `impl Clone` blocks, around line 103), insert the macro definition:

```rust
/// Generate fluent `Layout` passthrough methods on `MultiChild`.
///
/// Each entry `$method($args)` becomes `pub fn $method(mut self, $args) -> Self`
/// that delegates to `self.layout.$method($args)`, mutating `self.layout` in
/// place. Names and signatures mirror `Layout`'s instance builders exactly
/// (`vexo/src/layout/style.rs`), so the API reads identically to `Layout`'s.
macro_rules! impl_layout_passthrough {
    ($($method:ident($($arg:ident: $ty:ty),*)),* $(,)?) => {
        $(
            #[doc = concat!("Set [`Layout::", stringify!($method), "`] on this container's layout.")]
            #[doc = ""]
            #[doc = "Mirrors `Layout::", stringify!($method), "`; modifies the existing layout in place,"]
            #[doc = "preserving other fields (e.g. the `column`/`row` direction set by `column!`/`row!`)."]
            pub fn $method(mut self, $($arg: $ty),*) -> Self {
                self.layout = self.layout.$method($($arg),*);
                self
            }
        )*
    };
}
```

- [ ] **Step 5: Add the macro invocation with all 45 entries**

Inside the existing `impl MultiChild { ... }` block, immediately AFTER the `pub fn layout_ref(&self) -> &Layout` method (around line 85, before the closing `}` of the impl block), insert the macro invocation:

```rust
    impl_layout_passthrough! {
        // Box model
        padding(value: f32),
        padding_each(left: f32, right: f32, top: f32, bottom: f32),
        margin(value: f32),
        margin_each(left: f32, right: f32, top: f32, bottom: f32),
        width(value: f32),
        height(value: f32),
        width_percent(value: f32),
        height_percent(value: f32),
        min_width(value: f32),
        min_height(value: f32),
        max_width(value: f32),
        max_height(value: f32),

        // Flexbox
        flex_direction(value: FlexDirection),
        flex_wrap(),
        flex_wrap_mode(value: FlexWrap),
        flex_grow(value: f32),
        flex_shrink(value: f32),
        flex_basis(value: f32),
        justify(value: JustifyContent),
        align(value: AlignItems),
        align_content(value: AlignContent),
        gap(value: f32),
        gap_size(size: Size<Logical>),
        gap_each(width: f32, height: f32),

        // Grid
        columns(sizes: Vec<TrackSizing>),
        rows(sizes: Vec<TrackSizing>),
        grid_column(placement: GridPlacement),
        grid_row(placement: GridPlacement),
        grid_auto_flow(value: GridAutoFlow),
        auto_rows(sizes: Vec<TrackSizing>),
        auto_columns(sizes: Vec<TrackSizing>),

        // Positioning
        absolute(),
        relative(),
        position(value: Position),
        inset(value: f32),
        top(value: f32),
        right(value: f32),
        bottom(value: f32),
        left(value: f32),

        // Per-item alignment
        align_self(value: AlignSelf),

        // Display
        display(value: Display),

        // Sizing
        aspect_ratio(value: f32),

        // Overflow
        overflow(value: Overflow),
        overflow_x(value: Overflow),
        overflow_y(value: Overflow),
    }
```

Note the three zero-arg entries: `flex_wrap()`, `absolute()`, `relative()`. The macro pattern `$($arg:ident: $ty:ty),*` matches zero args, so `flex_wrap()` generates `pub fn flex_wrap(mut self) -> Self { self.layout = self.layout.flex_wrap(); self }`.

- [ ] **Step 6: Add missing layout type re-exports to `vexo/src/lib.rs`**

The methods accept types like `FlexWrap`, `AlignContent`, `GridPlacement`, `Position`, `TrackSizing`, `Size<Logical>` — these must be nameable from outside the crate as `vexo::FlexWrap`, etc.

Open `vexo/src/lib.rs`. Find lines 58-61:

```rust
pub use layout::{
    AlignItems, AlignSelf, Display, EdgeInsets, FlexDirection, GridAutoFlow, JustifyContent, Layout,
    Overflow, DEFAULT_LINE_HEIGHT_MULTIPLIER, LAYOUT_WIDTH_TOLERANCE,
};
```

Replace with the expanded set (add: `AlignContent`, `FlexWrap`, `GridPlacement`, `Position`, `TrackSizing`):

```rust
pub use layout::{
    AlignContent, AlignItems, AlignSelf, Display, EdgeInsets, FlexDirection, FlexWrap,
    GridAutoFlow, GridPlacement, JustifyContent, Layout, Overflow, Position, TrackSizing,
    DEFAULT_LINE_HEIGHT_MULTIPLIER, LAYOUT_WIDTH_TOLERANCE,
};
```

Then find the existing core re-exports (lines 6-8):

```rust
pub use core::AffineTransform;
pub use core::Color;
pub use core::{KeyboardAnimation, KeyboardAnimationSource, KeyboardInsetSource};
```

Add `Logical` and `Size` (needed for `gap_size(size: Size<Logical>)`). Insert a new line after line 8:

```rust
pub use core::{Logical, Size};
```

(The full block becomes:)

```rust
pub use core::AffineTransform;
pub use core::Color;
pub use core::{KeyboardAnimation, KeyboardAnimationSource, KeyboardInsetSource};
pub use core::{Logical, Size};
```

- [ ] **Step 7: Run unit tests to verify they pass**

Run: `cargo test -p vexo --lib widgets::multi_child::tests 2>&1 | tail -20`
Expected: All 9 new tests PASS, plus the existing 8 tests still pass (17 total in the mod). If any fail, check that the macro generated the method with the exact name/signature the test expects.

- [ ] **Step 8: Verify the whole `vexo` crate builds and tests pass**

Run: `cargo build -p vexo && cargo test -p vexo --lib 2>&1 | tail -10`
Expected: Build succeeds; all lib tests pass. No warnings about unused imports (if there are unused-import warnings, the imported type isn't referenced — but all 11 layout types in the import are used by the macro invocation, so this shouldn't happen).

- [ ] **Step 9: Commit**

```bash
git add vexo/src/widgets/multi_child.rs vexo/src/lib.rs
git commit -m "feat(multi_child): add 45 fluent layout passthrough methods

Generate via impl_layout_passthrough! macro; mirrors Layout's instance
builders so column!/row! results can be chained (.gap().padding()) and
MultiChild::empty(Layout::column()).flex_shrink(0.0) works. Methods
modify self.layout in place, preserving other fields. Re-export missing
layout types (FlexWrap, AlignContent, GridPlacement, Position, TrackSizing,
Size, Logical) at vexo crate root so external call sites can name them."
```

---

### Task 2: Add macro+fluent integration test

**Files:**
- Modify: `vexo/tests/builder_macros.rs` (append 1 test)

**Interfaces:**
- Consumes: `column!` macro (from Task 1's `vexo::column` re-export), `MultiChild::gap` / `MultiChild::padding` fluent methods (from Task 1).
- Produces: One integration test proving `column! { ... }.gap().padding()` composes end-to-end.

- [ ] **Step 1: Write the failing integration test**

Open `vexo/tests/builder_macros.rs`. The existing imports (lines 6-7) are:

```rust
use vexo::widgets::{MultiChild, Widget};
use vexo::{column, row};
```

Add `FlexDirection` to the `vexo::` import (now re-exported at crate root after Task 1):

```rust
use vexo::widgets::{MultiChild, Widget};
use vexo::{column, row, FlexDirection};
```

Append this test at the end of the file (after the `match_with_guard` test, after line 175):

```rust
#[test]
fn column_macro_with_fluent_layout_chain() {
    let mc: MultiChild = column! {
        vexo::Text::new("a"),
        vexo::Text::new("b"),
    }
    .gap(8.0)
    .padding(12.0);

    assert_eq!(mc.children().len(), 2);
    assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Column));
    assert_eq!(mc.layout_ref().gap, Some(vexo::Size::new(8.0, 8.0)));
    assert!(mc.layout_ref().padding.is_some());
    let p = mc.layout_ref().padding.unwrap();
    assert_eq!(p.top, 12.0);
    assert_eq!(p.bottom, 12.0);
}
```

- [ ] **Step 2: Run the test to verify it passes**

Run: `cargo test -p vexo --test builder_macros column_macro_with_fluent_layout_chain 2>&1 | tail -10`
Expected: PASS. (Task 1 already added the methods and re-exports, so this test should pass on first run — no separate "fail" step needed. If it fails, Task 1's methods or re-exports are incomplete; revisit Task 1.)

- [ ] **Step 3: Run the full `builder_macros` test suite to verify no regressions**

Run: `cargo test -p vexo --test builder_macros 2>&1 | tail -10`
Expected: All existing tests (column_produces_multichild_with_two_children, row_produces_multichild_with_two_children, etc.) plus the new test pass. No regressions — the macros themselves are unchanged.

- [ ] **Step 4: Commit**

```bash
git add vexo/tests/builder_macros.rs
git commit -m "test(builder_macros): add column!+fluent layout chain integration test

Proves column! { A, B }.gap(8.0).padding(12.0) composes end-to-end:
2 children, column direction preserved, gap and padding applied."
```

---

### Task 3: Final workspace verification and optional migration

**Files:**
- Verify only (no required changes). Optional: `shared_app/src/chats/chat_screen.rs:115`, `shared_app/src/chats/conversation_list.rs:167`.

- [ ] **Step 1: Build and test the entire workspace**

Run: `cargo build && cargo test 2>&1 | tail -30`
Expected: All crates (`vexo`, `vexo_macros`, `shared_app`, `desktop_demo`) build and all tests pass. No regressions in any existing test.

- [ ] **Step 2: Verify no unused-import warnings**

Run: `cargo build -p vexo 2>&1 | grep -i 'warning.*unused' || echo "no unused warnings"`
Expected: `no unused warnings`. If any appear, remove the unused import.

- [ ] **Step 3: Spot-check one real call site still compiles (regression guard)**

Run: `cargo build -p shared_app 2>&1 | tail -5`
Expected: Build succeeds. Existing `MultiChild::new(children![...], Layout::column().gap(8.0).padding(12.0))` call sites (e.g. `chat_screen.rs:115`) compile unchanged — the feature is purely additive.

- [ ] **Step 4 (OPTIONAL POLISH — skip if not wanted): Migrate 2 representative call sites**

These two call sites are the pain points called out in the spec. Migrating them proves the ergonomics in real code. Skip if you'd rather keep this PR minimal.

**Site 1:** `shared_app/src/chats/chat_screen.rs:115`

Read the current code around line 115:
```rust
let mut list = MultiChild::empty(Layout::column().gap(8.0).padding(12.0));
```

Replace with:
```rust
let mut list = MultiChild::empty(Layout::column()).gap(8.0).padding(12.0);
```

(Or, if the surrounding code uses `column!` style elsewhere, the more idiomatic form:
```rust
let mut list = crate::column! {}.gap(8.0).padding(12.0);
```
— but only if `column!` is already imported. Check imports at the top of the file first.)

**Site 2:** `shared_app/src/chats/conversation_list.rs:167`

Read the current code around line 167:
```rust
let right_col = MultiChild::new(children![time_text], Layout::column().flex_shrink(0.0));
```

Replace with:
```rust
let right_col = crate::column! { time_text }.flex_shrink(0.0);
```

(Assuming `column!` is reachable as `crate::column!` — it's re-exported from `vexo` which `shared_app` depends on. If `column` is already imported at the top of the file, use the unqualified form `column! { time_text }`.)

After both edits:
Run: `cargo build -p shared_app && cargo test -p shared_app 2>&1 | tail -10`
Expected: Build succeeds; tests pass.

- [ ] **Step 5: Final commit (only if Step 4 was done)**

```bash
git add shared_app/src/chats/chat_screen.rs shared_app/src/chats/conversation_list.rs
git commit -m "refactor(shared_app): use column!+fluent at 2 call sites as living examples

Migrate chat_screen.rs:115 and conversation_list.rs:167 to use the new
fluent API. Pure refactor — no behavior change."
```

- [ ] **Step 6: Report completion**

The feature is complete when:
- `cargo build` and `cargo test` pass across all crates (Step 1).
- No unused-import warnings (Step 2).
- `column! { A, B }.gap(8.0).padding(12.0)` compiles and produces the expected `MultiChild` (Task 2's integration test).
- All 45 `Layout` instance builders are mirrored onto `MultiChild` (Task 1's unit tests verify representatives of each group).
- Existing `MultiChild::new`, `with_layout`, `with_key`, `push`, `empty` APIs unchanged (Step 3 regression guard).

No manual GUI smoke test is required — the feature is pure layout-property plumbing, no rendering behavior change. (If the optional migration in Step 4 was done, the user may optionally run `cargo run -p desktop_demo` to visually confirm the migrated screens render identically, per CLAUDE.md's rule that the assistant never runs the GUI.)
