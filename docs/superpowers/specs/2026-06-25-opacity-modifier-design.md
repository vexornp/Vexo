# Opacity Modifier Design

## Problem

Vexo has no way to make a subtree semi-transparent. Individual colors can set their alpha component, but there is no group opacity that affects an entire widget subtree. This is needed for fade transitions, disabled states, overlays, and visual feedback.

## Decision

Add a dedicated `Opacity` wrapper widget with a `.opacity()` modifier on the `Widget` trait. Rendering uses CPU-side alpha multiplication — the same approach Transform uses for baking transforms into quad instances. No GPU infrastructure changes.

**Why not a Style field:** Opacity affects an entire subtree, not a single decoration layer. A Style field on DecoratedContainer would conflate decoration with subtree opacity and couldn't compose with other DecoratedContainers without nesting. A dedicated widget is cleaner and matches the Transform pattern.

**Why CPU multiplication over GPU render-to-texture:** Render-to-texture requires offscreen render targets, texture management, and a new shader pass — massive complexity for v1. CPU alpha multiplication is simple, consistent with the existing Transform pattern, and the WgpuBackend already uses alpha blending so multiplied alpha values blend correctly.

## Widget & Element

### Opacity Widget (`vexo/src/widgets/opacity.rs`)

```rust
pub struct Opacity {
    key: Option<WidgetKey>,
    child: Box<dyn Widget>,
    opacity: f32,  // 0.0 (invisible) to 1.0 (fully opaque), clamped
}

impl Opacity {
    pub fn new(child: impl Widget, opacity: f32) -> Self {
        Self {
            key: None,
            child: Box::new(child),
            opacity: opacity.clamp(0.0, 1.0),
        }
    }
}
```

Builder methods: `.key()`, `.opacity()` (update value).

Widget impl: `create_element()` returns `OpacityElement`, `create_render_object()` returns `OpacityRenderObject`.

### Widget Trait Modifier (`vexo/src/widgets/mod.rs`)

```rust
fn opacity(self, value: f32) -> Box<dyn Widget> {
    Box::new(Opacity::new(self, value))
}
```

### OpacityElement (`vexo/src/elements/opacity.rs`)

Single-child element following the `RenderObjectElement` pattern (same as Transform). Stores the opacity value, passes it to the render object on create and update.

## Render Object & Pipeline

### RenderObject Trait Addition (`vexo/src/render_object.rs`)

```rust
fn opacity(&self) -> Option<f32> { None }
```

Default returns `None`. `OpacityRenderObject` overrides to return `Some(self.opacity)`.

### OpacityRenderObject (`vexo/src/render_objects/opacity.rs`)

```rust
pub struct OpacityRenderObject {
    child: Option<RenderObjectKey>,
    opacity: f32,
    computed_bounds: Option<Bounds<Logical>>,
    layout_node: Option<LayoutNodeKey>,
}
```

- `paint()` returns `vec![]` (no visible content — the opacity wrapper produces no geometry)
- `opacity()` returns `Some(self.opacity)`
- Layout: creates a Taffy node, lays out child at own size (opacity doesn't affect layout)

### New RenderCommand Variants (`vexo/src/render/command.rs`)

```rust
PushOpacity { opacity: f32 },
PopOpacity,
```

### Painter (`vexo/src/painter.rs`)

In `paint_recursive()`, after checking `paint_transform()` and `clip_bounds()`, check `obj.opacity()`. If `Some(value)`, emit `PushOpacity { opacity: value }` before children and `PopOpacity` after children.

### CommandProcessor (`vexo/src/render/command_processor.rs`)

Add `current_opacity: f32` initialized to `1.0` and `opacity_stack: Vec<f32>`.

- `PushOpacity { opacity }` → push `current_opacity`, set `current_opacity = current_opacity * opacity`
- `PopOpacity` → pop previous opacity from stack
- When processing `Rect`, `Caret`: multiply `fill` and `stroke` color alpha by `current_opacity`
- When processing `Text`: multiply `color` alpha by `current_opacity`
- When processing `Image`: the image shader uses its own alpha; multiply into the image request's alpha field

This is the single place where alpha multiplication happens. FrameBuilder receives already-multiplied colors.

## Edge Cases

- **Clamping:** Values clamped to `[0.0, 1.0]` at widget construction time.
- **Zero opacity:** Fully invisible but still laid out and hit-testable. Matches Flutter `Opacity` and CSS `opacity: 0` behavior. If invisible + non-interactive is desired, the user handles gestures separately.
- **Hit testing:** Opacity does NOT affect hit testing. Pointer events pass through to children regardless of opacity value.
- **Nested opacity:** Values multiply through the stack. `.opacity(0.5).opacity(0.5)` = 0.25 effective opacity. Handled automatically by stack multiplication.
- **Text rendering:** Alpha multiplication happens via the text color's alpha component. Subpixel rendering artifacts at very low opacity are acceptable for v1.

## Testing

- Unit test: OpacityRenderObject returns correct opacity value
- Unit test: opacity stack multiplication in CommandProcessor- Integration test: Opacity widget produces PushOpacity/PopOpacity in render commands
- Integration test: nested opacity values multiply correctly
- Integration test: zero-opacity widget still produces layout (non-zero size)
- Visual test via desktop demo with opacity examples

## Files to Create/Modify

### New files
- `vexo/src/widgets/opacity.rs` — Opacity widget
- `vexo/src/elements/opacity.rs` — OpacityElement
- `vexo/src/render_objects/opacity.rs` — OpacityRenderObject

### Modified files
- `vexo/src/widgets/mod.rs` — add `opacity()` modifier to Widget trait, register Opacity widget
- `vexo/src/elements/mod.rs` — register OpacityElement
- `vexo/src/render_objects/mod.rs` — register OpacityRenderObject
- `vexo/src/render_object.rs` — add `opacity()` default method to RenderObject trait
- `vexo/src/render/command.rs` — add `PushOpacity`/`PopOpacity` variants
- `vexo/src/painter.rs` — emit PushOpacity/PopOpacity around children when `obj.opacity()` returns Some
- `vexo/src/render/command_processor.rs` — add opacity stack, multiply alpha into colors
- `vexo/src/lib.rs` — re-export Opacity
