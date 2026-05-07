use std::collections::HashSet;

use super::id::RenderObjectId;

/// Tracks which render objects need layout or paint.
pub struct DirtyTracking {
    needs_layout: HashSet<RenderObjectId>,
    needs_paint: HashSet<RenderObjectId>,
}

impl DirtyTracking {
    /// Create a new empty dirty tracking.
    pub fn new() -> Self {
        Self {
            needs_layout: HashSet::new(),
            needs_paint: HashSet::new(),
        }
    }

    /// Mark a render object as needing layout.
    pub fn mark_needs_layout(&mut self, id: RenderObjectId) {
        self.needs_layout.insert(id);
    }

    /// Mark a render object as needing paint.
    pub fn mark_needs_paint(&mut self, id: RenderObjectId) {
        self.needs_paint.insert(id);
    }

    /// Check if a render object needs layout.
    pub fn needs_layout(&self, id: RenderObjectId) -> bool {
        self.needs_layout.contains(&id)
    }

    /// Check if a render object needs paint.
    pub fn needs_paint(&self, id: RenderObjectId) -> bool {
        self.needs_paint.contains(&id)
    }

    /// Clear layout dirty flag for a render object.
    pub fn clear_layout(&mut self, id: RenderObjectId) {
        self.needs_layout.remove(&id);
    }

    /// Clear paint dirty flag for a render object.
    pub fn clear_paint(&mut self, id: RenderObjectId) {
        self.needs_paint.remove(&id);
    }

    /// Check if there are any objects needing layout.
    pub fn is_layout_empty(&self) -> bool {
        self.needs_layout.is_empty()
    }

    /// Check if there are any objects needing paint.
    pub fn is_paint_empty(&self) -> bool {
        self.needs_paint.is_empty()
    }

    /// Drain all objects needing layout.
    pub fn drain_layout(&mut self) -> impl Iterator<Item = RenderObjectId> + '_ {
        self.needs_layout.drain()
    }

    /// Drain all objects needing paint.
    pub fn drain_paint(&mut self) -> impl Iterator<Item = RenderObjectId> + '_ {
        self.needs_paint.drain()
    }

    /// Clear all dirty flags.
    pub fn clear(&mut self) {
        self.needs_layout.clear();
        self.needs_paint.clear();
    }

    /// Get the count of objects needing layout.
    pub fn layout_count(&self) -> usize {
        self.needs_layout.len()
    }

    /// Get the count of objects needing paint.
    pub fn paint_count(&self) -> usize {
        self.needs_paint.len()
    }
}

impl Default for DirtyTracking {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_needs_layout() {
        let mut tracking = DirtyTracking::new();
        let id = RenderObjectId::new();

        tracking.mark_needs_layout(id);

        assert!(tracking.needs_layout(id));
    }

    #[test]
    fn test_mark_needs_paint() {
        let mut tracking = DirtyTracking::new();
        let id = RenderObjectId::new();

        tracking.mark_needs_paint(id);

        assert!(tracking.needs_paint(id));
    }

    #[test]
    fn test_clear_layout() {
        let mut tracking = DirtyTracking::new();
        let id = RenderObjectId::new();

        tracking.mark_needs_layout(id);
        tracking.clear_layout(id);

        assert!(!tracking.needs_layout(id));
    }

    #[test]
    fn test_drain_layout() {
        let mut tracking = DirtyTracking::new();
        let id1 = RenderObjectId::new();
        let id2 = RenderObjectId::new();

        tracking.mark_needs_layout(id1);
        tracking.mark_needs_layout(id2);

        let ids: Vec<_> = tracking.drain_layout().collect();
        assert_eq!(ids.len(), 2);
        assert!(tracking.is_layout_empty());
    }
}
