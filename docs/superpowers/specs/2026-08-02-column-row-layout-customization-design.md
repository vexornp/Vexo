# `column!`/`row!` Layout Customization Design

Date: 2026-08-02
Status: Draft
Depends on: `2026-07-31-view-builder-design.md` (the `column!`/`row!` macros)

## Problem

The `column!`/`row!` macros (`vexo/vexo_macros/src/lib.rs:14-42`) hardcode
`Layout::column()` / `Layout::row()` as the final argument to `MultiChild::new`.
There is no way to chain layout properties onto the macro result. Call sites
that need `.gap()`, `.padding()`, `.flex_shrink()`, etc. cannot use the macros
and must fall back to the verbose two-layer form:

```rust
// Today — can't use column! because gap + padding are needed:
MultiChild::new(children![A, B], Layout::column().gap(8.0).padding(12.0))
```

This shows up across the codebase — `chat_screen.rs:115`
(`Layout::column().gap(8.0).padding(12.0)`), `conversation_list.rs:167`
(`Layout::column().flex_shrink(0.0)`), `titled_container.rs:31`
(`Layout::row().height(...).flex_shrink(0.0)`), and many more. The original
view-builder spec (`2026-07-31-view-builder-design.md:411-412`) explicitly
deferred layout customization as "Future work." This spec delivers it.

## Goals

1. `column! { A, B }.gap(8.0).padding(12.0)` compiles and produces a
   `MultiChild` whose `Layout` is `Layout::column().gap(8.0).padding(12.0)` —
   the macro's default direction/align preserved, the chained properties
   added.
