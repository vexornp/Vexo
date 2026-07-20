# Remove Widget Trait Layout Methods — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the 9 trait-default layout methods from `Widget` so the only way to introduce a `WithLayout` node is explicit `WithLayout::new(child, layout)` construction — symmetric with the prior DecoratedBox split.

**Architecture:** Delete the 9 trait default methods (`with_layout`, `padding`, `margin`, `width`, `height`, `flex_grow`, `flex_fill`, `align_self`, `absolute`) from `vexo/src/widgets/mod.rs`. Add `Layout::flex_fill()` constructor in `vexo/src/layout/style.rs` to preserve the `flex_fill` preset as a named concept. Migrate ~18 trait-default call sites across `vexo`, `shared_app`, and `vexo_uikit` to explicit `WithLayout::new(...)` construction. Inherent-method call sites (on `Flex`/`Text`/`Image`/`GestureDetector`/etc.) are unchanged — they set the widget's own `layout` field, no wrapping, no footgun.

**Tech Stack:** Rust, cargo workspace (`vexo`, `shared_app`, `vexo_uikit`, `desktop_demo` crates).

## Global Constraints

- Workspace dependency versions defined in root `Cargo.toml` — no version changes in this plan.
- No deprecation period — methods removed outright. Internal codebase, no external consumers.
- No edits to historical design specs in `docs/superpowers/specs/` that reference these methods as trait APIs.
- Per `CLAUDE.md`: run `cargo build` after Rust edits, `cargo test` after implementing. Never run `cargo run -p desktop_demo` — ask the user.
- The `WidgetExt` trait name referenced in some older docs does **not exist** in the codebase — the layout methods are default methods on `Widget` in `vexo/src/widgets/mod.rs`. Nothing to remove from a `WidgetExt` trait.
- `GestureDetector` has an **inherent** `with_layout` method at `vexo/src/widgets/gesture_detector.rs:103` that sets `self.layout` directly (no wrapping). Call sites that resolve to it are already safe and must NOT be migrated.

---

### Task 1: Add `Layout::flex_fill()` constructor

**Files:**
- Modify: `vexo/src/layout/style.rs:631-649` (the `impl Layout` block containing `fill()`, `fixed()`, `absolute_at()`)
- Test: `vexo/src/layout/style.rs` (extend the `#[cfg(test)] mod tests` block starting at line 953)

**Interfaces:**
- Produces: `Layout::flex_fill() -> Self` — a named preset constructor returning `Layout::default().flex_grow(1.0).flex_basis(0.0).min_height(0.0)`. Later tasks (Task 3+) rely on this for migrating `.flex_fill()` call sites.

**Why this task is first:** Adding the constructor before removing the trait method means the new path exists when call sites migrate. This task compiles independently — it only adds a new function, doesn't touch any existing code.

- [ ] **Step 1: Write the failing test**

Add to the test module in `vexo/src/layout/style.rs` (after the existing tests, before the closing `}` of `mod tests` at line 1381):

```rust
    #[test]
    fn test_layout_flex_fill_constructor() {
        let layout = Layout::flex_fill();
        assert_eq!(layout.flex_grow, Some(1.0));
        assert_eq!(layout.flex_basis, Some(Dimension::Length(0.0)));
        assert_eq!(layout.min_height, Some(Dimension::Length(0.0)));
        // All other fields stay at default (None)
        assert!(layout.padding.is_none());
        assert!(layout.margin.is_none());
        assert!(layout.width.is_none());
        assert!(layout.height.is_none());
        assert!(layout.flex_shrink.is_none());
        assert!(layout.align_self.is_none());
        assert!(layout.position.is_none());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vexo --lib layout::style::tests::test_layout_flex_fill_constructor`
Expected: compile error — `Layout::flex_fill` does not exist.

- [ ] **Step 3: Add the constructor**

In `vexo/src/layout/style.rs`, inside the `impl Layout` block that contains `fill()` (around line 631–649), add after the `fill()` method:

