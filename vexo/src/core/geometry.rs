//! Geometry types for the Vexo UI framework.
//!
//! This module provides type-safe 2D geometry types that distinguish between
//! logical (DPI-independent) and physical (screen pixel) coordinates.
//!
//! # Coordinate System
//!
//! - **Logical coordinates** are DPI-independent and used for layout
//! - **Physical coordinates** are actual screen pixels
//! - Conversion between them requires a scale factor (DPI)

use std::marker::PhantomData;

// ============================================================================
// MARKER TYPES
// ============================================================================

/// Marker type for logical (DPI-independent) coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Logical;

/// Marker type for physical (screen pixel) coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Physical;

// ============================================================================
// POINT
// ============================================================================

/// A 2D point in either logical or physical coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T> {
    pub x: f32,
    pub y: f32,
    _marker: PhantomData<T>,
}

impl<T> Point<T> {
    /// Create a new point.
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            _marker: PhantomData,
        }
    }

    /// Convert to array for GPU buffers.
    pub fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }

    /// Get the zero point.
    pub const fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            _marker: PhantomData,
        }
    }
}

impl Point<Logical> {
    /// Convert logical point to physical pixels.
    pub fn to_physical(self, scale: Scale) -> Point<Physical> {
        Point::new(self.x * scale.factor(), self.y * scale.factor())
    }

    /// Convert from Taffy's Point type.
    pub fn from_taffy(p: taffy::Point<f32>) -> Self {
        Point::new(p.x, p.y)
    }

    /// Convert to Taffy's Point type.
    pub fn to_taffy(self) -> taffy::Point<f32> {
        taffy::Point { x: self.x, y: self.y }
    }
}

impl Point<Physical> {
    /// Convert physical point to logical coordinates.
    pub fn to_logical(self, scale: Scale) -> Point<Logical> {
        Point::new(self.x / scale.factor(), self.y / scale.factor())
    }
}

// ============================================================================
// SIZE
// ============================================================================

/// A 2D size in either logical or physical coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size<T> {
    pub width: f32,
    pub height: f32,
    _marker: PhantomData<T>,
}

impl<T> Size<T> {
    /// Create a new size.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            _marker: PhantomData,
        }
    }

    /// Convert to array for GPU buffers.
    pub fn to_array(self) -> [f32; 2] {
        [self.width, self.height]
    }

    /// Get zero size.
    pub const fn zero() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            _marker: PhantomData,
        }
    }
}

impl Size<Logical> {
    /// Convert logical size to physical pixels.
    pub fn to_physical(self, scale: Scale) -> Size<Physical> {
        Size::new(self.width * scale.factor(), self.height * scale.factor())
    }

    /// Convert from Taffy's Size type.
    pub fn from_taffy(s: taffy::Size<f32>) -> Self {
        Size::new(s.width, s.height)
    }

    /// Convert to Taffy's Size type.
    pub fn to_taffy(self) -> taffy::Size<f32> {
        taffy::Size { width: self.width, height: self.height }
    }
}

impl Size<Physical> {
    /// Convert physical size to logical coordinates.
    pub fn to_logical(self, scale: Scale) -> Size<Logical> {
        Size::new(self.width / scale.factor(), self.height / scale.factor())
    }

    /// Get width as u32 for GPU APIs.
    pub fn width_u32(&self) -> u32 {
        self.width as u32
    }

    /// Get height as u32 for GPU APIs.
    pub fn height_u32(&self) -> u32 {
        self.height as u32
    }
}

// ============================================================================
// RECT
// ============================================================================

/// A rectangle with origin and size in either logical or physical coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect<T> {
    pub origin: Point<T>,
    pub size: Size<T>,
    _marker: PhantomData<T>,
}

impl<T> Rect<T> {
    /// Create a new rectangle from origin and size.
    pub fn new(origin: Point<T>, size: Size<T>) -> Self {
        Self {
            origin,
            size,
            _marker: PhantomData,
        }
    }

    /// Create a rectangle from x, y, width, height.
    pub fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::new(Point::new(x, y), Size::new(width, height))
    }
}

impl Rect<Logical> {
    /// Convert logical rect to physical pixels.
    pub fn to_physical(self, scale: Scale) -> Rect<Physical> {
        Rect::new(
            self.origin.to_physical(scale),
            self.size.to_physical(scale),
        )
    }

    /// Create from layout result.
    pub fn from_layout(location: taffy::Point<f32>, size: taffy::Size<f32>) -> Self {
        Rect::new(Point::from_taffy(location), Size::from_taffy(size))
    }

    /// Create from position and size.
    pub fn from_pos_size(pos: Point<Logical>, size: Size<Logical>) -> Self {
        Rect::new(pos, size)
    }

    /// Check if a logical point is inside this rectangle.
    pub fn contains(&self, point: &Point<Logical>) -> bool {
        point.x >= self.origin.x
            && point.x <= self.origin.x + self.size.width
            && point.y >= self.origin.y
            && point.y <= self.origin.y + self.size.height
    }

    /// Get the right edge x coordinate.
    pub fn right(&self) -> f32 {
        self.origin.x + self.size.width
    }

    /// Get the bottom edge y coordinate.
    pub fn bottom(&self) -> f32 {
        self.origin.y + self.size.height
    }
}

impl Rect<Physical> {
    /// Convert physical rect to logical coordinates.
    pub fn to_logical(self, scale: Scale) -> Rect<Logical> {
        Rect::new(
            self.origin.to_logical(scale),
            self.size.to_logical(scale),
        )
    }
}

