//! Focus scope types: [`FocusScopeData`], [`UnfocusDisposition`], [`TraversalEdgeBehavior`].

use super::node::FocusNodeId;

/// Extension data stored in a `SecondaryMap` for scope nodes.
///
/// A scope node has both a [`FocusNodeData`] entry in the primary slotmap
/// and a `FocusScopeData` entry in the secondary map. Rust has no class
/// inheritance, so we use structural composition instead.
#[derive(Debug, Clone)]
pub struct FocusScopeData {
    /// Stack of focused children within this scope.
    ///
    /// The most-recently focused child is at the end (stack top).
    /// When a scope is unfocused and then re-focused, the last entry
    /// is restored.
    pub focused_children: Vec<FocusNodeId>,
    /// How this scope behaves when unfocused.
    pub traversal_edge_behavior: TraversalEdgeBehavior,
}

impl FocusScopeData {
    /// Create a new scope data with default values.
    pub fn new() -> Self {
        Self {
            focused_children: Vec::new(),
            traversal_edge_behavior: TraversalEdgeBehavior::default(),
        }
    }

    /// Return the most-recently focused child, or `None` if the stack is empty.
    pub fn focused_child(&self) -> Option<FocusNodeId> {
        self.focused_children.last().copied()
    }
}

impl Default for FocusScopeData {
    fn default() -> Self {
        Self::new()
    }
}

/// Determines what happens when a scope is unfocused.
///
/// Mirrors Flutter's `UnfocusDisposition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnfocusDisposition {
    /// Move focus to the nearest focusable ancestor scope.
    /// This is the default behavior.
    PreviouslyFocusedChild,
    /// Move focus to the scope itself (the scope node receives focus).
    Scope,
}

impl Default for UnfocusDisposition {
    fn default() -> Self {
        Self::PreviouslyFocusedChild
    }
}

/// Determines how focus traversal behaves at the edges of a scope.
///
/// Mirrors Flutter's `TraversalEdgeBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalEdgeBehavior {
    /// When traversal reaches the edge of a scope, wrap around to the
    /// other side of the same scope.
    LoopAround,
    /// When traversal reaches the edge of a scope, move to the parent
    /// scope and continue traversal there.
    LeaveScope,
}

impl Default for TraversalEdgeBehavior {
    fn default() -> Self {
        Self::LoopAround
    }
}
