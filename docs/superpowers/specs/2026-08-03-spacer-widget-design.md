# Spacer Widget Design

**Date:** 2026-08-03
**Status:** Approved

## Context

`shared_app/src/chats/chat_screen.rs` uses `MultiChild::empty(Layout::default().flex_grow(1.0))`
in two places (lines 205 and 216) as a flexible spacer inside a `row!` to push
the chat bubble to one side.

This is a category error: `MultiChild` is a multi-child *container* (it owns a
`Vec<Box<dyn Widget>>`, a `ContainerElement` that reconciles children, and a
`ContainerRenderObject` that manages child layout nodes). Using it to express
"I take a share of free space" pays for machinery that is never exercised and
hides the intent behind an implementation detail.

The `row!`/`column!` macros cannot express this either: they hardcode
`Layout::row()` / `Layout::column()` and call `MultiChild::new(children,
layout)`, with no way to inject a custom layout like
`Layout::default().flex_grow(1.0)`.

## Goal

Add a dedicated leaf `Spacer` widget whose entire purpose is to claim a share
of the parent's free space. It paints nothing, hits nothing, has no children,
and carries a constant `flex_grow(1.0)`.

## Design

### Three-tree mapping

- **Widget** `Spacer` (`vexo/src/widgets/spacer.rs`) — leaf widget. Reuses
  `LeafRenderObjectElement` (same pattern as `Text`). Only field is
  `key: Option<WidgetKey>`. The flex factor is a compile-time `1.0` baked into
  the render object; there is no configurable parameter.
- **Element** — reuse `LeafRenderObjectElement`. No new element type.
- **Render object** `SpacerRenderObject` (`vexo/src/render_objects/spacer.rs`)
  — owns one Taffy leaf node created with
  `Layout::default().flex_grow(1.0)`. `paint()` returns `vec![]`,
  `hit_test()` returns `false`, `children()` returns `&[]`. Tracks
  `owned_node: Option<LayoutNodeKey>` and `computed_bounds: Option<Bounds<Logical>>`
  following the same shape as `OffstageRenderObject`'s offstage branch, but
  simpler (no flag, no child).

### Public API

```rust
pub struct Spacer {
    key: Option<WidgetKey>,
}

impl Spacer {
    pub fn new() -> Self { Self { key: None } }
    pub fn with_key(mut self, key: impl Into<WidgetKey>) -> Self { ... }
}

impl Widget for Spacer {
    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = LeafElement::new();
        elem.set_widget(self);
        Box::new(elem)
    }
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(SpacerRenderObject::new())
    }
    fn update_render_object(&self, _ro: &mut dyn RenderObject) -> UpdateResult {
        UpdateResult::NONE  // flex factor is constant; nothing to diff
    }
    // ...standard leaf impl (key, as_any, clone_boxed)
}

impl Default for Spacer { fn default() -> Self { Self::new() } }
impl Clone for Spacer { ... }  // derive
```

### Behavior

- **Direction-agnostic.** Sets only `flex_grow`. The parent's `flex_direction`
  decides which axis the spacer grows along — this is exactly why
  `Layout::default()` (not `Layout::row()` / `Layout::column()`) is the
  correct layout to bake in.
- **Even splits.** Multiple spacers split free space evenly (standard Taffy
  flex distribution).
- **No `min-size: auto` gotcha.** An empty leaf has zero content size, so
  Taffy's flex `min-size: auto` default cannot trap the spacer at a non-zero
  minimum.

### Exports

- `vexo/src/widgets/mod.rs`: add `mod spacer;` and `pub use spacer::Spacer;`.
- `vexo/src/render_objects/mod.rs`: add `mod spacer;` and
  `pub use spacer::SpacerRenderObject;`.
- `vexo/src/lib.rs`: add `Spacer` to the `pub use widgets::{...}` list
  (alphabetical slot between `SlideTransition` and `Stack`).

### Migration

Two call sites in `shared_app/src/chats/chat_screen.rs`:

```rust
// Before
MultiChild::empty(Layout::default().flex_grow(1.0))

// After
Spacer::new()
```

After replacement, audit the imports on line 8: `MultiChild` is only referenced
by these two call sites and should be removed from the import list. `Layout`
may still be referenced elsewhere in the file — verify with the compiler and
remove only what is now unused.

## Testing

### Render object unit tests (`vexo/src/render_objects/spacer.rs`)

- `layout()` creates a leaf node, reports it via `LayoutResult.node`, and
  stores it in `layout_node()`.
- The created node has `flex_grow == 1.0` set on it (assert via
  `TaffyLayoutEngine` inspection or by computing layout and confirming the
  spacer absorbs free space).
- `paint()` returns an empty `Vec`.
- `hit_test()` returns `false` for any point.
- `children()` returns `&[]`.
- `apply_layout()` populates `computed_bounds()` after `engine.compute()`.

### Widget unit tests (`vexo/src/widgets/spacer.rs`)

- `Spacer::new()` creates a render object that downcasts to
  `SpacerRenderObject`.
- `update_render_object` returns `UpdateResult::NONE`.
- `with_key` round-trips through `key()`.

### Integration test (`vexo/tests/spacer.rs`)

End-to-end test mirroring the chat_screen use case. Build a `row!` containing
`[Spacer::new(), Text::new("bubble")]` inside a fixed-width parent. After
layout, assert:

- The text's computed bounds sit on the right edge of the parent.
- The spacer's computed bounds fill the remaining width on the left.
- Total width of spacer + text equals parent width.

This exercises the full three-tree path (widget → element → render object →
Taffy) and is the evidence that the migration is behaviorally equivalent to
the old `MultiChild::empty(Layout::default().flex_grow(1.0))`.

## Out of scope

- Configurable flex factor (`Spacer::new(flex)` or `.with_flex(2.0)`). If a
  second factor is ever needed, it is a trivial additive change. YAGNI for now.
- Full `Layout` passthrough (`.flex_shrink()`, `.align_self()`, etc.). Same
  rationale — add when a concrete need appears.
- A `spacer!` macro token inside `row!`/`column!`. Spacer is a widget concern,
  not a macro concern; `Spacer::new()` is already as terse as a macro would be.