// ============================================================================
// SCALE
// ============================================================================

/// DPI scale factor for converting between logical and physical coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale(f64);

impl Scale {
    /// Create a new scale factor.
    pub fn new(factor: f64) -> Self {
        Self(factor)
    }

    /// Get the scale factor as f32.
    pub fn factor(&self) -> f32 {
        self.0 as f32
    }

    /// Get the scale factor as f64.
    pub fn factor_f64(&self) -> f64 {
        self.0
    }

    /// Check if this is a HiDPI/Retina scale (>= 2.0).
    pub fn is_hidpi(&self) -> bool {
        self.0 >= 2.0
    }
}

impl Default for Scale {
    fn default() -> Self {
        Self(1.0)
    }
}

impl From<f64> for Scale {
    fn from(factor: f64) -> Self {
        Self(factor)
    }
}

impl From<f32> for Scale {
    fn from(factor: f32) -> Self {
        Self(factor as f64)
    }
}

impl PartialEq<f64> for Scale {
    fn eq(&self, other: &f64) -> bool {
        self.0 == *other
    }
}

impl PartialEq<f32> for Scale {
    fn eq(&self, other: &f32) -> bool {
        self.0 == *other as f64
    }
}

impl std::ops::Mul<f64> for Scale {
    type Output = f64;
    fn mul(self, rhs: f64) -> f64 {
        self.0 * rhs
    }
}

impl std::ops::Mul<f32> for Scale {
    type Output = f32;
    fn mul(self, rhs: f32) -> f32 {
        (self.0 * rhs as f64) as f32
    }
}

impl std::ops::Div<f64> for Scale {
    type Output = f64;
    fn div(self, rhs: f64) -> f64 {
        self.0 / rhs
    }
}

impl std::ops::Div<f32> for Scale {
    type Output = f32;
    fn div(self, rhs: f32) -> f32 {
        (self.0 / rhs as f64) as f32
    }
}

// ============================================================================
// ARITHMETIC OPERATORS
// ============================================================================

impl<T> std::ops::Add for Point<T> {
    type Output = Point<T>;

    fn add(self, other: Point<T>) -> Point<T> {
        Point::new(self.x + other.x, self.y + other.y)
    }
}

impl<T> std::ops::AddAssign for Point<T> {
    fn add_assign(&mut self, other: Point<T>) {
        self.x += other.x;
        self.y += other.y;
    }
}

impl<T> std::ops::Sub for Point<T> {
    type Output = Point<T>;

    fn sub(self, other: Point<T>) -> Point<T> {
        Point::new(self.x - other.x, self.y - other.y)
    }
}

impl<T> std::ops::SubAssign for Point<T> {
    fn sub_assign(&mut self, other: Point<T>) {
        self.x -= other.x;
        self.y -= other.y;
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_logical_to_physical() {
        let logical = Point::<Logical>::new(100.0, 200.0);
        let physical = logical.to_physical(Scale::new(2.0));
        assert_eq!(physical.x, 200.0);
        assert_eq!(physical.y, 400.0);
    }

    #[test]
    fn test_point_physical_to_logical() {
        let physical = Point::<Physical>::new(200.0, 400.0);
        let logical = physical.to_logical(Scale::new(2.0));
        assert_eq!(logical.x, 100.0);
        assert_eq!(logical.y, 200.0);
    }

    #[test]
    fn test_rect_contains() {
        let rect = Rect::<Logical>::from_xywh(10.0, 10.0, 100.0, 100.0);
        assert!(rect.contains(&Point::new(50.0, 50.0)));
        assert!(!rect.contains(&Point::new(5.0, 5.0)));
        assert!(rect.contains(&Point::new(10.0, 10.0))); // Edge inclusive
        assert!(rect.contains(&Point::new(110.0, 110.0))); // Edge inclusive
    }

    #[test]
    fn test_point_add() {
        let p1 = Point::<Logical>::new(10.0, 20.0);
        let p2 = Point::<Logical>::new(5.0, 10.0);
        let sum = p1 + p2;
        assert_eq!(sum.x, 15.0);
        assert_eq!(sum.y, 30.0);
    }

    #[test]
    fn test_scale_factor() {
        let scale = Scale::new(2.0);
        assert_eq!(scale.factor(), 2.0);
        assert_eq!(scale.factor_f64(), 2.0);
    }

    #[test]
    fn test_scale_from() {
        let scale: Scale = 2.0_f64.into();
        assert_eq!(scale.factor_f64(), 2.0);

        let scale: Scale = 1.5_f32.into();
        assert_eq!(scale.factor_f64(), 1.5);
    }

    #[test]
    fn test_scale_partial_eq() {
        let scale = Scale::new(2.0);
        assert!(scale == 2.0_f64);
        assert!(scale == 2.0_f32);
    }

    #[test]
    fn test_scale_mul() {
        let scale = Scale::new(2.0);
        assert_eq!(scale * 10.0_f64, 20.0);
        assert_eq!(scale * 10.0_f32, 20.0);
    }

    #[test]
    fn test_scale_div() {
        let scale = Scale::new(2.0);
        assert_eq!(scale / 4.0_f64, 0.5);
        assert_eq!(scale / 4.0_f32, 0.5);
    }

    #[test]
    fn test_scale_is_hidpi() {
        assert!(!Scale::new(1.0).is_hidpi());
        assert!(!Scale::new(1.5).is_hidpi());
        assert!(Scale::new(2.0).is_hidpi());
        assert!(Scale::new(3.0).is_hidpi());
    }
}
