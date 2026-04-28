# Layout System Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace SwiftUI-style wrapper modifiers with a CSS-like Layout struct that maps directly to Taffy, enabling Grid, flex_wrap, margins, and absolute positioning.

**Architecture:** Hybrid approach - layout properties stored in a `Layout` struct on containers, visual decorators (background, border, corner_radius) remain as wrapper widgets. Single Taffy node per widget instead of wrapper nodes for padding/frame.

**Tech Stack:** Rust, Taffy 0.9.1 (CSS Flexbox + Grid layout engine)

---

## File Structure

| File | Responsibility |
|------|----------------|
| `vexo/src/layout/style.rs` | **New** - Layout struct, EdgeInsets, Dimension, Position, Inset, TrackSizing, GridPlacement |
| `vexo/src/layout/mod.rs` | Export new style types |
| `vexo/src/widgets/mod.rs` | Add `layout_props()` to Widget trait |
| `vexo/src/widgets/containers.rs` | Add builder methods to Column/Row, use Layout struct |
| `vexo/src/widgets/grid.rs` | **New** - Grid container widget |
| `vexo/src/widgets/modifiers.rs` | Deprecate Padding, Frame wrappers (keep Background, Border, CornerRadius) |
| `vexo/src/macros.rs` | Add `grid!` macro, update `column!`/`row!` for Layout support |
| `shared_app/src/lib.rs` | Update example to use new API |

---

## Task 1: Create Layout Struct and Types

**Files:**
- Create: `vexo/src/layout/style.rs`
- Modify: `vexo/src/layout/mod.rs`

- [ ] **Step 1: Create the Layout struct with all CSS properties**

Create `vexo/src/layout/style.rs`:

