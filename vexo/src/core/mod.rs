//! Core domain types for the Vexo UI framework.
//!
//! This module contains the fundamental types that are used throughout the
//! framework, independent of any specific implementation details like
//! rendering backends or layout engines.
//!
//! # Types
//!
//! - `WidgetId` - Unique identifier for widgets
//! - `Point`, `Size`, `Bounds` - Geometry types with logical/physical markers
//! - `Position` - Position with coordinate space AND reference frame markers
//! - `Scale` - DPI scale factor
//! - `Color` - RGBA color representation
//!
//! # Coordinate Systems
//!
//! The framework distinguishes between different coordinate systems using marker types:
//!
//! ## Logical vs Physical (Coordinate Space)
//!
//! - `Logical` - DPI-independent coordinates used for layout
//! - `Physical` - Actual screen pixels
//!
//! ```
//! use vexo::core::{Point, Logical, Physical, Scale};
//!
//! let logical = Point::<Logical>::new(100.0, 100.0);
//! let physical = logical.to_physical(Scale::new(2.0)); // 2x scale factor
//! assert_eq!(physical.x, 200.0);
//! ```
//!
//! ## Absolute vs Relative (Reference Frame)
//!
//! - `Absolute` - Coordinates relative to window origin
//! - `Relative` - Coordinates relative to parent container
//!
//! ```
//! use vexo::core::{Position, Logical, Absolute, Relative};
//!
//! // Position from layout (relative to parent)
//! let relative = Position::<Logical, Relative>::new(10.0, 20.0);
//!
//! // Parent's absolute position
//! let parent_absolute = Position::<Logical, Absolute>::new(100.0, 50.0);
//!
//! // Convert to absolute
//! let absolute = relative.to_absolute(parent_absolute);
//! assert_eq!(absolute.x, 110.0);
//! assert_eq!(absolute.y, 70.0);
//! ```

mod color;
mod geometry;
mod id;
mod stroke;

pub use color::Color;
pub use geometry::{
    Absolute, AffineTransform, Bounds, KeyboardCurve, KeyboardInsetSnapshot, KeyboardInsetSource,
    Logical, Physical, Point, Position, Relative, SafeAreaSource, Scale, ScaleSource, Size,
};
pub use id::WidgetId;
pub use stroke::Stroke;
