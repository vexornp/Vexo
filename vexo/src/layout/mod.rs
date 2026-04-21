//! Layout abstractions for the Vexo UI framework.
//!
//! This module provides the layout layer that sits between widgets and
//! the layout engine. It defines:
//!
//! - `LayoutEngine` trait for layout computation
//! - `LayoutNode` tree structure for describing layout
//! - `ComputedLayout` for layout results
//! - `TaffyLayoutEngine` implementation
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
//! use vexo::layout::{LayoutEngine, TaffyLayoutEngine, LayoutConstraints};
//!
//! let mut engine = TaffyLayoutEngine::new();
//! // Build and compute layout trees using the engine
//! ```

mod engine;
mod node;
mod taffy_engine;

pub use engine::{LayoutEngine, LayoutError, LayoutTreeHandle};
pub use node::{
    AlignItems,
    ComputedLayout,
    FlexDirection,
    LayoutConstraints,
    LayoutNode,
    LayoutNodeId,
    LayoutPadding,
    LayoutTree,
};
pub use taffy_engine::TaffyLayoutEngine;
