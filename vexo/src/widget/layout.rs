//! Layout trait for widget layout participation.

use crate::core::{Rect, Logical, Size};

/// Layout constraints that describe how a widget should be sized.
///
/// These constraints are provided by widgets during the layout phase
/// and used by the layout engine to compute final positions and sizes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConstraints {
    /// Minimum width in logical points.
    pub min_width: f32,
    /// Maximum width in logical points (f32::INFINITY for unbounded).
    pub max_width: f32,
    /// Minimum height in logical points.
    pub min_height: f32,
    /// Maximum height in logical points (f32::INFINITY for unbounded).
    pub max_height: f32,
    /// How much this widget should grow relative to siblings.
    pub flex_grow: f32,
    /// How much this widget should shrink relative to siblings.
    pub flex_shrink: f32,
}

impl Default for LayoutConstraints {
    fn default() -> Self {
        Self {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
            flex_grow: 0.0,
            flex_shrink: 1.0,
        }
    }
}

impl LayoutConstraints {
    /// Create constraints for a fixed-size widget.
    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            min_width: width,
            max_width: width,
            min_height: height,
            max_height: height,
            flex_grow: 0.0,
            flex_shrink: 0.0,
        }
    }

    /// Create constraints for a fixed-size widget using a Size value.
    pub fn fixed_size(size: Size<Logical>) -> Self {
        Self::fixed(size.width, size.height)
    }

    /// Create constraints for a widget that fills available space.
    pub fn fill() -> Self {
        Self {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
            flex_grow: 1.0,
            flex_shrink: 1.0,
        }
    }

    /// Check if the width is fixed (min == max).
    pub fn is_fixed_width(&self) -> bool {
        (self.min_width - self.max_width).abs() < f32::EPSILON
    }

    /// Check if the height is fixed (min == max).
    pub fn is_fixed_height(&self) -> bool {
        (self.min_height - self.max_height).abs() < f32::EPSILON
    }
}

/// The computed layout result delivered to a widget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComputedLayout {
    /// The bounds (position and size) in logical coordinates.
    pub bounds: Rect<Logical>,
}

impl Default for ComputedLayout {
    fn default() -> Self {
        Self {
            bounds: Rect::from_xywh(0.0, 0.0, 0.0, 0.0),
        }
    }
}

impl ComputedLayout {
    /// Create a new computed layout.
    pub fn new(bounds: Rect<Logical>) -> Self {
        Self { bounds }
    }

    /// Get the x position.
    pub fn x(&self) -> f32 {
        self.bounds.origin.x
    }

    /// Get the y position.
    pub fn y(&self) -> f32 {
        self.bounds.origin.y
    }

    /// Get the width.
    pub fn width(&self) -> f32 {
        self.bounds.size.width
    }

    /// Get the height.
    pub fn height(&self) -> f32 {
        self.bounds.size.height
    }
}

/// Trait for widgets that participate in layout.
///
/// Widgets describe their layout constraints, and the layout engine
/// computes final positions and sizes. After computation, widgets
/// receive their computed layout via `apply_layout`.
///
/// # Example
///
/// ```
/// use vexo::widget::{Layout, LayoutConstraints, ComputedLayout};
///
/// struct FixedSizeWidget {
///     layout: Option<ComputedLayout>,
/// }
///
/// impl Layout for FixedSizeWidget {
///     fn constraints(&self) -> LayoutConstraints {
///         LayoutConstraints::fixed(100.0, 50.0)
///     }
///
///     fn apply_layout(&mut self, layout: ComputedLayout) {
///         self.layout = Some(layout);
///     }
/// }
/// ```
pub trait Layout {
    /// Describe the layout constraints for this widget.
    ///
    /// The layout engine uses these constraints to compute the final size.
    fn constraints(&self) -> LayoutConstraints;

    /// Receive the computed layout after layout computation.
    ///
    /// Called by the framework after the layout engine has computed
    /// positions and sizes. Widgets should store this for use in `paint`.
    fn apply_layout(&mut self, layout: ComputedLayout);
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_computed_layout_default() {
        let layout = ComputedLayout::default();
        assert_eq!(layout.x(), 0.0);
        assert_eq!(layout.y(), 0.0);
        assert_eq!(layout.width(), 0.0);
        assert_eq!(layout.height(), 0.0);
    }

    #[test]
    fn test_computed_layout_new() {
        let layout = ComputedLayout::new(Rect::from_xywh(10.0, 20.0, 100.0, 50.0));
        assert_eq!(layout.x(), 10.0);
        assert_eq!(layout.y(), 20.0);
        assert_eq!(layout.width(), 100.0);
        assert_eq!(layout.height(), 50.0);
    }
}
