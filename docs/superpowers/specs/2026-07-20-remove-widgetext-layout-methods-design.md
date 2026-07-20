# Remove Widget Trait Layout Methods — Design

**Date:** 2026-07-20
**Status:** Superseded by the "Pure WithLayout-Only" model (see
`docs/superpowers/plans/2026-07-20-pure-withlayout-model.md`)
**Scope:** `vexo`, `shared_app`, `vexo_uikit` crates

> **Note (2026-07-21):** This design has been superseded. The migration went
> further than removing `Widget` trait layout methods — it eliminated
> `Flex`/`Column`/`Row` types, the `column!`/`row!`/`grid!` macros, and the
> `layout_builder_methods!`/`modifier_methods!`/`modifier_fields!` macros.
> The framework now uses only:
> - `WithLayout::new(child, layout)` for single-child layout
> - `MultiChild::new(children, layout)` for multi-child layout
> - `DecoratedBox::with_style(child, style)` for decoration
> - `Layout::column()` / `Layout::row()` / `Layout::stack()` / `Layout::grid()`
>   constructors for the common layouts
> - `children![...]` macro to build `Vec<Box<dyn Widget>>`
>
> See the plan linked above for the full migration history.

## Motivation

The `Widget` trait in `vexo/src/widgets/mod.rs:178-264` defines nine default
layout methods that wrap the receiver in `WithLayout::new(self,
Layout::default().<prop>(value))`:

- `.with_layout(Layout)`
- `.padding(f32)`, `.margin(f32)`, `.width(f32)`, `.height(f32)`
- `.flex_grow(f32)`, `.flex_fill()`, `.align_self(AlignSelf)`, `.absolute()`

`WithLayout::new` (`vexo/src/widgets/with_layout.rs:264-275`) injects
`FlexDirection::Column` + `AlignItems::Stretch` defaults on top of whatever
`Layout` the caller asked for. This is a layout opinion the caller did not
ask for and cannot see at the call site — a latent sizing footgun.

The DecoratedBox split (2026-07-19) eliminated the analogous footgun for
decoration methods (`.background()`, `.border()`, etc.) by routing them
through the pass-through `DecoratedBox` widget. Layout methods are the
last remaining trait-default path that silently injects layout opinions.

This spec finishes the symmetry: **the only way to introduce a
`WithLayout` node is to construct `WithLayout::new(child, layout)`
explicitly.** Symmetric with `DecoratedBox::new(child, ...)`.

## Goals

- Remove all nine trait-default layout methods from `Widget`.
- Add `Layout::flex_fill()` constructor so the `flex_fill` preset (CSS
  `flex: 1 1 0` + `min-height: 0`) survives as a named concept on `Layout`,
  where it belongs.
- Migrate every trait-default call site (~21 sites) to explicit
  `WithLayout::new(...)` construction.
- Preserve all existing behavior — the migration is purely syntactic for
  callers that already produced a `WithLayout` widget.

## Non-Goals

- No changes to inherent layout methods on widgets that own a `layout:
  Layout` field (`Flex`/`Column`/`Row`/`Stack`/`Grid`/`IndexedStack`/
  `WithLayout` via `layout_builder_methods!()`; `Text`/`Image`/
  `TextEditContent` via `modifier_methods!()`; hand-written on
  `DecoratedContainer`). These set the widget's own `layout` field — no
  wrapping, no footgun.
- No changes to behavioral/transform trait default methods
  (`.on_press()`, `.on_release()`, `.on_tap()`, `.cursor()`, `.on_enter()`,
  `.on_exit()`, `.translate()`, `.rotate()`, `.scale()`, `.opacity()`).
  These wrap in `GestureDetector`/`MouseRegion`/`Transform`/`Opacity`,
  none of which inject layout opinions.
- No changes to `.boxed()` or `.clone_boxed()` — not layout, not a
  footgun.
- No deprecation period. Internal codebase, no external consumers; a
  deprecation period would leave the footgun live longer.
- No edits to historical design specs in `docs/superpowers/specs/` that
  reference these methods as trait APIs — they document the design at
  the time of writing.
- No rename of `e2e_test.rs:802` test that mentions "WidgetExt" in its
  doc comment — cosmetic, leave for a separate cleanup.

## Architecture

### What gets removed

`vexo/src/widgets/mod.rs` lines 178–264: the nine trait-default layout
methods plus the `// Layout modifiers (fallback: wrap in WithLayout)`
section comment at line 196.

