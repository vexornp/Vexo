//! Focus tree data model for the retain-mode system.
//!
//! Implements a Flutter-style sparse focus tree with `FocusNode` and
//! `FocusScopeNode` types stored in a slotmap. The `FocusManager` owns
//! the tree and provides operations for focus requests, unfocus,
//! reparenting, and scope-aware traversal.
//!
//! # Key types
//!
//! - [`FocusNodeId`] — opaque slotmap key (generational, ABA-safe)
//! - [`FocusNodeData`] — per-node data (parent, children, flags)
//! - [`FocusScopeData`] — extension data for scope nodes (focused-children stack)
//! - [`FocusManager`] — owns the slotmap, provides all focus operations

mod node;
mod scope;
mod manager;
mod attachment;

pub use node::{FocusNodeId, FocusNodeData};
pub use scope::{FocusScopeData, UnfocusDisposition, TraversalEdgeBehavior};
pub use manager::FocusManager;
pub use attachment::FocusAttachment;
