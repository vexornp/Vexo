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
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

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

/// Keyboard animation curve, mirroring UIKit's
/// `UIViewAnimationCurve` raw values reported via
/// `UIResponder.keyboardAnimationCurveUserInfoKey`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyboardCurve {
    /// UIKit raw value 0. The default keyboard curve; ease-in-ease-out.
    EaseInOut = 0,
    /// UIKit raw value 1.
    EaseIn = 1,
    /// UIKit raw value 2.
    EaseOut = 2,
    /// UIKit raw value 3.
    Linear = 3,
}

impl Default for KeyboardCurve {
    fn default() -> Self {
        Self::EaseInOut
    }
}

impl KeyboardCurve {
    /// Map a UIKit `UIViewAnimationCurve` raw value to our enum.
    ///
    /// The standard `UIViewAnimationCurve` values are 0 (easeInOut), 1
    /// (easeIn), 2 (easeOut), 3 (linear). However, iOS keyboard notifications
    /// report `raw = 7` — a private value that, when converted to
    /// `UIViewAnimationOptions` via `curve << 16`, yields `0x70000`: bits
    /// 16-17 (the curve field) are `0b11 = 3 = linear`, and bit 18 is
    /// `allowUserInteraction`. So **the keyboard actually animates with a
    /// LINEAR curve**, not EaseInOut.
    ///
    /// Extracting the bottom 2 bits (`raw & 0x3`) handles all cases
    /// correctly: standard values 0-3 pass through unchanged, and the
    /// keyboard's private value 7 → 3 → Linear. Without this, raw=7 fell
    /// into the `_ => EaseInOut` fallback, making the input view start
    /// slowly (ease-in phase) while the keyboard moved at constant speed —
    /// the keyboard dismissed far faster than the input view moved down.
    pub fn from_uikit_raw(raw: u8) -> Self {
        match raw & 0x3 {
            0 => Self::EaseInOut,
            1 => Self::EaseIn,
            2 => Self::EaseOut,
            3 => Self::Linear,
            // Unreachable: `u8 & 0x3` is always 0..=3.
            _ => Self::EaseInOut,
        }
    }
}

/// Snapshot of the keyboard-inset state at a point in time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KeyboardInsetSnapshot {
    /// Target bottom inset in logical pixels (0 when keyboard is down).
    pub target_height: f32,
    /// Duration of the keyboard's own animation, in seconds.
    /// 0.0 means "snap immediately" (no animation).
    pub duration_secs: f32,
    /// Keyboard animation curve.
    pub curve: KeyboardCurve,
    /// The instant the OS keyboard animation began, captured in the iOS shim
    /// at the moment `keyboardWillShow/Hide` fired.
    ///
    /// `KeyboardAvoidance` uses this to seed its avoidance tween's
    /// `start_time` so the input view tracks the keyboard in lockstep instead
    /// of lagging by a frame (which otherwise makes the keyboard appear to
    /// cover the input before it lifts). `None` for the snap path
    /// (`duration_secs == 0`), on non-iOS platforms (no shim ever writes), and
    /// in tests that don't care about timing — the widget falls back to
    /// `Instant::now()` in that case.
    pub animation_start: Option<Instant>,
}

/// Shared handle to the keyboard's target inset (logical pixels),
/// animation duration, and curve.
///
/// Mirrors [`SafeAreaSource`]'s design: a dumb `Arc`-atomic value with no
/// callbacks. The iOS keyboard shim writes via [`set_target`] on each
/// `keyboardWillShow/Hide` notification; the [`KeyboardAvoidance`] widget
/// reads via [`get`] each render and owns the animated tween in its own state.
///
/// On desktop / Android the shim is absent, so this stays at its default
/// (all-zero) and `KeyboardAvoidance` is a transparent pass-through.
///
/// [`KeyboardAvoidance`]: crate::widgets::KeyboardAvoidance
/// [`set_target`]: Self::set_target
/// [`get`]: Self::get
#[derive(Clone)]
pub struct KeyboardInsetSource {
    inner: Arc<KeyboardInsetInner>,
}

struct KeyboardInsetInner {
    target_height: AtomicU32,
    duration_secs: AtomicU32,
    curve: AtomicU8,
    /// Wall-clock instant the OS keyboard animation began. Stored in a
    /// `Mutex` (not an atomic) because `Instant` has no atomic representation;
    /// updates are rare (one per keyboard notification) and, on iOS, always
    /// main-thread. Reads are also main-thread (the render loop), so contention
    /// is nonexistent.
    animation_start: Mutex<Option<Instant>>,
}

impl KeyboardInsetSource {
    /// Create a new source with all-zero defaults (keyboard down, no animation).
    pub fn new() -> Self {
        Self {
            inner: Arc::new(KeyboardInsetInner {
                target_height: AtomicU32::new(0.0_f32.to_bits()),
                duration_secs: AtomicU32::new(0.0_f32.to_bits()),
                curve: AtomicU8::new(KeyboardCurve::EaseInOut as u8),
                animation_start: Mutex::new(None),
            }),
        }
    }

    /// Read the current snapshot.
    pub fn get(&self) -> KeyboardInsetSnapshot {
        KeyboardInsetSnapshot {
            target_height: f32::from_bits(self.inner.target_height.load(Ordering::Relaxed)),
            duration_secs: f32::from_bits(self.inner.duration_secs.load(Ordering::Relaxed)),
            curve: KeyboardCurve::from_uikit_raw(self.inner.curve.load(Ordering::Relaxed)),
            animation_start: *self.inner.animation_start.lock().unwrap(),
        }
    }