```rust
// All nine removed:
fn with_layout(self, layout: Layout) -> WithLayout where Self: Sized + 'static { ... }
fn padding(self, value: f32) -> Box<dyn Widget> where Self: Sized + 'static { ... }
fn margin(self, value: f32) -> Box<dyn Widget> where Self: Sized + 'static { ... }
fn width(self, value: f32) -> Box<dyn Widget> where Self: Sized + 'static { ... }
fn height(self, value: f32) -> Box<dyn Widget> where Self: Sized + 'static { ... }
fn flex_grow(self, value: f32) -> Box<dyn Widget> where Self: Sized + 'static { ... }
fn flex_fill(self) -> Box<dyn Widget> where Self: Sized + 'static { ... }
fn align_self(self, value: AlignSelf) -> Box<dyn Widget> where Self: Sized + 'static { ... }
fn absolute(self) -> Box<dyn Widget> where Self: Sized + 'static { ... }
```

### What stays

- `.boxed()`, `.clone_boxed()` — not layout.
- Behavioral/transform trait defaults (`.on_press()`, `.cursor()`,
  `.translate()`, `.opacity()`, etc.) — wrap in widgets that don't inject
  layout opinions.
- Inherent layout methods on widgets with a `layout: Layout` field
  (via `layout_builder_methods!()` and `modifier_methods!()`). These are
  the safe, non-wrapping path.

### `Layout::flex_fill()` constructor

Add to `vexo/src/layout/style.rs`, next to `Layout::default()` /
`Layout::fixed()` / `Layout::absolute()`:

```rust
impl Layout {
    /// CSS `flex: 1 1 0` + `min-height: 0` — fill remaining space without
    /// propagating min-content upward.
    ///
    /// Convenience for `Layout::default().flex_grow(1.0).flex_basis(0.0).min_height(0.0)`.
    /// Use this for scrollable content areas that should fill the remaining
    /// space in a flex column without pushing siblings off screen.
    pub fn flex_fill() -> Self {
        Self::default().flex_grow(1.0).flex_basis(0.0).min_height(0.0)
    }
}
```

This is a *named preset constructor* on `Layout`, not a builder method —
it returns a fresh `Layout` rather than mutating `self`. The semantic is
preserved as a named concept where it belongs (Layout itself), rather
than living as a trait method on Widget.

### Why this is the right cut

The footgun is specifically *trait methods that wrap the receiver in
`WithLayout::new(self, Layout::default().<prop>(value))`*, because
`WithLayout::new` injects `FlexDirection::Column` + `AlignItems::Stretch`
defaults — an opinion the caller didn't ask for and can't see at the
call site. Inherent methods on widgets with a `layout` field don't wrap,
so they're safe. Behavioral/transform wrappers (`GestureDetector`,
`MouseRegion`, `Transform`, `Opacity`) don't inject layout opinions, so
they're safe. Decoration methods (`.background()` etc.) were already made
safe by the DecoratedBox split (they wrap in pass-through `DecoratedBox`).

After this change: **the only way to introduce a `WithLayout` node is to
construct `WithLayout::new(child, layout)` explicitly.** Symmetric with
`DecoratedBox::new(child, ...)`.

## Migration

Trait-default call sites (the ones requiring migration) break into three
groups. Inherent-method call sites (the common case — ~40 of ~60
`.padding()` sites, all `.flex_grow()` on `Flex`, etc.) are unchanged.

### Group A: `.flex_fill()` on widgets without inherent layout methods (6 sites)

These widgets have no `layout: Layout` field and no
`layout_builder_methods!()`: `ScrollView`, `SafeArea`, `SafeAreaClaim`,
`DecoratedBox`. Every `.flex_fill()` / `.flex_grow()` / `.padding()` etc.
on these currently goes through the trait default.

| File:line | Receiver | Migration |
|---|---|---|
| `shared_app/src/chats/conversation_list.rs:26` | `ScrollView` | `WithLayout::new(ScrollView::new(...), Layout::flex_fill()).boxed()` |
| `shared_app/src/chats/chat_screen.rs:114` | `ScrollView` | `WithLayout::new(ScrollView::new(...), Layout::flex_fill())` |
| `shared_app/src/contacts/contacts_screen.rs:13` | `ScrollView` | `WithLayout::new(ScrollView::new(...), Layout::flex_fill()).boxed()` |
| `vexo_uikit/src/navigation.rs:748` | `SafeArea` | `WithLayout::new(SafeArea::new(clipped).top(false), Layout::flex_fill())` |
| `vexo_uikit/src/tab_bar.rs:224` | `SafeAreaClaim` | `WithLayout::new(SafeAreaClaim::bottom(stack), Layout::flex_fill())` |
| `vexo/src/widgets/safe_area.rs:1414` (test) | `SafeArea` (inner) | `SafeAreaClaim::bottom(WithLayout::new(SafeArea::new(Text::new("Hi")), Layout::flex_fill()))` |

