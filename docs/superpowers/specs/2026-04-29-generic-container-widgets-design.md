# Generic Container Widgets for Retain Mode

## Context

The retain mode container widgets (`Column` and `Row`) are currently hardcoded to `Widget<()>`, preventing composition with message-emitting widgets like `Button<M>`. This design makes containers generic over message type `M` to enable proper widget composition.

## Goal

Make `Column` and `Row` widgets generic over message type `M`, following the pattern established by `Button<M>` and `GestureDetector<M>`.

## Design

### Changes to Column and Row structs

```rust
pub struct Column<M: Clone + Send + 'static = ()> {
    key: Option<Key>,
    children: Vec<Box<dyn Widget<M>>>,
}

pub struct Row<M: Clone + Send + 'static = ()> {
    key: Option<Key>,
    children: Vec<Box<dyn Widget<M>>>,
}
```

### Method updates

- `new()` → returns `Self` with default `M = ()`
- `push(child: impl Widget<M> + 'static)` → accepts generic children
- `with_key(key)` → unchanged
- Implement `Clone` manually via `clone_box()` on children

### Widget<M> implementation

- `create_element()` → create `ContainerElement::<M>::new()`
- `children()` → return `&self.children`
- `clone_box()` → return `Box::new(self.clone())`
- `as_any()` → return `self`

### Default type parameter

The default `= ()` preserves backward compatibility:
- `Column::new()` still works as `Column<()>`
- Existing code using `Column` without type annotation continues to work

## Files to Modify

- `vexo/src/retain/widgets/container.rs`

## Verification

1. Run `cargo build -p vexo` to verify compilation
2. Run `cargo test -p vexo` to verify existing tests pass

## Out of Scope

- Adapting `Text` and other `Widget<()>` leaf widgets to work inside generic containers (deferred)