```rust
    /// CSS `flex: 1 1 0` + `min-height: 0` — fill remaining space without
    /// propagating min-content upward.
    ///
    /// Convenience for `Layout::default().flex_grow(1.0).flex_basis(0.0).min_height(0.0)`.
    /// Use this for scrollable content areas that should fill the remaining
    /// space in a flex column without pushing siblings off screen.
    pub fn flex_fill() -> Self {
        Self::default().flex_grow(1.0).flex_basis(0.0).min_height(0.0)
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vexo --lib layout::style::tests::test_layout_flex_fill_constructor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/layout/style.rs
git commit -m "feat(layout): add Layout::flex_fill() constructor

CSS flex: 1 1 0 + min-height: 0 preset as a named constructor on Layout.
Preserves the flex_fill semantic as a named concept on Layout (where it
belongs) ahead of removing the .flex_fill() trait method from Widget."
```

---

### Task 2: Remove the 9 trait-default layout methods from `Widget`

**Files:**
- Modify: `vexo/src/widgets/mod.rs:178-264` (delete the 9 trait default layout methods + section comment)
- Modify: `vexo/src/widgets/mod.rs:51` (drop `use crate::layout::Layout` if now unused)
- Modify: `vexo/src/widgets/with_layout.rs:233-247` (update doc example)
- Modify: `vexo/src/widgets/with_layout.rs:446-449` (delete `test_with_layout_method_on_widget` test)

**Interfaces:**
- Consumes: `Layout::flex_fill()` from Task 1 (not directly in this task, but the constructor must exist before downstream tasks migrate `.flex_fill()` call sites).
- Produces: `Widget` trait without the 9 layout default methods. Downstream tasks migrate call sites that used them.

**Why this task is second:** The framework change breaks compilation of every trait-default call site. Tasks 3–6 migrate those call sites. This task + the migration tasks must land together in a single PR, but they're split for review granularity. The compile errors from this task are the migration map for Tasks 3–6.

**Important:** This task will leave the workspace non-compiling until Tasks 3–6 complete. Do NOT commit until the workspace builds. The commit at the end of Task 6 includes this task's changes.

- [ ] **Step 1: Delete the 9 trait default layout methods**

In `vexo/src/widgets/mod.rs`, delete lines 178–264. This removes:
- `fn with_layout(self, layout: Layout) -> WithLayout` (lines 178–186)
- The `// Layout modifiers (fallback: wrap in WithLayout)` comment (line 196)
- `fn padding(self, value: f32) -> Box<dyn Widget>` (lines 198–203)
- `fn margin(self, value: f32) -> Box<dyn Widget>` (lines 205–210)
- `fn width(self, value: f32) -> Box<dyn Widget>` (lines 212–217)
- `fn height(self, value: f32) -> Box<dyn Widget>` (lines 219–224)
- `fn flex_grow(self, value: f32) -> Box<dyn Widget>` (lines 226–231)
- `fn flex_fill(self) -> Box<dyn Widget>` (lines 233–250, including the doc comment at 233–238)
- `fn align_self(self, value: crate::layout::AlignSelf) -> Box<dyn Widget>` (lines 252–257)
- `fn absolute(self) -> Box<dyn Widget>` (lines 259–264)

What remains: `.boxed()` (lines 188–194), `.clone_boxed()` (line 176), and the behavioral/transform defaults starting at line 266 (`// Behavioral modifiers (always wrap)`).

After deletion, the trait should flow directly from `.clone_boxed()` to `.boxed()` to `// Behavioral modifiers (always wrap)`.

- [ ] **Step 2: Drop the now-unused `Layout` import if applicable**

Check whether `crate::layout::Layout` (imported at `vexo/src/widgets/mod.rs:51`) is still referenced anywhere in this file after the deletion. Run:

```bash
rg -n "Layout" vexo/src/widgets/mod.rs
```

If `Layout` no longer appears in the file body (only the import line), delete line 51:

```rust
use crate::layout::Layout;
```

If `Layout` still appears (e.g., in other comments or test code), leave the import.

- [ ] **Step 3: Update the doc example in `WithLayout`**

In `vexo/src/widgets/with_layout.rs`, replace the doc example at lines 233–247. The current example shows the trait method form. Replace it with the explicit constructor form.

Replace this block (lines 233–248):

