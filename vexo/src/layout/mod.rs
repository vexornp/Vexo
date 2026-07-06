//! Layout abstractions for the Vexo UI framework.
//!
//! This module provides the layout layer that sits between widgets and
//! the layout engine. It defines:
//!
//! - `LayoutEngine` trait for layout computation
//! - `LayoutNodeKey` for node handles
//! - `ComputedLayout` for layout results
//! - `TaffyLayoutEngine` implementation
//! - `Layout` struct for CSS-style layout properties
//! - `LayoutContext` and `LayoutView` for widget interaction
//!
//! # Architecture
//!
//! The layout abstraction enables:
//! - Testing layout without Taffy dependency
//! - Swapping to different layout algorithms
//! - Centralized layout logic (not scattered in widgets)
//!
//! # Example
//!
//! ```
//! use vexo::layout::{LayoutEngine, TaffyLayoutEngine, Layout};
//!
//! let mut engine = TaffyLayoutEngine::new();
//!
//! // Or use the Layout struct for CSS-style properties
//! let layout = Layout::default()
//!     .padding(10.0)
//!     .margin(5.0)
//!     .flex_grow(1.0);
//! ```

/// Default line-height multiplier applied to font size when measuring text.
///
/// Text measurement (`TextMeasurer::measure`) and render objects use this
/// when no explicit line height is provided. The value matches CSS browser
/// defaults and cosmic-text's convention: line box = font_size * 1.2.
pub const DEFAULT_LINE_HEIGHT_MULTIPLIER: f32 = 1.2;

/// Tolerance (in logical pixels) applied when comparing a text leaf's natural
/// width against its computed layout box width.
///
/// Taffy floors layout widths to integers, so a text whose natural width is
/// e.g. 41.51 may receive a 41px box. Without tolerance, `apply_layout` and
/// `paint` would treat that 0.51px shortfall as a wrap constraint and split
/// "Inbox" → "Inbo" + "x". 1.0px covers typical subpixel rounding.
pub const LAYOUT_WIDTH_TOLERANCE: f32 = 1.0;

mod context;
mod engine;
mod measurement;
mod node;
mod style;
mod taffy_engine;

pub use context::{LayoutContext, LayoutView};
pub use engine::{LayoutEngine, LayoutError};
pub use measurement::{
    MeasureCache, MeasureCacheKey, MeasureContext, TextMeasureContext, TextMeasurer,
};
pub use node::{
    AlignItems as NodeAlignItems, ComputedLayout, FlexDirection as NodeFlexDirection,
    LayoutConstraints, LayoutNodeKey, LayoutPadding,
};
pub use style::{
    AlignContent, AlignItems, AlignSelf, Dimension, Display, EdgeInsets, FlexDirection, FlexWrap,
    GridAutoFlow, GridPlacement, Inset, JustifyContent, Layout, Overflow, Position, TrackSizing,
};
pub use taffy_engine::TaffyLayoutEngine;
