//! CSS-style layout properties that map directly to Taffy.
//!
//! This module provides a builder-style struct for specifying layout
//! properties like padding, margin, flex, and grid settings.

use crate::core::{Size, Logical};

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

    /// Create an absolute positioned layout with specific insets.
    pub fn absolute_at(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self::default()
            .absolute()
            .top(top)
            .right(right)
            .bottom(bottom)
            .left(left)
    }
}

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
            }).unwrap_or_else(Rect::zero),
            margin: self.margin.map(|m| Rect {
                left: length(m.left),
                right: length(m.right),
                top: length(m.top),
                bottom: length(m.bottom),
            }).unwrap_or_else(Rect::zero),
            size: Size {
                width: self.width.map(|d| d.to_taffy()).unwrap_or_else(auto),
                height: self.height.map(|d| d.to_taffy()).unwrap_or_else(auto),
            },
            min_size: Size {
                width: self.min_width.map(|d| d.to_taffy()).unwrap_or_else(auto),
                height: self.min_height.map(|d| d.to_taffy()).unwrap_or_else(auto),
            },
            max_size: Size {
                width: self.max_width.map(|d| d.to_taffy()).unwrap_or_else(auto),
                height: self.max_height.map(|d| d.to_taffy()).unwrap_or_else(auto),
            },

            // Flexbox
            display: Display::Flex,
            flex_direction: self.flex_direction.map(|d| d.to_taffy()).unwrap_or_default(),
            flex_wrap: self.flex_wrap.map(|w| w.to_taffy()).unwrap_or_default(),
            flex_grow: self.flex_grow.unwrap_or(0.0),
            flex_shrink: self.flex_shrink.unwrap_or(1.0),
            flex_basis: self.flex_basis.map(|d| d.to_taffy()).unwrap_or_else(auto),
            justify_content: self.justify_content.map(|j| j.to_taffy()),
            align_items: self.align_items.map(|a| a.to_taffy()),
            align_content: self.align_content.map(|a| a.to_taffy()),
            gap: self.gap.map(|g| Size {
                width: length(g.width),
                height: length(g.height),
            }).unwrap_or_else(Size::zero),

            // Grid - use GridTemplateComponent for templates
            grid_template_columns: self.grid_template_columns.as_ref()
                .map(|t| t.iter().map(|ts| ts.to_taffy_template()).collect())
                .unwrap_or_default(),
            grid_template_rows: self.grid_template_rows.as_ref()
                .map(|t| t.iter().map(|ts| ts.to_taffy_template()).collect())
                .unwrap_or_default(),
            grid_column: self.grid_column.map(|p| p.to_taffy_line()).unwrap_or_default(),
            grid_row: self.grid_row.map(|p| p.to_taffy_line()).unwrap_or_default(),

            // Positioning
            position: self.position.map(|p| p.to_taffy()).unwrap_or_default(),
            inset: self.inset.map(|i| i.to_taffy()).unwrap_or_else(Rect::auto),

            ..Default::default()
        }
    }
}

// ============================================================================
// TAFFY CONVERSION TRAITS
// ============================================================================

