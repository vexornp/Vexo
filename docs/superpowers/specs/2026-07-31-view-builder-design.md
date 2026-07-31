# View Builder (SwiftUI-style Result Builder) Design

Date: 2026-07-31
Status: Draft

## Problem

Vexo's widget composition today requires a two-layer ceremony for every
container:

```rust
MultiChild::new(
    children![
        Text::new("a"),
        Text::new("b"),
    ],
    Layout::column(),
)
```

The `children![]` macro produces a `Vec<Box<dyn Widget>>`, which the caller
must then wrap in `MultiChild::new(vec, layout)` with the layout passed
positionally. The layout is a separate argument, not bound to the children
block. Conditionals require an `Option<Box<dyn Widget>>` dance
(`if cond { Some(x.boxed()) } else { None }` then `children![x]`), and
dynamic lists require an imperative `list = list.push(row)` loop
(`shared_app/src/chats/conversation_list.rs:53-71`).

SwiftUI's `@ViewBuilder` solves this with a result-builder attribute that
rewrites control flow (`if`/`else`, `for`, `match`) into `buildBlock` /
`buildOptional` / `buildEither` / `buildArray` calls, letting you write:

```swift
VStack {
    Text("title")
    if showSubtitle { Text("subtitle") }
    ForEach(items) { item in Row(item) }
}
```

Vexo wants an analogous feature: `column!`/`row!` macros that accept plain
widget expressions, `if`/`else`, `for`, and `match` inside a single block,
eliminating the two-layer ceremony and the imperative-loop pattern.

## Goals

1. `column! { ... }` and `row! { ... }` macros that produce a `MultiChild`
   with the correct `Layout` baked in.
2. Support `if` (with and without `else`), `for`, and `match` as
   first-class statements inside the block, flattened into the parent
   (no extra tree nodes).
3. Exact Swift-like syntax — no marker prefixes (`@if`, `@for`).
4. A documented, macro-callable `view_builder` helper API mirroring
   Swift's `ViewBuilder` vocabulary (`build_block`, `build_optional`,
   `build_either`, `build_array`).
5. Sample migration of 3 representative call sites in `shared_app` to
   prove the ergonomics in real code.
6. `children![]` and `MultiChild::new(children, layout)` remain public
   and unchanged — additive, not breaking.

## Non-Goals

- `stack!`/`grid!` macros. Only column/row in scope; the dedicated
  `Stack`/`Grid` widgets and their `.push()` builder remain the path for
  those.
- Postfix modifier chaining that preserves a generic type
  (`.padding().background().onTapGesture()` returning `some Widget`).
  The existing `Widget` trait modifiers (`.on_tap()`, `.cursor()`) and
  `WithLayout`/`DecoratedBox` wrappers remain the way to apply these.
  The "Full SwiftUI ergonomics" option was explicitly declined.
- Type-preserving builder (Swift's `buildBlock` returning tuples,
  `buildEither` returning `Either<A, B>`). Vexo's `can_update()` uses
  `TypeId + key`, not structural type identity, so preserved types buy
  nothing for reconciliation. The design uses type-erased helpers.
- `#[view_builder]` attribute on functions/closures. A natural follow-up
  but separate work.
- `while`/`loop` support. Rare in declarative UI; use `for`.
- `let` bindings inside the block. Compute outside.
- Crate-rename support. `::vexo::` paths are hardcoded (documented
  limitation, matching ecosystem norm).
- Macro-time type checking. Non-widget expressions fail at Rust's type
  checker via a missing `ChildPush` impl — that is the type safety.
- Full migration of all `MultiChild::new(...)` call sites. Only 3 sample
  sites migrate; the rest stay on the old API.

## Architecture

### New crate: `vexo_macros`

Proc-macros must be their own crate (`proc-macro = true`). A new
`vexo_macros/` crate lives alongside `vexo/`:

