# Port Border and CornerRadius Modifiers Design

**Date:** 2026-04-28
**Status:** Design Approved

## Goal

Port the `Border` and `CornerRadius` modifier widgets from immediate-mode to retain-mode, following the pattern established by `Background`.

## Architecture

Both widgets follow the same three-tree pattern as Background:

```
Widget (immutable config) → Element (stateful lifecycle) → RenderObject (layout/paint)
```

Each modifier:
- Uses `ModifierElement` for element tree (already supports any widget with `child()` method)
- Implements `Widget::child()` to expose child widget
- Implements `RenderObject` with `set_child_id()` and `children()` for render tree linking

## Components

### 1. Border Widget

**File:** `vexo/src/retain/widgets/border.rs`

```rust
pub struct Border {
    key: Option<Key>,
    child: Box<dyn Widget>,
    color: Color,
    width: f32,
}

pub struct BorderRenderObject {
    color: Color,
    width: f32,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
}
```

**Behavior:**
- Layout: Uses child's size (same as Background)
- Paint: Returns border rect command (transparent fill, colored stroke)
- Paint order: Child paints first, then border on top

**RenderCommand:** `RenderCommand::rect_with_border(bounds, TRANSPARENT, color, width)`

### 2. CornerRadius Widget

**File:** `vexo/src/retain/widgets/corner_radius.rs`

```rust
pub struct CornerRadius {
    key: Option<Key>,
    child: Box<dyn Widget>,
    radius: f32,
}

pub struct CornerRadiusRenderObject {
    radius: f32,
    child: Option<RenderObjectId>,
    computed_bounds: Option<Bounds<Logical>>,
}
```

**Behavior:**
- Layout: Uses child's size
- Paint: Returns push/pop commands wrapping child's commands
- Paint order: Push radius → child paints → Pop radius

**RenderCommands:**
- `RenderCommand::PushCornerRadius { radius }`
- `RenderCommand::PopCornerRadius`

## Files to Create

- `vexo/src/retain/widgets/border.rs` - Border widget + BorderRenderObject
- `vexo/src/retain/widgets/corner_radius.rs` - CornerRadius widget + CornerRadiusRenderObject

## Files to Modify

- `vexo/src/retain/widgets/mod.rs` - Add Border, CornerRadius exports
- `vexo/src/retain/mod.rs` - Add to widgets re-export

## Tests

**Border:**
- `test_border_widget_creation` - Widget construction
- `test_border_widget_with_key` - Key handling
- `test_border_creates_render_object` - Render object creation and paint

**CornerRadius:**
- `test_corner_radius_widget_creation` - Widget construction
- `test_corner_radius_widget_with_key` - Key handling
- `test_corner_radius_creates_render_object` - Render object creation and paint

**Integration:**
- `test_border_widget_in_pipeline` - End-to-end pipeline test
- `test_corner_radius_widget_in_pipeline` - End-to-end pipeline test

## Implementation Order

1. Port Border widget (simpler, similar to Background)
2. Port CornerRadius widget (push/pop pattern)
3. Add integration tests for both

## Out of Scope

- Padding modifier (not in immediate-mode yet)
- Opacity, Shadow, or other advanced modifiers
- Chained modifier optimization
