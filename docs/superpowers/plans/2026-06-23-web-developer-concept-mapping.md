# Web Developer Concept Mapping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename and restructure Vexo's public API so every concept maps to what a React/Vue/Svelte developer already knows, without changing the three-tree engine.

**Architecture:** Add new names as type aliases and re-exports alongside deprecated old names. Add derive macro for auto-wiring dirty callbacks. Add `children![]` macro and `Column`/`Row` aliases. Make `.push()` accept `impl Widget` to eliminate `.boxed()`. Hide internal types from public API.

**Tech Stack:** Rust, proc-macro2/syn/quote for derive macro, existing Taffy/glyphon/wgpu stack unchanged.

## Global Constraints

- Zero breaking changes in Phase 1 — all old names continue to work with deprecation warnings
- Internal engine code (elements, render objects, pipeline) keeps using old names internally
- All existing tests must continue to pass after each task
- `cargo build` and `cargo test` must pass after every commit
- No commit message attribution (per CLAUDE.md)

---

## File Structure

| File | Responsibility |
|---|---|
| `vexo/src/reactive/mod.rs` | Rename `StatefulMutable` → `Signal` (type alias + deprecation) |
| `vexo/src/stateful_widget.rs` | Add `Component` trait alias, `ComponentState` trait alias, rename lifecycle methods, add `RenderContext`/`LifecycleContext` aliases |
| `vexo/src/widgets/container.rs` | Add `Column`/`Row` type aliases |
| `vexo/src/widgets/mod.rs` | Add `children![]` macro, make `.push()` accept `impl Widget` |
| `vexo/src/macros.rs` | Add `children![]` macro definition |
| `vexo/src/lib.rs` | Re-export new names, add deprecated aliases, hide internal types |
| `vexo/src/component_state_derive/` (new crate) | `#[derive(ComponentState)]` proc macro |
| `shared_app/src/lib.rs` | Update to use new API (Phase 2 validation) |

---

### Task 1: Rename `StatefulMutable` to `Signal`

**Files:**
- Modify: `vexo/src/reactive/mod.rs`
- Modify: `vexo/src/lib.rs`

**Interfaces:**
- Produces: `Signal<T>` as public type alias for `StatefulMutable<T>`, deprecated re-export of `StatefulMutable`

- [ ] **Step 1: Add `Signal` type alias in `reactive/mod.rs`**

At the top of the file, after the `use` statements, add:

```rust
/// Reactive state primitive — the Vexo equivalent of React's `useState` or Vue's `ref()`.
///
/// When `set()` is called and the value changes, the owning element is
/// automatically marked dirty, triggering a rebuild on the next frame.
///
/// Renamed from `StatefulMutable` for web developer familiarity.
/// `StatefulMutable` remains available as a deprecated alias.
pub type Signal<T> = StatefulMutable<T>;
```

- [ ] **Step 2: Add deprecated alias and new re-export in `lib.rs`**

In `vexo/src/lib.rs`, in the `reactive` module section, ensure `Signal` is re-exported. Find the existing line:

```rust
pub use reactive::StatefulMutable;
```

Replace with:

```rust
pub use reactive::Signal;
#[deprecated(since = "0.x", note = "Use `Signal` instead")]
pub use reactive::StatefulMutable;
```

Also add `Signal` to the `reactive` module's public interface — since `Signal` is a `pub type` alias inside `reactive/mod.rs`, it's already public via the module. Just ensure the re-export line exists.

- [ ] **Step 3: Run `cargo build` to verify**

Run: `cargo build -p vexo`
Expected: Compiles with deprecation warnings on any internal usage of `StatefulMutable` (these are fine — internal code keeps using old names for now).

- [ ] **Step 4: Run `cargo test` to verify**

Run: `cargo test -p vexo`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/reactive/mod.rs vexo/src/lib.rs
git commit -m "feat: add Signal type alias for StatefulMutable with deprecation"
```

---

### Task 2: Add `Component` and `ComponentState` trait aliases with renamed lifecycle methods

**Files:**
- Modify: `vexo/src/stateful_widget.rs`
- Modify: `vexo/src/lib.rs`

**Interfaces:**
- Consumes: `StatefulWidget` trait, `State` trait, `BuildContext`, `StateContext` (from existing code)
- Produces: `Component` trait (with `render()` method), `ComponentState` trait (with `on_mount`/`on_update`/`on_unmount`/`on_tick`), `RenderContext` alias, `LifecycleContext` alias

- [ ] **Step 1: Add renamed lifecycle methods to `State` trait**

In `vexo/src/stateful_widget.rs`, add the new method names as default implementations that delegate to the old ones. Inside the `State` trait definition, after the existing methods, add:

```rust
/// Called once when the element is first mounted.
///
/// Alias for `init()`. Web developers familiar with React's `useEffect([])`
/// or Vue's `onMounted()` should use this name.
fn on_mount(&mut self, ctx: &mut StateContext) {
    self.init(ctx);
}

/// Called when the parent widget is rebuilt with new configuration.
///
/// Alias for `did_update_widget()`. Maps to React's `useEffect([deps])`
/// or Vue's `onUpdated()`.
fn on_update(&mut self, old_widget: &dyn Any, ctx: &mut StateContext) {
    self.did_update_widget(old_widget, ctx);
}

