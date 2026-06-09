# CSS-like Layout Authoring Design

## Goal

Make Vexo's layout authoring feel familiar to CSS developers — same concepts, same mental model, accessible from the widget API. The syntax differs (Rust builder pattern vs CSS declarations), but the concepts and overall feel are identical.

## Problem

Vexo's `Layout` struct already covers most CSS flexbox and grid properties with full Taffy conversion, but the widget layer completely hides it:

- Column/Row hardcode `flex_direction` and `align_items`, with no way to set padding, margin, gap, justify, or any other property
- DecoratedContainer overrides `flex_direction` to Column, ignoring user-provided Layout
- No widget exposes child-level layout properties (flex_grow, align_self, margin, etc.)
- No Grid widget exists
- `Style::padding` duplicates `Layout::padding` and conflates visual with layout concerns

## Design

### 1. WithLayout Widget

A single-child wrapper that applies layout properties to any child. The Vexo equivalent of inline styles on a child element in CSS.

```rust
// Explicit construction
WithLayout::new(child, Layout::new().flex_grow(1).margin(10))

// Shorthand via Widget trait
child.with_layout(Layout::new().flex_grow(1).margin(10))
```

**Implementation:**
- `WithLayout` struct: holds `child: Box<dyn Widget>` and `layout: Layout`
- Uses `DecoratedContainerElement` (single-child element that owns a render object) — same pattern as DecoratedContainer but with no Style
- Render object: `ContainerRenderObject` with the user-provided Layout — no painting, just layout
- Widget trait gets a default method:

```rust
fn with_layout(self, layout: Layout) -> WithLayout
where Self: Sized + 'static
{
    WithLayout::new(self, layout)
}
```

### 2. Column/Row Accept Layout

Add a `layout` field and `.layout()` builder method to Column and Row. Pass it to ContainerRenderObject instead of hardcoding.

```rust
// Before: hardcoded, no customization
Column::new().push(child1).push(child2)

// After: full CSS-like control
Column::new()
    .layout(Layout::new()
        .padding(16)
        .gap(8)
        .justify(JustifyContent::Center)
        .align(AlignItems::Start))
    .push(child1)
    .push(child2)
```

**Changes to Column/Row:**
- Add `layout: Layout` field (defaults to column/row-appropriate Layout)
- Add `.layout()` builder method
- `create_render_object()` passes the Layout to ContainerRenderObject
- `update_render_object()` diff-checks Layout changes and returns LAYOUT | PAINT when changed

**Changes to ContainerRenderObject:**
- Replace `is_row: bool` with `layout: Layout`
- Constructor: `new(layout: Layout)` instead of `new_column()` / `new_row()`
- In `layout()`, use `self.layout` directly — no more hardcoding
- Column provides default Layout with `flex_direction(Column) + align(Stretch)`
- Row provides default Layout with `flex_direction(Row) + align(Stretch)`
- When user calls `.layout(...)`, their Layout is used as-is

### 3. DecoratedContainer Layout Fix

Two problems to fix:

**a) Stop overriding flex_direction.** The render object's `layout()` method currently forces `flex_direction(Column)`. Instead, use `self.layout` as-is. DecoratedContainer is a decoration wrapper, not a layout container. If the user wants column behavior, they set it explicitly.

**b) Remove Style::padding.** Padding is a layout property, not a visual one. It belongs in Layout. The merge logic in the render object (`if style.padding, override layout.padding`) goes away.

```rust
// Before: padding on Style (confusing)
DecoratedContainer::new(child)
    .style(Style::new().padding(24).background(Color::BLUE))

// After: padding on Layout (clear)
DecoratedContainer::new(child)
    .style(Style::new().background(Color::BLUE))
    .layout(Layout::new().padding(24))
```

**Migration:** Remove `Style::padding` field and its builder method. Any code using it moves to `Layout::padding`. The `Style` struct becomes purely visual: `background`, `border`, `corner_radius`, `clip`.

### 4. Grid Widget

A new container widget that uses the existing Layout grid properties.

