//! Focus tree data model for the retain-mode system.
//!
//! Implements a sparse focus tree with `FocusNodeData` entries stored in a
//! slotmap. The `FocusManager` owns the tree and provides operations for
//! focus requests, unfocus, and reparenting.
//!
//! # Key types
//!
//! - [`FocusNodeId`] — opaque slotmap key (generational, ABA-safe)
//! - [`FocusNodeData`] — per-node data (parent, children, flags)
//! - [`FocusManager`] — owns the slotmap, provides all focus operations

mod node;
mod manager;
pub mod attachment;
mod widget;

pub use node::{FocusNodeId, FocusNodeData};
pub use manager::FocusManager;
pub use attachment::FocusAttachment;
pub use widget::{Focus, FocusElement};

#[cfg(test)]
mod integration_tests;