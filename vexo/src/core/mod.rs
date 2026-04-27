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
//! - `Scale` - DPI scale factor
//! - `Color` - RGBA color representation
//!
//! # Logical vs Physical Coordinates
//!
//! The framework distinguishes between logical (DPI-independent) and physical
//! (screen pixel) coordinates using marker types:
//!
//! ```
//! use vexo::core::{Point, Logical, Physical, Scale};
//!
//! let logical = Point::<Logical>::new(100.0, 100.0);
//! let physical = logical.to_physical(Scale::new(2.0)); // 2x scale factor
//! assert_eq!(physical.x, 200.0);
//! ```

mod color;
mod geometry;
mod id;
mod stroke;

pub use color::Color;
pub use geometry::{Bounds, Logical, Physical, Point, Scale, Size};
pub use id::WidgetId;
pub use stroke::Stroke;