**No change** for `.flex_fill()` on `Flex`/`Column`:
`shared_app/src/chats/chat_screen.rs:110` (`Column::new().flex_fill()`)
and `vexo_uikit/src/navigation.rs:756` (`Flex::column().flex_fill()`) —
both resolve to `Flex`'s inherent `flex_fill()` via
`layout_builder_methods!()`.

### Group B: other layout methods on widgets without inherent methods (3 sites)

| File:line | Before | After |
|---|---|---|
| `vexo/src/integration_tests.rs:475` | `ScrollView::new(column.boxed()).width(200.0).height(300.0)` | `WithLayout::new(ScrollView::new(column.boxed()), Layout::default().width(200.0).height(300.0))` |
| `vexo/src/focus/integration_tests.rs:804` | `ScrollView::new(column).width(200.0).height(100.0)` | `WithLayout::new(ScrollView::new(column), Layout::default().width(200.0).height(100.0))` |
| `shared_app/src/chats/chat_screen.rs:176` | `TextEdit::new(controller).flex_grow(1.0)` | `WithLayout::new(TextEdit::new(controller), Layout::default().flex_grow(1.0))` |

### Group C: `.with_layout(Layout)` call sites that resolve to the trait default (9 sites)

These are call sites where the receiver is `Text` (no inherent `with_layout`) or `Box<dyn Widget>` — they resolve to the trait default and must migrate.

Note: `GestureDetector` has an **inherent** `with_layout` at `gesture_detector.rs:103` that sets `self.layout` directly (no wrapping). Call sites like `GestureDetector::new(content).on_press(...).with_layout(layout)` (`tab_bar.rs:175`) and the test sites at `gesture_detector.rs:748, 769` resolve to the inherent method and are **not migrated** — they're already safe.

| File:line | Before | After |
|---|---|---|
| `vexo_uikit/src/tab_bar.rs:197` | `bar.with_layout(Layout::default()...)` (`bar: Box<dyn Widget>`) | `WithLayout::new(bar, Layout::default()...)` |
| `vexo/src/widgets/with_layout.rs:447` (test) | `Text::new("Hello").with_layout(Layout::default().padding(10.0))` | `WithLayout::new(Text::new("Hello"), Layout::default().padding(10.0))` |
| `vexo/src/e2e_test.rs:611` | `Text::new("Left").with_layout(Layout::default().flex_grow(1.0))` | `WithLayout::new(Text::new("Left"), Layout::default().flex_grow(1.0))` |
| `vexo/src/e2e_test.rs:612` | `Text::new("Center").with_layout(Layout::default().width(100.0))` | `WithLayout::new(Text::new("Center"), Layout::default().width(100.0))` |
| `vexo/src/e2e_test.rs:613` | `Text::new("Right").with_layout(Layout::default().flex_grow(2.0))` | `WithLayout::new(Text::new("Right"), Layout::default().flex_grow(2.0))` |
| `vexo/src/e2e_test.rs:645, 652, 659, 666` (4 sites) | `Text::new(...).with_layout(...)` | `WithLayout::new(Text::new(...), ...)` |

### Out-of-scope call sites (no migration)

- `vexo/src/widgets/text.rs:131` and
  `vexo/src/widgets/text_edit_content.rs:128` — these call
  `.with_layout()` on `TextRenderObject` / `TextEditRenderObject`
  (render objects, not widgets). Different method, different concern.
  Left unchanged.
- `vexo/src/render_objects/text.rs:543` and
  `vexo/src/render_objects/text_edit.rs:753` — same, render objects.
- `vexo_uikit/src/tab_bar.rs:175` — `GestureDetector::new(content)
  .on_press(...).with_layout(...)`. `GestureDetector` has an inherent
  `with_layout` (`gesture_detector.rs:103`) that sets `self.layout`
  directly. No wrapping, no footgun. Left unchanged.
- `vexo/src/widgets/gesture_detector.rs:748, 769` (test sites) — same,
  resolve to `GestureDetector`'s inherent `with_layout`. Left unchanged.