impl Dimension {
    /// Convert to Taffy dimension.
    pub fn to_taffy(self) -> taffy::prelude::Dimension {
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
    fn to_taffy(self) -> taffy::prelude::JustifyContent {
        use taffy::prelude::JustifyContent as TaffyJustify;
        match self {
            JustifyContent::Start => TaffyJustify::Start,
            JustifyContent::End => TaffyJustify::End,
            JustifyContent::Center => TaffyJustify::Center,
            JustifyContent::SpaceBetween => TaffyJustify::SpaceBetween,
            JustifyContent::SpaceAround => TaffyJustify::SpaceAround,
            JustifyContent::SpaceEvenly => TaffyJustify::SpaceEvenly,
        }
    }
}

impl AlignItems {
    fn to_taffy(self) -> taffy::prelude::AlignItems {
        use taffy::prelude::AlignItems as TaffyAlign;
        match self {
            AlignItems::Stretch => TaffyAlign::Stretch,
            AlignItems::Start => TaffyAlign::Start,
            AlignItems::End => TaffyAlign::End,
            AlignItems::Center => TaffyAlign::Center,
            AlignItems::Baseline => TaffyAlign::Baseline,
        }
    }
}

impl AlignContent {
    fn to_taffy(self) -> taffy::prelude::AlignContent {
        use taffy::prelude::AlignContent as TaffyAlign;
        match self {
            AlignContent::Start => TaffyAlign::Start,
            AlignContent::End => TaffyAlign::End,
            AlignContent::Center => TaffyAlign::Center,
            AlignContent::Stretch => TaffyAlign::Stretch,
            AlignContent::SpaceBetween => TaffyAlign::SpaceBetween,
            AlignContent::SpaceAround => TaffyAlign::SpaceAround,
        }
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
            top: self.top.map(taffy::prelude::length).unwrap_or_else(taffy::prelude::auto),
            right: self.right.map(taffy::prelude::length).unwrap_or_else(taffy::prelude::auto),
            bottom: self.bottom.map(taffy::prelude::length).unwrap_or_else(taffy::prelude::auto),
            left: self.left.map(taffy::prelude::length).unwrap_or_else(taffy::prelude::auto),
        }
    }
}

impl TrackSizing {
    /// Convert to a Taffy track sizing function for grid auto tracks.
    fn to_taffy(&self) -> taffy::prelude::TrackSizingFunction {
        use taffy::prelude::*;
        match self {
            TrackSizing::Auto => auto(),
            TrackSizing::Fr(v) => fr(*v),
            TrackSizing::Px(v) => length(*v),
            TrackSizing::Percent(v) => percent(*v),
            TrackSizing::MinMax { min, max } => minmax(min.to_taffy_min(), max.to_taffy_max()),
        }
    }

    /// Convert to a Taffy GridTemplateComponent for grid template tracks.
    fn to_taffy_template(&self) -> taffy::prelude::GridTemplateComponent<String> {
        use taffy::prelude::*;
        match self {
            TrackSizing::Auto => GridTemplateComponent::Single(auto()),
            TrackSizing::Fr(v) => GridTemplateComponent::Single(fr(*v)),
            TrackSizing::Px(v) => GridTemplateComponent::Single(length(*v)),
            TrackSizing::Percent(v) => GridTemplateComponent::Single(percent(*v)),
            TrackSizing::MinMax { min, max } => GridTemplateComponent::Single(
                minmax(min.to_taffy_min(), max.to_taffy_max())
            ),
        }
    }

    fn to_taffy_min(&self) -> taffy::prelude::MinTrackSizingFunction {
        use taffy::prelude::*;
        match self {
            TrackSizing::Auto => auto(),
            TrackSizing::Fr(_) => zero(), // fr is only valid for max
            TrackSizing::Px(v) => length(*v),
            TrackSizing::Percent(v) => percent(*v),
            TrackSizing::MinMax { min, .. } => min.to_taffy_min(),
        }
    }

    fn to_taffy_max(&self) -> taffy::prelude::MaxTrackSizingFunction {
        use taffy::prelude::*;
        match self {
            TrackSizing::Auto => auto(),
            TrackSizing::Fr(v) => fr(*v),
            TrackSizing::Px(v) => length(*v),
            TrackSizing::Percent(v) => percent(*v),
            TrackSizing::MinMax { max, .. } => max.to_taffy_max(),
        }
    }
}

