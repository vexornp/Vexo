# Merge Column + Row into Flex Widget

## Problem

`Column` and `Row` are structurally identical widgets that differ only in their default `Layout` — specifically the `flex_direction` field. Both use the same element type (`ContainerElement`), same render object type (`ContainerRenderObject`), and same builder methods. This is unnecessary duplication.

Meanwhile, CSS flexbox supports `RowReverse` and `ColumnReverse` directions that have no widget representation at all.

## Decision

Merge `Column` and `Row` into a single `Flex` widget. Preserve ergonomic macros (`column![]`, `row![]`) as sugar over `Flex` constructors.

Grid remains a separate widget — its layout concerns (template rows/columns, grid placement, auto-flow) are fundamentally different from flex.

## Design

### Flex Widget

```rust
pub struct Flex {
    key: Option<WidgetKey>,
    children: Vec<Box<dyn Widget>>,
    layout: Layout,
}
```

Same shape as the current `Column`/`Row`. Uses `ContainerElement` and `ContainerRenderObject` unchanged.

### Constructors

- `Flex::new()` — `FlexDirection::Row` + `AlignItems::Stretch` (same as `Flex::row()`)
- `Flex::column()` — `FlexDirection::Column` + `AlignItems::Stretch` (current Column defaults)
- `Flex::row()` — `FlexDirection::Row` + `AlignItems::Stretch` (current Row defaults)

`Flex::new()` is an alias for `Flex::row()` — both produce identical results. `new()` exists for the common case (row is the CSS default), `row()` exists for explicit readability.

### Builder Methods

Same as current Column/Row:

- `.push(child)` — append a child
- `.layout(layout)` — override the entire `Layout`
- `.key(key)` — set widget key

### Macros

```rust
column![child1, child2]  // → Flex::column().push(child1).push(child2)
row![child1, child2]     // → Flex::row().push(child1).push(child2)
```

Source-compatible with current usage. Only internal implementation changes.

### What Gets Deleted

- `Column` struct and its `impl` blocks
- `Row` struct and its `impl` blocks
- Re-exports of `Column` and `Row` from `vexo/src/lib.rs`

### What Stays Unchanged

- `Grid` widget — separate, not affected
- `ContainerElement` — same element type for Flex
- `ContainerRenderObject` — same render object type for Flex
- `Layout` struct — no changes
- `WithLayout` widget — no changes
- `column!` and `row!` macros — same external API, different internals

### Public API Change

Before:
```rust
pub use widgets::{Column, Row, ...};
```

After:
```rust
pub use widgets::{Flex, ...};
```

`FlexDirection` should also be re-exported from the crate root for discoverability:
```rust
pub use layout::FlexDirection;
```

### Migration

| Before | After |
|--------|-------|
| `Column::new()` | `Flex::column()` |
| `Row::new()` | `Flex::row()` |
| `column![...]` | `column![...]` (unchanged) |
| `row![...]` | `row![...]` (unchanged) |
| `Column::new().push(a).push(b)` | `Flex::column().push(a).push(b)` |

## Files to Modify

1. `vexo/src/widgets/container.rs` — delete Column/Row, add Flex
2. `vexo/src/widgets/mod.rs` — update re-exports (Flex instead of Column/Row)
3. `vexo/src/macros.rs` — update column!/row! to use Flex constructors
4. `vexo/src/lib.rs` — update public API exports (Flex, FlexDirection)
5. `shared_app/src/lib.rs` — migrate to Flex API
6. `desktop_demo/src/main.rs` — migrate to Flex API (if applicable)
7. Tests — migrate any test code using Column/Row