```rust
//! CSS-style layout properties that map directly to Taffy.
//!
//! This module provides a builder-style struct for specifying layout
//! properties like padding, margin, flex, and grid settings.

use crate::core::Size;
use crate::core::Logical;

// ============================================================================
// DIMENSION
// ============================================================================

/// A dimension value that can be auto, fixed length, or percentage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dimension {
    /// Automatic sizing based on content.
    Auto,
    /// Fixed length in logical points.
    Length(f32),
    /// Percentage of parent size (0.0-1.0).
    Percent(f32),
}

impl Default for Dimension {
    fn default() -> Self {
        Self::Auto
    }
}

// ============================================================================
// EDGE INSETS
// ============================================================================

/// Spacing values for each edge (padding or margin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeInsets {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Default for EdgeInsets {
    fn default() -> Self {
        Self { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 }
    }
}

impl EdgeInsets {
    /// Create uniform insets on all sides.
    pub fn all(value: f32) -> Self {
        Self { left: value, right: value, top: value, bottom: value }
    }

    /// Create horizontal insets (left and right).
    pub fn horizontal(value: f32) -> Self {
        Self { left: value, right: value, ..Default::default() }
    }

    /// Create vertical insets (top and bottom).
    pub fn vertical(value: f32) -> Self {
        Self { top: value, bottom: value, ..Default::default() }
    }

    /// Create symmetric horizontal and vertical insets.
    pub fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self { left: horizontal, right: horizontal, top: vertical, bottom: vertical }
    }
}

// ============================================================================
// FLEX TYPES
// ============================================================================

/// Direction of flex layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

/// How to wrap flex items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

/// How to distribute items along the main axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum JustifyContent {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// How to align items on the cross axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlignItems {
    #[default]
    Stretch,
    Start,
    End,
    Center,
    Baseline,
}

/// How to align content when wrapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlignContent {
    #[default]
    Start,
    End,
    Center,
    Stretch,
    SpaceBetween,
    SpaceAround,
}

// ============================================================================
// POSITION TYPES
// ============================================================================

/// Positioning mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Relative,
    Absolute,
}

/// Inset values for absolute positioning.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Inset {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

impl Inset {
    /// Create uniform inset on all sides.
    pub fn all(value: f32) -> Self {
        Self { top: Some(value), right: Some(value), bottom: Some(value), left: Some(value) }
    }
}

// ============================================================================
// GRID TYPES
// ============================================================================

/// Track sizing for grid columns/rows.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackSizing {
    /// Size to content.
    Auto,
    /// Fraction of available space.
    Fr(f32),
    /// Fixed pixels.
    Px(f32),
    /// Percentage of container.
    Percent(f32),
    /// Min-max constraint.
    MinMax { min: Box<TrackSizing>, max: Box<TrackSizing> },
}

/// Grid item placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridPlacement {
    pub start: i16,
    pub span: u16,
}

impl GridPlacement {
    /// Place in the next available cell.
    pub fn auto() -> Self {
        Self { start: -1, span: 1 }
    }

    /// Start at a specific line (1-indexed).
    pub fn start(start: i16) -> Self {
        Self { start, span: 1 }
    }

    /// Span multiple cells.
    pub fn span(span: u16) -> Self {
        Self { start: -1, span }
    }

    /// Start at a specific line and span multiple cells.
    pub fn start_span(start: i16, span: u16) -> Self {
        Self { start, span }
    }
}

// ============================================================================
// LAYOUT STRUCT
// ============================================================================

/// Builder-style struct for all CSS layout properties.
///
/// Maps directly to Taffy's Style type. Use builder methods to set
/// properties, then call `to_taffy_style()` to convert.
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
    pub gap: Option<Size<Logical>>,

    // Grid
    pub grid_template_columns: Option<Vec<TrackSizing>>,
    pub grid_template_rows: Option<Vec<TrackSizing>>,
    pub grid_column: Option<GridPlacement>,
    pub grid_row: Option<GridPlacement>,

    // Positioning
    pub position: Option<Position>,
    pub inset: Option<Inset>,
}

impl Layout {
    // ========================================================================
    // Box Model Builders
    // ========================================================================

    /// Set uniform padding on all sides.
    pub fn padding(mut self, value: f32) -> Self {
        self.padding = Some(EdgeInsets::all(value));
        self
    }

    /// Set padding with specific values for each side.
    pub fn padding_each(mut self, left: f32, right: f32, top: f32, bottom: f32) -> Self {
        self.padding = Some(EdgeInsets { left, right, top, bottom });
        self
    }

    /// Set uniform margin on all sides.
    pub fn margin(mut self, value: f32) -> Self {
        self.margin = Some(EdgeInsets::all(value));
        self
    }

    /// Set margin with specific values for each side.
    pub fn margin_each(mut self, left: f32, right: f32, top: f32, bottom: f32) -> Self {
        self.margin = Some(EdgeInsets { left, right, top, bottom });
        self
    }

    /// Set fixed width.
    pub fn width(mut self, value: f32) -> Self {
        self.width = Some(Dimension::Length(value));
        self
    }

    /// Set fixed height.
    pub fn height(mut self, value: f32) -> Self {
        self.height = Some(Dimension::Length(value));
        self
    }

    /// Set percentage width (0.0-1.0).
    pub fn width_percent(mut self, value: f32) -> Self {
        self.width = Some(Dimension::Percent(value));
        self
    }

    /// Set percentage height (0.0-1.0).
    pub fn height_percent(mut self, value: f32) -> Self {
        self.height = Some(Dimension::Percent(value));
        self
    }

    /// Set minimum width.
    pub fn min_width(mut self, value: f32) -> Self {
        self.min_width = Some(Dimension::Length(value));
        self
    }

    /// Set minimum height.
    pub fn min_height(mut self, value: f32) -> Self {
        self.min_height = Some(Dimension::Length(value));
        self
    }

    /// Set maximum width.
    pub fn max_width(mut self, value: f32) -> Self {
        self.max_width = Some(Dimension::Length(value));
        self
    }

    /// Set maximum height.
    pub fn max_height(mut self, value: f32) -> Self {
        self.max_height = Some(Dimension::Length(value));
        self
    }

    // ========================================================================
    // Flexbox Builders
    // ========================================================================

    /// Set flex direction.
    pub fn flex_direction(mut self, value: FlexDirection) -> Self {
        self.flex_direction = Some(value);
        self
    }

    /// Enable flex wrapping.
    pub fn flex_wrap(mut self) -> Self {
        self.flex_wrap = Some(FlexWrap::Wrap);
        self
    }

    /// Set flex wrap mode.
    pub fn flex_wrap_mode(mut self, value: FlexWrap) -> Self {
        self.flex_wrap = Some(value);
        self
    }

    /// Set flex grow factor.
    pub fn flex_grow(mut self, value: f32) -> Self {
        self.flex_grow = Some(value);
        self
    }

    /// Set flex shrink factor.
    pub fn flex_shrink(mut self, value: f32) -> Self {
        self.flex_shrink = Some(value);
        self
    }

    /// Set flex basis.
    pub fn flex_basis(mut self, value: f32) -> Self {
        self.flex_basis = Some(Dimension::Length(value));
        self
    }

    /// Set justify content.
    pub fn justify(mut self, value: JustifyContent) -> Self {
        self.justify_content = Some(value);
        self
    }

    /// Set align items.
    pub fn align(mut self, value: AlignItems) -> Self {
        self.align_items = Some(value);
        self
    }

    /// Set align content.
    pub fn align_content(mut self, value: AlignContent) -> Self {
        self.align_content = Some(value);
        self
    }

    /// Set gap between items.
    pub fn gap(mut self, value: f32) -> Self {
        self.gap = Some(Size::new(value, value));
        self
    }

    /// Set horizontal and vertical gap separately.
    pub fn gap_each(mut self, width: f32, height: f32) -> Self {
        self.gap = Some(Size::new(width, height));
        self
    }

    // ========================================================================
    // Grid Builders
    // ========================================================================

    /// Set grid column template.
    pub fn columns(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.grid_template_columns = Some(sizes);
        self
    }

    /// Set grid row template.
    pub fn rows(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.grid_template_rows = Some(sizes);
        self
    }

    /// Set grid column placement.
    pub fn grid_column(mut self, placement: GridPlacement) -> Self {
        self.grid_column = Some(placement);
        self
    }

    /// Set grid row placement.
    pub fn grid_row(mut self, placement: GridPlacement) -> Self {
        self.grid_row = Some(placement);
        self
    }

    // ========================================================================
    // Positioning Builders
    // ========================================================================

    /// Set position to absolute.
    pub fn absolute(mut self) -> Self {
        self.position = Some(Position::Absolute);
        self
    }

    /// Set position to relative (default).
    pub fn relative(mut self) -> Self {
        self.position = Some(Position::Relative);
        self
    }

    /// Set all insets (top, right, bottom, left).
    pub fn inset(mut self, value: f32) -> Self {
        self.inset = Some(Inset::all(value));
        self
    }

    /// Set top inset.
    pub fn top(mut self, value: f32) -> Self {
        let mut inset = self.inset.unwrap_or_default();
        inset.top = Some(value);
        self.inset = Some(inset);
        self
    }

    /// Set right inset.
    pub fn right(mut self, value: f32) -> Self {
        let mut inset = self.inset.unwrap_or_default();
        inset.right = Some(value);
        self.inset = Some(inset);
        self
    }

    /// Set bottom inset.
    pub fn bottom(mut self, value: f32) -> Self {
        let mut inset = self.inset.unwrap_or_default();
        inset.bottom = Some(value);
        self.inset = Some(inset);
        self
    }

    /// Set left inset.
    pub fn left(mut self, value: f32) -> Self {
        let mut inset = self.inset.unwrap_or_default();
        inset.left = Some(value);
        self.inset = Some(inset);
        self
    }

    // ========================================================================
    // Convenience Methods
    // ========================================================================

    /// Create a layout that fills available space.
    pub fn fill() -> Self {
        Self::default().flex_grow(1.0)
    }

    /// Create a layout with fixed dimensions.
    pub fn fixed(width: f32, height: f32) -> Self {
        Self::default().width(width).height(height)
    }

    /// Create an absolute positioned layout.
    pub fn absolute_at(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self::default()
            .absolute()
            .inset(EdgeInsets { top, right, bottom, left })
    }
}
```

