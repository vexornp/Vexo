//! Geometry types for the Vexo UI framework.
//!
//! This module provides type-safe 2D geometry types that distinguish between
//! different coordinate systems:
//!
//! # Coordinate Spaces
//!
//! - **Logical vs Physical**: DPI-independent vs screen pixels
//! - **Absolute vs Relative**: Window coordinates vs parent-relative coordinates
//!
//! # Type Safety
//!
//! The type system prevents mixing coordinates from different spaces:
//!
//! ```ignore
//! use vexo::core::{Position, Logical, Absolute, Relative};
//!
//! // This would be a compile error:
//! let absolute: Position<Logical, Absolute> = Position::new(10.0, 20.0);
//! let relative: Position<Logical, Relative> = absolute; // Error: type mismatch!
//! ```
//!
//! # Conversion
//!
//! Convert between coordinate spaces explicitly:
//!
//! ```ignore
//! use vexo::core::{Position, Logical, Absolute, Relative};
//!
//! let relative = Position::<Logical, Relative>::new(10.0, 20.0);
//! let parent_absolute = Position::<Logical, Absolute>::new(100.0, 50.0);
//! let absolute = relative.to_absolute(parent_absolute);
//! // absolute = (110.0, 70.0)
//! ```

use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

// ============================================================================
// MARKER TYPES: Logical vs Physical
// ============================================================================

/// Marker type for logical (DPI-independent) coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Logical;

/// Marker type for physical (screen pixel) coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Physical;

// ============================================================================
// MARKER TYPES: Absolute vs Relative
// ============================================================================

/// Marker type for absolute coordinates (relative to window origin).
///
/// Absolute coordinates specify a position in the window's coordinate system,
/// where (0, 0) is the top-left corner of the window.
///
/// Use `Point<Absolute>` when:
/// - Painting to the screen
/// - Hit testing with window mouse positions
/// - Storing final render positions
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Absolute;

/// Marker type for relative coordinates (relative to parent container).
///
/// Relative coordinates specify a position within a parent container,
/// where (0, 0) is the top-left corner of the parent.
///
/// Use `Point<Relative>` when:
/// - Storing layout positions from Taffy
/// - Describing child positions within containers
/// - Intermediate layout calculations
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Relative;

// ============================================================================
// POINT
// ============================================================================

/// A 2D point in either logical or physical coordinates.
///
/// For positions that also need to track absolute vs relative coordinate space,
/// use `Position` instead.
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
        taffy::Point {
            x: self.x,
            y: self.y,
        }
    }
}

impl Point<Physical> {
    /// Convert physical point to logical coordinates.
    pub fn to_logical(self, scale: Scale) -> Point<Logical> {
        Point::new(self.x / scale.factor(), self.y / scale.factor())
    }
}

// ============================================================================
// POSITION (combines coordinate space + reference frame)
// ============================================================================

/// A 2D position with explicit coordinate space and reference frame.
///
/// This type combines two orthogonal concepts:
/// - **Coordinate space**: `Logical` (DPI-independent) or `Physical` (screen pixels)
/// - **Reference frame**: `Absolute` (window origin) or `Relative` (parent origin)
///
/// # Type Parameters
///
/// - `C`: Coordinate space (`Logical` or `Physical`)
/// - `R`: Reference frame (`Absolute` or `Relative`)
///
/// # Examples
///
/// ```
/// use vexo::core::{Position, Logical, Absolute, Relative};
///
/// // A position relative to parent (e.g., from layout)
/// let relative_pos = Position::<Logical, Relative>::new(10.0, 20.0);
///
/// // Convert to absolute by providing parent's absolute position
/// let parent_absolute = Position::<Logical, Absolute>::new(100.0, 50.0);
/// let absolute_pos = relative_pos.to_absolute(parent_absolute);
/// // absolute_pos = (110.0, 70.0)
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Position<C, R> {
    pub x: f32,
    pub y: f32,
    _marker: PhantomData<(C, R)>,
}

impl<C, R> Position<C, R> {
    /// Create a new position.
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

    /// Get the zero position.
    pub const fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            _marker: PhantomData,
        }
    }

    /// Get the underlying point (discards reference frame information).
    pub fn to_point(self) -> Point<C> {
        Point::new(self.x, self.y)
    }
}

// Conversions between reference frames
impl<C> Position<C, Relative> {
    /// Convert a relative position to absolute by adding parent's absolute position.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use vexo::core::{Position, Logical, Absolute, Relative};
    ///
    /// let child_relative = Position::<Logical, Relative>::new(10.0, 20.0);
    /// let parent_absolute = Position::<Logical, Absolute>::new(100.0, 50.0);
    /// let child_absolute = child_relative.to_absolute(parent_absolute);
    /// // child_absolute = (110.0, 70.0)
    /// ```
    pub fn to_absolute(self, parent_absolute: Position<C, Absolute>) -> Position<C, Absolute> {
        Position::new(self.x + parent_absolute.x, self.y + parent_absolute.y)
    }
}

impl<C> Position<C, Absolute> {
    /// Convert an absolute position to relative by subtracting parent's absolute position.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use vexo::core::{Position, Logical, Absolute, Relative};
    ///
    /// let child_absolute = Position::<Logical, Absolute>::new(110.0, 70.0);
    /// let parent_absolute = Position::<Logical, Absolute>::new(100.0, 50.0);
    /// let child_relative = child_absolute.to_relative(parent_absolute);
    /// // child_relative = (10.0, 20.0)
    /// ```
    pub fn to_relative(self, parent_absolute: Position<C, Absolute>) -> Position<C, Relative> {
        Position::new(self.x - parent_absolute.x, self.y - parent_absolute.y)
    }
}

