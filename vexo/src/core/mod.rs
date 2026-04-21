//! Core domain types for the Vexo UI framework.
//!
//! This module contains the fundamental types that are used throughout the
//! framework, independent of any specific implementation details like
//! rendering backends or layout engines.
//!
//! # Types
//!
//! - `WidgetId` - Unique identifier for widgets
//! - `Point`, `Size`, `Rect` - Geometry types with logical/physical markers
//! - `Scale` - DPI scale factor
//! - `Color` - RGBA color representation
//!
//! # Logical vs Physical Coordinates
//!
//! The framework distinguishes between logical (DPI-independent) and physical
//! (screen pixel) coordinates using marker types:
//!
//! ```
//! use vexo::core::{Point, Logical, Physical};
//!
//! let logical = Point::<Logical>::new(100.0, 100.0);
//! let physical = logical.to_physical(2.0); // 2x scale factor
//! assert_eq!(physical.x, 200.0);
//! ```

mod color;
mod geometry;
mod id;

pub use color::Color;
pub use geometry::{Logical, Physical, Point, Rect, Scale, Size};
pub use id::WidgetId;