- [ ] **Step 2: Add the to_taffy_style conversion method**

Add to `vexo/src/layout/style.rs` at the end:

```rust
// ============================================================================
// TAFFY CONVERSION
// ============================================================================

impl Layout {
    /// Convert to Taffy Style for layout computation.
    pub fn to_taffy_style(&self) -> taffy::Style {
        use taffy::prelude::*;

        taffy::Style {
            // Box model
            padding: self.padding.map(|p| Rect {
                left: length(p.left),
                right: length(p.right),
                top: length(p.top),
                bottom: length(p.bottom),
            }).unwrap_or_default(),
            margin: self.margin.map(|m| Rect {
                left: length(m.left),
                right: length(m.right),
                top: length(m.top),
                bottom: length(m.bottom),
            }).unwrap_or_default(),
            size: Size {
                width: self.width.map(|d| d.to_taffy()).unwrap_or(auto()),
                height: self.height.map(|d| d.to_taffy()).unwrap_or(auto()),
            },
            min_size: Size {
                width: self.min_width.map(|d| d.to_taffy()).unwrap_or(auto()),
                height: self.min_height.map(|d| d.to_taffy()).unwrap_or(auto()),
            },
            max_size: Size {
                width: self.max_width.map(|d| d.to_taffy()).unwrap_or(auto()),
                height: self.max_height.map(|d| d.to_taffy()).unwrap_or(auto()),
            },

            // Flexbox
            display: Display::Flex,
            flex_direction: self.flex_direction.map(|d| d.to_taffy()).unwrap_or_default(),
            flex_wrap: self.flex_wrap.map(|w| w.to_taffy()).unwrap_or_default(),
            flex_grow: self.flex_grow.unwrap_or(0.0),
            flex_shrink: self.flex_shrink.unwrap_or(1.0),
            flex_basis: self.flex_basis.map(|d| d.to_taffy()).unwrap_or(auto()),
            justify_content: self.justify_content.map(|j| j.to_taffy()),
            align_items: self.align_items.map(|a| a.to_taffy()),
            align_content: self.align_content.map(|a| a.to_taffy()),
            gap: self.gap.map(|g| Size {
                width: length(g.width),
                height: length(g.height),
            }).unwrap_or_default(),

            // Grid
            grid_template_columns: self.grid_template_columns.as_ref().map(|t| t.to_taffy_tracks()),
            grid_template_rows: self.grid_template_rows.as_ref().map(|t| t.to_taffy_tracks()),
            grid_column: self.grid_column.map(|p| p.to_taffy()),
            grid_row: self.grid_row.map(|p| p.to_taffy()),

            // Positioning
            position: self.position.map(|p| p.to_taffy()).unwrap_or_default(),
            inset: self.inset.map(|i| i.to_taffy()).unwrap_or_default(),

            ..Default::default()
        }
    }
}

// ============================================================================
// TAFFY CONVERSION TRAITS
// ============================================================================

impl Dimension {
    fn to_taffy(self) -> taffy::prelude::Dimension {
        match self {
            Dimension::Auto => taffy::prelude::auto(),
            Dimension::Length(v) => taffy::prelude::length(v),
            Dimension::Percent(v) => taffy::prelude::percent(v),
        }
    }
}

impl FlexDirection {
    fn to_taffy(self) -> taffy::prelude::FlexDirection {
        match self {
            FlexDirection::Row => taffy::prelude::FlexDirection::Row,
            FlexDirection::Column => taffy::prelude::FlexDirection::Column,
            FlexDirection::RowReverse => taffy::prelude::FlexDirection::RowReverse,
            FlexDirection::ColumnReverse => taffy::prelude::FlexDirection::ColumnReverse,
        }
    }
}

impl FlexWrap {
    fn to_taffy(self) -> taffy::prelude::FlexWrap {
        match self {
            FlexWrap::NoWrap => taffy::prelude::FlexWrap::NoWrap,
            FlexWrap::Wrap => taffy::prelude::FlexWrap::Wrap,
            FlexWrap::WrapReverse => taffy::prelude::FlexWrap::WrapReverse,
        }
    }
}

impl JustifyContent {
    fn to_taffy(self) -> Option<taffy::prelude::JustifyContent> {
        use taffy::prelude::JustifyContent as TaffyJustify;
        Some(match self {
            JustifyContent::Start => TaffyJustify::Start,
            JustifyContent::End => TaffyJustify::End,
            JustifyContent::Center => TaffyJustify::Center,
            JustifyContent::SpaceBetween => TaffyJustify::SpaceBetween,
            JustifyContent::SpaceAround => TaffyJustify::SpaceAround,
            JustifyContent::SpaceEvenly => TaffyJustify::SpaceEvenly,
        })
    }
}

impl AlignItems {
    fn to_taffy(self) -> Option<taffy::prelude::AlignItems> {
        use taffy::prelude::AlignItems as TaffyAlign;
        Some(match self {
            AlignItems::Stretch => TaffyAlign::Stretch,
            AlignItems::Start => TaffyAlign::Start,
            AlignItems::End => TaffyAlign::End,
            AlignItems::Center => TaffyAlign::Center,
            AlignItems::Baseline => TaffyAlign::Baseline,
        })
    }
}

impl AlignContent {
    fn to_taffy(self) -> Option<taffy::prelude::AlignContent> {
        use taffy::prelude::AlignContent as TaffyAlign;
        Some(match self {
            AlignContent::Start => TaffyAlign::Start,
            AlignContent::End => TaffyAlign::End,
            AlignContent::Center => TaffyAlign::Center,
            AlignContent::Stretch => TaffyAlign::Stretch,
            AlignContent::SpaceBetween => TaffyAlign::SpaceBetween,
            AlignContent::SpaceAround => TaffyAlign::SpaceAround,
        })
    }
}

impl Position {
    fn to_taffy(self) -> taffy::prelude::Position {
        match self {
            Position::Relative => taffy::prelude::Position::Relative,
            Position::Absolute => taffy::prelude::Position::Absolute,
        }
    }
}

impl Inset {
    fn to_taffy(self) -> taffy::Rect<taffy::prelude::LengthPercentageAuto> {
        taffy::Rect {
            top: self.top.map(taffy::prelude::length).unwrap_or(taffy::prelude::auto()),
            right: self.right.map(taffy::prelude::length).unwrap_or(taffy::prelude::auto()),
            bottom: self.bottom.map(taffy::prelude::length).unwrap_or(taffy::prelude::auto()),
            left: self.left.map(taffy::prelude::length).unwrap_or(taffy::prelude::auto()),
        }
    }
}

impl TrackSizing {
    fn to_taffy(&self) -> taffy::prelude::NonRepeatedTrackSizing {
        use taffy::prelude::*;
        match self {
            TrackSizing::Auto => auto(),
            TrackSizing::Fr(v) => fr(*v),
            TrackSizing::Px(v) => length(*v),
            TrackSizing::Percent(v) => percent(*v),
            TrackSizing::MinMax { min, max } => minmax(min.to_taffy(), max.to_taffy()),
        }
    }
}

impl Vec<TrackSizing> {
    fn to_taffy_tracks(&self) -> taffy::prelude::TrackSizingFunction {
        taffy::prelude::TrackSizingFunction::Multiple(
            self.iter().map(|t| t.to_taffy()).collect()
        )
    }
}

impl GridPlacement {
    fn to_taffy(self) -> taffy::prelude::GridPlacement {
        taffy::prelude::GridPlacement {
            start: taffy::prelude::GridLine::from(self.start),
            span: self.span,
        }
    }
}
```