// Conversions between coordinate spaces
impl Position<Logical, Absolute> {
    /// Convert logical absolute position to physical pixels.
    pub fn to_physical(self, scale: Scale) -> Position<Physical, Absolute> {
        Position::new(self.x * scale.factor(), self.y * scale.factor())
    }
}

impl Position<Logical, Relative> {
    /// Convert logical relative position to physical pixels.
    pub fn to_physical(self, scale: Scale) -> Position<Physical, Relative> {
        Position::new(self.x * scale.factor(), self.y * scale.factor())
    }
}

impl Position<Physical, Absolute> {
    /// Convert physical absolute position to logical coordinates.
    pub fn to_logical(self, scale: Scale) -> Position<Logical, Absolute> {
        Position::new(self.x / scale.factor(), self.y / scale.factor())
    }
}

impl Position<Physical, Relative> {
    /// Convert physical relative position to logical coordinates.
    pub fn to_logical(self, scale: Scale) -> Position<Logical, Relative> {
        Position::new(self.x / scale.factor(), self.y / scale.factor())
    }
}

// Convenience type aliases
impl Position<Logical, Absolute> {
    /// Create from Taffy layout location (which is always relative to parent).
    pub fn from_taffy_relative(location: taffy::Point<f32>) -> Position<Logical, Relative> {
        Position::new(location.x, location.y)
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
    pub const fn new(width: f32, height: f32) -> Self {
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
        taffy::Size {
            width: self.width,
            height: self.height,
        }
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
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
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

    /// Intersect this bounds with another. Returns the overlapping region,
    /// or None if they do not overlap.
    pub fn intersect(&self, other: &Bounds<T>) -> Option<Bounds<T>> {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        let right = self.right.min(other.right);
        let bottom = self.bottom.min(other.bottom);
        if left < right && top < bottom {
            Some(Bounds::new(left, top, right, bottom))
        } else {
            None
        }
    }
}

impl Bounds<Logical> {
    /// A zero-area bounds at the origin. Used as a sentinel for "fully
    /// clipped" — the GPU backend's `w == 0 || h == 0` check skips ops
    /// with this as their scissor rect.
    pub const ZERO: Self = Bounds::new(0.0, 0.0, 0.0, 0.0);

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
// SCALE SOURCE
// ============================================================================

/// Shared handle to the DPI scale factor.
///
/// Wraps `Arc<AtomicU64>` so all consumers read from the same memory.
/// One `set()` call updates the value for every holder of a clone.
pub struct ScaleSource {
    inner: Arc<AtomicU64>,
}

impl ScaleSource {
    /// Create a new scale source with the given initial value.
    pub fn new(initial: f64) -> Self {
        Self {
            inner: Arc::new(AtomicU64::new(initial.to_bits())),
        }
    }

    /// Read the current scale factor.
    pub fn get(&self) -> Scale {
        let bits = self.inner.load(Ordering::Relaxed);
        Scale::new(f64::from_bits(bits))
    }

    /// Update the scale factor. Visible to all holders immediately.
    pub fn set(&self, value: f64) {
        self.inner.store(value.to_bits(), Ordering::Relaxed);
    }
}

impl Clone for ScaleSource {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Default for ScaleSource {
    fn default() -> Self {
        Self::new(1.0)
    }
}

// ============================================================================
// SAFE AREA SOURCE
// ============================================================================

/// Shared handle to the device safe-area insets (logical pixels).
///
/// Wraps four `AtomicU64`-backed floats (one per edge) behind a single `Arc`
/// so all consumers read from the same memory. One `set()` call updates the
/// value for every holder of a clone — mirroring [`ScaleSource`] but for the
/// four-edge safe area (status bar / notch / home indicator on mobile).
///
/// On desktop the underlying insets are always zero, so this is a no-op.
#[derive(Clone)]
pub struct SafeAreaSource {
    inner: Arc<SafeAreaInner>,
}

struct SafeAreaInner {
    left: AtomicU32,
    right: AtomicU32,
    top: AtomicU32,
    bottom: AtomicU32,
}

impl SafeAreaSource {
    /// Create a new source with the given logical insets.
    pub fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            inner: Arc::new(SafeAreaInner {
                left: AtomicU32::new(left.to_bits()),
                right: AtomicU32::new(right.to_bits()),
                top: AtomicU32::new(top.to_bits()),
                bottom: AtomicU32::new(bottom.to_bits()),
            }),
        }
    }

    /// Read the current insets as an [`EdgeInsets`] (logical pixels).
    ///
    /// Field order matches `crate::layout::EdgeInsets` (`left, right, top, bottom`).
    pub fn get(&self) -> crate::layout::EdgeInsets {
        crate::layout::EdgeInsets {
            left: f32::from_bits(self.inner.left.load(Ordering::Relaxed)),
            right: f32::from_bits(self.inner.right.load(Ordering::Relaxed)),
            top: f32::from_bits(self.inner.top.load(Ordering::Relaxed)),
            bottom: f32::from_bits(self.inner.bottom.load(Ordering::Relaxed)),
        }
    }

    /// Update the insets. Visible to all holders immediately.
    pub fn set(&self, left: f32, right: f32, top: f32, bottom: f32) {
        self.inner.left.store(left.to_bits(), Ordering::Relaxed);
        self.inner.right.store(right.to_bits(), Ordering::Relaxed);
        self.inner.top.store(top.to_bits(), Ordering::Relaxed);
        self.inner.bottom.store(bottom.to_bits(), Ordering::Relaxed);
    }
}

impl Default for SafeAreaSource {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

// ============================================================================
// KEYBOARD INSET SOURCE
// ============================================================================

/// Shared atomic cell holding the current keyboard height (logical px).
/// Updated each frame by the render loop's interpolation driver (which reads
/// animation params from [`KeyboardAnimationSource`]); stays 0 on desktop /
/// Android (no shim installed).
#[derive(Clone)]
pub struct KeyboardInsetSource {
    inner: Arc<KeyboardInsetInner>,
}

struct KeyboardInsetInner {
    current_height: AtomicU32,
}

impl KeyboardInsetSource {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(KeyboardInsetInner {
                current_height: AtomicU32::new(0.0_f32.to_bits()),
            }),
        }
    }

    pub fn get(&self) -> f32 {
        f32::from_bits(self.inner.current_height.load(Ordering::Relaxed))
    }

    pub fn set(&self, current_height: f32) {
        self.inner
            .current_height
            .store(current_height.to_bits(), Ordering::Relaxed);
    }
}

impl Default for KeyboardInsetSource {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// KEYBOARD ANIMATION SOURCE
// ============================================================================

/// Animation parameters for the current keyboard show/hide transition.
/// Written by the iOS shim on `keyboardWillShow/Hide`; read + interpolated
/// by `WindowState::render_retain()` each frame to drive
/// `KeyboardInsetSource::set()`.
#[derive(Clone, Debug)]
pub struct KeyboardAnimation {
    /// Height the animation starts from (the source's current height at
    /// notification time).
    pub from: f32,
    /// Height the animation is tweening toward (`target_height` for show,
    /// `0.0` for hide).
    pub target: f32,
    /// Animation duration in seconds. `0.0` means "snap immediately."
    pub duration_secs: f32,
    /// The instant the OS keyboard animation began (captured in the iOS
    /// shim at the moment the notification fired).
    pub start: std::time::Instant,
    /// The animation curve, mapped from UIKit's raw curve value. Standard
    /// `UIViewAnimationCurve` values are 0=EaseInOut, 1=EaseIn, 2=EaseOut,
    /// 3=Linear. iOS keyboard notifications report the private value `7`
    /// (`UIKeyboardAnimationCurveKeyboard` = `0x4 | EaseInOut`), which Apple
    /// renders as a smooth easeInOut-style curve — NOT linear. Treating the
    /// bottom 2 bits (`7 & 0x3 = 3 → Linear`) is wrong: linear motion keeps
    /// moving at constant velocity near the end (95% at t=0.95) while the
    /// keyboard has nearly settled (~99.5% at t=0.95), so the input bar
    /// appears to finish after the keyboard. We special-case raw=7 to
    /// `EaseInOutCurve` to match the keyboard's actual deceleration.
    /// Stored as a raw `u8` so the cell is `Send + Sync`; the render loop
    /// maps it to a `Box<dyn Curve>` when interpolating.
    pub curve_raw: u8,
}

impl KeyboardAnimation {
    /// Map a UIKit `UIViewAnimationCurve` raw value (or the keyboard's
    /// private curve raw value) to a curve. The standard documented values
    /// are 0=EaseInOut, 1=EaseIn, 2=EaseOut, 3=Linear. iOS keyboard
    /// notifications report the private value `7`, which Apple renders as a
    /// smooth easeInOut-style curve (the high bit `0x4` is a "keyboard"
    /// flag, not a curve-type selector). Map raw=7 directly to
    /// `EaseInOutCurve` to match the keyboard's perceived deceleration.
    pub fn curve(&self) -> Box<dyn crate::animation::Curve> {
        use crate::animation::{EaseInCurve, EaseInOutCurve, EaseOutCurve, LinearCurve};
        // iOS keyboard's private curve raw=7 = 0x4 | EaseInOut. Apple renders
        // it as a smooth easeInOut, NOT linear. The bottom-2-bits trick
        // (`7 & 0x3 = 3 → Linear`) is wrong and causes the input bar to
        // visibly lag the keyboard near the end of the animation.
        if self.curve_raw == 7 {
            return Box::new(EaseInOutCurve);
        }
        match self.curve_raw & 0x3 {
            0 => Box::new(EaseInOutCurve),
            1 => Box::new(EaseInCurve),
            2 => Box::new(EaseOutCurve),
            _ => Box::new(LinearCurve),
        }
    }