```
ui_platform/
├── vexo_macros/                 ← NEW
│   ├── Cargo.toml               ← proc-macro = true; deps: syn 2, quote 1, proc-macro2 1
│   └── src/
│       └── lib.rs               ← parser + codegen for column!/row!
├── vexo/
│   └── src/
│       └── view_builder.rs      ← NEW: type-erased helper API
├── shared_app/
├── desktop_demo/
└── Cargo.toml                   ← workspace members add vexo_macros
```

### New module: `vexo/src/view_builder.rs`

The type-erased helper API. Free functions, no trait. Mirrors Swift's
`ViewBuilder` vocabulary. Each function is a thin adapter over
`Vec<Box<dyn Widget>>` + `ChildPush`.

### Crate dependency graph

```
vexo_macros  →  (syn, quote, proc-macro2)   [build-time only]
vexo         →  vexo_macros                  [path dep]
shared_app   →  vexo                          [existing]
desktop_demo →  vexo, shared_app              [existing]
```

`vexo_macros` depends on nothing in the workspace — it only generates
code referencing `vexo` paths. No circular dep.

`vexo_macros`'s dev-dependencies include `vexo` (path dep) so UI test
files can `use vexo::{column, row}`. This creates a dev-only cycle
(`vexo` build-depends on `vexo_macros`; `vexo_macros` dev-depends on
`vexo`), which Cargo permits because dev-deps aren't used for the
library build.

### Re-exports

```rust
// vexo/src/lib.rs
pub mod view_builder;
pub use view_builder::{build_block, build_optional, build_array, build_either};
pub use vexo_macros::{column, row};
```

## Approach: Type-erased helpers (Approach 2a)

Three approaches were considered:

1. **Vec-accumulator + `ChildPush`** — macro emits inline `push_into`
   calls; no named builder API. Simplest, but no vocabulary surface.
2. **Explicit `ViewBuilder` helper functions** (this design) — macro
   emits calls to named `build_*` functions mirroring Swift's API.
   Sub-variants:
   - **2a: Type-erased helpers** (chosen) — helpers are thin adapters
     over `Vec<Box<dyn Widget>>`; `build_either` is identity. Named
     API surface, macro-callable, path to future `#[view_builder]`
     attribute, low cost.
   - **2b: Type-preserving builder** — `build_block` returns tuples,
     `build_either` returns `Either<A, B>`, tuples implement `Widget`.
     Requires per-arity traits (no variadic generics on stable Rust)
     and combinatorial tuple impls. Vexo's `TypeId`-based `can_update`
     doesn't exploit preserved types — marginal payoff. Rejected.
3. **Branches-as-`MultiChild`** — each `if`/`else` arm and `for` body
   compiles to its own `MultiChild` node. Injects extra layout nodes,
   changing layout behavior and breaking "flatten into parent"
   semantics. Rejected — semantically wrong.

The chosen design (2a) provides a documented, macro-callable builder
API mirroring Swift's vocabulary, at low cost, while flattening
correctly via `ChildPush`. The named layer earns its keep as
vocabulary and as a path to a future `#[view_builder]` attribute; it
does not change runtime behavior vs. inline pushes.

## Macro Syntax

### Basic containers

```rust
// Column — vertical stack
column! {
    Text::new("title"),
    Text::new("subtitle"),
}

// Row — horizontal stack
row! {
    avatar,
    info_col,
    right_col,
}
```

Comma-separated children (trailing separator allowed). Semicolons also
accepted as separators. A separator is **required between** children;
`column! { A B }` (no separator) is rejected. A single child with no
trailing separator is fine (`column! { A }`). **Mixing `,` and `;`
within one block is rejected** with a clear error pointing at the first
mixed separator.

### Conditionals

```rust
column! {
    Text::new("name"),

    if conv.unread_count > 0 {
        unread_badge
    } else {
        Empty::new()
    },

    if item.has_subtitle {     // if WITHOUT else — buildOptional
        Text::new(item.subtitle)
    },
}
```