- [ ] **Step 3: Update layout/mod.rs to export new types**

Modify `vexo/src/layout/mod.rs`:

```rust
//! Layout abstractions for the Vexo UI framework.
//!
//! This module provides the layout layer that sits between widgets and
//! the layout engine. It defines:
//!
//! - `LayoutEngine` trait for layout computation
//! - `LayoutNode` tree structure for describing layout
//! - `ComputedLayout` for layout results
//! - `TaffyLayoutEngine` implementation
//! - `Layout` struct for CSS-style layout properties
//!
//! # Architecture
//!
//! The layout abstraction enables:
//! - Testing layout without Taffy dependency
//! - Swapping to different layout algorithms
//! - Centralized layout logic (not scattered in widgets)
//!
//! # Example
//!
//! ```
//! use vexo::layout::{LayoutEngine, TaffyLayoutEngine, LayoutConstraints, Layout};
//!
//! let mut engine = TaffyLayoutEngine::new();
//! // Build and compute layout trees using the engine
//!
//! // Or use the Layout struct for CSS-style properties
//! let layout = Layout::default()
//!     .padding(10.0)
//!     .margin(5.0)
//!     .flex_grow(1.0);
//! ```

mod engine;
mod node;
mod style;
mod taffy_engine;

pub use engine::{LayoutEngine, LayoutError, LayoutTreeHandle};
pub use node::{
    AlignItems,
    ComputedLayout,
    FlexDirection,
    LayoutConstraints,
    LayoutNode,
    LayoutNodeId,
    LayoutPadding,
    LayoutTree,
};
pub use style::{
    AlignContent,
    Dimension,
    EdgeInsets,
    FlexWrap,
    GridPlacement,
    Inset,
    JustifyContent,
    Layout,
    Position,
    TrackSizing,
};
pub use taffy_engine::TaffyLayoutEngine;
```

- [ ] **Step 4: Run cargo check to verify compilation**

Run: `cargo check -p vexo`
Expected: No errors (warnings about unused imports are OK)

- [ ] **Step 5: Commit**

```bash
git add vexo/src/layout/style.rs vexo/src/layout/mod.rs
git commit -m "feat(layout): add Layout struct with CSS-style properties

- Layout struct with padding, margin, flex, grid, positioning
- EdgeInsets, Dimension, Position, Inset types
- TrackSizing and GridPlacement for grid layouts
- to_taffy_style() conversion method"
```

---

## Task 2: Add layout_props() to Widget Trait

**Files:**
- Modify: `vexo/src/widgets/mod.rs`

- [ ] **Step 1: Add layout_props() method to Widget trait**

Modify `vexo/src/widgets/mod.rs` to add the new method after `key()`:

```rust
use crate::renderer::UiBatcher;
use crate::utils::Physical;
use crate::core::WidgetId;
use crate::state::WidgetStateRegistry;
use crate::input::InputEvent;
use crate::layout::Layout;
use glyphon::FontSystem;
use taffy::prelude::NodeId;

pub trait Widget<M: Clone + std::fmt::Debug + Send> {
    /// Optional stable key for identity across reorders.
    /// Widgets that need focus tracking must have a unique key.
    fn key(&self) -> Option<&str> {
        None
    }

    /// Return layout properties for this widget.
    /// Default implementation returns empty Layout.
    fn layout_props(&self) -> Layout {
        Layout::default()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId;

    // ... rest of trait unchanged
```

- [ ] **Step 2: Update Box<dyn Widget<M>> implementation**

Add `layout_props()` to the blanket impl for `Box<dyn Widget<M>>`:

```rust
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Box<dyn Widget<M>> {
    fn key(&self) -> Option<&str> {
        (**self).key()
    }

    fn layout_props(&self) -> Layout {
        (**self).layout_props()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        (**self).layout(taffy, ctx)
    }

    // ... rest unchanged
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/mod.rs
git commit -m "feat(widget): add layout_props() to Widget trait

Default implementation returns empty Layout. Box<dyn Widget<M>>
delegates to inner widget."
```

---

## Task 3: Update Column and Row with Layout Builder Methods

**Files:**
- Modify: `vexo/src/widgets/containers.rs`

- [ ] **Step 1: Add Layout field and builder methods to Column**

Replace the Column struct and impl with:

```rust
use crate::layout::{Layout, JustifyContent, AlignItems, FlexWrap, FlexDirection};
use crate::renderer::UiBatcher;
use crate::widgets::{WidgetContext, WidgetId, WidgetResponse};
use crate::Widget;
use crate::input::InputEvent;
use taffy::prelude::{length, Display, NodeId, Size};
use taffy::Style;

pub struct Column<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
    pub layout: Layout,
}

impl<M: Clone + std::fmt::Debug + Send> Column<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
            layout: Layout::default(),
        }
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    // Layout builder methods

