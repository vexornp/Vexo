# View Builder (SwiftUI-style Result Builder) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `column!`/`row!` proc-macros and a `view_builder` helper module that let Vexo users compose widget trees with Swift-like syntax, supporting `if`/`else`, `for`, and `match` inside the block.

**Architecture:** A new `vexo/vexo_macros/` proc-macro crate (following the `vexo/component_state_derive/` precedent) parses the block with `syn`, classifies each statement (plain widget / `if` / `for` / `match` / `let`), and emits code that pushes children into a `Vec<Box<dyn Widget>>` via the existing `ChildPush` trait, then wraps in `MultiChild::new(vec, Layout::column()/row())`. A new `vexo/src/view_builder.rs` module provides type-erased helper functions (`build_block`, `build_optional`, `build_either`, `build_array`) mirroring Swift's `ViewBuilder` vocabulary; control-flow statements route through these helpers, which return `Vec<Box<dyn Widget>>` and flatten via a new `ChildPush for Vec` impl.

**Tech Stack:** Rust 2021, `syn = "2"`, `quote = "1"`, `proc-macro2 = "1"`, `trybuild = "1"` (dev-dep for UI tests).

## Spec Refinement

The spec (§ Architecture) specified `vexo_macros/` as a top-level workspace member. The codebase precedent is `vexo/component_state_derive/` — a proc-macro crate living as a **subdirectory of `vexo/`**, referenced via `vexo_macros = { path = "vexo_macros" }` in `vexo/Cargo.toml`, and **not** listed as a workspace member. This plan follows the precedent: the crate lives at `vexo/vexo_macros/`. The architectural intent (separate proc-macro crate, path dep) is preserved; only the filesystem location changes for consistency with the existing `component_state_derive` crate.

## Migration Scope Refinement (Design Gap Found During Planning)

The spec's success criterion #4 calls for migrating 3 representative call sites. During planning, all 3 candidate sites were examined:

1. `shared_app/src/app.rs:87-95` (tab bar cell) — uses `Layout::column().gap(2.0).align(AlignItems::Center)`. **Custom modifiers — cannot migrate** (the macro bakes in plain `Layout::column()`; wrapping in `WithLayout` would add nesting and is worse than the original).
2. `shared_app/src/widgets/titled_container.rs:35-43` — uses `Layout::column().width_percent(1.0).height_percent(1.0)`. **Custom modifiers — cannot migrate.**
3. `shared_app/src/chats/conversation_list.rs:53-71` (dynamic loop) — uses plain `Layout::column()`. **CAN migrate cleanly** — the `for` loop in `column!` replaces the imperative `list = list.push(row)` loop.

Only site #3 uses a plain layout that the macro supports. This plan migrates site #3 (the highest-value site anyway — it demonstrates the `for` loop, the main ergonomic win). Sites #1 and #2 stay on the existing `MultiChild::new(children![...], layout)` form, which remains public API.

**Design gap surfaced:** The macros as scoped (plain `Layout::column()`/`row()` only) cannot migrate sites that customize the layout — which is most real-world sites. A follow-up spec could extend the macro to accept an optional layout expression (e.g. `column!(Layout::column().gap(2.0)) { A, B }`). That is **out of scope** for this plan; it would be a separate spec → plan → implementation cycle. The success criterion is met by migrating site #3 (the `for`-loop case, which is the primary ergonomic motivation for the feature).

## Global Constraints

