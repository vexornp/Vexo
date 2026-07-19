//! Visual decoration properties for containers.
//!
//! Style is analogous to Flutter's BoxDecoration - it holds all visual
//! properties in one place for efficient single-pass rendering.

use crate::core::{Color, Logical, Point};

#[derive(Clone, Debug, PartialEq)]
pub struct BoxShadow {
    pub color: Color,
    pub offset: Point<Logical>,
    pub blur_radius: f32,
    pub spread_radius: f32,
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            color: Color::TRANSPARENT,
            offset: Point::zero(),
            blur_radius: 0.0,
            spread_radius: 0.0,
        }
    }
}

impl BoxShadow {
    pub fn new(color: Color) -> Self {
        Self {
            color,
            offset: Point::zero(),
            blur_radius: 0.0,
            spread_radius: 0.0,
        }
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = Point::new(x, y);
        self
    }

    pub fn blur(mut self, radius: f32) -> Self {
        self.blur_radius = radius;
        self
    }

    pub fn spread(mut self, radius: f32) -> Self {
        self.spread_radius = radius;
        self
    }
}

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

    pub shadows: Vec<BoxShadow>,
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

    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadows.push(shadow);
        self
    }

    pub fn shadows(mut self, shadows: Vec<BoxShadow>) -> Self {
        self.shadows = shadows;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Logical, Point};
    use crate::Color;

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

    #[test]
    fn test_box_shadow_new() {
        let s = BoxShadow::new(Color::RED);
        assert_eq!(s.color, Color::RED);
        assert_eq!(s.offset, Point::<Logical>::zero());
        assert_eq!(s.blur_radius, 0.0);
        assert_eq!(s.spread_radius, 0.0);
    }

    #[test]
    fn test_box_shadow_builder_chain() {
        let s = BoxShadow::new(Color::BLACK)
            .offset(2.0, 4.0)
            .blur(12.0)
            .spread(2.0);
        assert_eq!(s.color, Color::BLACK);
        assert_eq!(s.offset, Point::new(2.0, 4.0));
        assert_eq!(s.blur_radius, 12.0);
        assert_eq!(s.spread_radius, 2.0);
    }

    #[test]
    fn test_box_shadow_default() {
        let s = BoxShadow::default();
        assert_eq!(s.color, Color::TRANSPARENT);
        assert_eq!(s.offset, Point::<Logical>::zero());
        assert_eq!(s.blur_radius, 0.0);
        assert_eq!(s.spread_radius, 0.0);
    }

    #[test]
    fn test_box_shadow_clone_eq() {
        let s1 = BoxShadow::new(Color::RED).blur(8.0);
        let s2 = s1.clone();
        assert_eq!(s1, s2);
        let s3 = BoxShadow::new(Color::RED).blur(10.0);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_style_shadow_default_empty() {
        let style = Style::default();
        assert!(style.shadows.is_empty());
    }

    #[test]
    fn test_style_shadow_appends() {
        let s1 = BoxShadow::new(Color::RED);
        let s2 = BoxShadow::new(Color::BLACK);
        let style = Style::new().shadow(s1.clone()).shadow(s2.clone());
        assert_eq!(style.shadows.len(), 2);
        assert_eq!(style.shadows[0], s1);
        assert_eq!(style.shadows[1], s2);
    }

    #[test]
    fn test_style_shadows_replaces() {
        let s1 = BoxShadow::new(Color::RED);
        let s2 = BoxShadow::new(Color::BLACK);
        let style = Style::new().shadow(s1);
        let style = style.shadows(vec![s2.clone()]);
        assert_eq!(style.shadows.len(), 1);
        assert_eq!(style.shadows[0], s2);
    }

    #[test]
    fn test_style_with_shadows_clone() {
        let style = Style::new().shadow(BoxShadow::new(Color::RED).blur(8.0));
        let cloned = style.clone();
        assert_eq!(style, cloned);
        assert_eq!(cloned.shadows.len(), 1);
    }

    #[test]
    fn test_style_with_shadows_eq() {
        let s1 = Style::new().shadow(BoxShadow::new(Color::RED));
        let s2 = Style::new().shadow(BoxShadow::new(Color::RED));
        let s3 = Style::new().shadow(BoxShadow::new(Color::BLACK));
        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_style_shadow_does_not_overwrite_background() {
        let style = Style::new()
            .background(Color::RED)
            .shadow(BoxShadow::new(Color::BLACK));
        assert_eq!(style.background, Some(Color::RED));
        assert_eq!(style.shadows.len(), 1);
    }
}