`if` without `else` renders nothing when the condition is false (the
`build_optional` helper returns an empty `Vec`). `if`/`else` picks one
branch. Both arms must produce widget expressions.

### Loops

```rust
column! {
    for conv in &self.conversations {
        build_conversation_row(conv)
    },
}
```

`for` loops interleave with plain children and conditionals freely.

### Match (bonus)

```rust
column! {
    match state {
        State::Loading => spinner,
        State::Error(msg) => error_view(msg),
        State::Loaded(data) => content(data),
    },
}
```

`match` arms are treated like `if`/`else` arms — each arm produces a
widget, the taken arm renders. Arm bodies must be single widget
expressions; a block `=> { let s = ..; Text::new(&s) }` works because
the block's trailing expression is the widget.

### Nesting

```rust
column! {
    row! {
        avatar,
        column! { name, subtitle },
    },
    body,
}
```

Nested builders produce widgets that flow into the outer block via the
existing `W: Widget` blanket `ChildPush` impl.

### Empty block

`column! { }` expands to `MultiChild::new(Vec::new(), Layout::column())`
— a valid empty container (same as today's
`MultiChild::empty(Layout::column())`).

## Macro Expansion Model

### The accumulator pattern

Every `column!`/`row!` block expands to the same skeleton:

```rust
// column! { A, B, if c { D } else { E }, for x in xs { F } }
{
    let mut __vexo_children: ::std::vec::Vec<::std::boxed::Box<dyn ::vexo::widgets::Widget>>
        = ::std::vec::Vec::new();

    ::vexo::widgets::ChildPush::push_into(A, &mut __vexo_children);
    ::vexo::widgets::ChildPush::push_into(B, &mut __vexo_children);

    ::vexo::widgets::ChildPush::push_into(
        ::vexo::view_builder::build_either(
            if c { D.boxed() } else { E.boxed() }
        ),
        &mut __vexo_children,
    );

    ::vexo::widgets::ChildPush::push_into(
        ::vexo::view_builder::build_array(
            xs.into_iter().map(|x| F.boxed()).collect::<::std::vec::Vec<_>>()
        ),
        &mut __vexo_children,
    );

    ::vexo::widgets::MultiChild::new(__vexo_children, ::vexo::layout::Layout::column())
}
```

`row!` is identical except the final line emits `Layout::row()`.

### Invariants

- **One `Vec`, one `MultiChild`.** No matter how many conditionals/loops,
  the block produces exactly one `MultiChild` node.
- **Real Rust control flow.** `if`/`for`/`match` are emitted as real
  Rust expressions — lazy, borrow-checked, lifetime-checked normally.