- Proc-macro crate must be its own crate with `proc-macro = true` in `[lib]`.
- Proc-macro crate lives at `vexo/vexo_macros/` (following `vexo/component_state_derive/` precedent), referenced as `vexo_macros = { path = "vexo_macros" }` in `vexo/Cargo.toml`, NOT a workspace member.
- Generated code uses absolute `::vexo::...` paths (proc-macros can't use `$crate`). Crate rename is unsupported (documented limitation).
- `children![]` and `MultiChild::new(children, layout)` remain public and unchanged — additive only.
- `syn = "2"`, `quote = "1"`, `proc-macro2 = "1"` (match `vexo/component_state_derive/Cargo.toml` versions exactly).
- `trybuild = "1"` as a dev-dependency in `vexo_macros/Cargo.toml`.
- All macro errors use `syn::Error::new(span, msg).to_compile_error()` for span-accurate diagnostics.
- Per CLAUDE.md: never run `cargo run -p desktop_demo` — ask the user to run it for visual smoke tests.

## File Structure

| File | Responsibility |
|---|---|
| `vexo/vexo_macros/Cargo.toml` (CREATE) | Proc-macro crate manifest; deps: syn, quote, proc-macro2; dev-deps: trybuild, vexo |
| `vexo/vexo_macros/src/lib.rs` (CREATE) | `column!`/`row!` proc-macro entry points; parser + codegen |
| `vexo/vexo_macros/tests/ui/` (CREATE) | `trybuild` UI tests (compile-pass + compile-fail with `.stderr`) |
| `vexo/src/view_builder.rs` (CREATE) | Type-erased helper API: `build_block`, `build_optional`, `build_either`, `build_array` + unit tests |
| `vexo/src/widgets/container.rs` (MODIFY) | Add `impl ChildPush for Vec<Box<dyn Widget>>` |
| `vexo/Cargo.toml` (MODIFY) | Add `vexo_macros = { path = "vexo_macros" }` dep |
| `vexo/src/lib.rs` (MODIFY) | Add `pub mod view_builder;`, re-export helpers, `pub use vexo_macros::{column, row};` |
| `vexo/tests/builder_macros.rs` (CREATE) | Behavioral integration tests for `column!`/`row!` |
| `shared_app/src/chats/conversation_list.rs` (MODIFY) | Migrate dynamic loop to `for` inside `column!` |

---

## Task 1: Scaffold `vexo_macros` crate

**Files:**
- Create: `vexo/vexo_macros/Cargo.toml`
- Create: `vexo/vexo_macros/src/lib.rs`
- Modify: `vexo/Cargo.toml` (add dep)
- Modify: `vexo/src/lib.rs` (add re-export stubs)

**Interfaces:**
- Produces: `vexo_macros::column` and `vexo_macros::row` proc-macro functions (stubbed to panic), re-exported as `vexo::column!` / `vexo::row!`.

- [ ] **Step 1: Create `vexo/vexo_macros/Cargo.toml`**

```toml
[package]
name = "vexo_macros"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true

[dependencies]
syn = "2"
quote = "1"
proc-macro2 = "1"

[dev-dependencies]
trybuild = "1"
vexo = { path = ".." }
```

- [ ] **Step 2: Create `vexo/vexo_macros/src/lib.rs` with stub macros**

```rust
//! Proc-macros for SwiftUI-style result-builder widget composition.
//!
//! See `docs/superpowers/specs/2026-07-31-view-builder-design.md`.
//!
//! NOTE: Generated code uses absolute `::vexo::...` paths. Renaming the
//! `vexo` dependency in a downstream crate is unsupported.

use proc_macro::TokenStream;

/// Build a `MultiChild` with `Layout::column()`. See spec § Macro Syntax.
#[proc_macro]
pub fn column(_input: TokenStream) -> TokenStream {
    unimplemented!("column! macro — implemented in Task 3")
}

/// Build a `MultiChild` with `Layout::row()`. See spec § Macro Syntax.
#[proc_macro]
pub fn row(_input: TokenStream) -> TokenStream {
    unimplemented!("row! macro — implemented in Task 3")
}
```

- [ ] **Step 3: Add `vexo_macros` dep to `vexo/Cargo.toml`**

In `vexo/Cargo.toml`, after the `component_state_derive` line (line 21), add:

```toml
component_state_derive = { path = "component_state_derive" }
vexo_macros = { path = "vexo_macros" }
```

- [ ] **Step 4: Add re-exports to `vexo/src/lib.rs`**

In `vexo/src/lib.rs`, after line 41 (`pub use component_state_derive::ComponentState;`), add:

```rust
pub use vexo_macros::{column, row};
```

- [ ] **Step 5: Verify the scaffold builds**

Run: `cargo build -p vexo`
Expected: builds successfully. The stub macros are re-exported but not yet called, so they compile. (Calling `column!{}` would panic at macro-expansion time, but nothing calls them yet.)

- [ ] **Step 6: Commit**

```bash
git add vexo/vexo_macros/ vexo/Cargo.toml vexo/src/lib.rs
git commit -m "feat: scaffold vexo_macros proc-macro crate with column!/row! stubs"
```

---

## Task 2: `view_builder` module + `ChildPush for Vec`

**Files:**
- Create: `vexo/src/view_builder.rs`
- Modify: `vexo/src/widgets/container.rs` (add `Vec` impl)
- Modify: `vexo/src/lib.rs` (declare module + re-export helpers)

**Interfaces:**
- Produces:
  - `vexo::view_builder::build_block(Vec<Box<dyn Widget>>) -> Vec<Box<dyn Widget>>`
  - `vexo::view_builder::build_optional(Option<Box<dyn Widget>>) -> Vec<Box<dyn Widget>>`
  - `vexo::view_builder::build_either(Box<dyn Widget>) -> Box<dyn Widget>`
  - `vexo::view_builder::build_array(Vec<Box<dyn Widget>>) -> Vec<Box<dyn Widget>>`
  - `ChildPush for Vec<Box<dyn Widget>>` (flattens via `extend`)
- Consumes: `vexo::widgets::{ChildPush, Widget}` (existing)

- [ ] **Step 1: Write the failing test for `view_builder` helpers**

Create `vexo/src/view_builder.rs`:

```rust
//! Type-erased result-builder helpers. Mirrors Swift's `ViewBuilder` vocabulary.
//!
//! Each function is a thin adapter over `Vec<Box<dyn Widget>>` + `ChildPush`.
//! The `column!`/`row!` macros (in `vexo_macros`) emit calls to these helpers
//! for control-flow statements (`if`, `for`); plain widgets go straight through
//! `ChildPush::push_into`.

use crate::widgets::{ChildPush, Widget};

/// Identity for the block's collected children. The macro builds the `Vec`
/// inline via `ChildPush::push_into`, so this fn is called only when a user
//! invokes the builder API by hand. Mirrors Swift's `buildBlock`.
pub fn build_block(children: Vec<Box<dyn Widget>>) -> Vec<Box<dyn Widget>> {
    children
}

/// `if cond { body }` (no else). `None` renders nothing. Returns a `Vec`
/// (0 or 1 elements) so it flattens into the parent via `ChildPush for Vec`.
/// Mirrors Swift's `buildOptional`.
pub fn build_optional(child: Option<Box<dyn Widget>>) -> Vec<Box<dyn Widget>> {
    match child {
        Some(c) => vec![c],
        None => vec![],
    }
}

/// `if cond { a } else { b }`. Both arms already erased to `Box<dyn Widget>`,
/// so this is identity. Kept for vocabulary symmetry with Swift's
/// `buildEither`; documents intent. The macro wraps `if/else` in this call.
pub fn build_either(child: Box<dyn Widget>) -> Box<dyn Widget> {
    child
}

/// `for x in xs { body }`. Collects all iterations into a `Vec`, which then
/// flattens into the parent via `ChildPush for Vec`. Mirrors Swift's
/// `buildArray`.
pub fn build_array(children: Vec<Box<dyn Widget>>) -> Vec<Box<dyn Widget>> {
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[test]
    fn build_block_is_identity() {
        let v = build_block(vec![Text::new("a").boxed(), Text::new("b").boxed()]);
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn build_optional_some_yields_one() {
        let v = build_optional(Some(Text::new("x").boxed()));
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn build_optional_none_yields_zero() {
        let v = build_optional::<Box<dyn Widget>>(None);
        assert_eq!(v.len(), 0);
    }

    #[test]
    fn build_either_is_identity() {
        let w: Box<dyn Widget> = Text::new("x").boxed();
        let out = build_either(w);
        assert!(out.as_any().is::<Text>());
    }

    #[test]
    fn build_array_passes_through() {
        let v = build_array(vec![Text::new("a").boxed(), Text::new("b").boxed()]);
        assert_eq!(v.len(), 2);
    }
}
```

- [ ] **Step 2: Write the failing test for `ChildPush for Vec`**

Append to `vexo/src/widgets/container.rs` (after the existing `Option` impl, end of file):

```rust
/// Splice a pre-built `Vec<Box<dyn Widget>>` into a container. Used by
/// `view_builder::build_optional` / `build_array` to flatten control-flow
/// results (0 or N children) into the parent without injecting extra tree
/// nodes.
impl ChildPush for Vec<Box<dyn Widget>> {
    fn push_into(self, children: &mut Vec<Box<dyn Widget>>) {
        children.extend(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Text;

    #[test]
    fn vec_push_into_extends_destination() {
        let mut dst: Vec<Box<dyn Widget>> = vec![Text::new("a").boxed()];
        let src: Vec<Box<dyn Widget>> = vec![Text::new("b").boxed(), Text::new("c").boxed()];
        src.push_into(&mut dst);
        assert_eq!(dst.len(), 3);
    }

    #[test]
    fn vec_push_into_empty_is_noop() {
        let mut dst: Vec<Box<dyn Widget>> = vec![Text::new("a").boxed()];
        let src: Vec<Box<dyn Widget>> = vec![];
        src.push_into(&mut dst);
        assert_eq!(dst.len(), 1);
    }
}
```

- [ ] **Step 3: Declare module + re-export helpers in `vexo/src/lib.rs`**

In `vexo/src/lib.rs`, after line 41 (`pub use component_state_derive::ComponentState;`), add:

```rust
pub use vexo_macros::{column, row};
pub mod view_builder;
pub use view_builder::{build_array, build_block, build_either, build_optional};
```

(The `pub use vexo_macros::{column, row};` line was added in Task 1 Step 4 — if it's already there, don't duplicate it. The other three lines are new.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vexo --lib view_builder`
Expected: PASS (5 tests in view_builder, 2 tests in container)

Run: `cargo test -p vexo --lib container`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add vexo/src/view_builder.rs vexo/src/widgets/container.rs vexo/src/lib.rs
git commit -m "feat: add view_builder helper module and ChildPush for Vec"
```

---

## Task 3: Implement `column!`/`row!` for plain widgets

This task implements the core macro: parsing comma/semicolon-separated plain widget expressions and emitting the accumulator skeleton. Control flow (`if`/`for`/`match`) comes in later tasks.

**Files:**
- Modify: `vexo/vexo_macros/src/lib.rs` (replace stubs with real impl)
- Create: `vexo/tests/builder_macros.rs` (behavioral tests)
- Create: `vexo/vexo_macros/tests/ui/column_basic_passes.rs` (compile-pass)

**Interfaces:**
- Produces: `vexo::column! { A, B, ... }` and `vexo::row! { A, B, ... }` expanding to `MultiChild` (plain widget expressions only in this task).
- Consumes: `::vexo::widgets::{ChildPush, Widget, MultiChild}`, `::vexo::layout::Layout` (the `view_builder` helpers are consumed in Tasks 4-5).

- [ ] **Step 1: Write the failing behavioral test**

Create `vexo/tests/builder_macros.rs`:

```rust
//! Behavioral tests for the `column!` / `row!` builder macros.
//!
//! These verify the *runtime* shape of the produced widget tree (child counts,
//! layout). Compile-pass/compile-fail cases live in `vexo_macros/tests/ui/`.

use vexo::{column, row};
use vexo::widgets::{MultiChild, Widget};

#[test]
fn column_produces_multichild_with_two_children() {
    let w: MultiChild = column! {
        vexo::Text::new("a"),
        vexo::Text::new("b"),
    };
    assert_eq!(w.children().len(), 2);
}

#[test]
fn row_produces_multichild_with_two_children() {
    let w: MultiChild = row! {
        vexo::Text::new("a"),
        vexo::Text::new("b"),
    };
    assert_eq!(w.children().len(), 2);
}

#[test]
fn empty_column_has_zero_children() {
    let w: MultiChild = column! {};
    assert_eq!(w.children().len(), 0);
}

#[test]
fn single_child_no_trailing_separator() {
    let w: MultiChild = column! { vexo::Text::new("only") };
    assert_eq!(w.children().len(), 1);
}

#[test]
fn trailing_comma_allowed() {
    let w: MultiChild = column! {
        vexo::Text::new("a"),
        vexo::Text::new("b"),
    };
    assert_eq!(w.children().len(), 2);
}

#[test]
fn semicolon_separators_match_comma() {
    let w_comma: MultiChild = column! { vexo::Text::new("a"), vexo::Text::new("b") };
    let w_semi: MultiChild = column! { vexo::Text::new("a"); vexo::Text::new("b") };
    assert_eq!(w_comma.children().len(), w_semi.children().len());
}

#[test]
fn nested_builders_produce_correct_child_count() {
    let w: MultiChild = column! {
        row! {
            vexo::Text::new("a"),
            vexo::Text::new("b"),
        },
        vexo::Text::new("c"),
    };
    assert_eq!(w.children().len(), 2);
    let inner = &w.children()[0];
    assert_eq!(inner.children().len(), 2);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p vexo --test builder_macros`
Expected: FAIL — the stub macros panic at expansion time (`unimplemented!("column! macro — implemented in Task 3")`). The error is a compile-time panic from the proc-macro.

- [ ] **Step 3: Implement the macro — parser + codegen for plain widgets**

Replace the entire contents of `vexo/vexo_macros/src/lib.rs` with:

```rust
//! Proc-macros for SwiftUI-style result-builder widget composition.
//!
//! See `docs/superpowers/specs/2026-07-31-view-builder-design.md`.
//!
//! NOTE: Generated code uses absolute `::vexo::...` paths. Renaming the
//! `vexo` dependency in a downstream crate is unsupported.

use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::quote;

/// Entry shared by `column!` and `row!`. `layout_ctor` is either
/// `Layout::column` or `Layout::row`.
fn build_container(input: TokenStream, layout_ctor: &str) -> TokenStream {
    let tokens: proc_macro2::TokenStream = input.into();
    let statements = match split_statements(tokens.clone()) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error().into(),
    };

    let layout_path = match layout_ctor {
        "column" => quote! { ::vexo::layout::Layout::column() },
        "row" => quote! { ::vexo::layout::Layout::row() },
        _ => unreachable!(),
    };

    let mut push_calls = Vec::new();
    for stmt in statements {
        match expand_statement(&stmt) {
            Ok(tokens) => push_calls.push(tokens),
            Err(e) => return e.to_compile_error().into(),
        }
    }

    let expanded = quote! {{
        let mut __vexo_children: ::std::vec::Vec<::std::boxed::Box<dyn ::vexo::widgets::Widget>>
            = ::std::vec::Vec::new();
        #(#push_calls)*
        ::vexo::widgets::MultiChild::new(__vexo_children, #layout_path)
    }};
    expanded.into()
}

/// Split a token stream into statements at top-level `,` or `;`.
/// Rejects mixing `,` and `;`.
fn split_statements(
    tokens: proc_macro2::TokenStream,
) -> syn::Result<Vec<proc_macro2::TokenStream>> {
    let mut statements: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut current: Vec<TokenTree> = Vec::new();
    let mut seen_comma = false;
    let mut seen_semi = false;
    let mut first_mixed_span: Option<proc_macro2::Span> = None;

    for tt in tokens {
        match &tt {
            TokenTree::Punct(p) if p.as_char() == ',' || p.as_char() == ';' => {
                if p.as_char() == ',' {
                    seen_comma = true;
                    if seen_semi && first_mixed_span.is_none() {
                        first_mixed_span = Some(p.span());
                    }
                } else {
                    seen_semi = true;
                    if seen_comma && first_mixed_span.is_none() {
                        first_mixed_span = Some(p.span());
                    }
                }
                if !current.is_empty() {
                    statements.push(current.into_iter().collect());
                    current = Vec::new();
                }
            }
            _ => current.push(tt),
        }
    }
    if !current.is_empty() {
        statements.push(current.into_iter().collect());
    }

    if let Some(span) = first_mixed_span {
        return Err(syn::Error::new(
            span,
            "mixing `,` and `;` separators is not allowed inside `column!`/`row!`; pick one",
        ));
    }

    Ok(statements)
}

/// Classify a single statement and emit the corresponding `ChildPush::push_into` call.
///
/// In this task (Task 3), only the "plain widget" case is implemented. The
/// `if` / `for` / `match` / `let` cases are added in Tasks 4-7.
fn expand_statement(stmt: &proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    // Try to parse as an expression. If it fails, check for `let` (statement,
    // not expression) and emit a clear error; otherwise reparse-error.
    let expr: syn::Expr = match syn::parse2::<syn::Expr>(stmt.clone()) {
        Ok(e) => e,
        Err(_) => {
            // Check for `let` binding.
            let first_token = stmt.clone().into_iter().next();
            if let Some(TokenTree::Ident(ident)) = first_token {
                if ident == "let" {
                    return Err(syn::Error::new(
                        ident.span(),
                        "let bindings are not allowed inside a builder block; compute outside",
                    ));
                }
            }
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "could not parse statement in builder block",
            ));
        }
    };

    // Plain widget expression.
    Ok(quote! {
        ::vexo::widgets::ChildPush::push_into(#expr, &mut __vexo_children);
    })
}

/// Build a `MultiChild` with `Layout::column()`. See spec § Macro Syntax.
#[proc_macro]
pub fn column(input: TokenStream) -> TokenStream {
    build_container(input, "column")
}

/// Build a `MultiChild` with `Layout::row()`. See spec § Macro Syntax.
#[proc_macro]
pub fn row(input: TokenStream) -> TokenStream {
    build_container(input, "row")
}
```

- [ ] **Step 4: Run the behavioral test to verify it passes**

Run: `cargo test -p vexo --test builder_macros`
Expected: PASS (7 tests)

- [ ] **Step 5: Write the compile-pass UI test**

Create `vexo/vexo_macros/tests/ui/column_basic_passes.rs`:

```rust
// Compile-pass tests for `column!`/`row!` basic syntax.
// trybuild verifies these compile without error.

use vexo::{column, row};
use vexo::Text;

fn basic_column() {
    column! {
        Text::new("a"),
        Text::new("b"),
    };
}

fn basic_row() {
    row! { Text::new("a"), Text::new("b") };
}

fn semicolon_separators() {
    column! {
        Text::new("a");
        Text::new("b");
    };
}

fn trailing_comma() {
    column! { Text::new("a"), Text::new("b"), };
}

fn empty_block() {
    column! {};
}

fn nested() {
    column! {
        row! {
            Text::new("a"),
            Text::new("b"),
        },
        Text::new("c"),
    };
}

fn single_child_no_separator() {
    column! { Text::new("only") };
}

fn main() {}
```

- [ ] **Step 6: Generate trybuild snapshots and verify pass**

Run: `TRYBUILD=overwrite cargo test -p vexo_macros --test ui`
Expected: generates `column_basic_passes.stderr` (empty for pass cases — trybuild writes an empty `.stderr` for passing files, or no `.stderr` at all depending on version). The test passes.

Run: `cargo test -p vexo_macros --test ui`
Expected: PASS (without overwrite, compares against snapshots)

- [ ] **Step 7: Commit**

```bash
git add vexo/vexo_macros/src/lib.rs vexo/tests/builder_macros.rs vexo/vexo_macros/tests/ui/
git commit -m "feat: implement column!/row! macros for plain widget expressions"
```

---

## Task 4: Add `if`/`else` support

**Files:**
- Modify: `vexo/vexo_macros/src/lib.rs` (extend `expand_statement`)
- Modify: `vexo/tests/builder_macros.rs` (add `if` tests)
- Create: `vexo/vexo_macros/tests/ui/conditionals_passes.rs`

**Interfaces:**
- Produces: `if cond { body }` (no else) and `if cond { a } else { b }` support inside `column!`/`row!`.

- [ ] **Step 1: Write the failing behavioral tests**

Append to `vexo/tests/builder_macros.rs`:

```rust
#[test]
fn if_without_else_false_renders_nothing() {
    let cond = false;
    let w: MultiChild = column! {
        vexo::Text::new("always"),
        if cond { vexo::Text::new("maybe") },
    };
    assert_eq!(w.children().len(), 1);
}

#[test]
fn if_without_else_true_renders_one() {
    let cond = true;
    let w: MultiChild = column! {
        vexo::Text::new("always"),
        if cond { vexo::Text::new("maybe") },
    };
    assert_eq!(w.children().len(), 2);
}

#[test]
fn if_with_else_renders_exactly_one() {
    let w: MultiChild = column! {
        if true { vexo::Text::new("a") } else { vexo::Text::new("b") },
    };
    assert_eq!(w.children().len(), 1);
}

#[test]
fn if_with_else_false_takes_else_branch() {
    let w: MultiChild = column! {
        if false { vexo::Text::new("a") } else { vexo::Text::new("b") },
    };
    assert_eq!(w.children().len(), 1);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vexo --test builder_macros`
Expected: FAIL — currently `if cond { ... }` is parsed as a plain `syn::Expr::If` and pushed as a single widget, which fails because an `if` expression isn't a `Widget` (compile error: `ChildPush` not satisfied).

- [ ] **Step 3: Extend `expand_statement` to handle `if`**

In `vexo/vexo_macros/src/lib.rs`, replace the `expand_statement` function's match-on-`expr` block. The current function ends with the "plain widget expression" case. Replace the entire function body after the `let expr = ...` parse with:

```rust
fn expand_statement(stmt: &proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    let expr: syn::Expr = match syn::parse2::<syn::Expr>(stmt.clone()) {
        Ok(e) => e,
        Err(_) => {
            let first_token = stmt.clone().into_iter().next();
            if let Some(TokenTree::Ident(ident)) = first_token {
                if ident == "let" {
                    return Err(syn::Error::new(
                        ident.span(),
                        "let bindings are not allowed inside a builder block; compute outside",
                    ));
                }
            }
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "could not parse statement in builder block",
            ));
        }
    };

    match &expr {
        syn::Expr::If(if_expr) => {
            let cond = &if_expr.cond;
            let then_body = &if_expr.then_branch;
            if let Some((_, else_body)) = &if_expr.else_branch {
                // if cond { a } else { b } -> build_either(if c { a.boxed() } else { b.boxed() })
                Ok(quote! {
                    ::vexo::widgets::ChildPush::push_into(
                        ::vexo::view_builder::build_either(
                            if #cond { (#then_body).boxed() } else { (#else_body).boxed() }
                        ),
                        &mut __vexo_children,
                    );
                })
            } else {
                // if cond { body } (no else) -> build_optional(if c { Some(body.boxed()) } else { None })
                Ok(quote! {
                    ::vexo::widgets::ChildPush::push_into(
                        ::vexo::view_builder::build_optional(
                            if #cond { Some((#then_body).boxed()) } else { None }
                        ),
                        &mut __vexo_children,
                    );
                })
            }
        }
        _ => {
            // Plain widget expression.
            Ok(quote! {
                ::vexo::widgets::ChildPush::push_into(#expr, &mut __vexo_children);
            })
        }
    }
}
```

Note: `(#then_body).boxed()` wraps the block expression in parens so `.boxed()` applies to the block's value (the trailing widget), not the block-as-unit. The same applies to `#else_body`.

- [ ] **Step 4: Run the behavioral tests to verify they pass**

Run: `cargo test -p vexo --test builder_macros`
Expected: PASS (11 tests now)

- [ ] **Step 5: Add compile-pass UI test for conditionals**

Create `vexo/vexo_macros/tests/ui/conditionals_passes.rs`:

```rust
use vexo::{column, row};
use vexo::Text;

fn if_without_else() {
    let cond = true;
    column! {
        Text::new("a"),
        if cond { Text::new("b") },
    };
}

fn if_with_else() {
    let cond = true;
    column! {
        if cond { Text::new("yes") } else { Text::new("no") },
    };
}

fn if_in_row() {
    let cond = false;
    row! {
        Text::new("a"),
        if cond { Text::new("b") } else { Text::new("c") },
    };
}

fn nested_if() {
    let a = true;
    let b = false;
    column! {
        if a {
            if b { Text::new("ab") } else { Text::new("a") }
        } else {
            Text::new("not-a")
        },
    };
}

fn main() {}
```

- [ ] **Step 6: Regenerate trybuild snapshots and verify**

Run: `TRYBUILD=overwrite cargo test -p vexo_macros --test ui`
Run: `cargo test -p vexo_macros --test ui`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add vexo/vexo_macros/src/lib.rs vexo/tests/builder_macros.rs vexo/vexo_macros/tests/ui/conditionals_passes.rs
git commit -m "feat: add if/else support to column!/row! macros"
```

---

## Task 5: Add `for` loop support

**Files:**
- Modify: `vexo/vexo_macros/src/lib.rs` (extend `expand_statement` with `Expr::ForLoop`)
- Modify: `vexo/tests/builder_macros.rs` (add `for` tests)
- Modify: `vexo/vexo_macros/tests/ui/conditionals_passes.rs` (add `for` cases)

**Interfaces:**
- Produces: `for x in xs { body }` support inside `column!`/`row!`, flattening all iterations into the parent.

- [ ] **Step 1: Write the failing behavioral tests**

Append to `vexo/tests/builder_macros.rs`:

```rust
#[test]
fn for_loop_renders_all_iterations() {
    let items = vec!["a", "b", "c", "d"];
    let w: MultiChild = column! {
        for s in &items { vexo::Text::new(s) },
    };
    assert_eq!(w.children().len(), 4);
}

#[test]
fn for_loop_empty_renders_nothing() {
    let items: Vec<&str> = vec![];
    let w: MultiChild = column! {
        for s in &items { vexo::Text::new(s) },
    };
    assert_eq!(w.children().len(), 0);
}

#[test]
fn for_loop_interleaved_with_plain() {
    let items = vec!["x", "y"];
    let w: MultiChild = column! {
        vexo::Text::new("header"),
        for s in &items { vexo::Text::new(s) },
    };
    assert_eq!(w.children().len(), 3);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vexo --test builder_macros`
Expected: FAIL — `for x in xs { body }` parses as `syn::Expr::ForLoop`, which the current `match` arm treats as a plain widget (no `ChildPush` impl for a `for` expression → compile error).

- [ ] **Step 3: Extend `expand_statement` to handle `for`**

In `vexo/vexo_macros/src/lib.rs`, in the `match &expr` block inside `expand_statement`, add a `ForLoop` arm **before** the `_` catch-all:

```rust
        syn::Expr::ForLoop(for_expr) => {
            let pat = &for_expr.pat;
            let expr_iter = &for_expr.expr;
            let body = &for_expr.body;
            Ok(quote! {
                ::vexo::widgets::ChildPush::push_into(
                    ::vexo::view_builder::build_array(
                        (#expr_iter).into_iter().map(|#pat| (#body).boxed()).collect::<::std::vec::Vec<_>>()
                    ),
                    &mut __vexo_children,
                );
            })
        }
```

The full `match` block should now be (in order): `Expr::If`, `Expr::ForLoop`, `_` (plain widget).

- [ ] **Step 4: Run the behavioral tests to verify they pass**

Run: `cargo test -p vexo --test builder_macros`
Expected: PASS (14 tests now)

- [ ] **Step 5: Add `for` cases to the UI compile-pass test**

Append to `vexo/vexo_macros/tests/ui/conditionals_passes.rs` (before `fn main()`):

```rust
fn for_loop() {
    let items = vec!["a", "b", "c"];
    column! {
        for s in &items {
            Text::new(s)
        },
    };
}

fn for_loop_interleaved() {
    let cond = true;
    let items = vec!["x"];
    column! {
        Text::new("header"),
        if cond { Text::new("cond") },
        for s in &items { Text::new(s) },
    };
}
```

- [ ] **Step 6: Regenerate trybuild snapshots and verify**

Run: `TRYBUILD=overwrite cargo test -p vexo_macros --test ui`
Run: `cargo test -p vexo_macros --test ui`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add vexo/vexo_macros/src/lib.rs vexo/tests/builder_macros.rs vexo/vexo_macros/tests/ui/conditionals_passes.rs
git commit -m "feat: add for loop support to column!/row! macros"
```

---

## Task 6: Add `match` support

**Files:**
- Modify: `vexo/vexo_macros/src/lib.rs` (extend `expand_statement` with `Expr::Match`)
- Modify: `vexo/tests/builder_macros.rs` (add `match` test)
- Modify: `vexo/vexo_macros/tests/ui/conditionals_passes.rs` (add `match` case)

**Interfaces:**
- Produces: `match e { arms }` support inside `column!`/`row!`. Each arm body must be a widget expression.

- [ ] **Step 1: Write the failing behavioral test**

Append to `vexo/tests/builder_macros.rs`:

```rust
#[test]
fn match_renders_taken_arm() {
    #[derive(PartialEq)]
    enum S { A, B, C }
    let s = S::B;
    let w: MultiChild = column! {
        match s {
            S::A => vexo::Text::new("a"),
            S::B => vexo::Text::new("b"),
            S::C => vexo::Text::new("c"),
        },
    };
    assert_eq!(w.children().len(), 1);
}

#[test]
fn match_with_guard() {
    #[derive(PartialEq)]
    enum S { Loading, Error(String) }
    let s = S::Error("oops".into());
    let w: MultiChild = column! {
        match s {
            S::Loading => vexo::Text::new("loading"),
            S::Error(msg) if msg.is_empty() => vexo::Text::new("empty error"),
            S::Error(_) => vexo::Text::new("error"),
        },
    };
    assert_eq!(w.children().len(), 1);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p vexo --test builder_macros`
Expected: FAIL — `match` parses as `syn::Expr::Match`, currently falls through to `_` (plain widget) → compile error.

- [ ] **Step 3: Extend `expand_statement` to handle `match`**

In `vexo/vexo_macros/src/lib.rs`, add a `Match` arm in the `match &expr` block (after `ForLoop`, before `_`). The arm wraps each match arm's body in `.boxed()`:

```rust
        syn::Expr::Match(match_expr) => {
            let scrutinee = &match_expr.expr;
            // Rebuild each arm with its body wrapped in `.boxed()`.
            let mut new_arms = Vec::new();
            for arm in &match_expr.arms {
                let pat = &arm.pat;
                let guard = &arm.guard;
                let body = &arm.body;
                let guard_tokens = match guard {
                    Some((if_token, cond)) => quote! { #if_token #cond },
                    None => quote! {},
                };
                new_arms.push(quote! { #pat #guard_tokens => (#body).boxed(), });
            }
            Ok(quote! {
                ::vexo::widgets::ChildPush::push_into(
                    match #scrutinee {
                        #(#new_arms)*
                    },
                    &mut __vexo_children,
                );
            })
        }
```

Note: `(#body).boxed()` wraps the arm body (which may be a block `{ ... }` or a bare expression) in parens so `.boxed()` applies to the value. Patterns and guards pass through unchanged. Trailing commas in each arm are required by Rust syntax.

- [ ] **Step 4: Run the behavioral tests to verify they pass**

Run: `cargo test -p vexo --test builder_macros`
Expected: PASS (16 tests now)

- [ ] **Step 5: Add `match` case to the UI compile-pass test**

Append to `vexo/vexo_macros/tests/ui/conditionals_passes.rs` (before `fn main()`):

```rust
fn match_expr() {
    #[derive(PartialEq)]
    enum S { Loading, Error }
    let s = S::Loading;
    column! {
        match s {
            S::Loading => Text::new("loading"),
            S::Error => Text::new("error"),
        },
    };
}

fn match_with_block_body() {
    let n = 2;
    column! {
        match n {
            1 => { Text::new("one") },
            _ => {
                let s = "other";
                Text::new(s)
            },
        },
    };
}
```

- [ ] **Step 6: Regenerate trybuild snapshots and verify**

Run: `TRYBUILD=overwrite cargo test -p vexo_macros --test ui`
Run: `cargo test -p vexo_macros --test ui`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add vexo/vexo_macros/src/lib.rs vexo/tests/builder_macros.rs vexo/vexo_macros/tests/ui/conditionals_passes.rs
git commit -m "feat: add match support to column!/row! macros"
```

---

## Task 7: Error cases — `let`, mixed separators (compile-fail UI tests)

Tasks 3-6 already implement the error *emission* (the `let` check and mixed-separator check were in Task 3's code). This task adds compile-fail UI tests with `.stderr` snapshots to lock in the error messages.

**Files:**
- Create: `vexo/vexo_macros/tests/ui/let_binding_fails.rs`
- Create: `vexo/vexo_macros/tests/ui/let_binding_fails.stderr`
- Create: `vexo/vexo_macros/tests/ui/mixed_separators_fails.rs`
- Create: `vexo/vexo_macros/tests/ui/mixed_separators_fails.stderr`

**Interfaces:**
- Produces: snapshot-locked error messages for `let` and mixed-separator rejection.

- [ ] **Step 1: Write the `let` compile-fail test**

Create `vexo/vexo_macros/tests/ui/let_binding_fails.rs`:

```rust
use vexo::column;
use vexo::Text;

fn bad() {
    column! {
        let x = 42;
        Text::new("a"),
    };
}

fn main() {}
```

- [ ] **Step 2: Write the mixed-separators compile-fail test**

Create `vexo/vexo_macros/tests/ui/mixed_separators_fails.rs`:

```rust
use vexo::column;
use vexo::Text;

fn bad() {
    column! {
        Text::new("a"),
        Text::new("b");
        Text::new("c"),
    };
}

fn main() {}
```

- [ ] **Step 3: Generate the `.stderr` snapshots**

Run: `TRYBUILD=overwrite cargo test -p vexo_macros --test ui`
Expected: generates `let_binding_fails.stderr` and `mixed_separators_fails.stderr` in `vexo/vexo_macros/tests/ui/`.

- [ ] **Step 4: Review the generated `.stderr` files**

Read `vexo/vexo_macros/tests/ui/let_binding_fails.stderr` — verify it contains `let bindings are not allowed inside a builder block; compute outside`.

Read `vexo/vexo_macros/tests/ui/mixed_separators_fails.stderr` — verify it contains `mixing `,` and `;` separators is not allowed inside `column!`/`row!`; pick one`.

If either message is wrong, fix the macro error string in `vexo/vexo_macros/src/lib.rs` and re-run Step 3.

- [ ] **Step 5: Verify tests pass against snapshots (no overwrite)**

Run: `cargo test -p vexo_macros --test ui`
Expected: PASS — the `.stderr` files match.

- [ ] **Step 6: Commit**

```bash
git add vexo/vexo_macros/tests/ui/let_binding_fails.rs vexo/vexo_macros/tests/ui/let_binding_fails.stderr vexo/vexo_macros/tests/ui/mixed_separators_fails.rs vexo/vexo_macros/tests/ui/mixed_separators_fails.stderr
git commit -m "test: add compile-fail UI tests for let-binding and mixed-separator errors"
```

---

## Task 8: Sample migration — `shared_app/src/chats/conversation_list.rs` dynamic loop

This task migrates the one clean migration target: the dynamic loop that builds the conversation list using plain `Layout::column()`. This is the highest-value site — it demonstrates the `for` loop replacing the imperative `list = list.push(row)` pattern.

The other two candidate sites (`app.rs` tab bar cell, `titled_container.rs`) use custom layout modifiers (`gap()`, `align()`, `width_percent()`) that the macros as scoped don't support. They stay on the existing `MultiChild::new(children![...], layout)` form. See "Migration Scope Refinement" at the top of this plan.

**Files:**
- Modify: `shared_app/src/chats/conversation_list.rs` (lines ~53-71, the dynamic loop)

**Interfaces:**
- Consumes: `vexo::column!` with `for` support (Task 5+)

- [ ] **Step 1: Read the current code**

Read `shared_app/src/chats/conversation_list.rs` around lines 45-75 to find the exact dynamic loop construction. The current form (from exploration) is:

```rust
let mut list = MultiChild::empty(Layout::column());
for conv in &self.conversations {
    let row = build_conversation_row(conv, /* on_press closure */);
    list = list.push(row);
}
```

The exact variable names, closure arguments, and surrounding context may differ — read the file to get the precise code before editing.

- [ ] **Step 2: Migrate the dynamic loop to `for` inside `column!`**

Replace the imperative loop with a declarative `for` inside `column!`. The original uses plain `Layout::column()`, which is exactly what `column!` bakes in, so the layout is preserved:

```rust
// Before:
let mut list = MultiChild::empty(Layout::column());
for conv in &self.conversations {
    let row = build_conversation_row(conv, /* on_press closure */);
    list = list.push(row);
}

// After:
let list = column! {
    for conv in &self.conversations {
        build_conversation_row(conv, /* on_press closure */)
    }
};
```

Key changes:
- `let mut list` becomes `let list` (immutable — `column!` returns the final `MultiChild` directly)
- The `MultiChild::empty(Layout::column())` + imperative `.push()` loop collapses into a single `column! { for ... { ... } }` expression
- The `build_conversation_row(...)` call moves inside the `for` body; remove the intermediate `let row = ...` binding (it's now the `for` body's trailing expression)
- Preserve the exact arguments to `build_conversation_row` — read the current code to copy them verbatim

**Important:** Read the actual file first. The `build_conversation_row` call may capture variables from the surrounding scope (e.g., `self`, theme refs, callback closures). Those captures work unchanged inside `column! { for ... { ... } }` because the `for` body is a normal Rust closure-like block that borrows the environment.

- [ ] **Step 3: Verify build**

Run: `cargo build -p shared_app`
Expected: builds successfully. If it fails, the most likely cause is a borrow/move issue inside the `for` body (e.g., `build_conversation_row` takes `self` by value instead of reference). Fix by adjusting the call to match the original's capture style.

- [ ] **Step 4: Verify existing tests pass**

Run: `cargo test -p shared_app`
Expected: PASS (or no tests exist for this module — verify there's no regression).

If `shared_app` has no tests, run `cargo test -p vexo` to confirm the framework still passes.

- [ ] **Step 5: Ask user to run visual smoke test**

Per CLAUDE.md, do NOT run `cargo run -p desktop_demo` yourself. Ask the user:

> "Migration complete. Please run `cargo run -p desktop_demo` and verify the conversation list renders identically (same rows, same order, same tap behavior). The dynamic loop was migrated from an imperative `MultiChild::push` pattern to a declarative `column! { for ... }` — the layout should be unchanged since both use plain `Layout::column()`."

Wait for user confirmation before proceeding.

- [ ] **Step 6: Commit**

```bash
git add shared_app/src/chats/conversation_list.rs
git commit -m "refactor: migrate conversation_list dynamic loop to column! with for"
```

---

## Task 9: Final verification

**Files:** (none — verification only)

- [ ] **Step 1: Build all crates**

Run: `cargo build`
Expected: all workspace members build successfully.

- [ ] **Step 2: Run all tests**

Run: `cargo test`
Expected: all tests pass, including:
- `vexo` lib tests (`view_builder`, `container`)
- `vexo/tests/builder_macros.rs` (behavioral — 16 tests)
- `vexo_macros` UI tests (`trybuild` — pass + fail snapshots)
- `shared_app` tests (if any)

- [ ] **Step 3: Spot-check macro expansion (optional)**

Run: `cargo expand -p vexo --test builder_macros`
Expected: review the expanded code for `column!`/`row!` — should show the accumulator skeleton with `ChildPush::push_into` calls, `build_optional`/`build_either`/`build_array` for control flow, and `MultiChild::new(...)` at the end. Verify no extra tree nodes are injected.

- [ ] **Step 4: Confirm success criteria**

Verify the success criteria from the spec (adjusted per the Migration Scope Refinement):

1. ✅ `cargo build` and `cargo test` pass across all crates (Step 2)
2. ✅ `column!`/`row!` accept plain widgets, `if` (with/without else), `for`, `match` (Tasks 3-6 behavioral tests)
3. ✅ `let` bindings and mixed separators rejected with clear errors (Task 7 `.stderr` snapshots)
4. ✅ The conversation_list dynamic loop migrated to `column! { for ... }` and renders identically (Task 8, user-verified)
5. ✅ `view_builder` helpers public and unit-tested (Task 2)
6. ✅ trybuild UI tests snapshot expected errors (Tasks 3, 4, 5, 6, 7)

- [ ] **Step 5: Final commit (if any stray changes)**

```bash
git status
# if clean, nothing to commit; if not, commit remaining changes
```

---

## Self-Review Notes

**Spec coverage:**
- Macro syntax (plain, if/else, for, match, nesting, empty, separators) → Tasks 3-6
- `view_builder` helper API (4 functions) → Task 2
- `ChildPush for Vec` extension → Task 2
- Proc-macro parsing (syn-based, brace-aware splitting, separator uniformity) → Task 3
- Error cases (let, mixed separators) → Task 3 (emission), Task 7 (snapshot tests)
- Behavioral tests → Tasks 3-6 (incremental, 16 total)
- trybuild UI tests (pass + fail) → Tasks 3, 4, 5, 6, 7
- Sample migration → Task 8 (1 site: conversation_list dynamic loop)
- Re-exports → Task 1 (macro), Task 2 (helpers)
- Crate scaffolding → Task 1

**Migration scope adjustment:** The spec called for 3 migration sites. Planning revealed 2 of 3 use custom layout modifiers the macro doesn't support (see "Migration Scope Refinement" at the top). The plan migrates the 1 clean site (the highest-value `for`-loop case) and documents the gap as a follow-up spec opportunity. This is the honest outcome — migrating the other 2 sites to `WithLayout::new(column! { ... }, custom_layout)` would add nesting and make the code worse, not better.

**Placeholder scan:** No TBD/TODO/placeholder text. Every step contains complete code or exact commands.

**Type consistency:**
- `build_block`, `build_optional`, `build_either`, `build_array` signatures match between Task 2 (definition) and Tasks 4-6 (macro codegen calls).
- `ChildPush for Vec<Box<dyn Widget>>` defined in Task 2, consumed by macro-generated code in Tasks 4-5.
- `MultiChild::new(vec, layout)` signature matches the existing API (used in macro codegen).
- `Layout::column()` / `Layout::row()` are existing constructors (used in macro codegen).
- `Widget::boxed()` is the existing erasure method (used in `if`/`for`/`match` arm wrapping).
- `ChildPush::push_into` is the existing trait method (used in all codegen push calls).