### Migration strategy

- **Framework change first** (remove 9 trait methods, add
  `Layout::flex_fill()`), then migrate all call sites. The framework
  commit will not compile until call sites are migrated, so the framework
  + all call-site migrations land in a single PR.
- **No deprecation period** — methods removed outright. Internal
  codebase, no external consumers.
- **No inherent-method call sites are touched.** `Flex::column().padding(8.0)`,
  `Text::new("x").padding(8.0)`, `DecoratedContainer::new(...).padding(...)`,
  etc. keep working unchanged. Only trait-default call sites migrate, and
  those already produced `WithLayout` widgets.

## Error Handling & Edge Cases

### Compile-time safety is the entire point

The 9 removed methods were the only path through which
`WithLayout::new(self, Layout::default().<prop>(value))` could be called
implicitly. After removal:

- **No way to introduce a `WithLayout` node without writing
  `WithLayout::new(...)` at the call site.** The `FlexDirection::Column` +
  `AlignItems::Stretch` injection that `WithLayout::new` applies
  (`with_layout.rs:264-269`) is now visible to every caller — they wrote
  the constructor themselves.
- **No silent wrapping.** A widget that didn't ask for layout opinion
  gets none. The latent sizing bug class (where `ScrollView.flex_fill()`
  injected a Column wrapper around the scroll view) is structurally
  impossible.

### Migration edge cases

1. **`GestureDetector::new(content).on_press(...).with_layout(layout)`**
   (`tab_bar.rs:175`) — `GestureDetector` has an **inherent** `with_layout`
   at `gesture_detector.rs:103` that sets `self.layout` directly (no
   wrapping). `on_press` returns `Self` (not `Box<dyn Widget>`), so the
   chain resolves to the inherent method. **No migration needed** — already
   safe.

2. **`bar.with_layout(...)`** where `bar: Box<dyn Widget>`
   (`tab_bar.rs:197`) — same pattern. `WithLayout::new(bar, layout)`
   accepts `impl Widget + 'static`, and `Box<dyn Widget>` implements
   `Widget` (the `impl Widget for Box<dyn Widget>` delegation at
   `mod.rs:346`). Mechanical migration.

3. **`Text::new("Hello").with_layout(layout)`** in tests — `Text` has no
   inherent `with_layout` (only `modifier_methods!()` properties). The
   call resolves to the trait default. After migration:
   `WithLayout::new(Text::new("Hello"), layout)`. No behavior change.

4. **Reconciliation stability across the migration** —
   `WithLayout::new(Text, layout)` and `Text.with_layout(layout)` both
   produce a `WithLayout` widget (same `type_id()`). If the migration
   lands in a tree position where an existing `WithLayout` element is
   present, `can_update()` returns true and the element updates in
   place. **No tree rebuild risk.** (In practice this only matters if
   migration lands mid-session against a running app, which it won't —
   these are source edits.)

5. **`Layout::flex_fill()` vs the old `.flex_fill()` trait method** —
   the trait method wrapped in `WithLayout::new(self,
   Layout::default().flex_grow(1.0).flex_basis(0.0).min_height(0.0))`,
   and `WithLayout::new` then injected `FlexDirection::Column` +
   `AlignItems::Stretch` on top. The new `Layout::flex_fill()`
   constructor returns `Layout::default().flex_grow(1.0).flex_basis(0.0).min_height(0.0)`
   — same starting Layout, and `WithLayout::new(..., Layout::flex_fill())`
   applies the same Column+Stretch injection. **Behavior is identical.**
   The migration is purely syntactic for `flex_fill` callers.

6. **Inherent methods on `Flex`/`Column`/`Row`/`Stack`/`Grid`/
   `IndexedStack`/`Text`/`Image`/`TextEditContent`/`DecoratedContainer`**
   — these set the widget's own `layout` field, no wrapping. They are
   **not affected** by the trait method removal. Callers like
   `Flex::column().padding(8.0)` keep working unchanged. This is the
   most common call pattern in the codebase (~40 of ~60 `.padding()`
   call sites) so the migration stays small.

### What can go wrong

- **Missed call site** → compile error. The compiler catches every one.
  No silent runtime regression possible.
- **Migration introduces a different `type_id()` at a tree position** →
  reconciler unmounts old element, mounts new. Only happens if a caller
  switches from inherent method to `WithLayout` wrapper (e.g.
  `Flex::column().padding(8.0)` →
  `WithLayout::new(Flex::column(), Layout::default().padding(8.0))`).
  **The migration plan does not do this** — inherent-method call sites
  are left alone. Only trait-default call sites migrate, and those
  already produced `WithLayout` widgets.
