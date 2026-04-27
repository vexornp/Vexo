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

    /// Convert from winit's PhysicalSize<u32>.
    pub fn from_winit(size: winit::dpi::PhysicalSize<u32>) -> Self {
        Size::new(size.width as f32, size.height as f32)
    }
}

// ============================================================================
// BOUNDS
// ============================================================================

/// A bounding box with edge coordinates in either logical or physical coordinates.
///
/// Bounds uses left/top/right/bottom edges, which is a natural representation
/// for clipping, intersection testing, and integration with glyphon's TextBounds.
///
/// # Example
///
/// ```
/// use vexo::core::{Bounds, Logical, Point};
///
/// // Create from position and size
/// let bounds = Bounds::<Logical>::from_xywh(10.0, 20.0, 100.0, 50.0);
///
/// // Or from edges directly
/// let bounds = Bounds::<Logical>::new(10.0, 20.0, 110.0, 70.0);
///
/// // Check containment
/// assert!(bounds.contains(&Point::new(50.0, 40.0)));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds<T> {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    _marker: PhantomData<T>,
}

impl<T> Bounds<T> {
    /// Create bounds from edge coordinates (left, top, right, bottom).
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
            _marker: PhantomData,
        }
    }

    /// Create bounds from x, y, width, height.
    pub fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::new(x, y, x + width, y + height)
    }

    /// Create bounds from position and size.
    pub fn from_pos_size(position: crate::core::Point<T>, size: crate::core::Size<T>) -> Self {
        Self::from_xywh(position.x, position.y, size.width, size.height)
    }

    /// Get the width (right - left).
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    /// Get the height (bottom - top).
    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// Get the position (top-left corner).
    pub fn position(&self) -> crate::core::Point<T> {
        crate::core::Point::new(self.left, self.top)
    }

    /// Get the size.
    pub fn size(&self) -> crate::core::Size<T> {
        crate::core::Size::new(self.width(), self.height())
    }

    /// Convert to [x, y, width, height] array for GPU buffers.
    pub fn to_array_xywh(&self) -> [f32; 4] {
        [self.left, self.top, self.width(), self.height()]
    }

    /// Check if a point is inside these bounds.
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.left && x <= self.right && y >= self.top && y <= self.bottom
    }

    /// Check if these bounds are valid (left <= right and top <= bottom).
    pub fn is_valid(&self) -> bool {
        self.left <= self.right && self.top <= self.bottom
    }
}

impl Bounds<Logical> {
    /// Convert logical bounds to physical pixels.
    pub fn to_physical(&self, scale: Scale) -> Bounds<Physical> {
        let f = scale.factor();
        Bounds::new(self.left * f, self.top * f, self.right * f, self.bottom * f)
    }

    /// Create from Taffy layout result.
    pub fn from_taffy(location: taffy::Point<f32>, size: taffy::Size<f32>) -> Self {
        Bounds::from_xywh(location.x, location.y, size.width, size.height)
    }

    /// Check if a logical point is inside these bounds.
    pub fn contains(&self, point: &Point<Logical>) -> bool {
        self.contains_point(point.x, point.y)
    }
}

impl Bounds<Physical> {
    /// Convert physical bounds to logical coordinates.
    pub fn to_logical(&self, scale: Scale) -> Bounds<Logical> {
        let f = scale.factor();
        Bounds::new(self.left / f, self.top / f, self.right / f, self.bottom / f)
    }

    /// Convert to glyphon::TextBounds with proper edge rounding.
    /// Uses floor for left/top (inclusive) and ceil for right/bottom (exclusive).
    pub fn to_glyphon_bounds(&self) -> glyphon::TextBounds {
        glyphon::TextBounds {
            left: self.left.floor() as i32,
            top: self.top.floor() as i32,
            right: self.right.ceil() as i32,
            bottom: self.bottom.ceil() as i32,
        }
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
    fn test_bounds_contains() {
        let bounds = Bounds::<Logical>::from_xywh(10.0, 10.0, 100.0, 100.0);
        assert!(bounds.contains(&Point::new(50.0, 50.0)));
        assert!(!bounds.contains(&Point::new(5.0, 5.0)));
        assert!(bounds.contains(&Point::new(10.0, 10.0))); // Edge inclusive
        assert!(bounds.contains(&Point::new(110.0, 110.0))); // Edge inclusive
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