    /// Compute the interpolated height at time `now`.
    /// Returns `None` if the animation has completed (elapsed >= duration),
    /// so the caller can set the final value and mark the animation inactive.
    pub fn interpolate(&self, now: std::time::Instant) -> Option<f32> {
        if self.duration_secs <= 0.0 {
            return None;
        }
        let elapsed = now.duration_since(self.start).as_secs_f32();
        let t = (elapsed / self.duration_secs).min(1.0);
        if t >= 1.0 {
            return None;
        }
        let eased = self.curve().transform(t as f64) as f32;
        Some(self.from + (self.target - self.from) * eased)
    }
}

/// Shared cell holding the current keyboard animation params (or `None` when
/// the keyboard is at rest). Written by the iOS shim; read by the render loop.
///
/// Uses `Mutex<Option<KeyboardAnimation>>` (not atomics) because
/// `Instant` and `f32` pairs have no atomic representation, and updates are
/// rare (one per keyboard notification) + main-thread-only, so contention
/// is nonexistent.
#[derive(Clone)]
pub struct KeyboardAnimationSource {
    inner: Arc<std::sync::Mutex<Option<KeyboardAnimation>>>,
}

impl KeyboardAnimationSource {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Replace the current animation params. Called by the iOS shim on
    /// each `keyboardWillShow/Hide` notification.
    pub fn set(&self, animation: KeyboardAnimation) {
        *self.inner.lock().unwrap() = Some(animation);
    }