```rust
Grid::new()
    .layout(Layout::new()
        .columns(vec![TrackSizing::Fr(1.0), TrackSizing::Fr(2.0)])
        .rows(vec![TrackSizing::Auto, TrackSizing::Px(100.0)]))
    .push(child1.with_layout(Layout::new().grid_column(GridPlacement::span(2))))
    .push(child2)
    .push(child3)
```

**Implementation:**
- `Grid` widget: same shape as Column/Row — holds `children: Vec<Box<dyn Widget>>` and `layout: Layout`
- Uses `ContainerElement` (multi-child, same as Column/Row)
- Uses `ContainerRenderObject` (same render object — handles any Layout)
- Default Layout: `display(Grid)` with no templates (auto-placement)
- Children use `WithLayout` to set `grid_column`/`grid_row` placement

### 5. Missing Layout Properties

Add to the `Layout` struct:

| Property | Type | CSS Equivalent | Builder Method |
|----------|------|---------------|----------------|
| `align_self` | `Option<AlignSelf>` | `align-self` | `.align_self(AlignSelf::Center)` |
| `aspect_ratio` | `Option<f32>` | `aspect-ratio` | `.aspect_ratio(1.5)` |
| `overflow` | `Option<Overflow>` | `overflow` | `.overflow(Overflow::Hidden)` |
| `grid_auto_flow` | `Option<GridAutoFlow>` | `grid-auto-flow` | `.grid_auto_flow(GridAutoFlow::Row)` |
| `grid_auto_rows` | `Option<Vec<TrackSizing>>` | `grid-auto-rows` | `.auto_rows(vec![TrackSizing::Px(100.0)])` |
| `grid_auto_columns` | `Option<Vec<TrackSizing>>` | `grid-auto-columns` | `.auto_columns(vec![TrackSizing::Fr(1.0)])` |

New enums:

```rust
enum AlignSelf { Auto, Start, End, Center, Stretch, Baseline }
enum Overflow { Visible, Hidden, Scroll, Auto }
enum GridAutoFlow { Row, Column, RowDense, ColumnDense }
```

Each gets a Taffy conversion and builder method on Layout.

**Not adding now (YAGNI):** `box_sizing`, `order`, `justify_self` — add when needed.

### 6. No Convenience Widgets

`.with_layout()` covers all cases. Patterns like Expanded (`flex_grow(1)`) or SizedBox (`width/height`) are expressible directly:

```rust
child.with_layout(Layout::new().flex_grow(1))       // instead of Expanded
child.with_layout(Layout::new().width(200).height(100))  // instead of SizedBox
```

If verbose patterns emerge from real usage, add shorthands then.

## CSS-to-Vexo Mapping

| CSS | Vexo |
|-----|------|
| `.container { padding: 16px; }` | `.layout(Layout::new().padding(16))` on container |
| `.container { gap: 8px; }` | `.layout(Layout::new().gap(8))` on container |
| `.container { justify-content: center; }` | `.layout(Layout::new().justify(JustifyContent::Center))` on container |
| `.container { display: grid; grid-template-columns: 1fr 1fr; }` | `Grid::new().layout(Layout::new().columns(vec![Fr(1.0), Fr(1.0)]))` |
| `.item { flex: 1; }` | `.with_layout(Layout::new().flex_grow(1))` on child |
| `.item { align-self: center; }` | `.with_layout(Layout::new().align_self(AlignSelf::Center))` on child |
| `.item { margin: 10px; }` | `.with_layout(Layout::new().margin(10))` on child |
| `.item { grid-column: span 2; }` | `.with_layout(Layout::new().grid_column(GridPlacement::span(2)))` on child |
| `.item { position: absolute; top: 0; right: 0; }` | `.with_layout(Layout::new().absolute().top(0).right(0))` on child |
| `.item { aspect-ratio: 16/9; }` | `.with_layout(Layout::new().aspect_ratio(16.0/9.0))` on child |
| `.item { overflow: hidden; }` | `.with_layout(Layout::new().overflow(Overflow::Hidden))` on child |
