# Layout System Redesign: Hybrid CSS-Taffy Approach

**Date:** 2026-04-21
**Status:** Approved

## Context

The current Vexo layout system has several issues:

1. **SwiftUI modifier mismatch** - Modifiers like `Padding` create wrapper widgets that generate extra Taffy nodes, which is inefficient and doesn't map cleanly to CSS concepts.

2. **Limited Taffy coverage** - Only Flexbox is used; CSS Grid, flex_wrap, margins, and absolute positioning are missing.

3. **Dual layout systems** - Two parallel systems exist (separated traits vs legacy Widget trait), causing confusion.

4. **Verbose API** - The current modifier pattern requires multiple wrapper widgets for basic layout properties.

## Decision

Adopt a **Hybrid approach** that separates layout properties from visual decorators:

- **Layout properties** (padding, margin, gap, flex, grid) → stored in a `Layout` struct on the widget, maps directly to Taffy `Style`
- **Visual decorators** (background, border, corner_radius) → remain as wrapper widgets

This aligns with CSS's separation of box model from visual effects.

## Design

### 1. Layout Struct

A builder-style struct that holds all Taffy-exposed properties:

```rust
// vexo/src/layout/style.rs

#[derive(Clone, Debug, Default)]
pub struct Layout {
    // Box model
    pub padding: Option<EdgeInsets>,
    pub margin: Option<EdgeInsets>,
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub min_width: Option<Dimension>,
    pub min_height: Option<Dimension>,
    pub max_width: Option<Dimension>,
    pub max_height: Option<Dimension>,

    // Flexbox
    pub flex_direction: Option<FlexDirection>,
    pub flex_wrap: Option<FlexWrap>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<Dimension>,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    pub align_content: Option<AlignContent>,
    pub gap: Option<Size>,

    // Grid
    pub grid_template_columns: Option<Vec<TrackSizing>>,
    pub grid_template_rows: Option<Vec<TrackSizing>>,
    pub grid_column: Option<GridPlacement>,
    pub grid_row: Option<GridPlacement>,

    // Positioning
    pub position: Option<Position>,
    pub inset: Option<Inset>,
}

#[derive(Clone, Copy, Debug)]
pub enum Dimension {
    Auto,
    Length(f32),
    Percent(f32),
}

#[derive(Clone, Copy, Debug)]
pub struct EdgeInsets {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl EdgeInsets {
    pub fn all(value: f32) -> Self;
    pub fn horizontal(value: f32) -> Self;
    pub fn vertical(value: f32) -> Self;
    pub fn symmetric(horizontal: f32, vertical: f32) -> Self;
}
```

### 2. Widget Trait Extension

Add `layout_props()` method with default implementation:

```rust
// vexo/src/widgets/mod.rs

pub trait Widget<M: Clone + Debug + Send> {
    // Existing methods unchanged...

    /// Return layout properties for this widget.
    /// Default implementation returns empty Layout.
    fn layout_props(&self) -> Layout {
        Layout::default()
    }
}
```

### 3. Container Widget API

Column and Row get builder methods that modify internal `Layout`:

```rust
impl<M: Clone + Debug + Send> Column<M> {
    pub fn with_layout(mut self, layout: Layout) -> Self;
    pub fn padding(mut self, value: f32) -> Self;
    pub fn margin(mut self, value: f32) -> Self;
    pub fn gap(mut self, value: f32) -> Self;
    pub fn flex_wrap(mut self) -> Self;
    pub fn justify(mut self, value: JustifyContent) -> Self;
    pub fn align(mut self, value: AlignItems) -> Self;
    pub fn push(mut self, child: Box<dyn Widget<M>>) -> Self;
}
```

**Example usage:**

```rust
Column::new()
    .padding(10.0)
    .margin(5.0)
    .gap(8.0)
    .flex_wrap()
    .justify(JustifyContent::SpaceBetween)
    .push(child1)
    .push(child2)
```

### 4. Grid Widget

New container for 2D layouts:

```rust
// vexo/src/widgets/grid.rs

pub struct Grid<M: Clone + Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub layout: Layout,
    pub key: Option<String>,
}

impl<M: Clone + Debug + Send> Grid<M> {
    pub fn new() -> Self;
    pub fn columns(mut self, sizes: Vec<TrackSizing>) -> Self;
    pub fn rows(mut self, sizes: Vec<TrackSizing>) -> Self;
    pub fn gap(mut self, value: f32) -> Self;
    pub fn push(mut self, child: Box<dyn Widget<M>>) -> Self;
}

pub enum TrackSizing {
    Auto,
    Fr(f32),
    Px(f32),
    Percent(f32),
    MinMax { min: Box<TrackSizing>, max: Box<TrackSizing> },
}
```