    /// Set the entire Layout struct.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Set uniform padding.
    pub fn padding(mut self, value: f32) -> Self {
        self.layout = self.layout.padding(value);
        self
    }

    /// Set uniform margin.
    pub fn margin(mut self, value: f32) -> Self {
        self.layout = self.layout.margin(value);
        self
    }

    /// Set gap between children.
    pub fn gap(mut self, value: f32) -> Self {
        self.layout = self.layout.gap(value);
        self
    }

    /// Enable flex wrapping.
    pub fn flex_wrap(mut self) -> Self {
        self.layout = self.layout.flex_wrap();
        self
    }

    /// Set justify content.
    pub fn justify(mut self, value: JustifyContent) -> Self {
        self.layout = self.layout.justify(value);
        self
    }

    /// Set align items.
    pub fn align(mut self, value: AlignItems) -> Self {
        self.layout = self.layout.align(value);
        self
    }

    /// Set flex grow.
    pub fn flex_grow(mut self, value: f32) -> Self {
        self.layout = self.layout.flex_grow(value);
        self
    }
}
```

- [ ] **Step 2: Update Column Widget impl to use Layout**

Replace the Widget impl for Column:

```rust
#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Column<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let mut child_nodes: Vec<NodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(taffy, ctx));
        }

        // Merge column defaults with user-specified layout
        let style = Layout {
            flex_direction: Some(FlexDirection::Column),
            ..self.layout.clone()
        }.to_taffy_style();

        taffy.new_with_children(style, &child_nodes).unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
        use crate::utils::Point;

        let layout = taffy.layout(node).unwrap();
        let my_offset = Point::<crate::utils::Logical>::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        let child_ids = taffy.children(node).unwrap();
        for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
            child_widget.draw(
                taffy,
                child_node_id,
                renderer,
                my_offset,
                focused_id,
                cursor_blink,
                ctx,
            );
        }
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: crate::utils::Point<crate::utils::Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        use crate::utils::Point;

        let child_ids = taffy.children(node).unwrap();
        let layout = taffy.layout(node).unwrap();
        let my_offset = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
            let child_response =
                child.on_event(taffy, child_node_id, my_offset, event, focused_id, ctx);

            if child_response.handled || child_response.focus_request.is_some() {
                return child_response;
            }
        }
        WidgetResponse::default()
    }
}
```

- [ ] **Step 3: Update Row struct similarly**

Replace the Row struct and impl:

```rust
pub struct Row<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
    pub layout: Layout,
}

impl<M: Clone + std::fmt::Debug + Send> Row<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
            layout: Layout::default(),
        }
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    // Layout builder methods

    /// Set the entire Layout struct.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Set uniform padding.
    pub fn padding(mut self, value: f32) -> Self {
        self.layout = self.layout.padding(value);
        self
    }

    /// Set uniform margin.
    pub fn margin(mut self, value: f32) -> Self {
        self.layout = self.layout.margin(value);
        self
    }

    /// Set gap between children.
    pub fn gap(mut self, value: f32) -> Self {
        self.layout = self.layout.gap(value);
        self
    }

    /// Enable flex wrapping.
    pub fn flex_wrap(mut self) -> Self {
        self.layout = self.layout.flex_wrap();
        self
    }

    /// Set justify content.
    pub fn justify(mut self, value: JustifyContent) -> Self {
        self.layout = self.layout.justify(value);
        self
    }

    /// Set align items.
    pub fn align(mut self, value: AlignItems) -> Self {
        self.layout = self.layout.align(value);
        self
    }

    /// Set flex grow.
    pub fn flex_grow(mut self, value: f32) -> Self {
        self.layout = self.layout.flex_grow(value);
        self
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Row<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let mut child_nodes: Vec<NodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(taffy, ctx));
        }

        // Merge row defaults with user-specified layout
        let style = Layout {
            flex_direction: Some(FlexDirection::Row),
            ..self.layout.clone()
        }.to_taffy_style();

        taffy.new_with_children(style, &child_nodes).unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: crate::utils::Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
        use crate::utils::Point;

        let layout = taffy.layout(node).unwrap();
        let my_offset = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        let child_ids = taffy.children(node).unwrap();
        for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
            child_widget.draw(
                taffy,
                child_node_id,
                renderer,
                my_offset,
                focused_id,
                cursor_blink,
                ctx,
            );
        }
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: crate::utils::Point<crate::utils::Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        use crate::utils::Point;

        let child_ids = taffy.children(node).unwrap();
        let layout = taffy.layout(node).unwrap();
        let my_offset = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
            let child_response =
                child.on_event(taffy, child_node_id, my_offset, event, focused_id, ctx);

            if child_response.handled || child_response.focus_request.is_some() {
                return child_response;
            }
        }
        WidgetResponse::default()
    }
}
```

- [ ] **Step 4: Run cargo check**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add vexo/src/widgets/containers.rs
git commit -m "feat(containers): add Layout builder methods to Column and Row

- padding(), margin(), gap(), flex_wrap(), justify(), align()
- Use Layout.to_taffy_style() instead of manual Style construction
- Single Taffy node per container (no wrapper nodes)"
```

---

## Task 4: Create Grid Widget

**Files:**
- Create: `vexo/src/widgets/grid.rs`
- Modify: `vexo/src/widgets/mod.rs`

