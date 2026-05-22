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
    pub unfocus_disposition: UnfocusDisposition,
    /// How focus traversal behaves at the edges of this scope.
    pub traversal_edge_behavior: TraversalEdgeBehavior,
}

impl FocusScopeData {
    /// Create a new scope data with default values.
    pub fn new() -> Self {
        Self {
            focused_children: Vec::new(),
            unfocus_disposition: UnfocusDisposition::default(),
            traversal_edge_behavior: TraversalEdgeBehavior::default(),
        }
    }

    /// Create scope data with the given unfocus disposition.
    pub fn with_unfocus_disposition(mut self, disposition: UnfocusDisposition) -> Self {
        self.unfocus_disposition = disposition;
        self
    }

    /// Create scope data with the given traversal edge behavior.
    pub fn with_traversal_edge_behavior(mut self, behavior: TraversalEdgeBehavior) -> Self {
        self.traversal_edge_behavior = behavior;
        self
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
    /// Restore the scope's previously focused child from the history stack.
    RestorePrevious,
    /// Clear focus entirely (set primary_focus to None).
    Clear,
}

impl Default for UnfocusDisposition {
    fn default() -> Self {
        Self::RestorePrevious
    }
}

/// Determines how focus traversal behaves at the edges of a scope.
///
/// Mirrors Flutter's `TraversalEdgeBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalEdgeBehavior {
    /// Wrap around within the scope (Tab from last goes to first).
    ClosedLoop,
    /// Exit to the parent scope and continue traversal there.
    ParentScope,
    /// Stay at the current position (no wrapping, no exit).
    Stop,
}

impl Default for TraversalEdgeBehavior {
    fn default() -> Self {
        Self::ParentScope
    }
}