- **Absolute paths.** All generated code uses `::vexo::...` absolute
  paths so the macro works regardless of what's in scope at the call
  site (except the user's own expressions like `A`, `D`, `F`).

### Per-statement expansion rules

| Statement form | Emitted code | Flattens as |
|---|---|---|
| `expr,` (plain widget) | `ChildPush::push_into(expr, &mut __v);` | 1 child |
| `if c { body }` (no else) | `ChildPush::push_into(build_optional(if c { Some(body.boxed()) } else { None }), &mut __v);` | 0 or 1 |
| `if c { a } else { b }` | `ChildPush::push_into(build_either(if c { a.boxed() } else { b.boxed() }), &mut __v);` | exactly 1 |
| `for x in xs { body }` | `ChildPush::push_into(build_array(xs.into_iter().map(\|x\| body.boxed()).collect()), &mut __v);` | 0..N |
| `match e { arms }` | `ChildPush::push_into(match e { arms => arm_expr.boxed() }, &mut __v);` | exactly 1 |
| `let x = ...;` | **Compile error**: "let bindings are not allowed inside a builder block; compute outside" | — |
| `expr;` (non-widget) | Fails to compile (no `ChildPush` impl) — type-safe | — |

### Match arm expansion

`match` is parsed with `syn`. Each arm's body expression is wrapped in
`.boxed()`; patterns, guards, and the scrutinee pass through unchanged:

```rust
// user writes:
match state {
    State::Loading => spinner,
    State::Error(msg) => error_view(msg),
}
// macro emits:
::vexo::widgets::ChildPush::push_into(
    match state {
        State::Loading => spinner.boxed(),
        State::Error(msg) => error_view(msg).boxed(),
    },
    &mut __vexo_children,
);
```

A bare block `=> { let s = ..; Text::new(&s) }` works: the block
evaluates to its trailing expression (the widget), and `.boxed()`
erases it.

## Proc-macro Parsing Strategy

### Input shape

The proc-macro receives the block contents as a `TokenStream` (the
interior of `{ ... }` — no outer braces).

### Brace-aware splitting

`proc-macro2::TokenTree::Group` is atomic — a `{ ... }` block or
struct literal arrives as a single token, so its interior commas don't
leak. The splitter walks top-level tokens, ending a statement at a
top-level `,` or `;` (a depth counter is kept as defensive
belt-and-suspenders for the rare non-Group case).

### Separator uniformity check

After splitting, the macro checks that all separators are the same
kind. If both `,` and `;` appear at top level, it emits a compile
error spanning the first mixed separator:

```
error: mixing `,` and `;` separators is not allowed inside `column!`; pick one
  --> src/foo.rs:42:18
   |
42 | column! { A, B; C, }
   |                   ^
```

### Statement classification via `syn`

Each split statement is parsed as `syn::Expr`:

1. **`Expr::If`** → check `.else_branch`:
   - `None` → `build_optional` path
   - `Some(_)` → `build_either` path (both arms present)
2. **`Expr::ForLoop`** → `build_array` path
3. **`Expr::Match`** → wrap each arm's body in `.boxed()`, emit the
   `match` verbatim
4. **Any other `Expr`** → plain widget, emit `ChildPush::push_into`
5. **Parse fails** → compile error "could not parse statement in
   builder block"
6. **Leading `let`** (statement that fails `syn::Expr` parse because
   it's a statement, not an expression) → compile error "let bindings
   are not allowed inside a builder block; compute outside"

`syn` is the main reason for the proc-macro dependency — hand-rolling
`if`/`else` and `match` parsing is error-prone.

### Error reporting

All macro errors use `syn::Error::new(span, message).to_compile_error()`,
which produces a compiler error with a span pointing at the offending
token.

### What the macro does NOT do

- **No type checking.** A non-widget expression fails at Rust's
  type-checker stage (missing `ChildPush` impl), not at macro
  expansion. `syn` has no type info.
- **No name resolution.** `Text::new`, `build_conversation_row`, etc.
  are whatever's in scope at the call site.
- **No custom attributes.** `column!`/`row!` take only the block — no
  `#[key = ...]` or `#[layout = ...]`. (Future work.)

### Crate-name handling

Proc-macros can't use `$crate` (a `macro_rules!` feature). The macro
hardcodes `::vexo::` paths. If a downstream user renames the `vexo`
dependency, the generated paths break. This matches ecosystem norm
(e.g., `serde::Serialize` paths assume the crate is named `serde`).
Documented limitation; no runtime fix planned.

## The `view_builder` Helper API

### Signatures & behavior

```rust
// vexo/src/view_builder.rs
use crate::widgets::{ChildPush, Widget};
use std::boxed::Box;

/// Identity for the block's collected children. Exists as the
/// `buildBlock` vocabulary entry point; the macro builds the Vec inline
/// via `ChildPush::push_into`, so this fn is called only when a user
/// invokes the builder API by hand.
pub fn build_block(children: Vec<Box<dyn Widget>>) -> Vec<Box<dyn Widget>> {
    children
}

/// `if cond { body }` (no else). None -> renders nothing.
/// Returns a Vec (0 or 1 elements) so it flattens into the parent
/// via `ChildPush for Vec`.
pub fn build_optional(child: Option<Box<dyn Widget>>) -> Vec<Box<dyn Widget>> {
    match child {
        Some(c) => vec![c],
        None => vec![],
    }
}

/// `if cond { a } else { b }`. Both arms already erased to
/// Box<dyn Widget>, so this is identity. Kept for vocabulary symmetry.
pub fn build_either(child: Box<dyn Widget>) -> Box<dyn Widget> {
    child
}

/// `for x in xs { body }`. Collects all iterations into a Vec, which
/// then flattens into the parent via `ChildPush for Vec`.
pub fn build_array(children: Vec<Box<dyn Widget>>) -> Vec<Box<dyn Widget>> {
    children
}
```

### Why `Vec<Box<dyn Widget>>` as the splice type

`build_optional` and `build_array` return `Vec`, not a single `Box`.
This is deliberate:

- `build_optional` must represent "zero children" (false branch). A
  single `Box<dyn Widget>` can't be empty without a sentinel. `Vec`
  lets the false-branch produce `vec![]`, flattening to zero children.
  Matches Swift's `buildOptional` returning an empty view.
- `build_array` represents "N children." A single `Box` would require
  a "list widget" wrapper (extra tree node — Approach 3, rejected).
  `Vec` and flattening keep the tree flat.

The cost: `ChildPush` must accept `Vec<Box<dyn Widget>>`.

### `ChildPush` extension

```rust
// vexo/src/widgets/container.rs — ADD to existing trait impls

impl ChildPush for Vec<Box<dyn Widget>> {
    fn push_into(self, children: &mut Vec<Box<dyn Widget>>) {
        children.extend(self);
    }
}
```

This is the **only** change to existing code outside the new
crate/module. Additive — the existing `W: Widget` and
`Option<Box<dyn Widget>>` impls are untouched.

### Dispatch table

| Statement | Helper called | Helper returns | `ChildPush` impl that flattens it |
|---|---|---|---|
| `if c { body }` (no else) | `build_optional` | `Vec` (0 or 1) | `Vec` impl (NEW) |
| `if c { a } else { b }` | `build_either` | `Box<dyn Widget>` | `W: Widget` blanket (via `Box<dyn Widget>: Widget`) |
| `for x in xs { body }` | `build_array` | `Vec` (0..N) | `Vec` impl (NEW) |
| `match e { arms }` | (none) | `Box<dyn Widget>` (one arm) | `W: Widget` blanket |
| plain widget | (none) | the widget itself | `W: Widget` blanket |

### Why `build_block` / `build_either` exist despite being no-ops

- **Vocabulary completeness** — Swift's `ViewBuilder` has `buildBlock`
  and `buildEither`; having the full set makes the API
  self-documenting and recognizable to Swift devs.
- **Hand-rolled use** — a user can call
  `view_builder::build_block(children![A, B, C])` without the macro.
- **Future hook** — if we add branch-tracing (debug logging of which
  branch rendered), `build_either` is the existing hook.
- **Macro output** — the macro emits `build_either(...)` for `if/else`
  to document intent in expanded code.

### What's NOT in the helper API

- **`build_final_result`** — Swift uses this to wrap the final value.
  Vexo's macro emits `MultiChild::new(vec, layout)` directly; no
  separate finalization step. A no-op wrapper.
- **`build_limit`** — Swift caps `buildBlock` at 10 children. Vexo
  uses a `Vec`; no arity limit.
- **`build_expression`** — Swift uses this to coerce expressions
  (e.g., `String` -> `Text`). Vexo doesn't do implicit coercion.
- **A `ViewBuilder` trait** — the helpers are free functions. A trait
  would add indirection without benefit in a monomorphic-dispatch
  setting.

## Testing Strategy

### Layer 1: Macro expansion tests (UI tests via `trybuild`)

**Location:** `vexo_macros/tests/ui/`

**Compile-pass tests** (`*_passes.rs`) — verify the macro accepts valid
input and expands to something that type-checks. Cases:
- `column_basic_passes` — basic column, basic row, semicolon
  separators, trailing comma, empty block, nested builders
- `conditionals_passes` — `if` without else, `if` with else,
  `match` expr, mixed children (plain + if + for + match)

**Compile-fail tests** (`*_fails.rs` + `.stderr` snapshots) — verify
the macro rejects invalid input with good errors:
- `let_binding_fails` — `column! { let x = 42; ... }` -> "let
  bindings are not allowed inside a builder block; compute outside"
- `mixed_separators_fails` — `column! { A, B; C }` -> "mixing `,`
  and `;` is not allowed"
- `non_widget_fails` — `column! { 42 }` -> type error (missing
  `ChildPush` impl)
- `unparseable_fails` — malformed input -> "could not parse statement
  in builder block"

**trybuild workflow:** First run with `TRYBUILD=overwrite` to generate
`.stderr` snapshots, review, commit. Subsequent runs compare. Re-run
with `TRYBUILD=overwrite` when error messages change intentionally.

### Layer 2: Behavioral tests (runtime)

**Location:** `vexo/tests/builder_macros.rs`

Verify the expanded code produces the correct widget tree. Cases:
- `column_produces_multichild_with_column_layout` — 2 children,
  column layout
- `row_produces_multichild_with_row_layout` — 2 children, row layout
- `empty_column_has_zero_children`
- `if_without_else_false_renders_nothing` — 1 child (only the
  unconditional one)
- `if_without_else_true_renders_one` — 2 children
- `if_with_else_renders_exactly_one`
- `for_loop_renders_all_iterations` — 4 children from a 4-item vec
- `for_loop_empty_renders_nothing`
- `match_renders_taken_arm` — 1 child
- `nested_builders_produce_correct_child_count` — outer 2, inner 2
- `intermixed_control_flow` — 1 + 1 + 2 = 4 children
- `semicolon_separators_match_comma` — same count either way

**Accessor:** tests `use vexo::widgets::Widget;` and call `.children()`
on the `MultiChild` returned by `column!`/`row!`. `MultiChild` has a
`pub fn children(&self) -> &[Box<dyn Widget>]` inherent method
(`vexo/src/widgets/multi_child.rs:80`), which Rust resolves before the
trait method. No downcast needed — `column!` returns `MultiChild`
directly.

### Layer 3: Sample migration tests (in `shared_app`)

The "sample migration" scope: pick 3 representative call sites, convert
to `column!`/`row!`, verify the app still builds and behaves.

1. **`shared_app/src/app.rs:87-95`** — tab bar cell. Basic `column!`
   with 2 children (icon + label). Tests the simplest case.
2. **`shared_app/src/widgets/titled_container.rs:35-43`** — titled
   container. Basic `column!` with 3 children (header + hairline +
   flex-fill child). Tests a clean 3-child column.
3. **`shared_app/src/chats/conversation_list.rs:53-71` + `:161-173`**
   — conversation list. Two sites: the dynamic loop (lines 53-71)
   becomes `for` inside `column!`; the row (lines 161-173) becomes
   `row!`. Tests `for` and `row!` in production code.

Verification:
- `cargo build -p shared_app` passes (compile-time)
- Existing `shared_app` tests pass
- **Manual smoke test** — user runs `cargo run -p desktop_demo` to
  confirm the migrated screens render identically. Per CLAUDE.md, the
  assistant never runs the GUI; the user does.

### Test inventory

| Layer | Location | What it tests | Count |
|---|---|---|---|
| Macro UI (pass) | `vexo_macros/tests/ui/*_passes.rs` | Macro accepts valid syntax | ~11 cases |
| Macro UI (fail) | `vexo_macros/tests/ui/*_fails.rs` + `.stderr` | Macro rejects invalid syntax | ~4 cases |
| Behavioral | `vexo/tests/builder_macros.rs` | Expanded code produces correct tree | ~12 cases |
| Sample migration | `shared_app/src/...` (3 sites) | Macro works in real app code | 3 conversions |

### New dev-dependencies

- `vexo_macros/Cargo.toml`: `[dev-dependencies] trybuild = "1"` and
  `vexo = { path = "../vexo" }` (dev-dep, for UI test files)
- `vexo/Cargo.toml`: no new dev-deps for behavioral tests (use only
  `vexo` itself)

### Acknowledged gaps

- **Layout correctness** (e.g., "column actually lays out vertically")
  — out of scope; the macro emits `Layout::column()`/`Layout::row()`
  which are already tested in `vexo/src/layout/`. The macro doesn't
  touch layout logic.
- **Reconciliation behavior** — the expanded `MultiChild` reconciles
  identically to a hand-written `MultiChild::new(children![...],
  Layout::column())` because it's the same code. No new
  reconciliation paths. Existing pipeline tests cover this.
- **Performance** — no benchmark. The macro generates the same `Vec` +
  `MultiChild` as the hand-written form; helpers are identity fns
  likely inlined.
- **Render output** — not tested at pixel level. Visual smoke test
  (user runs demo) is the verification.

## Rollout Plan

### Implementation order

```
Step 1: vexo_macros crate scaffold
   │   (workspace wiring, empty macro stubs, cargo build passes)
   ▼
Step 2: view_builder module + ChildPush extension
   │   (vexo/src/view_builder.rs, ChildPush for Vec, unit tests)
   ▼
Step 3: column!/row! proc-macro implementation
   │   (syn-based parser, codegen for all statement forms, trybuild UI tests)
   ▼
Step 4: re-exports in vexo/src/lib.rs
   │   (pub use vexo_macros::{column, row}; pub use view_builder::{...})
   ▼
Step 5: behavioral tests
   │   (vexo/tests/builder_macros.rs — tree-shape assertions)
   ▼
Step 6: sample migration in shared_app
       (3 call sites: app.rs tab bar cell, conversation_list.rs row + loop,
        titled_container.rs)
```

**Why this order:**
- Step 2 before Step 3: the macro generates calls to `view_builder`
  functions; they must exist first or the expanded code won't compile.
- Step 4 after Step 3: re-exports can't reference macros that don't
  exist yet.
- Step 5 after Step 4: behavioral tests `use vexo::{column, row}` —
  need the re-exports.
- Step 6 last: migration is the proof-of-concept, not a dependency
  for the feature itself.

### Step details

**Step 1 — Scaffold** (small)
- Create `vexo_macros/` with `Cargo.toml` (`proc-macro = true`; deps:
  `syn = "2"`, `quote = "1"`, `proc-macro2 = "1"`)
- Add `vexo_macros` to workspace `members` in root `Cargo.toml`
- Add `vexo_macros = { path = "../vexo_macros" }` to `vexo/Cargo.toml`
- Empty `src/lib.rs` with stub `#[proc_macro]` fns for `column`/`row`
  that panic ("not yet implemented")
- Verify: `cargo build -p vexo_macros && cargo build -p vexo`

**Step 2 — view_builder + ChildPush** (small)
- Create `vexo/src/view_builder.rs` with the four functions
- Add `impl ChildPush for Vec<Box<dyn Widget>>` to
  `vexo/src/widgets/container.rs`
- Add `pub mod view_builder;` and `pub use view_builder::{...}` to
  `vexo/src/lib.rs`
- Unit tests in `view_builder.rs`
- Verify: `cargo test -p vexo --lib view_builder` + `cargo test -p vexo
  --lib container`

**Step 3 — Macro implementation** (the bulk)
- Implement the syn-based parser: brace-aware splitting, separator
  uniformity check, `syn::Expr` classification
- Implement codegen for each statement form: plain, `if`-no-else,
  `if`-else, `for`, `match`, `let`-error, unparseable-error
- Add `trybuild` UI tests — generate snapshots with
  `TRYBUILD=overwrite`
- Verify: `cargo test -p vexo_macros` (UI tests pass) + `cargo expand`
  manual spot-check

**Step 4 — Re-exports** (trivial)
- `pub use vexo_macros::{column, row};` in `vexo/src/lib.rs`
- Verify: `cargo build -p vexo`

**Step 5 — Behavioral tests** (small)
- Create `vexo/tests/builder_macros.rs`
- Verify: `cargo test -p vexo --test builder_macros`

**Step 6 — Sample migration** (small, but needs user to smoke-test)
- Convert `shared_app/src/app.rs:87-95` (tab bar cell) — basic `column!`
- Convert `shared_app/src/widgets/titled_container.rs:35-43` (titled
  container) — basic `column!`
- Convert `shared_app/src/chats/conversation_list.rs:53-71` (dynamic
  loop) + `:161-173` (row) — `for` in `column!`/`row!`
- Verify: `cargo build -p shared_app` + ask user to run
  `cargo run -p desktop_demo` for visual smoke test

## Open Questions

Decisions that don't block the design but should be settled in the
implementation plan:

1. **`build_either` wrapping in macro output.** The design says the
   macro emits `build_either(...)` for `if/else`. During implementation,
   decide whether wrapping adds clarity or just noise in `cargo expand`
   output. Tentative: emit it for vocabulary consistency; revisit if
   expansion looks cluttered.

2. **`trybuild` version pinning.** `trybuild` updates can change
   `.stderr` snapshot formatting. Pin to `trybuild = "1"` and re-run
   `TRYBUILD=overwrite` on bumps. Minor maintenance cost.

3. **`match` arm with block body.** `State::X => { let s = format!(..);
   Text::new(&s) }` — the block's trailing expression is wrapped in
   `.boxed()`. Confirm `syn` parses this as `Expr::Block` and `.boxed()`
   applies to the trailing expr, not the block-as-unit. Verify during
   implementation with a trybuild test case.

4. **Error message quality for non-widget expressions.** The
   `let`-binding and mixed-separator errors are macro-emitted (good
   spans). The non-widget-expression error is Rust's type checker
   (span points at the expression, message is "trait `ChildPush` is
   not satisfied"). Acceptable. Custom trait error messages are out of
   scope (YAGNI).

## Success Criteria

The feature is done when:
1. `cargo build` and `cargo test` pass across all crates.
2. `column!`/`row!` macros accept plain widgets, `if` (with/without
   else), `for`, and `match`.
3. `let` bindings and mixed separators are rejected with clear errors.
4. 3 call sites in `shared_app` use the new macros and the app renders
   identically (user-verified).
5. `view_builder` helpers are public and unit-tested.
6. `trybuild` UI tests snapshot the expected errors.

## Out of Scope (Deferred)

- **`stack!`/`grid!`** — only column/row. Can be added later following
  the same pattern (different layout arg + the dedicated `Stack`/`Grid`
  widgets instead of `MultiChild`).
- **Postfix modifier chaining** (`.padding().background().onTapGesture()`
  preserving generic type) — the "Full SwiftUI ergonomics" option
  declined. Existing `Widget` trait modifiers and `WithLayout`/
  `DecoratedBox` wrappers remain.
- **`#[view_builder]` attribute on functions/closures** — natural
  follow-up, separate work.
- **`while`/`loop` support** — rare in declarative UI. Add only if a
  real need arises.
- **Type-preserving builder (2b)** — the type-erased design (2a) is
  final; no tuple-impl-Widget machinery.
- **Full migration of all `MultiChild::new(...)` call sites.** Only 3
  sample sites migrate.
- **Crate-rename support.** `::vexo::` paths are hardcoded.