- **Doc drift** — old specs
  (`2026-04-21-layout-system-redesign.md`,
  `2026-06-09-css-like-layout-authoring-design.md`,
  `2026-06-23-web-developer-concept-mapping-design.md`) reference
  `.with_layout()` / `.padding()` / `.flex_fill()` as trait methods.
  These are historical design docs, not live API docs. **Left as-is** —
  they document the design at the time of writing. This spec documents
  the change.

## Testing

### New unit tests

**`vexo/src/layout/style.rs`** (extend existing tests):

- `test_layout_flex_fill_constructor` — `Layout::flex_fill()` produces
  `flex_grow == Some(1.0)`, `flex_basis == Some(Dimension::Length(0.0))`,
  `min_height == Some(Dimension::Length(0.0))`, and all other fields
  stay at `None`/default. Regression guard for the preset.

**`vexo/src/widgets/with_layout.rs`**:

- `test_with_layout_doc_example_compiles` — sanity check that the
  updated doc example (`WithLayout::new(Text::new("Hello"),
  Layout::default().padding(16.0).align_self(AlignSelf::Center))`)
  compiles and produces a widget with the expected layout. Replaces the
  deleted `test_with_layout_method_on_widget`.

### Tests that must still pass (regression guards)

- All `with_layout.rs` tests except the deleted one — they exercise
  `WithLayout::new(...)` directly, unaffected by the trait method
  removal.
- All `mod.rs` widget tests (`test_widget_trait_on_press_wraps`,
  `test_widget_trait_cursor_wraps`, `test_widget_trait_translate_wraps`,
  `test_widget_trait_on_press_chain`, `test_widget_trait_boxed`) — these
  exercise behavioral/transform trait defaults, which are not removed.
- `e2e_test.rs:802` "Regression guard for the latent WidgetExt sizing
  bug" — currently tests that a `Box<dyn Widget>.background(RED)` doesn't
  break sizing. The test name references "WidgetExt" historically; the
  test itself exercises `DecoratedBox` routing (already migrated by the
  prior DecoratedBox split). **Still passes unchanged.** Optionally
  rename the test to drop the obsolete "WidgetExt" reference — cosmetic,
  left for a separate cleanup if desired.
- All `integration_tests.rs`, `focus/integration_tests.rs` tests that
  use `ScrollView.width().height()` — **migrated to
  `WithLayout::new(ScrollView::new(...), Layout::default().width(...).height(...))`**,
  then must still pass. These verify width propagation to children (the
  original sizing-bug regression).
- `shared_app/src/integration_tests.rs` chat-screen layout tests —
  after migration of `ScrollView.flex_fill()` call sites, must still
  pass. These verify the scroll view fills available space.

### Compile-test as the primary guard

The strongest test for this change is **`cargo build --workspace`**.
Every missed migration site is a compile error. Once the workspace
builds, the migration is complete by construction — there is no runtime
behavior change (Group A/B/C migrations are purely syntactic rewrites
that produce the same widget trees).

### Test plan

1. `cargo build -p vexo` — framework compiles, `Layout::flex_fill()`
   exists, trait methods gone.
2. `cargo build --workspace` — all call sites migrated, workspace
   compiles.
3. `cargo test -p vexo` — framework unit + integration tests pass,
   including `Layout::flex_fill()` test and the sizing-bug regression
   guards in `e2e_test.rs` / `integration_tests.rs`.
4. `cargo test -p shared_app` — chat-screen / contacts-screen
   integration tests pass (these exercise the migrated
   `ScrollView.flex_fill()` paths).
5. `cargo test -p vexo_uikit` — navigation / tab-bar tests pass (these
   exercise the migrated `SafeArea.flex_fill()` / `Box<dyn
   Widget>.with_layout()` paths).
6. `cargo test --workspace` — full sweep.

### Manual verification (out of scope for this spec)

The user runs `cargo run -p desktop_demo` to visually confirm chat
screen / contacts / tab bar layouts are unchanged. Per `CLAUDE.md`, the
agent does not run the GUI itself.

## File-Level Change Summary

