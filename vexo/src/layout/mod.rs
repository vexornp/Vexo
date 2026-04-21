//! Layout abstractions for the Vexo UI framework.
//!
//! This module provides the layout layer that sits between widgets and
//! the layout engine. It defines:
//!
//! - `LayoutEngine` trait for layout computation
//! - `LayoutNode` tree structure for describing layout
//! - `ComputedLayout` for layout results
//! - `TaffyLayoutEngine` implementation
//! - `Layout` struct for CSS-style layout properties
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
//! use vexo::layout::{LayoutEngine, TaffyLayoutEngine, LayoutConstraints, Layout};
//!
//! let mut engine = TaffyLayoutEngine::new();
//! // Build and compute layout trees using the engine
//!
//! // Or use the Layout struct for CSS-style properties
//! let layout = Layout::default()
//!     .padding(10.0)
//!     .margin(5.0)
//!     .flex_grow(1.0);
//! ```

mod engine;
mod node;
mod style;
mod taffy_engine;

pub use engine::{LayoutEngine, LayoutError, LayoutTreeHandle};
pub use node::{
    AlignItems as NodeAlignItems,
    ComputedLayout,
    FlexDirection as NodeFlexDirection,
    LayoutConstraints,
    LayoutNode,
    LayoutNodeId,
    LayoutPadding,
    LayoutTree,
};
pub use style::{
    AlignContent,
    AlignItems,
    Dimension,
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
