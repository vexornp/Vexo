use std::collections::HashSet;

use super::id::RenderObjectKey;

/// Tracks which render objects need layout or paint.
pub struct DirtyTracking {
    needs_layout: HashSet<RenderObjectKey>,
    needs_paint: HashSet<RenderObjectKey>,
    /// Set to true when mark_needs_layout/mark_needs_paint transitions
    /// dirty set from empty to non-empty. Signals that a frame is needed.
    frame_request_needed: bool,
}

impl DirtyTracking {
    /// Create a new empty dirty tracking.
    pub fn new() -> Self {
        Self {
            needs_layout: HashSet::new(),
            needs_paint: HashSet::new(),
            frame_request_needed: false,
        }
    }

    /// Mark a render object as needing layout.
    pub fn mark_needs_layout(&mut self, key: RenderObjectKey) {
        let was_empty = self.needs_layout.is_empty();
        self.needs_layout.insert(key);
        if was_empty {
            self.frame_request_needed = true;
        }
    }

    /// Mark a render object as needing paint.
    pub fn mark_needs_paint(&mut self, key: RenderObjectKey) {
        let was_empty = self.needs_paint.is_empty();
        self.needs_paint.insert(key);
        if was_empty {
            self.frame_request_needed = true;
        }
    }

    /// Check if a render object needs layout.
    pub fn needs_layout(&self, key: RenderObjectKey) -> bool {
        self.needs_layout.contains(&key)
    }

    /// Check if a render object needs paint.
    pub fn needs_paint(&self, key: RenderObjectKey) -> bool {
        self.needs_paint.contains(&key)
    }

    /// Clear layout dirty flag for a render object.
    pub fn clear_layout(&mut self, key: RenderObjectKey) {
        self.needs_layout.remove(&key);
    }

    /// Clear paint dirty flag for a render object.
    pub fn clear_paint(&mut self, key: RenderObjectKey) {
        self.needs_paint.remove(&key);
    }

    /// Check if there are any objects needing layout.
    pub fn is_layout_empty(&self) -> bool {
        self.needs_layout.is_empty()
    }

    /// Returns true if a frame is needed due to dirty state changes,
    /// and clears the flag.
    pub fn take_frame_request_needed(&mut self) -> bool {
        let needed = self.frame_request_needed;
        self.frame_request_needed = false;
        needed
    }

    /// Check if there are any objects needing paint.
    pub fn is_paint_empty(&self) -> bool {
        self.needs_paint.is_empty()
    }

    /// Drain all objects needing layout.
    pub fn drain_layout(&mut self) -> impl Iterator<Item = RenderObjectKey> + '_ {
        self.needs_layout.drain()
    }

    /// Drain all objects needing paint.
    pub fn drain_paint(&mut self) -> impl Iterator<Item = RenderObjectKey> + '_ {
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
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let key = sm.insert(());

        tracking.mark_needs_layout(key);

        assert!(tracking.needs_layout(key));
    }

    #[test]
    fn test_mark_needs_paint() {
        let mut tracking = DirtyTracking::new();
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let key = sm.insert(());

        tracking.mark_needs_paint(key);

        assert!(tracking.needs_paint(key));
    }

    #[test]
    fn test_clear_layout() {
        let mut tracking = DirtyTracking::new();
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let key = sm.insert(());

        tracking.mark_needs_layout(key);
        tracking.clear_layout(key);

        assert!(!tracking.needs_layout(key));
    }

    #[test]
    fn test_drain_layout() {
        let mut tracking = DirtyTracking::new();
        let mut sm: slotmap::SlotMap<RenderObjectKey, ()> = slotmap::SlotMap::with_key();
        let key1 = sm.insert(());
        let key2 = sm.insert(());

        tracking.mark_needs_layout(key1);
        tracking.mark_needs_layout(key2);

        let ids: Vec<_> = tracking.drain_layout().collect();
        assert_eq!(ids.len(), 2);
        assert!(tracking.is_layout_empty());
    }
}