- [ ] **Step 1: Create the Grid widget**

Create `vexo/src/widgets/grid.rs`:

```rust
//! Grid container widget for 2D layouts.

use crate::layout::{Layout, TrackSizing, FlexDirection};
use crate::renderer::UiBatcher;
use crate::widgets::{Widget, WidgetContext, WidgetId, WidgetResponse};
use crate::input::InputEvent;
use crate::utils::Point;
use taffy::prelude::NodeId;

/// Grid container for 2D layouts with rows and columns.
pub struct Grid<M: Clone + std::fmt::Debug + Send> {
    pub children: Vec<Box<dyn Widget<M>>>,
    pub key: Option<String>,
    pub layout: Layout,
}

impl<M: Clone + std::fmt::Debug + Send> Grid<M> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            key: None,
            layout: Layout::default(),
        }
    }

    /// Define column sizes.
    pub fn columns(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.layout = self.layout.columns(sizes);
        self
    }

    /// Define row sizes.
    pub fn rows(mut self, sizes: Vec<TrackSizing>) -> Self {
        self.layout = self.layout.rows(sizes);
        self
    }

    /// Set gap between cells.
    pub fn gap(mut self, value: f32) -> Self {
        self.layout = self.layout.gap(value);
        self
    }

    /// Set the entire Layout struct.
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    pub fn push(mut self, widget: Box<dyn Widget<M>>) -> Self {
        self.children.push(widget);
        self
    }

    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl<M: Clone + std::fmt::Debug + Send> Default for Grid<M> {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(unused_variables)]
impl<M: Clone + std::fmt::Debug + Send> Widget<M> for Grid<M> {
    fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    fn layout(&mut self, taffy: &mut taffy::TaffyTree, ctx: &mut WidgetContext) -> NodeId {
        let mut child_nodes: Vec<NodeId> = Vec::new();
        for child in self.children.iter_mut() {
            child_nodes.push(child.layout(taffy, ctx));
        }

        // Build grid style
        let mut style = self.layout.clone().to_taffy_style();
        style.display = taffy::prelude::Display::Grid;

        taffy.new_with_children(style, &child_nodes).unwrap()
    }

    fn draw(
        &self,
        taffy: &mut taffy::TaffyTree,
        node: NodeId,
        renderer: &mut UiBatcher,
        offset: Point<crate::utils::Logical>,
        focused_id: Option<WidgetId>,
        cursor_blink: &crate::CursorBlinkState,
        ctx: &mut WidgetContext,
    ) {
        let layout = taffy.layout(node).unwrap();
        let my_offset = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        let child_ids = taffy.children(node).unwrap();
        for (child_widget, child_node_id) in self.children.iter().zip(child_ids) {
            child_widget.draw(
                taffy,
                child_node_id,
                renderer,
                my_offset,
                focused_id,
                cursor_blink,
                ctx,
            );
        }
    }

    fn on_event(
        &mut self,
        taffy: &taffy::TaffyTree,
        node: NodeId,
        offset: Point<crate::utils::Logical>,
        event: &InputEvent,
        focused_id: Option<WidgetId>,
        ctx: &mut WidgetContext,
    ) -> WidgetResponse<M> {
        let child_ids = taffy.children(node).unwrap();
        let layout = taffy.layout(node).unwrap();
        let my_offset = Point::new(
            offset.x + layout.location.x,
            offset.y + layout.location.y,
        );

        for (child, child_node_id) in self.children.iter_mut().zip(child_ids) {
            let child_response =
                child.on_event(taffy, child_node_id, my_offset, event, focused_id, ctx);

            if child_response.handled || child_response.focus_request.is_some() {
                return child_response;
            }
        }
        WidgetResponse::default()
    }
}
```

- [ ] **Step 2: Export Grid from widgets/mod.rs**

Add to `vexo/src/widgets/mod.rs`:

```rust
mod button;
mod color_widget;
mod containers;
mod grid;
mod modifiers;
mod text;
mod text_edit;

pub use button::Button;
pub use color_widget::ColorWidget;
pub use containers::Column;
pub use containers::Row;
pub use grid::Grid;
pub use modifiers::Background;
pub use modifiers::Border;
pub use modifiers::CornerRadius;
pub use modifiers::Frame;
pub use modifiers::FrameSize;
pub use modifiers::Padding;
pub use modifiers::WidgetExt;
pub use text::Text;
pub use text_edit::TextEdit;
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/grid.rs vexo/src/widgets/mod.rs
git commit -m "feat(widgets): add Grid container widget

- 2D layout with columns() and rows() methods
- Supports TrackSizing (Auto, Fr, Px, Percent, MinMax)
- Uses Taffy's CSS Grid implementation"
```

---

## Task 5: Add Grid Macro and Update Container Macros

**Files:**
- Modify: `vexo/src/macros.rs`

- [ ] **Step 1: Add grid! macro and update column!/row! macros**

Add to `vexo/src/macros.rs`:

