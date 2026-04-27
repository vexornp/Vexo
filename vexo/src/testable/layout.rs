//! Layout trait for widget layout participation.

use crate::core::{Bounds, Logical, Size};

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

    /// Create constraints from a Layout struct.
    ///
    /// This converts the CSS-style Layout properties into LayoutConstraints
    /// that can be used by the layout engine.
    pub fn from_layout(layout: &crate::layout::Layout) -> Self {
        use crate::layout::Dimension;

        // Convert width dimension to min/max constraints
        let (min_width, max_width) = match &layout.width {
            Some(Dimension::Length(w)) => (*w, *w),
            Some(Dimension::Percent(_)) => (0.0, f32::INFINITY), // Percentage not directly supported
            Some(Dimension::Auto) | None => (0.0, f32::INFINITY),
        };

        // Convert height dimension to min/max constraints
        let (min_height, max_height) = match &layout.height {
            Some(Dimension::Length(h)) => (*h, *h),
            Some(Dimension::Percent(_)) => (0.0, f32::INFINITY), // Percentage not directly supported
            Some(Dimension::Auto) | None => (0.0, f32::INFINITY),
        };

        Self {
            min_width,
            max_width,
            min_height,
            max_height,
            flex_grow: layout.flex_grow.unwrap_or(0.0),
            flex_shrink: layout.flex_shrink.unwrap_or(1.0),
        }
    }
}

/// The computed layout result delivered to a widget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComputedLayout {
    /// The bounds (position and size) in logical coordinates.
    pub bounds: Bounds<Logical>,
}

impl Default for ComputedLayout {
    fn default() -> Self {
        Self {
            bounds: Bounds::from_xywh(0.0, 0.0, 0.0, 0.0),
        }
    }
}

impl ComputedLayout {
    /// Create a new computed layout.
    pub fn new(bounds: Bounds<Logical>) -> Self {
        Self { bounds }
    }

    /// Get the x position.
    pub fn x(&self) -> f32 {
        self.bounds.left
    }

    /// Get the y position.
    pub fn y(&self) -> f32 {
        self.bounds.top
    }

    /// Get the width.
    pub fn width(&self) -> f32 {
        self.bounds.width()
    }

    /// Get the height.
    pub fn height(&self) -> f32 {
        self.bounds.height()
    }
}

/// Trait for widgets that participate in layout.
///
/// Widgets describe their layout constraints, and the layout engine
/// computes final positions and sizes. After computation, widgets
/// receive their computed layout via `apply_layout`.
///
/// # Implementation Pattern
///
/// Widgets should store the `ComputedLayout` received in `apply_layout`
/// for use during the paint phase:
///
/// ```ignore
/// struct MyWidget {
///     layout: Option<ComputedLayout>,
/// }
///
/// impl Layout for MyWidget {
///     fn constraints(&self) -> LayoutConstraints {
///         LayoutConstraints::fixed(100.0, 50.0)
///     }
///
///     fn apply_layout(&mut self, layout: ComputedLayout) {
///         self.layout = Some(layout);
///     }
/// }
/// ```
///
/// # Example
///
/// ```
/// use vexo::testable::{Layout, LayoutConstraints, ComputedLayout};
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
        let layout = ComputedLayout::new(Bounds::from_xywh(10.0, 20.0, 100.0, 50.0));
        assert_eq!(layout.x(), 10.0);
        assert_eq!(layout.y(), 20.0);
        assert_eq!(layout.width(), 100.0);
        assert_eq!(layout.height(), 50.0);
    }

    #[test]
    fn test_layout_constraints_from_layout() {
        use crate::layout::Layout;

        let layout = Layout::default().width(100.0).height(50.0);
        let constraints = LayoutConstraints::from_layout(&layout);

        assert!(constraints.is_fixed_width());
        assert!(constraints.is_fixed_height());
        assert_eq!(constraints.min_width, 100.0);
        assert_eq!(constraints.min_height, 50.0);
    }
}
