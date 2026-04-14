# Unified Color System Design

**Date:** 2026-04-14

## Context

The Vexo framework currently uses multiple color representations across the codebase:
- `[f32; 3]` - RGB in widgets (Rectangle, Button, Text, TextEdit)
- `[f32; 4]` - RGBA in renderer (QuadInstance, UiBatcher, TextRequest)
- `wgpu::Color` - RGBA f64 for GPU clear color
- `cosmic_text::Color` / `glyphon::Color` - RGBA u8 for text rendering

This causes confusion and requires manual conversions at boundaries (e.g., adding alpha=1.0 when passing widget colors to renderer, converting f32 to u8 for text).

## Goal

Create a single `Color` type that:
1. Provides a consistent API across all framework code
2. Converts seamlessly to/from all external color types
3. Stores RGBA internally as f32 (0.0-1.0)
4. Improves code clarity and reduces conversion bugs

## Design

### Color Struct

```rust
// vexo/src/color.rs

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
```

### Constructors

```rust
impl Color {
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self;
    pub fn rgb(r: f32, g: f32, b: f32) -> Self;  // alpha = 1.0
    pub fn from_hex(hex: u32) -> Self;           // 0xRRGGBB or 0xRRGGBBAA
}
```

### Preset Colors

```rust
impl Color {
    pub const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const RED: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const GREEN: Color = Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
    pub const BLUE: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
}
```

### Conversion Traits

```rust
// From external types to Color
impl From<wgpu::Color> for Color;
impl From<[f32; 3]> for Color;  // alpha defaults to 1.0
impl From<[f32; 4]> for Color;
impl From<cosmic_text::Color> for Color;

// From Color to external types
impl From<Color> for wgpu::Color;
impl From<Color> for [f32; 4];
impl From<Color> for cosmic_text::Color;
```

### Helper Methods

```rust
impl Color {
    pub fn to_array(&self) -> [f32; 4];
    pub fn to_wgpu(&self) -> wgpu::Color;
    pub fn to_cosmic(&self) -> cosmic_text::Color;
    pub fn with_alpha(&self, a: f32) -> Self;
}
```

## Files to Modify

| File | Change |
|------|--------|
| `vexo/src/color.rs` | **New** - Color struct with all implementations |
| `vexo/src/lib.rs` | Export `Color`, update `CLEAR_COLOR` constant |
| `vexo/src/widgets/rectangle.rs` | Change `color: [f32; 3]` to `color: Color` |
| `vexo/src/widgets/button.rs` | Change `background_color: [f32; 3]` to `Color` |
| `vexo/src/widgets/text.rs` | Change `color: [f32; 3]` and `text_color: [f32; 3]` to `Color` |
| `vexo/src/renderer.rs` | Accept `Color` in `add_rect()` and `add_text()`, convert internally |
| `vexo/src/quad_instance.rs` | Keep `[f32; 4]` for GPU layout, add `From<Color>` |
| `vexo/src/macros.rs` | Update macros to accept Color expressions |

## Migration

1. Create `color.rs` with full Color implementation
2. Export from `lib.rs` and update `CLEAR_COLOR`
3. Update widget APIs (breaking change - accept Color)
4. Update renderer to accept Color and convert to needed formats
5. Update macros to work with Color
6. Update `shared_app/src/lib.rs` demo

## Verification

1. `cargo build -p vexo` compiles without errors
2. `cargo run -p desktop_demo` displays correctly with colors
3. `./build_for_ios.sh` succeeds