    /// Update the target inset, animation duration, curve, and the OS
    /// animation's start instant.
    ///
    /// Called only by the iOS keyboard shim on each notification.
    /// `animation_start` should be `Instant::now()` captured at the moment the
    /// notification fired, so the [`KeyboardAvoidance`] widget can align its
    /// tween's start time with the keyboard's own animation. Pass `None` for
    /// the snap path (`duration_secs == 0`) or when there's no associated
    /// animation (tests, non-iOS). Visible to all clone holders immediately.
    ///
    /// [`KeyboardAvoidance`]: crate::widgets::KeyboardAvoidance
    pub fn set_target(
        &self,
        height: f32,
        duration_secs: f32,
        curve: KeyboardCurve,
        animation_start: Option<Instant>,
    ) {
        self.inner
            .target_height
            .store(height.to_bits(), Ordering::Relaxed);
        self.inner
            .duration_secs
            .store(duration_secs.to_bits(), Ordering::Relaxed);
        self.inner.curve.store(curve as u8, Ordering::Relaxed);
        *self.inner.animation_start.lock().unwrap() = animation_start;
    }

    /// Convenience: read just the current target height.
    pub fn current_target_height(&self) -> f32 {
        f32::from_bits(self.inner.target_height.load(Ordering::Relaxed))
    }
}

impl Default for KeyboardInsetSource {
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

        // BuildOwner (and thus RenderContext::safe_area()) sees the update
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
    use super::{KeyboardCurve, KeyboardInsetSnapshot, KeyboardInsetSource};
    use std::time::Instant;

    #[test]
    fn default_is_all_zero() {
        let s = KeyboardInsetSource::default();
        let snap = s.get();
        assert_eq!(snap.target_height, 0.0);
        assert_eq!(snap.duration_secs, 0.0);
        assert_eq!(snap.curve, KeyboardCurve::EaseInOut);
        assert_eq!(snap.animation_start, None);
    }

    #[test]
    fn set_target_then_get_returns_written_values() {
        let s = KeyboardInsetSource::default();
        let start = Instant::now();
        s.set_target(300.0, 0.25, KeyboardCurve::EaseIn, Some(start));
        let snap = s.get();
        assert_eq!(snap.target_height, 300.0);
        assert_eq!(snap.duration_secs, 0.25);
        assert_eq!(snap.curve, KeyboardCurve::EaseIn);
        assert_eq!(snap.animation_start, Some(start));
    }

    #[test]
    fn clones_share_storage() {
        let s = KeyboardInsetSource::default();
        let clone = s.clone();
        let start = Instant::now();
        s.set_target(250.0, 0.3, KeyboardCurve::Linear, Some(start));
        let snap = clone.get();
        assert_eq!(snap.target_height, 250.0);
        assert_eq!(snap.duration_secs, 0.3);
        assert_eq!(snap.curve, KeyboardCurve::Linear);
        assert_eq!(snap.animation_start, Some(start));
    }

    #[test]
    fn current_target_height_returns_latest() {
        let s = KeyboardInsetSource::default();
        s.set_target(336.0, 0.25, KeyboardCurve::EaseInOut, None);
        assert_eq!(s.current_target_height(), 336.0);
        s.set_target(0.0, 0.25, KeyboardCurve::EaseInOut, None);
        assert_eq!(s.current_target_height(), 0.0);
    }

    #[test]
    fn from_uikit_raw_maps_all_values() {
        assert_eq!(KeyboardCurve::from_uikit_raw(0), KeyboardCurve::EaseInOut);
        assert_eq!(KeyboardCurve::from_uikit_raw(1), KeyboardCurve::EaseIn);
        assert_eq!(KeyboardCurve::from_uikit_raw(2), KeyboardCurve::EaseOut);
        assert_eq!(KeyboardCurve::from_uikit_raw(3), KeyboardCurve::Linear);
    }

    #[test]
    fn from_uikit_raw_keyboard_private_value_7_is_linear() {
        // iOS keyboard notifications report raw=7 — a private
        // UIViewAnimationCurve value. When converted to
        // UIViewAnimationOptions via `curve << 16` (0x70000), bits 16-17
        // (the curve field) are 0b11 = 3 = linear. The keyboard animates
        // with a LINEAR curve, not EaseInOut. Mapping raw=7 to EaseInOut
        // (the old fallback) caused the input view to start slowly while
        // the keyboard moved at constant speed — the keyboard dismissed
        // far faster than the input view moved down.
        assert_eq!(KeyboardCurve::from_uikit_raw(7), KeyboardCurve::Linear);

        // The bottom-2-bits masking also handles higher private values.
        // raw=4 (0b100) → bits 0-1 = 0b00 = 0 → EaseInOut.
        assert_eq!(KeyboardCurve::from_uikit_raw(4), KeyboardCurve::EaseInOut);
        // raw=5 (0b101) → bits 0-1 = 0b01 = 1 → EaseIn.
        assert_eq!(KeyboardCurve::from_uikit_raw(5), KeyboardCurve::EaseIn);
        // raw=6 (0b110) → bits 0-1 = 0b10 = 2 → EaseOut.
        assert_eq!(KeyboardCurve::from_uikit_raw(6), KeyboardCurve::EaseOut);
        // raw=255 (0b11111111) → bits 0-1 = 0b11 = 3 → Linear.
        assert_eq!(KeyboardCurve::from_uikit_raw(255), KeyboardCurve::Linear);
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
