//! Stroke (border) configuration for shapes.

use crate::core::Color;

/// Stroke (border) configuration for shapes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    /// The stroke color.
    pub color: Color,
    /// The stroke width in logical points.
    pub width: f32,
}

impl Stroke {
    /// Create a new stroke.
    pub fn new(color: Color, width: f32) -> Self {
        Self { color, width }
    }

    /// Create a stroke with default width (1.0).
    pub fn with_color(color: Color) -> Self {
        Self { color, width: 1.0 }
    }

    /// Create a stroke with default color (black).
    pub fn with_width(width: f32) -> Self {
        Self {
            color: Color::BLACK,
            width,
        }
    }
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            width: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stroke_default() {
        let s = Stroke::default();
        assert_eq!(s.color, Color::BLACK);
        assert_eq!(s.width, 1.0);
    }

    #[test]
    fn test_stroke_new() {
        let s = Stroke::new(Color::RED, 2.0);
        assert_eq!(s.color, Color::RED);
        assert_eq!(s.width, 2.0);
    }

    #[test]
    fn test_stroke_with_color() {
        let s = Stroke::with_color(Color::BLUE);
        assert_eq!(s.color, Color::BLUE);
        assert_eq!(s.width, 1.0);
    }

    #[test]
    fn test_stroke_with_width() {
        let s = Stroke::with_width(3.0);
        assert_eq!(s.color, Color::BLACK);
        assert_eq!(s.width, 3.0);
    }
}
