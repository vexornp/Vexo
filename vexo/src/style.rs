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
/// ```ignore
/// let style = Style::new()
///     .background(Color::RED)
///     .border(Color::BLACK, 2.0)
///     .corner_radius(8.0);
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    /// Background color (drawn behind child).
    pub background: Option<Color>,

    /// Border decoration.
    pub border: Option<Border>,

    /// Corner radius for rounded rectangles.
    pub corner_radius: Option<CornerRadius>,

    /// Whether to clip children to this container's bounds.
    pub clip: bool,
}

/// Border decoration properties.
#[derive(Clone, Debug, PartialEq)]
pub struct Border {
    /// Border color.
    pub color: Color,
    /// Border width in logical pixels.
    pub width: f32,
}

/// Corner radius for rounded rectangles.
#[derive(Clone, Debug, PartialEq)]
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

    /// Enable clipping of children to this container's bounds.
    pub fn clip(mut self) -> Self {
        self.clip = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_new() {
        let style = Style::new();

        assert!(style.background.is_none());
        assert!(style.border.is_none());
        assert!(style.corner_radius.is_none());
    }

    #[test]
    fn test_style_default() {
        let style = Style::default();

        assert!(style.background.is_none());
        assert!(style.border.is_none());
        assert!(style.corner_radius.is_none());
    }

    #[test]
    fn test_style_builder_background() {
        let style = Style::new().background(Color::RED);

        assert_eq!(style.background, Some(Color::RED));
    }

    #[test]
    fn test_style_builder_border() {
        let style = Style::new().border(Color::BLACK, 2.0);

        let border = style.border.unwrap();
        assert_eq!(border.color, Color::BLACK);
        assert_eq!(border.width, 2.0);
    }

    #[test]
    fn test_style_builder_corner_radius() {
        let style = Style::new().corner_radius(8.0);

        let cr = style.corner_radius.unwrap();
        assert_eq!(cr.radius, 8.0);
    }

    #[test]
    fn test_style_builder_all_properties() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0)
            .corner_radius(8.0);

        assert_eq!(style.background, Some(Color::RED));
        assert_eq!(style.border.unwrap().color, Color::BLACK);
        assert_eq!(style.corner_radius.unwrap().radius, 8.0);
    }

    #[test]
    fn test_style_clone() {
        let style = Style::new()
            .background(Color::RED)
            .border(Color::BLACK, 2.0);

        let cloned = style.clone();

        assert_eq!(cloned.background, Some(Color::RED));
        assert_eq!(cloned.border.unwrap().color, Color::BLACK);
    }
}
