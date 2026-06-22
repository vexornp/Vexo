//! Color representation for the Vexo UI framework.
//!
//! This module provides a unified color representation using RGBA f32 values
//! (0.0-1.0) with conversions to/from various formats.

use glyphon::cosmic_text;

/// A unified color representation using RGBA f32 values (0.0-1.0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Create a new color with RGBA components (0.0-1.0).
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Create an opaque color with RGB components (alpha = 1.0).
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Create a color from a hex value.
    /// - 0xRRGGBB creates an opaque color
    /// - 0xRRGGBBAA creates a color with alpha
    pub fn from_hex(hex: u32) -> Self {
        let r = ((hex >> 24) & 0xFF) as f32 / 255.0;
        let g = ((hex >> 16) & 0xFF) as f32 / 255.0;
        let b = ((hex >> 8) & 0xFF) as f32 / 255.0;
        let a = (hex & 0xFF) as f32 / 255.0;
        Self { r, g, b, a }
    }

    /// Convert to [f32; 4] array.
    pub const fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Linearly interpolate between two colors.
    ///
    /// `t` ranges from 0.0 (returns `a`) to 1.0 (returns `b`).
    pub fn lerp(a: Color, b: Color, t: f64) -> Color {
        Color {
            r: a.r + (b.r - a.r) * t as f32,
            g: a.g + (b.g - a.g) * t as f32,
            b: a.b + (b.b - a.b) * t as f32,
            a: a.a + (b.a - a.a) * t as f32,
        }
    }

    /// Create a new color with a different alpha value.
    pub const fn with_alpha(&self, a: f32) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a,
        }
    }

    /// Convert to wgpu::Color in a const context.
    pub const fn to_wgpu_color(&self) -> wgpu::Color {
        wgpu::Color {
            r: self.r as f64,
            g: self.g as f64,
            b: self.b as f64,
            a: self.a as f64,
        }
    }
}

// Preset colors
impl Color {
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const TRANSPARENT: Color = Color::new(0.0, 0.0, 0.0, 0.0);
    pub const RED: Color = Color::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
    pub const YELLOW: Color = Color::rgb(1.0, 1.0, 0.0);
    pub const CYAN: Color = Color::rgb(0.0, 1.0, 1.0);
    pub const MAGENTA: Color = Color::rgb(1.0, 0.0, 1.0);
    pub const GRAY: Color = Color::rgb(0.5, 0.5, 0.5);
}

// Conversion from [f32; 3] (RGB, alpha defaults to 1.0)
impl From<[f32; 3]> for Color {
    fn from(rgb: [f32; 3]) -> Self {
        Self::rgb(rgb[0], rgb[1], rgb[2])
    }
}

// Conversion from [f32; 4] (RGBA)
impl From<[f32; 4]> for Color {
    fn from(rgba: [f32; 4]) -> Self {
        Self::new(rgba[0], rgba[1], rgba[2], rgba[3])
    }
}

// Conversion to [f32; 4]
impl From<Color> for [f32; 4] {
    fn from(color: Color) -> Self {
        color.to_array()
    }
}

// Conversion from wgpu::Color (f64)
impl From<wgpu::Color> for Color {
    fn from(color: wgpu::Color) -> Self {
        Self::new(color.r as f32, color.g as f32, color.b as f32, color.a as f32)
    }
}

// Conversion to wgpu::Color
impl From<Color> for wgpu::Color {
    fn from(color: Color) -> Self {
        Self {
            r: color.r as f64,
            g: color.g as f64,
            b: color.b as f64,
            a: color.a as f64,
        }
    }
}

// Conversion from cosmic_text::Color (u8 RGBA)
impl From<cosmic_text::Color> for Color {
    fn from(color: cosmic_text::Color) -> Self {
        let rgba = color.as_rgba();
        Self::new(
            rgba[0] as f32 / 255.0,
            rgba[1] as f32 / 255.0,
            rgba[2] as f32 / 255.0,
            rgba[3] as f32 / 255.0,
        )
    }
}

// Conversion to cosmic_text::Color
impl From<Color> for cosmic_text::Color {
    fn from(color: Color) -> Self {
        cosmic_text::Color::rgba(
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            (color.a * 255.0) as u8,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_rgb() {
        let color = Color::rgb(1.0, 0.5, 0.0);
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 0.5);
        assert_eq!(color.b, 0.0);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn test_color_with_alpha() {
        let color = Color::rgb(1.0, 0.5, 0.0).with_alpha(0.5);
        assert_eq!(color.a, 0.5);
    }

    #[test]
    fn test_color_to_array() {
        let color = Color::new(0.1, 0.2, 0.3, 0.4);
        let arr = color.to_array();
        assert_eq!(arr, [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn test_color_from_hex() {
        let color = Color::from_hex(0xFF8000FF);
        assert!((color.r - 1.0).abs() < 0.01);
        assert!((color.g - 0.5).abs() < 0.01);
        assert!((color.b - 0.0).abs() < 0.01);
        assert!((color.a - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_color_presets() {
        assert_eq!(Color::WHITE, Color::rgb(1.0, 1.0, 1.0));
        assert_eq!(Color::BLACK, Color::rgb(0.0, 0.0, 0.0));
        assert_eq!(Color::TRANSPARENT.a, 0.0);
    }

    #[test]
    fn test_color_lerp_midpoint() {
        let a = Color::rgb(1.0, 0.0, 0.0);
        let b = Color::rgb(0.0, 0.0, 1.0);
        let mid = Color::lerp(a, b, 0.5);
        assert!((mid.r - 0.5).abs() < 0.001);
        assert!((mid.g).abs() < 0.001);
        assert!((mid.b - 0.5).abs() < 0.001);
        assert!((mid.a - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_color_lerp_boundaries() {
        let a = Color::rgb(1.0, 0.0, 0.0);
        let b = Color::rgb(0.0, 0.0, 1.0);
        assert_eq!(Color::lerp(a, b, 0.0), a);
        assert_eq!(Color::lerp(a, b, 1.0), b);
    }

    #[test]
    fn test_color_lerp_with_alpha() {
        let a = Color::new(1.0, 0.0, 0.0, 1.0);
        let b = Color::new(0.0, 0.0, 1.0, 0.0);
        let mid = Color::lerp(a, b, 0.5);
        assert!((mid.r - 0.5).abs() < 0.001);
        assert!((mid.a - 0.5).abs() < 0.001);
    }
}