impl GridPlacement {
    /// Convert to a Taffy Line<GridPlacement> for grid item placement.
    fn to_taffy_line(self) -> taffy::Line<taffy::prelude::GridPlacement> {
        use taffy::prelude::*;
        if self.start < 0 {
            // Auto placement with span
            Line {
                start: GridPlacement::Span(self.span),
                end: GridPlacement::Auto,
            }
        } else {
            // Specific line placement
            Line {
                start: GridPlacement::from_line_index(self.start),
                end: if self.span > 1 {
                    GridPlacement::Span(self.span)
                } else {
                    GridPlacement::Auto
                },
            }
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

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
    fn test_edge_insets_horizontal() {
        let insets = EdgeInsets::horizontal(5.0);
        assert_eq!(insets.left, 5.0);
        assert_eq!(insets.right, 5.0);
        assert_eq!(insets.top, 0.0);
        assert_eq!(insets.bottom, 0.0);
    }

    #[test]
    fn test_edge_insets_vertical() {
        let insets = EdgeInsets::vertical(8.0);
        assert_eq!(insets.left, 0.0);
        assert_eq!(insets.right, 0.0);
        assert_eq!(insets.top, 8.0);
        assert_eq!(insets.bottom, 8.0);
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
    fn test_layout_default() {
        let layout = Layout::default();
        assert!(layout.padding.is_none());
        assert!(layout.margin.is_none());
        assert!(layout.width.is_none());
        assert!(layout.height.is_none());
    }

    #[test]
    fn test_layout_padding() {
        let layout = Layout::default().padding(10.0);
        assert!(layout.padding.is_some());
        let p = layout.padding.unwrap();
        assert_eq!(p.left, 10.0);
        assert_eq!(p.right, 10.0);
        assert_eq!(p.top, 10.0);
        assert_eq!(p.bottom, 10.0);
    }

    #[test]
    fn test_layout_padding_each() {
        let layout = Layout::default().padding_each(1.0, 2.0, 3.0, 4.0);
        let p = layout.padding.unwrap();
        assert_eq!(p.left, 1.0);
        assert_eq!(p.right, 2.0);
        assert_eq!(p.top, 3.0);
        assert_eq!(p.bottom, 4.0);
    }

    #[test]
    fn test_layout_margin() {
        let layout = Layout::default().margin(5.0);
        assert!(layout.margin.is_some());
        let m = layout.margin.unwrap();
        assert_eq!(m.top, 5.0);
    }

    #[test]
    fn test_layout_width_height() {
        let layout = Layout::default().width(100.0).height(50.0);
        assert_eq!(layout.width, Some(Dimension::Length(100.0)));
        assert_eq!(layout.height, Some(Dimension::Length(50.0)));
    }

    #[test]
    fn test_layout_percent_dimensions() {
        let layout = Layout::default().width_percent(0.5).height_percent(0.25);
        assert_eq!(layout.width, Some(Dimension::Percent(0.5)));
        assert_eq!(layout.height, Some(Dimension::Percent(0.25)));
    }

    #[test]
    fn test_layout_min_max() {
        let layout = Layout::default()
            .min_width(50.0)
            .min_height(30.0)
            .max_width(200.0)
            .max_height(100.0);
        assert_eq!(layout.min_width, Some(Dimension::Length(50.0)));
        assert_eq!(layout.min_height, Some(Dimension::Length(30.0)));
        assert_eq!(layout.max_width, Some(Dimension::Length(200.0)));
        assert_eq!(layout.max_height, Some(Dimension::Length(100.0)));
    }

    #[test]
    fn test_layout_flex_direction() {
        let layout = Layout::default().flex_direction(FlexDirection::Column);
        assert_eq!(layout.flex_direction, Some(FlexDirection::Column));
    }

    #[test]
    fn test_layout_flex_wrap() {
        let layout = Layout::default().flex_wrap();
        assert_eq!(layout.flex_wrap, Some(FlexWrap::Wrap));
    }

    #[test]
    fn test_layout_flex_grow_shrink() {
        let layout = Layout::default().flex_grow(2.0).flex_shrink(0.5);
        assert_eq!(layout.flex_grow, Some(2.0));
        assert_eq!(layout.flex_shrink, Some(0.5));
    }

    #[test]
    fn test_layout_justify() {
        let layout = Layout::default().justify(JustifyContent::SpaceBetween);
        assert_eq!(layout.justify_content, Some(JustifyContent::SpaceBetween));
    }

    #[test]
    fn test_layout_align() {
        let layout = Layout::default().align(AlignItems::Center);
        assert_eq!(layout.align_items, Some(AlignItems::Center));
    }

    #[test]
    fn test_layout_align_content() {
        let layout = Layout::default().align_content(AlignContent::Stretch);
        assert_eq!(layout.align_content, Some(AlignContent::Stretch));
    }

    #[test]
    fn test_layout_gap() {
        let layout = Layout::default().gap(10.0);
        assert!(layout.gap.is_some());
        let g = layout.gap.unwrap();
        assert_eq!(g.width, 10.0);
        assert_eq!(g.height, 10.0);
    }

    #[test]
    fn test_layout_gap_each() {
        let layout = Layout::default().gap_each(5.0, 10.0);
        let g = layout.gap.unwrap();
        assert_eq!(g.width, 5.0);
        assert_eq!(g.height, 10.0);
    }

    #[test]
    fn test_layout_absolute() {
        let layout = Layout::default().absolute().top(10.0).right(5.0);
        assert_eq!(layout.position, Some(Position::Absolute));
        assert!(layout.inset.is_some());
        let inset = layout.inset.unwrap();
        assert_eq!(inset.top, Some(10.0));
        assert_eq!(inset.right, Some(5.0));
        assert_eq!(inset.bottom, None);
        assert_eq!(inset.left, None);
    }

    #[test]
    fn test_layout_absolute_at() {
        let layout = Layout::absolute_at(10.0, 20.0, 30.0, 40.0);
        assert_eq!(layout.position, Some(Position::Absolute));
        let inset = layout.inset.unwrap();
        assert_eq!(inset.top, Some(10.0));
        assert_eq!(inset.right, Some(20.0));
        assert_eq!(inset.bottom, Some(30.0));
        assert_eq!(inset.left, Some(40.0));
    }

    #[test]
    fn test_layout_inset() {
        let layout = Layout::default().inset(5.0);
        let inset = layout.inset.unwrap();
        assert_eq!(inset.top, Some(5.0));
        assert_eq!(inset.right, Some(5.0));
        assert_eq!(inset.bottom, Some(5.0));
        assert_eq!(inset.left, Some(5.0));
    }

    #[test]
    fn test_layout_grid_columns_rows() {
        let layout = Layout::default()
            .columns(vec![TrackSizing::Fr(1.0), TrackSizing::Fr(2.0)])
            .rows(vec![TrackSizing::Auto, TrackSizing::Px(100.0)]);
        assert!(layout.grid_template_columns.is_some());
        assert!(layout.grid_template_rows.is_some());
        let cols = layout.grid_template_columns.unwrap();
        assert_eq!(cols.len(), 2);
        assert!(matches!(cols[0], TrackSizing::Fr(1.0)));
        assert!(matches!(cols[1], TrackSizing::Fr(2.0)));
    }

    #[test]
    fn test_layout_grid_placement() {
        let layout = Layout::default()
            .grid_column(GridPlacement::span(2))
            .grid_row(GridPlacement::start(1));
        assert!(layout.grid_column.is_some());
        assert!(layout.grid_row.is_some());
        let col = layout.grid_column.unwrap();
        assert_eq!(col.span, 2);
        let row = layout.grid_row.unwrap();
        assert_eq!(row.start, 1);
    }

    #[test]
    fn test_layout_fill() {
        let layout = Layout::fill();
        assert_eq!(layout.flex_grow, Some(1.0));
    }

    #[test]
    fn test_layout_fixed() {
        let layout = Layout::fixed(100.0, 50.0);
        assert_eq!(layout.width, Some(Dimension::Length(100.0)));
        assert_eq!(layout.height, Some(Dimension::Length(50.0)));
    }

    #[test]
    fn test_grid_placement_auto() {
        let p = GridPlacement::auto();
        assert_eq!(p.start, -1);
        assert_eq!(p.span, 1);
    }

    #[test]
    fn test_grid_placement_start() {
        let p = GridPlacement::start(2);
        assert_eq!(p.start, 2);
        assert_eq!(p.span, 1);
    }

    #[test]
    fn test_grid_placement_span() {
        let p = GridPlacement::span(3);
        assert_eq!(p.start, -1);
        assert_eq!(p.span, 3);
    }

    #[test]
    fn test_grid_placement_start_span() {
        let p = GridPlacement::start_span(1, 2);
        assert_eq!(p.start, 1);
        assert_eq!(p.span, 2);
    }

    #[test]
    fn test_track_sizing() {
        let track = TrackSizing::Fr(1.0);
        assert!(matches!(track, TrackSizing::Fr(1.0)));

        let track = TrackSizing::Px(100.0);
        assert!(matches!(track, TrackSizing::Px(100.0)));

        let track = TrackSizing::Percent(0.5);
        assert!(matches!(track, TrackSizing::Percent(0.5)));

        let track = TrackSizing::Auto;
        assert!(matches!(track, TrackSizing::Auto));
    }

    #[test]
    fn test_track_sizing_minmax() {
        let track = TrackSizing::MinMax {
            min: Box::new(TrackSizing::Px(50.0)),
            max: Box::new(TrackSizing::Fr(1.0)),
        };
        assert!(matches!(
            track,
            TrackSizing::MinMax { min, max } if matches!(*min, TrackSizing::Px(50.0)) && matches!(*max, TrackSizing::Fr(1.0))
        ));
    }

    #[test]
    fn test_layout_to_taffy_style() {
        let layout = Layout::default()
            .padding(10.0)
            .margin(5.0)
            .flex_grow(1.0);

        let style = layout.to_taffy_style();
        assert_eq!(style.flex_grow, 1.0);
        assert_eq!(style.flex_shrink, 1.0); // default value

        // Create a default style to compare against
        let default_style: taffy::Style = Layout::default().to_taffy_style();

        // Verify padding was set (should differ from default which is zero)
        // Since LengthPercentage doesn't expose internal values, we can verify by checking
        // that padding is not equal to the default (zero)
        assert_ne!(style.padding.left, default_style.padding.left);
        assert_ne!(style.padding.top, default_style.padding.top);
    }

    #[test]
    fn test_layout_to_taffy_style_flex_direction() {
        let layout = Layout::default().flex_direction(FlexDirection::Column);
        let style = layout.to_taffy_style();
        assert_eq!(style.flex_direction, taffy::prelude::FlexDirection::Column);
    }

    #[test]
    fn test_layout_to_taffy_style_position() {
        let layout = Layout::default().absolute();
        let style = layout.to_taffy_style();
        assert_eq!(style.position, taffy::prelude::Position::Absolute);
    }

    #[test]
    fn test_layout_chained_builders() {
        let layout = Layout::default()
            .padding(10.0)
            .margin(5.0)
            .gap(8.0)
            .flex_wrap()
            .justify(JustifyContent::SpaceBetween)
            .align(AlignItems::Center);

        assert!(layout.padding.is_some());
        assert!(layout.margin.is_some());
        assert!(layout.gap.is_some());
        assert_eq!(layout.flex_wrap, Some(FlexWrap::Wrap));
        assert_eq!(layout.justify_content, Some(JustifyContent::SpaceBetween));
        assert_eq!(layout.align_items, Some(AlignItems::Center));
    }

    #[test]
    fn test_dimension_default() {
        let d = Dimension::default();
        assert!(matches!(d, Dimension::Auto));
    }

    #[test]
    fn test_flex_direction_default() {
        let d = FlexDirection::default();
        assert!(matches!(d, FlexDirection::Row));
    }

    #[test]
    fn test_flex_wrap_default() {
        let w = FlexWrap::default();
        assert!(matches!(w, FlexWrap::NoWrap));
    }

    #[test]
    fn test_justify_content_default() {
        let j = JustifyContent::default();
        assert!(matches!(j, JustifyContent::Start));
    }

    #[test]
    fn test_align_items_default() {
        let a = AlignItems::default();
        assert!(matches!(a, AlignItems::Stretch));
    }

    #[test]
    fn test_align_content_default() {
        let a = AlignContent::default();
        assert!(matches!(a, AlignContent::Start));
    }

    #[test]
    fn test_position_default() {
        let p = Position::default();
        assert!(matches!(p, Position::Relative));
    }
}