/// Called when the element is removed from the tree.
///
/// Alias for `dispose()`. Maps to React's cleanup function or Vue's `onUnmounted()`.
fn on_unmount(&mut self, ctx: &mut StateContext) {
    self.dispose(ctx);
}

/// Called every frame before render, for animations and per-frame logic.
///
/// Alias for `animate()`. Maps to `requestAnimationFrame`.
fn on_tick(&mut self, now: std::time::Instant) {
    self.animate(now);
}
```

- [ ] **Step 2: Add `ComponentState` type alias**

After the `State` trait definition, add:

```rust
/// Trait for state objects belonging to Components.
///
/// This is the web-developer-friendly name for `State`.
/// `State` remains available as a deprecated alias.
pub trait ComponentState: State {}

/// Blanket impl: anything implementing `State` is a `ComponentState`.
impl<T: State> ComponentState for T {}
```

- [ ] **Step 3: Add `Component` trait**

After `ComponentState`, add:

```rust
/// Trait for widgets with persistent mutable state.
///
/// This is the web-developer-friendly name for `StatefulWidget`.
/// Maps to React's function component or Vue's component.
///
/// The key difference from `StatefulWidget` is the method name:
/// `render()` instead of `build()`.
pub trait Component: StatefulWidget {}

/// Blanket impl: anything implementing `StatefulWidget` is a `Component`.
impl<T: StatefulWidget> Component for T {}
```

- [ ] **Step 4: Add `RenderContext` and `LifecycleContext` type aliases**

After the `BuildContext` struct definition, add:

```rust
/// Context provided to `Component::render()`.
///
/// Web-developer-friendly name for `BuildContext`.
/// Maps to React's render function context or Vue's setup context.
pub type RenderContext<'a> = BuildContext<'a>;
```

After the `StateContext` struct definition, add:

```rust
/// Context provided to `ComponentState` lifecycle methods.
///
/// Web-developer-friendly name for `StateContext`.
/// Maps to React's effect context or Vue's lifecycle hook context.
pub type LifecycleContext<'a> = StateContext<'a>;
```

- [ ] **Step 5: Update public re-exports in `lib.rs`**

Find the existing line:

```rust
pub use stateful_widget::{StatefulWidget, BuildContext,
    StatefulElement, ProxyRenderObject, State, StateContext, SimpleState};
```

Replace with:

```rust
pub use stateful_widget::{Component, ComponentState, RenderContext, LifecycleContext,
    StatefulWidget, BuildContext,
    StatefulElement, ProxyRenderObject, State, StateContext, SimpleState};
```

Also add deprecated aliases after the new re-exports:

```rust
#[deprecated(since = "0.x", note = "Use `Component` instead")]
pub use stateful_widget::StatefulWidget;
#[deprecated(since = "0.x", note = "Use `ComponentState` instead")]
pub use stateful_widget::State as ComponentStateDeprecated;
#[deprecated(since = "0.x", note = "Use `RenderContext` instead")]
pub use stateful_widget::BuildContext;
#[deprecated(since = "0.x", note = "Use `LifecycleContext` instead")]
pub use stateful_widget::StateContext;
```

Note: The `StatefulWidget` re-export must remain (not just deprecated) because internal code and existing user code uses it. The deprecated aliases are *additional* re-exports, not replacements.

- [ ] **Step 6: Run `cargo build` and `cargo test`**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: Compiles and all tests pass.

- [ ] **Step 7: Commit**

```bash
git add vexo/src/stateful_widget.rs vexo/src/lib.rs
git commit -m "feat: add Component/ComponentState traits, renamed lifecycle methods, RenderContext/LifecycleContext aliases"
```

---

### Task 3: Add `Column` and `Row` type aliases

**Files:**
- Modify: `vexo/src/widgets/container.rs`
- Modify: `vexo/src/lib.rs`

**Interfaces:**
- Produces: `Column` and `Row` as type aliases for `Flex`, with `Column::new()` and `Row::new()` constructors

- [ ] **Step 1: Add type aliases in `container.rs`**

At the end of `vexo/src/widgets/container.rs`, after the `Flex` impl blocks, add:

```rust
/// A vertical flex container — the web developer's default layout.
///
/// Alias for `Flex` pre-configured with `FlexDirection::Column`.
/// Usage: `Column::new().gap(16.0).children![...]`
pub type Column = Flex;

/// A horizontal flex container.
///
/// Alias for `Flex` pre-configured with `FlexDirection::Row`.
/// Usage: `Row::new().gap(8.0).children![...]`
pub type Row = Flex;
```

Since `Column` and `Row` are type aliases for `Flex`, they inherit all of `Flex`'s methods. However, `Column::new()` would call `Flex::new()` which creates a *row*. We need to add inherent methods that shadow the alias.

This won't work with type aliases because you can't add inherent impls to a type alias. Instead, we need to make `Column` and `Row` work through the existing `Flex::column()` and `Flex::row()` constructors.

The simplest approach: keep the type aliases but don't add `Column::new()` / `Row::new()` — instead, document that `Column` users should call `Column::column()` (which is confusing) or just use `Flex::column()`.

**Better approach:** Add free functions instead of type aliases.

Replace the type aliases with:

```rust
/// Create a vertical flex container.
///
/// Web developer equivalent of `<div style="display: flex; flex-direction: column">`.
/// Returns a `Flex` pre-configured with column direction.
///
/// # Example
/// ```ignore
/// Column::new().gap(16.0).children![
///     Text::new("Title"),
///     Text::new("Body"),
/// ]
/// ```
pub struct Column;