| File | Change |
|---|---|
| `vexo/src/widgets/mod.rs` | **DELETE** lines 178–264: 9 trait-default layout methods (`.with_layout`, `.padding`, `.margin`, `.width`, `.height`, `.flex_grow`, `.flex_fill`, `.align_self`, `.absolute`) + `// Layout modifiers` section comment. Drop now-unused `use crate::layout::Layout` import if no longer referenced. |
| `vexo/src/layout/style.rs` | **ADD** `Layout::flex_fill()` constructor with the doc text currently on the trait method (CSS `flex: 1 1 0` + `min-height: 0` explanation). **ADD** `test_layout_flex_fill_constructor` unit test. |
| `vexo/src/widgets/with_layout.rs` | **UPDATE** doc example at lines 233–247 to use `WithLayout::new(...)` form instead of `Text::new(...).with_layout(...)`. **DELETE** `test_with_layout_method_on_widget` test at lines 446–449 (tests removed method). **ADD** `test_with_layout_doc_example_compiles` test. |
| `vexo/src/e2e_test.rs` | **MIGRATE** 7 `.with_layout()` call sites (lines 611, 612, 613, 645, 652, 659, 666) to `WithLayout::new(...)` form. |
| `vexo/src/integration_tests.rs` | **MIGRATE** line 475 — `ScrollView::new(...).width(200.0).height(300.0)` → `WithLayout::new(ScrollView::new(...), Layout::default().width(200.0).height(300.0))`. |
| `vexo/src/focus/integration_tests.rs` | **MIGRATE** line 804 — `ScrollView::new(...).width(200.0).height(100.0)` → `WithLayout::new(...)` form. |
| `vexo/src/widgets/safe_area.rs` | **MIGRATE** line 1414 (test) — `SafeArea::new(Text::new("Hi")).flex_fill()` → `WithLayout::new(SafeArea::new(Text::new("Hi")), Layout::flex_fill())`. |
| `shared_app/src/chats/conversation_list.rs` | **MIGRATE** line 26 — `ScrollView::new(...).flex_fill().boxed()` → `WithLayout::new(ScrollView::new(...), Layout::flex_fill()).boxed()`. |
| `shared_app/src/chats/chat_screen.rs` | **MIGRATE** line 114 — `ScrollView::new(...).flex_fill()` → `WithLayout::new(...)` form. **MIGRATE** line 176 — `TextEdit::new(controller).flex_grow(1.0)` → `WithLayout::new(TextEdit::new(controller), Layout::default().flex_grow(1.0))`. |
| `shared_app/src/contacts/contacts_screen.rs` | **MIGRATE** line 13 — `ScrollView::new(...).flex_fill().boxed()` → `WithLayout::new(...)` form. |
| `vexo_uikit/src/navigation.rs` | **MIGRATE** line 748 — `SafeArea::new(clipped).top(false).flex_fill()` → `WithLayout::new(SafeArea::new(clipped).top(false), Layout::flex_fill())`. |
| `vexo_uikit/src/tab_bar.rs` | **MIGRATE** line 197 — `bar.with_layout(...)` → `WithLayout::new(bar, ...)`. **MIGRATE** line 224 — `SafeAreaClaim::bottom(stack).flex_fill()` → `WithLayout::new(SafeAreaClaim::bottom(stack), Layout::flex_fill())`. (Line 175 `GestureDetector.with_layout(...)` is NOT migrated — uses inherent method.) |

**Total: 11 files edited, ~18 call sites migrated, 9 methods removed, 1
constructor + 2 tests added, 1 test deleted.**

## Resolved Decisions

1. **Fate of `.with_layout(Layout)`** → Remove it too. Maximum symmetry
   with `DecoratedBox::new(...)`. The 9 `.with_layout()` call sites that
   resolve to the trait default migrate to `WithLayout::new(...)` form.
   (Call sites that resolve to `GestureDetector`'s inherent `with_layout`
   are already safe and unchanged.)
2. **`Layout::flex_fill()` constructor** → Add it. The `flex_fill`
   preset (CSS `flex: 1 1 0` + `min-height: 0`) is a meaningful concept
   that deserves a named home on `Layout`. Call sites stay readable:
   `WithLayout::new(scrollView, Layout::flex_fill())`.
3. **No deprecation period** → Remove outright. Internal codebase, no
   external consumers. A deprecation period would leave the footgun live
   longer.
4. **No inherent-method call sites touched** → `Flex::column().padding(8.0)`
   and friends keep working unchanged. Only trait-default call sites
   migrate, and those already produced `WithLayout` widgets. This keeps
   the migration to ~21 sites instead of ~60.
5. **Historical specs left as-is** → They document the design at the
   time of writing. This spec documents the change.