```rust
/// Create a Grid container widget wrapped in Box.
///
/// # Example
/// ```
/// use vexo::widgets::Grid;
/// use vexo::layout::TrackSizing;
/// let grid: Box<Grid<()>> = vexo::grid![
///     columns: vec![TrackSizing::Fr(1.0), TrackSizing::Fr(2.0)],
///     rows: vec![TrackSizing::Auto],
///     vexo::text!("Left"),
///     vexo::text!("Right"),
/// ];
/// ```
#[macro_export]
macro_rules! grid {
    // With columns and rows
    (columns: $cols:expr, rows: $rows:expr, $($child:expr),* $(,)?) => {
        {
            let mut grid = $crate::widgets::Grid::new()
                .columns($cols)
                .rows($rows);
            $(
                grid = grid.push($child);
            )*
            Box::new(grid)
        }
    };
    // With columns only
    (columns: $cols:expr, $($child:expr),* $(,)?) => {
        {
            let mut grid = $crate::widgets::Grid::new().columns($cols);
            $(
                grid = grid.push($child);
            )*
            Box::new(grid)
        }
    };
    // Children only (auto columns)
    ($($child:expr),* $(,)?) => {
        {
            let mut grid = $crate::widgets::Grid::new();
            $(
                grid = grid.push($child);
            )*
            Box::new(grid)
        }
    };
}
```

- [ ] **Step 2: Update column! macro to support layout properties**

Replace the existing `column!` macro:

```rust
/// Create a Column container widget wrapped in Box.
///
/// # Examples
/// ```
/// use vexo::widgets::Column;
/// use vexo::layout::JustifyContent;
/// let col: Box<Column<()>> = vexo::column![
///     vexo::text!("Title"),
/// ];
///
/// // With alignment
/// let col: Box<Column<()>> = vexo::column![
///     align: JustifyContent::Center,
///     vexo::text!("Centered"),
/// ];
///
/// // With padding
/// let col: Box<Column<()>> = vexo::column![
///     padding: 10.0,
///     vexo::text!("Padded"),
/// ];
/// ```
#[macro_export]
macro_rules! column {
    // With alignment
    (align: $align:expr, $($child:expr),* $(,)?) => {
        {
            let mut col = $crate::widgets::Column::new().align($align);
            $(
                col = col.push($child);
            )*
            Box::new(col)
        }
    };
    // With padding
    (padding: $padding:expr, $($child:expr),* $(,)?) => {
        {
            let mut col = $crate::widgets::Column::new().padding($padding);
            $(
                col = col.push($child);
            )*
            Box::new(col)
        }
    };
    // With gap
    (gap: $gap:expr, $($child:expr),* $(,)?) => {
        {
            let mut col = $crate::widgets::Column::new().gap($gap);
            $(
                col = col.push($child);
            )*
            Box::new(col)
        }
    };
    // Without options
    ($($child:expr),* $(,)?) => {
        {
            let mut col = $crate::widgets::Column::new();
            $(
                col = col.push($child);
            )*
            Box::new(col)
        }
    };
}
```

- [ ] **Step 3: Update row! macro similarly**

Replace the existing `row!` macro:

```rust
/// Create a Row container widget wrapped in Box.
///
/// # Example
/// ```
/// use vexo::widgets::Row;
/// let row: Box<Row<()>> = vexo::row![
///     vexo::text!("Left"),
///     vexo::text!("Right"),
/// ];
///
/// // With padding
/// let row: Box<Row<()>> = vexo::row![
///     padding: 10.0,
///     vexo::text!("Padded"),
/// ];
/// ```
#[macro_export]
macro_rules! row {
    // With padding
    (padding: $padding:expr, $($child:expr),* $(,)?) => {
        {
            let mut row = $crate::widgets::Row::new().padding($padding);
            $(
                row = row.push($child);
            )*
            Box::new(row)
        }
    };
    // With gap
    (gap: $gap:expr, $($child:expr),* $(,)?) => {
        {
            let mut row = $crate::widgets::Row::new().gap($gap);
            $(
                row = row.push($child);
            )*
            Box::new(row)
        }
    };
    // Without options
    ($($child:expr),* $(,)?) => {
        {
            let mut row = $crate::widgets::Row::new();
            $(
                row = row.push($child);
            )*
            Box::new(row)
        }
    };
}
```

- [ ] **Step 4: Run cargo check**

Run: `cargo check -p vexo`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add vexo/src/macros.rs
git commit -m "feat(macros): add grid! macro, update column!/row! with layout options

- grid! supports columns/rows parameters
- column!/row! support padding and gap options"
```

---

## Task 6: Deprecate Padding and Frame Wrappers

**Files:**
- Modify: `vexo/src/widgets/modifiers.rs`

- [ ] **Step 1: Add deprecation warnings to Padding and Frame**

Add deprecation attributes to the Padding struct:

```rust
/// Adds padding around a child widget using Taffy layout.
///
/// **Deprecated:** Use container's `.padding()` method instead.
/// This wrapper creates an extra Taffy node which is inefficient.
///
/// # Migration
/// ```rust
/// // Old (deprecated):
/// text!("Hello").padding(10.0).boxed()
///
/// // New (preferred):
/// Column::new().padding(10.0).push(text!("Hello")).boxed()
/// ```
#[deprecated(
    since = "0.2.0",
    note = "Use container's `.padding()` method instead. This wrapper creates extra Taffy nodes."
)]
pub struct Padding<W, M> {
    child: W,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    _marker: PhantomData<M>,
}
```

Add deprecation to Frame:

```rust
/// Applies size constraints to a child widget.
///
/// **Deprecated:** Use container's `.width()`/`.height()` methods or
/// `Layout::fixed()` instead. This wrapper creates an extra Taffy node.
///
/// # Migration
/// ```rust
/// // Old (deprecated):
/// text!("Hello").frame(100.0, 50.0).boxed()
///
/// // New (preferred):
/// Column::new()
///     .with_layout(Layout::fixed(100.0, 50.0))
///     .push(text!("Hello"))
///     .boxed()
/// ```
#[deprecated(
    since = "0.2.0",
    note = "Use Layout::fixed() or container's width/height methods instead. This wrapper creates extra Taffy nodes."
)]
pub struct Frame<W, M> {
    child: W,
    constraints: FrameSize,
    _marker: PhantomData<M>,
}
```

- [ ] **Step 2: Add deprecation to WidgetExt methods**

Update the padding and frame methods in WidgetExt:

```rust
    /// Add uniform padding around the widget.
    ///
    /// **Deprecated:** Use container's `.padding()` method instead.
    #[deprecated(
        since = "0.2.0",
        note = "Use container's `.padding()` method instead"
    )]
    fn padding(self, amount: f32) -> Padding<Self, M> {
        Padding::uniform(self, amount)
    }

    // ... other padding methods with same deprecation ...

    /// Apply fixed size constraints to the widget.
    ///
    /// **Deprecated:** Use `Layout::fixed()` instead.
    #[deprecated(
        since = "0.2.0",
        note = "Use Layout::fixed() or container methods instead"
    )]
    fn frame(self, width: f32, height: f32) -> Frame<Self, M> {
        Frame::new(self, FrameSize::fixed(width, height))
    }

    // ... other frame methods with same deprecation ...
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check -p vexo`
Expected: Warnings about deprecated items, no errors

- [ ] **Step 4: Commit**

```bash
git add vexo/src/widgets/modifiers.rs
git commit -m "deprecate(modifiers): mark Padding and Frame wrappers as deprecated

