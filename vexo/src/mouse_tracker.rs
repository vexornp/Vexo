//! Mouse tracker for cursor resolution and hover dispatch.
//!
//! Follows Flutter's MouseTracker design:
//! - Resolves cursor via `firstNonDeferred()` annotation traversal
//! - Dispatches hover enter/exit callbacks
//! - Handles post-frame cursor updates (widgets moving under still mouse)

use std::collections::HashSet;

use crate::core::{Absolute, Logical, Position};
use crate::id::ElementKey;
use crate::input::{MouseCursor, MouseTrackerAnnotation, SystemCursorKind};

/// Tracks mouse cursor state and hover annotations.
///
/// Encapsulates cursor resolution (Flutter's `firstNonDeferred()`) and
/// hover enter/exit dispatch. The pipeline calls `resolve_cursor()` on
/// pointer move events and `post_frame_cursor_update()` after paint.
pub struct MouseTracker {
    /// Currently displayed cursor icon.
    current_cursor: SystemCursorKind,
    /// Whether the hover set changed during the last dispatch.
    hover_changed: bool,
    /// Element keys that were hovered in the last pointer move.
    last_hovered_elements: HashSet<ElementKey>,
    /// Last known mouse position (absolute logical coordinates).
    last_mouse_position: Option<Position<Logical, Absolute>>,
}

impl MouseTracker {
    /// Create a new mouse tracker with default (Arrow) cursor.
    pub fn new() -> Self {
        Self {
            current_cursor: SystemCursorKind::Arrow,
            hover_changed: false,
            last_hovered_elements: HashSet::new(),
            last_mouse_position: None,
        }
    }

    /// Resolve the cursor icon from annotation-element pairs.
    ///
    /// Walks annotations deepest-first (reverse of path order, which is
    /// root→deepest). Applies Flutter's `firstNonDeferred()` logic:
    /// - First `System` variant wins and returns its cursor kind.
    /// - `Uncontrolled` blocks further traversal, returns Arrow.
    /// - `Defer` skips (falls through to next annotation).
    /// - If all annotations are `Defer`, returns Arrow (default).
    pub fn resolve_cursor(annotations: &[(ElementKey, MouseTrackerAnnotation)]) -> SystemCursorKind {
        // Walk deepest-first: annotations are in root→deepest order from
        // hit test, so we reverse to get deepest→root.
        for (_, annotation) in annotations.iter().rev() {
            match annotation.cursor {
                MouseCursor::System(kind) => return kind,
                MouseCursor::Uncontrolled => return SystemCursorKind::Arrow,
                MouseCursor::Defer => continue,
            }
        }
        SystemCursorKind::Arrow
    }

    /// Update the tracked mouse position.
    pub fn update_mouse_position(&mut self, position: Position<Logical, Absolute>) {
        self.last_mouse_position = Some(position);
    }

    /// Dispatch hover enter/exit callbacks.
    ///
    /// Compares the new set of hovered annotations against the last set.
    /// Fires on_exit for elements leaving hover, on_enter for elements
    /// entering hover. Updates the tracked hover set.
    ///
    /// Returns the element keys that left hover (so the pipeline can look
    /// up their annotations from the registry for on_exit dispatch if needed).
    pub fn dispatch_hover_changes(
        &mut self,
        new_hovered: &[(ElementKey, MouseTrackerAnnotation)],
    ) -> Vec<ElementKey> {
        let new_keys: HashSet<ElementKey> =
            new_hovered.iter().map(|(k, _)| *k).collect();

        self.hover_changed = new_keys != self.last_hovered_elements;

        // Fire on_enter for elements entering hover
        for (key, annotation) in new_hovered {
            if !self.last_hovered_elements.contains(key) {
                if let Some(callback) = &annotation.on_enter {
                    (callback.borrow_mut())();
                }
            }
        }

        // Collect elements leaving hover
        let exiting: Vec<ElementKey> = self
            .last_hovered_elements
            .iter()
            .filter(|k| !new_keys.contains(k))
            .copied()
            .collect();

        self.last_hovered_elements = new_keys;

        exiting
    }

    /// Whether the hover set changed during the last dispatch.
    pub fn hover_changed(&self) -> bool {
        self.hover_changed
    }

    /// Fire on_exit for specific elements that are leaving hover.
    ///
    /// Called by the pipeline with annotations looked up from the registry
    /// for elements that were in the previous hover set but not the new one.
    pub fn dispatch_hover_exit_for(&self, annotations: &[MouseTrackerAnnotation]) {
        for annotation in annotations {
            if let Some(callback) = &annotation.on_exit {
                (callback.borrow_mut())();
            }
        }
    }

    /// Elements that were hovered in the last pointer move event.
    pub fn last_hovered_elements(&self) -> &HashSet<ElementKey> {
        &self.last_hovered_elements
    }

    /// Update cursor after a frame. Returns the new cursor if it changed.
    ///
    /// Called by the pipeline after paint/render. The pipeline re-hit-tests
    /// at the last mouse position and passes the resolved cursor here.
    /// Returns `Some(new_cursor)` if the cursor changed, `None` if unchanged.
    pub fn update_cursor_post_frame(&mut self, new_cursor: SystemCursorKind) -> Option<SystemCursorKind> {
        if new_cursor != self.current_cursor {
            self.current_cursor = new_cursor;
            Some(new_cursor)
        } else {
            None
        }
    }

    /// Get the current cursor icon.
    pub fn current_cursor(&self) -> SystemCursorKind {
        self.current_cursor
    }

    /// Set the current cursor directly (used on pointer move).
    pub fn set_current_cursor(&mut self, cursor: SystemCursorKind) -> bool {
        if cursor != self.current_cursor {
            self.current_cursor = cursor;
            true
        } else {
            false
        }
    }

    /// Get the last known mouse position.
    pub fn last_mouse_position(&self) -> Option<Position<Logical, Absolute>> {
        self.last_mouse_position
    }
}

impl Default for MouseTracker {
    fn default() -> Self {
        Self::new()
    }
}