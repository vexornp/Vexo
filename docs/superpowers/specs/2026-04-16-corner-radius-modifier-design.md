# Corner Radius Widget Modifier Design Spec

**Date:** 2026-04-16
**Status:** Draft
**Author:** Claude

## Problem Statement

The `CornerRadius` modifier exists in `vexo/src/widgets/modifiers.rs` but does not render rounded corners. The struct stores a radius value and delegates to its child without applying any visual effect. The shader receives the `corner_radius` value via `QuadInstance` but ignores it in the fragment shader.

Users expect `.corner_radius(8.0)` to produce visually rounded corners on backgrounds and borders.

## Goals

1. Render rectangles with rounded corners using the existing `corner_radius` field
2. Support anti-aliasing for smooth, high-quality corner edges
3. Maintain the SwiftUI-style modifier chaining API

## Non-Goals

- Per-corner radius control (all four corners use the same radius)
- Elliptical corners (only circular corners)
- Shadow effects (out of scope)
- Stencil-based clipping of child content (child widgets can still draw outside rounded bounds; this would require stencil buffer operations and is a future enhancement)

## Architecture

### Approach: Renderer Context State

Add corner radius as context state in `UiBatcher`. The `CornerRadius` modifier sets the radius before drawing its child, and any `add_rect` calls within that scope use the current radius.

**Why this approach:**
- Minimal changes to existing widget code
- Background and Border modifiers automatically pick up the radius
- No need to modify `Widget::draw()` signature
- Clean separation of concerns

### Data Flow

```
.corner_radius(8.0)
    ↓
CornerRadius::draw()
    ↓
renderer.push_corner_radius(8.0)  // Set context
child.draw()                       // Child draws, add_rect uses radius
renderer.pop_corner_radius()       // Reset
    ↓
Background::draw() calls add_rect(..., current_radius)
    ↓
Shader renders rounded rect using SDF
```

## Implementation Details

### 1. Renderer Context State

**File:** `vexo/src/renderer.rs`

Add a stack-based corner radius context to `UiBatcher`:

```rust
pub struct UiBatcher {
    // ... existing fields ...
    corner_radius_stack: Vec<f32>,  // Stack for nested radius contexts
}

impl UiBatcher {
    pub fn push_corner_radius(&mut self, radius: f32) {
        self.corner_radius_stack.push(radius);
    }

    pub fn pop_corner_radius(&mut self) {
        self.corner_radius_stack.pop();
    }

    pub fn current_corner_radius(&self) -> f32 {
        self.corner_radius_stack.last().copied().unwrap_or(0.0)
    }
}
```

Modify `add_rect` to use the current corner radius:

```rust
pub fn add_rect(
    &mut self,
    pos: [f32; 2],
    size: [f32; 2],
    color: impl Into<Color>,
    border_color: impl Into<Color>,
    border_width: f32,
    corner_radius: f32,  // Keep parameter for explicit override
) {
    // Use explicit radius if > 0, otherwise use context
    let radius = if corner_radius > 0.0 {
        corner_radius
    } else {
        self.current_corner_radius()
    };
    // ... rest of implementation
}
```

### 2. CornerRadius Modifier

**File:** `vexo/src/widgets/modifiers.rs`

Update `CornerRadius::draw()` to set the renderer context:

```rust
fn draw(
    &self,
    taffy: &mut taffy::TaffyTree,
    node: NodeId,
    renderer: &mut UiBatcher,
    offset: Point<Logical>,
    focused_id: Option<WidgetId>,
    cursor_blink: &crate::CursorBlinkState,
    ctx: &mut WidgetContext,
) {
    renderer.push_corner_radius(self.radius);
    self.child.draw(taffy, node, renderer, offset, focused_id, cursor_blink, ctx);
    renderer.pop_corner_radius();
}
```

### 3. Shader SDF Implementation

**File:** `vexo/src/shader.wgsl`

Add corner radius to the vertex output and implement SDF rounded rect in fragment shader:

```wgsl
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) border_width: f32,
    @location(4) size: vec2<f32>,
    @location(5) corner_radius: f32,  // ADD THIS
};

@vertex
fn vs_main(
    @location(0) model_pos: vec2<f32>,
    @location(1) inst_pos: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_color: vec4<f32>,
    @location(4) inst_border_color: vec4<f32>,
    @location(5) inst_border_width: f32,
    @location(6) inst_corner_radius: f32,  // ADD THIS
) -> VertexOutput {
    // ... existing vertex transform code ...
    out.corner_radius = inst_corner_radius * globals.scale_factor;  // Convert to physical pixels
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // SDF for rounded rectangle
    let radius = min(in.corner_radius, min(in.size.x, in.size.y) * 0.5);

    // Calculate distance from rounded rect edge
    // UV is 0-1, convert to pixel coordinates relative to center
    let pixel_pos = in.uv * in.size;
    let half_size = in.size * 0.5;
    let center_pos = pixel_pos - half_size;

    // SDF: distance from rounded rectangle edge
    // For a rounded rect, the distance is computed by considering
    // the corner circles and the rectangular center
    let inner_dist = abs(center_pos) - (half_size - radius);
    let corner_dist = length(max(inner_dist, vec2<f32>(0.0))) - radius;
    let sdf = min(max(inner_dist.x, inner_dist.y), 0.0) + corner_dist;

    // Alpha based on SDF (1px anti-aliasing)
    let fill_alpha = 1.0 - smoothstep(-1.0, 1.0, sdf);

    // Apply fill color with rounded rect clipping
    let fill_color = vec4<f32>(in.color.rgb, in.color.a * fill_alpha);

    // Border calculation (similar approach with inner SDF)
    let border_px = in.border_width;
    let inner_sdf = sdf + border_px;
    let border_alpha = 1.0 - smoothstep(-1.0, 1.0, inner_sdf);

    // Mix fill and border
    let border_color = vec4<f32>(in.border_color.rgb, in.border_color.a * border_alpha * fill_alpha);

    return mix(fill_color, border_color, border_alpha * fill_alpha);
}
```

### 4. Update Existing add_rect Calls

**Files:** `vexo/src/widgets/button.rs`, `vexo/src/widgets/color_widget.rs`, `vexo/src/widgets/modifiers.rs`

Update all `add_rect` calls to pass `0.0` for corner_radius (will use context value):

```rust
// Before
renderer.add_rect(pos.to_array(), size.to_array(), self.color, Color::TRANSPARENT, 0.0);

// After (add corner_radius parameter)
renderer.add_rect(pos.to_array(), size.to_array(), self.color, Color::TRANSPARENT, 0.0, 0.0);
```

## File Changes Summary

| File | Change |
|------|--------|
| `vexo/src/renderer.rs` | Add `corner_radius_stack`, push/pop methods, update `add_rect` |
| `vexo/src/shader.wgsl` | Add `corner_radius` to vertex output, implement SDF in fragment |
| `vexo/src/widgets/modifiers.rs` | Update `CornerRadius::draw()` to use push/pop, update `add_rect` calls |
| `vexo/src/widgets/button.rs` | Update `add_rect` call signature |
| `vexo/src/widgets/color_widget.rs` | Update `add_rect` call signature |

## Testing

### Visual Verification

Run `cargo run -p desktop_demo` and verify:

1. **Basic rounded background:**
   ```rust
   text!("Hello")
       .background(Color::RED)
       .corner_radius(10.0)
   ```
   Expected: Red background with 10px rounded corners

2. **Rounded border:**
   ```rust
   text!("Hello")
       .border(Color::BLUE, 2.0)
       .corner_radius(8.0)
   ```
   Expected: Blue border with rounded corners

3. **Combined modifiers:**
   ```rust
   text!("Hello")
       .padding(10.0)
       .background(Color::GREEN)
       .border(Color::BLACK, 2.0)
       .corner_radius(12.0)
   ```
   Expected: Green background with black border, both rounded

4. **Nested corner radius:**
   ```rust
   column![
       text!("Outer")
           .background(Color::RED)
           .corner_radius(20.0),
       text!("Inner")
           .background(Color::BLUE)
           .corner_radius(5.0),
   ]
   ```
   Expected: Each element has its own corner radius

### Edge Cases

- Radius larger than half the smallest dimension: Should clamp to half
- Zero radius: Should render as square rectangle
- Very small radius (1-2px): Should still show subtle rounding

## Success Criteria

1. `.corner_radius(N)` produces visually rounded corners on backgrounds and borders
2. Anti-aliasing produces smooth edges (no jagged pixels)
3. Existing widgets continue to work without modification
4. Modifier chaining order is flexible (corner_radius can appear anywhere in chain)

## Future Enhancements

- **Content clipping:** Use stencil buffer to clip child widgets to rounded bounds. This would prevent text or other content from overflowing past rounded corners.