```rust
/// # Example
///
/// ```ignore
/// // Add padding and center a text widget
/// Text::new("Hello").with_layout(
///     Layout::default()
///         .padding(16.0)
///         .align_self(AlignSelf::Center)
/// )
///
/// // Fixed-size container
/// WithLayout::new(
///     Text::new("Fixed"),
///     Layout::fixed(200.0, 100.0),
/// )
/// ```
```

With:

```rust
/// # Example
///
/// ```ignore
/// // Add padding and center a text widget
/// WithLayout::new(
///     Text::new("Hello"),
///     Layout::default()
///         .padding(16.0)
///         .align_self(AlignSelf::Center),
/// )
///
/// // Fixed-size container
/// WithLayout::new(
///     Text::new("Fixed"),
///     Layout::fixed(200.0, 100.0),
/// )
/// ```
```

- [ ] **Step 4: Delete the `test_with_layout_method_on_widget` test**

In `vexo/src/widgets/with_layout.rs`, delete lines 445–449:

```rust
    #[test]
    fn test_with_layout_method_on_widget() {
        let w = Text::new("Hello").with_layout(Layout::default().padding(10.0));
        assert!(w.child().is_some());
    }
```

This test exercises the trait method `.with_layout()` which no longer exists. The behavior it tests (constructing a `WithLayout` wrapping a `Text`) is already covered by `test_with_layout_creation` at line 353.

- [ ] **Step 5: Add a replacement doc-example test**

In `vexo/src/widgets/with_layout.rs`, in the `#[cfg(test)] mod tests` block, add a new test (where the deleted test was):

```rust
    #[test]
    fn test_with_layout_doc_example_compiles() {
        // Mirrors the updated doc example — verifies the explicit
        // constructor form compiles and produces a widget with the
        // expected layout.
        let w = WithLayout::new(
            Text::new("Hello"),
            Layout::default().padding(16.0).align_self(AlignSelf::Center),
        );
        assert!(w.child().is_some());
        assert!(w.layout_ref().padding.is_some());
        assert_eq!(
            w.layout_ref().align_self,
            Some(AlignSelf::Center)
        );
    }
```

- [ ] **Step 6: Do NOT commit yet**

This task leaves the workspace non-compiling (call sites in Tasks 3–6 still reference the removed methods). The commit happens at the end of Task 6. Proceed to Task 3.

---

### Task 3: Migrate `vexo` test call sites

**Files:**
- Modify: `vexo/src/e2e_test.rs:611, 612, 613, 645, 652, 659, 666` (7 `.with_layout()` sites)
- Modify: `vexo/src/integration_tests.rs:475` (1 `.width().height()` site)
- Modify: `vexo/src/focus/integration_tests.rs:804` (1 `.width().height()` site)
- Modify: `vexo/src/widgets/safe_area.rs:1414` (1 `.flex_fill()` site)

**Interfaces:**
- Consumes: `Layout::flex_fill()` from Task 1, `WithLayout::new(...)` (existing), removal of trait methods from Task 2.
- Produces: All `vexo`-crate test call sites use explicit `WithLayout::new(...)` construction. The `vexo` crate compiles after this task.

**Pattern:** Every migration is the same mechanical rewrite:
- `widget.with_layout(layout)` → `WithLayout::new(widget, layout)`
- `widget.flex_fill()` → `WithLayout::new(widget, Layout::flex_fill())`
- `widget.width(w).height(h)` → `WithLayout::new(widget, Layout::default().width(w).height(h))`
- `widget.flex_grow(g)` → `WithLayout::new(widget, Layout::default().flex_grow(g))`

- [ ] **Step 1: Migrate `e2e_test.rs` lines 611–613**

In `vexo/src/e2e_test.rs`, find the `test_with_layout_on_children` function (line 609). Replace lines 610–613:

Before:
```rust
    let widget = Flex::row()
        .push(Text::new("Left").with_layout(Layout::default().flex_grow(1.0)))
        .push(Text::new("Center").with_layout(Layout::default().width(100.0)))
        .push(Text::new("Right").with_layout(Layout::default().flex_grow(2.0)))
```

After:
```rust
    let widget = Flex::row()
        .push(WithLayout::new(Text::new("Left"), Layout::default().flex_grow(1.0)))
        .push(WithLayout::new(Text::new("Center"), Layout::default().width(100.0)))
        .push(WithLayout::new(Text::new("Right"), Layout::default().flex_grow(2.0)))
```

- [ ] **Step 2: Migrate `e2e_test.rs` lines 645, 652, 659, 666 (Grid test)**

In `vexo/src/e2e_test.rs`, find the `test_grid_widget` function (line 642). Replace each `Text::new("X").with_layout(...)` with `WithLayout::new(Text::new("X"), ...)`.

