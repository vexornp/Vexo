//! Layout abstractions for the Vexo UI framework.
//!
//! This module provides the layout layer that sits between widgets and
//! the layout engine. It defines:
//!
//! - `LayoutEngine` trait for layout computation
//! - `LayoutNodeId` for node handles
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

mod context;
mod engine;
mod measurement;
mod node;
mod style;
mod taffy_engine;

pub use context::{LayoutContext, LayoutView};
pub use engine::{LayoutEngine, LayoutError};
pub use measurement::{
    MeasureCache,
    MeasureCacheKey,
    MeasureContext,
    TextMeasureContext,
    TextMeasurer,
};
pub use node::{
    AlignItems as NodeAlignItems,
    ComputedLayout,
    FlexDirection as NodeFlexDirection,
    LayoutConstraints,
    LayoutNodeId,
    LayoutPadding,
};
pub use style::{
    AlignContent,
    AlignItems,
    Dimension,
    Display,
    EdgeInsets,
    FlexDirection,
    FlexWrap,
    GridPlacement,
    Inset,
    JustifyContent,
    Layout,
    Position,
    TrackSizing,
};
pub use taffy_engine::TaffyLayoutEngine;