Add deprecation warnings with migration guidance.
Users should use container's .padding() method or Layout::fixed() instead."
```

---

## Task 7: Update shared_app Example

**Files:**
- Modify: `shared_app/src/lib.rs`

- [ ] **Step 1: Read current shared_app implementation**

Read the file to understand current usage patterns.

- [ ] **Step 2: Update to use new Layout API**

Replace deprecated `.padding()` and `.frame()` calls with container methods. Example changes:

```rust
// Before (deprecated):
text!("Modified Text", font_size: 24.0)
    .padding(10.0)
    .background(Color::RED)
    .border(Color::GREEN, 2.0)
    .corner_radius(8.0)
    .boxed()

// After (new API):
Column::new()
    .padding(10.0)
    .push(
        text!("Modified Text", font_size: 24.0)
            .background(Color::RED)
            .border(Color::GREEN, 2.0)
            .corner_radius(8.0)
    )
    .boxed()
```

- [ ] **Step 3: Run cargo build**

Run: `cargo build -p shared_app`
Expected: Success with deprecation warnings

- [ ] **Step 4: Run desktop demo to verify**

Run: `cargo run -p desktop_demo`
Expected: Application runs with same visual output

- [ ] **Step 5: Commit**

```bash
git add shared_app/src/lib.rs
git commit -m "refactor(shared_app): migrate to new Layout API

Replace deprecated .padding() and .frame() with container methods.
Visual output unchanged."
```

---

## Task 8: Add Unit Tests

**Files:**
- Modify: `vexo/src/layout/style.rs`

- [ ] **Step 1: Add tests for Layout to Taffy conversion**

Add to `vexo/src/layout/style.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_insets_all() {
        let insets = EdgeInsets::all(10.0);
        assert_eq!(insets.left, 10.0);
        assert_eq!(insets.right, 10.0);
        assert_eq!(insets.top, 10.0);
        assert_eq!(insets.bottom, 10.0);
    }

    #[test]
    fn test_edge_insets_symmetric() {
        let insets = EdgeInsets::symmetric(5.0, 10.0);
        assert_eq!(insets.left, 5.0);
        assert_eq!(insets.right, 5.0);
        assert_eq!(insets.top, 10.0);
        assert_eq!(insets.bottom, 10.0);
    }

    #[test]
    fn test_layout_padding() {
        let layout = Layout::default().padding(10.0);
        assert!(layout.padding.is_some());
        let p = layout.padding.unwrap();
        assert_eq!(p.left, 10.0);
    }

    #[test]
    fn test_layout_margin() {
        let layout = Layout::default().margin(5.0);
        assert!(layout.margin.is_some());
        let m = layout.margin.unwrap();
        assert_eq!(m.top, 5.0);
    }

    #[test]
    fn test_layout_flex_wrap() {
        let layout = Layout::default().flex_wrap();
        assert_eq!(layout.flex_wrap, Some(FlexWrap::Wrap));
    }

    #[test]
    fn test_layout_justify() {
        let layout = Layout::default().justify(JustifyContent::SpaceBetween);
        assert_eq!(layout.justify_content, Some(JustifyContent::SpaceBetween));
    }

    #[test]
    fn test_layout_absolute() {
        let layout = Layout::default().absolute().top(10.0).right(5.0);
        assert_eq!(layout.position, Some(Position::Absolute));
        assert!(layout.inset.is_some());
        let inset = layout.inset.unwrap();
        assert_eq!(inset.top, Some(10.0));
        assert_eq!(inset.right, Some(5.0));
    }

    #[test]
    fn test_layout_to_taffy_style() {
        let layout = Layout::default()
            .padding(10.0)
            .margin(5.0)
            .flex_grow(1.0);

        let style = layout.to_taffy_style();
        assert_eq!(style.flex_grow, 1.0);
    }

    #[test]
    fn test_grid_placement() {
        let p = GridPlacement::span(2);
        assert_eq!(p.span, 2);

        let p = GridPlacement::start(1);
        assert_eq!(p.start, 1);
    }

    #[test]
    fn test_track_sizing() {
        let track = TrackSizing::Fr(1.0);
        assert!(matches!(track, TrackSizing::Fr(1.0)));

        let track = TrackSizing::Px(100.0);
        assert!(matches!(track, TrackSizing::Px(100.0)));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p vexo`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add vexo/src/layout/style.rs
git commit -m "test(layout): add unit tests for Layout struct and types

Test EdgeInsets, Layout builders, absolute positioning, grid placement"
```

---

## Verification Checklist

After completing all tasks:

- [ ] Run `cargo test -p vexo` - all tests pass
- [ ] Run `cargo build -p vexo` - no errors
- [ ] Run `cargo run -p desktop_demo` - application runs correctly
- [ ] Visual verification - layouts render as expected
- [ ] Deprecation warnings appear for old API usage

## Summary

This plan implements the Hybrid CSS-Taffy layout system:

1. **Layout struct** - CSS-style properties that map directly to Taffy
2. **Container builder methods** - `.padding()`, `.margin()`, `.gap()`, `.flex_wrap()`, `.justify()`, `.align()`
3. **Grid widget** - 2D layouts with columns/rows
4. **Deprecated wrappers** - Padding and Frame marked for removal
5. **Updated macros** - `grid!`, enhanced `column!`/`row!`

The result: single Taffy node per widget, full CSS feature coverage, cleaner API.