Before (lines 644–671):
```rust
        .push(
            Text::new("A").with_layout(
                Layout::default()
                    .grid_column(GridPlacement::start(1))
                    .grid_row(GridPlacement::start(1)),
            ),
        )
        .push(
            Text::new("B").with_layout(
                Layout::default()
                    .grid_column(GridPlacement::start(2))
                    .grid_row(GridPlacement::start(1)),
            ),
        )
        .push(
            Text::new("C").with_layout(
                Layout::default()
                    .grid_column(GridPlacement::start(1))
                    .grid_row(GridPlacement::start(2)),
            ),
        )
        .push(
            Text::new("D").with_layout(
                Layout::default()
                    .grid_column(GridPlacement::start(2))
                    .grid_row(GridPlacement::start(2)),
            ),
        )
```

After:
```rust
        .push(WithLayout::new(
            Text::new("A"),
            Layout::default()
                .grid_column(GridPlacement::start(1))
                .grid_row(GridPlacement::start(1)),
        ))
        .push(WithLayout::new(
            Text::new("B"),
            Layout::default()
                .grid_column(GridPlacement::start(2))
                .grid_row(GridPlacement::start(1)),
        ))
        .push(WithLayout::new(
            Text::new("C"),
            Layout::default()
                .grid_column(GridPlacement::start(1))
                .grid_row(GridPlacement::start(2)),
        ))
        .push(WithLayout::new(
            Text::new("D"),
            Layout::default()
                .grid_column(GridPlacement::start(2))
                .grid_row(GridPlacement::start(2)),
        ))
```

- [ ] **Step 3: Migrate `integration_tests.rs` line 475**

In `vexo/src/integration_tests.rs`, find line 475.

Before:
```rust
        let scroll_view = ScrollView::new(column.boxed()).width(200.0).height(300.0);
```

After:
```rust
        let scroll_view = WithLayout::new(
            ScrollView::new(column.boxed()),
            Layout::default().width(200.0).height(300.0),
        );
```

- [ ] **Step 4: Migrate `focus/integration_tests.rs` line 804**

In `vexo/src/focus/integration_tests.rs`, find line 804.

Before:
```rust
        let focus_widget = Focus::new(ScrollView::new(column).width(200.0).height(100.0))
```

After:
```rust
        let focus_widget = Focus::new(WithLayout::new(
            ScrollView::new(column),
            Layout::default().width(200.0).height(100.0),
        ))
```

Note: verify the surrounding syntax (the line may continue with `.child(...)` or similar). Only replace the `ScrollView::new(column).width(200.0).height(100.0)` portion with `WithLayout::new(ScrollView::new(column), Layout::default().width(200.0).height(100.0))`.

- [ ] **Step 5: Migrate `widgets/safe_area.rs` line 1414**

In `vexo/src/widgets/safe_area.rs`, find line 1414.

Before:
```rust
        let tree = SafeAreaClaim::bottom(SafeArea::new(Text::new("Hi")).flex_fill());
```

After:
```rust
        let tree = SafeAreaClaim::bottom(WithLayout::new(
            SafeArea::new(Text::new("Hi")),
            Layout::flex_fill(),
        ));
```

- [ ] **Step 6: Verify imports**

Check that `WithLayout` and `Layout` are imported in each modified file. Run:

```bash
rg -n "use.*WithLayout|use.*Layout" vexo/src/e2e_test.rs vexo/src/integration_tests.rs vexo/src/focus/integration_tests.rs vexo/src/widgets/safe_area.rs
```

If `WithLayout` is not imported in a file where it's now used, add `use crate::WithLayout;` (or `use vexo::WithLayout;` depending on existing import style) to the imports. Check the existing `use` statements for the pattern — most test files use `use crate::*;` or `use vexo::*;` which already re-export `WithLayout` via `vexo/src/lib.rs`.

- [ ] **Step 7: Verify `vexo` crate compiles**

Run: `cargo build -p vexo`
Expected: PASS (compiles with no errors). If there are errors, they indicate a missed migration site — fix it before proceeding.

- [ ] **Step 8: Run `vexo` tests**

