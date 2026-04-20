//! Layout abstractions for the Vexo UI framework.
//!
//! This module provides the layout layer that sits between widgets and
//! the layout engine. It defines:
//!
//! - `LayoutEngine` trait for layout computation
//! - `LayoutNode` tree structure for describing layout
//! - `ComputedLayout` for layout results
//! - `TaffyLayoutEngine` implementation

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
