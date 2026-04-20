//! Core domain types for the Vexo UI framework.
//!
//! This module contains the fundamental types that are used throughout the
//! framework, independent of any specific implementation details like
//! rendering backends or layout engines.

mod color;
mod geometry;
mod id;

pub use color::Color;
pub use geometry::{Logical, Physical, Point, Rect, Scale, Size};
pub use id::WidgetId;