Run: `cargo test -p vexo`
Expected: all tests PASS, including:
- `layout::style::tests::test_layout_flex_fill_constructor` (from Task 1)
- `widgets::with_layout::tests::test_with_layout_doc_example_compiles` (from Task 2)
- `e2e_test::test_with_layout_on_children`
- `e2e_test::test_grid_widget`
- All integration tests that exercise `ScrollView.width().height()` (the sizing-bug regression guards)

- [ ] **Step 9: Do NOT commit yet**

The `shared_app` and `vexo_uikit` crates still reference removed methods. The commit happens at the end of Task 6. Proceed to Task 4.

---

### Task 4: Migrate `shared_app` call sites

**Files:**
- Modify: `shared_app/src/chats/conversation_list.rs:26` (1 `.flex_fill()` site)
- Modify: `shared_app/src/chats/chat_screen.rs:114` (1 `.flex_fill()` site)
- Modify: `shared_app/src/chats/chat_screen.rs:176` (1 `.flex_grow()` site)
- Modify: `shared_app/src/contacts/contacts_screen.rs:13` (1 `.flex_fill()` site)

**Interfaces:**
- Consumes: `Layout::flex_fill()` from Task 1, `WithLayout::new(...)` (existing, re-exported via `vexo::WithLayout`).
- Produces: All `shared_app` call sites use explicit `WithLayout::new(...)`. The `shared_app` crate compiles after this task.

**Important:** `shared_app` references `vexo` types via `vexo::WithLayout`, `vexo::Layout`, etc. (check existing imports for the exact pattern). Verify the import style before editing.

- [ ] **Step 1: Migrate `conversation_list.rs` line 26**

In `shared_app/src/chats/conversation_list.rs`, find line 26.

Before:
```rust
    ScrollView::new(list.boxed()).flex_fill().boxed()
```

After:
```rust
    WithLayout::new(ScrollView::new(list.boxed()), Layout::flex_fill()).boxed()
```

- [ ] **Step 2: Migrate `chat_screen.rs` line 114**

In `shared_app/src/chats/chat_screen.rs`, find line 114 (inside the `Column::new().push(ScrollView::new(...).flex_fill(), ...)` chain).

Before:
```rust
                 ScrollView::new(list.boxed())
                     .controller(self.scroll_controller.clone())
                     .flex_fill(),
```

After:
```rust
                 WithLayout::new(
                     ScrollView::new(list.boxed())
                         .controller(self.scroll_controller.clone()),
                     Layout::flex_fill(),
                 ),
```

- [ ] **Step 3: Migrate `chat_screen.rs` line 176**

In `shared_app/src/chats/chat_screen.rs`, find line 176.

Before:
```rust
        .push(TextEdit::new(controller).flex_grow(1.0))
```

After:
```rust
        .push(WithLayout::new(
            TextEdit::new(controller),
            Layout::default().flex_grow(1.0),
        ))
```

- [ ] **Step 4: Migrate `contacts_screen.rs` line 13**

In `shared_app/src/contacts/contacts_screen.rs`, find line 13.

Before:
```rust
    ScrollView::new(list.boxed()).flex_fill().boxed()
```

After:
```rust
    WithLayout::new(ScrollView::new(list.boxed()), Layout::flex_fill()).boxed()
```

- [ ] **Step 5: Verify imports**

Check that `WithLayout` and `Layout` are imported in each modified file. Run:

```bash
rg -n "^use" shared_app/src/chats/conversation_list.rs shared_app/src/chats/chat_screen.rs shared_app/src/contacts/contacts_screen.rs
```

`shared_app` typically uses `vexo::` prefixed paths. If `WithLayout` is not in scope, add `use vexo::WithLayout;` to the imports (or use the fully-qualified `vexo::WithLayout::new(...)` form at each call site — match the existing style in the file).

- [ ] **Step 6: Verify `shared_app` crate compiles**

Run: `cargo build -p shared_app`
Expected: PASS. If errors, fix missed migration sites.

- [ ] **Step 7: Run `shared_app` tests**

Run: `cargo test -p shared_app`
Expected: all tests PASS, including chat-screen and contacts-screen integration tests that exercise the migrated `ScrollView.flex_fill()` paths.

- [ ] **Step 8: Do NOT commit yet**

The `vexo_uikit` crate still references removed methods. The commit happens at the end of Task 6. Proceed to Task 5.

---

### Task 5: Migrate `vexo_uikit` call sites