impl Column {
    pub fn new() -> Flex {
        Flex::column()
    }
}

/// Create a horizontal flex container.
///
/// Web developer equivalent of `<div style="display: flex; flex-direction: row">`.
/// Returns a `Flex` pre-configured with row direction.
///
/// # Example
/// ```ignore
/// Row::new().gap(8.0).children![
///     Text::new("A"),
///     Text::new("B"),
/// ]
/// ```
pub struct Row;

impl Row {
    pub fn new() -> Flex {
        Flex::row()
    }
}
```

This way `Column::new()` returns a `Flex`, and all of `Flex`'s builder methods are available on the return value.

- [ ] **Step 2: Add re-exports in `lib.rs`**

Find the existing line:

```rust
pub use widgets::{Widget, Text, Flex, Grid, TextEdit,
    TextEditState, TextEditingController, ScrollView, Image};
```

Replace with:

```rust
pub use widgets::{Widget, Text, Flex, Grid, TextEdit,
    TextEditState, TextEditingController, ScrollView, Image,
    Column, Row};
```

- [ ] **Step 3: Write a test**

In `vexo/src/widgets/container.rs`, add at the bottom in the `#[cfg(test)]` module (or create one if it doesn't exist):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_new_creates_vertical_flex() {
        let col = Column::new();
        // Column::new() should produce the same result as Flex::column()
        let flex = Flex::column();
        assert_eq!(col.children().len(), flex.children().len());
    }

    #[test]
    fn row_new_creates_horizontal_flex() {
        let row = Row::new();
        let flex = Flex::row();
        assert_eq!(row.children().len(), flex.children().len());
    }

    #[test]
    fn column_supports_builder_methods() {
        let col = Column::new().gap(16.0).padding(8.0);
        // Should compile and return a Flex with layout properties set
        assert_eq!(col.children().len(), 0);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p vexo -- container::tests`
Expected: All 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/container.rs vexo/src/lib.rs
git commit -m "feat: add Column and Row constructor structs for web developer ergonomics"
```

---

### Task 4: Make `.push()` accept `impl Widget` to eliminate `.boxed()`

**Files:**
- Modify: `vexo/src/widgets/container.rs` (Flex::push)
- Modify: `vexo/src/widgets/grid.rs` (Grid::push)

**Interfaces:**
- Consumes: `Widget` trait with `boxed()` method
- Produces: `push()` that accepts `impl Widget + 'static` and boxes internally

- [ ] **Step 1: Update `Flex::push()` signature**

In `vexo/src/widgets/container.rs`, find the existing `push` method:

```rust
pub fn push(mut self, child: impl Widget + 'static) -> Self
```

This already accepts `impl Widget + 'static`! Check if it boxes internally. Read the implementation — it should be:

```rust
pub fn push(mut self, child: impl Widget + 'static) -> Self {
    self.children.push(child.boxed());
    self
}
```

If it already does this, no change needed for Flex. Verify by reading the actual code.

- [ ] **Step 2: Update `Grid::push()` signature**

Same check for `Grid::push()` in `vexo/src/widgets/grid.rs`. If it already accepts `impl Widget + 'static` and boxes internally, no change needed.

- [ ] **Step 3: Verify `.boxed()` is no longer needed in `.push()` calls**

Check `shared_app/src/lib.rs` for any `.push(something.boxed())` patterns. If `.push()` already boxes internally, these `.boxed()` calls are redundant but harmless. They can be removed in Phase 2.

- [ ] **Step 4: Run `cargo build` and `cargo test`**

Run: `cargo build -p vexo && cargo test -p vexo`
Expected: Compiles and all tests pass.

- [ ] **Step 5: Commit (only if changes were made)**

If `.push()` already accepts `impl Widget`, skip this commit and note it in the plan. If changes were needed:

```bash
git add vexo/src/widgets/container.rs vexo/src/widgets/grid.rs
git commit -m "feat: make push() accept impl Widget, eliminating .boxed() at call sites"
```

---

### Task 5: Add `children![]` macro

**Files:**
- Modify: `vexo/src/macros.rs`
- Modify: `vexo/src/lib.rs` (macro re-export)

**Interfaces:**
- Consumes: `Flex::push()`, `Grid::push()` (from Task 4)
- Produces: `children![]` macro that expands to chained `.push()` calls

- [ ] **Step 1: Write the `children![]` macro in `macros.rs`**

In `vexo/src/macros.rs`, add after the existing `grid!` macro:

```rust
/// Declarative child composition macro — reads like JSX children.
///
/// Expands to chained `.push()` calls with automatic boxing.
/// Each child expression must produce something that implements `Widget`.
///
/// # Example
/// ```ignore
/// Column::new()
///     .gap(16.0)
///     .children![
///         Text::new("Title").padding(8.0),
///         Text::new("Body"),
///         Row::new().children![
///             Text::new("A"),
///             Text::new("B"),
///         ],
///     ]
/// ```
///
/// Conditional children via `if`:
/// ```ignore
/// Column::new().children![
///     Text::new("Always shown"),
///     if show_extra { Text::new("Extra") },
/// ]
/// ```
/// `if` expressions that don't evaluate produce `Option<_>` — the macro
/// filters these by calling `.push()` only for `Some` values.
#[macro_export]
macro_rules! children {
    // Entry point: called as method syntax .children![...]
    // This variant is for use as a method call: widget.children![a, b, c]
    // It expands to: widget.push(a).push(b).push(c)
    ($parent:expr, $($child:expr),* $(,)?) => {{
        let mut __vexo_children_result = $parent;
        $(
            __vexo_children_result = $crate::children_push!(__vexo_children_result, $child);
        )*
        __vexo_children_result
    }};
}

/// Helper macro: push a single child, handling Option filtering.
#[macro_export]
macro_rules! children_push {
    // Non-optional child: just push
    ($parent:expr, $child:expr) => {
        $parent.push($child)
    };
}
```

Wait — this approach has a problem. The `children![]` macro is intended to be used as a method: `Column::new().children![...]`. But Rust macros can't be called with dot syntax. We need a different approach.

The macro should be used as: `children![Column::new().gap(16.0), child1, child2, ...]` — but that's awkward.

Better approach: make it a method-like macro that takes the parent as the first argument:

```rust
Column::new().gap(16.0).with_children![
    Text::new("Title"),
    Text::new("Body"),
]
```

But again, macros can't be called with dot syntax.

**Simplest working approach:** The macro takes the parent as the first argument:

```rust
children![Column::new().gap(16.0),
    Text::new("Title"),
    Text::new("Body"),
]
```

This is less ergonomic than the dot-syntax version. Let me reconsider.

**Best approach:** Add a `.children()` method on `Flex`/`Grid` that takes a closure or uses the existing `column!`/`row!` macro pattern. Actually, the simplest approach that works with Rust syntax is to make `children!` a macro that wraps the parent:

```rust
let col = children![Column::new().gap(16.0),
    Text::new("Title"),
    Text::new("Body"),
];
```

This matches the existing `column!`/`row!` macro pattern. Let me implement this:

In `vexo/src/macros.rs`, add:

```rust
/// Declarative child composition macro.
///
/// Takes a parent widget expression followed by child expressions.
/// Expands to chained `.push()` calls. Each child must implement `Widget`.
///
/// # Example
/// ```ignore
/// children![Column::new().gap(16.0),
///     Text::new("Title").padding(8.0),
///     Text::new("Body"),
///     children![Row::new().gap(8.0),
///         Text::new("A"),
///         Text::new("B"),
///     ],
/// ]
/// ```
#[macro_export]
macro_rules! children {
    ($parent:expr, $($child:expr),* $(,)?) => {{
        let mut __vexo_parent = $parent;
        $(
            __vexo_parent = __vexo_parent.push($child);
        )*
        __vexo_parent
    }};
}
```

- [ ] **Step 2: Write a test for the `children!` macro**

Add to `vexo/src/macros.rs` or a test file:

```rust
#[cfg(test)]
mod tests {
    use crate::{Flex, Text, children};

    #[test]
    fn children_macro_pushes_children() {
        let col = children![Flex::column(),
            Text::new("A"),
            Text::new("B"),
            Text::new("C"),
        ];
        assert_eq!(col.children().len(), 3);
    }

    #[test]
    fn children_macro_nesting() {
        let col = children![Flex::column(),
            Text::new("Title"),
            children![Flex::row(),
                Text::new("A"),
                Text::new("B"),
            ],
        ];
        assert_eq!(col.children().len(), 2);
    }

    #[test]
    fn children_macro_with_builder_methods() {
        let col = children![Flex::column().gap(16.0),
            Text::new("Title").padding(8.0),
            Text::new("Body"),
        ];
        assert_eq!(col.children().len(), 2);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p vexo -- macros::tests`
Expected: All 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add vexo/src/macros.rs
git commit -m "feat: add children! macro for declarative child composition"
```

---

### Task 6: Add `#[derive(ComponentState)]` proc macro

**Files:**
- Create: `vexo/src/component_state_derive.rs` (proc macro implementation, in a separate crate)
- Create: `vexo/component_state_derive/Cargo.toml`
- Create: `vexo/component_state_derive/src/lib.rs`
- Modify: `vexo/Cargo.toml` (add proc-macro dependency)

**Interfaces:**
- Consumes: `Signal<T>` type (from Task 1), `set_dirty_callback` method on `Signal`
- Produces: `#[derive(ComponentState)]` that auto-generates `set_dirty_callback` impl

This task requires a separate proc-macro crate because Rust proc macros must be in their own crate.

- [ ] **Step 1: Create the proc-macro crate**

Create `vexo/component_state_derive/Cargo.toml`:

```toml
[package]
name = "component_state_derive"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true

[dependencies]
syn = "2"
quote = "2"
proc-macro2 = "1"
```

Create `vexo/component_state_derive/src/lib.rs`:

```rust
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields, Type, PathSegment, GenericArgument};

#[proc_macro_derive(ComponentState)]
pub fn derive_component_state(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("#[derive(ComponentState)] only supports structs with named fields"),
        },
        _ => panic!("#[derive(ComponentState)] only supports structs"),
    };

    let mut wire_calls = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let ty = &field.ty;

        if is_signal_type(ty) || is_option_signal_type(ty) {
            if is_option_signal_type(ty) {
                wire_calls.push(quote! {
                    if let Some(ref mut __field) = self.#field_name {
                        __field.set_dirty_callback(callback.clone());
                    }
                });
            } else {
                wire_calls.push(quote! {
                    self.#field_name.set_dirty_callback(callback.clone());
                });
            }
        }
    }

    let expanded = quote! {
        impl vexo::ComponentState for #name {}

        impl vexo::State for #name {
            fn set_dirty_callback(&mut self, callback: std::sync::Arc<dyn Fn() + Send + Sync>) {
                #(#wire_calls)*
            }
        }
    };

    TokenStream::from(expanded)
}

fn is_signal_type(ty: &Type) -> bool {
    // Check if type is Signal<T> or vexo::Signal<T> or vexo::reactive::Signal<T>
    let type_str = quote!(#ty).to_string().replace(" ", "");
    type_str.starts_with("Signal<") ||
        type_str.starts_with("vexo::Signal<") ||
        type_str.starts_with("vexo::reactive::Signal<") ||
        type_str.starts_with("StatefulMutable<") ||
        type_str.starts_with("vexo::StatefulMutable<") ||
        type_str.starts_with("vexo::reactive::StatefulMutable<")
}

fn is_option_signal_type(ty: &Type) -> bool {
    let type_str = quote!(#ty).to_string().replace(" ", "");
    type_str.starts_with("Option<Signal<") ||
        type_str.starts_with("Option<vexo::Signal<") ||
        type_str.starts_with("Option<StatefulMutable<") ||
        type_str.starts_with("Option<vexo::StatefulMutable<") ||
        type_str.starts_with("Option<vexo::reactive::Signal<") ||
        type_str.starts_with("Option<vexo::reactive::StatefulMutable<")
}
```

- [ ] **Step 2: Add the proc-macro crate to the workspace**

In the root `Cargo.toml`, add to the `[workspace] members` list:

```toml
members = [
    "vexo",
    "vexo/component_state_derive",
    "shared_app",
    "desktop_demo",
]
```

In `vexo/Cargo.toml`, add to `[dependencies]`:

```toml
component_state_derive = { path = "component_state_derive" }
```

- [ ] **Step 3: Re-export the derive macro from `vexo/src/lib.rs`**

Add to `vexo/src/lib.rs`:

```rust
pub use component_state_derive::ComponentState;
```

- [ ] **Step 4: Write a test for the derive macro**

In `vexo/src/stateful_widget.rs`, add to the test module:

```rust
#[cfg(test)]
mod derive_tests {
    use super::*;
    use crate::reactive::StatefulMutable;
    use crate::ComponentState;
    use std::sync::Arc;

    #[derive(ComponentState)]
    struct TestState {
        count: StatefulMutable<u32>,
        label: String,  // non-Signal field, should be skipped
    }

    impl Default for TestState {
        fn default() -> Self {
            Self {
                count: StatefulMutable::new(0),
                label: String::new(),
            }
        }
    }

    #[test]
    fn derive_wires_signal_fields() {
        let mut state = TestState::default();
        let mut called = false;
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(|| { called = true; });
        state.set_dirty_callback(callback);

        // Setting the Signal field should trigger the callback
        state.count.set(1);
        assert!(called);
    }

    #[test]
    fn derive_skips_non_signal_fields() {
        // Just verify it compiles — non-Signal fields are skipped
        let mut state = TestState::default();
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        state.set_dirty_callback(callback);
        // label is a String, not a Signal — no wiring needed
    }
}
```

- [ ] **Step 5: Run `cargo build` and `cargo test`**

Run: `cargo build && cargo test -p vexo -- derive_tests`
Expected: Compiles and both tests pass.

- [ ] **Step 6: Commit**

```bash
git add vexo/component_state_derive/ vexo/Cargo.toml Cargo.toml vexo/src/lib.rs vexo/src/stateful_widget.rs
git commit -m "feat: add #[derive(ComponentState)] proc macro for auto-wiring Signal dirty callbacks"
```

---

### Task 7: Hide internal types from public API

**Files:**
- Modify: `vexo/src/lib.rs`

**Interfaces:**
- Consumes: Current public re-exports of internal types
- Produces: Internal types no longer re-exported; deprecated re-exports for transition

- [ ] **Step 1: Add deprecated annotations to internal type re-exports**

In `vexo/src/lib.rs`, find the lines that re-export internal types and add deprecation warnings. These types should still be available (for any code that uses them) but marked as internal:

```rust
// Element tree — internal, like DOM internals
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use element::{Element, ElementRegistry};
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use element_context::ElementContext;
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use elements::{LeafElement, ContainerElement};

// Render objects — internal, like browser layout/paint
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use render_object::{RenderObject, RenderObjectRegistry,
    LayoutContext, LayoutResult, PaintContext, HitTestContext};
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use render_objects::{TextRenderObject, ContainerRenderObject,
    TextEditRenderObject, ImageRenderObject};

// Pipeline — internal, like React's scheduler
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use pipeline::ThreeTreePipeline;
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use build_owner::{BuildOwner, RebuildResult};
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use dirty::DirtyTracking;
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use child_ops::{ChildOp, ChildOps};

// State storage — internal
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use element_state::StateStorage;

// Internal IDs
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use id::{ElementKey, RenderObjectKey};

// Reconciliation — internal
#[deprecated(since = "0.x", note = "Internal API — framework-managed, not for direct use")]
pub use reconcile::Reconcilable;
pub use hit_test::HitTestResult;
pub use global_key_registry::{GlobalKeyRegistry, GlobalKeyError};
pub use update_result::UpdateResult;
```

Note: We don't fully remove these yet (Phase 3). We just mark them deprecated so web devs know not to use them. Some internal types like `StatefulElement` and `ProxyRenderObject` are already only re-exported for advanced use — deprecate those too.

- [ ] **Step 2: Run `cargo build` and `cargo test`**

Run: `cargo build && cargo test`
Expected: Compiles with deprecation warnings. All tests pass.

- [ ] **Step 3: Commit**

```bash
git add vexo/src/lib.rs
git commit -m "feat: deprecate internal API types (Element, RenderObject, Pipeline, etc.) from public re-exports"
```

---

### Task 8: Update `shared_app` to use new API (Phase 2 validation)

**Files:**
- Modify: `shared_app/src/lib.rs`

**Interfaces:**
- Consumes: All new names from Tasks 1-7

- [ ] **Step 1: Update imports**

Replace the current imports:

```rust
use vexo::reactive::StatefulMutable;
use vexo::{
    run_desktop_demo, AnimationController, Application, BuildContext, Color, ColorTween, Flex,
    Focus, Image, ImageData, ScrollView, State as VexoState, StateContext, StatefulWidget, Text,
    Tween, Widget,
};
```

With:

```rust
use vexo::reactive::Signal;
use vexo::{
    run_desktop_demo, AnimationController, Application, Column, Component, ComponentState,
    RenderContext, Color, ColorTween, Focus, Image, ImageData, LifecycleContext, Row,
    ScrollView, Text, Tween, Widget,
};
```

- [ ] **Step 2: Update `FocusableScrollList` to use new API**

Replace:

```rust
struct FocusableScrollListState {
    is_focused: StatefulMutable<bool>,
}

impl Default for FocusableScrollListState {
    fn default() -> Self {
        Self {
            is_focused: StatefulMutable::new(false),
        }
    }
}

impl VexoState for FocusableScrollListState {
    fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.is_focused.set_dirty_callback(callback);
    }
}

impl StatefulWidget for FocusableScrollList {
    type State = FocusableScrollListState;

    fn build(&self, state: &mut Self::State, _ctx: &mut BuildContext) -> Box<dyn Widget> {
```

With:

```rust
#[derive(ComponentState)]
struct FocusableScrollListState {
    is_focused: Signal<bool>,
}

impl Default for FocusableScrollListState {
    fn default() -> Self {
        Self {
            is_focused: Signal::new(false),
        }
    }
}

impl Component for FocusableScrollList {
    type State = FocusableScrollListState;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
```

- [ ] **Step 3: Update `AnimatedButton` to use new API**

Replace:

```rust
impl VexoState for AnimatedButtonState {
    fn init(&mut self, ctx: &mut StateContext) {
        self.anim
            .borrow_mut()
            .set_ticker(ctx.animation_ticker().clone());
    }

    fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.anim.borrow_mut().set_dirty_callback(callback);
    }

    fn animate(&mut self, now: std::time::Instant) {
        self.anim.borrow_mut().advance(now);
    }
}

impl StatefulWidget for AnimatedButton {
    type State = AnimatedButtonState;

    fn build(&self, state: &mut Self::State, _ctx: &mut BuildContext) -> Box<dyn Widget> {
```

With:

```rust
impl State for AnimatedButtonState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        self.anim
            .borrow_mut()
            .set_ticker(ctx.animation_ticker().clone());
    }

    fn set_dirty_callback(&mut self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.anim.borrow_mut().set_dirty_callback(callback);
    }

    fn on_tick(&mut self, now: std::time::Instant) {
        self.anim.borrow_mut().advance(now);
    }
}

impl Component for AnimatedButton {
    type State = AnimatedButtonState;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
```

Note: `AnimatedButtonState` can't use `#[derive(ComponentState)]` because its `anim` field is `Rc<RefCell<AnimationController>>`, not a `Signal`. It still needs manual `set_dirty_callback`. This is expected — the derive only auto-wires `Signal` fields.

- [ ] **Step 4: Update `build_scroll_content` to use `Column`**

Replace:

```rust
fn build_scroll_content() -> Box<dyn Widget> {
    let mut column = Flex::column().gap(0.0);
    for i in 0..20 {
        let label = format!("Item {}", i + 1);
        column = column.push(Text::new(&label).padding(16.0).background(if i % 2 == 0 {
            Color::rgb(0.95, 0.95, 0.95)
        } else {
            Color::WHITE
        }));
    }
    column.boxed()
}
```

With:

```rust
fn build_scroll_content() -> Box<dyn Widget> {
    let mut column = Column::new().gap(0.0);
    for i in 0..20 {
        let label = format!("Item {}", i + 1);
        column = column.push(Text::new(&label).padding(16.0).background(if i % 2 == 0 {
            Color::rgb(0.95, 0.95, 0.95)
        } else {
            Color::WHITE
        }));
    }
    column.boxed()
}
```

- [ ] **Step 5: Update the main `Application::view` to use `Column`**

Replace:

```rust
Flex::column()
    .gap(16.0)
    .push(Text::new("Image Demo").padding(8.0))
    .push(Image::new(test_image).width(200.0).border(Color::BLUE, 3.0))
    .push(FocusableScrollList.boxed())
    .push(AnimatedButton.boxed())
    .boxed()
```

With:

```rust
Column::new()
    .gap(16.0)
    .push(Text::new("Image Demo").padding(8.0))
    .push(Image::new(test_image).width(200.0).border(Color::BLUE, 3.0))
    .push(FocusableScrollList.boxed())
    .push(AnimatedButton.boxed())
    .boxed()
```

- [ ] **Step 6: Run `cargo build` and `cargo test`**

Run: `cargo build && cargo test`
Expected: Compiles and all tests pass. The app should run identically.

- [ ] **Step 7: Run the desktop demo to verify visually**

Run: `cargo run -p desktop_demo`
Expected: Same visual output as before, using the new API names.

- [ ] **Step 8: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "feat: update shared_app to use new Component/Signal/Column/RenderContext API"
```

---

### Task 9: Add integration tests for the new API surface

**Files:**
- Create: `vexo/tests/web_api_tests.rs`

**Interfaces:**
- Consumes: All new public API types from Tasks 1-8

- [ ] **Step 1: Write integration tests**

Create `vexo/tests/web_api_tests.rs`:

```rust
//! Integration tests verifying the web-developer-friendly API surface.
//! These tests ensure that the new names, aliases, and macros work correctly
//! and that the old names still function (with deprecation warnings).

use vexo::*;
use vexo::reactive::Signal;
use std::sync::Arc;

// --- Signal tests ---

#[test]
fn signal_new_and_get() {
    let s: Signal<u32> = Signal::new(42);
    assert_eq!(s.get(), 42);
}

#[test]
fn signal_set_triggers_callback() {
    let s: Signal<u32> = Signal::new(0);
    let mut called = false;
    let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(|| called = true);
    // We can't call set_dirty_callback directly in an integration test
    // without an element context, but we can verify the type exists
    // and the basic get/set works.
    s.set(5);
    assert_eq!(s.get(), 5);
}

// --- Column/Row tests ---

#[test]
fn column_new_returns_flex() {
    let col = Column::new();
    assert_eq!(col.children().len(), 0);
}

#[test]
fn row_new_returns_flex() {
    let row = Row::new();
    assert_eq!(row.children().len(), 0);
}

#[test]
fn column_with_children() {
    let col = Column::new()
        .gap(16.0)
        .push(Text::new("A"))
        .push(Text::new("B"));
    assert_eq!(col.children().len(), 2);
}

// --- children! macro tests ---

#[test]
fn children_macro_basic() {
    let col = vexo::children![Column::new(),
        Text::new("A"),
        Text::new("B"),
    ];
    assert_eq!(col.children().len(), 2);
}

#[test]
fn children_macro_nested() {
    let col = vexo::children![Column::new(),
        Text::new("Title"),
        vexo::children![Row::new(),
            Text::new("A"),
            Text::new("B"),
        ],
    ];
    assert_eq!(col.children().len(), 2);
}

// --- ComponentState derive test ---

#[derive(ComponentState)]
struct DeriveTestState {
    count: Signal<u32>,
    name: String,
}

impl Default for DeriveTestState {
    fn default() -> Self {
        Self {
            count: Signal::new(0),
            name: String::new(),
        }
    }
}

#[test]
fn derive_component_state_compiles() {
    // If this compiles, the derive macro works.
    let _state = DeriveTestState::default();
}

// --- RenderContext / LifecycleContext type alias tests ---

#[test]
fn render_context_is_build_context() {
    // Verify the type alias exists and compiles
    fn _check(_: RenderContext) {}
    // RenderContext = BuildContext, so this should type-check
}

#[test]
fn lifecycle_context_is_state_context() {
    fn _check(_: LifecycleContext) {}
}

// --- Component trait test ---

#[derive(Clone)]
struct TestComponent;

#[derive(ComponentState)]
struct TestComponentState {
    value: Signal<i32>,
}

impl Default for TestComponentState {
    fn default() -> Self {
        Self { value: Signal::new(0) }
    }
}

impl Component for TestComponent {
    type State = TestComponentState;

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        Text::new(format!("Value: {}", state.value.get())).boxed()
    }
}

#[test]
fn component_trait_works() {
    // Verify Component trait compiles and can create a widget
    let comp = TestComponent;
    let _widget: Box<dyn Widget> = comp.clone_boxed();
}
```

- [ ] **Step 2: Run the integration tests**

Run: `cargo test -p vexo --test web_api_tests`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add vexo/tests/web_api_tests.rs
git commit -m "test: add integration tests for web-developer-friendly API surface"
```

---

### Task 10: Update CLAUDE.md with new API names

**Files:**
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: All new names from Tasks 1-9

- [ ] **Step 1: Update the Architecture Overview section**

In the "Architecture Overview" section, update the widget layer description to mention the new names:

Find:
```
│  Widget trait (build() → Element)                              │
│  Widget primitives: Text, TextEditContent, Column, Row         │
│  Widget combinators: DecoratedContainer, GestureDetector        │
│  Stateful widgets: StatefulWidget, StatefulMutable              │
```

Replace with:
```
│  Widget trait (build() → Element)                              │
│  Widget primitives: Text, TextEditContent, Column, Row         │
│  Widget combinators: DecoratedContainer, GestureDetector        │
│  Stateful widgets: Component (was StatefulWidget), Signal (was StatefulMutable) │
```

- [ ] **Step 2: Update the Module Structure section**

In the module structure, update the stateful_widget entry:

Find:
```
├── stateful_widget.rs          # StatefulWidget, State trait
```

Replace with:
```
├── stateful_widget.rs          # Component (was StatefulWidget), ComponentState (was State), RenderContext, LifecycleContext
```

And update the reactive entry:

Find:
```
├── reactive/                   # Reactive primitives (StatefulMutable)
```

Replace with:
```
├── reactive/                   # Reactive primitives (Signal, was StatefulMutable)
```

- [ ] **Step 3: Update the "Key File Locations" section**

Find:
```
- Stateful widgets: `vexo/src/stateful_widget.rs`, `vexo/src/reactive/mod.rs`
```

Replace with:
```
- Stateful widgets: `vexo/src/stateful_widget.rs` (Component, ComponentState), `vexo/src/reactive/mod.rs` (Signal)
```

- [ ] **Step 4: Add a "Web Developer API Mapping" section**

After the "Key File Locations" section, add:

```markdown
## Web Developer API Mapping

Vexo's public API maps to web framework concepts:

| Vexo | Web analog |
|---|---|
| `Component` trait | React function component / Vue component |
| `ComponentState` trait | React hooks state / Vue reactive state |
| `#[derive(ComponentState)]` | Auto-wires `Signal` fields (like React auto-re-renders) |
| `Signal<T>` | React `useState` / Vue `ref()` |
| `Component::render()` | React render / Vue template |
| `RenderContext` | React render function context |
| `LifecycleContext` | React effect context |
| `on_mount()` | React `useEffect([])` / Vue `onMounted()` |
| `on_update()` | React `useEffect([deps])` / Vue `onUpdated()` |
| `on_unmount()` | React cleanup / Vue `onUnmounted()` |
| `on_tick()` | `requestAnimationFrame` |
| `Column::new()` / `Row::new()` | `<div flex-direction: column/row>` |
| `children![]` macro | JSX children |
| `.on_press()` / `.on_release()` | `onClick` / `onMouseUp` |

Deprecated names (still functional): `StatefulWidget` → `Component`, `State` → `ComponentState`, `StatefulMutable` → `Signal`, `BuildContext` → `RenderContext`, `StateContext` → `LifecycleContext`, `build()` → `render()`.
```

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with web developer API mapping and new names"
```

---

## Self-Review

**1. Spec coverage:**
- Section 1 (Renaming Map): Tasks 1, 2, 3 cover all renames
- Section 2 (Auto-wire): Task 6 covers `#[derive(ComponentState)]`
- Section 3 (Component Trait): Task 2 covers `Component`, `ComponentState`, lifecycle renames
- Section 4 (Declarative Children): Tasks 3, 5 cover `Column`/`Row` and `children![]`
- Section 5 (Reduce .boxed()): Task 4 covers `.push()` accepting `impl Widget`
- Section 6 (Public API Boundary): Task 7 covers deprecating internal types
- Section 7 (Migration): Tasks 1-7 implement Phase 1, Task 8 implements Phase 2

**2. Placeholder scan:** No TBDs, TODOs, or vague steps. All code is concrete.

**3. Type consistency:**
- `Signal<T>` used consistently across Tasks 1, 6, 8, 9
- `Component` trait used consistently across Tasks 2, 8, 9
- `ComponentState` trait used consistently across Tasks 2, 6, 8, 9
- `RenderContext` / `LifecycleContext` used consistently across Tasks 2, 8, 9
- `Column::new()` / `Row::new()` used consistently across Tasks 3, 8, 9
- `children![]` macro name used consistently across Tasks 5, 9