2. Every `Layout` instance builder (45 methods) is mirrored onto `MultiChild`
   as a fluent method with the same name and signature. No "method missing"
   for any layout property. (Instance builders are the `pub fn ... (mut self,
   ...) -> Self` methods in `vexo/src/layout/style.rs:365-683`; constructors
   like `Layout::column()` and conversion methods like `to_taffy_style()` are
   excluded — they don't fit the passthrough pattern.)
3. The fluent methods work on **all** `MultiChild`, not just macro output —
   `MultiChild::empty(Layout::column()).gap(8.0)` works too.
4. The `column!`/`row!` macros themselves are unchanged.
5. Existing public API (`MultiChild::new`, `with_layout`, `with_key`, `push`,
   `empty`) is unchanged. Purely additive.

## Non-Goals

- **Changing the `column!`/`row!` macro syntax.** No leading layout argument,
  no `#[attr]` syntax inside the block. The macro stays as-is; customization
  happens via postfix methods on the returned `MultiChild`.
- **Migrating existing call sites.** This feature is purely additive. Every
  existing `MultiChild::new(children![...], Layout::column().gap(8.0))` keeps
  working unchanged. Opportunistic migration of 1-2 sites as living examples
  is optional polish, not a success criterion.
- **Type-preserving builders, postfix modifier chaining, `stack!`/`grid!`
  macros.** All still out of scope, same as the parent spec
  (`2026-07-31-view-builder-design.md:62-84`).
- **New `Layout` methods.** `Layout`'s API is the source of truth; this spec
  only mirrors it onto `MultiChild`.
- **Behavioral change to reconciliation or rendering.** The generated
  `MultiChild` is identical to one built by hand with the same `Layout`.

## Approach: Postfix fluent methods on `MultiChild`, macro-generated

Three syntax options were considered:

1. **Postfix fluent on `MultiChild`** (chosen) — add 45 builder methods to
   `MultiChild` that modify its existing `Layout` in place. Works on all
   `MultiChild`, not just macro output. Mirrors `Layout`'s idiom exactly.
   Requires 45 methods, but generated via a single `macro_rules!` so they
   never drift from `Layout`.
2. **Single `.tune_layout(|l| ...)` method** — one closure-taking method that
   receives the current layout and returns a modified one. Minimal API
   surface, full power, but closure noise at every call site
   (`.tune_layout(|l| l.gap(8.0).padding(12.0))`) and doesn't match `Layout`'s
   idiom.
3. **Leading layout expression in the macro** — `column!(Layout::column().gap(8.0)) { A, B }`.
   Explicit, but the user writes `column` twice (macro name + `Layout::column()`),
  and it doesn't help non-macro `MultiChild`.

Option 1 was chosen because it gives the best ergonomics, works uniformly on
macro and non-macro `MultiChild`, and mirrors the `Layout` intuition users
already have. The macro-generation mechanic (below) addresses the maintenance
cost of 45 methods.

## Architecture

### Where the code lives

All changes are in **one file**: `vexo/src/widgets/multi_child.rs`. No changes
to `vexo_macros/`, `Layout`, `column!`/`row!`, `view_builder`, `ChildPush`, or
any existing public API.

### Method naming and semantics

The new methods mirror `Layout`'s instance builders **exactly** — same name,
same signature, same return type (`Self`). No `with_` prefix. The API reads
identically to `Layout`'s:

```rust
column! { A, B }.gap(8.0).padding(12.0)
row! { avatar, name }.justify(JustifyContent::SpaceBetween)
MultiChild::empty(Layout::column()).gap(8.0).flex_shrink(0.0)
```

**Modify, not replace.** Each method mutates `self.layout` by calling the
corresponding `Layout` builder, then returns `self`:

```rust
pub fn gap(mut self, value: f32) -> Self {
    self.layout = self.layout.gap(value);  // preserves flex_direction, align_items, etc.
    self
}
```

So `column! { A, B }.gap(8.0)` produces `Layout::column().gap(8.0)` — the
`column` direction and `Stretch` align from `Layout::column()` are preserved,
gap is added. This is the key behavioral distinction from `.with_layout()`,
which **replaces** the entire layout (throwing away the macro's default
direction/align).

**No errors for overrides.** Calling `.flex_direction(Row)` on a `column!`
result is allowed — it overrides the direction. The methods are low-level
setters; we trust the user's intent.

**`with_layout` stays public.** It remains the "replace the whole layout"
escape hatch, useful when you want `Layout::stack()` instead of
`Layout::column()` after the fact, or when modifying field-by-field is awkward.

### Naming distinction: `with_*` vs. bare

- **Bare names** (`.gap()`, `.padding()`, `.flex_shrink()`, ...) — **modify**
  the existing layout, preserving other fields. New in this spec.
- **`with_*` names** (`.with_layout()`, `.with_key()`) — **replace** a field
  wholesale. Existing API, unchanged.

This matches the convention already established by `Layout` (bare builders
modify-and-return) vs. `MultiChild::with_layout` (replace). The new methods
follow the dominant `Layout` convention.

### Generation via `macro_rules!`

`Layout`'s 45 builder methods are spread across
`vexo/src/layout/style.rs:365-683`. Hand-mirroring each onto `MultiChild`
would be 45 nearly-identical one-liners that must track `Layout` forever
(drift risk). Instead, generate them with a single `macro_rules!` inside
`multi_child.rs`:

```rust
macro_rules! impl_layout_passthrough {
    ($($method:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty),* $(,)?) => {
        $(
            #[doc = concat!("Set [`Layout::", stringify!($method), "`] on this container's layout.")]
            pub fn $method(mut self, $($arg: $ty),*) -> Self {
                self.layout = self.layout.$method($($arg),*);
                self
            }
        )*
    };
}

impl MultiChild {
    impl_layout_passthrough! {
        // Box model
        padding(value: f32) -> Self,
        padding_each(left: f32, right: f32, top: f32, bottom: f32) -> Self,
        margin(value: f32) -> Self,
        margin_each(left: f32, right: f32, top: f32, bottom: f32) -> Self,
        width(value: f32) -> Self,
        height(value: f32) -> Self,
        width_percent(value: f32) -> Self,
        height_percent(value: f32) -> Self,
        min_width(value: f32) -> Self,
        min_height(value: f32) -> Self,
        max_width(value: f32) -> Self,
        max_height(value: f32) -> Self,

        // Flexbox
        flex_direction(value: FlexDirection) -> Self,
        flex_wrap() -> Self,
        flex_wrap_mode(value: FlexWrap) -> Self,
        flex_grow(value: f32) -> Self,
        flex_shrink(value: f32) -> Self,
        flex_basis(value: f32) -> Self,
        justify(value: JustifyContent) -> Self,
        align(value: AlignItems) -> Self,
        align_content(value: AlignContent) -> Self,
        gap(value: f32) -> Self,
        gap_size(size: Size<Logical>) -> Self,
        gap_each(width: f32, height: f32) -> Self,

        // Grid
        columns(sizes: Vec<TrackSizing>) -> Self,
        rows(sizes: Vec<TrackSizing>) -> Self,
        grid_column(placement: GridPlacement) -> Self,
        grid_row(placement: GridPlacement) -> Self,
        grid_auto_flow(value: GridAutoFlow) -> Self,
        auto_rows(sizes: Vec<TrackSizing>) -> Self,
        auto_columns(sizes: Vec<TrackSizing>) -> Self,

        // Positioning
        absolute() -> Self,
        relative() -> Self,
        position(value: Position) -> Self,
        inset(value: f32) -> Self,
        top(value: f32) -> Self,
        right(value: f32) -> Self,
        bottom(value: f32) -> Self,
        left(value: f32) -> Self,

        // Per-item alignment
        align_self(value: AlignSelf) -> Self,

        // Display
        display(value: Display) -> Self,

        // Sizing
        aspect_ratio(value: f32) -> Self,

        // Overflow
        overflow(value: Overflow) -> Self,
        overflow_x(value: Overflow) -> Self,
        overflow_y(value: Overflow) -> Self,
    }
}
```

**Why a macro, not hand-written.**
- **Single source of truth for shape.** Every method body is
  `self.layout = self.layout.$method(args); self` — exactly the same shape.
  The macro enforces uniformity; can't accidentally write `gap` taking `self`
  by `&mut` and `padding` by value.
- **Low drift risk.** If `Layout` renames `.gap()` → `.spacing()`, only the
  one line in the invocation changes.
- **Mechanical bodies.** The generation is pure syntax — no proc-macro
  introspection of `Layout`'s trait impls (which would be heavy and fragile).

**Why still hand-list signatures.** The macro doesn't extract from `Layout`;
the invocation lists the signatures explicitly. This is a one-time cost. If
`Layout` adds a 31st method, someone adds one line to the invocation. The
macro guarantees the *bodies* stay uniform; the list guarantees *coverage*.

**What stays hand-written.** `with_layout` (replace), `with_key` (unrelated
field), `push` (mutates `children`, not `layout`), `empty`/`new`
(constructors). These don't fit the passthrough pattern.

**Doc forwarding.** Each generated method gets a one-line doc pointing to
`Layout::$method` via `concat!` + `stringify!`. Not a full doc copy — keeps
the macro simple and avoids stale prose if `Layout`'s docs evolve. Users click
through to `Layout` for details.

**Note on `flex_wrap()`.** `Layout::flex_wrap()` takes no argument (always
sets `Wrap`). The macro handles zero-arg methods: the `$($arg: $ty),*` repeat
matches empty, and the call `self.layout.flex_wrap()` is generated correctly.

### Required imports in `multi_child.rs`

The generated methods reference types that must be imported. The file
currently imports `Layout` only. Add:

```rust
use crate::core::{Logical, Size};
use crate::layout::{
    AlignContent, AlignItems, AlignSelf, Display, FlexDirection, FlexWrap, GridAutoFlow,
    GridPlacement, JustifyContent, Overflow, Position, TrackSizing,
};
```

(These are re-exported from `vexo/src/layout/mod.rs:66-69` and are part of the
public API. If any are not currently re-exported from `vexo/src/lib.rs`, they
must be added there so users can write `JustifyContent::SpaceBetween` at the
call site — verify during implementation.)

## Testing Strategy

Three layers, all small:

### Layer 1: Unit tests in `multi_child.rs`

Verify each generated method actually mutates `layout`. Don't test all 45
individually (that's testing the macro mechanic, not behavior); test one
representative per *group* (box model, flex, grid, positioning, overflow) plus
the override case and the chaining case:

- `column! { A, B }.gap(8.0)` → `layout.gap == Some(8.0)` and `flex_direction`
  still `Column` (preservation)
- `.padding(12.0)` preserves `flex_direction`
- `.flex_shrink(0.0)` on `MultiChild::empty(Layout::row())` → `flex_shrink`
  set and `flex_direction` preserved
- `.justify(SpaceBetween)` override works
- `.columns(vec![TrackSizing::Fr(1.0)])` sets `grid_template_columns`
- `.absolute().top(10.0)` sets position + inset
- `.overflow(Overflow::Hidden)` sets both x and y
- **Chaining:** `.gap(8.0).padding(12.0).flex_shrink(0.0)` → all three set
- **Override semantic:** `column! { A }.flex_direction(Row)` →
  `flex_direction == Row` (no error, user intent honored)

**Accessor:** tests use the existing `MultiChild::layout_ref()` inherent method
(`vexo/src/widgets/multi_child.rs:83`), which returns `&Layout`. No new
accessor needed.

### Layer 2: Macro integration test in `vexo/tests/builder_macros.rs`

One test that proves the macro + fluent chain composes end-to-end:

```rust
#[test]
fn column_macro_with_fluent_layout_chain() {
    let mc = column! { Text::new("a"), Text::new("b") }
        .gap(8.0)
        .padding(12.0);

    assert_eq!(mc.children().len(), 2);
    assert_eq!(mc.layout_ref().flex_direction, Some(FlexDirection::Column));
    assert_eq!(mc.layout_ref().gap, Some(Size::new(8.0, 8.0)));
    assert!(mc.layout_ref().padding.is_some());
}
```

This is the single test that proves the feature works as the user experiences
it. Existing `builder_macros.rs` tests (from the parent spec) verify the
macros themselves; this one verifies the new composition.

### Layer 3: No new trybuild tests

The `column!`/`row!` macros themselves don't change. The fluent methods are
ordinary Rust methods — type errors (e.g. wrong arg type) are Rust's job, and
macro UI tests already cover macro syntax. No new `.stderr` snapshots needed.

### Test inventory

| Layer | Location | What it tests | Count |
|---|---|---|---|
| Unit | `vexo/src/widgets/multi_child.rs` (tests mod) | Generated methods mutate layout; preservation; chaining; override | ~9 cases |
| Integration | `vexo/tests/builder_macros.rs` | `column!{...}.gap().padding()` composes end-to-end | 1 case |

### Acknowledged gaps

- **Full coverage of all 45 methods.** Tested representatively, not
  exhaustively. The macro mechanic is uniform; testing `gap` proves `padding`,
  `margin`, etc. follow the same shape. If a method were ever hand-written
  (none are), it would need its own test.
- **Layout correctness.** Out of scope — the methods delegate to `Layout`'s
  builders, which are already tested in `vexo/src/layout/style.rs:1122+`. The
  passthrough doesn't touch layout logic.
- **Reconciliation behavior.** The generated `MultiChild` reconciles
  identically to a hand-written one with the same `Layout` because it's the
  same code. Existing pipeline tests cover this.
- **Performance.** No benchmark. The methods are `self.layout =
  self.layout.$method(args); self` — same cost as chaining on `Layout`
  directly.

## Rollout Plan

Single small PR, all in `vexo/src/widgets/multi_child.rs` plus one test file:

1. **Add imports** to `multi_child.rs` (`Size`, `Logical`, the layout enums).
2. **Add the `impl_layout_passthrough!` macro** definition + invocation
   (45 entries matching `Layout`'s instance builders, including zero-arg
   `flex_wrap`).
3. **Add unit tests** in the `tests` mod of `multi_child.rs`.
4. **Add one integration test** in `vexo/tests/builder_macros.rs`.
5. **Verify:** `cargo build && cargo test` across the workspace.
6. **(Optional polish)** Migrate 1-2 representative call sites
   (`chat_screen.rs:115`, `conversation_list.rs:167`) as living examples. Not
   a gate.

No changes to: `vexo_macros/`, `Layout`, `column!`/`row!` macros,
`view_builder`, `ChildPush`, or any existing public API. Purely additive.

## Success Criteria

The feature is done when:

1. `cargo build` and `cargo test` pass across all crates.
2. `column! { A, B }.gap(8.0).padding(12.0)` compiles and produces a
   `MultiChild` with 2 children, `flex_direction == Column`, `gap == Some(8.0)`,
   and `padding` set.
3. All `Layout` instance builders are mirrored onto `MultiChild`; calling
   `.gap()` / `.padding()` / `.flex_shrink()` / `.justify()` / etc. on any
   `MultiChild` works without falling back to `.with_layout()`.
4. Existing `MultiChild::new`, `with_layout`, `with_key`, `push`, `empty` APIs
   are unchanged and existing call sites compile without modification.
5. Unit tests cover preservation, chaining, and override semantics.
6. One integration test proves the macro + fluent chain composes end-to-end.

## Out of Scope (Deferred)

- **Migrating existing `MultiChild::new(children![...], Layout::column()...)`
  call sites.** Purely additive feature; migration is optional cleanup.
- **Changing `column!`/`row!` macro syntax** (leading layout arg, attribute
  syntax, etc.). The macros stay as-is.
- **`stack!`/`grid!` macros.** Same as parent spec; the dedicated `Stack`/
  `Grid` widgets and `.push()` builder remain the path for those.
- **Postfix modifier chaining preserving generic type** (`.padding()
  .background().onTapGesture()` returning `some Widget`). Declined in parent
  spec; `WithLayout`/`DecoratedBox` wrappers remain.
- **`#[view_builder]` attribute on functions/closures.** Separate follow-up.
- **Type-preserving builder.** Declined in parent spec.
- **Auto-extraction of `Layout` methods via proc-macro.** The `macro_rules!`
  approach with hand-listed signatures is simpler and sufficient.