**Files:**
- Modify: `vexo_uikit/src/navigation.rs:748` (1 `.flex_fill()` site)
- Modify: `vexo_uikit/src/tab_bar.rs:197` (1 `.with_layout()` site)
- Modify: `vexo_uikit/src/tab_bar.rs:224` (1 `.flex_fill()` site)

**Interfaces:**
- Consumes: `Layout::flex_fill()` from Task 1, `WithLayout::new(...)` (existing).
- Produces: All `vexo_uikit` call sites use explicit `WithLayout::new(...)`. The `vexo_uikit` crate compiles after this task.

**Important — DO NOT migrate these sites (they use inherent methods, already safe):**
- `vexo_uikit/src/tab_bar.rs:175` — `GestureDetector::new(content).on_press(...).with_layout(...)`. `GestureDetector` has an inherent `with_layout` at `vexo/src/widgets/gesture_detector.rs:103` that sets `self.layout` directly. `on_press` returns `Self`, so the chain resolves to the inherent method. **Leave this line unchanged.**

- [ ] **Step 1: Migrate `navigation.rs` line 748**

In `vexo_uikit/src/navigation.rs`, find line 748.

Before:
```rust
        let content = SafeArea::new(clipped).top(false).flex_fill();
```

After:
```rust
        let content = WithLayout::new(SafeArea::new(clipped).top(false), Layout::flex_fill());
```

Note: the comment block at lines 750–754 explains why `flex_fill()` is used. Leave that comment in place — it still applies (the `WithLayout` wrapper with `Layout::flex_fill()` has the same effect).

- [ ] **Step 2: Migrate `tab_bar.rs` line 197**

In `vexo_uikit/src/tab_bar.rs`, find line 197.

Before:
```rust
        let bar = bar.with_layout(Layout::default().flex_grow(0.0).flex_shrink(0.0));
```

After:
```rust
        let bar = WithLayout::new(bar, Layout::default().flex_grow(0.0).flex_shrink(0.0));
```

Here `bar: Box<dyn Widget>` (from line 196: `let bar = SafeArea::new(bar.boxed()).top(false).boxed();`). `WithLayout::new` accepts `impl Widget + 'static`, and `Box<dyn Widget>` implements `Widget` via the delegation impl in `vexo/src/widgets/mod.rs:346`. This is a mechanical rewrite.

- [ ] **Step 3: Migrate `tab_bar.rs` line 224**

In `vexo_uikit/src/tab_bar.rs`, find line 224.

Before:
```rust
            .push(SafeAreaClaim::bottom(stack).flex_fill())
```

After:
```rust
            .push(WithLayout::new(SafeAreaClaim::bottom(stack), Layout::flex_fill()))
```

- [ ] **Step 4: Verify imports**

Check that `WithLayout` and `Layout` are imported in each modified file. Run:

```bash
rg -n "^use" vexo_uikit/src/navigation.rs vexo_uikit/src/tab_bar.rs
```

If `WithLayout` is not in scope, add it to the imports. Match the existing import style (likely `use vexo::WithLayout;` or `use vexo::{..., WithLayout, ...};`).

- [ ] **Step 5: Verify `vexo_uikit` crate compiles**

Run: `cargo build -p vexo_uikit`
Expected: PASS. If errors, fix missed migration sites.

- [ ] **Step 6: Run `vexo_uikit` tests**

Run: `cargo test -p vexo_uikit`
Expected: all tests PASS, including navigation and tab-bar tests that exercise the migrated `SafeArea.flex_fill()` / `Box<dyn Widget>.with_layout()` / `SafeAreaClaim.flex_fill()` paths.

- [ ] **Step 7: Do NOT commit yet**

Proceed to Task 6 for the full workspace verification and commit.

---

### Task 6: Full workspace verification and commit

**Files:**
- No new file edits — this task verifies the workspace and commits all changes from Tasks 1–5.

**Interfaces:**
- Consumes: all changes from Tasks 1–5.
- Produces: a single commit with the framework change + all call-site migrations. The workspace compiles and all tests pass.

- [ ] **Step 1: Verify the full workspace compiles**

Run: `cargo build --workspace`
Expected: PASS with no errors. Every missed migration site would surface here as a compile error.