**Example usage:**

```rust
Grid::new()
    .columns(vec![TrackSizing::Fr(1.0), TrackSizing::Fr(2.0)])
    .rows(vec![TrackSizing::Auto, TrackSizing::Auto])
    .gap(10.0)
    .push(text!("Left").boxed())
    .push(text!("Right (2x width)").boxed())
```

### 5. Absolute Positioning

Layout supports absolute positioning:

```rust
pub enum Position {
    Relative,  // Default - normal flow
    Absolute,  // Removed from flow, positioned via inset
}

pub struct Inset {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

impl Layout {
    pub fn absolute() -> Self;
    pub fn inset(mut self, value: f32) -> Self;
    pub fn top(mut self, value: f32) -> Self;
    pub fn right(mut self, value: f32) -> Self;
    pub fn bottom(mut self, value: f32) -> Self;
    pub fn left(mut self, value: f32) -> Self;
}
```

**Example usage:**

```rust
Column::new()
    .push(text!("Content"))
    .push(
        text!("Badge")
            .with_layout(Layout::absolute().top(5.0).right(5.0))
            .background(Color::RED)
            .boxed()
    )
```

### 6. Visual Modifiers (Unchanged)

Background, Border, and CornerRadius remain as wrapper widgets:

```rust
pub trait WidgetExt<M: Clone + Debug + Send>: Widget<M> + Sized {
    fn background(self, color: Color) -> Background<Self, M>;
    fn border(self, color: Color, width: f32) -> Border<Self, M>;
    fn corner_radius(self, radius: f32) -> CornerRadius<Self, M>;
    fn boxed(self) -> Box<dyn Widget<M>>;
}
```

### 7. Layout to Taffy Conversion

Single conversion function:

```rust
impl Layout {
    pub fn to_taffy_style(&self) -> taffy::Style {
        // Maps all Layout fields to taffy::Style fields
    }
}
```

## Migration Plan

### Phase 1: Add Layout Struct (Non-Breaking)

- Create `vexo/src/layout/style.rs` with `Layout`, `EdgeInsets`, `Dimension`, etc.
- Add `layout_props()` to Widget trait with default impl
- Add builder methods to Column, Row
- Add `to_taffy_style()` conversion

### Phase 2: Add Grid Widget (Non-Breaking)

- Create `vexo/src/widgets/grid.rs`
- Implement Grid container with Taffy grid support
- Add `TrackSizing`, `GridPlacement` types

### Phase 3: Deprecate Padding/Frame Wrappers (Breaking)

- Mark `Padding` and `Frame` in `modifiers.rs` as deprecated
- Update all examples to use container methods
- Add migration guide

### Phase 4: Update Macros (Breaking)

- Update `column!`, `row!` macros to support Layout properties
- Add `grid!` macro
- Remove deprecated wrappers

## Files Changed

| File | Change |
|------|--------|
| `vexo/src/layout/style.rs` | **New** - Layout struct and conversion |
| `vexo/src/layout/mod.rs` | Export new style module |
| `vexo/src/widgets/mod.rs` | Add `layout_props()` to Widget trait |
| `vexo/src/widgets/containers.rs` | Add builder methods, update `layout()` |
| `vexo/src/widgets/grid.rs` | **New** - Grid container widget |
| `vexo/src/widgets/modifiers.rs` | Deprecate Padding, Frame wrappers |
| `vexo/src/macros.rs` | Update macros for new API |
| `shared_app/src/lib.rs` | Update examples |

## Verification

1. **Unit tests** - Test Layout → Taffy Style conversion for all properties
2. **Integration tests** - Verify Grid, flex_wrap, margin, absolute positioning work correctly
3. **Visual tests** - Run desktop demo and verify layouts render correctly
4. **Migration test** - Update shared_app example and verify behavior unchanged

## Trade-offs

| Aspect | Choice | Rationale |
|--------|--------|-----------|
| Layout properties | On widget, not wrapper | Single Taffy node per widget, direct CSS mapping |
| Visual decorators | Keep as wrappers | They don't affect layout, wrapper pattern is appropriate |
| Grid support | New Grid widget | Clean separation from flexbox containers |
| Migration | Phased approach | Allows gradual adoption, non-breaking additions first |
