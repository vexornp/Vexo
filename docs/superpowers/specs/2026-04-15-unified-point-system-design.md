# Unified Point System Design

**Date:** 2026-04-15
**Status:** Approved

## Context

Vexo uses both logical (DPI-independent points) and physical (screen pixels) coordinates throughout the codebase. Currently there's no unified type system:

- `(f32, f32)` tuples for widget offsets
- `[f32; 2]` arrays for GPU buffers
- `taffy::Point<f32>` for layout engine
- Individual `x, y, width, height` fields in `Bounds`
- Implicit conversions via inline `* scale_factor`

This leads to:
1. **No compile-time safety** - easy to mix logical and physical values
2. **Scattered conversion logic** - `* scale_factor` repeated everywhere
3. **Inconsistency bug** - ColorWidget does double scaling (converts to physical before passing to add_rect, but shader also converts)

## Design

### Core Types

```rust
// Marker types (zero-sized, compile-time only)
pub struct Logical;
pub struct Physical;

// Generic point
pub struct Point<T> {
    pub x: f32,
    pub y: f32,
}

// Generic size
pub struct Size<T> {
    pub width: f32,
    pub height: f32,
}

// Generic rectangle
pub struct Rect<T> {
    pub origin: Point<T>,
    pub size: Size<T>,
}
```

### Conversions

```rust
impl Point<Logical> {
    pub fn to_physical(self, scale: f32) -> Point<Physical> {
        Point { x: self.x * scale, y: self.y * scale }
    }
}

impl Point<Physical> {
    pub fn to_logical(self, scale: f32) -> Point<Logical> {
        Point { x: self.x / scale, y: self.y / scale }
    }
}

// Same pattern for Size<T> and Rect<T>
```

### Scale Type (Enhanced)

The existing `Scale` wrapper in `utils.rs` stays but gains helper methods:

```rust
pub struct Scale(f64);

impl Scale {
    pub fn new(factor: f64) -> Self;
    pub fn factor(&self) -> f32;  // Already exists
}
```

### Taffy Interop

```rust
impl Point<Logical> {
    pub fn from_taffy(p: taffy::Point<f32>) -> Self {
        Point { x: p.x, y: p.y }
    }

    pub fn to_taffy(self) -> taffy::Point<f32> {
        taffy::Point { x: self.x, y: self.y }
    }
}

impl Size<Logical> {
    pub fn from_taffy(s: taffy::Size<f32>) -> Self;
    pub fn to_taffy(self) -> taffy::Size<f32>;
}
```

### GPU Buffer Interop

`QuadInstance` still uses `[f32; 2]` for GPU compatibility, but gains helpers:

```rust
impl QuadInstance {
    pub fn from_logical(pos: Point<Logical>, size: Size<Logical>, ...) -> Self;
}
```

### PhysicalLocation Update

The existing `PhysicalLocation` becomes a thin wrapper:

```rust
pub struct PhysicalLocation(Point<Physical>);

impl PhysicalLocation {
    pub fn from_winit(pos: winit::dpi::PhysicalPosition<f64>) -> Self;
    pub fn to_logical(self, scale: &Scale) -> Point<Logical>;
}
```

## Implementation Plan

### Phase 1: Add Types (utils.rs)

1. Add `Logical`, `Physical` marker structs
2. Add `Point<T>`, `Size<T>`, `Rect<T>` with all conversions
3. Add Taffy interop methods
4. Update `PhysicalLocation` to use `Point<Physical>`

### Phase 2: Update Renderer (renderer.rs, quad_instance.rs)

1. Replace `Bounds` with `Rect<Logical>`
2. Update `UiBatcher::add_rect()` signature to take `Point<Logical>`, `Size<Logical>`
3. Update `UiBatcher::add_text()` signature
4. Add `QuadInstance::from_logical()` helper

### Phase 3: Update Render Loop (lib.rs)

1. Update text rendering to use `Point<Logical>` with `.to_physical(scale)`
2. Update editor rendering similarly
3. Remove inline `* scale_factor` conversions

### Phase 4: Update Widgets

1. **ColorWidget** - FIX BUG: remove physical conversion, pass logical coordinates
2. **Column/Row** - Use `Point<Logical>` for offset accumulation
3. **Button** - Use typed points
4. **Text** - Use typed points
5. **TextEdit** - Use typed points

### Phase 5: Cleanup

1. Remove unused `Bounds` struct
2. Remove inline conversion patterns
3. Update any remaining `(f32, f32)` usages

## Files Modified

| File | Changes |
|------|---------|
| `vexo/src/utils.rs` | Add Point, Size, Rect types; update PhysicalLocation |
| `vexo/src/renderer.rs` | Update Bounds → Rect, UiBatcher signatures |
| `vexo/src/quad_instance.rs` | Add from_logical helper |
| `vexo/src/lib.rs` | Update render loop, remove inline conversions |
| `vexo/src/widgets/color_widget.rs` | Fix double-scaling bug |
| `vexo/src/widgets/containers.rs` | Use Point for offsets |
| `vexo/src/widgets/button.rs` | Use typed points |
| `vexo/src/widgets/text.rs` | Use typed points |
| `vexo/src/widgets/text_edit.rs` | Use typed points |

## Verification

1. `cargo build -p vexo --release` - Must compile without warnings
2. `cargo run -p desktop_demo` - Visual check: widgets render correctly
3. Test on different scale factors (Retina display) - no double-scaling artifacts
4. iOS build: `./build_for_ios.sh` - Must compile

## Out of Scope

- Changing shader coordinate system (shader expects logical, converts to physical)
- Changing winit integration (winit provides physical positions)
- Changing Taffy integration (Taffy uses logical coordinates)
