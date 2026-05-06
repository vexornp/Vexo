//! Visual decoration properties for containers.
//!
//! Style is analogous to Flutter's BoxDecoration - it holds all visual
//! properties in one place for efficient single-pass rendering.

use crate::core::Color;

/// Visual decoration properties for a DecoratedContainer.
///
/// Holds all decoration properties (background, border, corner radius)
/// in a single struct for efficient rendering. This allows multiple
/// decorations to be applied with a single element and render object.
///
/// # Example
///
/// ```
/// let style = Style::new()
///     .background(Color::RED)
///     .border(Color::BLACK, 2.0)
///     .corner_radius(8.0);
/// ```
#[derive(Clone, Debug, Default)]
pub struct Style {
    /// Background color (drawn behind child).
    pub background: Option<Color>,

    /// Border decoration.
    pub border: Option<Border>,

    /// Corner radius for rounded rectangles.
    pub corner_radius: Option<CornerRadius>,
}

/// Border decoration properties.
#[derive(Clone, Debug)]
pub struct Border {
    /// Border color.
    pub color: Color,
    /// Border width in logical pixels.
    pub width: f32,
}

/// Corner radius for rounded rectangles.
#[derive(Clone, Debug)]
pub struct CornerRadius {
    /// Radius for all corners (uniform).
    pub radius: f32,
}

impl Style {
    /// Create a new empty style.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set background color.
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Set border with color and width.
    pub fn border(mut self, color: Color, width: f32) -> Self {
        self.border = Some(Border { color, width });
        self
    }

    /// Set uniform corner radius for all corners.
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = Some(CornerRadius { radius });
        self
    }
}
