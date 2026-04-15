# ViewModifier System Design

**Date:** 2026-04-15
**Status:** Approved

## Context

Vexo currently handles styling directly on each widget struct via builder methods. This leads to:
- Duplicated styling logic across widgets
- No way to apply common styling to arbitrary widgets
- Inconsistent styling APIs between widget types

SwiftUI's ViewModifier pattern provides a clean solution: modifiers wrap views to add styling or behavior, with a fluent chaining API.

## Goal

Add a ViewModifier-like system to vexo that allows applying visual decorators (padding, background, border, corner radius) to any widget via SwiftUI-style chaining.

## Design

### Architecture

Three components:

1. **`WidgetExt<M>` trait** - Extension trait providing the chaining API
2. **Modifier widgets** - Individual wrapper widgets (`Padding`, `Background`, `Border`, `CornerRadius`)
3. **Blanket impl for `Box<dyn Widget<M>>`** - Enables modifiers on macro-produced widgets

Each modifier is a `Widget<M>` implementation that wraps a child widget and modifies its layout or drawing.

### WidgetExt Trait

```rust
pub trait WidgetExt<M: Clone + Debug + Send>: Widget<M> + Sized {
    fn padding(self, amount: f32) -> Padding<Self, M>;
    fn padding_horizontal(self, horizontal: f32) -> Padding<Self, M>;
    fn padding_vertical(self, vertical: f32) -> Padding<Self, M>;
    fn background(self, color: Color) -> Background<Self, M>;
    fn border(self, color: Color, width: f32) -> Border<Self, M>;
    fn corner_radius(self, radius: f32) -> CornerRadius<Self, M>;
}

impl<M, W: Widget<M>> WidgetExt<M> for W {}
```

### Modifier Widgets

**Padding** - Adds space around content via Taffy layout:
```rust
pub struct Padding<W, M> {
    child: W,
    left: f32, right: f32, top: f32, bottom: f32,
    _marker: PhantomData<M>,
}
```

**Background** - Draws colored rect behind child:
```rust
pub struct Background<W, M> {
    child: W,
    color: Color,
    _marker: PhantomData<M>,
}
```

**Border** - Draws border outline around child:
```rust
pub struct Border<W, M> {
    child: W,
    color: Color,
    width: f32,
    _marker: PhantomData<M>,
}
```

**CornerRadius** - Applies rounded corners to background/border:
```rust
pub struct CornerRadius<W, M> {
    child: W,
    radius: f32,
    _marker: PhantomData<M>,
}
```

### Usage Example

```rust
// Before (widget-specific styling)
Button::new(text!("Click"), Message::Clicked)
    .color(Color::RED)
    .padding(10.0)  // Only works if Button has padding field

// After (universal modifiers)
text!("Click")
    .padding(10.0)
    .background(Color::RED)
    .border(Color::BLACK, 2.0)
    .corner_radius(8.0)
```

### File Structure

```
vexo/src/
├── widgets/
│   ├── mod.rs           # Add: pub mod modifiers; pub use modifiers::*;
│   ├── modifiers.rs     # NEW: WidgetExt + all modifier widgets
│   └── ...
├── macros.rs            # No changes needed
└── lib.rs               # Add: pub use widgets::WidgetExt;
```

### Implementation Details

**Layout handling:**
- `Padding` creates a Taffy node with padding style, wraps child node
- Other modifiers delegate layout to child, apply effects in `draw()`

**Draw order:**
- `Background` draws rect first, then child
- `Border` draws child first, then border on top
- `CornerRadius` passes radius to renderer's rect methods

**Event handling:**
- All modifiers delegate `on_event()` to child unchanged
- Hit testing uses the layout from Taffy (includes padding)

**Box<dyn Widget<M>> support:**
```rust
impl<M: Clone + Debug + Send> WidgetExt<M> for Box<dyn Widget<M>> {
    fn padding(self, amount: f32) -> Padding<Box<dyn Widget<M>>, M> {
        Padding::new(self, amount)
    }
    // ... other methods delegate to wrapper constructors
}
```

## Scope

**In scope:**
- WidgetExt trait with chaining API
- Padding, Background, Border, CornerRadius modifiers
- Support for both concrete widgets and `Box<dyn Widget<M>>`

**Out of scope:**
- Behavioral modifiers (gesture handlers, accessibility)
- Environment values or preference keys
- Animation modifiers

## Verification

1. Build: `cargo build -p vexo`
2. Run desktop demo: `cargo run -p desktop_demo`
3. Update `shared_app/src/lib.rs` to use modifiers in example UI
4. Visual verification of padding, background, border, corner radius

## Critical Files

- `vexo/src/widgets/mod.rs` - Export modifiers module
- `vexo/src/widgets/modifiers.rs` - NEW: All modifier implementation
- `vexo/src/lib.rs` - Export WidgetExt
- `shared_app/src/lib.rs` - Example usage