    /// Read + take the current animation. Returns `Some(animation)` if an
    /// animation is active; the caller is expected to either re-store it
    /// (still animating) or leave it `None` (completed).
    pub fn take(&self) -> Option<KeyboardAnimation> {
        self.inner.lock().unwrap().take()
    }

    /// Store an animation back after reading (if still active).
    pub fn restore(&self, animation: KeyboardAnimation) {
        *self.inner.lock().unwrap() = Some(animation);
    }

    pub fn has_pending(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }
}

impl Default for KeyboardAnimationSource {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// MEDIA QUERY DATA SOURCE
// ============================================================================

/// Snapshot of the platform-derived parts of `MediaQueryData` that have
/// no existing source. Read by the root `MediaQuery` component when
/// composing `MediaQueryData` each render.
///
/// Uses `bool` for brightness (not `Brightness`) so this core cell has no
/// dependency on `widgets/theme.rs`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaQueryDataSourceSnapshot {
    pub size: crate::core::Size<crate::core::Logical>,
    pub device_pixel_ratio: f32,
    pub is_dark: bool,
}

/// Shared atomic cell holding the platform-derived parts of `MediaQueryData`
/// that have no existing source. Updated by `WindowState` each frame; read by
/// the root `MediaQuery` component.
///
/// `padding` / `viewInsets` / `viewPadding` stay on the existing
/// `SafeAreaSource` / `KeyboardInsetSource` cells (they already propagate
/// correctly); this cell carries only the new fields.
#[derive(Clone)]
pub struct MediaQueryDataSource {
    inner: Arc<MediaQueryDataInner>,
}

struct MediaQueryDataInner {
    size_w: AtomicU32,
    size_h: AtomicU32,
    device_pixel_ratio: AtomicU32,
    is_dark: AtomicBool,
}

impl MediaQueryDataSource {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MediaQueryDataInner {
                size_w: AtomicU32::new(0.0_f32.to_bits()),
                size_h: AtomicU32::new(0.0_f32.to_bits()),
                device_pixel_ratio: AtomicU32::new(1.0_f32.to_bits()),
                is_dark: AtomicBool::new(false),
            }),
        }
    }

    pub fn set(
        &self,
        size: crate::core::Size<crate::core::Logical>,
        device_pixel_ratio: f32,
        is_dark: bool,
    ) {
        self.inner
            .size_w
            .store(size.width.to_bits(), Ordering::Relaxed);
        self.inner
            .size_h
            .store(size.height.to_bits(), Ordering::Relaxed);
        self.inner
            .device_pixel_ratio
            .store(device_pixel_ratio.to_bits(), Ordering::Relaxed);
        self.inner.is_dark.store(is_dark, Ordering::Relaxed);
    }

    pub fn get(&self) -> MediaQueryDataSourceSnapshot {
        MediaQueryDataSourceSnapshot {
            size: crate::core::Size::new(
                f32::from_bits(self.inner.size_w.load(Ordering::Relaxed)),
                f32::from_bits(self.inner.size_h.load(Ordering::Relaxed)),
            ),
            device_pixel_ratio: f32::from_bits(
                self.inner.device_pixel_ratio.load(Ordering::Relaxed),
            ),
            is_dark: self.inner.is_dark.load(Ordering::Relaxed),
        }
    }
}