- [ ] **Step 2: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: all tests PASS, including:
- `vexo` layout/with_layout unit tests
- `vexo` e2e tests (sizing-bug regression guards)
- `vexo` integration tests (ScrollView width/height propagation)
- `shared_app` chat/contacts integration tests
- `vexo_uikit` navigation/tab-bar tests

If any test fails, investigate — the migration should be purely syntactic with no behavior change. A failure indicates either a missed migration (compile error, already caught in Step 1) or an actual behavior difference (shouldn't happen — `WithLayout::new(widget, layout)` produces the same widget tree as `widget.with_layout(layout)`).

- [ ] **Step 3: Verify no stray references to removed methods**

Run a search for any remaining trait-default call sites that might have been missed:

```bash
rg -n "\.(with_layout|flex_fill|flex_grow|align_self|absolute)\(" --type rust -g '!vexo/src/layout/style.rs' -g '!vexo/src/macros.rs' -g '!vexo/src/widgets/gesture_detector.rs' -g '!docs/'
```

Manually inspect each result. Valid remaining references (do NOT migrate):
- `.flex_grow()`, `.align_self()`, `.absolute()`, `.padding()`, `.width()`, `.height()`, `.margin()` on widgets that own a `layout: Layout` field (`Flex`/`Column`/`Row`/`Stack`/`Grid`/`IndexedStack`/`Text`/`Image`/`TextEditContent`/`DecoratedContainer`/`WithLayout` itself) — these are inherent methods, safe.
- `.with_layout()` on `GestureDetector` (inherent method at `gesture_detector.rs:103`) — safe.
- `.with_layout()` on render objects (`TextRenderObject`, `TextEditRenderObject`) — different method, different concern.
- `Layout::default().flex_grow(...)` etc. on `Layout` itself — these are `Layout` builder methods, safe.
- References in `docs/` — historical specs, left as-is.

Any result that doesn't fit the above categories is a missed migration — fix it.

- [ ] **Step 4: Commit all changes**

Stage all modified files and commit. This single commit includes: the framework change (Task 2), the constructor (Task 1), and all call-site migrations (Tasks 3–5).

```bash
git add vexo/src/layout/style.rs vexo/src/widgets/mod.rs vexo/src/widgets/with_layout.rs vexo/src/e2e_test.rs vexo/src/integration_tests.rs vexo/src/focus/integration_tests.rs vexo/src/widgets/safe_area.rs shared_app/src/chats/conversation_list.rs shared_app/src/chats/chat_screen.rs shared_app/src/contacts/contacts_screen.rs vexo_uikit/src/navigation.rs vexo_uikit/src/tab_bar.rs
git commit -m "refactor(widgets): remove trait-default layout methods from Widget

Remove 9 trait-default layout methods (.with_layout, .padding, .margin,
.width, .height, .flex_grow, .flex_fill, .align_self, .absolute) so the
only way to introduce a WithLayout node is explicit WithLayout::new(child,
layout) construction. Symmetric with the prior DecoratedBox split —
eliminates the latent sizing footgun where WithLayout::new injected
FlexDirection::Column + AlignItems::Stretch defaults invisibly.

Adds Layout::flex_fill() constructor to preserve the flex_fill preset
(CSS flex: 1 1 0 + min-height: 0) as a named concept on Layout.

Migrates ~18 trait-default call sites across vexo, shared_app, and
vexo_uikit to explicit WithLayout::new(...) construction. Inherent-method
call sites on Flex/Text/Image/GestureDetector/etc. (~40+) are unchanged
— they set the widget's own layout field, no wrapping, no footgun."
```

- [ ] **Step 5: Final verification**

Run: `cargo test --workspace`
Expected: all tests PASS. This confirms the commit is clean.

---

## Self-Review Checklist

After completing all tasks, verify:

1. **`cargo build --workspace` passes** — no missed migration sites.
2. **`cargo test --workspace` passes** — no behavior regression.
3. **No `.flex_fill()` / `.with_layout()` / `.width()` / etc. trait-default calls remain** — search per Task 6 Step 3.
4. **`GestureDetector.with_layout(...)` call sites are untouched** — they use the inherent method.
5. **`Flex`/`Text`/`Image`/etc. inherent layout method calls are untouched** — they're already safe.
6. **`Layout::flex_fill()` exists and is tested** — Task 1.
7. **The `Widget` trait has no layout default methods** — only `.boxed()`, `.clone_boxed()`, and behavioral/transform defaults remain.