impl Default for MediaQueryDataSource {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// AFFINE TRANSFORM
// ============================================================================

/// A 2D affine transform represented as 6 floats [a, b, c, d, e, f].
///
/// Corresponds to the 3x3 homogeneous matrix:
/// ```text
/// | a  c  e |     a = scaleX,   b = skewY
/// | b  d  f |     c = skewX,    d = scaleY
/// | 0  0  1 |     e = translateX, f = translateY
/// ```
///
/// This is sufficient for all 2D transforms: rotation, scaling,
/// translation, and skew. No 3D perspective is needed for a 2D UI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AffineTransform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl AffineTransform {
    /// Identity transform (no-op).
    pub const fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Translation by (dx, dy).
    pub fn translation(dx: f32, dy: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: dx,
            f: dy,
        }
    }

    /// Rotation by `radians` around the origin.
    pub fn rotation(radians: f32) -> Self {
        let cos = radians.cos();
        let sin = radians.sin();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Uniform or non-uniform scale.
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Skew by the given angles (in radians).
    pub fn skew(sx: f32, sy: f32) -> Self {
        Self {
            a: 1.0,
            b: sy.tan(),
            c: sx.tan(),
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Create from a [a, b, c, d, e, f] array.
    pub fn from_array(arr: [f32; 6]) -> Self {
        Self {
            a: arr[0],
            b: arr[1],
            c: arr[2],
            d: arr[3],
            e: arr[4],
            f: arr[5],
        }
    }

    /// Convert to [a, b, c, d, e, f] array.
    pub fn to_array(&self) -> [f32; 6] {
        [self.a, self.b, self.c, self.d, self.e, self.f]
    }

    /// Compose this transform with another: `self * other`.
    ///
    /// The result applies `other` first, then `self`.
    pub fn mul(&self, other: &AffineTransform) -> AffineTransform {
        AffineTransform {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    /// Compute the inverse transform.
    ///
    /// Returns `None` for singular matrices (determinant near zero).
    pub fn inverse(&self) -> Option<AffineTransform> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-10 {
            return None;
        }
        let inv_det = 1.0 / det;
        Some(AffineTransform {
            a: self.d * inv_det,
            b: -self.b * inv_det,
            c: -self.c * inv_det,
            d: self.a * inv_det,
            e: (self.c * self.f - self.d * self.e) * inv_det,
            f: (self.b * self.e - self.a * self.f) * inv_det,
        })
    }

    /// Apply the transform to a point.
    pub fn transform_point(&self, p: Point<Logical>) -> Point<Logical> {
        Point::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    /// Check if this is approximately the identity transform.
    pub fn is_identity(&self) -> bool {
        let id = Self::identity();
        (self.a - id.a).abs() < 1e-6
            && (self.b - id.b).abs() < 1e-6
            && (self.c - id.c).abs() < 1e-6
            && (self.d - id.d).abs() < 1e-6
            && (self.e - id.e).abs() < 1e-6
            && (self.f - id.f).abs() < 1e-6
    }

    /// Check if this is a pure translation (no rotation, scale, or skew).
    pub fn is_translation_only(&self) -> bool {
        let id = Self::identity();
        (self.a - id.a).abs() < 1e-6
            && (self.b - id.b).abs() < 1e-6
            && (self.c - id.c).abs() < 1e-6
            && (self.d - id.d).abs() < 1e-6
    }

    /// Compute the determinant of the 2x2 linear part.
    pub fn determinant(&self) -> f32 {
        self.a * self.d - self.b * self.c
    }

    /// Transform a bounding box and return the axis-aligned bounding box of the result.
    ///
    /// Transforms all 4 corners and returns the AABB that encloses them.
    pub fn transform_bounds(&self, bounds: &Bounds<Logical>) -> Bounds<Logical> {
        let tl = self.transform_point(Point::new(bounds.left, bounds.top));
        let tr = self.transform_point(Point::new(bounds.right, bounds.top));
        let bl = self.transform_point(Point::new(bounds.left, bounds.bottom));
        let br = self.transform_point(Point::new(bounds.right, bounds.bottom));

        Bounds::new(
            tl.x.min(tr.x).min(bl.x).min(br.x),
            tl.y.min(tr.y).min(bl.y).min(br.y),
            tl.x.max(tr.x).max(bl.x).max(br.x),
            tl.y.max(tr.y).max(bl.y).max(br.y),
        )
    }
}

impl Default for AffineTransform {
    fn default() -> Self {
        Self::identity()
    }
}

impl std::ops::Mul for AffineTransform {
    type Output = AffineTransform;
    fn mul(self, other: AffineTransform) -> AffineTransform {
        self.mul(&other)
    }
}

impl std::ops::Mul<&AffineTransform> for AffineTransform {
    type Output = AffineTransform;
    fn mul(self, other: &AffineTransform) -> AffineTransform {
        AffineTransform {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
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

impl<C, R> std::ops::Add for Position<C, R> {
    type Output = Position<C, R>;

    fn add(self, other: Position<C, R>) -> Position<C, R> {
        Position::new(self.x + other.x, self.y + other.y)
    }
}

impl<C, R> std::ops::AddAssign for Position<C, R> {
    fn add_assign(&mut self, other: Position<C, R>) {
        self.x += other.x;
        self.y += other.y;
    }
}

impl<C, R> std::ops::Sub for Position<C, R> {
    type Output = Position<C, R>;

    fn sub(self, other: Position<C, R>) -> Position<C, R> {
        Position::new(self.x - other.x, self.y - other.y)
    }
}

impl<C, R> std::ops::SubAssign for Position<C, R> {
    fn sub_assign(&mut self, other: Position<C, R>) {
        self.x -= other.x;
        self.y -= other.y;
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod scale_source_tests {
    use super::ScaleSource;

    #[test]
    fn test_scale_source_get_returns_initial_value() {
        let source = ScaleSource::new(2.0);
        let scale = source.get();
        assert_eq!(scale.factor_f64(), 2.0);
    }

    #[test]
    fn test_scale_source_set_updates_value() {
        let source = ScaleSource::new(1.0);
        source.set(3.0);
        assert_eq!(source.get().factor_f64(), 3.0);
    }

    #[test]
    fn test_scale_source_clones_share_state() {
        let source = ScaleSource::new(1.0);
        let clone = source.clone();
        source.set(2.5);
        assert_eq!(clone.get().factor_f64(), 2.5);
    }

    #[test]
    fn test_scale_source_default() {
        let source = ScaleSource::default();
        assert_eq!(source.get().factor_f64(), 1.0);
    }

    #[test]
    fn test_scale_source_multi_holder_propagation() {
        // Simulates: WgpuBackend creates ScaleSource, WindowState and others get clones
        let backend_source = ScaleSource::new(1.0);
        let window_source = backend_source.clone();
        let context_source = backend_source.clone();

        // Backend updates scale (e.g., OS reports Retina)
        backend_source.set(2.0);

        // All holders see the new value
        assert_eq!(window_source.get().factor_f64(), 2.0);
        assert_eq!(context_source.get().factor_f64(), 2.0);

        // WindowState can also update (e.g., ScaleFactorChanged event)
        window_source.set(3.0);
        assert_eq!(backend_source.get().factor_f64(), 3.0);
        assert_eq!(context_source.get().factor_f64(), 3.0);
    }
}

#[cfg(test)]
mod safe_area_source_tests {
    use super::SafeAreaSource;
    use crate::layout::EdgeInsets;

    #[test]
    fn test_safe_area_source_get_returns_initial_values() {
        let source = SafeAreaSource::new(10.0, 20.0, 30.0, 40.0);
        let insets = source.get();
        assert_eq!(
            insets,
            EdgeInsets {
                left: 10.0,
                right: 20.0,
                top: 30.0,
                bottom: 40.0
            }
        );
    }

    #[test]
    fn test_safe_area_source_set_updates_values() {
        let source = SafeAreaSource::new(0.0, 0.0, 0.0, 0.0);
        source.set(5.0, 15.0, 25.0, 35.0);
        let insets = source.get();
        assert_eq!(
            insets,
            EdgeInsets {
                left: 5.0,
                right: 15.0,
                top: 25.0,
                bottom: 35.0
            }
        );
    }

    #[test]
    fn test_safe_area_source_clones_share_state() {
        let source = SafeAreaSource::new(0.0, 0.0, 0.0, 0.0);
        let clone = source.clone();
        source.set(1.0, 2.0, 3.0, 4.0);
        assert_eq!(
            clone.get(),
            EdgeInsets {
                left: 1.0,
                right: 2.0,
                top: 3.0,
                bottom: 4.0
            }
        );
    }

    #[test]
    fn test_safe_area_source_default_is_zero() {
        let source = SafeAreaSource::default();
        let insets = source.get();
        assert_eq!(
            insets,
            EdgeInsets {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0
            }
        );
    }

    #[test]
    fn test_safe_area_source_multi_holder_propagation() {
        // Simulates: WindowState owns the source, BuildOwner holds a clone.
        let window_source = SafeAreaSource::new(0.0, 0.0, 0.0, 0.0);
        let build_owner_source = window_source.clone();

        // WindowState updates insets each frame (e.g., after rotation)
        window_source.set(44.0, 0.0, 44.0, 34.0);

        // BuildOwner (and thus RenderContext::media_query_sources()) sees the update
        assert_eq!(
            build_owner_source.get(),
            EdgeInsets {
                left: 44.0,
                right: 0.0,
                top: 44.0,
                bottom: 34.0
            }
        );
    }
}

#[cfg(test)]
mod keyboard_inset_source_tests {
    use super::*;

    #[test]
    fn default_is_zero() {
        let src = KeyboardInsetSource::new();
        assert_eq!(src.get(), 0.0);
    }

    #[test]
    fn set_updates_value() {
        let src = KeyboardInsetSource::new();
        src.set(300.0);
        assert_eq!(src.get(), 300.0);
    }

    #[test]
    fn clones_share_state() {
        let src = KeyboardInsetSource::new();
        let clone = src.clone();
        src.set(250.0);
        assert_eq!(clone.get(), 250.0);
    }
}

#[cfg(test)]
mod keyboard_animation_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn interpolate_returns_from_at_t0() {
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 300.0,
            duration_secs: 0.25,
            start: Instant::now(),
            curve_raw: 3, // Linear
        };
        let val = anim.interpolate(anim.start).unwrap();
        assert!(val.abs() < 1.0, "expected ~0 at t=0, got {}", val);
    }

    #[test]
    fn interpolate_returns_none_when_complete() {
        let start = Instant::now() - Duration::from_millis(500);
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 300.0,
            duration_secs: 0.25,
            start,
            curve_raw: 3,
        };
        assert!(anim.interpolate(Instant::now()).is_none());
    }

    #[test]
    fn interpolate_returns_none_when_duration_zero() {
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 300.0,
            duration_secs: 0.0,
            start: Instant::now(),
            curve_raw: 3,
        };
        assert!(anim.interpolate(Instant::now()).is_none());
    }

    #[test]
    fn interpolate_linear_halfway() {
        let start = Instant::now();
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 300.0,
            duration_secs: 0.25,
            start,
            curve_raw: 3, // Linear
        };
        // At t=0.125s (halfway through 0.25s), linear → 150px.
        let val = anim
            .interpolate(start + Duration::from_millis(125))
            .unwrap();
        assert!(
            (val - 150.0).abs() < 2.0,
            "expected ~150 at halfway, got {}",
            val
        );
    }

    #[test]
    fn curve_raw_7_maps_to_ease_in_out() {
        // iOS keyboard notifications report the private curve raw value 7
        // (`UIKeyboardAnimationCurveKeyboard`), which Apple renders as a
        // smooth easeInOut-style curve. Mapping it to Linear (via `7 & 3`)
        // is wrong: linear motion keeps moving at constant velocity near
        // the end while the keyboard has nearly settled, so the input bar
        // appears to finish after the keyboard.
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 100.0,
            duration_secs: 1.0,
            start: Instant::now(),
            curve_raw: 7,
        };
        // EaseInOut at t=0.75 → 1 - ((-2*0.75 + 2)² / 2) = 0.875 → 87.5px.
        // (Linear at t=0.75 would be 75px, so this distinguishes the curves.)
        let val = anim
            .interpolate(anim.start + Duration::from_millis(750))
            .unwrap();
        assert!(
            (val - 87.5).abs() < 1.0,
            "expected ~87.5 (easeInOut) for raw=7 at t=0.75, got {}",
            val
        );
        // EaseInOut at t=0.5 → 2 * 0.5² = 0.5 → 50px (matches Linear here,
        // so this is just a sanity check that the curve is monotonic).
        let val_mid = anim
            .interpolate(anim.start + Duration::from_millis(500))
            .unwrap();
        assert!(
            (val_mid - 50.0).abs() < 1.0,
            "expected ~50 (easeInOut midpoint) for raw=7 at t=0.5, got {}",
            val_mid
        );
    }

    #[test]
    fn animation_source_set_take_restore() {
        let src = KeyboardAnimationSource::new();
        assert!(src.take().is_none());
        let anim = KeyboardAnimation {
            from: 0.0,
            target: 300.0,
            duration_secs: 0.25,
            start: Instant::now(),
            curve_raw: 3,
        };
        src.set(anim.clone());
        let taken = src.take().unwrap();
        assert_eq!(taken.target, 300.0);
        assert!(src.take().is_none());
        src.restore(taken);
        assert!(src.take().is_some());
    }
}

#[cfg(test)]
mod media_query_data_source_tests {
    use super::*;
    use crate::core::{Logical, Size};

    #[test]
    fn default_is_all_zero() {
        let src = MediaQueryDataSource::new();
        let snap = src.get();
        assert_eq!(snap.size, Size::<Logical>::new(0.0, 0.0));
        assert_eq!(snap.device_pixel_ratio, 1.0);
        assert!(!snap.is_dark);
    }

    #[test]
    fn set_updates_values() {
        let src = MediaQueryDataSource::new();
        src.set(Size::new(400.0, 800.0), 2.0, true);
        let snap = src.get();
        assert_eq!(snap.size, Size::<Logical>::new(400.0, 800.0));
        assert_eq!(snap.device_pixel_ratio, 2.0);
        assert!(snap.is_dark);
    }

    #[test]
    fn clones_share_state() {
        let src = MediaQueryDataSource::new();
        let clone = src.clone();
        src.set(Size::new(100.0, 200.0), 3.0, false);
        let snap = clone.get();
        assert_eq!(snap.size, Size::<Logical>::new(100.0, 200.0));
        assert_eq!(snap.device_pixel_ratio, 3.0);
        assert!(!snap.is_dark);
    }
}

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

    // ========================================================================
    // Position Tests
    // ========================================================================

    #[test]
    fn test_position_relative_to_absolute() {
        let relative = Position::<Logical, Relative>::new(10.0, 20.0);
        let parent_absolute = Position::<Logical, Absolute>::new(100.0, 50.0);
        let absolute = relative.to_absolute(parent_absolute);

        assert_eq!(absolute.x, 110.0);
        assert_eq!(absolute.y, 70.0);
    }

    #[test]
    fn test_position_absolute_to_relative() {
        let absolute = Position::<Logical, Absolute>::new(110.0, 70.0);
        let parent_absolute = Position::<Logical, Absolute>::new(100.0, 50.0);
        let relative = absolute.to_relative(parent_absolute);

        assert_eq!(relative.x, 10.0);
        assert_eq!(relative.y, 20.0);
    }

    #[test]
    fn test_position_logical_to_physical() {
        let logical_abs = Position::<Logical, Absolute>::new(100.0, 200.0);
        let physical_abs = logical_abs.to_physical(Scale::new(2.0));

        assert_eq!(physical_abs.x, 200.0);
        assert_eq!(physical_abs.y, 400.0);

        let logical_rel = Position::<Logical, Relative>::new(50.0, 100.0);
        let physical_rel = logical_rel.to_physical(Scale::new(2.0));

        assert_eq!(physical_rel.x, 100.0);
        assert_eq!(physical_rel.y, 200.0);
    }

    #[test]
    fn test_position_physical_to_logical() {
        let physical_abs = Position::<Physical, Absolute>::new(200.0, 400.0);
        let logical_abs = physical_abs.to_logical(Scale::new(2.0));

        assert_eq!(logical_abs.x, 100.0);
        assert_eq!(logical_abs.y, 200.0);

        let physical_rel = Position::<Physical, Relative>::new(100.0, 200.0);
        let logical_rel = physical_rel.to_logical(Scale::new(2.0));

        assert_eq!(logical_rel.x, 50.0);
        assert_eq!(logical_rel.y, 100.0);
    }

    #[test]
    fn test_position_to_point() {
        let position = Position::<Logical, Absolute>::new(10.0, 20.0);
        let point = position.to_point();

        assert_eq!(point.x, 10.0);
        assert_eq!(point.y, 20.0);
    }

    #[test]
    fn test_position_add() {
        let p1 = Position::<Logical, Relative>::new(10.0, 20.0);
        let p2 = Position::<Logical, Relative>::new(5.0, 10.0);
        let sum = p1 + p2;

        assert_eq!(sum.x, 15.0);
        assert_eq!(sum.y, 30.0);
    }

    #[test]
    fn test_position_sub() {
        let p1 = Position::<Logical, Absolute>::new(110.0, 70.0);
        let p2 = Position::<Logical, Absolute>::new(100.0, 50.0);
        let diff = p1 - p2;

        assert_eq!(diff.x, 10.0);
        assert_eq!(diff.y, 20.0);
    }

    #[test]
    fn test_position_zero() {
        let zero = Position::<Logical, Absolute>::zero();
        assert_eq!(zero.x, 0.0);
        assert_eq!(zero.y, 0.0);
    }

    #[test]
    fn test_position_chain_conversions() {
        // Start with relative position from layout
        let child_relative = Position::<Logical, Relative>::new(10.0, 20.0);
        let parent_absolute = Position::<Logical, Absolute>::new(100.0, 50.0);

        // Convert to absolute
        let child_absolute = child_relative.to_absolute(parent_absolute);

        // Convert to physical for rendering
        let child_physical = child_absolute.to_physical(Scale::new(2.0));

        assert_eq!(child_physical.x, 220.0);
        assert_eq!(child_physical.y, 140.0);
    }

    // ========================================================================
    // AffineTransform Tests
    // ========================================================================

    #[test]
    fn test_affine_transform_identity() {
        let t = AffineTransform::identity();
        assert!(t.is_identity());
        assert!(t.is_translation_only());

        let p = Point::<Logical>::new(10.0, 20.0);
        let result = t.transform_point(p);
        assert_eq!(result.x, 10.0);
        assert_eq!(result.y, 20.0);
    }

    #[test]
    fn test_affine_transform_translation() {
        let t = AffineTransform::translation(5.0, 10.0);
        assert!(t.is_translation_only());
        assert!(!t.is_identity());

        let p = Point::<Logical>::new(10.0, 20.0);
        let result = t.transform_point(p);
        assert_eq!(result.x, 15.0);
        assert_eq!(result.y, 30.0);
    }

    #[test]
    fn test_affine_transform_rotation() {
        let t = AffineTransform::rotation(std::f32::consts::FRAC_PI_2);
        let p = Point::<Logical>::new(1.0, 0.0);
        let result = t.transform_point(p);
        assert!((result.x - 0.0).abs() < 1e-6);
        assert!((result.y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_affine_transform_scale() {
        let t = AffineTransform::scale(2.0, 3.0);
        let p = Point::<Logical>::new(10.0, 20.0);
        let result = t.transform_point(p);
        assert_eq!(result.x, 20.0);
        assert_eq!(result.y, 60.0);
    }

    #[test]
    fn test_affine_transform_composition() {
        let t1 = AffineTransform::translation(10.0, 20.0);
        let t2 = AffineTransform::scale(2.0, 2.0);
        let combined = t1 * t2;

        // scale first, then translate: scale(10,10) -> (20,20), then translate -> (30,40)
        let p = Point::<Logical>::new(10.0, 10.0);
        let result = combined.transform_point(p);
        assert_eq!(result.x, 30.0);
        assert_eq!(result.y, 40.0);
    }

    #[test]
    fn test_affine_transform_inverse() {
        let t = AffineTransform::rotation(0.5);
        let inv = t.inverse().expect("rotation should be invertible");
        let combined = t * inv;
        assert!(combined.is_identity());
    }

    #[test]
    fn test_affine_transform_inverse_singular() {
        let t = AffineTransform::scale(0.0, 0.0);
        assert!(t.inverse().is_none());
    }

    #[test]
    fn test_affine_transform_inverse_translation() {
        let t = AffineTransform::translation(5.0, 10.0);
        let inv = t.inverse().expect("translation should be invertible");
        let p = Point::<Logical>::new(15.0, 30.0);
        let result = inv.transform_point(p);
        assert!((result.x - 10.0).abs() < 1e-6);
        assert!((result.y - 20.0).abs() < 1e-6);
    }

    #[test]
    fn test_affine_transform_to_array() {
        let t = AffineTransform::translation(3.0, 4.0);
        let arr = t.to_array();
        assert_eq!(arr, [1.0, 0.0, 0.0, 1.0, 3.0, 4.0]);
    }

    #[test]
    fn test_affine_transform_from_array() {
        let arr = [2.0, 0.0, 0.0, 3.0, 5.0, 6.0];
        let t = AffineTransform::from_array(arr);
        assert_eq!(t.a, 2.0);
        assert_eq!(t.d, 3.0);
        assert_eq!(t.e, 5.0);
        assert_eq!(t.f, 6.0);
    }

    #[test]
    fn test_affine_transform_determinant() {
        let t = AffineTransform::scale(2.0, 3.0);
        assert_eq!(t.determinant(), 6.0);

        let singular = AffineTransform::scale(0.0, 1.0);
        assert_eq!(singular.determinant(), 0.0);
    }

    #[test]
    fn test_transform_bounds_identity() {
        let t = AffineTransform::identity();
        let b = Bounds::<Logical>::from_xywh(10.0, 20.0, 100.0, 50.0);
        let result = t.transform_bounds(&b);
        assert!((result.left - 10.0).abs() < 1e-6);
        assert!((result.top - 20.0).abs() < 1e-6);
        assert!((result.width() - 100.0).abs() < 1e-6);
        assert!((result.height() - 50.0).abs() < 1e-6);
    }

    #[test]
    fn test_transform_bounds_rotation() {
        let t = AffineTransform::rotation(std::f32::consts::FRAC_PI_4);
        let b = Bounds::<Logical>::from_xywh(-50.0, -50.0, 100.0, 100.0);
        let result = t.transform_bounds(&b);
        let width = result.width();
        let height = result.height();
        let expected = 100.0 * std::f32::consts::SQRT_2;
        assert!(
            (width - expected).abs() < 1.0,
            "width should be ~{expected}, got {width}"
        );
        assert!(
            (height - expected).abs() < 1.0,
            "height should be ~{expected}, got {height}"
        );
    }

    #[test]
    fn test_transform_bounds_translation() {
        let t = AffineTransform::translation(10.0, 20.0);
        let b = Bounds::<Logical>::from_xywh(0.0, 0.0, 100.0, 50.0);
        let result = t.transform_bounds(&b);
        assert!((result.left - 10.0).abs() < 1e-6);
        assert!((result.top - 20.0).abs() < 1e-6);
        assert!((result.width() - 100.0).abs() < 1e-6);
        assert!((result.height() - 50.0).abs() < 1e-6);
    }
}
